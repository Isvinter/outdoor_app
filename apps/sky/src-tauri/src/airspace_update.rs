//! Updating the airspaces from openAIP's public export, one file per country.
//!
//! Runs only when the user asks for it. Each file is fetched with the version tag (ETag)
//! of the copy on the device; if nothing changed, the server answers "304 Not Modified"
//! and nothing is downloaded. A new file is checked by reading it before it replaces the
//! old one, so a broken download never removes working airspaces.

use std::collections::BTreeMap;
use std::time::Duration;

use flight_core::openaip;
use serde::Serialize;
use tauri::AppHandle;

use crate::settings::{config_file, write_file};

const EXPORT_BASE: &str = "https://storage.openaip.net/openaip-system-exports";
const COUNTRIES_FILE_NAME: &str = "airspace-countries.txt";
const ETAGS_FILE_NAME: &str = "airspace-etags.txt";
/// Largest file accepted; France, the largest neighbour, has about 4 MB.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Countries on offer: code (as in the export's file names) and name.
pub const COUNTRIES: [(&str, &str); 4] = [
    ("de", "Deutschland"),
    ("at", "Österreich"),
    ("ch", "Schweiz"),
    ("it", "Italien"),
];
const DEFAULT_COUNTRIES: [&str; 2] = ["de", "at"];

/// A country for the settings page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Country {
    pub code: &'static str,
    pub name: &'static str,
    pub selected: bool,
}

/// What happened to one country.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryResult {
    pub code: String,
    pub name: String,
    /// `updated`, `unchanged` or `failed`.
    pub outcome: &'static str,
    /// Airspaces in a newly downloaded file.
    pub airspaces: Option<usize>,
    pub error: Option<String>,
}

/// Name of the file a country's airspaces are kept in.
pub fn file_name(code: &str) -> String {
    format!("{code}_asp.geojson")
}

pub fn countries(app: &AppHandle) -> Vec<Country> {
    let selected = load_selected(app);
    COUNTRIES
        .iter()
        .map(|&(code, name)| Country {
            code,
            name,
            selected: selected.iter().any(|c| c == code),
        })
        .collect()
}

fn load_selected(app: &AppHandle) -> Vec<String> {
    config_file(app, COUNTRIES_FILE_NAME)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| known_codes(text.lines()))
        .unwrap_or_else(|| DEFAULT_COUNTRIES.map(str::to_owned).to_vec())
}

/// Known country codes from `codes`, in the order of [`COUNTRIES`].
fn known_codes<'a>(codes: impl Iterator<Item = &'a str> + Clone) -> Vec<String> {
    COUNTRIES
        .iter()
        .filter(|(code, _)| codes.clone().any(|c| c.trim() == *code))
        .map(|(code, _)| (*code).to_owned())
        .collect()
}

/// Remembers the chosen countries and updates their files. Blocks while downloading.
pub fn update(app: &AppHandle, codes: &[String]) -> Result<Vec<CountryResult>, String> {
    let codes = known_codes(codes.iter().map(String::as_str));
    write_file(config_file(app, COUNTRIES_FILE_NAME)?, &codes.join("\n"))?;
    let dir = crate::airspace::dir(app).ok_or("no data folder")?;
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;

    let etags_path = config_file(app, ETAGS_FILE_NAME)?;
    let mut etags = parse_etags(&std::fs::read_to_string(&etags_path).unwrap_or_default());
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut results = Vec::new();
    for code in &codes {
        let name = COUNTRIES
            .iter()
            .find(|(c, _)| c == code)
            .map_or(code.as_str(), |(_, name)| name);
        let path = dir.join(file_name(code));
        // Without the file the old tag is worthless: download in any case.
        let known = if path.exists() { etags.get(code) } else { None };
        let result = match fetch(&agent, code, known.map(String::as_str)) {
            Ok(Fetch::Unchanged) => CountryResult::new(code, name, "unchanged"),
            Ok(Fetch::New { bytes, etag }) => match store(&path, &bytes) {
                Ok(count) => {
                    match etag {
                        Some(etag) => etags.insert(code.clone(), etag),
                        None => etags.remove(code),
                    };
                    CountryResult {
                        airspaces: Some(count),
                        ..CountryResult::new(code, name, "updated")
                    }
                }
                Err(error) => CountryResult::failed(code, name, error),
            },
            Err(error) => CountryResult::failed(code, name, error),
        };
        results.push(result);
    }
    write_file(etags_path, &etags_text(&etags))?;
    Ok(results)
}

impl CountryResult {
    fn new(code: &str, name: &str, outcome: &'static str) -> Self {
        CountryResult {
            code: code.to_owned(),
            name: name.to_owned(),
            outcome,
            airspaces: None,
            error: None,
        }
    }

    fn failed(code: &str, name: &str, error: String) -> Self {
        CountryResult {
            error: Some(error),
            ..CountryResult::new(code, name, "failed")
        }
    }
}

enum Fetch {
    Unchanged,
    New {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

fn fetch(agent: &ureq::Agent, code: &str, etag: Option<&str>) -> Result<Fetch, String> {
    let url = format!("{EXPORT_BASE}/{}", file_name(code));
    let mut request = agent.get(&url);
    if let Some(etag) = etag {
        request = request.header("If-None-Match", etag);
    }
    let mut response = request.call().map_err(|error| error.to_string())?;
    match response.status().as_u16() {
        304 => Ok(Fetch::Unchanged),
        200 => {
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let bytes = response
                .body_mut()
                .with_config()
                .limit(MAX_BYTES)
                .read_to_vec()
                .map_err(|error| error.to_string())?;
            Ok(Fetch::New { bytes, etag })
        }
        status => Err(format!("server answered {status}")),
    }
}

/// Checks a downloaded file and puts it in place; returns the number of airspaces.
fn store(path: &std::path::Path, bytes: &[u8]) -> Result<usize, String> {
    let file = openaip::parse(bytes);
    if file.airspaces.is_empty() {
        let reason = file
            .issues
            .first()
            .map_or("no airspaces".to_owned(), |issue| issue.message.clone());
        return Err(format!("download unusable: {reason}"));
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
    Ok(file.airspaces.len())
}

/// The tags file: one `code etag` per line.
fn parse_etags(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.trim().split_once(' '))
        .map(|(code, etag)| (code.to_owned(), etag.trim().to_owned()))
        .filter(|(_, etag)| !etag.is_empty())
        .collect()
}

fn etags_text(etags: &BTreeMap<String, String>) -> String {
    etags
        .iter()
        .map(|(code, etag)| format!("{code} {etag}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etags_survive_a_round_trip() {
        let mut etags = BTreeMap::new();
        etags.insert("de".to_owned(), "\"97d3f502\"".to_owned());
        etags.insert("at".to_owned(), "\"abc\"".to_owned());
        assert_eq!(parse_etags(&etags_text(&etags)), etags);
        assert!(parse_etags("de\n\nat   \n").is_empty());
    }

    #[test]
    fn only_known_countries_are_kept_in_order() {
        let codes = known_codes(["it", "xx", " de ", "../etc"].into_iter());
        assert_eq!(codes, ["de", "it"]);
    }
}

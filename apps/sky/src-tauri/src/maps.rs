//! Offline maps: region packages from ZeroGrid Base, read with its
//! `core-maps` crate.
//!
//! A region is a folder with `manifest.json` and PMTiles files (base map, elevation,
//! contour lines, ...), built by ZeroGrid Base's region builder. Regions are copied
//! into the `maps` folder next to `flights` (on iOS visible in the "Files" app, filled
//! from the Mac with the Finder), or downloaded from ZeroGrid Base on the desktop over
//! the home network (see `share`).
//!
//! The map asks for tiles through the URI scheme `tiles`, answered here in Rust:
//!
//! ```text
//! tiles://localhost/<layer>/<z>/<x>/<y>          tile, or 204 if no region has it
//! tiles://localhost/fonts/<font>/<range>.pbf     glyphs bundled with the app
//! ```
//!
//! (On Windows and Android, Tauri spells the same as `http://tiles.localhost/...`.)

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use core_maps::{Contours, RegionStore, StyleOptions, TilePath};
use serde::Serialize;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager};

use crate::thermal_cache::{Answer, ThermalState};

pub const SCHEME: &str = "tiles";

/// Number of tiles served so far; the smoke test reports it.
pub static TILES_SERVED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(any(windows, target_os = "android"))]
const TILES_BASE: &str = "http://tiles.localhost/";
#[cfg(not(any(windows, target_os = "android")))]
const TILES_BASE: &str = "tiles://localhost/";

/// The tile URL template of one thermal layer, for MapLibre (`{z}/{x}/{y}` stay as is).
pub fn thermal_url(key: &str) -> String {
    format!("{TILES_BASE}thermals/{key}/{{z}}/{{x}}/{{y}}")
}

/// Where the glyph files live among the bundled frontend assets (`ui/map/fonts`).
const FONT_ASSETS: &str = "map/fonts/";

/// The map styles for installed regions, with placeholders for tile and font URLs.
/// Topo is ZeroGrid Base's hiking map (`styles/topo-summer.json`); the flying map is
/// derived from it: strong terrain, peaks, few details, obstacles stand out.
const FLYING_STYLE: &str = include_str!("../map-styles/flying.json");
const TOPO_STYLE: &str = include_str!("../map-styles/topo-summer.json");

/// File in the config folder holding the chosen offline style.
const STYLE_FILE_NAME: &str = "offline-map-style.txt";

/// File in the config folder holding light or dark and the relief.
const LOOK_FILE_NAME: &str = "map-look.json";

/// Which of the offline styles to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OfflineStyle {
    #[default]
    Flying,
    Topo,
}

impl OfflineStyle {
    fn name(self) -> &'static str {
        match self {
            OfflineStyle::Flying => "flying",
            OfflineStyle::Topo => "topo",
        }
    }

    fn template(self) -> &'static str {
        match self {
            OfflineStyle::Flying => FLYING_STYLE,
            OfflineStyle::Topo => TOPO_STYLE,
        }
    }
}

/// The chosen offline style; the flying map unless the user picked another.
pub fn load_style(app: &AppHandle) -> OfflineStyle {
    let text = crate::settings::config_file(app, STYLE_FILE_NAME)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    match text.trim() {
        "topo" => OfflineStyle::Topo,
        _ => OfflineStyle::Flying,
    }
}

pub fn save_style(app: &AppHandle, style: OfflineStyle) -> Result<(), String> {
    crate::settings::write_file(
        crate::settings::config_file(app, STYLE_FILE_NAME)?,
        style.name(),
    )
}

/// Light or dark, and whether the dark map shows its relief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MapLook {
    pub dark: bool,
    pub relief: bool,
}

impl MapLook {
    /// Reads a saved look; anything unreadable counts as the default.
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }
}

/// The saved look; light without relief if none was saved.
pub fn load_look(app: &AppHandle) -> MapLook {
    let text = crate::settings::config_file(app, LOOK_FILE_NAME)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    MapLook::parse(&text)
}

pub fn save_look(app: &AppHandle, look: MapLook) -> Result<(), String> {
    let text = serde_json::to_string(&look).map_err(|error| error.to_string())?;
    crate::settings::write_file(crate::settings::config_file(app, LOOK_FILE_NAME)?, &text)
}

/// The installed regions. Read again whenever the map style is requested, so regions
/// copied in while the app runs show up when the map is opened.
///
/// `Arc` (a shared pointer with a reference count): a tile request copies the pointer
/// and releases the lock at once, so tiles are read in parallel and a reload does not
/// wait for running requests.
#[derive(Default)]
pub struct Maps {
    store: Mutex<Option<Arc<RegionStore>>>,
}

/// An installed region as the user interface shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
}

/// What the map page needs to know about the offline maps.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineMaps {
    /// The folder to copy regions into.
    pub dir: Option<String>,
    pub regions: Vec<RegionInfo>,
    /// Which offline style `style` is.
    pub variant: OfflineStyle,
    /// The filled-in map style (JSON), or `None` without installed regions.
    pub style: Option<String>,
}

/// The regions read last, or read now if that has not happened yet.
pub fn store(app: &AppHandle) -> Option<Arc<RegionStore>> {
    let current = app
        .state::<Maps>()
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if current.is_some() {
        return current;
    }
    reload(app).ok()?;
    app.state::<Maps>()
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Ground elevation from the `dem.pmtiles` of the installed regions, for airspace
/// limits above ground.
pub struct RegionTerrain(pub Arc<RegionStore>);

impl flight_core::terrain::Terrain for RegionTerrain {
    fn elevation_m(&self, lat_deg: f64, lon_deg: f64) -> Option<f64> {
        // A broken tile is treated like missing data: the warning then assumes the
        // worst case instead of failing.
        self.0
            .elevation(lat_deg, lon_deg)
            .ok()
            .flatten()
            .filter(|metres| metres.is_finite())
            .map(f64::from)
    }
}

/// The folder holding the regions.
pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    crate::data_dir(app).map(|dir| dir.join("maps"))
}

/// Reads the regions folder again and returns the regions and the style for them.
pub fn reload(app: &AppHandle) -> Result<OfflineMaps, String> {
    let variant = load_style(app);
    let Some(dir) = dir(app) else {
        return Ok(OfflineMaps {
            dir: None,
            regions: Vec::new(),
            variant,
            style: None,
        });
    };
    let store = RegionStore::open(&dir).map_err(|error| error.to_string())?;
    let installed: Vec<_> = store
        .installed()
        .into_iter()
        .filter(|region| region.enabled)
        .collect();

    let regions = installed
        .iter()
        .map(|region| RegionInfo {
            id: region.manifest.id.clone(),
            name: region.manifest.name.clone(),
            size_bytes: region.bytes,
        })
        .collect();

    let style = if installed.is_empty() {
        None
    } else {
        // Smallest box around all regions, and every attribution line once.
        let mut bounds = installed[0].manifest.bbox;
        let mut attribution: Vec<String> = Vec::new();
        for region in &installed {
            let [west, south, east, north] = region.manifest.bbox;
            bounds = [
                bounds[0].min(west),
                bounds[1].min(south),
                bounds[2].max(east),
                bounds[3].max(north),
            ];
            for line in &region.manifest.attribution {
                if !attribution.contains(line) {
                    attribution.push(line.clone());
                }
            }
        }
        let options = StyleOptions {
            tiles_base: TILES_BASE,
            bounds: Some(bounds),
            attribution: &attribution,
            contours: Contours::Tiles,
            appearance: if load_look(app).dark {
                core_maps::Appearance::Dark
            } else {
                core_maps::Appearance::Light
            },
        };
        Some(
            core_maps::offline_style(variant.template(), &options)
                .map_err(|error| error.to_string())?,
        )
    };

    *app.state::<Maps>()
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(store));
    Ok(OfflineMaps {
        dir: Some(dir.to_string_lossy().into_owned()),
        regions,
        variant,
        style,
    })
}

fn response(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    // The page lives on another origin (tauri://localhost).
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    if let Ok(value) = header::HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    response
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    response(status, "application/octet-stream", Vec::new())
}

/// Answers one request to the `tiles` scheme. Blocking: reads from disk.
pub fn respond(app: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path();
    if let Some(font) = path.strip_prefix("/fonts/") {
        // Font names contain spaces. A range the app does not bundle is answered with
        // an empty glyph set, so labels render with what is there.
        let asset = format!("{FONT_ASSETS}{}", font.replace("%20", " "));
        let glyphs = app
            .asset_resolver()
            .get(asset)
            .map(|asset| asset.bytes)
            .unwrap_or_default();
        return response(StatusCode::OK, "application/x-protobuf", glyphs);
    }
    if let Some(rest) = path.strip_prefix("/thermals/") {
        return respond_thermal(app, rest);
    }
    let Ok(tile) = path.parse::<TilePath>() else {
        return empty(StatusCode::BAD_REQUEST);
    };
    let store = app
        .state::<Maps>()
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some(store) = store else {
        return empty(StatusCode::NO_CONTENT);
    };
    match store.tile(tile.layer, tile.z, tile.x, tile.y) {
        Ok(Some(tile)) => {
            TILES_SERVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            response(StatusCode::OK, tile.mime, tile.data)
        }
        Ok(None) => empty(StatusCode::NO_CONTENT),
        Err(error) => {
            eprintln!("map tile {path}: {error}");
            empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Answers a thermal tile request from the on-device cache (`rest` is `<key>/<z>/<x>/<y>`).
fn respond_thermal(app: &AppHandle, rest: &str) -> Response<Vec<u8>> {
    let Some(cache) = app.state::<ThermalState>().get(app) else {
        return empty(StatusCode::NO_CONTENT);
    };
    thermal_response(cache.answer(rest, std::time::SystemTime::now()))
}

/// The HTTP answer for a thermal tile. Built on `response`, so it carries the same
/// cross-origin header as the region tiles: without it the webview drops the image.
fn thermal_response(answer: Answer) -> Response<Vec<u8>> {
    match answer {
        Answer::Tile(tile) => {
            let mut response = response(StatusCode::OK, "image/png", tile.data);
            // The cache decides what is fresh; the webview must not keep its own copy.
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("no-store"),
            );
            response
        }
        Answer::Empty => empty(StatusCode::NO_CONTENT),
        Answer::BadRequest => empty(StatusCode::BAD_REQUEST),
    }
}

#[cfg(test)]
mod look_tests {
    use super::MapLook;

    #[test]
    fn look_defaults_to_light_without_relief() {
        for text in ["", "not json", "[1,2]"] {
            assert_eq!(MapLook::parse(text), MapLook::default());
        }
        assert!(!MapLook::default().dark);
        assert!(!MapLook::default().relief);
    }

    #[test]
    fn look_survives_saving() {
        let look = MapLook {
            dark: true,
            relief: true,
        };
        let text = serde_json::to_string(&look).unwrap();
        assert_eq!(MapLook::parse(&text), look);
    }

    #[test]
    fn missing_field_keeps_default() {
        let look = MapLook::parse(r#"{"dark":true}"#);
        assert!(look.dark);
        assert!(!look.relief);
    }
}

#[cfg(test)]
mod thermal_response_tests {
    use super::{StatusCode, header, thermal_response};
    use crate::thermal_cache::{Answer, ServedTile};

    #[test]
    fn thermal_tile_may_be_read_by_the_page() {
        // The page lives on tauri://localhost; without this header the webview drops
        // the image and the map stays empty although the tile arrived.
        let tile = Answer::Tile(ServedTile {
            data: b"png".to_vec(),
            fallback: false,
        });
        let response = thermal_response(tile);
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }

    #[test]
    fn empty_and_bad_thermal_answers() {
        assert_eq!(
            thermal_response(Answer::Empty).status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            thermal_response(Answer::BadRequest).status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[cfg(test)]
mod thermal_url_tests {
    use super::thermal_url;

    #[test]
    fn thermal_tiles_url_template() {
        assert!(thermal_url("jul_07").ends_with("thermals/jul_07/{z}/{x}/{y}"));
    }
}

//! The folder of recorded flights: listing, loading and deleting IGC files.

use std::io;
use std::path::{Path, PathBuf};

use flight_core::flightlog::{self, FlightSummary};
use flight_core::geo::distance_m;
use flight_core::igc::{IgcFile, Record};
use flight_core::time::UtcDateTime;
use serde::Serialize;

use crate::processor::TrackPoint;

/// One file in the flights folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightEntry {
    pub file_name: String,
    /// `None` if the file could not be read as a flight.
    pub summary: Option<SummaryInfo>,
    /// `false` for a session saved without a detected takeoff (see [`NO_TAKEOFF_MARK`]).
    pub takeoff_detected: bool,
}

/// IGC comment record (`L`, then a three-letter code; `XZG` because ZeroGrid has no
/// manufacturer code of its own) marking a session saved without a detected takeoff.
pub const NO_TAKEOFF_MARK: &str = "LXZG NO TAKEOFF DETECTED";
/// Start of the names of the hidden files that hold a whole session. They start with a
/// dot, so the flight list and the iOS "Files" app leave them out.
const SESSION_PREFIX: &str = ".session-";
/// A session without takeoff is kept as a flight only if it got this far from its first
/// fix ...
const MOVED_DISTANCE_M: f64 = 200.0;
/// ... or changed its GPS altitude by this much; otherwise the pilot only stood around.
const MOVED_ALTITUDE_M: i32 = 50;

/// [`FlightSummary`] in the form the user interface receives.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryInfo {
    /// Start in UTC as `2026-07-28T10:00:00Z`, or `None` if the file has no date.
    /// JavaScript turns this format directly into local time.
    pub start_utc: Option<String>,
    pub duration_s: u32,
    pub fix_count: usize,
    pub start_altitude_m: i32,
    pub max_altitude_m: i32,
    pub track_length_m: f64,
    pub distance_m: f64,
}

impl From<FlightSummary> for SummaryInfo {
    fn from(summary: FlightSummary) -> Self {
        SummaryInfo {
            start_utc: summary.date.map(|date| {
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    date.year(),
                    date.month(),
                    date.day(),
                    summary.start.hour(),
                    summary.start.minute(),
                    summary.start.second()
                )
            }),
            duration_s: summary.duration_s,
            fix_count: summary.fix_count,
            start_altitude_m: summary.start_altitude_m,
            max_altitude_m: summary.max_altitude_m,
            track_length_m: summary.track_length_m,
            distance_m: summary.distance_m,
        }
    }
}

/// All flights in `dir`, newest first. A missing folder is an empty list.
pub fn list(dir: &Path) -> io::Result<Vec<FlightEntry>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut flights = Vec::new();
    for entry in entries {
        let file_name = entry?.file_name().to_string_lossy().into_owned();
        if !is_flight_file_name(&file_name) {
            continue;
        }
        let file = read(dir, &file_name).ok();
        let summary = file
            .as_ref()
            .and_then(flightlog::summarize)
            .map(SummaryInfo::from);
        let takeoff_detected = file.as_ref().is_none_or(takeoff_detected);
        flights.push(FlightEntry {
            file_name,
            summary,
            takeoff_detected,
        });
    }
    // File names start with date and time, so sorting by name sorts by time.
    flights.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(flights)
}

/// The track of one flight, for drawing it on the map.
pub fn track(dir: &Path, file_name: &str) -> io::Result<Vec<TrackPoint>> {
    let file = read(dir, file_name)?;
    Ok(file
        .fixes()
        .map(|fix| TrackPoint {
            latitude_deg: fix.latitude_deg,
            longitude_deg: fix.longitude_deg,
            altitude_m: f64::from(flightlog::display_altitude_m(fix)),
        })
        .collect())
}

/// Deletes one flight for good.
pub fn delete(dir: &Path, file_name: &str) -> io::Result<()> {
    std::fs::remove_file(checked_path(dir, file_name)?)
}

/// Name of the hidden file that holds the whole session starting at `start`.
pub fn session_file_name(start: UtcDateTime) -> String {
    format!("{SESSION_PREFIX}{}", flightlog::file_name(start))
}

/// Whether the file was saved without a detected takeoff.
pub fn takeoff_detected(file: &IgcFile) -> bool {
    !file
        .records
        .iter()
        .any(|record| matches!(record, Record::Other(text) if text == NO_TAKEOFF_MARK))
}

/// Whether the track got more than [`MOVED_DISTANCE_M`] from its first fix or changed
/// its GPS altitude by more than [`MOVED_ALTITUDE_M`].
pub fn moved(file: &IgcFile) -> bool {
    let mut fixes = file.fixes();
    let Some(first) = fixes.next() else {
        return false;
    };
    let (mut lowest, mut highest) = (first.gps_altitude_m, first.gps_altitude_m);
    for fix in fixes {
        lowest = lowest.min(fix.gps_altitude_m);
        highest = highest.max(fix.gps_altitude_m);
        let far = distance_m(
            first.latitude_deg,
            first.longitude_deg,
            fix.latitude_deg,
            fix.longitude_deg,
        ) > MOVED_DISTANCE_M;
        if far || highest - lowest > MOVED_ALTITUDE_M {
            return true;
        }
    }
    false
}

/// Saves a session that never took off as a flight marked with [`NO_TAKEOFF_MARK`],
/// then removes the session file. The flight is written completely before the session
/// file goes, so the track is never only in neither. Returns the flight's file name.
pub fn keep_session(dir: &Path, session_path: &Path, mut file: IgcFile) -> io::Result<String> {
    let (start, _) = span(&file)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "session without fixes"))?;
    let first_fix = file
        .records
        .iter()
        .position(|record| matches!(record, Record::Fix(_)))
        .unwrap_or(file.records.len());
    file.records
        .insert(first_fix, Record::Other(NO_TAKEOFF_MARK.to_owned()));
    let path = unused_path(dir, &flightlog::file_name(start));
    write_file(&path, &file)?;
    remove_if_present(session_path)?;
    Ok(path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// Deals with session files left over from a session that did not end with "Stopp"
/// (the app was killed or crashed): a session whose flights are saved is removed; one
/// without flights is kept as a marked flight if it moved, removed if not. A file that
/// cannot be read stays where it is. Returns one note per file for the log.
pub fn recover_sessions(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let flight_spans: Vec<(UtcDateTime, UtcDateTime)> = names
        .iter()
        .filter(|name| is_flight_file_name(name))
        .filter_map(|name| read(dir, name).ok())
        .filter_map(|file| span(&file))
        .collect();

    let mut notes = Vec::new();
    for name in names
        .iter()
        .filter(|name| name.starts_with(SESSION_PREFIX) && name.ends_with(".igc"))
    {
        let path = dir.join(name);
        let file = match std::fs::read(&path).map(|bytes| IgcFile::parse_bytes(&bytes)) {
            Ok(Ok(file)) => file,
            Ok(Err(error)) => {
                notes.push(format!("kept unreadable session file {name}: {error}"));
                continue;
            }
            Err(error) => {
                notes.push(format!("kept unreadable session file {name}: {error}"));
                continue;
            }
        };
        let note = match span(&file) {
            None => remove_if_present(&path).map(|()| format!("removed empty session {name}")),
            Some((start, end))
                if flight_spans
                    .iter()
                    .any(|&(first, last)| first <= end && start <= last) =>
            {
                remove_if_present(&path)
                    .map(|()| format!("removed session {name}: its flights are saved"))
            }
            Some(_) if moved(&file) => keep_session(dir, &path, file)
                .map(|flight| format!("saved session {name} without takeoff as {flight}")),
            Some(_) => remove_if_present(&path)
                .map(|()| format!("removed session {name}: no flight, no movement")),
        };
        notes.push(note.unwrap_or_else(|error| format!("session file {name}: {error}")));
    }
    notes
}

/// First and last fix of the file in UTC.
fn span(file: &IgcFile) -> Option<(UtcDateTime, UtcDateTime)> {
    let summary = flightlog::summarize(file)?;
    let start = UtcDateTime::new(summary.date?, summary.start);
    Some((start, start.plus_seconds(u64::from(summary.duration_s))))
}

/// Writes the whole file to a temporary file which then replaces `path`. Whatever
/// happens (crash, battery empty), the file on disk is always complete.
pub(crate) fn write_file(path: &Path, file: &IgcFile) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, file.to_string())?;
    std::fs::rename(&temporary, path)
}

/// `dir/name`, or with `-2`, `-3`, ... before the extension if that file exists, so an
/// existing flight is never overwritten.
pub(crate) fn unused_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".igc").unwrap_or(name);
    (2..)
        .map(|number| dir.join(format!("{stem}-{number}.igc")))
        .find(|path| !path.exists())
        .unwrap_or(first)
}

pub(crate) fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn read(dir: &Path, file_name: &str) -> io::Result<IgcFile> {
    let bytes = std::fs::read(checked_path(dir, file_name)?)?;
    IgcFile::parse_bytes(&bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// The file names come from the user interface. Accept only plain names of IGC files,
/// so that a name like `../../something` can never reach a file outside the folder.
fn is_flight_file_name(name: &str) -> bool {
    name.ends_with(".igc")
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.contains("..")
}

fn checked_path(dir: &Path, file_name: &str) -> io::Result<PathBuf> {
    if is_flight_file_name(file_name) {
        Ok(dir.join(file_name))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a flight file name",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flight_core::igc::{FlightMeta, IgcFix, Record};
    use flight_core::time::{Date, TimeOfDay};

    fn write_flight(dir: &Path, name: &str, day: u8, fixes: u8) {
        let mut file =
            IgcFile::with_header(Date::new(2026, 7, day).unwrap(), &FlightMeta::default());
        for second in 0..fixes {
            file.records.push(Record::Fix(IgcFix {
                time: TimeOfDay::new(10, 0, second).unwrap(),
                latitude_deg: 47.0 + f64::from(second) * 0.001,
                longitude_deg: 11.0,
                fix_3d: true,
                pressure_altitude_m: 1000,
                gps_altitude_m: 1000 + i32::from(second),
                extension: String::new(),
            }));
        }
        std::fs::write(dir.join(name), file.to_string()).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vario-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_flights_newest_first_and_tolerates_other_files() {
        let dir = test_dir("list");
        write_flight(&dir, "2026-07-27-100000.igc", 27, 5);
        write_flight(&dir, "2026-07-28-100000.igc", 28, 10);
        std::fs::write(dir.join("broken.igc"), "B123\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a flight").unwrap();
        std::fs::write(dir.join("2026-07-28-100000.tmp"), "half written").unwrap();

        let flights = list(&dir).unwrap();
        let names: Vec<&str> = flights.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "broken.igc",
                "2026-07-28-100000.igc",
                "2026-07-27-100000.igc"
            ]
        );
        assert_eq!(flights[0].summary, None);
        assert!(flights.iter().all(|flight| flight.takeoff_detected));
        let newest = flights[1].summary.as_ref().unwrap();
        assert_eq!(newest.start_utc.as_deref(), Some("2026-07-28T10:00:00Z"));
        assert_eq!(newest.duration_s, 9);
        assert_eq!(newest.max_altitude_m, 1009);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Writes a hidden session file of `fixes` seconds starting at 10:00:00 on 28 July,
    /// moving `step_m` metres north per second.
    fn write_session(dir: &Path, fixes: u8, step_m: f64) -> PathBuf {
        let mut file =
            IgcFile::with_header(Date::new(2026, 7, 28).unwrap(), &FlightMeta::default());
        for second in 0..fixes {
            file.records.push(Record::Fix(IgcFix {
                time: TimeOfDay::new(10, 0, second).unwrap(),
                latitude_deg: 47.0 + f64::from(second) * step_m / 111_195.0,
                longitude_deg: 11.0,
                fix_3d: true,
                pressure_altitude_m: 1000,
                gps_altitude_m: 1000,
                extension: String::new(),
            }));
        }
        let path = dir.join(".session-2026-07-28-100000.igc");
        std::fs::write(&path, file.to_string()).unwrap();
        path
    }

    #[test]
    fn a_left_over_session_that_moved_becomes_a_marked_flight() {
        let dir = test_dir("recover-moved");
        let session = write_session(&dir, 50, 10.0);
        let notes = recover_sessions(&dir);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(!session.exists());
        let flights = list(&dir).unwrap();
        assert_eq!(flights.len(), 1);
        assert_eq!(flights[0].file_name, "2026-07-28-100000.igc");
        assert!(!flights[0].takeoff_detected);
        assert_eq!(flights[0].summary.as_ref().unwrap().fix_count, 50);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_left_over_session_without_movement_is_removed() {
        let dir = test_dir("recover-still");
        let session = write_session(&dir, 50, 1.0);
        recover_sessions(&dir);
        assert!(!session.exists());
        assert_eq!(list(&dir).unwrap(), Vec::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_left_over_session_with_a_saved_flight_is_removed() {
        let dir = test_dir("recover-flight");
        let session = write_session(&dir, 50, 10.0);
        // The flight of this session: 10:00:00 to 10:00:09.
        write_flight(&dir, "2026-07-28-100000.igc", 28, 10);
        recover_sessions(&dir);
        assert!(!session.exists());
        let flights = list(&dir).unwrap();
        assert_eq!(flights.len(), 1);
        assert!(flights[0].takeoff_detected);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_session_file_is_left_alone() {
        let dir = test_dir("recover-broken");
        let path = dir.join(".session-2026-07-28-100000.igc");
        std::fs::write(&path, "B123\n").unwrap();
        let notes = recover_sessions(&dir);
        assert!(path.exists());
        assert!(notes[0].starts_with("kept unreadable"), "{notes:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let dir = std::env::temp_dir().join("vario-does-not-exist");
        assert_eq!(list(&dir).unwrap(), Vec::new());
    }

    #[test]
    fn loads_and_deletes_one_flight() {
        let dir = test_dir("load");
        write_flight(&dir, "2026-07-28-100000.igc", 28, 4);

        let points = track(&dir, "2026-07-28-100000.igc").unwrap();
        assert_eq!(points.len(), 4);
        assert!((points[3].latitude_deg - 47.003).abs() < 1e-5);
        assert!((points[3].altitude_m - 1003.0).abs() < 1e-9);

        delete(&dir, "2026-07-28-100000.igc").unwrap();
        assert_eq!(list(&dir).unwrap(), Vec::new());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_names_that_point_outside_the_folder() {
        let dir = test_dir("names");
        for name in [
            "../secret.igc",
            "a/b.igc",
            "..\\x.igc",
            ".hidden.igc",
            "flight.txt",
            "",
        ] {
            assert!(track(&dir, name).is_err(), "{name:?} must be rejected");
            assert!(delete(&dir, name).is_err(), "{name:?} must be rejected");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

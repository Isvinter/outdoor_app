//! Running the build script: output to a log, steps to the caller, cancel.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::BuildError;

/// Lines of output kept for the error message of a failed build.
const TAIL_LINES: usize = 15;
/// How often the cancel flag is looked at while the script is silent.
const POLL: Duration = Duration::from_millis(100);
/// A cancelled build that ignores the request is killed after this long.
const KILL_AFTER: Duration = Duration::from_secs(10);

/// The build script started a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepProgress {
    pub region: String,
    /// Name of the step as the script calls it, e.g. `basemap`.
    pub step: String,
    /// Position of the step among the steps of this run, starting at 1.
    pub index: u32,
    pub total: u32,
}

/// Runs `command` to its end. Everything it prints goes to the file `log`.
/// Setting `cancel` stops the command and everything it started.
pub(crate) fn run(
    mut command: Command,
    region: &str,
    log: &Path,
    on_step: &mut dyn FnMut(StepProgress),
    cancel: &AtomicBool,
) -> Result<(), BuildError> {
    let mut log_file = File::create(log).map_err(BuildError::io(log))?;
    let failed = |tail: String| BuildError::Failed {
        tail,
        log: log.to_path_buf(),
    };

    // A process group of its own, so that cancelling reaches the programs the
    // script starts as well.
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| failed(format!("the build script cannot be started: {e}")))?;
    let group = child.id();

    let (sender, lines) = mpsc::channel::<Vec<u8>>();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|err| Box::new(err) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(n) if n > 0 => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        })
    })
    .collect();
    drop(sender);

    let mut tail: VecDeque<String> = VecDeque::new();
    let mut signalled: Option<Instant> = None;
    let mut killed = false;
    loop {
        match lines.recv_timeout(POLL) {
            Ok(mut line) => {
                if !line.ends_with(b"\n") {
                    line.push(b'\n');
                }
                log_file.write_all(&line).map_err(BuildError::io(log))?;
                let text = plain(&String::from_utf8_lossy(&line));
                if let Some(step) = parse_step(&text, region) {
                    on_step(step);
                }
                if !text.is_empty() {
                    if tail.len() == TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(text);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Both streams are closed: the script and its children are gone.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if cancel.load(Ordering::SeqCst) {
            match signalled {
                None => {
                    signal(group, "TERM");
                    signalled = Some(Instant::now());
                }
                Some(at) if !killed && at.elapsed() >= KILL_AFTER => {
                    signal(group, "KILL");
                    killed = true;
                }
                Some(_) => {}
            }
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child
        .wait()
        .map_err(|e| failed(format!("the build script was lost: {e}")))?;
    let _ = log_file.flush();

    if status.success() {
        Ok(())
    } else if signalled.is_some() {
        Err(BuildError::Cancelled)
    } else {
        Err(failed(Vec::from(tail).join("\n")))
    }
}

/// Sends a signal to a whole process group. Goes through `kill(1)` because the
/// system call itself needs unsafe code.
fn signal(group: u32, name: &str) {
    let _ = Command::new("kill")
        .arg(format!("-{name}"))
        .arg("--")
        .arg(format!("-{group}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// A line of output without its line end and without terminal colour codes.
fn plain(line: &str) -> String {
    let mut text = String::new();
    let mut chars = line.trim_end().chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // "ESC [ … letter"
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c == '\r' {
            // A progress bar redrawing its line: keep the last state.
            text.clear();
        } else {
            text.push(c);
        }
    }
    text
}

/// Reads `== step <name> (<n>/<total>)`.
fn parse_step(line: &str, region: &str) -> Option<StepProgress> {
    let rest = line.strip_prefix("== step ")?;
    let (step, counts) = rest.split_once(" (")?;
    let (index, total) = counts.strip_suffix(')')?.split_once('/')?;
    Some(StepProgress {
        region: region.to_string(),
        step: step.to_string(),
        index: index.parse().ok()?,
        total: total.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_step, plain};

    #[test]
    fn step_lines_are_recognised() {
        let step = parse_step("== step basemap (2/7)", "r");
        assert_eq!(
            step.map(|s| (s.step, s.index, s.total)),
            Some(("basemap".to_string(), 2, 7))
        );
        assert_eq!(parse_step("== writing manifest", "r"), None);
        assert_eq!(parse_step("== step x (a/b)", "r"), None);
    }

    #[test]
    fn colour_codes_and_redrawn_lines_are_removed() {
        assert_eq!(
            plain("\u{1b}[m\u{1b}[0m0:00:20 INF done\n"),
            "0:00:20 INF done"
        );
        assert_eq!(plain("10 %\r50 %\r100 %\n"), "100 %");
    }
}

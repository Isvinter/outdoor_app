//! What must be on this machine before a region can be built.

use std::path::Path;
use std::process::{Command, Stdio};

/// Programs `scripts/build-region.sh` needs.
const TOOLS: [&str; 6] = ["bash", "curl", "python3", "java", "docker", "cargo"];
const MIN_JAVA: u32 = 21;

/// Codes of everything that is missing; empty if a build can start. With
/// `tools` unset only the script itself is looked for.
pub(crate) fn missing(script: &Path, tools: bool) -> Vec<String> {
    let mut missing = Vec::new();
    if !cfg!(target_os = "linux") {
        missing.push("platform".to_string());
    }
    if !script.is_file() {
        missing.push("repo".to_string());
    }
    if !tools {
        return missing;
    }
    for tool in TOOLS {
        if !on_path(tool) {
            missing.push(tool.to_string());
        }
    }
    if on_path("java") && java_major().is_none_or(|major| major < MIN_JAVA) {
        missing.push("java_version".to_string());
    }
    if on_path("docker") && !runs(Command::new("docker").arg("info")) {
        missing.push("docker_daemon".to_string());
    }
    missing
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

fn runs(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Major version of the Java on the path.
fn java_major() -> Option<u32> {
    let output = Command::new("java").arg("-version").output().ok()?;
    parse_java_major(&String::from_utf8_lossy(&output.stderr))
}

/// Reads `openjdk version "21.0.2" …`; Java 8 and older call themselves `1.8`.
fn parse_java_major(text: &str) -> Option<u32> {
    let version = text.split('"').nth(1)?;
    let mut parts = version.split(['.', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_java_major;

    #[test]
    fn java_versions_are_read() {
        let v25 = "openjdk version \"25.0.5.0.3\" 2026-10-24\nOpenJDK Runtime";
        assert_eq!(parse_java_major(v25), Some(25));
        assert_eq!(parse_java_major("java version \"1.8.0_392\""), Some(8));
        assert_eq!(parse_java_major("openjdk version \"21-ea\" 2023"), Some(21));
        assert_eq!(parse_java_major("no java here"), None);
    }
}

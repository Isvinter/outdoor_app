//! Takes over the data directory of the app's former name, once, before Tauri starts.
//! Tauri creates the new directory while it opens the window, so this cannot wait.

use std::ffi::OsStr;
use std::path::Path;

/// The per-user data directory under the app's former identifier.
pub const LEGACY_DIR_NAME: &str = "de.outdoor-app.desktop";
/// The per-user data directory under the current identifier (`tauri.conf.json`).
pub const CURRENT_DIR_NAME: &str = "de.zeibig.zerogrid.base";

#[derive(Debug, PartialEq, Eq)]
pub enum Adoption {
    Adopted,
    NotNeeded,
}

/// Renames `legacy` to `current` if only `legacy` exists. Never copies or deletes, so a
/// failure leaves the data where it was.
pub fn adopt_legacy_dir(legacy: &Path, current: &Path) -> std::io::Result<Adoption> {
    if !legacy.is_dir() || current.symlink_metadata().is_ok() {
        return Ok(Adoption::NotNeeded);
    }
    std::fs::rename(legacy, current)?;
    Ok(Adoption::Adopted)
}

/// Adopts the data under the per-user `data_root`. With `data_override` (the app was told
/// to use another directory) the user's real data stays untouched.
pub fn adopt_user_data(
    data_override: Option<&OsStr>,
    data_root: Option<&Path>,
) -> std::io::Result<Adoption> {
    match (data_override, data_root) {
        (None, Some(root)) => {
            adopt_legacy_dir(&root.join(LEGACY_DIR_NAME), &root.join(CURRENT_DIR_NAME))
        }
        _ => Ok(Adoption::NotNeeded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A fresh directory for one test, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("zerogrid-adopt-{}", uuid::Uuid::now_v7()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn with_file(dir: &Path, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("outdoor.sqlite"), content).unwrap();
    }

    fn file(dir: &Path) -> String {
        fs::read_to_string(dir.join("outdoor.sqlite")).unwrap()
    }

    #[test]
    fn moves_legacy_dir_when_current_is_missing() {
        let root = Scratch::new();
        let (legacy, current) = (root.0.join("old"), root.0.join("new"));
        with_file(&legacy, "routes");

        assert_eq!(
            adopt_legacy_dir(&legacy, &current).unwrap(),
            Adoption::Adopted
        );

        assert_eq!(file(&current), "routes");
        assert!(!legacy.exists());
    }

    #[test]
    fn leaves_both_dirs_alone_when_both_exist() {
        let root = Scratch::new();
        let (legacy, current) = (root.0.join("old"), root.0.join("new"));
        with_file(&legacy, "old data");
        with_file(&current, "new data");

        assert_eq!(
            adopt_legacy_dir(&legacy, &current).unwrap(),
            Adoption::NotNeeded
        );

        assert_eq!(file(&legacy), "old data");
        assert_eq!(file(&current), "new data");
    }

    #[test]
    fn does_nothing_without_legacy_dir() {
        let root = Scratch::new();
        let (legacy, current) = (root.0.join("old"), root.0.join("new"));

        assert_eq!(
            adopt_legacy_dir(&legacy, &current).unwrap(),
            Adoption::NotNeeded
        );

        assert!(!current.exists());
    }

    #[test]
    fn override_never_touches_the_user_data_root() {
        let root = Scratch::new();
        let legacy = root.0.join(LEGACY_DIR_NAME);
        with_file(&legacy, "routes");

        let adoption =
            adopt_user_data(Some(std::ffi::OsStr::new("/somewhere/else")), Some(&root.0)).unwrap();

        assert_eq!(adoption, Adoption::NotNeeded);
        assert_eq!(file(&legacy), "routes");
        assert!(!root.0.join(CURRENT_DIR_NAME).exists());
    }

    #[test]
    fn adopts_under_the_user_data_root() {
        let root = Scratch::new();
        with_file(&root.0.join(LEGACY_DIR_NAME), "routes");

        assert_eq!(
            adopt_user_data(None, Some(&root.0)).unwrap(),
            Adoption::Adopted
        );

        assert_eq!(file(&root.0.join(CURRENT_DIR_NAME)), "routes");
    }

    #[test]
    fn failed_move_is_an_error_and_creates_no_current_dir() {
        let root = Scratch::new();
        let legacy = root.0.join("old");
        with_file(&legacy, "routes");
        let blocker = root.0.join("blocker");
        fs::write(&blocker, "a file, not a directory").unwrap();
        let current = blocker.join("new");

        assert!(adopt_legacy_dir(&legacy, &current).is_err());

        assert_eq!(file(&legacy), "routes");
        assert!(blocker.is_file());
    }
}

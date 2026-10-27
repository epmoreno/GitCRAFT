//! Persistent app settings (`settings.json` in the app config dir).
//!
//! Never stores credentials.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_RECENT_REPOSITORIES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepository {
    pub path: String,
    pub name: String,
    /// Seconds since the Unix epoch (exported as `number`: fits until 2106).
    #[specta(type = u32)]
    pub last_opened: u64,
}

/// Unknown fields are ignored and missing ones defaulted, so older and newer
/// versions of GitCRAFT can share the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub recent_repositories: Vec<RecentRepository>,
}

impl Settings {
    /// Moves `path` to the top of the recent list, capped at
    /// [`MAX_RECENT_REPOSITORIES`].
    pub fn record_opened(&mut self, path: &str, name: &str, now: u64) {
        self.recent_repositories.retain(|r| r.path != path);
        self.recent_repositories.insert(
            0,
            RecentRepository {
                path: path.to_owned(),
                name: name.to_owned(),
                last_opened: now,
            },
        );
        self.recent_repositories.truncate(MAX_RECENT_REPOSITORIES);
    }

    /// Drops recent entries whose directory no longer exists. Returns whether
    /// anything was removed.
    pub fn prune_missing(&mut self, exists: impl Fn(&Path) -> bool) -> bool {
        let before = self.recent_repositories.len();
        self.recent_repositories
            .retain(|r| exists(Path::new(&r.path)));
        self.recent_repositories.len() != before
    }
}

/// Reads and writes [`Settings`] to a JSON file.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// A missing or unreadable file yields default settings: losing the recent
    /// list must never prevent the app from starting.
    pub fn load(&self) -> Settings {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                eprintln!(
                    "ignoring corrupt settings file {}: {err}",
                    self.path.display()
                );
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    /// Writes atomically (temp file + rename) so a crash never leaves a
    /// half-written file.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(settings: &Settings) -> Vec<&str> {
        settings
            .recent_repositories
            .iter()
            .map(|r| r.path.as_str())
            .collect()
    }

    #[test]
    fn most_recent_first_without_duplicates() {
        let mut s = Settings::default();
        s.record_opened("/a", "a", 1);
        s.record_opened("/b", "b", 2);
        s.record_opened("/a", "a", 3);
        assert_eq!(paths(&s), ["/a", "/b"]);
        assert_eq!(s.recent_repositories[0].last_opened, 3);
    }

    #[test]
    fn keeps_at_most_twenty() {
        let mut s = Settings::default();
        for i in 0..25 {
            s.record_opened(&format!("/repo{i}"), "r", i);
        }
        assert_eq!(s.recent_repositories.len(), MAX_RECENT_REPOSITORIES);
        assert_eq!(s.recent_repositories[0].path, "/repo24");
        assert_eq!(s.recent_repositories[19].path, "/repo5");
    }

    #[test]
    fn prunes_paths_that_no_longer_exist() {
        let mut s = Settings::default();
        s.record_opened("/gone", "gone", 1);
        s.record_opened("/here", "here", 2);
        assert!(s.prune_missing(|p| p == Path::new("/here")));
        assert_eq!(paths(&s), ["/here"]);
        assert!(!s.prune_missing(|_| true));
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = SettingsFile::new(dir.path().join("nested/settings.json"));
        assert_eq!(file.load(), Settings::default());

        let mut s = Settings::default();
        s.record_opened("/a b/ñ", "ñ", 42);
        file.save(&s).expect("save");
        assert_eq!(file.load(), s);
        assert!(!dir.path().join("nested/settings.json.tmp").exists());
    }

    #[test]
    fn corrupt_or_foreign_files_fall_back_to_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let file = SettingsFile::new(&path);

        std::fs::write(&path, "{ not json").expect("write");
        assert_eq!(file.load(), Settings::default());

        std::fs::write(
            &path,
            r#"{"futureSetting": true, "recentRepositories": [{"path": "/x", "name": "x", "lastOpened": 5}]}"#,
        )
        .expect("write");
        assert_eq!(file.load().recent_repositories.len(), 1);
    }
}

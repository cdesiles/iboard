//! What needs the author's attention, kept beside the board in
//! `issues/.notifications.json`: a build, review or ask that finished while
//! the board was away or while another issue was open. Questions are not
//! stored: the opencode service still holds them, so they are read live.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::files;
use crate::settings::{Section, UserConfig};

pub const FILE: &str = ".notifications.json";

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    BuildDone,
    BuildFailed,
    ReviewReady,
    AskDone,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::BuildDone => "build_done",
            Kind::BuildFailed => "build_failed",
            Kind::ReviewReady => "review_ready",
            Kind::AskDone => "ask_done",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        [
            Kind::BuildDone,
            Kind::BuildFailed,
            Kind::ReviewReady,
            Kind::AskDone,
        ]
        .into_iter()
        .find(|kind| kind.key() == key)
    }
}

/// One notice, as the bell lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub title: String,
    pub detail: String,
    pub at: SystemTime,
    pub read: bool,
}

/// The file's shape. Fields added later must keep an empty default.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Stored {
    id: String,
    kind: String,
    path: String,
    title: String,
    detail: String,
    at: String,
    #[serde(default)]
    read: bool,
}

/// The file a project's notifications live in, under its issues root.
pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// A project's notifications; an unreadable or broken file is empty, never an
/// error, so the board still opens.
pub fn load(root: &Path) -> Vec<Notification> {
    let Ok(text) = std::fs::read_to_string(path(root)) else {
        return Vec::new();
    };
    let Ok(stored) = serde_json::from_str::<Vec<Stored>>(&text) else {
        return Vec::new();
    };
    stored
        .into_iter()
        .filter_map(|stored| {
            Some(Notification {
                kind: Kind::from_key(&stored.kind)?,
                at: chrono::DateTime::parse_from_rfc3339(&stored.at)
                    .ok()
                    .map(|at| {
                        let millis = at.timestamp_millis();
                        if millis >= 0 {
                            SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64)
                        } else {
                            SystemTime::UNIX_EPOCH - Duration::from_millis((-millis) as u64)
                        }
                    })
                    .unwrap_or(SystemTime::UNIX_EPOCH),
                id: stored.id,
                path: PathBuf::from(stored.path),
                title: stored.title,
                detail: stored.detail,
                read: stored.read,
            })
        })
        .collect()
}

/// Writes a project's notifications, through a temp file and a rename.
pub fn save(root: &Path, items: &[Notification]) -> Result<(), String> {
    let stored: Vec<Stored> = items
        .iter()
        .map(|item| Stored {
            id: item.id.clone(),
            kind: item.kind.key().to_string(),
            path: item.path.display().to_string(),
            title: item.title.clone(),
            detail: item.detail.clone(),
            at: chrono::DateTime::<chrono::Utc>::from(item.at).to_rfc3339(),
            read: item.read,
        })
        .collect();
    let text = serde_json::to_string_pretty(&stored)
        .map_err(|err| format!("Couldn't write notifications: {err}"))?;
    files::atomic_write(&path(root), text)
        .map_err(|err| format!("Couldn't write {}: {err}", path(root).display()))
}

/// The `[notifications]` switches, all on unless the user turns one off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    pub build_done: bool,
    pub build_failed: bool,
    pub review_ready: bool,
    pub ask_done: bool,
    /// Open a Doing issue's agent log as the ticket opens.
    pub auto_log: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            build_done: true,
            build_failed: true,
            review_ready: true,
            ask_done: true,
            auto_log: true,
        }
    }
}

impl Prefs {
    /// Whether `kind` should reach the bell.
    pub fn wants(self, kind: Kind) -> bool {
        match kind {
            Kind::BuildDone => self.build_done,
            Kind::BuildFailed => self.build_failed,
            Kind::ReviewReady => self.review_ready,
            Kind::AskDone => self.ask_done,
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
struct RawPrefs {
    build_done: Option<bool>,
    build_failed: Option<bool>,
    review_ready: Option<bool>,
    ask_done: Option<bool>,
    auto_log: Option<bool>,
}

pub fn load_prefs(config: &mut UserConfig) -> Prefs {
    let raw: RawPrefs = config.section("notifications");
    let default = Prefs::default();
    Prefs {
        build_done: raw.build_done.unwrap_or(default.build_done),
        build_failed: raw.build_failed.unwrap_or(default.build_failed),
        review_ready: raw.review_ready.unwrap_or(default.review_ready),
        ask_done: raw.ask_done.unwrap_or(default.ask_done),
        auto_log: raw.auto_log.unwrap_or(default.auto_log),
    }
}

pub fn prefs_section(prefs: &Prefs) -> Result<Section, String> {
    Section::new(
        "notifications",
        &RawPrefs {
            build_done: Some(prefs.build_done),
            build_failed: Some(prefs.build_failed),
            review_ready: Some(prefs.review_ready),
            ask_done: Some(prefs.ask_done),
            auto_log: Some(prefs.auto_log),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(id: &str, kind: Kind, read: bool) -> Notification {
        Notification {
            id: id.into(),
            kind,
            path: PathBuf::from("/code/app/issues/doing/001-build.md"),
            title: "Build it".into(),
            detail: "Build finished".into(),
            at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            read,
        }
    }

    #[test]
    fn a_notification_round_trips_and_keeps_its_read_state() {
        let dir = tempfile::tempdir().unwrap();
        let items = [
            notice("a", Kind::BuildDone, false),
            notice("b", Kind::ReviewReady, true),
        ];
        save(dir.path(), &items).unwrap();
        assert_eq!(load(dir.path()), items);
    }

    #[test]
    fn a_broken_file_is_empty_and_never_stops_the_board() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path(dir.path()), "not json").unwrap();
        assert!(load(dir.path()).is_empty());
    }

    #[test]
    fn the_switches_all_default_to_on() {
        let prefs = Prefs::default();
        for kind in [
            Kind::BuildDone,
            Kind::BuildFailed,
            Kind::ReviewReady,
            Kind::AskDone,
        ] {
            assert!(prefs.wants(kind));
        }
        assert!(prefs.auto_log);
    }
}

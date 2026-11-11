//! What a card shows beyond its file: when git says it was done, and how long
//! its last build took.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Local};

use crate::build_log;
use crate::domains::board::discovery::{self, Board};
use crate::history;

/// The facts a board reads outside its issue files, by issue path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardFacts {
    done: HashMap<PathBuf, DateTime<Local>>,
    built: HashMap<PathBuf, Duration>,
}

impl CardFacts {
    /// Reads the Done column's git history and the agent log of each issue in
    /// it.
    pub fn read(board: &Board) -> Self {
        let mut facts = Self::default();
        let done = board
            .columns
            .iter()
            .filter(|column| discovery::is_done_column(&column.dir));
        for column in done {
            let paths: Vec<&Path> = column
                .issues
                .iter()
                .map(|issue| issue.path.as_path())
                .collect();
            facts.done.extend(history::dates_for(&paths));
            for issue in &column.issues {
                let log = build_log::path(board.root_of(&issue.path), &issue.path);
                let took = build_log::load(&log).as_deref().and_then(build_log::took);
                if let Some(took) = took {
                    facts.built.insert(issue.path.clone(), took);
                }
            }
        }
        facts
    }

    /// When git, or the file's own times, says the issue was done.
    pub fn done_at(&self, path: &Path) -> Option<DateTime<Local>> {
        self.done.get(path).copied()
    }

    /// How long the agent's last build of the issue took.
    pub fn built_in(&self, path: &Path) -> Option<Duration> {
        self.built.get(path).copied()
    }
}

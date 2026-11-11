//! A plan: one open conversation with the agent that can end in tickets.
//!
//! The plan is scratch, not record. Its tool session id, transcript and kind
//! live in one small file in the cache folder, keyed by project, next to
//! `iboard.sock`. Reopening only offers to continue it; it never resumes on
//! its own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::platform::files;

/// What the conversation is about, from its first topic or a chip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Spike,
    #[default]
    Epic,
    Bug,
    Debt,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Spike, Kind::Epic, Kind::Bug, Kind::Debt];

    /// The name in the chip and the plan's prompt.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Spike => "Spike",
            Kind::Epic => "Epic",
            Kind::Bug => "Bug review",
            Kind::Debt => "Tech debt review",
        }
    }

    /// The one line that widens the prompt's kind.
    pub fn blurb(self) -> &'static str {
        match self {
            Kind::Spike => "an investigation whose outcome is knowledge, not a change",
            Kind::Epic => "a feature that is worth more than one ticket",
            Kind::Bug => "a bug to reproduce, understand and fix",
            Kind::Debt => "technical debt to pay down, rather than a feature",
        }
    }

    /// The labels a ticket of this kind keeps.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Spike => "discovery",
            Kind::Epic => "feat",
            Kind::Bug => "bug",
            Kind::Debt => "refactor",
        }
    }

    /// The sections the agent drives the conversation toward.
    pub fn sections(self) -> &'static [&'static str] {
        match self {
            Kind::Spike => &[
                "Question",
                "Context",
                "What to investigate",
                "What we would do next",
            ],
            Kind::Epic => &[
                "Problem",
                "Outcome",
                "Context",
                "Acceptance criteria",
                "Out of scope",
            ],
            Kind::Bug => &[
                "Symptom",
                "Expected",
                "Actual",
                "Steps to reproduce",
                "Acceptance criteria",
            ],
            Kind::Debt => &[
                "Debt",
                "Where it costs",
                "Context",
                "Scope",
                "Acceptance criteria",
            ],
        }
    }

    /// Infers the kind from the first topic, in the order that matters: a
    /// reported failure is a bug before it is a spike, and debt is debt
    /// before it is an epic.
    pub fn infer(topic: &str) -> Kind {
        let topic = topic.to_lowercase();
        let says = |words: &[&str]| words.iter().any(|word| topic.contains(word));
        if says(&[
            "bug",
            "crash",
            "broken",
            "regression",
            "fails",
            "failing",
            "error",
            "wrong",
            "drops",
        ]) {
            Kind::Bug
        } else if says(&[
            "debt",
            "refactor",
            "cleanup",
            "clean up",
            "legacy",
            "complexity",
        ]) {
            Kind::Debt
        } else if says(&[
            "spike",
            "investigate",
            "research",
            "explore",
            "discovery",
            "prototype",
            "unknown",
        ]) {
            Kind::Spike
        } else {
            Kind::Epic
        }
    }
}

/// Who spoke one turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Author,
    Agent,
}

impl Speaker {
    pub fn name(self) -> &'static str {
        match self {
            Speaker::Author => "You",
            Speaker::Agent => "Agent",
        }
    }
}

/// One turn of the plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub speaker: Speaker,
    pub text: String,
}

/// The plan: the tool session, the transcript and the kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The plan's own id, handed to a custom command as `{session}`.
    #[serde(default)]
    pub id: String,
    /// The tool's session id, once it named one; `None` on the first turn.
    #[serde(default)]
    pub session: Option<String>,
    /// `None` until the first topic infers it, or a chip picks it.
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub turns: Vec<Turn>,
}

impl Default for Plan {
    fn default() -> Self {
        Self::fresh()
    }
}

impl Plan {
    /// A plan with a new id and nothing said yet.
    pub fn fresh() -> Self {
        Plan {
            id: new_id(),
            session: None,
            kind: None,
            turns: Vec::new(),
        }
    }

    /// The kind shown and used now.
    pub fn kind(&self) -> Kind {
        self.kind.unwrap_or_default()
    }

    /// The chip's choice, and the kind later turns keep.
    pub fn set_kind(&mut self, kind: Kind) {
        self.kind = Some(kind);
    }

    /// Adds the author's turn, inferring the kind from the very first topic.
    pub fn ask(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.turns.is_empty() && self.kind.is_none() {
            self.kind = Some(Kind::infer(text));
        }
        self.turns.push(Turn {
            speaker: Speaker::Author,
            text: text.to_string(),
        });
    }

    /// Adds the agent's turn.
    pub fn answer(&mut self, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.turns.push(Turn {
                speaker: Speaker::Agent,
                text: text.to_string(),
            });
        }
    }

    /// The topic that opened the plan.
    pub fn topic(&self) -> Option<&str> {
        self.turns
            .iter()
            .find(|turn| turn.speaker == Speaker::Author)
            .map(|turn| turn.text.as_str())
    }

    /// The author's latest words.
    pub fn last_author(&self) -> Option<&str> {
        self.turns
            .iter()
            .rev()
            .find(|turn| turn.speaker == Speaker::Author)
            .map(|turn| turn.text.as_str())
    }

    /// Whether the agent has answered at least once.
    pub fn has_exchange(&self) -> bool {
        self.turns.iter().any(|turn| turn.speaker == Speaker::Agent)
    }
}

/// A fresh plan id, unique enough across runs and machines.
fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("iboard-plan-{}-{millis}-{n}", std::process::id())
}

/// Where `project`'s plan is kept, or `None` without a cache folder.
pub fn file(project: &Path) -> Option<PathBuf> {
    files::cache_dir().map(|dir| path_in(&dir, project))
}

/// The plan file for `project` under `dir`, next to `iboard.sock`.
pub fn path_in(dir: &Path, project: &Path) -> PathBuf {
    let text = project.to_string_lossy();
    let trimmed = text.trim_end_matches(std::path::MAIN_SEPARATOR);
    let trimmed = if trimmed.is_empty() { &*text } else { trimmed };
    let project = std::fs::canonicalize(trimmed).unwrap_or_else(|_| PathBuf::from(trimmed));
    let name = files::slugify(&files::file_name(&project));
    let hash = hash(project.to_string_lossy().as_bytes());
    dir.join(format!("plan-{name}-{hash:016x}.json"))
}

/// A plan read back from `path`, if it is one.
pub fn load(path: &Path) -> Option<Plan> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Stores `plan` at `path`, creating the cache folder if needed.
pub fn save(path: &Path, plan: &Plan) -> Result<(), String> {
    let text = serde_json::to_string_pretty(plan)
        .map_err(|err| format!("Couldn't write the plan: {err}"))?;
    files::write_creating(path, text)
        .map_err(|err| format!("Couldn't write {}: {err:?}", path.display()))
}

/// Forgets the plan at `path`: starting fresh leaves nothing behind.
pub fn forget(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// FNV-1a over a project path: stable across runs, unlike `DefaultHasher`.
fn hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_topic_infers_the_kind() {
        assert_eq!(Kind::infer("Investigate how sync works"), Kind::Spike);
        assert_eq!(Kind::infer("A calendar view for the board"), Kind::Epic);
        assert_eq!(Kind::infer("The board crashes on open"), Kind::Bug);
        assert_eq!(Kind::infer("Refactor the discovery module"), Kind::Debt);
        assert_eq!(Kind::infer("Pay down the debt in the reader"), Kind::Debt);
        assert_eq!(Kind::infer("Investigate a crash in sync"), Kind::Bug);
    }

    #[test]
    fn a_chosen_kind_survives_the_first_topic() {
        let mut plan = Plan::fresh();
        plan.set_kind(Kind::Spike);
        plan.ask("A calendar view for the board");
        assert_eq!(plan.kind(), Kind::Spike);
    }

    #[test]
    fn the_transcript_keeps_who_spoke() {
        let mut plan = Plan::fresh();
        plan.ask("  A calendar view ");
        plan.answer("");
        plan.answer("Who uses it?");
        assert_eq!(plan.topic(), Some("A calendar view"));
        assert_eq!(plan.last_author(), Some("A calendar view"));
        assert!(plan.has_exchange());
        assert_eq!(plan.kind(), Kind::Epic);
        plan.ask("Everyone");
        assert_eq!(plan.last_author(), Some("Everyone"));
        assert_eq!(plan.topic(), Some("A calendar view"));
    }

    #[test]
    fn the_plan_round_trips_through_its_cache_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = Path::new("/code/app");
        let path = path_in(dir.path(), project);
        let mut plan = Plan::fresh();
        plan.ask("A calendar view");
        plan.answer("Who uses it?");
        plan.set_kind(Kind::Spike);
        plan.session = Some("ses_1".to_string());

        save(&path, &plan).unwrap();
        assert_eq!(load(&path), Some(plan.clone()));

        forget(&path);
        assert_eq!(load(&path), None);
    }

    #[test]
    fn the_plan_file_is_keyed_by_project() {
        let dir = Path::new("/cache");
        let one = path_in(dir, Path::new("/code/app"));
        let same = path_in(dir, Path::new("/code/app/"));
        let other = path_in(dir, Path::new("/code/site"));
        assert_eq!(one, same);
        assert_ne!(one, other);
        assert!(
            one.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("plan-app-")
        );
        assert_eq!(one.parent(), Some(dir));
    }

    #[test]
    fn fresh_plans_get_distinct_ids() {
        let one = Plan::fresh();
        let other = Plan::fresh();
        assert!(!one.id.is_empty());
        assert_ne!(one.id, other.id);
        assert_eq!(one.kind, None);
    }
}

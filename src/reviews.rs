//! An issue's last review and its comments, kept beside the board in
//! `issues/.reviews/<slug>.toml`. The slug is the file name without its
//! `001-` priority, so renumbering a column or moving the issue keeps them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::discovery;
use crate::files;

/// The folder under the issues root; discovery skips dot-folders.
pub const DIR: &str = ".reviews";

/// Everything kept for one issue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Sidecar {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<StoredReview>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
}

/// The agent's last answer, and the text it answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct StoredReview {
    /// [`content_hash`] of the text the agent read.
    pub hash: String,
    /// The tool and model, as `opencode · default`.
    pub agent: String,
    /// True when the agent read an unsaved editor draft.
    #[serde(default)]
    pub draft: bool,
    /// When the answer came in, RFC 3339.
    pub at: String,
    pub raw: String,
}

impl StoredReview {
    /// Whether the issue changed since the agent read it.
    pub fn is_stale(&self, markdown: &str) -> bool {
        self.hash != content_hash(markdown)
    }
}

/// A note the reader left on one block of the issue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Comment {
    pub id: u32,
    /// The source text of the block it is attached to.
    pub anchor: String,
    pub note: String,
    pub at: String,
    /// Whether a review already carried it to the agent.
    #[serde(default)]
    pub sent: bool,
}

/// A stable 64-bit FNV-1a of `text`, in hex: stored on disk, so it must not
/// depend on the Rust version the way `DefaultHasher` does.
pub fn content_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// The key an issue's sidecar goes by: `003-split.md` → `split`.
pub fn slug(issue: &Path) -> String {
    let name = files::file_name(issue);
    let name = discovery::strip_priority(&name);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn sidecar_path(root: &Path, issue: &Path) -> PathBuf {
    root.join(DIR).join(format!("{}.toml", slug(issue)))
}

/// The sidecar of `issue`; empty when there is none or it can't be read.
pub fn load(root: &Path, issue: &Path) -> Sidecar {
    std::fs::read_to_string(sidecar_path(root, issue))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Every sidecar under `root`, by slug. Unreadable ones are skipped.
pub fn load_all(root: &Path) -> HashMap<String, Sidecar> {
    let Ok(entries) = std::fs::read_dir(root.join(DIR)) else {
        return HashMap::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let slug = path
                .file_name()?
                .to_str()?
                .strip_suffix(".toml")?
                .to_string();
            let text = std::fs::read_to_string(&path).ok()?;
            Some((slug, toml::from_str(&text).ok()?))
        })
        .collect()
}

/// Writes the sidecar of `issue`, creating `.reviews/` on first use.
pub fn save(root: &Path, issue: &Path, sidecar: &Sidecar) -> Result<(), String> {
    let path = sidecar_path(root, issue);
    let text =
        toml::to_string(sidecar).map_err(|err| format!("Couldn't encode a review: {err}"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;
    }
    files::atomic_write(&path, text)
        .map_err(|err| format!("Couldn't save {}: {err}", path.display()))
}

/// Keeps `review` as the last review of `issue`, with its comments.
pub fn save_review(root: &Path, issue: &Path, review: StoredReview) -> Result<(), String> {
    let mut sidecar = load(root, issue);
    sidecar.review = Some(review);
    save(root, issue, &sidecar)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(markdown: &str) -> StoredReview {
        StoredReview {
            hash: content_hash(markdown),
            agent: "opencode · default".into(),
            draft: false,
            at: "2026-10-12T10:00:00+02:00".into(),
            raw: "RELEVANCE: Pass\nok".into(),
        }
    }

    #[test]
    fn a_sidecar_round_trips_and_keeps_its_comments() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        let issue = root.join("todo/split.md");
        let sidecar = Sidecar {
            review: None,
            comments: vec![Comment {
                id: 1,
                anchor: "Three files over 1,100 lines.".into(),
                note: "Only design.rs matters now".into(),
                at: "2026-10-12T10:00:00+02:00".into(),
                sent: false,
            }],
        };
        save(root, &issue, &sidecar).unwrap();
        save_review(root, &issue, review("# Split\n")).unwrap();

        let loaded = load(root, &issue);
        assert_eq!(loaded.comments, sidecar.comments);
        assert_eq!(loaded.review, Some(review("# Split\n")));
        assert_eq!(load_all(root).get("split"), Some(&loaded));
    }

    #[test]
    fn renumbering_and_moving_keep_the_sidecar() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        save_review(root, &root.join("ready/001-x.md"), review("# X\n")).unwrap();
        assert!(load(root, &root.join("ready/003-x.md")).review.is_some());
        assert!(load(root, &root.join("todo/x.md")).review.is_some());
        assert!(load(root, &root.join("todo/y.md")).review.is_none());
    }

    #[test]
    fn an_edit_makes_the_review_stale() {
        let stored = review("# X\n\nBody.\n");
        assert!(!stored.is_stale("# X\n\nBody.\n"));
        assert!(stored.is_stale("# X\n\nBody, edited.\n"));
        assert_eq!(content_hash(""), "cbf29ce484222325");
    }

    #[test]
    fn a_missing_or_broken_sidecar_is_empty() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        let issue = root.join("todo/x.md");
        assert_eq!(load(root, &issue), Sidecar::default());
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        std::fs::write(root.join(DIR).join("x.toml"), "not = [toml").unwrap();
        assert_eq!(load(root, &issue), Sidecar::default());
        assert!(load_all(root).is_empty());
    }
}

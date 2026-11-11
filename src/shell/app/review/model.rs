//! What the review keeps between frames: the runs in the background, the
//! panel's state, and the words and colours of a card's review pill.

use std::collections::HashSet;

use super::{Color32, HashMap, Job, Palette, PastReview, Path, PathBuf, ReviewMode, Split, agent};
use crate::reviews::verdict_hint;

/// What a review is called while it runs and once it is in.
pub(in crate::shell::app) const REVIEW_RUNNING: &str = "Reviewing…";
/// The key that cancels a review.
pub(in crate::shell::app) const REVIEW_KEY: &str = "⌘⇧R";
const REVIEW_RUNNING_HINT: &str = "The review is running";
pub(in crate::shell::app) const REVIEW_READY: &str = "Review ready";
pub(in crate::shell::app) const REVIEW_READY_HINT: &str = "The review is in: show it";

/// The review panel on one issue: its findings, or the split they led to.
pub(in crate::shell::app) struct ReviewState {
    pub(in crate::shell::app) path: PathBuf,
    pub(in crate::shell::app) review: PastReview,
    /// The split the findings led to, shown in their place.
    pub(in crate::shell::app) split: Option<Split>,
    /// The "Ask again" follow-up, once its button opened the field.
    pub(in crate::shell::app) follow_up: Option<String>,
}

impl ReviewState {
    /// The rewrite Apply and `⌘↩` take to the editor: the review's, while no
    /// split runs or shows its tickets in its place.
    pub(in crate::shell::app) fn proposal(&self) -> Option<&str> {
        if matches!(self.split, Some(Split::Cutting { .. } | Split::Ready(_))) {
            return None;
        }
        self.review.report.as_ref()?.proposal.as_deref()
    }
}

/// The reviews running in the background, one per issue, and the issues
/// whose review landed since it was last shown.
#[derive(Default)]
pub(in crate::shell::app) struct ReviewRuns {
    pub(in crate::shell::app) running: HashMap<PathBuf, ReviewRun>,
    pub(super) ready: HashSet<PathBuf>,
}

/// A review the agent is working on, and what to keep with its answer.
pub(in crate::shell::app) struct ReviewRun {
    pub(super) draft: bool,
    pub(super) mode: ReviewMode,
    pub(super) agent: String,
    /// [`reviews::content_hash`](crate::reviews::content_hash) of the text
    /// the agent reads.
    pub(super) hash: String,
    pub(super) answer: Job<Result<agent::Reviewed, String>>,
    /// The step the agent is on, for the panel.
    pub(in crate::shell::app) progress: agent::Progress,
}

impl ReviewRuns {
    pub(in crate::shell::app) fn is_running(&self, path: &Path) -> bool {
        self.running.contains_key(path)
    }

    pub(in crate::shell::app) fn is_ready(&self, path: &Path) -> bool {
        self.ready.contains(path)
    }

    pub(in crate::shell::app) fn any_running(&self) -> bool {
        !self.running.is_empty()
    }

    /// How many reviews run in the background.
    pub(in crate::shell::app) fn running_count(&self) -> usize {
        self.running.len()
    }

    /// Stops the run on `path`, if any, and drops its ready mark.
    pub(in crate::shell::app) fn forget(&mut self, path: &Path) {
        self.running.remove(path);
        self.ready.remove(path);
    }

    /// Follows an issue renamed from `old` to `new`.
    pub(in crate::shell::app) fn rename(&mut self, old: &Path, new: &Path) {
        if let Some(run) = self.running.remove(old) {
            self.running.insert(new.to_path_buf(), run);
        }
        if self.ready.remove(old) {
            self.ready.insert(new.to_path_buf());
        }
    }
}

/// A card's review pill: its word, colour and tooltip.
pub(in crate::shell::app) type ReviewPill = (&'static str, Color32, &'static str);

/// The pill of a past review: the worst verdict of the three, or "Reviewed"
/// when the answer did not parse.
pub(in crate::shell::app) fn past_pill(past: &PastReview, p: &Palette) -> (&'static str, Color32) {
    past.verdict()
        .map_or(("Reviewed", p.review), |verdict| verdict_look(verdict, p))
}

/// The card's pill of a past review: the verdict, faint once the issue
/// changed since.
pub(in crate::shell::app) fn card_pill(past: &PastReview, p: &Palette) -> ReviewPill {
    let (word, color) = past_pill(past, p);
    let color = if past.stale { p.ink_faint } else { color };
    (word, color, verdict_hint(past.verdict(), past.stale))
}

/// A verdict's word, and the colour of its pill.
pub(super) fn verdict_look(verdict: agent::Verdict, p: &Palette) -> (&'static str, Color32) {
    match verdict {
        agent::Verdict::Pass => ("Pass", p.status_done),
        agent::Verdict::Challenge => ("Challenge", p.review),
        agent::Verdict::Block => ("Block", p.danger),
    }
}

/// What the cards' review pill says: the review running or ready, else the
/// last verdict.
#[derive(Clone, Copy)]
pub(in crate::shell::app) struct CardReviews<'a> {
    pub(in crate::shell::app) past: &'a HashMap<PathBuf, PastReview>,
    pub(in crate::shell::app) runs: &'a ReviewRuns,
}

impl CardReviews<'_> {
    pub(in crate::shell::app) fn pill(&self, path: &Path, p: &Palette) -> Option<ReviewPill> {
        if self.runs.is_running(path) {
            return Some((REVIEW_RUNNING, p.review, REVIEW_RUNNING_HINT));
        }
        if self.runs.is_ready(path) {
            return Some((REVIEW_READY, p.review, REVIEW_READY_HINT));
        }
        self.past.get(path).map(|past| card_pill(past, p))
    }
}

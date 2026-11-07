//! What the review keeps between frames: the runs in the background, the
//! panel's state, and the words and colours of a card's review pill.

use super::{Color32, HashMap, Job, Palette, PastReview, Path, PathBuf, ReviewMode, Split, agent};

/// What a run asked the agent, for what the button and the card call it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum RunKind {
    Review,
    /// Update from the code.
    Update,
}

/// What a run of one kind is called while it runs and once it is in.
struct RunWords {
    running: &'static str,
    /// The key that cancels a run of this kind, and only this kind.
    key: &'static str,
    running_hint: &'static str,
    ready: &'static str,
    ready_hint: &'static str,
}

const REVIEW_WORDS: RunWords = RunWords {
    running: "Reviewing…",
    key: "⌘⇧R",
    running_hint: "The review is running",
    ready: "Review ready",
    ready_hint: "The review is in: show it",
};

const UPDATE_WORDS: RunWords = RunWords {
    running: "Updating…",
    key: "⌘U",
    running_hint: "The update is running",
    ready: "Update ready",
    ready_hint: "The agent's answer is in: show it",
};

impl RunKind {
    fn words(self) -> &'static RunWords {
        match self {
            RunKind::Review => &REVIEW_WORDS,
            RunKind::Update => &UPDATE_WORDS,
        }
    }

    pub(in crate::app) fn running(self) -> &'static str {
        self.words().running
    }

    pub(in crate::app) fn key(self) -> &'static str {
        self.words().key
    }

    fn running_hint(self) -> &'static str {
        self.words().running_hint
    }

    pub(super) fn ready(self) -> &'static str {
        self.words().ready
    }

    pub(super) fn ready_hint(self) -> &'static str {
        self.words().ready_hint
    }
}

/// The review panel on one issue: its findings, or the split they led to.
pub(in crate::app) struct ReviewState {
    pub(in crate::app) path: PathBuf,
    pub(in crate::app) review: PastReview,
    /// The split the findings led to, shown in their place.
    pub(in crate::app) split: Option<Split>,
    /// The "Ask again" follow-up, once its button opened the field.
    pub(in crate::app) follow_up: Option<String>,
}

impl ReviewState {
    /// The rewrite Apply and `⌘↩` take to the editor: the review's, while no
    /// split runs or shows its tickets in its place.
    pub(in crate::app) fn proposal(&self) -> Option<&str> {
        if matches!(self.split, Some(Split::Cutting { .. } | Split::Ready(_))) {
            return None;
        }
        self.review.report.as_ref()?.proposal.as_deref()
    }
}

/// The reviews running in the background, one per issue, and the issues
/// whose review landed since it was last shown.
#[derive(Default)]
pub(in crate::app) struct ReviewRuns {
    pub(in crate::app) running: HashMap<PathBuf, ReviewRun>,
    pub(super) ready: HashMap<PathBuf, RunKind>,
}

/// A review the agent is working on, and what to keep with its answer.
pub(in crate::app) struct ReviewRun {
    pub(in crate::app) kind: RunKind,
    pub(super) draft: bool,
    pub(super) mode: ReviewMode,
    pub(super) agent: String,
    /// [`reviews::content_hash`](crate::reviews::content_hash) of the text
    /// the agent reads.
    pub(super) hash: String,
    pub(super) answer: Job<Result<agent::Reviewed, String>>,
    /// The step the agent is on, for the panel.
    pub(in crate::app) progress: agent::Progress,
}

impl ReviewRuns {
    pub(in crate::app) fn is_running(&self, path: &Path) -> bool {
        self.running.contains_key(path)
    }

    /// Whether a run of `kind` works on `path`.
    pub(in crate::app) fn runs(&self, path: &Path, kind: RunKind) -> bool {
        self.running.get(path).is_some_and(|run| run.kind == kind)
    }

    pub(in crate::app) fn is_ready(&self, path: &Path) -> bool {
        self.ready.contains_key(path)
    }

    pub(in crate::app) fn any_running(&self) -> bool {
        !self.running.is_empty()
    }

    /// How many reviews run in the background.
    pub(in crate::app) fn running_count(&self) -> usize {
        self.running.len()
    }

    /// Stops the run on `path`, if any, and drops its ready mark.
    pub(in crate::app) fn forget(&mut self, path: &Path) {
        self.running.remove(path);
        self.ready.remove(path);
    }

    /// Follows an issue renamed from `old` to `new`.
    pub(in crate::app) fn rename(&mut self, old: &Path, new: &Path) {
        if let Some(run) = self.running.remove(old) {
            self.running.insert(new.to_path_buf(), run);
        }
        if let Some(kind) = self.ready.remove(old) {
            self.ready.insert(new.to_path_buf(), kind);
        }
    }
}

/// A card's review pill: its word, colour and tooltip.
pub(in crate::app) type ReviewPill = (&'static str, Color32, &'static str);

/// The pill of a past review: the worst verdict of the three, or "Reviewed"
/// when the answer did not parse.
pub(in crate::app) fn past_pill(past: &PastReview, p: &Palette) -> (&'static str, Color32) {
    past.verdict()
        .map_or(("Reviewed", p.review), |verdict| verdict_look(verdict, p))
}

/// The card's pill of a past review: the verdict, faint once the issue
/// changed since.
pub(in crate::app) fn card_pill(past: &PastReview, p: &Palette) -> ReviewPill {
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

/// What a card's verdict pill means: the worst of the review's three axes,
/// then whether the issue changed since.
fn verdict_hint(verdict: Option<agent::Verdict>, stale: bool) -> &'static str {
    macro_rules! hints {
        ($said:literal) => {
            [
                concat!("Last AI review", $said, ". Click to show it"),
                concat!(
                    "Last AI review",
                    $said,
                    ", before the issue's last edit. Click to show it"
                ),
            ]
        };
    }
    let hints = match verdict {
        Some(agent::Verdict::Pass) => hints!(": Pass. Relevance, size and clarity are all fine"),
        Some(agent::Verdict::Challenge) => {
            hints!(": Challenge. Relevance, size or clarity is worth another look, nothing blocks")
        }
        Some(agent::Verdict::Block) => hints!(
            ": Block. Relevance, size or clarity is a real problem \
             (for an update: done, or no longer worth doing)"
        ),
        None => hints!(""),
    };
    hints[usize::from(stale)]
}

/// What the cards' review pill says: the review running or ready, else the
/// last verdict.
#[derive(Clone, Copy)]
pub(in crate::app) struct CardReviews<'a> {
    pub(in crate::app) past: &'a HashMap<PathBuf, PastReview>,
    pub(in crate::app) runs: &'a ReviewRuns,
}

impl CardReviews<'_> {
    pub(in crate::app) fn pill(&self, path: &Path, p: &Palette) -> Option<ReviewPill> {
        if let Some(run) = self.runs.running.get(path) {
            return Some((run.kind.running(), p.review, run.kind.running_hint()));
        }
        if let Some(kind) = self.runs.ready.get(path) {
            return Some((kind.ready(), p.review, kind.ready_hint()));
        }
        self.past.get(path).map(|past| card_pill(past, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_hint_is_the_verdict_s_and_the_edit_since() {
        let verdicts = [
            None,
            Some(agent::Verdict::Pass),
            Some(agent::Verdict::Challenge),
            Some(agent::Verdict::Block),
        ];
        for verdict in verdicts {
            let fresh = verdict_hint(verdict, false);
            let stale = verdict_hint(verdict, true);
            let said = fresh.strip_suffix(". Click to show it").unwrap();
            assert_eq!(
                stale,
                format!("{said}, before the issue's last edit. Click to show it")
            );
        }
    }
}

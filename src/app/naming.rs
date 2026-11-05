//! Naming a new issue in the background: the composer writes it at once as
//! `unnamed-ticket-xxxx`, several engines may be at work, and each title lands
//! when it lands, wherever the issue has moved to.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use super::{
    Duration, Engine, IboardApp, Instant, Job, Path, atomic_write, file_name, job, reviews, titles,
    unique_name,
};
use crate::assets::{self, Carry};
use crate::issue;
use iboard::markdown;

/// How often a title held back by the Editor checks whether it can land.
const HELD_POLL_EVERY: Duration = Duration::from_secs(1);

/// An engine at work on an issue, known by the placeholder it was written
/// with: a file that lost it was renamed by hand.
pub(super) struct Naming {
    pub(super) placeholder: String,
    pub(super) answer: Job<Result<String, String>>,
    /// When the wait stops and the placeholder stays.
    pub(super) deadline: Instant,
    /// The answer, waiting for the Editor or a pressed pointer to let go.
    title: Option<String>,
}

/// Whether the issue at `path` still waits for its title.
pub(super) fn is_naming(namings: &[Naming], path: &Path) -> bool {
    let slug = reviews::slug(path);
    namings.iter().any(|naming| naming.placeholder == slug)
}

/// `title` for `placeholder` on the heading of `text`, labels kept.
fn retitled(text: &str, placeholder: &str, title: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let heading = markdown::title_line(&lines);
    text.split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            if Some(index) == heading {
                line.replacen(placeholder, title, 1)
            } else {
                line.to_string()
            }
        })
        .collect()
}

impl IboardApp {
    /// `unnamed-ticket-xxxx`, no issue on the board having it.
    pub(super) fn unnamed_title(&self) -> String {
        loop {
            let tag = RandomState::new().hash_one(()) as u16;
            let title = format!("unnamed-ticket-{tag:04x}");
            let taken = self
                .board
                .columns
                .iter()
                .flat_map(|column| &column.issues)
                .any(|issue| reviews::slug(&issue.path) == title);
            if !taken {
                return title;
            }
        }
    }

    /// Asks `engine` for the title of the issue just written as `placeholder`.
    pub(super) fn start_naming(&mut self, engine: Engine, placeholder: String, description: &str) {
        self.session.namings.push(Naming {
            placeholder,
            answer: titles::start(engine, &self.prefs.agent, description),
            deadline: Instant::now() + titles::TIMEOUT,
            title: None,
        });
    }

    /// Lands the titles that came in. A failure or [`titles::TIMEOUT`] leaves
    /// the placeholder. Renaming changes a card's id, so it waits for the
    /// pointer to settle, as [`Self::flush_pending_front`] does.
    pub(super) fn poll_namings(&mut self, ctx: &egui::Context) -> Option<Duration> {
        let pressed = ctx.input(|i| i.pointer.any_down());
        let mut namings = std::mem::take(&mut self.session.namings);
        namings.retain_mut(|naming| {
            if naming.title.is_none() {
                match naming.answer.answer() {
                    Some(Ok(title)) => naming.title = Some(title),
                    Some(Err(_)) => return false,
                    None => return Instant::now() < naming.deadline,
                }
            }
            if pressed || self.editor_holds(&naming.placeholder) {
                return true;
            }
            if let Some(title) = &naming.title {
                self.land_title(&naming.placeholder, title);
            }
            false
        });
        self.session.namings = namings;
        let waiting = self.session.namings.iter().any(|n| n.title.is_none());
        (!self.session.namings.is_empty()).then_some(if waiting {
            job::POLL_EVERY
        } else {
            HELD_POLL_EVERY
        })
    }

    /// The Editor is open on the issue, or holds edits of it: nothing moves
    /// under it, and once it lets go the heading decides.
    fn editor_holds(&self, placeholder: &str) -> bool {
        self.session.editor.as_ref().is_some_and(|editor| {
            reviews::slug(&editor.path) == placeholder
                && (editor.is_dirty() || self.session.editing() == Some(editor.path.as_path()))
        })
    }

    /// Renames the issue still called `placeholder` and sets its heading; one
    /// renamed by hand, retitled or gone keeps what it has.
    fn land_title(&mut self, placeholder: &str, title: &str) {
        self.refresh();
        let Some(issue) = self
            .board
            .columns
            .iter()
            .flat_map(|column| &column.issues)
            .find(|issue| issue.title == placeholder && reviews::slug(&issue.path) == placeholder)
        else {
            return;
        };
        let old = issue.path.clone();
        let text = retitled(&issue.markdown, placeholder, title);
        let Some(dir) = old.parent() else {
            return;
        };
        let old_name = file_name(&old);
        let number = old_name
            .strip_suffix(&format!("{placeholder}.md"))
            .unwrap_or_default();
        let (name, _) = issue::new_file_name(dir, title);
        let new = dir.join(unique_name(dir, &format!("{number}{name}")));
        let root = self.board.root_of(&old).to_path_buf();
        let landed = assets::relink(&root, &text, placeholder, &reviews::slug(&new), Carry::Move)
            .and_then(|text| atomic_write(&old, text))
            .and_then(|()| std::fs::rename(&old, &new));
        match landed {
            Ok(()) => {
                if self.session.editor.as_ref().is_some_and(|e| e.path == old) {
                    self.session.editor = None;
                }
                self.apply_renames(&[(old, new)]);
            }
            Err(err) => {
                self.error = Some(format!("Couldn't name {}: {err}", self.relative(&old)));
            }
        }
        self.refresh();
    }
}

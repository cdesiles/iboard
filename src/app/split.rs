//! Splitting an issue its review found too big: the agent's tickets, their
//! preview in the review panel, writing them in the original's place, and
//! taking it all back.

use super::*;

/// One proposed ticket, as the panel lets the user trim it.
#[derive(Clone)]
pub(super) struct SplitDraft {
    pub(super) title: String,
    pub(super) body: String,
    pub(super) include: bool,
}

/// A split `⌘Z` can still take back.
pub(super) struct SplitDone {
    /// Where the original was, and what it said before "Split into".
    pub(super) original: PathBuf,
    content: String,
    archived: PathBuf,
    pub(super) children: Vec<PathBuf>,
    pub(super) at: Instant,
}

impl IboardApp {
    /// Asks the agent to cut the reviewed issue, giving it the Size finding.
    pub(super) fn start_split(&mut self) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let ReviewStatus::Done {
            raw,
            report: Some(report),
        } = &review.status
        else {
            return;
        };
        let reason = report
            .findings
            .iter()
            .find(|finding| finding.axis == "Size")
            .map(|finding| finding.body.clone())
            .unwrap_or_default();
        let (path, mode) = (review.path.clone(), review.mode);
        let (raw, report) = (raw.clone(), Some(report.clone()));
        let Some(content) = self.board.find(&path).map(|issue| issue.markdown.clone()) else {
            return;
        };
        let notes = self.reviewer_notes(&path);
        let prompt = prompts::split_prompt(&content, &reason, &notes);
        let (answer, progress) = agent::start_review(&self.agent, &prompt, &path, mode);
        if let Some(review) = self.review.as_mut() {
            review.status = ReviewStatus::Splitting {
                answer,
                progress,
                raw,
                report,
            };
        }
    }

    /// Adopts the agent's tickets once they are in; called every frame.
    pub(super) fn poll_split(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        let ReviewStatus::Splitting { answer, .. } = &review.status else {
            return;
        };
        let Some(result) = answer.answer() else {
            return;
        };
        let placeholder = ReviewStatus::Failed {
            error: String::new(),
        };
        let ReviewStatus::Splitting { raw, report, .. } =
            std::mem::replace(&mut review.status, placeholder)
        else {
            return;
        };
        let drafts = result.and_then(|answer| {
            agent::parse_split(&answer)
                .ok_or_else(|| "The agent's answer had no tickets to create.".to_string())
        });
        review.status = match drafts {
            Ok(drafts) => ReviewStatus::SplitReady {
                drafts: drafts
                    .into_iter()
                    .map(|draft| SplitDraft {
                        title: draft.title,
                        body: draft.body,
                        include: true,
                    })
                    .collect(),
                raw,
                report,
            },
            Err(error) => ReviewStatus::Failed { error },
        };
    }

    /// "Back to review": the findings the split started from.
    pub(super) fn back_to_review(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        if let ReviewStatus::SplitReady { raw, report, .. }
        | ReviewStatus::Splitting { raw, report, .. } = &mut review.status
        {
            review.status = ReviewStatus::Done {
                raw: std::mem::take(raw),
                report: report.take(),
            };
        }
    }

    /// "Create N tickets": writes the kept drafts in place of the issue.
    pub(super) fn create_split(&mut self) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let ReviewStatus::SplitReady { drafts, .. } = &review.status else {
            return;
        };
        let drafts: Vec<SplitDraft> = drafts
            .iter()
            .filter(|draft| draft.include && !draft.title.trim().is_empty())
            .cloned()
            .collect();
        if drafts.is_empty() {
            return;
        }
        let original = review.path.clone();
        self.review = None;
        self.split_issue(&original, &drafts);
    }

    /// Writes one issue per draft beside `original`, each saying where it
    /// came from, puts them in its place in an ordered column, and archives
    /// it with a line naming them.
    pub(super) fn split_issue(&mut self, original: &Path, drafts: &[SplitDraft]) {
        let Some(dir) = original.parent().map(Path::to_path_buf) else {
            return;
        };
        let content = match std::fs::read_to_string(original) {
            Ok(content) => content,
            Err(err) => {
                self.error = Some(format!("Couldn't read {}: {err}", self.relative(original)));
                return;
            }
        };
        let from = file_name(original);
        let mut children = Vec::new();
        for draft in drafts {
            let title = draft.title.trim();
            let (name, _) = crate::issue::new_file_name(&dir, title);
            let path = dir.join(name);
            let text = format!(
                "# {title}\n\n{}\n\nSplit from `{from}`.\n",
                draft.body.trim()
            );
            if let Err(err) = atomic_write(&path, text) {
                self.error = Some(format!("Couldn't create {}: {err}", self.relative(&path)));
                for child in &children {
                    let _ = std::fs::remove_file(child);
                }
                return;
            }
            children.push(path);
        }
        self.refresh();
        let order: Vec<PathBuf> = self
            .board
            .columns
            .iter()
            .find(|column| column.dir == dir)
            .map(|column| {
                column
                    .issues
                    .iter()
                    .flat_map(|issue| match &issue.path {
                        path if path == original => children.clone(),
                        path if children.contains(path) => Vec::new(),
                        path => vec![path.clone()],
                    })
                    .collect()
            })
            .unwrap_or_default();
        let Some(archived) = self.move_to_archive(original) else {
            return;
        };
        if discovery::is_orderable_column(&dir) {
            match discovery::renumber(&dir, &order) {
                Ok(renames) => {
                    for child in &mut children {
                        if let Some((_, new)) = renames.iter().find(|(old, _)| old == child) {
                            *child = new.clone();
                        }
                    }
                    self.apply_renames(&renames);
                }
                Err(err) => self.error = Some(err),
            }
        }
        let names: Vec<String> = children
            .iter()
            .map(|child| format!("`{}`", file_name(child)))
            .collect();
        let marked = format!(
            "{}\n\nSplit into {}.\n",
            content.trim_end(),
            names.join(", ")
        );
        if let Err(err) = atomic_write(&archived, marked) {
            self.error = Some(format!(
                "Couldn't update {}: {err}",
                self.relative(&archived)
            ));
        }
        self.refresh();
        self.selected = children.first().cloned();
        self.scroll_to_selected = true;
        self.undo = Some(Undo::Split(SplitDone {
            original: original.to_path_buf(),
            content,
            archived,
            children,
            at: Instant::now(),
        }));
    }

    /// Trashes the tickets a split wrote and brings the original back.
    pub(super) fn undo_split(&mut self, split: SplitDone) {
        if split.original.exists() {
            self.error = Some(format!(
                "Couldn't restore {}: a file with that name exists now.",
                self.relative(&split.original)
            ));
            return;
        }
        for child in &split.children {
            if let Err(err) = self.trash.delete(child) {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(child)));
            }
        }
        let restored = std::fs::rename(&split.archived, &split.original)
            .and_then(|()| atomic_write(&split.original, &split.content));
        match restored {
            Ok(()) => {
                self.notice = Some((
                    format!("Restored {}", self.relative(&split.original)),
                    Instant::now(),
                ));
                self.refresh();
                self.selected = Some(split.original);
                self.scroll_to_selected = true;
            }
            Err(err) => {
                self.error = Some(format!(
                    "Couldn't restore {}: {err}",
                    self.relative(&split.original)
                ));
            }
        }
    }
}

/// The proposed tickets in the review panel: each can be left out or renamed.
pub(super) fn drafts_ui(ui: &mut egui::Ui, drafts: &mut [SplitDraft]) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new(format!(
            "The agent proposes {} tickets, in order:",
            drafts.len()
        ))
        .color(p.ink_muted),
    );
    ui.add_space(theme::SPACE_3);
    for (index, draft) in drafts.iter_mut().enumerate() {
        ui.push_id(("split-draft", index), |ui| {
            ui.horizontal(|ui| {
                widgets::checkbox(ui, &mut draft.include, "");
                ui.add(
                    egui::TextEdit::singleline(&mut draft.title)
                        .font(theme::ui_strong())
                        .desired_width(f32::INFINITY),
                );
            });
            let preview: Vec<&str> = draft
                .body
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(2)
                .collect();
            ui.add(
                egui::Label::new(RichText::new(preview.join(" ")).color(p.ink_muted)).truncate(),
            );
            ui.add_space(theme::SPACE_3);
        });
    }
}

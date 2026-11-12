//! The status bar, and the strip naming what failed.

use chrono::{DateTime, Utc};

use super::compose::DISCARD_UNDO;
use super::dock::IssueDrag;
use super::{
    Align, ButtonKind, Frame, IboardApp, Issue, Layout, NOTICE_FOR, Path, RichText, Screen, Undo,
    display_path, file_name, relative_time, sheet, theme, widgets,
};
use crate::agent::provider_name;
use crate::domains::agent::artifacts::usage_log::{self, Meter};

/// Between a status bar item and the `·` that parts it from the next.
const STATUS_GAP: f32 = 6.0;

impl IboardApp {
    /// The 24pt bottom strip: where you are on the left and on the right the
    /// Editor's save state, the usage trigger and, while ⌘ is held, what you
    /// can press.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = STATUS_GAP;
            let screen = self.session.screen.clone();
            let hints = match &screen {
                Screen::Board => self.board_status(ui),
                Screen::Detail(path, _) => self.detail_status(ui, path),
                Screen::Diff => self.diff_status(ui),
            };
            let editing = match &screen {
                Screen::Detail(path, _) if self.session.editing().is_some() => Some(path),
                _ => None,
            };
            let keys = widgets::key_hints_shown(ui.ctx());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                if let Some(path) = editing {
                    self.save_pill(ui, path);
                }
                self.meter_rings(ui);
                self.update_item(ui);
                if keys {
                    widgets::sliding_hints(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                        for (key, action) in hints.into_iter().rev() {
                            ui.horizontal(|ui| widgets::hint(ui, key, action));
                        }
                    });
                }
            });
        });
    }

    /// What the board's projects spent today, with what its running sessions
    /// cost so far, the week beside it and the last finished session. `None`
    /// off the board, where the trigger carries no spend.
    fn spend_today(&self, now: DateTime<Utc>) -> Option<BoardSpend> {
        if !matches!(self.session.screen, Screen::Board) {
            return None;
        }
        let spend = usage_log::Spend::of(&self.usage_entries, &self.board_projects(), now);
        let live = &self.session.live_spend;
        let mut providers = spend.providers;
        for (provider, cost) in &live.providers {
            *providers.entry(provider.clone()).or_default() += cost;
        }
        let mut providers: Vec<(String, f64)> = providers
            .into_iter()
            .map(|(provider, cost)| (provider_name(&provider), cost))
            .collect();
        providers.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Some(BoardSpend {
            today: spend.today + live.cost,
            week: spend.week,
            last: spend.last,
            running: live.running(),
            providers,
        })
    }

    /// The one usage trigger, and the popover it opens: the board's spend then
    /// the share of the window closest to its cap. It is absent when there is
    /// neither a spend nor a plan window.
    fn meter_rings(&mut self, ui: &mut egui::Ui) {
        let now = Utc::now();
        let spent = self.spend_today(now);
        let meters = usage_log::subscription_meters_with(
            &self.prefs.usage,
            &self.usage_entries,
            now,
            &self.session.quotas,
        );
        let id = super::usage_popover::popup_id();
        if spent.is_none() && meters.is_empty() {
            egui::Popup::close_id(ui.ctx(), id);
            return;
        }
        let closest = meters
            .iter()
            .filter_map(|meter| meter.share().map(|share| (meter, share)))
            .max_by(|(_, a), (_, b)| a.total_cmp(b));
        let share = closest.map(|(_, share)| share);
        let warn = closest.is_some_and(|(meter, _)| meter.over_pace());
        let figure = (!meters.is_empty()).then(|| match share {
            Some(share) => format!("{:.0}%", share * 100.0),
            None => "Usage".to_string(),
        });
        let detail = pill_detail(&meters, now);
        let pill = spent
            .as_ref()
            .map(|spend| (spend.figure(), spend.lit(), spend.line()));
        let pill = pill
            .as_ref()
            .map(|(figure, lit, line)| (figure.as_str(), *lit, line.as_str()));
        let open = egui::Popup::is_id_open(ui.ctx(), id);
        let response = widgets::usage_pill(ui, pill, share, warn, figure.as_deref(), &detail, open);
        if response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), id);
        }
        self.usage_popover(ui, response.rect, &meters, spent.as_ref(), now);
    }

    /// The board's status: where it reads from or what just happened, its
    /// warnings. Returns its key hints.
    fn board_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let hints = self.board_message(ui);
        if !self.board.warnings.is_empty() {
            let n = self.board.warnings.len();
            let noun = if n == 1 { "warning" } else { "warnings" };
            status_sep(ui);
            ui.label(status_meta(ui, format!("{n} {noun}")))
                .on_hover_text(self.board.warnings.join("\n"));
        }
        hints
    }

    /// While filtering: "4 of 9 issues match “bug”".
    fn filter_matches(&self) -> Option<String> {
        let filter = &self.filter_bar.filter;
        if filter.is_empty() {
            return None;
        }
        let total = self.board.issue_count();
        let issues = self.board.columns.iter().flat_map(|c| &c.issues);
        let shown = issues.filter(|issue| filter.matches(issue)).count();
        let noun = if total == 1 { "issue" } else { "issues" };
        let verb = if shown == 1 { "matches" } else { "match" };
        Some(format!(
            "{shown} of {total} {noun} {verb} “{}”",
            filter.describe()
        ))
    }

    /// The issues folder, then what the filter matches.
    fn folder_line(&self, ui: &mut egui::Ui) {
        match self.shown_group() {
            Some(group) => {
                ui.label(status_meta(ui, group.count()));
            }
            None => copyable_path(ui, display_path(&self.board.root), &self.board.root),
        }
        if let Some(matches) = self.filter_matches() {
            status_sep(ui);
            ui.label(status_meta(ui, matches));
        }
    }

    /// The board's first status line, by priority: a drag, an undo, a
    /// notice, else the folder. The selection and the composer set the keys.
    fn board_message(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        let notice = self
            .session
            .notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
            .map(|(text, at)| (text.clone(), *at));
        let mut idle = vec![("⌘N", "new"), ("⌘F", "filter")];
        if !self.prefs.auto {
            idle.push(("⌘R", "refresh"));
        }
        if let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
            ui.label(status_meta(ui, self.relative(&drag.path)));
            return vec![("esc", "cancel drag")];
        }
        if let Some((_, at)) = self
            .session
            .discarded
            .as_ref()
            .filter(|(_, at)| at.elapsed() < DISCARD_UNDO)
        {
            ui.label(status_meta(ui, "Discarded the new issue · ⌘Z".to_owned()));
            ui.ctx()
                .request_repaint_after(DISCARD_UNDO.saturating_sub(at.elapsed()));
            return vec![("⌘Z", "undo"), ("⌘N", "new")];
        }
        if let Some(undo) = self.session.undo.as_ref() {
            let text = match undo {
                Undo::Deleted(deleted) => format!("{} · ⌘Z", self.trashed_notice(deleted)),
                Undo::Split(split) => format!(
                    "Split {} into {} tickets",
                    file_name(&split.original),
                    split.children.len()
                ),
            };
            ui.label(status_meta(ui, text));
            return vec![("⌘Z", "undo"), ("⌘,", "settings")];
        }
        if let Some((text, at)) = notice {
            ui.label(status_meta(ui, text));
            ui.ctx()
                .request_repaint_after(NOTICE_FOR.saturating_sub(at.elapsed()));
            return idle;
        }
        self.folder_line(ui);
        if self.composer_open() && self.board.todo_column().is_some() {
            return vec![("↩", "add"), ("⌘↩", "add & edit"), ("esc", "close")];
        }
        let Some(selected) = self.selected_issue() else {
            return idle;
        };
        let done = self.is_done_issue(&selected);
        if self.can_edit(&selected) {
            vec![("↩", "open"), ("E", "edit"), ("⌘↩", "next column")]
        } else if done {
            vec![("↩", "open"), ("A", "archive")]
        } else {
            vec![("↩", "open"), ("Del", "delete")]
        }
    }

    /// The Diff screen's status: its commits and their size.
    fn diff_status(&mut self, ui: &mut egui::Ui) -> Vec<Hint> {
        if let Some(view) = &self.session.diff {
            ui.label(status_meta(ui, view.summary()));
        }
        vec![("⌘F", "find"), ("n p", "file"), ("esc", "back")]
    }

    /// The open issue's status: its file, then in the Reader when it was
    /// modified and how far it is. Returns its key hints.
    fn detail_status(&mut self, ui: &mut egui::Ui, path: &Path) -> Vec<Hint> {
        copyable_path(ui, self.relative(path), path);
        if self.session.editing().is_some() {
            return if self.session.leave_prompt.is_some() {
                vec![("esc", "keep editing")]
            } else {
                vec![("⌘S", "save"), ("esc", "leave")]
            };
        }
        if let Some(issue) = self.board.find(path) {
            reader_facts(ui, issue);
        }
        let todo = self
            .board
            .column_of(path)
            .is_some_and(|column| column.accepts_new_issues());
        if todo && self.prefs.agent.configured() {
            vec![
                ("E", "edit"),
                ("M", "move"),
                ("esc", "board"),
                ("⌘⇧R", "review"),
            ]
        } else if self.can_edit(path) {
            vec![
                ("E", "edit"),
                ("M", "move"),
                ("esc", "board"),
                ("Del", "delete"),
            ]
        } else if self.is_done_issue(path) {
            vec![("M", "move"), ("esc", "board")]
        } else {
            vec![("M", "move"), ("Del", "delete"), ("esc", "board")]
        }
    }

    /// A dismissible `danger` line naming what failed, across the top. A
    /// project with no issues directory is offered one.
    pub(super) fn error_strip(&mut self, ui: &mut egui::Ui, error: &str) {
        let p = theme::palette(ui);
        let uninitialised = self
            .uninitialised
            .as_ref()
            .filter(|(_, raised)| raised == error)
            .map(|(project, _)| project.clone());
        Frame::new().inner_margin(theme::BAND_PAD).show(ui, |ui| {
            // Dismiss first, so a long error truncates before it rather
            // than pushing it out of the window.
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None).clicked() {
                        self.error = None;
                        self.uninitialised = None;
                        self.scan_error = None;
                    }
                    if let Some(project) = uninitialised {
                        let root = crate::domains::onboarding::folders::root_name(&project);
                        let create = format!("Create {}/", root.trim_end_matches('/'));
                        if widgets::button(ui, ButtonKind::Primary, None, &create, None).clicked() {
                            self.initialise(project);
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let text = RichText::new(error).color(p.danger);
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(error);
                    });
                });
            });
        });
    }
}

/// A key hint of the status bar: the key, and what it does.
type Hint = (&'static str, &'static str);

/// The board's spend as the trigger and the popover show it: today's dollars
/// with the running sessions' cost, this week's, the last session's, and
/// whether a run is burning, and which providers today's dollars went to.
pub(super) struct BoardSpend {
    pub(super) today: f64,
    pub(super) week: f64,
    pub(super) last: Option<f64>,
    pub(super) running: bool,
    /// Provider names and today's dollars, the largest first.
    pub(super) providers: Vec<(String, f64)>,
}

impl BoardSpend {
    /// `$28.61`: today's dollars, as the bar and the popover write them.
    pub(super) fn figure(&self) -> String {
        format!("${:.2}", self.today)
    }

    /// Whether anything was spent today, which lights the figure.
    fn lit(&self) -> bool {
        self.today > 0.0
    }

    /// What VoiceOver reads on the trigger: what today cost, the week and the
    /// last session, which the bar no longer names on hover.
    fn line(&self) -> String {
        let name = if self.running { "Building" } else { "Today" };
        let mut line = format!("{name} ${:.2}, this week ${:.2}", self.today, self.week);
        if let Some(last) = self.last {
            line.push_str(&format!(", last session ${last:.2}"));
        }
        line
    }
}

/// What the Reader's status line says of an open issue after its path: "modified
/// 15d ago", then its checklist as a 48pt meter and "1/3 done", or with no
/// checkboxes its word count.
fn reader_facts(ui: &mut egui::Ui, issue: &Issue) {
    if let Some(modified) = issue.modified {
        status_sep(ui);
        let ago = format!("modified {}", relative_time(modified));
        ui.label(status_meta(ui, ago));
    }
    status_sep(ui);
    let (text, _) = sheet::hide_title_labels(&issue.markdown);
    let (ticked, total) = sheet::checklist(&text);
    if total > 0 {
        widgets::meter(ui, ticked, total);
        ui.label(status_meta(ui, format!("{ticked}/{total} done")));
    } else {
        let words = sheet::word_count(&text);
        let unit = if words == 1 { "word" } else { "words" };
        ui.label(status_meta(ui, format!("{words} {unit}")));
    }
}

/// What the pointer and VoiceOver read on the pill: one block per window,
/// its name, share or tokens, its pace, and its reset.
fn pill_detail(meters: &[Meter], now: DateTime<Utc>) -> String {
    meters
        .iter()
        .map(|meter| {
            let name = meter_name(meter, meters);
            match meter.share() {
                Some(_) => format!(
                    "{name} · {}{} · {}",
                    share_text(meter),
                    if meter.over_pace() {
                        " · over pace"
                    } else {
                        ""
                    },
                    reset_text(meter, now)
                ),
                None => format!(
                    "{name} · No cap set · {} tokens this {}",
                    short_tokens(meter.consumed.tokens),
                    meter.window.name().to_lowercase()
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A meter's name on the pill: the provider, and the window when it has more
/// than one.
fn meter_name(meter: &Meter, meters: &[Meter]) -> String {
    if meters
        .iter()
        .filter(|other| other.provider == meter.provider)
        .count()
        > 1
    {
        format!("{} · {}", meter.name, meter.window.name())
    } else {
        meter.name.clone()
    }
}

/// `3_100_000` as `3.1M`, the way a tooltip holds tokens.
pub(super) fn short_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    }
}

/// "42% of the week used", or "unknown" when no cap is known.
fn share_text(meter: &Meter) -> String {
    match meter.share() {
        Some(share) => format!(
            "{:.0}% of the {} used",
            share * 100.0,
            meter.window.name().to_lowercase()
        ),
        None => "unknown".to_string(),
    }
}

/// When the plan's window resets, from `now`.
pub(super) fn reset_text(meter: &Meter, now: DateTime<Utc>) -> String {
    reset_of(meter.reset, now)
}

/// [`reset_text`] from a window's own reset instant.
pub(super) fn reset_of(reset: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (reset - now).num_seconds().max(0);
    let left = match secs {
        0..=59 => "now".to_string(),
        60..=3_599 => format!("in {}m", secs / 60),
        3_600..=86_399 => format!("in {}h", secs / 3_600),
        _ => format!("in {}d", secs / 86_400),
    };
    match left.as_str() {
        "now" => "resets now".to_string(),
        left => format!("resets {left}"),
    }
}

/// A path in the status bar, `shown` as it reads there: a click copies its
/// full path, and its tooltip says so until the pointer leaves.
fn copyable_path(ui: &mut egui::Ui, shown: String, path: &Path) {
    let id = egui::Id::unique(("copied-path", path));
    let response = ui
        .add(egui::Label::new(status_meta(ui, shown)).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        ui.ctx().copy_text(path.display().to_string());
        ui.data_mut(|d| d.insert_temp(id, true));
    } else if !response.hovered() {
        ui.data_mut(|d| d.remove::<bool>(id));
    }
    if ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        response.show_tooltip_text("Copied the full path");
    } else {
        response.on_hover_text("Click to copy the full path");
    }
}

/// The `·` between two status bar items, in `ink-faint`.
fn status_sep(ui: &mut egui::Ui) {
    let faint = theme::palette(ui).ink_faint;
    ui.label(RichText::new("·").font(theme::meta()).color(faint));
}

/// Status bar text in `meta`, `ink-muted`.
fn status_meta(ui: &egui::Ui, text: String) -> RichText {
    RichText::new(text)
        .font(theme::meta())
        .color(theme::palette(ui).ink_muted)
}

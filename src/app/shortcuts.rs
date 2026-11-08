//! The keyboard shortcuts sheet: holding ⌘ alone lists every key of the
//! screen on one card, over it, so a newcomer can learn them at once. See
//! `design/components/Shortcuts`.

use egui::{Align, Layout, Pos2, RichText, Sense, Vec2};

use super::{DetailMode, IboardApp, Screen, theme, widgets};

/// The card's width at most, and how many rows a section's heading adds.
const SHEET_W: f32 = 600.0;
const HEADING_ROWS: f32 = 1.5;
/// A row's height, key cap and what it does.
const ROW_H: f32 = 20.0;

/// One group of the sheet: its heading and the keys under it.
struct Section {
    title: &'static str,
    rows: Vec<(&'static str, &'static str)>,
}

impl Section {
    fn new(title: &'static str, rows: Vec<(&'static str, &'static str)>) -> Self {
        Self { title, rows }
    }

    fn height(&self) -> f32 {
        (self.rows.len() as f32 + HEADING_ROWS) * ROW_H
    }
}

impl IboardApp {
    /// The sheet while ⌘ alone is held, over the screen it describes. A
    /// modal, a popup or a menu owns the screen and already shows its keys,
    /// so none of them gets one.
    pub(super) fn shortcuts_sheet(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        let reveal = widgets::key_hints(ctx);
        if reveal <= 0.0 || self.modal_up() || egui::Popup::is_any_open(ctx) {
            return;
        }
        let width = SHEET_W.min(ctx.content_rect().width() - 2.0 * theme::SPACE_6);
        egui::Area::new(egui::Id::unique("shortcuts-sheet"))
            .anchor(
                egui::Align2::CENTER_CENTER,
                Vec2::new(0.0, (1.0 - reveal) * theme::SPACE_2),
            )
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.multiply_opacity(reveal);
                widgets::modal_frame(ui)
                    .inner_margin(theme::SPACE_6)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        sheet(ui, self.sections());
                    });
            });
    }

    fn sections(&self) -> Vec<Section> {
        if self.session.stats.is_some() {
            return vec![
                Section::new("Stats", vec![("esc", "Back")]),
                Section::new(
                    "Agenda",
                    vec![
                        ("⌘R", "Refresh"),
                        ("← →", "Week"),
                        ("↑ ↓", "Day"),
                        ("1 – 4", "Range"),
                    ],
                ),
            ];
        }
        match &self.session.screen {
            Screen::Board => self.board_sections(),
            Screen::Detail(path, mode) => self.issue_sections(path, *mode),
            Screen::Diff => self.diff_sections(),
        }
        .into_iter()
        .chain(self.agent_section())
        .collect()
    }

    /// `⌘N`, Settings and going back, on the boards and pages that take them.
    fn anywhere(&self, back: &'static str) -> Section {
        let mut rows = vec![("⌘N", "New issue"), ("⌘,", "Settings")];
        if !matches!(self.session.screen, Screen::Board) {
            rows.push(("⌘[", "Back"));
        }
        rows.push(("esc", back));
        Section::new("Anywhere", rows)
    }

    fn board_sections(&self) -> Vec<Section> {
        let mut board = vec![("⌘F", "Filter"), ("⌘I", "Stats"), ("⌃⇥", "Project")];
        if !self.prefs.auto {
            board.push(("⌘R", "Refresh"));
        }
        board.push(("⌘Z", "Undo"));
        let cards = vec![
            ("↑ ↓ ← →", "Move"),
            ("↩", "Open"),
            ("E", "Edit"),
            ("L", "Labels"),
            ("⌘↩", "Next column"),
            ("A", "Archive done"),
            ("Del", "Delete"),
        ];
        vec![
            self.anywhere("Clear"),
            Section::new("Board", board),
            Section::new("Cards", cards),
        ]
    }

    fn issue_sections(&self, path: &super::Path, mode: DetailMode) -> Vec<Section> {
        let (title, rows) = match mode {
            DetailMode::Edit => ("Editing", vec![("⌘S", "Save"), ("⌘Z", "Undo typing")]),
            DetailMode::Preview => (
                "Reading",
                vec![
                    ("E", "Edit"),
                    ("L", "Labels"),
                    ("Del", "Delete"),
                    ("↑ ↓", "Scroll"),
                    ("Space", "Page"),
                    ("⇧Space", "Page up"),
                ],
            ),
        };
        let back = match mode {
            DetailMode::Edit => "Leave",
            DetailMode::Preview => "Board",
        };
        let mut sections = vec![self.anywhere(back), Section::new(title, rows)];
        if !self.prefs.auto && !self.is_todo_issue(path) {
            sections.push(Section::new("Refresh", vec![("⌘R", "Reload from disk")]));
        }
        sections
    }

    fn diff_sections(&self) -> Vec<Section> {
        let diff = vec![
            ("⌘F", "Find"),
            ("n p", "File"),
            ("[ ]", "Hunk"),
            ("↑ ↓", "Scroll"),
            ("↩", "Next match"),
            ("⇧↩", "Previous match"),
        ];
        vec![self.anywhere("Back"), Section::new("Diff", diff)]
    }

    /// `⌘R`, `⌘⇧R` and `⌘K` where the issue takes them.
    fn agent_section(&self) -> Option<Section> {
        let path = match &self.session.screen {
            Screen::Board => self.selected_issue(),
            Screen::Detail(path, _) => Some(path.clone()),
            Screen::Diff => None,
        }?;
        if self.is_done_issue(&path) || !self.prefs.agent.configured() {
            return None;
        }
        let mut rows = Vec::new();
        let open = matches!(self.session.screen, Screen::Detail(_, _));
        if open && self.is_todo_issue(&path) {
            rows.push(("⌘R", "Refine"));
        }
        if self.is_todo_issue(&path) {
            rows.push(("⌘⇧R", "Review"));
        }
        rows.push(("⌘K", "Ask"));
        (!rows.is_empty()).then(|| Section::new("Agent", rows))
    }
}

/// The head, the two balanced columns of sections, and their rows.
fn sheet(ui: &mut egui::Ui, sections: Vec<Section>) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Keyboard shortcuts")
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("Release ⌘ to hide")
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
        });
    });
    ui.add_space(theme::SPACE_4);
    let (left, right) = columns(sections);
    ui.spacing_mut().item_spacing.x = theme::SPACE_6;
    ui.columns(2, |columns| {
        for (column, sections) in columns.iter_mut().zip([left, right]) {
            for section in sections {
                section_ui(column, &section);
            }
        }
    });
}

/// Splits the sections between two columns, the taller one next.
fn columns(sections: Vec<Section>) -> (Vec<Section>, Vec<Section>) {
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let (mut left_h, mut right_h) = (0.0, 0.0);
    for section in sections {
        if left_h <= right_h {
            left_h += section.height();
            left.push(section);
        } else {
            right_h += section.height();
            right.push(section);
        }
    }
    (left, right)
}

fn section_ui(ui: &mut egui::Ui, section: &Section) {
    let p = theme::palette(ui);
    ui.label(
        RichText::new(section.title.to_uppercase())
            .font(theme::ui_label())
            .extra_letter_spacing(theme::LABEL_TRACKING)
            .color(p.ink_muted),
    );
    ui.add_space(theme::SPACE_1);
    ui.spacing_mut().item_spacing.y = theme::SPACE_1;
    for (key, action) in &section.rows {
        row(ui, key, action);
    }
    ui.add_space(theme::SPACE_4);
}

/// A key cap and what it does, read together by VoiceOver.
fn row(ui: &mut egui::Ui, key: &str, action: &str) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let width = widgets::text_width(ui, key, theme::meta_mono()) + 8.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width.max(18.0), 16.0), Sense::hover());
        widgets::paint_kbd(ui, key, Pos2::new(rect.min.x, rect.center().y));
        let response = ui.label(RichText::new(action).font(theme::meta()).color(p.ink_muted));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{key} {action}"))
        });
    });
}

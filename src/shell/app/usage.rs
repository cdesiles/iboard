//! The Usage screen: Tools › Usage…, the full view of what the providers
//! cost and consume (see `design/components/UsageScreen`). This part holds its
//! header bar, its rail and its blank states; the main pane's blocks come next.

use chrono::{Local, Utc};

use super::{
    Align, CornerRadius, Frame, IboardApp, Icon, Instant, Key, Layout, RichText, Sense, Stroke,
    Vec2, theme, widgets,
};
use crate::agent::{Provider, provider_key};
use crate::domains::agent::artifacts::usage_log::{self, Entry};
use crate::usage::{ListedProvider, ProviderUsage, Range, UsageView};

/// The rail's width, and a row's height.
const RAIL_W: f32 = 208.0;
const ROW_H: f32 = 28.0;
/// A provider row's swatch.
const SWATCH: f32 = 8.0;
/// Where a rail row's icon slot starts, and how wide it is.
const ROW_LEAD: f32 = theme::SPACE_4;
const ROW_SLOT: f32 = theme::ICON_SIZE;
/// The widest the blank states' sentence reads.
const BLANK_W: f32 = 460.0;

/// The Usage screen's state: whether it is up, its range and the provider it
/// shows, and the read model redrawn for the board's scope.
pub(super) struct UsageScreen {
    pub(super) open: bool,
    pub(super) range: Range,
    /// The provider selected in the rail; `None` is All providers.
    pub(super) provider: Option<String>,
    pub(super) view: UsageView,
}

impl Default for UsageScreen {
    fn default() -> Self {
        Self {
            open: false,
            range: Range::default(),
            provider: None,
            view: UsageView::read_with(&[], &[], Utc::now(), &[] as &[ListedProvider]),
        }
    }
}

impl UsageScreen {
    /// Whether the rail has more than one provider to choose between.
    pub(super) fn rail_shown(&self) -> bool {
        self.view.providers.len() >= 2
    }

    /// The provider pane drawn, or `None` for All providers.
    fn chosen(&self) -> Option<&ProviderUsage> {
        if self.rail_shown() {
            self.provider
                .as_deref()
                .and_then(|key| self.view.provider(key))
        } else {
            self.view.providers.first()
        }
    }
}

/// What the screen asked for this frame.
enum Ask {
    Provider(Option<String>),
    Range(Range),
    Settings,
}

impl IboardApp {
    /// Opens Usage on the board's scope, reading the log at once.
    pub(super) fn open_usage(&mut self) {
        if self.welcoming() {
            return;
        }
        self.project_menu = None;
        self.usage.open = true;
        self.usage_refresh();
    }

    /// Reads the log again now, then redraws the screen.
    fn usage_refresh(&mut self) {
        if let Some(path) = self.usage_log_file.clone() {
            self.usage_entries = usage_log::read_from(&path);
            self.usage_read = Some(Instant::now());
        }
        self.refresh_usage_view();
    }

    /// Rebuilds the read model for the board's scope. A board switch calls it,
    /// so the screen follows the group or project that opened.
    pub(super) fn refresh_usage_view(&mut self) {
        let projects = self.board_projects();
        let listed = listed(&self.usage_entries);
        self.usage.view = UsageView::read_with(&self.usage_entries, &projects, Utc::now(), &listed);
        if let Some(provider) = self.usage.provider.as_deref()
            && self.usage.view.provider(provider).is_none()
        {
            self.usage.provider = None;
        }
    }

    /// Usage owns the keyboard while up: `esc` goes back to the board, and the
    /// arrows move the rail when there is one.
    pub(super) fn usage_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.usage.open = false;
            return;
        }
        if !self.usage.rail_shown() {
            return;
        }
        let step = if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
            1
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
            -1
        } else {
            return;
        };
        self.step_provider(step);
    }

    fn step_provider(&mut self, step: isize) {
        let mut rows: Vec<Option<String>> = vec![None];
        rows.extend(
            self.usage
                .view
                .providers
                .iter()
                .map(|provider| Some(provider.provider.clone())),
        );
        let at = self
            .usage
            .provider
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| row.as_ref() == Some(selected)))
            .unwrap_or(0) as isize;
        let next = (at + step).clamp(0, rows.len() as isize - 1) as usize;
        self.usage.provider = rows[next].clone();
    }

    /// The header's trail while Usage is up: Board, a divider, then `Usage`.
    pub(super) fn usage_crumbs(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let back = widgets::button(
            ui,
            super::ButtonKind::Back,
            Some(Icon::Back),
            "Board",
            Some("esc"),
        )
        .on_hover_text("Back to the board  esc")
        .clicked();
        let (divider, _) = ui.allocate_exact_size(Vec2::new(1.0, theme::CONTROL_H), Sense::hover());
        ui.painter().line_segment(
            [
                egui::Pos2::new(divider.center().x, divider.center().y - 8.0),
                egui::Pos2::new(divider.center().x, divider.center().y + 8.0),
            ],
            Stroke::new(1.0, p.line),
        );
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        ui.add_space(widgets::CRUMB_PAD);
        ui.label(RichText::new("Usage").font(theme::ui_strong()).color(p.ink));
        if back {
            self.usage.open = false;
        }
    }

    /// The header's controls while Usage is up: Refresh, then when the log
    /// last read. A blank screen has neither.
    pub(super) fn usage_controls(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        if self.usage_blank() {
            return;
        }
        if widgets::button(
            ui,
            super::ButtonKind::Ghost,
            Some(Icon::Refresh),
            "Refresh",
            None,
        )
        .clicked()
        {
            self.usage_refresh();
        }
        ui.label(
            RichText::new(self.usage_read_line())
                .font(theme::meta())
                .color(p.ink_muted),
        );
    }

    /// Whether the screen has nothing to read: no tool, or a tool with no
    /// provider yet.
    fn usage_blank(&self) -> bool {
        !self.prefs.agent.configured() || self.usage.view.providers.is_empty()
    }

    /// `Read 14:02 · from the usage log`, from when the log last read.
    fn usage_read_line(&self) -> String {
        let at = match self.usage_read {
            Some(read) => Local::now() - read.elapsed(),
            None => Local::now(),
        };
        format!("Read {} · from the usage log", at.format("%H:%M"))
    }

    /// Usage in place of the board: its rail, then the provider's pane, or the
    /// blank state that says what to set up first.
    pub(super) fn usage_screen(&mut self, ui: &mut egui::Ui) {
        self.refresh_usage_view();
        ui.style_mut().interaction.selectable_labels = false;
        widgets::rule(ui);
        let mut ask = None;
        if !self.prefs.agent.configured() {
            if blank(
                ui,
                "Configure a tool first",
                "Usage reads the runs of the tool iboard drives. Choose OpenCode or Claude Code \
                 in Settings › AI build. Each provider it connects gets its own tab here, with \
                 its usage by day.",
                Some("Open Settings"),
            ) {
                ask = Some(Ask::Settings);
            }
        } else if self.usage.view.providers.is_empty() {
            let tool = self.prefs.agent.tool.name();
            blank(
                ui,
                &format!("{tool} has no provider connected yet"),
                &format!("Connect one in {tool}."),
                None,
            );
        } else {
            ask = self.usage_body(ui);
        }
        match ask {
            Some(Ask::Provider(provider)) => self.usage.provider = provider,
            Some(Ask::Range(range)) => self.usage.range = range,
            Some(Ask::Settings) => self.open_settings(crate::settings_ui::Tab::Build),
            None => {}
        }
    }

    fn usage_body(&mut self, ui: &mut egui::Ui) -> Option<Ask> {
        let p = theme::palette(ui);
        let scope = self.usage_scope();
        let mut ask = None;
        if self.usage.rail_shown() {
            let rail = egui::Panel::left("usage-rail")
                .exact_size(RAIL_W)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(p.bg))
                .show(ui, |ui| {
                    if let Some(picked) = rail_body(ui, &self.usage) {
                        ask = Some(Ask::Provider(picked));
                    }
                });
            let edge = rail.response.rect.right();
            ui.painter().line_segment(
                [
                    egui::Pos2::new(edge, rail.response.rect.top()),
                    egui::Pos2::new(edge, rail.response.rect.bottom()),
                ],
                Stroke::new(1.0, p.line),
            );
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                if let Some(picked) = pane(ui, &self.usage, &scope) {
                    ask = Some(picked);
                }
            });
        ask
    }

    /// What the head calls the board's scope: the group, or the project.
    fn usage_scope(&self) -> String {
        match self.shown_group() {
            Some(group) => format!("in the {} group", group.name),
            None => format!("in {}", self.board.name()),
        }
    }
}

/// The providers the log has seen, in the tool's order: the rail keeps a row
/// for one that ran in another project, and only the scope's runs are counted.
fn listed(entries: &[Entry]) -> Vec<ListedProvider> {
    Provider::ALL
        .iter()
        .copied()
        .filter(|provider| {
            entries.iter().any(|entry| {
                entry.provider.as_deref().map(provider_key).as_deref() == Some(provider.key())
            })
        })
        .map(ListedProvider::from_provider)
        .collect()
}

/// A blank state: a heading, its sentence, and the one filled button a screen
/// may hold. Returns whether the button was pressed.
fn blank(ui: &mut egui::Ui, title: &str, body: &str, action: Option<&str>) -> bool {
    let p = theme::palette(ui);
    let mut pressed = false;
    ui.vertical_centered(|ui| {
        ui.add_space(theme::SPACE_7 * 3.0);
        ui.set_max_width(BLANK_W);
        ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
        ui.add_space(theme::SPACE_3);
        ui.add(egui::Label::new(RichText::new(body).font(theme::meta()).color(p.ink_muted)).wrap());
        if let Some(label) = action {
            ui.add_space(theme::SPACE_5);
            if widgets::button(ui, super::ButtonKind::Primary, None, label, Some("⌘,")).clicked()
            {
                pressed = true;
            }
        }
    });
    pressed
}

/// The rail: All providers, a rule, then a row per provider. Returns the row
/// picked this frame: `Some(None)` for All providers.
fn rail_body(ui: &mut egui::Ui, screen: &UsageScreen) -> Option<Option<String>> {
    let p = theme::palette(ui);
    let mut picked = None;
    Frame::new()
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(RAIL_W - 2.0 * theme::SPACE_3);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if rail_row(
                ui,
                "All providers",
                Some(Icon::Bars),
                None,
                screen.provider.is_none(),
            ) {
                picked = Some(None);
            }
            ui.add_space(theme::SPACE_2);
            let (rule, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter()
                .rect_filled(rule, CornerRadius::ZERO, p.line_soft);
            ui.add_space(theme::SPACE_2);
            for provider in &screen.view.providers {
                let selected = screen.provider.as_deref() == Some(provider.provider.as_str());
                let swatch = p.label(provider.hue);
                if rail_row(ui, &provider.name, None, Some(swatch), selected) {
                    picked = Some(Some(provider.provider.clone()));
                }
            }
        });
    picked
}

/// One 28pt rail row: an icon or a swatch, then its name. A tab to VoiceOver.
fn rail_row(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<Icon>,
    swatch: Option<egui::Color32>,
    selected: bool,
) -> bool {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), selected, label)
    });
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            p.accent_soft
        } else if response.hovered() {
            p.surface_hover
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), fill);
        let slot = egui::Rect::from_center_size(
            egui::Pos2::new(rect.left() + ROW_LEAD + ROW_SLOT / 2.0, rect.center().y),
            Vec2::splat(ROW_SLOT),
        );
        if let Some(icon) = icon {
            theme::paint_icon(ui.painter(), icon, slot, p.ink_muted);
        }
        if let Some(color) = swatch {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(slot.center(), Vec2::splat(SWATCH)),
                CornerRadius::same(2),
                color,
            );
        }
        ui.painter().text(
            egui::Pos2::new(
                rect.left() + ROW_LEAD + ROW_SLOT + theme::SPACE_3,
                rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            label,
            theme::ui_body(),
            if selected { p.ink } else { p.ink_muted },
        );
    }
    response.clicked()
}

/// The main pane's head and, for now, its range and any empty-range word.
fn pane(ui: &mut egui::Ui, screen: &UsageScreen, scope: &str) -> Option<Ask> {
    let p = theme::palette(ui);
    let mut ask = None;
    Frame::new()
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_6 as i8,
            theme::SPACE_5 as i8,
        ))
        .show(ui, |ui| {
            let chosen = screen.chosen();
            let (title, billing) = match chosen {
                Some(provider) => (provider.name.clone(), Some(provider.billing.name())),
                None => ("All providers".to_string(), None),
            };
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                    ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
                    let meta = match billing {
                        Some(billing) => {
                            format!("{billing} · counted from the tool's sessions {scope}")
                        }
                        None => format!("counted from the tool's sessions {scope}"),
                    };
                    ui.label(RichText::new(meta).font(theme::meta()).color(p.ink_muted));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(range) = range_control(ui, screen.range) {
                        ask = Some(Ask::Range(range));
                    }
                });
            });
            ui.add_space(theme::SPACE_5);
            let empty = match chosen {
                Some(provider) => !provider.has_run_in(screen.range),
                None => screen.view.is_empty(screen.range),
            };
            if empty {
                ui.label(
                    RichText::new("No agent run in this range.")
                        .font(theme::meta())
                        .color(p.ink_faint),
                );
            }
        });
    ask
}

/// The `7d` `30d` `90d` segmented control; the active option is raised.
fn range_control(ui: &mut egui::Ui, current: Range) -> Option<Range> {
    let p = theme::palette(ui);
    let mut picked = None;
    Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for range in Range::ALL {
                    if segment(ui, range, range == current).clicked() {
                        picked = Some(range);
                    }
                }
            });
        });
    picked
}

/// One range option; VoiceOver reads it as a radio button in the range group.
fn segment(ui: &mut egui::Ui, range: Range, active: bool) -> egui::Response {
    let p = theme::palette(ui);
    let name = range.label();
    let text = ui.painter().layout_no_wrap(
        name.to_string(),
        theme::ui_button(),
        egui::Color32::PLACEHOLDER,
    );
    let size = Vec2::new(text.size().x + 2.0 * theme::SPACE_3, theme::CONTROL_H - 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), active, name)
    });
    let radius = CornerRadius::same(theme::RADIUS_SM - 1);
    if active {
        let shadow = egui::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: p.popover_shadow,
        };
        ui.painter().add(shadow.as_shape(rect, radius));
        ui.painter().rect_filled(rect, radius, p.surface);
    }
    let ink = if active || response.hovered() {
        p.ink
    } else {
        p.ink_muted
    };
    ui.painter()
        .galley(rect.center() - text.size() / 2.0, text, ink);
    response
}

#[cfg(test)]
mod tests {
    use super::{UsageScreen, listed};
    use crate::agent::{Billing, Tokens};
    use crate::domains::agent::artifacts::usage_log::Entry;
    use crate::usage::Range;
    use chrono::{Duration, Utc};

    fn entry(provider: &str) -> Entry {
        let at = Utc::now() - Duration::days(1);
        Entry {
            session: "ses_1".to_string(),
            project: "/app".into(),
            engine: "opencode".to_string(),
            provider: Some(provider.to_string()),
            model: Some("model".to_string()),
            billing: Billing::Metered,
            cost: Some(1.0),
            tokens: Tokens {
                input: 100,
                ..Tokens::default()
            },
            started: at.to_rfc3339(),
            ended: at.to_rfc3339(),
        }
    }

    #[test]
    fn the_rail_needs_two_providers() {
        let mut screen = UsageScreen::default();
        assert!(!screen.rail_shown());
        let one = vec![entry("deepseek")];
        let first = listed(&one);
        screen.view = crate::usage::UsageView::read_with(&one, &[], Utc::now(), &first);
        assert_eq!(screen.view.providers.len(), 1);
        assert!(!screen.rail_shown());
        let two = vec![entry("deepseek"), entry("copilot")];
        let both = listed(&two);
        screen.view = crate::usage::UsageView::read_with(&two, &[], Utc::now(), &both);
        assert_eq!(screen.view.providers.len(), 2);
        assert!(screen.rail_shown());
    }

    #[test]
    fn the_range_opens_on_thirty_days() {
        assert_eq!(UsageScreen::default().range, Range::D30);
    }
}

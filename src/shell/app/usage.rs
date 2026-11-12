//! The Usage screen: Tools › Usage…, the full view of what the providers
//! cost and consume (see `design/components/UsageScreen`). This part holds its
//! header bar, its rail, its blank states and the pane that lays out the
//! blocks; the blocks themselves live in `usage/`.

use chrono::{Local, Utc};

use super::{
    Align, ButtonKind, Color32, CornerRadius, Duration, Frame, IboardApp, Icon, Instant, Job, Key,
    Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2, job, theme, widgets,
};
use crate::agent::{Billing, Provider, provider_key};
use crate::domains::agent::artifacts::usage_log::{self, Entry};
use crate::usage::{ListedProvider, ProviderUsage, Range, UsageView};

mod blocks;
mod breaks;
mod chart;
mod plan;
mod tiles;

use blocks::Read;
pub(super) use chart::Metric;

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
/// How long the log read may take before a block says it failed.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// The Usage screen's state: whether it is up, its range and the provider it
/// shows, and the read model redrawn for the board's scope.
pub(super) struct UsageScreen {
    pub(super) open: bool,
    pub(super) range: Range,
    /// The provider selected in the rail; `None` is All providers.
    pub(super) provider: Option<String>,
    /// Tokens or Spend, kept for the session; only a metered provider's
    /// chart offers Spend.
    pub(super) metric: Metric,
    pub(super) chart_hover: Option<usize>,
    pub(super) chart_selected: Option<usize>,
    pub(super) read: Read,
    /// The log read under way, when it takes a frame to land.
    read_job: Option<Job<Result<Vec<Entry>, String>>>,
    pub(super) view: UsageView,
}

impl Default for UsageScreen {
    fn default() -> Self {
        Self {
            open: false,
            range: Range::default(),
            provider: None,
            metric: Metric::default(),
            chart_hover: None,
            chart_selected: None,
            read: Read::default(),
            read_job: None,
            view: UsageView::read_with(&[], &[], Utc::now(), &[] as &[ListedProvider]),
        }
    }
}

impl UsageScreen {
    /// Whether the rail has more than one provider to choose between.
    pub(super) fn rail_shown(&self) -> bool {
        self.view.providers.len() >= 2
    }

    /// The provider the pane shows: the chosen one with a rail, else the only
    /// one; `None` is All providers.
    fn chosen_key(&self) -> Option<String> {
        if self.rail_shown() {
            self.provider.clone()
        } else {
            self.view
                .providers
                .first()
                .map(|provider| provider.provider.clone())
        }
    }
}

/// What the screen's head calls the board's scope.
struct Scope {
    name: String,
    /// A group's board splits by project; one project's does not.
    grouped: bool,
}

/// What the screen asked for this frame.
enum Ask {
    Provider(Option<String>),
    Range(Range),
    Measure(Metric),
    Retry,
    Settings,
}

impl IboardApp {
    /// Opens Usage on the board's scope, reading the log on its own job.
    pub(super) fn open_usage(&mut self) {
        if self.welcoming() {
            return;
        }
        self.project_menu = None;
        self.usage.open = true;
        self.usage_refresh();
    }

    /// Reads the log again now, off the UI thread, then redraws the screen
    /// when it lands.
    fn usage_refresh(&mut self) {
        let Some(path) = self.usage_log_file.clone() else {
            self.usage.read = Read::Loaded;
            self.refresh_usage_view();
            return;
        };
        self.usage.read = Read::Waiting { pulse: false };
        let read = Job::spawn(move |_| usage_log::read_result(&path));
        if let Some(ctx) = self.ctx.clone() {
            read.wake(move || ctx.request_repaint());
        }
        self.usage.read_job = Some(read);
    }

    /// Takes the log read once it lands, or fails it after [`READ_TIMEOUT`];
    /// true while it still reads.
    pub(super) fn poll_usage(&mut self) -> Option<Duration> {
        if !self.usage.open {
            self.usage.read_job = None;
            return None;
        }
        let elapsed = self.usage.read_job.as_ref()?.elapsed();
        if elapsed >= READ_TIMEOUT {
            self.usage.read_job = None;
            self.usage.read = Read::Failed(format!(
                "Couldn't read the usage log: took longer than {}s",
                READ_TIMEOUT.as_secs()
            ));
            return None;
        }
        let landed = self.usage.read_job.as_ref().and_then(Job::answer);
        match landed {
            None => {
                if elapsed >= theme::SKELETON_AFTER && self.usage.read.waiting() == Some(false) {
                    self.usage.read = Read::Waiting { pulse: true };
                }
                Some(job::POLL_EVERY)
            }
            Some(Ok(entries)) => {
                self.usage_entries = entries;
                self.usage_read = Some(Instant::now());
                self.usage.read = Read::Loaded;
                self.usage.read_job = None;
                self.refresh_usage_view();
                None
            }
            Some(Err(error)) => {
                self.usage.read = Read::Failed(error);
                self.usage.read_job = None;
                None
            }
        }
    }

    /// Rebuilds the read model for the board's scope. A board switch calls it,
    /// so the screen follows the group or project that opened.
    pub(super) fn refresh_usage_view(&mut self) {
        let now = Utc::now();
        let projects = self.board_projects();
        let listed = listed(&self.usage_entries);
        let meters = usage_log::subscription_meters_with(
            &self.prefs.usage,
            &self.usage_entries,
            now,
            &self.session.quotas,
        );
        let mut view = UsageView::read_with(&self.usage_entries, &projects, now, &listed);
        view = view.with_meters(&meters);
        self.usage.view = view;
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
        let metered = rows[next].as_deref().is_some_and(|key| {
            self.usage
                .view
                .provider(key)
                .is_some_and(|provider| provider.billing == Billing::Metered)
        });
        if !metered {
            self.usage.metric = Metric::Tokens;
        }
        self.usage.chart_hover = None;
        self.usage.chart_selected = None;
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
        !self.prefs.agent.configured()
            || (self.usage.view.providers.is_empty() && self.usage.read.failed().is_none())
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
        } else if self.usage.view.providers.is_empty() && self.usage.read.failed().is_none() {
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
            Some(Ask::Provider(provider)) => {
                self.usage.chart_hover = None;
                self.usage.chart_selected = None;
                let metered = provider.as_deref().is_some_and(|key| {
                    self.usage
                        .view
                        .provider(key)
                        .is_some_and(|provider| provider.billing == Billing::Metered)
                });
                if !metered {
                    self.usage.metric = Metric::Tokens;
                }
                self.usage.provider = provider;
            }
            Some(Ask::Range(range)) => {
                self.usage.range = range;
                self.usage.chart_hover = None;
                self.usage.chart_selected = None;
            }
            Some(Ask::Measure(metric)) => {
                self.usage.metric = metric;
                self.usage.chart_hover = None;
                self.usage.chart_selected = None;
            }
            Some(Ask::Retry) => self.usage_refresh(),
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
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .content_margin(theme::scroll_margin(theme::SPACE_6, theme::SPACE_5))
                        .auto_shrink([false, false]),
                    |ui| {
                        if let Some(picked) = pane(ui, &mut self.usage, &scope) {
                            ask = Some(picked);
                        }
                    },
                );
            });
        ask
    }

    /// What the head calls the board's scope: the group, or the project.
    fn usage_scope(&self) -> Scope {
        match self.shown_group() {
            Some(group) => Scope {
                name: format!("in the {} group", group.name),
                grouped: true,
            },
            None => Scope {
                name: format!("in {}", self.board.name()),
                grouped: false,
            },
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

/// The main pane: the head, the plan window, the tiles, the per-day chart and
/// the breakdowns, each block on its own read.
fn pane(ui: &mut egui::Ui, screen: &mut UsageScreen, scope: &Scope) -> Option<Ask> {
    let mut ask = None;
    ui.spacing_mut().item_spacing.y = theme::SPACE_5;
    let key = screen.chosen_key();
    let UsageScreen {
        view,
        range,
        read,
        metric,
        chart_hover,
        chart_selected,
        ..
    } = screen;
    let chosen = key.as_deref().and_then(|key| view.provider(key));
    if let Some(picked) = head(ui, chosen, scope, *range) {
        ask = Some(picked);
    }
    if let Some(provider) = chosen
        && !provider.has_run()
    {
        if no_run_yet(ui, &provider.name, read) {
            ask = Some(Ask::Retry);
        }
        return ask;
    }
    if let Some(plan) = chosen.and_then(|provider| provider.plan.as_ref()) {
        if let Some(error) = read.failed() {
            blocks::card(ui, |ui| {
                if blocks::failure(ui, error) {
                    ask = Some(Ask::Retry);
                }
            });
        } else {
            plan::plan_panel(ui, plan, view.now);
        }
    }
    let effective = match chosen {
        Some(provider) if provider.billing == Billing::Metered => *metric,
        _ => Metric::Tokens,
    };
    let data = chart::ChartData::of(view, chosen, *range, effective);
    if tiles::tiles(ui, view, chosen, *range, &data, read) {
        ask = Some(Ask::Retry);
    }
    let mut state = chart::State {
        range: *range,
        metric,
        hover: chart_hover,
        selected: chart_selected,
    };
    match chart::chart_ui(ui, view, chosen, &data, &mut state, read) {
        Some(chart::Asked::Measure(metric)) => ask = Some(Ask::Measure(metric)),
        Some(chart::Asked::Retry) => ask = Some(Ask::Retry),
        None => {}
    }
    match breaks::breakdowns(ui, view, chosen, *range, scope.grouped, read) {
        Some(breaks::Asked::Provider(provider)) => ask = Some(Ask::Provider(Some(provider))),
        Some(breaks::Asked::Retry) => ask = Some(Ask::Retry),
        None => {}
    }
    ask
}

/// The head: the provider's swatch and name, how it is paid and what the
/// numbers cover, then the range.
fn head(
    ui: &mut egui::Ui,
    chosen: Option<&ProviderUsage>,
    scope: &Scope,
    range: Range,
) -> Option<Ask> {
    let p = theme::palette(ui);
    let mut ask = None;
    let (title, meta) = match chosen {
        Some(provider) => (
            provider.name.clone(),
            format!(
                "{} · counted from the tool's sessions {}",
                billing_note(provider.billing),
                scope.name
            ),
        ),
        None => (
            "All providers".to_string(),
            format!(
                "Every connected provider, stacked · counted from the tool's sessions {}",
                scope.name
            ),
        ),
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        if let Some(provider) = chosen {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(SWATCH), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), p.label(provider.hue));
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.label(RichText::new(title).font(theme::ui_title()).color(p.ink));
            ui.label(RichText::new(meta).font(theme::meta()).color(p.ink_muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(range) = range_control(ui, range) {
                ask = Some(Ask::Range(range));
            }
        });
    });
    ask
}

fn billing_note(billing: Billing) -> &'static str {
    match billing {
        Billing::Metered => "Metered · billed per token",
        Billing::Subscription => "Subscription · API-equivalent dollars",
    }
}

/// A provider that never ran: its pane says so under the head.
fn no_run_yet(ui: &mut egui::Ui, name: &str, read: &Read) -> bool {
    let mut retry = false;
    blocks::card(ui, |ui| {
        if let Some(error) = read.failed() {
            retry = blocks::failure(ui, error);
            return;
        }
        let p = theme::palette(ui);
        ui.label(
            RichText::new("No agent run yet.")
                .font(theme::ui_strong())
                .color(p.ink),
        );
        ui.label(
            RichText::new(format!("A finished run through {name} lands here."))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
    retry
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

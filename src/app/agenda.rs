//! The Agenda at the foot of Project stats: commits per day, a column per
//! week, and the selected day's commits beside it (see
//! `design/components/Agenda`).

use chrono::{Datelike, Duration as Days, NaiveDate};
use iboard::agenda::{
    Agenda, AgendaDays, DayCommit, ProjectRow, Range, Week, day_label, project_rows, streak,
};

use super::stats::blocks::{clock_note, failure};
use super::{
    Align, ButtonKind, Color32, CornerRadius, Duration, Frame, IboardApp, Key, Layout, MarginF32,
    Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2, theme, widgets,
};

/// A day's square, and the gap between squares.
const DAY: f32 = 18.0;
const GAP: f32 = 3.0;
/// The `Mon` `Wed` `Fri` labels' width, and the month row's height.
const WEEKDAYS_W: f32 = 30.0;
const MONTHS_H: f32 = 14.0;
/// A fold of empty weeks.
const FOLD_W: f32 = 8.0;
/// The day panel's narrowest width, and the window below which it goes under
/// the grid.
const DAY_PANEL_W: f32 = 260.0;
const SIDE_BY_SIDE: f32 = 640.0;
/// The commits listed for a day before "N more".
const DAY_COMMITS: usize = 5;
const COMMIT_TIME_W: f32 = 36.0;
const COMMIT_SHA_W: f32 = 56.0;
/// The block's height, so a failure keeps its size.
const AGENDA_H: f32 = 332.0;
/// The narrowest the grid's pane goes, to hold its head and legend.
const MAIN_W: f32 = 420.0;
/// A "By project" row: its height and inset, the weekly bars' column, and the
/// total and last-commit columns beside their gap.
const ROW_H: f32 = 28.0;
const ROW_PAD: f32 = 8.0;
const ROW_GAP: f32 = 10.0;
const SPARK_W: f32 = 94.0;
const TOTAL_W: f32 = 34.0;
const LAST_W: f32 = 72.0;
const BAR_GAP: f32 = 2.0;
const BAR_MAX_H: f32 = 16.0;

/// What the Agenda has to show.
pub(super) enum Show<'a> {
    /// No days yet; `pulse` once the placeholder may show, after reading
    /// `projects` repositories.
    Waiting {
        pulse: bool,
        projects: usize,
    },
    Failed(&'a str),
    Days(&'a AgendaDays),
}

impl IboardApp {
    /// Arrows move the selected day, `[` and `]` a week, `1`–`4` pick a range.
    pub(super) fn agenda_keys(&mut self, ctx: &egui::Context) {
        let Some(agenda) = self.session.stats.as_mut().map(|p| &mut p.agenda) else {
            return;
        };
        let moves = [
            (Key::ArrowUp, -1),
            (Key::ArrowDown, 1),
            (Key::ArrowLeft, -7),
            (Key::ArrowRight, 7),
            (Key::OpenBracket, -7),
            (Key::CloseBracket, 7),
        ];
        for (key, days) in moves {
            if ctx.input(|i| i.key_pressed(key)) {
                agenda.step(days);
            }
        }
        let ranges = [Key::Num1, Key::Num2, Key::Num3, Key::Num4];
        for (key, (range, _)) in ranges.into_iter().zip(Range::ALL) {
            if ctx.input(|i| i.key_pressed(key)) {
                agenda.set_range(range);
            }
        }
    }
}

/// The Agenda: its head, the grid and its legend, and the selected day. True
/// when Retry was pressed.
pub(super) fn agenda_ui(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    show: Show,
    clock: Option<Duration>,
) -> bool {
    let p = theme::palette(ui);
    let mut retry = false;
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_soft))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(AGENDA_H);
            match show {
                Show::Waiting { pulse, projects } => {
                    let width = WEEKDAYS_W + agenda.range.weeks() as f32 * (DAY + GAP);
                    panes(
                        ui,
                        agenda,
                        width,
                        |ui, agenda| {
                            head(ui, agenda, clock);
                            summary(ui, &reading(projects));
                            scrolled_grid(ui, agenda, &AgendaDays::default(), None, pulse);
                        },
                        |_, _| {},
                    );
                }
                Show::Failed(error) => {
                    pane(ui, ui.available_width(), |ui| {
                        head(ui, agenda, None);
                        retry = failure(ui, "git history", error);
                    });
                }
                Show::Days(days) => {
                    let width = grid_width(&agenda.weeks(days));
                    panes(
                        ui,
                        agenda,
                        width,
                        |ui, agenda| retry = main(ui, agenda, days, clock),
                        |ui, agenda| day_panel(ui, agenda, days),
                    );
                }
            }
        });
    retry
}

/// "Reading git history in 3 projects…", or without a count when none is
/// supervised.
fn reading(projects: usize) -> String {
    let unit = if projects == 1 { "project" } else { "projects" };
    if projects == 0 {
        "Reading git history…".to_string()
    } else {
        format!("Reading git history in {projects} {unit}…")
    }
}

/// The grid's pane and the day's, divided by a `line`: side by side with the
/// grid as wide as it needs and the day taking the rest, or one under the other
/// on a narrow window.
fn panes<S>(
    ui: &mut egui::Ui,
    state: &mut S,
    grid: f32,
    left: impl FnOnce(&mut egui::Ui, &mut S),
    right: impl FnOnce(&mut egui::Ui, &mut S),
) {
    let width = ui.available_width();
    if width < SIDE_BY_SIDE {
        pane(ui, width, |ui| left(ui, state));
        widgets::rule(ui);
        pane(ui, width, |ui| right(ui, state));
        return;
    }
    let main = (grid + 2.0 * theme::SPACE_5)
        .max(MAIN_W)
        .min(width - DAY_PANEL_W);
    let x = ui.min_rect().left() + main;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        pane(ui, main, |ui| left(ui, state));
        pane(ui, width - main, |ui| right(ui, state));
    });
    let p = theme::palette(ui);
    ui.painter()
        .vline(x, ui.min_rect().y_range(), Stroke::new(1.0, p.line));
}

/// One pane of the Agenda, `width` wide with the block's padding.
fn pane(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let spacing = ui.ctx().global_style().spacing.item_spacing;
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        Frame::new().inner_margin(theme::SPACE_5).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = spacing;
            ui.spacing_mut().item_spacing.y = theme::SPACE_4;
            add(ui);
        });
    });
}

/// "Agenda" with its project filter and range beside it, and the clock at the
/// far end.
fn head(ui: &mut egui::Ui, agenda: &mut Agenda, clock: Option<Duration>) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
        ui.label(RichText::new("Agenda").font(theme::ui_title()).color(p.ink));
        if let Some(project) = agenda.filter().map(str::to_string)
            && widgets::filter_token(ui, &project).clicked()
        {
            agenda.clear_filter();
        }
        if let Some(range) = widgets::segmented(ui, agenda.range, &Range::ALL) {
            agenda.set_range(range);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            clock_note(ui, clock);
        });
    });
}

fn summary(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
}

fn main(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: &AgendaDays,
    clock: Option<Duration>,
) -> bool {
    head(ui, agenda, clock);
    let filter = agenda.filter().map(str::to_string);
    let shown: Vec<(&NaiveDate, usize)> = days
        .days
        .range(agenda.first_day()..)
        .map(|(day, _)| (day, days.count_on(*day, filter.as_deref())))
        .filter(|(_, count)| *count > 0)
        .collect();
    let total: usize = shown.iter().map(|(_, n)| n).sum();
    if total == 0 {
        empty(ui, agenda, days);
        return false;
    }
    let weeks = agenda.weeks(days);
    summary(ui, &line(agenda, days, &shown, total, filter.as_deref()));
    folds(ui, &weeks);
    let busiest = shown.iter().map(|(_, n)| *n).max().unwrap_or(1);
    let step = busiest.div_ceil(4).max(1);
    scrolled_grid(ui, agenda, days, Some((&weeks, step)), false);
    legend(ui, step);
    by_project(ui, agenda, days)
}

/// What the Agenda counts in the range shown: total, projects and streak.
fn line(
    agenda: &Agenda,
    days: &AgendaDays,
    shown: &[(&NaiveDate, usize)],
    total: usize,
    filter: Option<&str>,
) -> String {
    let mut text = format!("{total} commits");
    let projects = days.projects_in(agenda.first_day(), agenda.today(), filter);
    if filter.is_none() && projects > 0 {
        let unit = if projects == 1 { "project" } else { "projects" };
        text.push_str(&format!(" · {projects} {unit}"));
    }
    let active = shown.len();
    let unit = if active == 1 {
        "active day"
    } else {
        "active days"
    };
    text.push_str(&format!(" · {active} {unit}"));
    let streak = streak(days, agenda.today(), filter);
    if streak >= 2 {
        text.push_str(&format!(" · {streak}-day streak"));
    }
    text
}

/// The folded spans the wide ranges left out, when they have any.
fn folds(ui: &mut egui::Ui, weeks: &[Week]) {
    let p = theme::palette(ui);
    let folds: Vec<String> = weeks
        .iter()
        .filter_map(|week| match week {
            Week::Folded(from, to) => Some(format!(
                "{} to {}",
                from.format("%-d %b"),
                to.format("%-d %b")
            )),
            Week::Days(_) => None,
        })
        .collect();
    if !folds.is_empty() {
        ui.label(
            RichText::new(format!("Folded, no commits: {}", folds.join(", ")))
                .font(theme::meta())
                .color(p.ink_faint),
        );
    }
}

/// Nothing in range: when the last commit was and where, and a wider range
/// when there is one.
fn empty(ui: &mut egui::Ui, agenda: &mut Agenda, days: &AgendaDays) {
    let last = days
        .last(agenda.filter())
        .map(|(day, commit)| {
            format!(
                " The last one was on {} in {}.",
                day.format("%-d %b"),
                commit.project
            )
        })
        .unwrap_or_default();
    summary(
        ui,
        &format!(
            "No commits in the last {} weeks.{last}",
            agenda.range.weeks()
        ),
    );
    if let Some(wider) = agenda.range.wider() {
        let label = format!("Show {}", wider.name());
        if widgets::button(ui, ButtonKind::Secondary, None, &label, None).clicked() {
            agenda.set_range(wider);
        }
    }
}

/// The heatmap in the width it has, its newest weeks showing.
fn scrolled_grid(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: &AgendaDays,
    shape: Option<(&[Week], usize)>,
    pulse: bool,
) {
    theme::scroll_area(
        ui,
        egui::ScrollArea::horizontal()
            .id_salt("agenda-grid")
            .content_margin(MarginF32 {
                bottom: theme::SCROLL_CLEAR,
                ..MarginF32::ZERO
            })
            .stick_to_right(true),
        |ui| grid(ui, agenda, days, shape, pulse),
    );
}

/// The heatmap's width over `weeks`.
fn grid_width(weeks: &[Week]) -> f32 {
    WEEKDAYS_W
        + weeks
            .iter()
            .map(|week| match week {
                Week::Days(_) => DAY + GAP,
                Week::Folded(..) => FOLD_W + GAP,
            })
            .sum::<f32>()
}

/// The heatmap as one `grid` node holding its days, for screen readers.
fn grid(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: &AgendaDays,
    shape: Option<(&[Week], usize)>,
    pulse: bool,
) {
    ui.scope(|ui| {
        ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
            node.set_role(egui::accesskit::Role::Grid);
            node.set_label("Commits per day");
        });
        grid_days(ui, agenda, days, shape, pulse);
    });
}

/// `Mon` `Wed` `Fri` down the grid's left, in `meta`.
fn weekday_marks(painter: &egui::Painter, p: &theme::Palette, rect: Rect) {
    for (row, name) in [(0, "Mon"), (2, "Wed"), (4, "Fri")] {
        let y = rect.top() + MONTHS_H + GAP + row as f32 * (DAY + GAP) + DAY / 2.0;
        painter.text(
            Pos2::new(rect.left(), y),
            egui::Align2::LEFT_CENTER,
            name,
            theme::meta(),
            p.ink_muted,
        );
    }
}

/// The heatmap's squares; `None` while loading draws them, pulsing from
/// `pulse` on.
fn grid_days(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: &AgendaDays,
    shape: Option<(&[Week], usize)>,
    pulse: bool,
) {
    let p = theme::palette(ui);
    let loading: Vec<Week>;
    let (weeks, step) = match shape {
        Some(shape) => shape,
        None => {
            loading = (0..agenda.range.weeks())
                .map(|week| Week::Days(agenda.first_day() + Days::weeks(week)))
                .collect();
            (loading.as_slice(), 1)
        }
    };
    let size = Vec2::new(grid_width(weeks), MONTHS_H + GAP + 7.0 * (DAY + GAP));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    // The last week's month name is wider than its column.
    let painter = ui.painter_at(rect.with_max_x(rect.max.x + theme::SPACE_2));
    let font = theme::meta();
    weekday_marks(&painter, p, rect);
    let placeholder = shape.is_none().then(|| {
        if pulse {
            theme::skeleton_fill(ui.ctx(), p)
        } else {
            p.surface
        }
    });
    let filter = agenda.filter().map(str::to_string);
    let mut x = rect.left() + WEEKDAYS_W;
    let mut month = None;
    for week in weeks {
        let Week::Days(start) = *week else {
            fold_mark(&painter, p, x, rect.top() + MONTHS_H + GAP);
            x += FOLD_W + GAP;
            continue;
        };
        if month != Some(start.month()) {
            if month.is_some() || start.day() <= 7 {
                painter.text(
                    Pos2::new(x, rect.top()),
                    egui::Align2::LEFT_TOP,
                    start.format("%b").to_string(),
                    font.clone(),
                    p.ink_muted,
                );
            }
            month = Some(start.month());
        }
        for weekday in 0..7 {
            let day = start + Days::days(weekday);
            if day > agenda.today() {
                break;
            }
            let top = rect.top() + MONTHS_H + GAP + weekday as f32 * (DAY + GAP);
            let square = Rect::from_min_size(Pos2::new(x, top), Vec2::splat(DAY));
            let count = days.count_on(day, filter.as_deref());
            let fill = placeholder.unwrap_or_else(|| theme::agenda_fill(p, count, step));
            painter.rect_filled(square, CornerRadius::same(4), fill);
            if shape.is_none() {
                continue;
            }
            let id = ui.scope_id().with(("agenda-day", day));
            let response = ui.interact(square, id, Sense::click());
            let label = day_label(day, count);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            if response.hovered() {
                painter.rect_stroke(
                    square.expand(1.0),
                    CornerRadius::same(5),
                    Stroke::new(1.0, p.line_strong),
                    StrokeKind::Outside,
                );
            }
            if day == agenda.today() {
                painter.rect_stroke(
                    square,
                    CornerRadius::same(4),
                    Stroke::new(2.0, p.ink),
                    StrokeKind::Inside,
                );
            }
            if day == agenda.selected {
                painter.rect_stroke(
                    square.expand(1.0),
                    CornerRadius::same(5),
                    Stroke::new(2.0, p.accent),
                    StrokeKind::Outside,
                );
            }
            if response.clicked() {
                agenda.selected = day;
            }
            if response.hovered() {
                response.on_hover_text(day_tooltip(days, day, count, filter.as_deref()));
            }
        }
        x += DAY + GAP;
    }
}

/// A day's tooltip: its total, then the projects that committed.
fn day_tooltip(days: &AgendaDays, day: NaiveDate, count: usize, filter: Option<&str>) -> String {
    let mut text = day_label(day, count);
    if filter.is_some() {
        return text;
    }
    let mut projects: Vec<(&str, usize)> = Vec::new();
    for commit in days.commits_on(day, None) {
        let project = commit.project.as_str();
        match projects.iter_mut().find(|(name, _)| *name == project) {
            Some((_, n)) => *n += 1,
            None => projects.push((project, 1)),
        }
    }
    projects.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (name, n) in projects {
        text.push_str(&format!("\n{name} · {n}"));
    }
    text
}

/// The dashed line of a fold of empty weeks, from `top`.
fn fold_mark(painter: &egui::Painter, p: &theme::Palette, x: f32, top: f32) {
    let fold = Rect::from_min_size(Pos2::new(x, top), Vec2::new(FOLD_W, 7.0 * DAY));
    painter.extend(Shape::dashed_line(
        &[fold.center_top(), fold.center_bottom()],
        Stroke::new(1.0, p.line),
        3.0,
        3.0,
    ));
}

/// What each shade stands for, in commits, and the today mark.
fn legend(ui: &mut egui::Ui, step: usize) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_4;
        for level in 1..=4 {
            let low = (level - 1) * step + 1;
            let text = match (level, step) {
                (4, _) => format!("{low}+"),
                (_, 1) => low.to_string(),
                _ => format!("{low}–{}", level * step),
            };
            swatch(ui, theme::agenda_fill(p, low, step), None);
            ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
        }
        swatch(ui, p.surface, Some(p.ink));
        ui.label(
            RichText::new("today")
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
}

fn swatch(ui: &mut egui::Ui, fill: Color32, ring: Option<Color32>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    if let Some(ring) = ring {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(2.0, ring),
            StrokeKind::Inside,
        );
    }
}

/// The "By project" rows, busiest first, and the projects git could not
/// read with Retry. True when Retry was pressed.
fn by_project(ui: &mut egui::Ui, agenda: &mut Agenda, days: &AgendaDays) -> bool {
    let p = theme::palette(ui);
    ui.label(
        RichText::new("BY PROJECT · LAST 12 WEEKS")
            .font(theme::ui_label())
            .color(p.ink_muted),
    );
    ui.spacing_mut().item_spacing.y = 0.0;
    let filtered = agenda.filter().map(str::to_string);
    let mut picked = None;
    for row in project_rows(days, agenda.today()) {
        if project_row(ui, &row, agenda.today(), filtered.as_deref()) {
            picked = Some(row.name);
        }
    }
    let mut retry = false;
    for (name, error) in &days.failed {
        if failed_row(ui, name, error) {
            retry = true;
        }
    }
    if let Some(name) = picked {
        agenda.toggle_filter(&name);
    }
    retry
}

/// One project's 12 weeks as bars, its total and the date of its last
/// commit; true when clicked.
fn project_row(
    ui: &mut egui::Ui,
    row: &ProjectRow,
    today: NaiveDate,
    filtered: Option<&str>,
) -> bool {
    let p = theme::palette(ui);
    let selected = filtered == Some(row.name.as_str());
    let dim = filtered.is_some() && !selected;
    let last = last_seen(row.last, today);
    let label = match row.last {
        Some(_) => format!("{}, {} commits, last {last}", row.name, row.total),
        None => format!("{}, no commits read", row.name),
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.accent_soft);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
    }
    let ink = if dim { p.ink_faint } else { p.ink };
    let inner = rect.shrink2(Vec2::new(ROW_PAD, 0.0));
    let fixed = SPARK_W + TOTAL_W + LAST_W + 3.0 * ROW_GAP;
    let name = Rect::from_min_size(
        inner.min,
        Vec2::new((inner.width() - fixed).max(0.0), ROW_H),
    );
    let galley = painter.layout_no_wrap(row.name.clone(), theme::ui_body(), ink);
    let text_y = name.center().y - galley.size().y / 2.0;
    painter
        .with_clip_rect(name)
        .galley(Pos2::new(name.left(), text_y), galley, ink);
    let spark = Rect::from_min_size(
        Pos2::new(name.right() + ROW_GAP, rect.center().y - BAR_MAX_H / 2.0),
        Vec2::new(SPARK_W, BAR_MAX_H),
    );
    spark_bars(painter, p, spark, row, dim);
    let total = Rect::from_min_size(
        Pos2::new(spark.right() + ROW_GAP, rect.top()),
        Vec2::new(TOTAL_W, ROW_H),
    );
    painter.text(
        total.right_center(),
        egui::Align2::RIGHT_CENTER,
        row.total.to_string(),
        theme::meta(),
        ink,
    );
    let last_rect = Rect::from_min_size(
        Pos2::new(total.right() + ROW_GAP, rect.top()),
        Vec2::new(LAST_W, ROW_H),
    );
    painter.text(
        last_rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        last,
        theme::meta(),
        if dim { p.ink_faint } else { p.ink_muted },
    );
    response.on_hover_text(&label).clicked()
}

/// A row's 12 weekly bars, each week scaled to the project's busiest.
fn spark_bars(
    painter: &egui::Painter,
    p: &theme::Palette,
    spark: Rect,
    row: &ProjectRow,
    dim: bool,
) {
    let weeks = row.weeks.len() as f32;
    let width = ((SPARK_W - (weeks - 1.0) * BAR_GAP) / weeks).max(1.0);
    let busiest = row.weeks.iter().copied().max().unwrap_or(0).max(1);
    let fill = if dim { p.line } else { p.accent };
    for (index, count) in row.weeks.iter().enumerate() {
        let x = spark.left() + index as f32 * (width + BAR_GAP);
        let (height, color) = if *count == 0 {
            (1.0, p.line)
        } else {
            (BAR_MAX_H * *count as f32 / busiest as f32, fill)
        };
        let bar = Rect::from_min_size(
            Pos2::new(x, spark.bottom() - height),
            Vec2::new(width, height),
        );
        painter.rect_filled(
            bar,
            CornerRadius {
                nw: 1,
                ne: 1,
                sw: 0,
                se: 0,
            },
            color,
        );
    }
}

/// "today", a day, or a dash for a project with no commits read.
fn last_seen(day: Option<NaiveDate>, today: NaiveDate) -> String {
    match day {
        None => "–".to_string(),
        Some(day) if day == today => "today".to_string(),
        Some(day) => day.format("%-d %b").to_string(),
    }
}

/// A project git could not read: its name, why in `danger`, and Retry; true
/// when Retry was pressed.
fn failed_row(ui: &mut egui::Ui, name: &str, error: &str) -> bool {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let retry = widgets::button(ui, ButtonKind::Ghost, None, "Retry", None).clicked();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label(RichText::new(name).font(theme::ui_body()).color(p.ink));
                ui.add(
                    egui::Label::new(RichText::new(error).font(theme::meta()).color(p.danger))
                        .truncate(),
                )
                .on_hover_text(error);
            });
            retry
        })
        .inner
    })
    .inner
}

/// The selected day: how many commits and projects, then each project's
/// commits, busiest first, newest first.
fn day_panel(ui: &mut egui::Ui, agenda: &Agenda, days: &AgendaDays) {
    let p = theme::palette(ui);
    let day = agenda.selected;
    let filter = agenda.filter().map(str::to_string);
    let commits: Vec<&DayCommit> = days.commits_on(day, filter.as_deref()).collect();
    let groups = group_commits(&commits);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    ui.label(
        RichText::new(day.format("%A %-d %B").to_string())
            .font(theme::ui_strong())
            .color(p.ink),
    );
    summary(ui, &commits_line(commits.len(), groups.len()));
    ui.add_space(theme::SPACE_2);
    for (name, list) in groups {
        commit_group(ui, name, &list);
    }
}

/// "No commits", or "15 commits · 2 projects".
fn commits_line(commits: usize, projects: usize) -> String {
    if commits == 0 {
        return "No commits".to_string();
    }
    let unit = if commits == 1 { "commit" } else { "commits" };
    let project = if projects == 1 { "project" } else { "projects" };
    format!("{commits} {unit} · {projects} {project}")
}

/// The day's commits grouped by project, busiest first, each newest first.
fn group_commits<'a>(commits: &[&'a DayCommit]) -> Vec<(&'a str, Vec<&'a DayCommit>)> {
    let mut groups: Vec<(&str, Vec<&DayCommit>)> = Vec::new();
    for commit in commits {
        let project = commit.project.as_str();
        match groups.iter_mut().find(|(name, _)| *name == project) {
            Some((_, list)) => list.push(*commit),
            None => groups.push((project, vec![*commit])),
        }
    }
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
    groups
}

/// One project's commits on the day, capped at [`DAY_COMMITS`].
fn commit_group(ui: &mut egui::Ui, name: &str, commits: &[&DayCommit]) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_1);
    ui.label(
        RichText::new(format!("{name} · {}", commits.len()))
            .font(theme::ui_label())
            .color(p.ink_muted),
    );
    for commit in commits.iter().take(DAY_COMMITS) {
        ui.horizontal(|ui| {
            commit_cell(
                ui,
                COMMIT_TIME_W,
                RichText::new(&commit.time)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
            commit_cell(
                ui,
                COMMIT_SHA_W,
                RichText::new(&commit.short)
                    .font(theme::meta())
                    .color(p.accent),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(&commit.subject)
                        .font(theme::ui_small())
                        .color(p.ink),
                )
                .truncate(),
            )
            .on_hover_text(&commit.subject);
        });
    }
    if commits.len() > DAY_COMMITS {
        ui.horizontal(|ui| {
            ui.add_space(COMMIT_TIME_W + ui.spacing().item_spacing.x);
            let more = commits.len() - DAY_COMMITS;
            summary(ui, &format!("{more} more in {name}"));
        });
    }
}

/// A commit's time or sha, in a column of its own so its subjects line up.
fn commit_cell(ui: &mut egui::Ui, width: f32, text: RichText) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, ui.available_height()),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.label(text);
        },
    );
}

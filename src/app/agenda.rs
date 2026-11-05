//! The Agenda at the foot of Project stats: commits per day, a column per
//! week, and the selected day's commits beside it (see
//! `design/components/Agenda`).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration as Days, NaiveDate};
use iboard::agenda::{Agenda, DayCommit, Range, Week, commits_on, day_label, streak};

use super::*;

/// A day's square, and the gap between squares.
const DAY: f32 = 18.0;
const GAP: f32 = 3.0;
/// The `Mon` `Wed` `Fri` labels' width, and the month row's height.
const WEEKDAYS_W: f32 = 26.0;
const MONTHS_H: f32 = 14.0;
/// A fold of empty weeks.
const FOLD_W: f32 = 8.0;
/// The day panel's narrowest width, and the window below which it goes under
/// the grid.
const DAY_PANEL_W: f32 = 260.0;
const SIDE_BY_SIDE: f32 = 640.0;
/// The commits listed for a day before "N more".
const DAY_COMMITS: usize = 8;

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

/// The Agenda: its head, the grid and its legend, and the selected day.
pub(super) fn agenda_ui(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: Option<&BTreeMap<NaiveDate, Vec<DayCommit>>>,
) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(theme::SPACE_5)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let side = ui.available_width() >= SIDE_BY_SIDE;
            let Some(days) = days else {
                head(ui, agenda);
                summary(ui, "Reading git history…");
                grid(ui, agenda, &BTreeMap::new(), None);
                return;
            };
            if side {
                ui.horizontal_top(|ui| {
                    let main_w = ui.available_width() - DAY_PANEL_W - theme::SPACE_5;
                    ui.allocate_ui_with_layout(
                        Vec2::new(main_w, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| main(ui, agenda, days),
                    );
                    ui.add_space(theme::SPACE_5);
                    ui.vertical(|ui| day_panel(ui, agenda.selected, days));
                });
            } else {
                main(ui, agenda, days);
                ui.add_space(theme::SPACE_4);
                widgets::rule(ui);
                ui.add_space(theme::SPACE_4);
                day_panel(ui, agenda.selected, days);
            }
        });
}

fn head(ui: &mut egui::Ui, agenda: &mut Agenda) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Agenda").font(theme::ui_title()).color(p.ink));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut options = Range::ALL;
            options.reverse();
            if let Some(range) = widgets::segmented(ui, agenda.range, &options) {
                agenda.set_range(range);
            }
        });
    });
}

fn summary(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(RichText::new(text).font(theme::meta()).color(p.ink_muted));
}

fn main(ui: &mut egui::Ui, agenda: &mut Agenda, days: &BTreeMap<NaiveDate, Vec<DayCommit>>) {
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.y = theme::SPACE_4;
    head(ui, agenda);
    let shown: Vec<(&NaiveDate, usize)> = days
        .range(agenda.first_day()..)
        .map(|(day, commits)| (day, commits.len()))
        .collect();
    let total: usize = shown.iter().map(|(_, n)| n).sum();
    if total == 0 {
        let last = days
            .keys()
            .next_back()
            .map(|day| format!(" The last one was on {}.", day.format("%-d %b")))
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
        return;
    }
    let weeks = agenda.weeks(days);
    let mut line = format!("{total} commits · {} active days", shown.len());
    let streak = streak(days, agenda.today());
    if streak >= 2 {
        line.push_str(&format!(" · {streak}-day streak"));
    }
    summary(ui, &line);
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
    let busiest = shown.iter().map(|(_, n)| *n).max().unwrap_or(1);
    let step = busiest.div_ceil(4).max(1);
    theme::scroll_area(
        ui,
        egui::ScrollArea::horizontal()
            .id_salt("agenda-grid")
            .content_margin(MarginF32 {
                bottom: theme::SCROLL_CLEAR,
                ..MarginF32::ZERO
            })
            .stick_to_right(true),
        true,
        |ui| grid(ui, agenda, days, Some((&weeks, step))),
    );
    legend(ui, step);
}

/// The heatmap; `None` while loading draws the grid with pulsing days.
fn grid(
    ui: &mut egui::Ui,
    agenda: &mut Agenda,
    days: &BTreeMap<NaiveDate, Vec<DayCommit>>,
    shape: Option<(&[Week], usize)>,
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
    let width = weeks
        .iter()
        .map(|week| match week {
            Week::Days(_) => DAY + GAP,
            Week::Folded(..) => FOLD_W + GAP,
        })
        .sum::<f32>();
    let size = Vec2::new(WEEKDAYS_W + width, MONTHS_H + GAP + 7.0 * (DAY + GAP));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let font = theme::meta();
    for (row, name) in [(0, "Mon"), (2, "Wed"), (4, "Fri")] {
        let y = rect.top() + MONTHS_H + GAP + row as f32 * (DAY + GAP) + DAY / 2.0;
        painter.text(
            Pos2::new(rect.left(), y),
            egui::Align2::LEFT_CENTER,
            name,
            font.clone(),
            p.ink_muted,
        );
    }
    let placeholder = shape.is_none().then(|| theme::skeleton_fill(ui.ctx(), p));
    let mut x = rect.left() + WEEKDAYS_W;
    let mut month = None;
    for week in weeks {
        let Week::Days(start) = *week else {
            let top = rect.top() + MONTHS_H + GAP;
            let fold = Rect::from_min_size(Pos2::new(x, top), Vec2::new(FOLD_W, 7.0 * DAY));
            let dash = Stroke::new(1.0, p.line_strong);
            painter.extend(Shape::dashed_line(
                &[fold.center_top(), fold.center_bottom()],
                dash,
                3.0,
                3.0,
            ));
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
            let count = commits_on(days, day).len();
            let fill = placeholder.unwrap_or_else(|| theme::agenda_fill(p, count, step));
            painter.rect_filled(square, CornerRadius::same(4), fill);
            if shape.is_none() {
                continue;
            }
            let id = ui.id().with(("agenda-day", day));
            let response = ui.interact(square, id, Sense::click());
            let label = day_label(day, count);
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
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
            response.on_hover_text(label);
        }
        x += DAY + GAP;
    }
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

/// The selected day: how many commits, then each, newest first.
fn day_panel(ui: &mut egui::Ui, day: NaiveDate, days: &BTreeMap<NaiveDate, Vec<DayCommit>>) {
    let p = theme::palette(ui);
    let commits = commits_on(days, day);
    ui.spacing_mut().item_spacing.y = theme::SPACE_2;
    ui.label(
        RichText::new(day.format("%A %-d %B").to_string())
            .font(theme::ui_strong())
            .color(p.ink),
    );
    let count = match commits.len() {
        0 => "No commits".to_string(),
        1 => "1 commit".to_string(),
        n => format!("{n} commits"),
    };
    summary(ui, &count);
    ui.add_space(theme::SPACE_2);
    for commit in commits.iter().take(DAY_COMMITS) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&commit.time)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
            ui.label(
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
        summary(ui, &format!("{} more", commits.len() - DAY_COMMITS));
    }
}

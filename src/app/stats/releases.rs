//! The release tags on the Trends: a lane of names over the charts, a dotted
//! line through all four, and the Releases checkbox of their head (see
//! `design/components/StatsScreen`). A release sits on a commit, as the trends
//! are one sample per commit.

use std::sync::Arc;

use egui::Galley;
use egui::epaint::Mesh;
use egui::layers::ShapeIdx;
use iboard::stats::{Release, Stats};

use super::blocks::{Blocks, Part, Phase, failed_row, stale_opacity};
use super::trends::{shared_plot, step_of};
use super::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2, theme, widgets};

const LANE_H: f32 = 16.0;
/// A named release's tick, and the longer one of a release with no name.
const TICK_H: f32 = 3.0;
const BARE_TICK_H: f32 = 6.0;
/// How close to a line hovers it.
const HOVER_REACH: f32 = 4.0;
/// The closest two names sit.
const NAME_GAP: f32 = 6.0;
/// A 1pt dot and a 3pt gap.
const DOT_PITCH: f32 = 4.0;

/// The Releases checkbox: disabled, saying why, once no tag matches.
pub(super) fn checkbox(ui: &mut egui::Ui, blocks: &Blocks, show: &mut bool) {
    let tags = blocks.source().release_tags();
    let any = blocks
        .loaded(Part::Releases)
        .is_none_or(|stats| stats.releases.matched > 0);
    ui.add_enabled_ui(any, |ui| widgets::quiet_checkbox(ui, show, "Releases"))
        .inner
        .on_hover_text(format!("Release tags ({tags}) as vertical lines"))
        .on_disabled_hover_text(format!("No release tags match {tags}"));
}

/// Which of the wanted names stay, left to right: the newer keeps its name
/// when two would touch.
fn kept_names(wanted: &[Option<egui::Rangef>]) -> Vec<bool> {
    let mut kept = vec![false; wanted.len()];
    let mut left = f32::INFINITY;
    for (at, span) in wanted.iter().enumerate().rev() {
        if let Some(span) = span
            && span.max + NAME_GAP <= left
        {
            kept[at] = true;
            left = span.min;
        }
    }
    kept
}

/// The dots of a line at `x`, 1pt each on a 4pt pitch, on whole pixels: drawn
/// as a mesh, since a stroke or a filled rect this small fades away at 1x.
fn dots(x: f32, ys: std::ops::Range<f32>, pixels: f32) -> impl Iterator<Item = Rect> {
    let snap = move |at: f32| (at * pixels).round() / pixels;
    let (left, top) = (snap(x - 0.5), snap(ys.start));
    let count = ((ys.end - top) / DOT_PITCH).ceil().max(0.0) as usize;
    (0..count).map(move |at| {
        let dot = Pos2::new(left, top + DOT_PITCH * at as f32);
        Rect::from_min_size(dot, Vec2::splat(1.0))
    })
}

/// A release as it lies in the lane.
struct Mark<'a> {
    release: &'a Release,
    x: f32,
    /// Where its name lies in the lane, when it has one.
    name: Option<(Rect, Arc<Galley>)>,
}

impl Mark<'_> {
    /// What the lane can hover: its name, or its tick.
    fn rect(&self, lane: Rect) -> Rect {
        self.name.as_ref().map_or_else(
            || Rect::from_x_y_ranges(self.x - HOVER_REACH..=self.x + HOVER_REACH, lane.y_range()),
            |(rect, _)| *rect,
        )
    }
}

/// The name under `pos`, else the closest line within reach, of the marks at
/// `xs` with `names`; the newer wins a tie.
fn hit(xs: &[f32], names: &[Option<Rect>], pos: Pos2) -> Option<usize> {
    if let Some(on_name) = names
        .iter()
        .rposition(|name| name.is_some_and(|name| name.contains(pos)))
    {
        return Some(on_name);
    }
    let mut best: Option<(usize, f32)> = None;
    for (at, x) in xs.iter().enumerate().rev() {
        let away = (x - pos.x).abs();
        if away <= HOVER_REACH && best.is_none_or(|(_, closest)| away < closest) {
            best = Some((at, away));
        }
    }
    best.map(|(at, _)| at)
}

/// The lane over the charts, its lines put in once the rows are drawn.
pub(super) struct Lane {
    rect: Rect,
    /// The lines' place in the paint order: under the rows' series.
    lines: ShapeIdx,
    phase: Phase,
}

impl Lane {
    /// Takes the lane's 16pt under the head, tight against the first chart,
    /// or says why the tags can't be read.
    pub(super) fn begin(ui: &mut egui::Ui, blocks: &mut Blocks) -> Option<Self> {
        let phase = blocks.phase(Part::Releases);
        if let Phase::Failed(error) = &phase {
            ui.horizontal(|ui| failed_row(ui, blocks, Part::Releases, error));
            return None;
        }
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), LANE_H), Sense::hover());
        ui.spacing_mut().item_spacing.y = spacing;
        Some(Self {
            rect,
            lines: ui.painter().add(Shape::Noop),
            phase,
        })
    }

    /// Draws the names and the lines down to `bottom`, the last plot's; returns
    /// the commit of the release under the pointer.
    pub(super) fn finish(self, ui: &mut egui::Ui, bottom: f32) -> Option<usize> {
        let Phase::Loaded { stats, .. } = &self.phase else {
            return None;
        };
        let mut painter = ui.painter().clone();
        painter.multiply_opacity(stale_opacity(&self.phase));
        let marks = self.marks(ui, stats, &painter);
        let hot = self.hovered(ui, &marks, bottom);
        self.paint(ui, &painter, &marks, hot, bottom);
        for mark in &marks {
            let id = ui.scope_id().with(("release", &mark.release.tag));
            let response = ui.interact(mark.rect(self.rect), id, Sense::hover());
            let text = mark.release.describe();
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Unknown, true, &text));
        }
        let hot = hot.and_then(|at| marks.get(at))?;
        let tip = egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            ui.scope_id().with("release-tip"),
            egui::PopupAnchor::Pointer,
        );
        tip.show(|ui| ui.label(hot.release.describe()));
        Some(hot.release.at)
    }

    fn marks<'a>(&self, ui: &egui::Ui, stats: &'a Stats, painter: &egui::Painter) -> Vec<Mark<'a>> {
        let plot = shared_plot(ui, self.rect);
        let step = step_of(plot, stats.snapshots.len());
        let shown = &stats.releases.shown;
        let x_of = |release: &Release| plot.left() + step * release.at as f32;
        let galleys: Vec<Arc<Galley>> = shown
            .iter()
            .map(|release| {
                let name = release.tag.clone();
                painter.layout_no_wrap(name, theme::release_name(), Color32::PLACEHOLDER)
            })
            .collect();
        let spans: Vec<Option<egui::Rangef>> = shown
            .iter()
            .zip(&galleys)
            .map(|(release, galley)| {
                let width = galley.size().x;
                let left = (x_of(release) - width / 2.0).min(self.rect.right() - width);
                (!release.is_patch).then(|| egui::Rangef::new(left, left + width))
            })
            .collect();
        let kept = kept_names(&spans);
        let names = self.rect.top()..=self.rect.bottom() - TICK_H;
        shown
            .iter()
            .zip(galleys)
            .enumerate()
            .map(|(at, (release, galley))| Mark {
                release,
                x: x_of(release),
                name: spans[at]
                    .filter(|_| kept[at])
                    .map(|span| (Rect::from_x_y_ranges(span, names.clone()), galley)),
            })
            .collect()
    }

    /// The mark under the pointer, if the pointer is over the lane or the
    /// charts under it.
    fn hovered(&self, ui: &egui::Ui, marks: &[Mark], bottom: f32) -> Option<usize> {
        let area = Rect::from_min_max(self.rect.min, Pos2::new(self.rect.right(), bottom));
        if !ui.rect_contains_pointer(area) {
            return None;
        }
        let pos = ui.ctx().pointer_hover_pos()?;
        let xs: Vec<f32> = marks.iter().map(|mark| mark.x).collect();
        let names: Vec<Option<Rect>> = marks
            .iter()
            .map(|mark| mark.name.as_ref().map(|(rect, _)| *rect))
            .collect();
        hit(&xs, &names, pos)
    }

    fn paint(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        marks: &[Mark],
        hot: Option<usize>,
        bottom: f32,
    ) {
        let p = theme::palette(ui);
        let top = self.rect.bottom();
        let mut lines = Mesh::default();
        for (at, mark) in marks.iter().enumerate() {
            let is_hot = hot == Some(at);
            let tick = if mark.name.is_some() {
                TICK_H
            } else {
                BARE_TICK_H
            };
            let tick_ink = if is_hot { p.ink_muted } else { p.ink_faint };
            painter.vline(mark.x, top - tick..=top, Stroke::new(1.0, tick_ink));
            if let Some((rect, galley)) = &mark.name {
                let ink = if is_hot { p.ink } else { p.ink_muted };
                let pos = Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0);
                painter.galley(pos, Arc::clone(galley), ink);
            }
            if is_hot {
                continue;
            }
            let strength = if mark.release.is_patch { 0.5 } else { 1.0 };
            let ink = p.ink_faint.gamma_multiply(strength);
            for dot in dots(mark.x, top..bottom, painter.pixels_per_point()) {
                lines.add_colored_rect(dot, ink);
            }
        }
        painter.set(self.lines, lines);
        if let Some(mark) = hot.and_then(|at| marks.get(at)) {
            painter.vline(mark.x, top..=bottom, Stroke::new(1.0, p.ink_muted));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(left: f32, right: f32) -> Option<egui::Rangef> {
        Some(egui::Rangef::new(left, right))
    }

    #[test]
    fn the_newer_name_stays_when_two_would_touch() {
        let wanted = [span(0.0, 40.0), span(30.0, 70.0), None, span(100.0, 140.0)];
        assert_eq!(kept_names(&wanted), [false, true, false, true]);
        let tight = [span(0.0, 40.0), span(44.0, 84.0), span(90.0, 130.0)];
        assert_eq!(kept_names(&tight), [false, true, true]);
    }

    #[test]
    fn a_line_is_dots_on_whole_pixels_down_to_its_end() {
        let all: Vec<Rect> = dots(100.2, 10.0..22.0, 2.0).collect();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].min, Pos2::new(99.5, 10.0));
        assert_eq!(all[2].min, Pos2::new(99.5, 18.0));
        assert_eq!(all[0].size(), Vec2::splat(1.0));
        assert_eq!(dots(0.0, 10.0..5.0, 1.0).count(), 0);
    }

    #[test]
    fn a_line_is_hovered_within_4pt_and_a_name_anywhere_on_it() {
        let xs = [100.0, 200.0, 202.0];
        let name = Rect::from_min_max(Pos2::new(80.0, 0.0), Pos2::new(120.0, 13.0));
        let names = [Some(name), None, None];
        let at = |x: f32, y: f32| hit(&xs, &names, Pos2::new(x, y));
        assert_eq!(at(85.0, 5.0), Some(0), "on the name, far from its line");
        assert_eq!(at(85.0, 50.0), None, "off the name");
        assert_eq!(at(104.0, 50.0), Some(0));
        assert_eq!(at(104.5, 50.0), None);
        assert_eq!(at(199.0, 50.0), Some(1));
        assert_eq!(at(201.0, 50.0), Some(2), "a tie goes to the newer");
    }
}

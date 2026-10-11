//! The iboard design system mapped onto egui.
//!
//! Tokens come from `design/tokens.json` (the brand book is `design/README.md`).
//! Sizes are points, which map 1:1 to egui points. egui has no font weights, so
//! the Medium and SemiBold faces are registered as their own named families.

use std::sync::Arc;

use egui::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Painter, Pos2,
    Rect, Stroke, TextStyle, Theme, Vec2,
};

/// One theme's color tokens.
pub struct Palette {
    /// App ground: board, header, status bar.
    pub bg: Color32,
    /// Cards, reader page, editor, inputs, buttons.
    pub surface: Color32,
    /// Hover fill for buttons, crumbs and menu rows.
    pub surface_hover: Color32,
    /// Inline code, empty-column well.
    pub surface_sunken: Color32,
    /// Card borders, dividers, bar rules. Decorative only.
    pub line: Color32,
    /// Every control edge.
    pub line_strong: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub ink_faint: Color32,
    /// Selection, primary button fill, links, focus ring.
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub status_todo: Color32,
    pub status_ready: Color32,
    pub status_doing: Color32,
    pub status_done: Color32,
    /// The AI review button and the Challenge verdict. Reserved: nothing else
    /// uses this hue.
    pub review: Color32,
    /// Review panel header fill and the pending-review card flag.
    pub review_soft: Color32,
    /// Write failures, conflicting file on disk, the Delete drop zone.
    pub danger: Color32,
    /// The Delete drop zone while a card is over it.
    pub danger_soft: Color32,
    /// Label on the `danger`-filled Delete button.
    pub on_danger: Color32,
    /// Dims the board behind a modal confirmation.
    pub scrim: Color32,
    pub popover_shadow: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    bg: hex(0xf4f5f7),
    surface: hex(0xffffff),
    surface_hover: hex(0xeef0f3),
    surface_sunken: hex(0xeceef2),
    line: hex(0xcfd4db),
    line_strong: hex(0x7c8491),
    ink: hex(0x15181d),
    ink_muted: hex(0x4d5562),
    ink_faint: hex(0x626a78),
    accent: hex(0x2b54c4),
    accent_soft: hex(0xe4eafb),
    on_accent: hex(0xffffff),
    status_todo: hex(0x5e6878),
    status_ready: hex(0x0f6e78),
    status_doing: hex(0xa04f00),
    status_done: hex(0x1d7347),
    review: hex(0x7a3fd1),
    review_soft: hex(0xf1ebfc),
    danger: hex(0xb42318),
    danger_soft: hex(0xfdecea),
    on_danger: hex(0xffffff),
    scrim: Color32::from_rgba_premultiplied(7, 8, 9, 82),
    popover_shadow: Color32::from_rgba_premultiplied(3, 3, 3, 31),
};

pub const DARK: Palette = Palette {
    bg: hex(0x15171a),
    surface: hex(0x1d2024),
    surface_hover: hex(0x262a2f),
    surface_sunken: hex(0x111316),
    line: hex(0x353a42),
    line_strong: hex(0x737c89),
    ink: hex(0xe7e9ec),
    ink_muted: hex(0xa7aeb9),
    ink_faint: hex(0x8c939e),
    accent: hex(0x86a4ff),
    accent_soft: hex(0x222c47),
    on_accent: hex(0x0b1433),
    status_todo: hex(0xa4adba),
    status_ready: hex(0x4fc4cf),
    status_doing: hex(0xeda24a),
    status_done: hex(0x5fc98e),
    review: hex(0xb79cff),
    review_soft: hex(0x2a2245),
    danger: hex(0xff8a7a),
    danger_soft: hex(0x3d1a17),
    on_danger: hex(0x3a0d08),
    scrim: Color32::from_rgba_premultiplied(0, 0, 0, 140),
    popover_shadow: Color32::from_rgba_premultiplied(0, 0, 0, 128),
};

/// The palette of the theme the given UI is drawn in.
pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// The relative luminance of a colour (WCAG 2.x), 0 (black) to 1 (white).
fn luminance(color: Color32) -> f32 {
    let channel = |value: u8| {
        let c = value as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// The WCAG contrast ratio between two colours, 1 (identical) to 21.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Blends `color` into `base`, keeping mostly `base`: a soft tint of a label
/// colour that reads as a chip fill in the current theme.
pub fn tint(color: Color32, base: Color32) -> Color32 {
    let keep = if luminance(base) < 0.5 { 0.80 } else { 0.86 };
    let lerp = |a: u8, b: u8| (a as f32 * (1.0 - keep) + b as f32 * keep).round() as u8;
    Color32::from_rgb(
        lerp(color.r(), base.r()),
        lerp(color.g(), base.g()),
        lerp(color.b(), base.b()),
    )
}

// Spacing (`space-*`), radii and fixed sizes.
pub const SPACE_1: f32 = 2.0;
pub const SPACE_2: f32 = 4.0;
pub const SPACE_3: f32 = 8.0;
pub const SPACE_4: f32 = 12.0;
pub const SPACE_5: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_7: f32 = 32.0;
pub const RADIUS_SM: u8 = 4;
pub const RADIUS_MD: u8 = 6;
pub const RADIUS_PILL: u8 = 9;
pub const CONTROL_H: f32 = 24.0;
pub const BAR_H: f32 = 40.0;
pub const STATUSBAR_H: f32 = 24.0;
/// Narrowest column before the board scrolls sideways. The design system says
/// 280; 200 lets four columns (Todo, Ready, Doing, Done) fit the 880pt
/// minimum window.
pub const COLUMN_MIN: f32 = 200.0;
pub const COLUMN_MAX: f32 = 420.0;
pub const READER_MAX: f32 = 720.0;

// Font families beyond egui's two built-in ones.
const SANS_MEDIUM: &str = "plex-sans-medium";
const SANS_SEMIBOLD: &str = "plex-sans-semibold";
const MONO_MEDIUM: &str = "plex-mono-medium";

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

/// `ui-title`: project name in the header.
pub fn ui_title() -> FontId {
    FontId::new(15.0, family(SANS_SEMIBOLD))
}
/// `ui-body`: default UI text and inputs.
pub fn ui_body() -> FontId {
    FontId::proportional(13.0)
}
/// `ui-strong`: card titles, current crumb.
pub fn ui_strong() -> FontId {
    FontId::new(13.0, family(SANS_MEDIUM))
}
/// Button labels: `ui-strong` at 12.
pub fn ui_button() -> FontId {
    FontId::new(12.0, family(SANS_MEDIUM))
}
/// `ui-small`: card summaries.
pub fn ui_small() -> FontId {
    FontId::proportional(12.0)
}
/// `ui-label`: uppercase column titles (use with [`LABEL_TRACKING`]).
pub fn ui_label() -> FontId {
    FontId::new(11.0, family(SANS_SEMIBOLD))
}
/// Letter spacing of `ui-label` (0.06em at 11pt).
pub const LABEL_TRACKING: f32 = 0.66;
/// `meta`: counts, file names, kbd hints, status bar.
pub fn meta() -> FontId {
    FontId::new(11.0, family(MONO_MEDIUM))
}
/// `read-h1`: the issue title in the reader.
pub fn read_h1() -> FontId {
    FontId::new(20.0, family(SANS_SEMIBOLD))
}
/// `read-body`: the issue's prose in the reader.
pub fn read_body() -> FontId {
    FontId::proportional(14.0)
}
/// `edit-body`: the plain-text editor.
pub fn edit_body() -> FontId {
    FontId::monospace(13.0)
}

/// Registers IBM Plex Sans and Mono, keeping egui's own fonts as fallbacks for
/// glyphs Plex lacks.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 5] = [
        (
            "IBMPlexSans-Regular",
            include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
        ),
        (
            "IBMPlexSans-Medium",
            include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
        ),
        (
            "IBMPlexSans-SemiBold",
            include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
        ),
        (
            "IBMPlexMono-Regular",
            include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
        ),
        (
            "IBMPlexMono-Medium",
            include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
    }

    let proportional = fonts.families[&FontFamily::Proportional].clone();
    let monospace = fonts.families[&FontFamily::Monospace].clone();
    let with = |primary: &str, fallbacks: &[String]| {
        let mut list = vec![primary.to_string()];
        list.extend(fallbacks.iter().cloned());
        list
    };
    fonts.families.insert(
        FontFamily::Proportional,
        with("IBMPlexSans-Regular", &proportional),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        with("IBMPlexMono-Regular", &monospace),
    );
    fonts.families.insert(
        family(SANS_MEDIUM),
        with("IBMPlexSans-Medium", &proportional),
    );
    fonts.families.insert(
        family(SANS_SEMIBOLD),
        with("IBMPlexSans-SemiBold", &proportional),
    );
    fonts
        .families
        .insert(family(MONO_MEDIUM), with("IBMPlexMono-Medium", &monospace));
    ctx.set_fonts(fonts);
}

/// Applies the tokens to both of egui's themes; eframe follows the system
/// appearance.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    for (theme, palette) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        ctx.set_style_of(theme, style(theme, palette));
    }
}

fn style(theme: Theme, p: &Palette) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(theme, p),
        ..Default::default()
    };
    style.text_styles = [
        (TextStyle::Heading, ui_title()),
        (TextStyle::Body, ui_body()),
        (TextStyle::Button, ui_button()),
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Monospace, edit_body()),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(SPACE_3, SPACE_2);
    spacing.button_padding = Vec2::new(SPACE_3, 4.0);
    spacing.interact_size = Vec2::new(CONTROL_H, CONTROL_H);
    spacing.icon_width = 14.0;
    spacing.icon_width_inner = 8.0;
    spacing.icon_spacing = 6.0;
    spacing.window_margin = Margin::same(SPACE_4 as i8);
    spacing.menu_margin = Margin::same(SPACE_2 as i8);
    style
}

fn visuals(theme: Theme, p: &Palette) -> egui::Visuals {
    let mut v = match theme {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark => egui::Visuals::dark(),
    };
    let control = |fill: Color32, edge: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, edge),
        corner_radius: CornerRadius::same(RADIUS_SM),
        fg_stroke: Stroke::new(1.0, p.ink),
        expansion: 0.0,
    };

    v.override_text_color = None;
    v.widgets.noninteractive = egui::style::WidgetVisuals {
        fg_stroke: Stroke::new(1.0, p.ink),
        ..control(p.surface, p.line)
    };
    v.widgets.inactive = control(p.surface, p.line_strong);
    v.widgets.hovered = control(p.surface_hover, p.line_strong);
    v.widgets.active = control(p.surface_hover, p.accent);
    v.widgets.open = control(p.surface_hover, p.line_strong);

    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(2.0, p.accent);
    v.hyperlink_color = p.accent;
    v.weak_text_color = Some(p.ink_muted);
    v.faint_bg_color = p.surface_sunken;
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.code_bg_color = p.surface_sunken;
    v.warn_fg_color = p.status_doing;
    v.error_fg_color = p.danger;
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.line_strong);
    v.window_corner_radius = CornerRadius::same(RADIUS_MD);
    v.menu_corner_radius = CornerRadius::same(RADIUS_MD);
    let shadow = Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: p.popover_shadow,
    };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v
}

/// The line icons of the design system, drawn on a 14pt box with a 1.6pt
/// stroke so they need no icon font.
#[derive(Clone, Copy)]
pub enum Icon {
    Plus,
    Refresh,
    Edit,
    /// The back chevron, pointing left.
    Back,
    /// A waste basket: delete an issue for good.
    Trash,
    /// A lidded box: move a done issue to `.archive`.
    Archive,
    /// A small down chevron, marking a control that opens a menu.
    ChevronDown,
    /// A speech bubble with a question mark: ask an agent to review.
    Review,
    /// A cross: close a dialog.
    Close,
    /// A tick: the chosen option of a select menu.
    Check,
    /// A magnifier: the board filter.
    Search,
    /// Two arrows pointing outwards: expand a column to ColumnFocus.
    Expand,
    /// Two arrows pointing inwards: leave ColumnFocus for the board.
    Collapse,
    /// A tag with a small plus at the lower right: add a label.
    TagPlus,
    /// A node on a line: the git commits behind an issue.
    Commit,
}

pub const ICON_SIZE: f32 = 14.0;

pub fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    // Coordinates below are on Lucide's 24-unit grid.
    let s = rect.width() / 24.0;
    let at = |x: f32, y: f32| Pos2::new(rect.min.x + x * s, rect.min.y + y * s);
    match icon {
        Icon::Plus => {
            painter.line_segment([at(12.0, 5.0), at(12.0, 19.0)], stroke);
            painter.line_segment([at(5.0, 12.0), at(19.0, 12.0)], stroke);
        }
        Icon::Back => {
            painter.line(vec![at(15.0, 18.0), at(9.0, 12.0), at(15.0, 6.0)], stroke);
        }
        Icon::Edit => {
            painter.line(
                vec![
                    at(17.0, 3.0),
                    at(21.0, 7.0),
                    at(7.5, 20.5),
                    at(2.0, 22.0),
                    at(3.5, 16.5),
                    at(17.0, 3.0),
                ],
                stroke,
            );
        }
        Icon::Refresh => {
            // A clockwise 300° arc with an arrow head where it ends, top right.
            let center = at(12.0, 12.0);
            let radius = 8.0 * s;
            let end_angle = 300.0_f32.to_radians();
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = (i as f32 / 24.0) * end_angle;
                    center + radius * Vec2::new(t.cos(), t.sin())
                })
                .collect();
            let end = *points.last().unwrap_or(&center);
            painter.line(points, stroke);
            let direction = Vec2::new(-end_angle.sin(), end_angle.cos());
            let head = 5.0 * s;
            let barb = |angle: f32| {
                end - head * (egui::emath::Rot2::from_angle(angle.to_radians()) * direction)
            };
            painter.line(vec![barb(40.0), end, barb(-40.0)], stroke);
        }
        Icon::Trash => {
            // Lid, handle, can and the two inner strokes.
            painter.line_segment([at(3.0, 6.0), at(21.0, 6.0)], stroke);
            painter.line(
                vec![at(8.0, 6.0), at(8.0, 4.5), at(16.0, 4.5), at(16.0, 6.0)],
                stroke,
            );
            painter.line(
                vec![at(5.0, 6.0), at(5.0, 20.0), at(19.0, 20.0), at(19.0, 6.0)],
                stroke,
            );
            painter.line_segment([at(10.0, 11.0), at(10.0, 16.5)], stroke);
            painter.line_segment([at(14.0, 11.0), at(14.0, 16.5)], stroke);
        }
        Icon::Archive => {
            // A lidded box; the handle becomes a short line.
            painter.line(
                vec![
                    at(2.0, 3.0),
                    at(22.0, 3.0),
                    at(22.0, 8.0),
                    at(2.0, 8.0),
                    at(2.0, 3.0),
                ],
                stroke,
            );
            painter.line(
                vec![at(4.0, 8.0), at(4.0, 21.0), at(20.0, 21.0), at(20.0, 8.0)],
                stroke,
            );
            painter.line_segment([at(10.0, 12.0), at(14.0, 12.0)], stroke);
        }
        Icon::Close => {
            painter.line_segment([at(18.0, 6.0), at(6.0, 18.0)], stroke);
            painter.line_segment([at(6.0, 6.0), at(18.0, 18.0)], stroke);
        }
        Icon::Check => {
            painter.line(vec![at(20.0, 6.0), at(9.0, 17.0), at(4.0, 12.0)], stroke);
        }
        Icon::Search => {
            painter.circle_stroke(at(11.0, 11.0), 7.0 * s, stroke);
            painter.line_segment([at(20.0, 20.0), at(16.0, 16.0)], stroke);
        }
        Icon::ChevronDown => {
            painter.line(vec![at(6.0, 9.5), at(12.0, 15.0), at(18.0, 9.5)], stroke);
        }
        Icon::Expand => {
            // Lucide's maximize-2: two corner brackets and a diagonal each.
            painter.line(vec![at(15.0, 3.0), at(21.0, 3.0), at(21.0, 9.0)], stroke);
            painter.line(vec![at(9.0, 21.0), at(3.0, 21.0), at(3.0, 15.0)], stroke);
            painter.line_segment([at(21.0, 3.0), at(14.0, 10.0)], stroke);
            painter.line_segment([at(3.0, 21.0), at(10.0, 14.0)], stroke);
        }
        Icon::Collapse => {
            // Lucide's minimize-2: the two brackets point inwards.
            painter.line(vec![at(4.0, 14.0), at(10.0, 14.0), at(10.0, 20.0)], stroke);
            painter.line(vec![at(20.0, 10.0), at(14.0, 10.0), at(14.0, 4.0)], stroke);
            painter.line_segment([at(14.0, 10.0), at(21.0, 3.0)], stroke);
            painter.line_segment([at(3.0, 21.0), at(10.0, 14.0)], stroke);
        }
        Icon::TagPlus => {
            // Lucide's tag, shrunk to leave the lower right to a small plus.
            painter.line(
                vec![
                    at(2.5, 2.5),
                    at(11.0, 2.5),
                    at(19.0, 10.5),
                    at(10.5, 19.0),
                    at(2.5, 11.0),
                    at(2.5, 2.5),
                ],
                stroke,
            );
            painter.circle_filled(at(7.0, 7.0), 1.3 * s, color);
            painter.line_segment([at(19.0, 15.0), at(19.0, 23.0)], stroke);
            painter.line_segment([at(15.0, 19.0), at(23.0, 19.0)], stroke);
        }
        Icon::Commit => {
            // Lucide's git-commit: a node on a vertical line.
            painter.line_segment([at(12.0, 2.5), at(12.0, 8.5)], stroke);
            painter.line_segment([at(12.0, 15.5), at(12.0, 21.5)], stroke);
            painter.circle_stroke(at(12.0, 12.0), 3.5 * s, stroke);
        }
        Icon::Review => {
            // A speech bubble (Lucide's message-circle-question): a circle
            // with a tail at the lower left, and a question mark inside.
            let center = at(11.5, 11.0);
            let radius = 7.6 * s;
            let bubble: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let turn = std::f32::consts::TAU * (i as f32 / 40.0);
                    center + radius * Vec2::new(turn.cos(), turn.sin())
                })
                .collect();
            painter.line(bubble, stroke);
            painter.line(vec![at(5.4, 16.6), at(3.6, 20.6), at(8.4, 19.2)], stroke);
            painter.line(
                vec![
                    at(9.6, 9.4),
                    at(10.2, 8.2),
                    at(11.8, 7.8),
                    at(13.4, 8.6),
                    at(13.5, 10.2),
                    at(11.8, 11.3),
                    at(11.7, 12.6),
                ],
                stroke,
            );
            painter.circle_filled(at(11.7, 15.0), 1.0 * s, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{DesignSystem, Token, Tokens};
    use std::path::Path;

    /// Colour tokens with no `Palette` field: `focus` is `{accent}`, and
    /// nothing is drawn on a `review` fill.
    const UNMAPPED_COLORS: [&str; 2] = ["focus", "on-review"];

    /// Sizes the theme overrides on purpose; see [`COLUMN_MIN`].
    const OVERRIDDEN_SIZES: [&str; 1] = ["column-min"];

    fn tokens() -> Tokens {
        DesignSystem::load(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .tokens
    }

    fn find<'a>(tokens: impl IntoIterator<Item = &'a Token>, name: &str) -> &'a Token {
        tokens
            .into_iter()
            .find(|token| token.name == name)
            .unwrap_or_else(|| panic!("tokens.json has no {name}"))
    }

    fn colors(p: &Palette) -> [(&'static str, Color32); 23] {
        [
            ("bg", p.bg),
            ("surface", p.surface),
            ("surface-hover", p.surface_hover),
            ("surface-sunken", p.surface_sunken),
            ("line", p.line),
            ("line-strong", p.line_strong),
            ("ink", p.ink),
            ("ink-muted", p.ink_muted),
            ("ink-faint", p.ink_faint),
            ("accent", p.accent),
            ("accent-soft", p.accent_soft),
            ("on-accent", p.on_accent),
            ("status-todo", p.status_todo),
            ("status-ready", p.status_ready),
            ("status-doing", p.status_doing),
            ("status-done", p.status_done),
            ("review", p.review),
            ("review-soft", p.review_soft),
            ("danger", p.danger),
            ("danger-soft", p.danger_soft),
            ("on-danger", p.on_danger),
            ("scrim", p.scrim),
            ("shadow-popover", p.popover_shadow),
        ]
    }

    /// The colour in a token value: `#rrggbb`, or `rgba(…)` premultiplied
    /// the way the palette is written. A shadow's colour is its last part.
    fn color_of(value: &str) -> Color32 {
        if let Some(at) = value.find("rgba(") {
            let channels: Vec<f32> = value[at + 5..]
                .trim_end_matches(')')
                .split(',')
                .map(|part| part.trim().parse().unwrap())
                .collect();
            let alpha = channels[3];
            let premultiply = |c: f32| (c * alpha).round() as u8;
            return Color32::from_rgba_premultiplied(
                premultiply(channels[0]),
                premultiply(channels[1]),
                premultiply(channels[2]),
                (alpha * 255.0).round() as u8,
            );
        }
        let word = value.split_whitespace().last().unwrap();
        hex(u32::from_str_radix(word.trim_start_matches('#'), 16).unwrap())
    }

    /// Opaque colours match exactly; translucent ones within the rounding of
    /// a premultiplication.
    fn same(a: Color32, b: Color32) -> bool {
        if a.a() == 255 && b.a() == 255 {
            return a == b;
        }
        a.to_array()
            .iter()
            .zip(b.to_array())
            .all(|(x, y)| x.abs_diff(y) <= 1)
    }

    fn px(value: &str) -> f32 {
        value.trim().trim_end_matches("px").parse().unwrap()
    }

    #[test]
    fn the_palettes_match_the_colour_tokens() {
        let tokens = tokens();
        let all = || tokens.color.iter().chain(&tokens.shadow);
        for (theme, palette) in [("light", &LIGHT), ("dark", &DARK)] {
            for (name, color) in colors(palette) {
                let value = find(all(), name)
                    .value
                    .theme(theme)
                    .unwrap_or_else(|| panic!("{name} has no {theme} value"));
                assert!(
                    same(color_of(value), color),
                    "{theme} {name}: tokens.json says {value}, theme.rs has {color:?}"
                );
            }
        }
        let mapped = colors(&LIGHT).map(|(name, _)| name);
        for token in all() {
            let name = token.name.as_str();
            assert!(
                mapped.contains(&name) || UNMAPPED_COLORS.contains(&name),
                "{name} has no Palette field: add one, or list it in UNMAPPED_COLORS"
            );
        }
    }

    #[test]
    fn the_popover_shadow_matches_its_token() {
        let value = find(&tokens().shadow, "shadow-popover")
            .value
            .theme("light")
            .unwrap()
            .to_string();
        let lengths: Vec<f32> = value.split_whitespace().take(3).map(px).collect();
        let shadow = style(Theme::Light, &LIGHT).visuals.popup_shadow;
        assert_eq!(
            [f32::from(shadow.offset[0]), f32::from(shadow.offset[1])],
            [lengths[0], lengths[1]]
        );
        assert_eq!(f32::from(shadow.blur), lengths[2]);
    }

    #[test]
    fn spacing_radii_and_sizes_match_the_tokens() {
        let tokens = tokens();
        let all = || {
            tokens
                .spacing
                .iter()
                .chain(&tokens.radius)
                .chain(&tokens.size)
        };
        let points = [
            ("space-1", SPACE_1),
            ("space-2", SPACE_2),
            ("space-3", SPACE_3),
            ("space-4", SPACE_4),
            ("space-5", SPACE_5),
            ("space-6", SPACE_6),
            ("space-7", SPACE_7),
            ("radius-sm", f32::from(RADIUS_SM)),
            ("radius-md", f32::from(RADIUS_MD)),
            ("radius-pill", f32::from(RADIUS_PILL)),
            ("control-h", CONTROL_H),
            ("bar-h", BAR_H),
            ("statusbar-h", STATUSBAR_H),
            ("column-max", COLUMN_MAX),
            ("reader-max", READER_MAX),
        ];
        for (name, points) in points {
            let value = find(all(), name).value.theme("light").unwrap();
            assert_eq!(px(value), points, "{name}: tokens.json says {value}");
        }
        let mapped = points.map(|(name, _)| name);
        for token in all() {
            let name = token.name.as_str();
            assert!(
                mapped.contains(&name) || OVERRIDDEN_SIZES.contains(&name),
                "{name} has no theme constant: add one, or list it in OVERRIDDEN_SIZES"
            );
        }
    }

    #[test]
    fn type_styles_match_the_tokens() {
        let fonts = [
            ("ui-title", ui_title()),
            ("ui-body", ui_body()),
            ("ui-strong", ui_strong()),
            ("ui-small", ui_small()),
            ("ui-label", ui_label()),
            ("read-h1", read_h1()),
            ("read-body", read_body()),
            ("edit-body", edit_body()),
            ("meta", meta()),
        ];
        let weight = |family: &FontFamily| match family {
            FontFamily::Name(name) if name.ends_with("-semibold") => 600,
            FontFamily::Name(name) if name.ends_with("-medium") => 500,
            _ => 400,
        };
        let mono = |family: &FontFamily| match family {
            FontFamily::Monospace => true,
            FontFamily::Name(name) => name.contains("mono"),
            FontFamily::Proportional => false,
        };
        for group in tokens().type_groups {
            for style in group.styles {
                let name = style.name.as_str();
                let (_, font) = fonts
                    .iter()
                    .find(|(known, _)| *known == name)
                    .unwrap_or_else(|| panic!("{name} has no font in theme.rs"));
                assert_eq!(
                    Some(font.size),
                    style.font_size.as_deref().map(px),
                    "{name}"
                );
                assert_eq!(Some(weight(&font.family)), style.weight, "{name}");
                assert_eq!(mono(&font.family), group.family == "mono", "{name}");
                if let Some(em) = style.letter_spacing.as_deref() {
                    let em: f32 = em.trim_end_matches("em").parse().unwrap();
                    assert!((em * font.size - LABEL_TRACKING).abs() < 1e-4, "{name}");
                }
            }
        }
    }
}

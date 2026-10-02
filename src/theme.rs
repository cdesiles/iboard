//! The Roadmap Viewer design system mapped onto egui.
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
    /// Write failures, conflicting file on disk.
    pub danger: Color32,
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
    danger: hex(0xb42318),
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
    danger: hex(0xff8a7a),
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
/// `read-h1`: the ticket title in the reader.
pub fn read_h1() -> FontId {
    FontId::new(20.0, family(SANS_SEMIBOLD))
}
/// `edit-body`: the plain-text editor.
pub fn edit_body() -> FontId {
    FontId::monospace(13.0)
}

/// Registers IBM Plex Sans and Mono, keeping egui's own fonts as fallbacks for
/// glyphs Plex lacks.
pub fn install_fonts(ctx: &egui::Context) {
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
    /// A waste basket: delete a ticket for good.
    Trash,
    /// A lidded box: move a done ticket to `.archive`.
    Archive,
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
    }
}

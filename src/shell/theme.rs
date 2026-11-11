//! The iboard design system mapped onto egui.
//!
//! Tokens come from `design/tokens.json` (the brand book is `design/README.md`).
//! Sizes are points, which map 1:1 to egui points. egui has no font weights, so
//! the Medium and SemiBold faces are registered as their own named families.

mod divider;
mod icons;
mod scroll;
mod status;
mod thinking;
mod tokens;

pub use divider::{
    PaneDivider, Split, pane_divider, pane_scroll, pane_split, pane_split_id, restore_pane_split,
    split_panes, stored_pane_split,
};
pub use icons::{CARD_ACTION_STROKE, ICON_SIZE, Icon, paint_icon, paint_icon_stroked};
pub use scroll::{SCROLL_CLEAR, scroll_area, scroll_margin, scroll_rows};
pub use status::{Status, status_icon};
pub use thinking::thinking_mark;
pub use tokens::*;

use std::sync::Arc;
use std::time::Duration;

use crate::domains::preferences::labels::{LabelColor, Rgb};
use crate::general::Appearance;
use crate::kernel::labels::Hue;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, Stroke, TextStyle,
    Theme, Vec2,
};

impl Palette {
    /// The `label-*` colour of `hue`.
    pub fn label(&self, hue: Hue) -> Color32 {
        self.labels[hue as usize]
    }
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

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

/// A label colour, as egui draws it.
pub fn rgb(Rgb(r, g, b): Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The WCAG contrast ratio between two colours, 1 (identical) to 21.
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// What a label's colour is in this theme: a preset's `label-*` value, or the
/// user's own, unchanged.
pub fn label_color(color: LabelColor, p: &Palette) -> Color32 {
    match color {
        LabelColor::Preset(hue) => p.label(hue),
        LabelColor::Own(own) => rgb(own),
    }
}

/// A loud label pill's fill: `color` at `label-tint` over `surface`.
pub fn label_tint(color: Color32, p: &Palette) -> Color32 {
    p.surface.lerp_to_gamma(color, p.label_tint)
}

/// A band across a modal or a screen, like its footer: `space-5` in from the
/// sides, `space-3` above and below.
pub const BAND_PAD: Vec2 = Vec2::new(SPACE_5, SPACE_3);
/// A row's padding: `space-4` in from the sides, `space-3` above and below.
pub const ROW_PAD: Vec2 = Vec2::new(SPACE_4, SPACE_3);

/// The centres of the three traffic lights, from the window's top left.
pub fn traffic_light_centres() -> [egui::Pos2; 3] {
    [0.0, 1.0, 2.0]
        .map(|nth| egui::Pos2::new(TITLEBAR_LIGHT_X + nth * TITLEBAR_LIGHT_X, TITLEBAR_LIGHT_Y))
}

/// Under this window width the inset drops to 16.
const MODAL_NARROW_WINDOW: f32 = 720.0;
/// XL's two panes stack under this modal width.
pub const MODAL_XL_STACK_BELOW: f32 = 900.0;

/// The four Modal sizes: one question, a form, one document, a work surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalSize {
    S,
    M,
    L,
    Xl,
}

/// What a [`ModalSize`] is: its width, the height it may grow to, and where
/// it sits in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalSpec {
    pub width: f32,
    pub max_height: f32,
    pub anchor: egui::Align2,
}

impl ModalSize {
    /// S and XL are centred, M and L hang from the top, so only their bottom
    /// edge moves as their content grows. XL is always `max_height` tall.
    pub const fn spec(self) -> ModalSpec {
        let (width, max_height, anchor) = match self {
            Self::S => (MODAL_S, f32::INFINITY, egui::Align2::CENTER_CENTER),
            Self::M => (MODAL_M, f32::INFINITY, egui::Align2::CENTER_TOP),
            Self::L => (MODAL_L, f32::INFINITY, egui::Align2::CENTER_TOP),
            Self::Xl => (MODAL_XL, MODAL_XL_H, egui::Align2::CENTER_CENTER),
        };
        ModalSpec {
            width,
            max_height,
            anchor,
        }
    }
}

/// The gap every modal keeps from the edges of `window`.
pub fn modal_inset(window: Rect) -> f32 {
    if window.width() < MODAL_NARROW_WINDOW {
        16.0
    } else {
        MODAL_INSET
    }
}

/// The largest frame a modal of `size` takes in `window`: its width, and
/// the height it may grow to (S, M and L fit their content up to it; XL is
/// always that tall).
pub fn modal_size(size: ModalSize, window: Rect) -> Vec2 {
    let inset = modal_inset(window);
    let room = Vec2::new(
        (window.width() - 2.0 * inset).max(0.0),
        (window.height() - 2.0 * inset).max(0.0),
    );
    let spec = size.spec();
    Vec2::new(spec.width.min(room.x), spec.max_height.min(room.y))
}

/// Where a modal of `size` sits: a top-anchored one 12% from the top.
pub fn modal_area(id: egui::Id, size: ModalSize, window: Rect, rise: f32) -> egui::Area {
    let anchor = size.spec().anchor;
    let top = match anchor.y() {
        egui::Align::Min => (window.height() * 0.12).max(modal_inset(window)),
        _ => 0.0,
    };
    egui::Modal::default_area(id).anchor(anchor, Vec2::new(0.0, top + rise))
}

// Font families beyond egui's two built-in ones.
const SANS_MEDIUM: &str = "inter-medium";
const SANS_SEMIBOLD: &str = "inter-semibold";
const MONO_MEDIUM: &str = "jetbrains-mono-medium";

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

/// Button labels: `ui-strong` at 12.
pub fn ui_button() -> FontId {
    FontId::new(12.0, family(SANS_MEDIUM))
}
/// A release's name on the Trends: 11pt Inter 500.
pub fn release_name() -> FontId {
    FontId::new(11.0, family(SANS_MEDIUM))
}

/// Inter and JetBrains Mono, with egui's own fonts as fallbacks.
fn font_definitions() -> FontDefinitions {
    use FontFamily::{Monospace, Proportional};
    let faces: [(FontFamily, &str, &'static [u8], FontFamily); 5] = [
        (
            Proportional,
            "Inter-Regular",
            include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
            Proportional,
        ),
        (
            family(SANS_MEDIUM),
            "Inter-Medium",
            include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
            Proportional,
        ),
        (
            family(SANS_SEMIBOLD),
            "Inter-SemiBold",
            include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
            Proportional,
        ),
        (
            Monospace,
            "JetBrainsMono-Regular",
            include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
            Monospace,
        ),
        (
            family(MONO_MEDIUM),
            "JetBrainsMono-Medium",
            include_bytes!("../../assets/fonts/JetBrainsMono-Medium.ttf"),
            Monospace,
        ),
    ];
    let mut fonts = FontDefinitions::default();
    let defaults = fonts.families.clone();
    for (family, face, bytes, fallback) in faces {
        fonts
            .font_data
            .insert(face.to_string(), Arc::new(FontData::from_static(bytes)));
        let list = std::iter::once(face.to_string())
            .chain(defaults[&fallback].iter().cloned())
            .collect();
        fonts.families.insert(family, list);
    }
    fonts
}

/// Applies the tokens to both of egui's themes, and draws in the light one
/// until [`apply`] says otherwise.
/// How far the pointer moves with the button down before a press is a drag.
pub const DRAG_START: f32 = 4.0;

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    for (theme, palette) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        ctx.set_style_of(theme, style(theme, palette));
    }
    apply(ctx, Appearance::default());
    set_reduced_motion(ctx, system_reduces_motion());
    ctx.options_mut(|options| options.input_options.max_click_dist = DRAG_START);
}

/// Whether macOS asks for reduced motion (Accessibility › Display).
pub fn system_reduces_motion() -> bool {
    #[cfg(target_os = "macos")]
    return objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
    #[cfg(not(target_os = "macos"))]
    false
}

fn reduced_motion_id() -> egui::Id {
    egui::Id::unique("theme-reduced-motion")
}

/// With `reduce`, every animation ends at once: [`motion`] gives zero
/// seconds, and egui's own transitions (collapsing, scrolling) are skipped.
pub fn set_reduced_motion(ctx: &egui::Context, reduce: bool) {
    ctx.data_mut(|d| d.insert_temp(reduced_motion_id(), reduce));
    let time = if reduce {
        0.0
    } else {
        egui::Style::default().animation_time
    };
    ctx.all_styles_mut(|style| style.animation_time = time);
}

/// Whether animations are off for this window.
pub fn reduced_motion(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(reduced_motion_id()).unwrap_or(false))
}

/// How long an animation of `secs` lasts here: none with reduced motion.
pub fn motion(ctx: &egui::Context, secs: f32) -> f32 {
    if reduced_motion(ctx) { 0.0 } else { secs }
}

/// Nothing of a Skeleton shows before this, so a fast answer never flashes.
pub const SKELETON_AFTER: Duration = Duration::from_millis(300);
/// From this long on, a wait shows the Thinking mark's elapsed clock.
pub const CLOCK_AFTER: Duration = Duration::from_secs(2);

/// A Skeleton placeholder's colour: `surface-sunken`, every one on screen
/// pulsing together with the Thinking mark (opacity 1 → 0.5 → 1 over 1.6s).
pub fn skeleton_fill(ctx: &egui::Context, p: &Palette) -> Color32 {
    if reduced_motion(ctx) {
        return p.surface_sunken;
    }
    ctx.request_repaint();
    let phase = (ctx.input(|i| i.time) % 1.6 / 1.6) as f32;
    let opacity = 0.75 + 0.25 * (phase * std::f32::consts::TAU).cos();
    p.surface.lerp_to_gamma(p.surface_sunken, opacity)
}

/// A day of the Agenda: `surface-sunken` without commits, else one of four
/// steps of `accent` mixed into `surface`, each `step` commits wide.
pub fn agenda_fill(p: &Palette, commits: usize, step: usize) -> Color32 {
    const MIX: [f32; 4] = [0.30, 0.52, 0.76, 1.0];
    match commits.div_ceil(step.max(1)).min(MIX.len()) {
        0 => p.surface_sunken,
        level => p.surface.lerp_to_gamma(p.accent, MIX[level - 1]),
    }
}

/// Draws in the chosen theme from the next frame on.
pub fn apply(ctx: &egui::Context, appearance: Appearance) {
    ctx.set_theme(match appearance {
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::Dark => egui::ThemePreference::Dark,
        Appearance::System => egui::ThemePreference::System,
    });
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
    spacing.window_margin = SPACE_4.into();
    spacing.menu_margin = SPACE_2.into();
    spacing.scroll = scroll::style();
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
    v.selection.stroke = Stroke::new(1.0, p.accent);
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
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(RADIUS_MD);
    v.menu_corner_radius = CornerRadius::same(RADIUS_MD);
    let shadow = popover_shadow(p);
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use crate::design::DesignSystem;

    #[test]
    fn the_bundled_faces_draw_the_key_symbols_themselves() {
        let ctx = egui::Context::default();
        let mut fonts = font_definitions();
        // `has_glyph` only sees a miss when another face owns the replacement
        // glyph ◻: Noto Emoji does, and has none of these keys but ↩.
        for (family, face) in [
            (FontFamily::Proportional, "Inter-Regular"),
            (FontFamily::Monospace, "JetBrainsMono-Regular"),
        ] {
            let faces = vec![face.to_string(), "NotoEmoji-Regular".to_string()];
            fonts.families.insert(family, faces);
        }
        ctx.set_fonts(fonts);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        for font in [FontId::proportional(12.0), FontId::monospace(11.0)] {
            for key in ['⌘', '⌥', '⇧', '⌃', '↩', '⌫', '⇥', '←', '↑', '→', '↓']
            {
                let shown = ctx.fonts_mut(|fonts| fonts.has_glyph(&font, key));
                assert!(shown, "{key} in {font:?}");
            }
        }
        let eject = ctx.fonts_mut(|fonts| fonts.has_glyph(&FontId::monospace(11.0), '⏏'));
        assert!(!eject, "JetBrains Mono has no ⏏, so the check can fail");
    }

    #[test]
    fn agenda_days_take_four_steps_of_accent() {
        let p = &LIGHT;
        assert_eq!(agenda_fill(p, 0, 3), p.surface_sunken);
        assert_eq!(agenda_fill(p, 12, 3), p.accent);
        assert_eq!(agenda_fill(p, 40, 3), p.accent);
        assert_eq!(agenda_fill(p, 1, 3), agenda_fill(p, 3, 3));
        assert_ne!(agenda_fill(p, 3, 3), agenda_fill(p, 4, 3));
    }

    #[test]
    fn the_theme_tokens_are_a_fresh_generation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let system = DesignSystem::load(&root.join("design")).expect("design/ loads");
        let fresh = crate::design::rust_tokens::theme_tokens(&system).expect("tokens generate");
        let path = root.join("src/shell/theme/tokens.rs");
        let committed = std::fs::read_to_string(&path).expect("src/shell/theme/tokens.rs exists");
        assert_eq!(
            committed, fresh,
            "src/shell/theme/tokens.rs is stale: run `make tokens`"
        );
    }

    #[test]
    fn a_loud_label_pill_stays_readable() {
        let (ink, surface) = (LIGHT.label(Hue::Red), LIGHT.surface);
        let fill = label_tint(ink, &LIGHT);
        assert_eq!(fill, surface.lerp_to_gamma(ink, LIGHT.label_tint));
        assert!(
            contrast_ratio(ink, fill) >= 4.5,
            "a loud pill stays readable"
        );
    }

    #[test]
    fn reduced_motion_ends_every_animation_at_once() {
        let slide = |reduce: bool| {
            let ctx = egui::Context::default();
            install(&ctx);
            set_reduced_motion(&ctx, reduce);
            let id = egui::Id::unique("slide");
            let mut value = 0.0;
            for target in [0.0, 1.0] {
                let raw = egui::RawInput::default();
                ctx.run_ui(raw, |ui| {
                    value = ui
                        .ctx()
                        .animate_value_with_time(id, target, motion(ui.ctx(), 0.5));
                })
                .textures_delta
                .clear();
            }
            (value, ctx.global_style().animation_time)
        };
        let (value, egui_time) = slide(true);
        assert_eq!(value, 1.0, "reduced: the change lands in one frame");
        assert_eq!(egui_time, 0.0, "and egui's own transitions are off");
        let (value, egui_time) = slide(false);
        assert!(value < 1.0, "otherwise it eases in");
        assert!(egui_time > 0.0);
    }
}

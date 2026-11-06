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

pub use divider::{
    PaneDivider, pane_divider, pane_split, pane_split_id, restore_pane_split, stored_pane_split,
};
pub use icons::{CARD_ACTION_STROKE, ICON_SIZE, Icon, paint_icon, paint_icon_stroked};
pub use scroll::{SCROLL_CLEAR, scroll_area, scroll_margin, scroll_rows};
pub use status::{Status, status_icon};
pub use thinking::thinking_mark;

use std::sync::Arc;
use std::time::Duration;

use crate::general::Appearance;
use crate::labels::{Hue, LabelColor, Rgb};

use egui::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, Stroke, TextStyle,
    Theme, Vec2,
};

/// One theme's color tokens.
pub struct Palette {
    /// App ground: board, header, status bar.
    pub bg: Color32,
    /// Cards, reader page, editor, inputs, menus and modals.
    pub surface: Color32,
    /// Hover fill for edgeless buttons, crumbs, menu rows and card actions.
    pub surface_hover: Color32,
    /// Inline code, key caps, neutral label pills, empty-column well.
    pub surface_sunken: Color32,
    /// Dividers only: bar rules, modal rules and frames.
    pub line: Color32,
    /// Card edges at rest, the hairline of secondary buttons in modal feet.
    pub line_soft: Color32,
    /// Only edges that identify a control, and a card under the pointer.
    pub line_strong: Color32,
    /// Scrollbar thumb while scrolling.
    pub scrollbar: Color32,
    /// Scrollbar thumb under the pointer.
    pub scrollbar_active: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub ink_faint: Color32,
    /// Selected, focused or a link, nothing else. Not a button fill.
    pub accent: Color32,
    /// The soft 3pt glow around a focused text field, outside its border.
    pub focus_halo: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    /// The one filled button per screen.
    pub primary: Color32,
    pub primary_hover: Color32,
    /// Label on `primary`.
    pub on_primary: Color32,
    pub status_todo: Color32,
    pub status_ready: Color32,
    pub status_doing: Color32,
    pub status_done: Color32,
    /// The AI review button and the Challenge verdict. Reserved: nothing else
    /// uses this hue.
    pub review: Color32,
    /// Review panel header fill and the pending-review card flag.
    pub review_soft: Color32,
    /// Label on the `review`-filled Send of the refinement.
    pub on_review: Color32,
    /// Write failures, conflicting file on disk, the Delete drop zone.
    pub danger: Color32,
    /// The Delete drop zone while a card is over it.
    pub danger_soft: Color32,
    /// Label on the `danger`-filled Delete button.
    pub on_danger: Color32,
    /// Dims the board behind a modal confirmation.
    pub scrim: Color32,
    pub popover_shadow: Color32,
    /// Under a modal, above the scrim.
    pub modal_shadow: Color32,
    pub label_red: Color32,
    pub label_orange: Color32,
    pub label_ochre: Color32,
    pub label_green: Color32,
    pub label_teal: Color32,
    pub label_pink: Color32,
    pub label_grey: Color32,
    /// `label-tint` in this theme: a loud label pill's colour over `surface`.
    pub label_tint: f32,
}

impl Palette {
    /// The `label-*` colours by hue, in token order.
    pub fn label_colors(&self) -> [(&'static str, Color32); 7] {
        [
            ("red", self.label_red),
            ("orange", self.label_orange),
            ("ochre", self.label_ochre),
            ("green", self.label_green),
            ("teal", self.label_teal),
            ("pink", self.label_pink),
            ("grey", self.label_grey),
        ]
    }
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    bg: hex(0xf4f5f7),
    surface: hex(0xffffff),
    surface_hover: hex(0xe9ebef),
    surface_sunken: hex(0xeceef2),
    line: hex(0xcfd4db),
    line_soft: hex(0xdfe2e7),
    line_strong: hex(0x7c8491),
    scrollbar: hex(0xbfc4cb),
    scrollbar_active: hex(0xa1a7b0),
    ink: hex(0x15181d),
    ink_muted: hex(0x4d5562),
    ink_faint: hex(0x626a78),
    accent: hex(0x2b54c4),
    focus_halo: Color32::from_rgba_premultiplied(9, 17, 39, 51),
    accent_soft: hex(0xe4eafb),
    on_accent: hex(0xffffff),
    primary: hex(0x15181d),
    primary_hover: hex(0x2e333b),
    on_primary: hex(0xffffff),
    status_todo: hex(0x5e6878),
    status_ready: hex(0x0f6e78),
    status_doing: hex(0xa04f00),
    status_done: hex(0x1d7347),
    review: hex(0x7a3fd1),
    review_soft: hex(0xf1ebfc),
    on_review: hex(0xffffff),
    danger: hex(0xb42318),
    danger_soft: hex(0xfdecea),
    on_danger: hex(0xffffff),
    scrim: Color32::from_rgba_premultiplied(7, 8, 9, 82),
    popover_shadow: Color32::from_rgba_premultiplied(3, 3, 3, 31),
    modal_shadow: Color32::from_rgba_premultiplied(4, 4, 5, 46),
    label_red: hex(0xbc3228),
    label_orange: hex(0xa65400),
    label_ochre: hex(0x826600),
    label_green: hex(0x287639),
    label_teal: hex(0x0c7380),
    label_pink: hex(0xb33270),
    label_grey: hex(0x646c79),
    label_tint: 0.10,
};

pub const DARK: Palette = Palette {
    bg: hex(0x15171a),
    surface: hex(0x1d2024),
    surface_hover: hex(0x262a2f),
    surface_sunken: hex(0x111316),
    line: hex(0x353a42),
    line_soft: hex(0x2d3138),
    line_strong: hex(0x737c89),
    scrollbar: hex(0x737c89),
    scrollbar_active: hex(0xa7aeb9),
    ink: hex(0xe7e9ec),
    ink_muted: hex(0xa7aeb9),
    ink_faint: hex(0x8c939e),
    accent: hex(0x86a4ff),
    focus_halo: Color32::from_rgba_premultiplied(35, 43, 66, 66),
    accent_soft: hex(0x222c47),
    on_accent: hex(0x0b1433),
    primary: hex(0xe7e9ec),
    primary_hover: hex(0xc9cdd3),
    on_primary: hex(0x15171a),
    status_todo: hex(0xa4adba),
    status_ready: hex(0x4fc4cf),
    status_doing: hex(0xeda24a),
    status_done: hex(0x5fc98e),
    review: hex(0xb79cff),
    review_soft: hex(0x2a2245),
    on_review: hex(0x1c0f3d),
    danger: hex(0xff8a7a),
    danger_soft: hex(0x3d1a17),
    on_danger: hex(0x3a0d08),
    scrim: Color32::from_rgba_premultiplied(0, 0, 0, 140),
    popover_shadow: Color32::from_rgba_premultiplied(0, 0, 0, 128),
    modal_shadow: Color32::from_rgba_premultiplied(0, 0, 0, 153),
    label_red: hex(0xf28b7d),
    label_orange: hex(0xeda05a),
    label_ochre: hex(0xd6b44c),
    label_green: hex(0x6dca8b),
    label_teal: hex(0x5ec4cf),
    label_pink: hex(0xf08dbe),
    label_grey: hex(0x9ca3ad),
    label_tint: 0.16,
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
        LabelColor::Preset(hue) => match hue {
            Hue::Red => p.label_red,
            Hue::Ochre => p.label_ochre,
            Hue::Green => p.label_green,
            Hue::Teal => p.label_teal,
            Hue::Grey => p.label_grey,
        },
        LabelColor::Own(own) => rgb(own),
    }
}

/// A loud label pill's fill: `color` at `label-tint` over `surface`.
pub fn label_tint(color: Color32, p: &Palette) -> Color32 {
    p.surface.lerp_to_gamma(color, p.label_tint)
}

// Spacing (`space-*`), radii and fixed sizes.
pub const SPACE_1: f32 = 2.0;
pub const SPACE_2: f32 = 4.0;
pub const SPACE_3: f32 = 8.0;
pub const SPACE_4: f32 = 12.0;
pub const SPACE_5: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_7: f32 = 32.0;
/// A band across a modal or a screen, like its footer: `space-5` in from the
/// sides, `space-3` above and below.
pub const BAND_PAD: Vec2 = Vec2::new(SPACE_5, SPACE_3);
/// A row's padding: `space-4` in from the sides, `space-3` above and below.
pub const ROW_PAD: Vec2 = Vec2::new(SPACE_4, SPACE_3);
pub const RADIUS_SM: u8 = 4;
pub const RADIUS_MD: u8 = 6;
pub const RADIUS_PILL: u8 = 9;
pub const CONTROL_H: f32 = 24.0;
pub const BAR_H: f32 = 40.0;
pub const STATUSBAR_H: f32 = 24.0;
/// The column status icon's box.
pub const STATUS_ICON: f32 = 14.0;
/// Narrowest column before the board scrolls sideways: four columns fit the
/// 880pt minimum window.
pub const COLUMN_MIN: f32 = 200.0;
pub const COLUMN_MAX: f32 = 420.0;
/// Reader and editor sheet: `READ_MEASURE` plus 32pt each side.
pub const READER_MAX: f32 = 624.0;
/// The reader's text column at `read-body`: about 75 characters a line.
pub const READ_MEASURE: f32 = 560.0;
/// The top strip the traffic lights occupy; no modal frame enters it.
pub const TITLEBAR_H: f32 = 28.0;
/// The header bar's leading width that the traffic lights take.
pub const TITLEBAR_LEAD: f32 = 80.0;
/// The traffic lights' centre line, from the window's top.
pub const TITLEBAR_LIGHT_Y: f32 = 20.0;
/// The close button's centre from the window's left; the others follow every 20pt.
pub const TITLEBAR_LIGHT_X: f32 = 20.0;
/// Scrollbar thumb at rest and while scrolling.
pub const SCROLL_THUMB: f32 = 4.0;
/// Scrollbar thumb under the pointer and while dragged.
pub const SCROLL_THUMB_HOVER: f32 = 8.0;
/// The strip along a pane's edge that points at its scrollbar.
pub const SCROLL_HIT: f32 = 12.0;
pub const SCROLL_THUMB_MIN: f32 = 24.0;
/// Modal widths: S (one question), M (forms), L (one document), XL (at most).
pub const MODAL_S: f32 = 360.0;
pub const MODAL_M: f32 = 480.0;
pub const MODAL_L: f32 = 760.0;
pub const MODAL_XL: f32 = 1200.0;
pub const MODAL_XL_H: f32 = 900.0;
/// The gap a modal keeps from the window's edge.
pub const MODAL_INSET: f32 = 32.0;
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
    let width = match size {
        ModalSize::S => MODAL_S,
        ModalSize::M => MODAL_M,
        ModalSize::L => MODAL_L,
        ModalSize::Xl => MODAL_XL,
    };
    let height = match size {
        ModalSize::Xl => MODAL_XL_H,
        _ => f32::INFINITY,
    };
    Vec2::new(width.min(room.x), height.min(room.y))
}

/// Where a modal of `size` sits: S and XL centred, M and L 12% from the
/// top, so only their bottom edge moves as their content grows.
pub fn modal_area(id: egui::Id, size: ModalSize, window: Rect, rise: f32) -> egui::Area {
    let area = egui::Modal::default_area(id);
    match size {
        ModalSize::S | ModalSize::Xl => {
            area.anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, rise))
        }
        ModalSize::M | ModalSize::L => area.anchor(
            egui::Align2::CENTER_TOP,
            Vec2::new(
                0.0,
                (window.height() * 0.12).max(modal_inset(window)) + rise,
            ),
        ),
    }
}

// Font families beyond egui's two built-in ones.
const SANS_MEDIUM: &str = "inter-medium";
const SANS_SEMIBOLD: &str = "inter-semibold";
const MONO_MEDIUM: &str = "jetbrains-mono-medium";

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
/// `meta`: paths, file names, counts, dates, the status bar, modal context.
pub fn meta() -> FontId {
    FontId::proportional(12.0)
}
/// `meta-mono`: key caps only.
pub fn meta_mono() -> FontId {
    FontId::new(11.0, family(MONO_MEDIUM))
}
/// `read-h1`: the issue title in the reader.
pub fn read_h1() -> FontId {
    FontId::new(20.0, family(SANS_SEMIBOLD))
}
/// `read-h2`: `##` headings in the reader.
pub fn read_h2() -> FontId {
    FontId::new(16.0, family(SANS_SEMIBOLD))
}
/// `read-h3`: `###` headings and deeper in the reader.
pub fn read_h3() -> FontId {
    FontId::new(14.0, family(SANS_SEMIBOLD))
}
/// `read-body`: the issue's prose in the reader.
pub fn read_body() -> FontId {
    FontId::proportional(15.0)
}
/// `edit-body`: the plain-text editor.
pub fn edit_body() -> FontId {
    FontId::monospace(13.0)
}

/// Inter and JetBrains Mono, with egui's own fonts as fallbacks.
fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 5] = [
        (
            "Inter-Regular",
            include_bytes!("../assets/fonts/Inter-Regular.ttf"),
        ),
        (
            "Inter-Medium",
            include_bytes!("../assets/fonts/Inter-Medium.ttf"),
        ),
        (
            "Inter-SemiBold",
            include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
        ),
        (
            "JetBrainsMono-Regular",
            include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        ),
        (
            "JetBrainsMono-Medium",
            include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
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
        with("Inter-Regular", &proportional),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        with("JetBrainsMono-Regular", &monospace),
    );
    fonts
        .families
        .insert(family(SANS_MEDIUM), with("Inter-Medium", &proportional));
    fonts
        .families
        .insert(family(SANS_SEMIBOLD), with("Inter-SemiBold", &proportional));
    fonts.families.insert(
        family(MONO_MEDIUM),
        with("JetBrainsMono-Medium", &monospace),
    );
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
    egui::Id::new("theme-reduced-motion")
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

// The `motion` tokens, in seconds. Pass an animation's through [`motion`].
/// Hover fills fading in; fading out is instant.
pub const MOTION_HOVER: f32 = 0.1;
/// Key hints sliding out beside their control, and back.
pub const MOTION_HINT: f32 = 0.12;
/// How long ⌘ alone is held before key hints show.
pub const HINT_HOLD: f32 = 0.5;
/// How long a scrollbar stays after the last movement or the pointer leaving.
pub const SCROLL_LINGER: f32 = 0.8;
/// A scrollbar thumb fading in and out.
pub const SCROLL_FADE: f32 = 0.15;

/// Nothing of a Skeleton shows before this, so a fast answer never flashes.
pub const SKELETON_AFTER: Duration = Duration::from_millis(300);

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::design::{DesignSystem, Token, Tokens};
    use std::path::Path;

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

    /// Colour tokens with no `Palette` field: `focus` is `{accent}`.
    const UNMAPPED_COLORS: [&str; 1] = ["focus"];

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

    fn colors(p: &Palette) -> Vec<(String, Color32)> {
        let named = [
            ("bg", p.bg),
            ("surface", p.surface),
            ("surface-hover", p.surface_hover),
            ("surface-sunken", p.surface_sunken),
            ("line", p.line),
            ("line-soft", p.line_soft),
            ("line-strong", p.line_strong),
            ("scrollbar", p.scrollbar),
            ("scrollbar-active", p.scrollbar_active),
            ("ink", p.ink),
            ("ink-muted", p.ink_muted),
            ("ink-faint", p.ink_faint),
            ("accent", p.accent),
            ("focus-halo", p.focus_halo),
            ("accent-soft", p.accent_soft),
            ("on-accent", p.on_accent),
            ("primary", p.primary),
            ("primary-hover", p.primary_hover),
            ("on-primary", p.on_primary),
            ("status-todo", p.status_todo),
            ("status-ready", p.status_ready),
            ("status-doing", p.status_doing),
            ("status-done", p.status_done),
            ("review", p.review),
            ("review-soft", p.review_soft),
            ("on-review", p.on_review),
            ("danger", p.danger),
            ("danger-soft", p.danger_soft),
            ("on-danger", p.on_danger),
            ("scrim", p.scrim),
            ("shadow-popover", p.popover_shadow),
            ("shadow-modal", p.modal_shadow),
        ];
        let labels = p
            .label_colors()
            .map(|(hue, color)| (format!("label-{hue}"), color));
        named
            .map(|(name, color)| (name.to_string(), color))
            .into_iter()
            .chain(labels)
            .collect()
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
                let token = find(all(), &name);
                let value = token
                    .value
                    .theme(theme)
                    .unwrap_or_else(|| panic!("{name} has no {theme} value"));
                let value = match value.strip_prefix('{') {
                    Some(reference) => find(all(), reference.trim_end_matches('}'))
                        .value
                        .theme(theme)
                        .unwrap(),
                    None => value,
                };
                assert!(
                    same(color_of(value), color),
                    "{theme} {name}: tokens.json says {value}, theme.rs has {color:?}"
                );
            }
        }
        let mapped = colors(&LIGHT);
        for token in all() {
            let name = token.name.as_str();
            assert!(
                mapped.iter().any(|(known, _)| known == name) || UNMAPPED_COLORS.contains(&name),
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
            ("status-icon", STATUS_ICON),
            ("column-min", COLUMN_MIN),
            ("column-max", COLUMN_MAX),
            ("reader-max", READER_MAX),
            ("read-measure", READ_MEASURE),
            ("titlebar-h", TITLEBAR_H),
            ("titlebar-lead", TITLEBAR_LEAD),
            ("titlebar-light-y", TITLEBAR_LIGHT_Y),
            ("titlebar-light-x", TITLEBAR_LIGHT_X),
            ("scroll-thumb", SCROLL_THUMB),
            ("scroll-thumb-hover", SCROLL_THUMB_HOVER),
            ("scroll-hit", SCROLL_HIT),
            ("scroll-thumb-min", SCROLL_THUMB_MIN),
            ("modal-s", MODAL_S),
            ("modal-m", MODAL_M),
            ("modal-l", MODAL_L),
            ("modal-xl", MODAL_XL),
            ("modal-xl-h", MODAL_XL_H),
            ("modal-inset", MODAL_INSET),
        ];
        for (name, points) in points {
            let value = find(all(), name).value.theme("light").unwrap();
            assert_eq!(px(value), points, "{name}: tokens.json says {value}");
        }
        let mapped = points.map(|(name, _)| name);
        for token in all() {
            let name = token.name.as_str();
            assert!(mapped.contains(&name), "{name} has no theme constant");
        }
    }

    #[test]
    fn motion_matches_its_tokens_in_seconds() {
        let tokens = tokens();
        let known = [
            ("motion-hover", MOTION_HOVER),
            ("motion-hint", MOTION_HINT),
            ("hint-hold", HINT_HOLD),
            ("scroll-linger", SCROLL_LINGER),
            ("scroll-fade", SCROLL_FADE),
        ];
        assert_eq!(tokens.motion.len(), known.len());
        for (name, secs) in known {
            let value = find(&tokens.motion, name).value.theme("light").unwrap();
            let millis: f32 = value.trim_end_matches("ms").parse().unwrap();
            assert_eq!(millis / 1000.0, secs, "{name}: tokens.json says {value}");
        }
    }

    #[test]
    fn label_tints_match_their_tokens_over_surface() {
        let tokens = tokens();
        for (name, palette) in [("label-tint", &LIGHT), ("label-tint-dark", &DARK)] {
            let value = find(&tokens.opacity, name).value.theme("light").unwrap();
            assert_eq!(value.parse::<f32>().unwrap(), palette.label_tint, "{name}");
        }
        assert_eq!(tokens.opacity.len(), 2);
        let (ink, surface) = (LIGHT.label_red, LIGHT.surface);
        let fill = label_tint(ink, &LIGHT);
        assert_eq!(fill, surface.lerp_to_gamma(ink, 0.10));
        assert!(
            contrast_ratio(ink, fill) >= 4.5,
            "a loud pill stays readable"
        );
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
            ("read-h2", read_h2()),
            ("read-h3", read_h3()),
            ("read-body", read_body()),
            ("edit-body", edit_body()),
            ("meta", meta()),
            ("meta-mono", meta_mono()),
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

    #[test]
    fn reduced_motion_ends_every_animation_at_once() {
        let slide = |reduce: bool| {
            let ctx = egui::Context::default();
            install(&ctx);
            set_reduced_motion(&ctx, reduce);
            let id = egui::Id::new("slide");
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

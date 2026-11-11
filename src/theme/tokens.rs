//! Generated from `design/tokens.json`; run `make tokens`, don't edit by hand.

use egui::epaint::Shadow;
use egui::{Color32, FontId};

use super::{MONO_MEDIUM, SANS_MEDIUM, SANS_SEMIBOLD, family, hex};
use crate::kernel::labels::Hue;

/// One theme's color tokens.
pub struct Palette {
    /// App ground: board, header, status bar.
    pub bg: Color32,
    /// Cards, reader page, editor, inputs, menus and modals.
    pub surface: Color32,
    /// Hover fill for edgeless buttons, crumbs, menu rows and card actions, on bg or surface (light: 1.11:1 on bg, 1.22:1 on surface; dark: 1.24:1 on bg, 1.13:1 on surface). Fades in over motion-hover.
    pub surface_hover: Color32,
    /// Inline code, key caps, neutral label pills, the empty-column well, editor gutter.
    pub surface_sunken: Color32,
    /// Dividers only: header and status bar rules, modal head and foot rules, the sheet's rules, a modal frame's edge. Decorative, never the edge of a control or a card.
    pub line: Color32,
    /// Card edges at rest; hairline of secondary buttons in modal feet and dialogs. Decorative.
    pub line_soft: Color32,
    /// Only edges that identify a control: text fields, Selects, checkboxes, radios, a ghost tile's dashed edge; plus, under the pointer, a card, an Agenda day, the Reader's edit outline and a pane divider. 3.2:1 or better on every surface in both themes (WCAG 1.4.11). Text buttons, labels and key caps have no line-strong edge.
    pub line_strong: Color32,
    /// Scrollbar thumb while scrolling. Hidden at rest: no thumb until the pane scrolls or the pointer reaches its edge. Light 1.75:1 on surface, dark 3.9:1.
    pub scrollbar: Color32,
    /// Scrollbar thumb under the pointer (8pt). Light 2.4:1 on surface; dark matches ink-muted.
    pub scrollbar_active: Color32,
    /// Primary text on bg, surface, surface-hover, surface-sunken.
    pub ink: Color32,
    /// Secondary text: summaries, paths, counts, dates, word counts, the status bar, modal context. At least 6.4:1 on all surfaces.
    pub ink_muted: Color32,
    /// Placeholders, separators, empty-state text, menu key caps. At least 4.5:1 on all surfaces; never smaller than 11px.
    pub ink_faint: Color32,
    /// On a control, blue means selected, focused or a link: selected card, focus ring, links, active filter, dragging scrollbar. Beyond controls, the data colour: charts, the Agenda's ramp, the build line. Not a button fill (see primary).
    pub accent: Color32,
    /// Selected card fill, text selection, active crumb.
    pub accent_soft: Color32,
    /// Text and icons on an accent fill (checked checkbox, and the primary button if primary is switched back to accent).
    pub on_accent: Color32,
    /// Fill of the one filled button per screen (New issue, Save, Add to Todo). An ink fill, so blue stays for selection and focus. Alternative kept for comparison: set to {accent} with on-primary {on-accent}.
    pub primary: Color32,
    /// Primary under the pointer.
    pub primary_hover: Color32,
    /// Label on primary.
    pub on_primary: Color32,
    /// 2px keyboard focus ring on every focusable element.
    pub focus: Color32,
    /// Soft 3px glow around a focused text field, textarea, composer or editor, outside its single focus-coloured border. Never a second ring.
    pub focus_halo: Color32,
    /// Todo column icon: an empty circle. Always shown with the word Todo.
    pub status_todo: Color32,
    /// Ready column icon: a dotted circle. Refined issues queued for an agent.
    pub status_ready: Color32,
    /// Doing column icon: a half-filled circle; Unsaved pill in the status bar.
    pub status_doing: Color32,
    /// Done column icon: a filled circle with a check; Saved pill; Pass verdict in a review.
    pub status_done: Color32,
    /// Violet. The AI split button (Ask, Update, Review, Refine) and the Challenge verdict. Reserved: nothing else uses this hue. Kept clearly apart from the blue accent and from danger red.
    pub review: Color32,
    /// Review panel header fill and the pending-review card flag.
    pub review_soft: Color32,
    /// Text on a review fill.
    pub on_review: Color32,
    /// Write failures, conflicting file on disk, Block verdict, and Delete inside the … menu and the ConfirmDialog. Never at rest in a header.
    pub danger: Color32,
    /// Delete drop zone when a card is over it. danger text reads 5.7:1 on it.
    pub danger_soft: Color32,
    /// Label on the danger-filled Delete button in the ConfirmDialog.
    pub on_danger: Color32,
    /// Dims the board behind a modal confirmation.
    pub scrim: Color32,
    /// Review panel, menus and a dragged card. Cards stay flat; modals use shadow-modal.
    pub popover_shadow: Color32,
    /// Modals only (S to XL), above the scrim. Stronger than shadow-popover because a modal floats over everything.
    pub modal_shadow: Color32,
    /// The `label-*` colours, indexed by [`Hue`].
    pub labels: [Color32; Hue::ALL.len()],
    /// A loud label pill's fill in the light theme: its label colour at this opacity over surface. Only for labels marked Stand out (bug by default).
    pub label_tint: f32,
}

/// The light palette.
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
    accent_soft: hex(0xe4eafb),
    on_accent: hex(0xffffff),
    primary: hex(0x15181d),
    primary_hover: hex(0x2e333b),
    on_primary: hex(0xffffff),
    focus: hex(0x2b54c4),
    focus_halo: Color32::from_rgba_premultiplied(9, 17, 39, 51),
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
    labels: [
        hex(0xbc3228),
        hex(0xa65400),
        hex(0x826600),
        hex(0x287639),
        hex(0x0c7380),
        hex(0xb33270),
        hex(0x646c79),
    ],
    label_tint: 0.1,
};

/// The dark palette.
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
    accent_soft: hex(0x222c47),
    on_accent: hex(0x0b1433),
    primary: hex(0xe7e9ec),
    primary_hover: hex(0xc9cdd3),
    on_primary: hex(0x15171a),
    focus: hex(0x86a4ff),
    focus_halo: Color32::from_rgba_premultiplied(35, 43, 66, 66),
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
    labels: [
        hex(0xf28b7d),
        hex(0xeda05a),
        hex(0xd6b44c),
        hex(0x6dca8b),
        hex(0x5ec4cf),
        hex(0xf08dbe),
        hex(0x9ca3ad),
    ],
    label_tint: 0.16,
};

/// Icon to label nudges, kbd padding.
pub const SPACE_1: f32 = 2.0;
/// Gap between labels, icon to text in pills.
pub const SPACE_2: f32 = 4.0;
/// Gap between cards, between a card's rows (labels, title, summary), between header controls.
pub const SPACE_3: f32 = 8.0;
/// Card horizontal padding, review row padding.
pub const SPACE_4: f32 = 12.0;
/// Window gutter, column gap.
pub const SPACE_5: f32 = 16.0;
/// Reader page vertical padding.
pub const SPACE_6: f32 = 24.0;
/// Reader page horizontal padding.
pub const SPACE_7: f32 = 32.0;

/// Buttons, inputs, kbd, crumbs.
pub const RADIUS_SM: u8 = 4;
/// Cards, reader page, editor, review panel.
pub const RADIUS_MD: u8 = 6;
/// 18px-tall pills (Saved, Unsaved, verdicts).
pub const RADIUS_PILL: u8 = 9;

/// Buttons, filter input, checkbox row.
pub const CONTROL_H: f32 = 24.0;
/// Column status icon: 14pt box, 1.5pt stroke, in the status colour.
pub const STATUS_ICON: f32 = 14.0;
/// Header bar. The traffic lights sit on its centre line (titlebar-light-y).
pub const BAR_H: f32 = 40.0;
/// Bottom status bar.
pub const STATUSBAR_H: f32 = 24.0;
/// Narrowest column before the board scrolls sideways: four columns fit the 880pt minimum window.
pub const COLUMN_MIN: f32 = 200.0;
/// Widest column; extra width stays empty on the right.
pub const COLUMN_MAX: f32 = 420.0;
/// Reader and editor sheet width: read-measure plus 32pt padding each side.
pub const READER_MAX: f32 = 624.0;
/// Text column of the reader at read-body: about 75 characters per line (measured 70 to 79 on prose).
pub const READ_MEASURE: f32 = 560.0;
/// Scrollbar thumb width at rest and while scrolling. Overlay: takes no layout width.
pub const SCROLL_THUMB: f32 = 4.0;
/// Scrollbar thumb width under the pointer and while dragging.
pub const SCROLL_THUMB_HOVER: f32 = 8.0;
/// Pointer strip along the pane's edge, wider than the thumb so a trackpad finds it. The pointer entering it reveals a hidden thumb.
pub const SCROLL_HIT: f32 = 12.0;
/// Shortest thumb on a very long page, the same as control-h.
pub const SCROLL_THUMB_MIN: f32 = 24.0;
/// Modal S: one question, one action (ConfirmDialog).
pub const MODAL_S: f32 = 360.0;
/// Modal M: forms and settings (Settings, GroupsDialog).
pub const MODAL_M: f32 = 480.0;
/// Modal L: one document, a 720pt sheet with 20pt each side.
pub const MODAL_L: f32 = 760.0;
/// Modal XL maximum width; otherwise the window minus modal-inset each side (NewIssueModal, RefinePanel).
pub const MODAL_XL: f32 = 1200.0;
/// Modal XL maximum height; XL is always this tall or the window minus the inset, so nothing jumps while content streams in.
pub const MODAL_XL_H: f32 = 900.0;
/// Gap kept between the window and a modal: 16 at the sides and bottom under 720pt of window width, but always 32 at the top, below the traffic lights (titlebar-h).
pub const MODAL_INSET: f32 = 32.0;
/// Top strip the traffic lights occupy (they span 14 to 26 once centred on the bar). No modal frame enters it; the top modal inset never drops below modal-inset.
pub const TITLEBAR_H: f32 = 28.0;
/// Leading width the traffic lights take in the header bar (three 12pt lights, centres 20, 40, 60). Nothing is drawn or clickable there; the project name or the Board button starts after it.
pub const TITLEBAR_LEAD: f32 = 80.0;
/// Centre line of the traffic lights, from the window's top: bar-h / 2. The header's controls and crumbs centre on the same line.
pub const TITLEBAR_LIGHT_Y: f32 = 20.0;
/// Centre of the close button from the window's left edge; the next two follow every 20pt.
pub const TITLEBAR_LIGHT_X: f32 = 20.0;

/// Hover fills fading in (buttons, crumbs, menu rows, card actions, card edge). Fading out is instant. Keyboard selection never animates.
pub const MOTION_HOVER: f32 = 0.1;
/// Key hints sliding out beside their control when ⌘ is held, and back.
pub const MOTION_HINT: f32 = 0.12;
/// How long ⌘ alone must be held before key hints show. Any other key, or letting go, cancels and hides them.
pub const HINT_HOLD: f32 = 0.5;
/// How long a scrollbar stays after the last scroll movement, or after the pointer leaves the edge strip.
pub const SCROLL_LINGER: f32 = 0.8;
/// Scrollbar thumb fading in and out.
pub const SCROLL_FADE: f32 = 0.15;

/// Project name in the header, modal titles.
pub fn ui_title() -> FontId {
    FontId::new(15.0, family(SANS_SEMIBOLD))
}
/// Default UI text, inputs, card titles (at 500).
pub fn ui_body() -> FontId {
    FontId::proportional(13.0)
}
/// Card title, column titles (title case), current crumb, button labels at 12px.
pub fn ui_strong() -> FontId {
    FontId::new(13.0, family(SANS_MEDIUM))
}
/// Card summary, review findings.
pub fn ui_small() -> FontId {
    FontId::proportional(12.0)
}
/// Uppercase section labels (Not written yet). Column titles use ui-strong in title case.
pub fn ui_label() -> FontId {
    FontId::new(11.0, family(SANS_SEMIBOLD))
}
/// Letter spacing of `ui-label` (0.06em at 11pt).
pub const LABEL_TRACKING: f32 = 0.66;
/// Inter, ink-muted: paths, file names, counts, dates, word counts, the status bar, modal context (in todo/, named in the background, Saved for this session).
pub fn meta() -> FontId {
    FontId::proportional(12.0)
}
/// Issue title in the reader.
pub fn read_h1() -> FontId {
    FontId::new(20.0, family(SANS_SEMIBOLD))
}
/// ## headings in the reader. 24pt above, 6pt below.
pub fn read_h2() -> FontId {
    FontId::new(16.0, family(SANS_SEMIBOLD))
}
/// ### headings and deeper in the reader. 16pt above, 4pt below.
pub fn read_h3() -> FontId {
    FontId::new(14.0, family(SANS_SEMIBOLD))
}
/// Rendered markdown paragraphs and lists, in a read-measure (560pt, about 75 characters) column.
pub fn read_body() -> FontId {
    FontId::proportional(15.0)
}
/// The plain-text editor, code blocks and diffs.
pub fn edit_body() -> FontId {
    FontId::monospace(13.0)
}
/// JetBrains Mono: key caps only (Kbd, key hints in buttons, menus and tooltips).
pub fn meta_mono() -> FontId {
    FontId::new(11.0, family(MONO_MEDIUM))
}

/// Review panel, menus and a dragged card. Cards stay flat; modals use shadow-modal.
pub fn popover_shadow(p: &Palette) -> Shadow {
    Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: p.popover_shadow,
    }
}

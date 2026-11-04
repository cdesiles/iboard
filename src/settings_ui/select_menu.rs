//! The menu under a Settings select: which one is open, its keyboard
//! highlight and the model menu's filter.

use egui::{Id, Key, Margin, Modifiers, Vec2};

use crate::theme;
use crate::widgets;

/// Which select's menu is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Menu {
    Tool,
    Model,
}

/// The dialog's selects share one menu, so opening one closes the other.
#[derive(Default)]
pub(super) struct SelectMenu {
    open: Option<Menu>,
    /// The model menu's filter.
    pub(super) filter: String,
    /// The keyboard highlight among the open menu's options.
    pub(super) highlight: usize,
    /// Give the model filter focus on the next frame.
    focus_filter: bool,
}

impl SelectMenu {
    pub(super) fn is_open(&self, menu: Menu) -> bool {
        self.open == Some(menu)
    }

    pub(super) fn is_closed(&self) -> bool {
        self.open.is_none()
    }

    pub(super) fn open(&mut self, menu: Menu) {
        self.open = Some(menu);
        self.filter.clear();
        self.focus_filter = menu == Menu::Model;
        self.highlight = 0;
    }

    pub(super) fn close(&mut self) {
        self.open = None;
    }

    /// Opens or closes `menu` from its select: a click, or `space` / `↓` while
    /// the select has focus.
    pub(super) fn toggle_on(&mut self, response: &egui::Response, menu: Menu) {
        let keyed = response.has_focus()
            && self.open.is_none()
            && response.ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Space)
                    || i.consume_key(Modifiers::NONE, Key::ArrowDown)
            });
        if response.clicked() || keyed {
            if self.open == Some(menu) {
                self.open = None;
            } else {
                self.open(menu);
            }
        }
    }

    /// Moves the highlight among `count` options with `↑ ↓`; returns the one
    /// `↩` picked. `esc` closes the menu and gives focus back to the dialog.
    pub(super) fn keyboard(&mut self, ui: &egui::Ui, count: usize) -> Option<usize> {
        let (up, down, enter, escape) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            self.open = None;
            return None;
        }
        if count == 0 {
            return None;
        }
        if down {
            self.highlight = (self.highlight + 1).min(count - 1);
        }
        if up {
            self.highlight = self.highlight.saturating_sub(1);
        }
        self.highlight = self.highlight.min(count - 1);
        enter.then_some(self.highlight)
    }

    /// The filter at the top of the menu: it takes focus as the menu opens,
    /// and typing brings the highlight back to the first option.
    pub(super) fn filter_field(&mut self, ui: &mut egui::Ui, id: Id, hint: &str) {
        let filter = widgets::text_entry(
            ui,
            widgets::field_edit(ui, &mut self.filter, hint, theme::ui_body())
                .id(id)
                .margin(Margin::symmetric(theme::SPACE_3 as i8, 3))
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.focus_filter) {
            filter.request_focus();
        }
        if filter.changed() {
            self.highlight = 0;
        }
    }

    /// Shows `menu` under its select, as wide as it, and closes it on a click
    /// elsewhere.
    pub(super) fn popup(
        &mut self,
        ui: &egui::Ui,
        anchor: &egui::Response,
        menu: Menu,
        content: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        if self.open != Some(menu) {
            return;
        }
        let mut open = true;
        egui::Popup::from_response(anchor)
            .id(Id::new(("review-menu", menu as u8)))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(theme::SPACE_2)
            .width(anchor.rect.width())
            .frame(widgets::popover_frame(ui).inner_margin(Margin::same(theme::SPACE_2 as i8)))
            .show(|ui| {
                ui.set_width(anchor.rect.width() - 2.0 * theme::SPACE_2);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                content(self, ui);
            });
        // A click on the select itself is handled by `toggle_on`.
        if !open && !anchor.clicked() {
            self.open = None;
        }
    }
}

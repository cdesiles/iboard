//! What each frame does before drawing: background work, the window's
//! requests, the dialogs' keys, the autosave and the rescan.

use super::editor::EditorState;
use super::release::ReleaseChoice;
use super::{Confirm, IboardApp, Instant, Key, RESCAN_EVERY, Vec2, quitting, settings_ui, widgets};

/// How far two fingers swipe right, in points, to go back.
const SWIPE_BACK: f32 = 120.0;

impl IboardApp {
    /// The work each frame does before drawing: what runs in the background,
    /// menu requests, keys and shortcuts, the autosave and the rescan. Tests
    /// drive it with no window.
    pub(super) fn frame_logic(&mut self, ctx: &egui::Context) {
        // A project switch sets the title; apply it now that we have a context.
        if let Some(title) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        self.forget_undo_on_action(ctx);
        self.poll_jobs(ctx);
        self.window_requests(ctx);
        self.note_stray_press(ctx);
        let confirming = self.confirm_keys(ctx);
        if self.settings.is_none() {
            self.watch_config();
        }
        let welcoming = self.welcoming();
        if !confirming && !self.modal_keys(ctx) && !welcoming {
            self.screen_keys(ctx);
        }
        self.autosave();
        if !welcoming {
            self.rescan(ctx);
        }
        self.remember_splits(ctx);
    }

    /// Adopts the background work's answers as soon as they land. Each poll
    /// says how soon to look again while its work runs: the frame wakes up at
    /// the soonest.
    fn poll_jobs(&mut self, ctx: &egui::Context) {
        let wake = [
            self.take_scan(),
            self.poll_review(),
            self.poll_refinement(),
            self.poll_split(),
            self.poll_namings(ctx),
            self.poll_draft(),
            self.poll_design_draft(),
            self.poll_enrichment(),
            self.poll_build(),
            self.poll_asks(),
            self.poll_answers(),
            self.poll_stats(),
            self.poll_plan(),
        ];
        if let Some(after) = wake.into_iter().flatten().min() {
            ctx.request_repaint_after(after);
        }
    }

    /// The dialog up owns the keyboard: Settings, which `⌘,` closes as it
    /// opened it, Groups, Commits, the project menu, Project stats, Ask AI,
    /// NewIssueModal or the refinement. True when one does.
    fn modal_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.settings.is_some() {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma))
                && self
                    .settings
                    .as_mut()
                    .is_some_and(|dialog| dialog.close() == settings_ui::Action::Close)
            {
                self.settings = None;
            }
            return true;
        }
        if self.session.release.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
                self.release_choice(ReleaseChoice::Confirm);
            }
            return true;
        }
        if self.groups_dialog.is_some() || self.session.commits.is_some() {
            return true;
        }
        if self.project_menu.is_some() {
            self.project_menu_keys(ctx);
        } else if self.session.stats.is_some() {
            self.stats_keys(ctx);
        } else if self.session.ask_draft.is_some() {
            self.ask_keys(ctx);
        } else if self.composer_open() {
            self.new_issue_keys(ctx);
            self.paste_images(ctx);
        } else if self.refinement_shown() {
            self.escape_keys(ctx);
        } else {
            return false;
        }
        true
    }

    /// Whether `pos` is on the Reader's sheet with nothing over it: no
    /// dialog, no menu, no header or status bar the page scrolled under.
    pub(super) fn on_the_sheet(&self, ctx: &egui::Context, pos: egui::Pos2) -> bool {
        self.reader.sheet_rect.contains(pos) && self.uncovered(ctx, pos)
    }

    /// Whether what is at `pos` on the Reader's page has nothing over it.
    fn uncovered(&self, ctx: &egui::Context, pos: egui::Pos2) -> bool {
        self.session.confirm.is_none()
            && self.session.label_picker.is_none()
            && !egui::Popup::is_any_open(ctx)
            && ctx.layer_id_at(pos) == Some(egui::LayerId::background())
    }

    /// Remembers a press off the sheet and the rest below it: the click that closes a menu or a
    /// dialog is not the first half of a double-click on what it uncovers.
    fn note_stray_press(&mut self, ctx: &egui::Context) {
        let press = ctx.input(|i| {
            i.pointer
                .primary_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
                .map(|pos| (pos, i.time))
        });
        if let Some((pos, time)) = press
            && !(self.reader.rest_rect.contains(pos) && self.uncovered(ctx, pos))
            && !self.on_the_sheet(ctx, pos)
        {
            self.reader.stray_press = time;
        }
    }

    /// A two-finger swipe to the right on the trackpad, as a browser's
    /// back. It reaches egui as horizontal scrolling: added up while it lasts,
    /// it goes back once past [`SWIPE_BACK`], then waits for the fingers to
    /// lift so the momentum after it never goes back twice.
    pub(super) fn swiped_back(&mut self, ctx: &egui::Context) -> bool {
        let delta = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        ..
                    } => Some(*delta),
                    _ => None,
                })
                .fold(Vec2::ZERO, |sum, delta| sum + delta)
        });
        if delta.x == 0.0 && delta.y == 0.0 {
            self.session.swipe = Some(0.0);
            return false;
        }
        let Some(swipe) = self.session.swipe.as_mut() else {
            return false;
        };
        if delta.x.abs() < 2.0 * delta.y.abs() {
            *swipe = 0.0;
            return false;
        }
        *swipe += delta.x;
        if *swipe < SWIPE_BACK {
            return false;
        }
        self.session.swipe = None;
        true
    }

    /// The window's own requests: a quit, Settings or Project Stats picked
    /// from the macOS menu, a later `iboard`'s board, and closing the window,
    /// one more way of leaving the editor.
    fn window_requests(&mut self, ctx: &egui::Context) {
        self.answer_handovers(ctx);
        // A quit from the menu arrives as a flag rather than a window close,
        // so it can run the save prompt before we exit.
        #[cfg(target_os = "macos")]
        if crate::shell::menu::take_quit_request() {
            self.request_quit();
        }
        if quitting::requested() {
            self.save_pending();
            self.quit(ctx);
        }
        #[cfg(target_os = "macos")]
        self.menu_picks(
            crate::shell::menu::take_settings_request(),
            crate::shell::menu::take_stats_request(),
            crate::shell::menu::take_plan_request(),
        );
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.request_quit();
            if !self.closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if std::mem::take(&mut self.send_close) {
            self.quit(ctx);
        }
    }

    /// Settings, Plan or Project Stats picked from the menu bar. A modal stops
    /// the board until it is answered, so a pick made under one is dropped.
    pub(super) fn menu_picks(&mut self, settings: bool, stats: bool, plan: bool) {
        if self.modal_up() {
            return;
        }
        if settings {
            self.open_settings(self.settings_tab);
        }
        if stats {
            self.open_stats();
        }
        if plan {
            self.open_plan();
        }
    }

    pub(super) fn modal_up(&self) -> bool {
        self.session.confirm.is_some()
            || self.settings.is_some()
            || self.groups_dialog.is_some()
            || self.session.commits.is_some()
            || self.session.release.is_some()
            || self.session.ask_draft.is_some()
            || self.composer_open()
            || self.refinement_shown()
    }

    /// The confirmation owns Escape and Enter while it is up, and no other
    /// key acts behind it. True while it is.
    fn confirm_keys(&mut self, ctx: &egui::Context) -> bool {
        let Some(confirm) = self.session.confirm.clone() else {
            return false;
        };
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.session.confirm = None;
        } else if widgets::enter_confirms(ctx) && ctx.input(|i| i.key_pressed(Key::Enter)) {
            match confirm {
                Confirm::Delete(paths) => self.delete_issues(&paths),
                Confirm::Stop(removal, paths) => self.remove_anyway(removal, &paths),
                Confirm::Quit(_) => self.quit_anyway(),
            }
        }
        true
    }

    /// Debounced autosave, while Auto is on.
    fn autosave(&mut self) {
        let due = self.prefs.auto
            && self
                .session
                .editor
                .as_ref()
                .is_some_and(EditorState::autosave_due);
        if due {
            self.flush_autosave(false);
        }
    }

    /// Auto, or Play watching Ready, reads the board again every
    /// [`RESCAN_EVERY`], off the UI thread, and takes it unless the board
    /// changed meanwhile.
    pub(super) fn rescan(&mut self, ctx: &egui::Context) {
        if self.prefs.auto || self.watching_ready() {
            if self.session.scan.is_none() && self.last_refresh.elapsed() >= RESCAN_EVERY {
                self.last_refresh = Instant::now();
                self.start_scan();
            }
            ctx.request_repaint_after(RESCAN_EVERY);
        }
    }
}

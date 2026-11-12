//! The newer release the status bar offers, and the dialog that installs it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::epaint::MarginF32;

use super::{Align, ButtonKind, IboardApp, Job, Layout, Leave, RichText, Vec2, theme, widgets};
use crate::job::{POLL_EVERY, Poll};
use crate::release::update::{self as updater, Update};

/// How often GitHub is asked for a newer release while the window shows.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// What the updater knows: the app bundle it may replace, the newer release
/// GitHub reported, the download in flight and the staged bundle.
#[derive(Default)]
pub(super) struct Updater {
    /// Whether this copy runs from an `iboard.app` and may update itself.
    pub(super) live: bool,
    pub(super) bundle: Option<PathBuf>,
    pub(super) latest: Option<Update>,
    pub(super) open: bool,
    pub(super) downloading: bool,
    pub(super) staged: Option<PathBuf>,
    pub(super) error: Option<String>,
    check: Option<Job<Result<Option<Update>, String>>>,
    checked: Option<Instant>,
    stage: Option<Job<Result<PathBuf, String>>>,
}

impl Updater {
    /// The updater of a running app: offered only from inside the bundle CI
    /// makes, so a checkout or a test never replaces itself.
    pub(super) fn new() -> Self {
        let bundle = std::env::current_exe()
            .ok()
            .and_then(|executable| updater::bundle_of(&executable));
        Self {
            live: bundle.is_some(),
            bundle,
            ..Self::default()
        }
    }

    fn cancel(&mut self) {
        self.stage = None;
        self.downloading = false;
        self.error = None;
    }
}

impl IboardApp {
    /// Asks GitHub for the newest release, and takes the download when it
    /// lands. Every frame, like the other background work.
    pub(super) fn poll_update(&mut self) -> Option<Duration> {
        if !self.update.live {
            return None;
        }
        if self.poll_update_stage() {
            return Some(POLL_EVERY);
        }
        if self.poll_update_check() {
            return Some(POLL_EVERY);
        }
        let left = self
            .update
            .checked
            .map(|checked| EVERY.saturating_sub(checked.elapsed()))
            .unwrap_or_default();
        if left.is_zero() {
            self.start_update_check();
        }
        Some(left.max(Duration::from_millis(1)))
    }

    fn poll_update_stage(&mut self) -> bool {
        let Some(stage) = self.update.stage.take() else {
            return false;
        };
        match stage.poll() {
            Poll::Waiting => {
                self.update.stage = Some(stage);
                true
            }
            Poll::Ready(Ok(staged)) => {
                self.update.staged = Some(staged);
                self.update.downloading = false;
                self.update.error = None;
                false
            }
            Poll::Ready(Err(error)) => {
                self.update.downloading = false;
                self.update.error = Some(error);
                false
            }
            Poll::Lost => {
                self.update.downloading = false;
                self.update.error = Some("The download stopped unexpectedly.".to_string());
                false
            }
        }
    }

    fn poll_update_check(&mut self) -> bool {
        let Some(check) = self.update.check.take() else {
            return false;
        };
        match check.poll() {
            Poll::Waiting => {
                self.update.check = Some(check);
                true
            }
            Poll::Ready(Ok(Some(latest))) => {
                self.update.latest = Some(latest);
                false
            }
            Poll::Ready(Ok(None)) => {
                self.update.latest = None;
                false
            }
            Poll::Ready(Err(_)) | Poll::Lost => false,
        }
    }

    fn start_update_check(&mut self) {
        self.update.checked = Some(Instant::now());
        let check = Job::spawn(|cancel| updater::latest(updater::VERSION, cancel));
        if let Some(ctx) = self.ctx.clone() {
            check.wake(move || ctx.request_repaint());
        }
        self.update.check = Some(check);
    }

    /// The status bar's offer, before the usage pill: `Update to 0.5.0`, or
    /// what the download is doing. A click opens the update dialog.
    pub(super) fn update_item(&mut self, ui: &mut egui::Ui) {
        if !self.update.live {
            return;
        }
        let Some(latest) = self.update.latest.as_ref() else {
            return;
        };
        let (label, tooltip) = match (&self.update.staged, self.update.downloading) {
            (Some(_), _) => (
                "Restart to update".to_string(),
                format!("iboard {} is ready to install", latest.version),
            ),
            (None, true) => (
                "Downloading update…".to_string(),
                format!("Downloading iboard {}", latest.version),
            ),
            (None, false) => (
                format!("Update to {}", latest.version),
                format!("iboard {} is available", latest.version),
            ),
        };
        let response =
            widgets::button(ui, ButtonKind::Ghost, None, &label, None).on_hover_text(tooltip);
        if response.clicked() {
            self.update.open = true;
        }
    }

    /// The update dialog: what the newer release is, then Download, then
    /// Restart. `esc` or Later closes it; a download left running finishes in
    /// the background and the status bar says when it is ready.
    pub(super) fn update_window(&mut self, ui: &egui::Ui) {
        if !self.update.open {
            return;
        }
        let Some(latest) = self.update.latest.clone() else {
            self.update.open = false;
            return;
        };
        let downloading = self.update.downloading;
        let staged = self.update.staged.is_some();
        let error = self.update.error.clone();
        let p = theme::palette(ui);
        let size = theme::ModalSize::S;
        let (width, pad) = (theme::modal_size(size, ui.ctx().content_rect()).x, 20.0);
        let mut close = false;
        let mut download = false;
        let mut restart = false;
        let mut cancel = false;
        let frame = widgets::modal_frame(ui).inner_margin(MarginF32 {
            left: pad,
            right: pad,
            top: pad,
            bottom: theme::SPACE_5,
        });
        let modal = widgets::modal(ui, "update", size, "Update iboard?")
            .frame(frame)
            .show(ui.ctx(), |ui| {
                ui.set_width(width - 2.0 * pad);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                ui.label(
                    RichText::new("Update iboard?")
                        .font(theme::ui_title())
                        .color(p.ink),
                );
                update_subject(ui, &latest.version);
                update_body(ui, &latest.version, downloading, staged, error.as_deref());
                ui.add_space(theme::SPACE_3);
                widgets::focus_here(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    if staged {
                        if widgets::button(
                            ui,
                            ButtonKind::Primary,
                            None,
                            "Restart to update",
                            Some("↩"),
                        )
                        .clicked()
                        {
                            restart = true;
                        }
                        let later = widgets::button(ui, ButtonKind::Secondary, None, "Later", None);
                        ui.data_mut(|data| data.insert_temp(secondary_id(), later.id));
                        if later.clicked() {
                            close = true;
                        }
                    } else if downloading {
                        widgets::button(ui, ButtonKind::DisabledFilled, None, "Downloading…", None);
                        let stop = widgets::button(ui, ButtonKind::Secondary, None, "Cancel", None);
                        ui.data_mut(|data| data.insert_temp(secondary_id(), stop.id));
                        if stop.clicked() {
                            cancel = true;
                        }
                    } else {
                        let verb = if error.is_some() {
                            "Try again"
                        } else {
                            "Download"
                        };
                        if widgets::button(ui, ButtonKind::Primary, None, verb, Some("↩")).clicked()
                        {
                            download = true;
                        }
                        let later = widgets::button(ui, ButtonKind::Secondary, None, "Later", None);
                        ui.data_mut(|data| data.insert_temp(secondary_id(), later.id));
                        if later.clicked() {
                            close = true;
                        }
                    }
                });
            });
        if close || modal.should_close() {
            self.update.open = false;
        } else if cancel {
            self.update.cancel();
            self.update.open = false;
        } else if download {
            self.start_update_download();
        } else if restart {
            self.update.open = false;
            self.leave(Leave::Update);
        }
    }

    fn start_update_download(&mut self) {
        let Some(update) = self.update.latest.clone() else {
            return;
        };
        self.update.downloading = true;
        self.update.error = None;
        let stage = Job::spawn(move |cancel| updater::stage(&update, cancel));
        if let Some(ctx) = self.ctx.clone() {
            stage.wake(move || ctx.request_repaint());
        }
        self.update.stage = Some(stage);
    }

    /// Runs what the dialog's primary does: download, or restart.
    pub(super) fn update_confirm(&mut self) {
        if self.update.staged.is_some() {
            self.update.open = false;
            self.leave(Leave::Update);
        } else if !self.update.downloading {
            self.start_update_download();
        }
    }

    /// What the dialog's secondary does: Later, or Cancel a download.
    pub(super) fn update_secondary(&mut self) {
        if self.update.downloading {
            self.update.cancel();
        }
        self.update.open = false;
    }

    /// Puts the staged bundle in place and reopens it once this process quits.
    /// False, with the reason in the dialog, when it could not.
    pub(super) fn relaunch_update(&mut self) -> bool {
        let (Some(bundle), Some(staged)) = (&self.update.bundle, &self.update.staged) else {
            self.update.error = Some("This copy does not run from an iboard.app.".to_string());
            self.update.open = true;
            return false;
        };
        let args = self.relaunch_args();
        match updater::relaunch(bundle, staged, &args) {
            Ok(()) => true,
            Err(error) => {
                self.update.error = Some(error);
                self.update.open = true;
                false
            }
        }
    }

    /// The project the next launch opens: this one, or every group for a group
    /// board. Nothing to pass when there is no board yet.
    fn relaunch_args(&self) -> Vec<String> {
        if self.welcoming() {
            Vec::new()
        } else if self.group.is_some() {
            vec!["--all".to_string()]
        } else {
            vec![self.located.project_dir.display().to_string()]
        }
    }
}

/// The dialog's secondary button: Later, or Cancel while downloading.
fn secondary_id() -> egui::Id {
    egui::Id::unique("update-secondary")
}

/// Whether the secondary button holds the focus, so `↩` means it, not the
/// primary.
pub(super) fn update_secondary_focused(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<egui::Id>(secondary_id()))
        .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
}

fn update_subject(ui: &mut egui::Ui, version: &str) {
    let p = theme::palette(ui);
    let width = ui.available_width();
    egui::Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(egui::CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Vec2::new(10.0, theme::SPACE_3))
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            ui.spacing_mut().item_spacing.y = theme::SPACE_1;
            ui.label(
                RichText::new(format!("iboard {version}"))
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            ui.label(
                RichText::new(format!("You're on {}.", updater::VERSION))
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
}

fn update_body(
    ui: &mut egui::Ui,
    version: &str,
    downloading: bool,
    staged: bool,
    error: Option<&str>,
) {
    let p = theme::palette(ui);
    let text = match (staged, downloading) {
        (true, _) => format!(
            "iboard {version} is ready. Restarting replaces this copy and reopens it on this project."
        ),
        (false, true) => format!("Downloading iboard {version}…"),
        (false, false) => {
            "It replaces this copy and reopens it on this project. Your issues and settings stay where they are."
                .to_string()
        }
    };
    ui.label(
        RichText::new(text)
            .font(theme::ui_small())
            .color(p.ink_muted),
    );
    if let Some(error) = error {
        ui.label(RichText::new(error).font(theme::ui_small()).color(p.danger));
    }
}

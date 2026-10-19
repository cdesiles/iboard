//! Images in an issue: pasted or dropped into the composer or the editor,
//! shown by the reader, and carried through renames, deletes and splits.

use super::editor::EDITOR_ID;
use super::*;
use crate::assets::{self, Carry};

/// Where a pasted image comes from.
pub(super) enum Clipboard {
    /// The system pasteboard.
    System,
    /// An image the app holds itself: what a test pastes.
    Held(Option<Vec<u8>>),
}

/// The composer's images before its issue has a name, under
/// `.assets/.new-<pid>-<n>/`. Writing the issue moves them to its slug;
/// whatever is left goes when the composer does.
pub(super) struct PendingImages {
    root: PathBuf,
    pub(super) slug: String,
}

impl PendingImages {
    fn new(root: &Path) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            root: root.to_path_buf(),
            slug: format!(".new-{}-{n}", std::process::id()),
        }
    }
}

impl Drop for PendingImages {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(assets::folder(&self.root, &self.slug));
    }
}

/// An issue's images sent to the Trash with it, for `⌘Z`.
pub(super) struct TrashedImages {
    folder: PathBuf,
    trashed: PathBuf,
}

/// The field an image goes into.
#[derive(Clone, Copy)]
enum Target {
    Composer,
    Editor,
}

impl IboardApp {
    /// Takes this frame's dropped files and `⌘V`, and puts their images in
    /// the composer or the editor. Text on the clipboard wins over an image.
    pub(super) fn paste_images(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        let pressed = iboard::pasteboard::take_paste();
        #[cfg(not(target_os = "macos"))]
        let pressed = false;
        let (pasted, text, dropped) = ctx.input(|i| {
            let key = i.events.iter().any(|event| {
                matches!(event, egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.command)
            });
            let text = i
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(text) if !text.is_empty()));
            (key, text, i.raw.dropped_files.clone())
        });
        for file in dropped {
            if let Some(target) = self.image_target(ctx, false) {
                match dropped_bytes(&file) {
                    Ok(bytes) => self.insert_image(ctx, target, &bytes),
                    Err(err) => self.error = Some(err),
                }
            }
        }
        if !(pressed || pasted) || text || self.clipboard_has_text() {
            return;
        }
        if let Some(target) = self.image_target(ctx, true)
            && let Some(bytes) = self.clipboard_image()
        {
            self.insert_image(ctx, target, &bytes);
        }
    }

    fn clipboard_has_text(&self) -> bool {
        match &self.clipboard {
            #[cfg(target_os = "macos")]
            Clipboard::System => iboard::pasteboard::has_text(),
            #[cfg(not(target_os = "macos"))]
            Clipboard::System => false,
            Clipboard::Held(_) => false,
        }
    }

    fn clipboard_image(&self) -> Option<Vec<u8>> {
        match &self.clipboard {
            #[cfg(target_os = "macos")]
            Clipboard::System => iboard::pasteboard::image(),
            #[cfg(not(target_os = "macos"))]
            Clipboard::System => None,
            Clipboard::Held(image) => image.clone(),
        }
    }

    /// The editor or the composer, when one is open; a paste also needs it
    /// to have the keyboard.
    fn image_target(&self, ctx: &egui::Context, paste: bool) -> Option<Target> {
        let focused = |id: &str| !paste || ctx.memory(|m| m.has_focus(egui::Id::new(id)));
        let editing = matches!(self.session.screen, Screen::Detail(_))
            && self.session.detail_mode == DetailMode::Edit
            && self.session.editor.is_some();
        if editing && focused(EDITOR_ID) {
            return Some(Target::Editor);
        }
        let composing = matches!(self.session.screen, Screen::Board)
            && self
                .session
                .composer
                .as_ref()
                .is_some_and(|c| c.stage != TitleStage::Naming);
        (composing && (focused(TITLE_ID) || focused(DESCRIPTION_ID))).then_some(Target::Composer)
    }

    /// Writes the image to the issue's folder and links it at the caret; a
    /// refused image says why in the status strip.
    fn insert_image(&mut self, ctx: &egui::Context, target: Target, bytes: &[u8]) {
        let png = match assets::to_png(bytes) {
            Ok(png) => png,
            Err(err) => {
                self.error = Some(err);
                return;
            }
        };
        let root = self.board.root.clone();
        let written = match target {
            Target::Editor => self.session.editor.as_mut().map(|editor| {
                let link = assets::add(&root, &reviews::slug(&editor.path), &png)?;
                insert_at_caret(ctx, EDITOR_ID, &mut editor.text, &link);
                editor.last_edit = Some(Instant::now());
                ctx.request_repaint_after(AUTOSAVE_AFTER);
                Ok::<(), std::io::Error>(())
            }),
            Target::Composer => self.session.composer.as_mut().map(|composer| {
                let pending = composer
                    .images
                    .get_or_insert_with(|| PendingImages::new(&root));
                let link = assets::add(&root, &pending.slug, &png)?;
                if !ctx.memory(|m| m.has_focus(egui::Id::new(DESCRIPTION_ID))) {
                    let end = CCursor::new(composer.description.chars().count());
                    let mut state = egui::TextEdit::load_state(ctx, egui::Id::new(DESCRIPTION_ID))
                        .unwrap_or_default();
                    state.cursor.set_char_range(Some(CCursorRange::one(end)));
                    state.store(ctx, egui::Id::new(DESCRIPTION_ID));
                }
                insert_at_caret(ctx, DESCRIPTION_ID, &mut composer.description, &link);
                Ok(())
            }),
        };
        if let Some(Err(err)) = written {
            self.error = Some(format!("Couldn't save the image: {err}"));
        }
    }

    /// The composer's description, its images moved to the issue `name`.
    pub(super) fn adopt_pending_images(&mut self, name: &str, description: &str) -> String {
        let Some(pending) = self
            .session
            .composer
            .as_ref()
            .and_then(|c| c.images.as_ref())
        else {
            return description.to_string();
        };
        let slug = reviews::slug(Path::new(name));
        match assets::relink(
            &self.board.root,
            description,
            &pending.slug,
            &slug,
            Carry::Move,
        ) {
            Ok(text) => text,
            Err(err) => {
                self.error = Some(format!("Couldn't move the images: {err}"));
                description.to_string()
            }
        }
    }

    /// After the editor renamed `old` to `new`: its images follow it, and
    /// the issue's links with them.
    pub(super) fn rename_images(&mut self, old: &Path, new: &Path) {
        let (from, to) = (reviews::slug(old), reviews::slug(new));
        let root = self.board.root.clone();
        let Some(editor) = self.session.editor.as_mut().filter(|e| e.path == new) else {
            return;
        };
        let relinked =
            assets::relink(&root, &editor.text, &from, &to, Carry::Move).and_then(|text| {
                if text != editor.text {
                    atomic_write(new, &text)?;
                }
                Ok(text)
            });
        match relinked {
            Ok(text) => {
                editor.text = text.clone();
                editor.baseline = text;
            }
            Err(err) => {
                self.error = Some(format!(
                    "Couldn't move the images of {}: {err}",
                    file_name(new)
                ))
            }
        }
    }

    /// `text` for the split ticket `child` of `original`, with its own copy
    /// of the images it links.
    pub(super) fn copy_images(&mut self, original: &Path, child: &Path, text: &str) -> String {
        let (from, to) = (reviews::slug(original), reviews::slug(child));
        match assets::relink(&self.board.root, text, &from, &to, Carry::Copy) {
            Ok(text) => text,
            Err(err) => {
                self.error = Some(format!("Couldn't copy the images: {err}"));
                text.to_string()
            }
        }
    }

    /// Sends the images of the deleted `path` to the Trash, unless another
    /// issue goes by the same slug.
    pub(super) fn trash_images(&mut self, path: &Path) -> Option<TrashedImages> {
        let slug = reviews::slug(path);
        let shared = self
            .board
            .columns
            .iter()
            .flat_map(|column| &column.issues)
            .any(|issue| issue.path != path && reviews::slug(&issue.path) == slug);
        let folder = assets::folder(&self.board.root, &slug);
        if shared || !folder.is_dir() {
            return None;
        }
        match self.trash.delete(&folder) {
            Ok(trashed) => trashed.map(|trashed| TrashedImages { folder, trashed }),
            Err(err) => {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(&folder)));
                None
            }
        }
    }

    /// Brings back the images [`Self::trash_images`] sent away.
    pub(super) fn restore_images(&mut self, images: TrashedImages) {
        if !images.folder.exists()
            && let Err(err) = trash::restore(&images.trashed, &images.folder)
        {
            self.error = Some(format!(
                "Couldn't restore {}: {err}",
                self.relative(&images.folder)
            ));
        }
    }

    /// Says once in the status strip which images of `issue` can't be shown.
    pub(super) fn report_missing_images(&mut self, issue: &Path, missing: &[String]) {
        let fresh: Vec<&String> = missing
            .iter()
            .filter(|url| {
                self.reader
                    .reported
                    .insert((issue.to_path_buf(), (*url).clone()))
            })
            .collect();
        if let Some(url) = fresh.first() {
            self.error = Some(format!(
                "Image missing or unreadable in {}: {url}",
                self.relative(issue)
            ));
        }
    }
}

/// A dropped file's bytes; a file over the limit is refused unread.
fn dropped_bytes(file: &egui::DroppedFileHandle) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(file.path()).map_or(0, |m| m.len());
    if size > assets::MAX_BYTES as u64 {
        return Err(assets::too_big(size));
    }
    file.bytes()
        .map_err(|err| format!("Couldn't read {}: {err}", display_path(file.path())))
}

/// Puts `insert` in place of the selection of the text field `id`, or at
/// its caret, or at the end when it never had one; the caret lands after it.
fn insert_at_caret(ctx: &egui::Context, id: &str, text: &mut String, insert: &str) {
    let id = egui::Id::new(id);
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let chars = text.chars().count();
    let [start, end] = state
        .cursor
        .char_range()
        .map(|range| range.sorted_cursors().map(|c| c.index.0.min(chars)))
        .unwrap_or([chars, chars]);
    let byte = |at: usize| text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
    let range = byte(start)..byte(end);
    text.replace_range(range, insert);
    let caret = CCursor::new(start + insert.chars().count());
    state.cursor.set_char_range(Some(CCursorRange::one(caret)));
    state.store(ctx, id);
}

/// The local images of one reader block that can't be shown, by link.
pub(super) fn missing_images(issue: &Path, block: &str) -> Vec<assets::ImageLink> {
    assets::image_links(block)
        .into_iter()
        .filter(|link| {
            assets::local_file(issue, &link.url)
                .is_some_and(|file| image::image_dimensions(file).is_err())
        })
        .collect()
}

/// `block` with each of `missing` drawn as a placeholder naming its file.
pub(super) fn with_placeholders(block: &str, missing: &[assets::ImageLink]) -> String {
    let mut out = block.to_string();
    for link in missing.iter().rev() {
        let name = link.url.rsplit('/').next().unwrap_or(&link.url);
        out.replace_range(link.span.clone(), &format!("`image missing: {name}`"));
    }
    out
}

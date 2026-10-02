//! The egui application: a kanban board with an inline quick-add composer, a
//! rendered markdown reader, and an editor with debounced autosave. Its look
//! follows the design system in `design/` (see `theme.rs`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{
    Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::discovery::{self, Column, Located, Roadmap};
use crate::theme::{self, Icon, Palette};
use crate::ticket::Ticket;
use crate::widgets::{self, ButtonKind};

/// How long after the last keystroke an edit is written to disk.
const AUTOSAVE_AFTER: Duration = Duration::from_millis(800);

/// How long a ticket created from the composer keeps its `accent-soft` fill.
const NEW_CARD_FLASH: Duration = Duration::from_secs(2);

/// On macOS the header bar doubles as the window's title bar (the native
/// title is hidden and the content runs under the traffic lights).
pub const UNIFIED_TITLEBAR: bool = cfg!(target_os = "macos");

/// Room the macOS traffic lights take at the left of the header bar.
const TRAFFIC_LIGHTS_WIDTH: i8 = 80;

/// Width of the `⌘F` key cap drawn inside the filter field.
const FILTER_KBD_WIDTH: f32 = 26.0;

/// Longest slug the composer derives from a title.
const MAX_SLUG_LEN: usize = 60;

/// Longest file name the editor accepts, in bytes, including its extension.
/// 255 bytes is the limit of APFS, ext4 and NTFS: longer names fail to write.
const MAX_FILE_NAME_BYTES: usize = 255;

/// Directory done tickets are archived into, under the roadmap root. It starts
/// with a dot, so discovery never reads it back.
const ARCHIVE_DIR: &str = ".archive";

/// A ticket action available from a card or the detail header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CardAction {
    /// Open the ticket in the reader.
    Open,
    /// Delete the ticket's file.
    Delete,
    /// Move a done ticket to `.archive`.
    Archive,
}

/// A destructive action waiting for confirmation.
#[derive(Clone)]
enum Confirm {
    Delete(PathBuf),
}

/// Which half of the detail view is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DetailMode {
    Preview,
    Edit,
}

#[derive(Clone)]
enum Screen {
    Board,
    Detail(PathBuf),
}

/// Where the user asked to go when leaving the editor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Leave {
    Reader,
    Board,
    /// The board with the quick-add composer open (`⌘N`).
    Composer,
    /// Closing the window.
    Quit,
}

/// The answer to "Save changes to … ?" when leaving with unsaved edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LeaveChoice {
    Save,
    Discard,
    KeepEditing,
}

/// A ticket being dragged across the board. The payload is the ticket's file,
/// which a drop moves into the target column's directory.
#[derive(Clone)]
struct TicketDrag {
    path: PathBuf,
}

/// What the user did inside one column during a frame.
#[derive(Default)]
struct ColumnActions {
    /// A card was clicked, opening it in the reader.
    clicked: Option<PathBuf>,
    /// A card was dropped onto this column: the path to move here.
    dropped: Option<PathBuf>,
    /// Where in the column the drop landed: insert before this card, or at the
    /// end when it is `None`.
    drop_anchor: Option<PathBuf>,
    /// A card's delete icon was pressed.
    delete: Option<PathBuf>,
    /// A done card's archive icon was pressed.
    archive: Option<PathBuf>,
    /// The "+" button was pressed: open the composer.
    compose: bool,
    /// The composer submitted its title; `true` also opens the editor.
    submit: Option<bool>,
    /// The composer lost focus while empty.
    close_composer: bool,
}

/// The inline quick-add composer at the top of the Todo column.
#[derive(Default)]
struct Composer {
    title: String,
    /// Give the title field keyboard focus on the next frame.
    focus: bool,
    error: Option<String>,
}

/// The in-progress edit of one ticket.
struct EditorState {
    path: PathBuf,
    /// Current buffer, edited by the user.
    text: String,
    /// Content as last seen on disk (loaded or saved), for dirty detection.
    baseline: String,
    /// File name as edited by the user.
    name: String,
    /// File name as last seen on disk (loaded or saved).
    baseline_name: String,
    /// Newer on-disk content that arrived while we have unsaved edits.
    conflict: Option<String>,
    last_edit: Option<Instant>,
    saved_flash: Option<Instant>,
    error: Option<String>,
}

impl EditorState {
    fn new(path: PathBuf, content: String) -> Self {
        let name = file_name(&path);
        Self {
            path,
            baseline: content.clone(),
            text: content,
            baseline_name: name.clone(),
            name,
            conflict: None,
            last_edit: None,
            saved_flash: Some(Instant::now()),
            error: None,
        }
    }

    fn is_dirty(&self) -> bool {
        self.text != self.baseline || self.name != self.baseline_name
    }
}

/// The file name part of a path, as UTF-8 (empty when there is none).
fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Makes sure a user-typed file name keeps a markdown extension so the ticket
/// stays visible to discovery, and caps it at [`MAX_FILE_NAME_BYTES`] so the
/// filesystem accepts it. A missing or non-markdown extension is replaced with
/// `.md`.
fn normalize_file_name(input: &str) -> String {
    let trimmed = input.trim();
    let is_markdown = Path::new(trimmed)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false);
    let (stem, ext) = if is_markdown {
        let ext = Path::new(trimmed)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("md");
        let stem = trimmed.strip_suffix(&format!(".{ext}")).unwrap_or(trimmed);
        (stem.to_string(), ext.to_string())
    } else {
        (trimmed.to_string(), "md".to_string())
    };
    let budget = MAX_FILE_NAME_BYTES.saturating_sub(ext.len() + 1);
    format!("{}.{ext}", truncate_stem(&stem, budget))
}

/// Cuts `stem` to at most `max_bytes`, on a character boundary, without leaving
/// a trailing hyphen or space.
fn truncate_stem(stem: &str, max_bytes: usize) -> String {
    if stem.len() <= max_bytes {
        return stem.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !stem.is_char_boundary(end) {
        end -= 1;
    }
    stem[..end].trim_end_matches(['-', ' ']).to_string()
}

/// A file name in `dir` that does not exist yet: `name`, then `name-2`, ...
fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) => (
            name.strip_suffix(&format!(".{ext}")).unwrap_or(name),
            format!(".{ext}"),
        ),
        None => (name, String::new()),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}{ext}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Renames the files of an ordered column so their `001-` prefixes match
/// `order`. Renames go through hidden temp names in two phases, so a swap
/// (`001-a` ↔ `002-b`) cannot clobber a file. Returns every `(old, new)` pair
/// that changed.
fn renumber(dir: &Path, order: &[PathBuf]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let changes: Vec<(PathBuf, PathBuf)> = order
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let target = dir.join(discovery::numbered_name(index + 1, &file_name(path)));
            (target != *path).then(|| (path.clone(), target))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: every changing file moves to a unique hidden temp name.
    let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
    for (index, (original, target)) in changes.iter().enumerate() {
        let temp = dir.join(format!(".renumber-{}-{index}.tmp", std::process::id()));
        if let Err(err) = std::fs::rename(original, &temp) {
            restore(&staged);
            return Err(format!("Couldn't renumber {}: {err}", file_name(original)));
        }
        staged.push((temp, original.clone(), target.clone()));
    }

    // Phase 2: temps take their final names. On failure, put back what moved.
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (index, (temp, original, target)) in staged.iter().enumerate() {
        if let Err(err) = std::fs::rename(temp, target) {
            restore(&staged[index..]);
            for (final_name, original) in &done {
                let _ = std::fs::rename(final_name, original);
            }
            return Err(format!("Couldn't renumber {}: {err}", file_name(original)));
        }
        done.push((target.clone(), original.clone()));
    }
    Ok(changes)
}

/// Best-effort undo of the staging phase.
fn restore(staged: &[(PathBuf, PathBuf, PathBuf)]) {
    for (temp, original, _) in staged {
        let _ = std::fs::rename(temp, original);
    }
}

pub struct RoadmapApp {
    located: Located,
    roadmap: Roadmap,
    cache: CommonMarkCache,
    screen: Screen,
    detail_mode: DetailMode,
    editor: Option<EditorState>,
    composer: Option<Composer>,
    /// The ticket the composer just wrote, highlighted for a moment.
    new_card: Option<(PathBuf, Instant)>,
    filter: String,
    focus_filter: bool,
    error: Option<String>,
    /// A destructive action waiting for the user to confirm it.
    confirm: Option<Confirm>,
    /// Auto: reload when files change on disk, save edits as you type and when
    /// leaving the editor. Off, the editor asks before dropping unsaved edits.
    auto: bool,
    last_refresh: Instant,
    /// The card selected on the board, moved with the arrow keys.
    selected: Option<PathBuf>,
    /// Scroll the selected card into view on the next frame.
    scroll_to_selected: bool,
    /// Leaving the editor is on hold until the user answers the save prompt.
    leave_prompt: Option<Leave>,
    /// The window may close: unsaved edits were saved, discarded or absent.
    closing: bool,
    /// Ask the window to close on the next frame (after the save prompt).
    send_close: bool,
    /// The rendered ticket content, used to tell a double-click on the page
    /// from one on the header when deciding whether to start editing.
    preview_rect: Rect,
    /// Set when a freshly created ticket should receive keyboard focus.
    focus_editor: bool,
}

impl RoadmapApp {
    /// `window_title` names the window in Mission Control and the Window menu;
    /// see `main.rs` for why it is set here rather than at startup.
    pub fn new(cc: &eframe::CreationContext<'_>, located: Located, window_title: String) -> Self {
        theme::install(&cc.egui_ctx);
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(window_title));
        // After winit's default menu is up, so this one replaces it.
        #[cfg(target_os = "macos")]
        crate::menu::install();
        Self::from_located(located)
    }

    fn from_located(located: Located) -> Self {
        let (roadmap, error) = match discovery::build(&located) {
            Ok(roadmap) => (roadmap, None),
            Err(err) => (Roadmap::empty(&located), Some(format!("{err:#}"))),
        };
        Self {
            located,
            roadmap,
            cache: CommonMarkCache::default(),
            screen: Screen::Board,
            detail_mode: DetailMode::Preview,
            editor: None,
            composer: None,
            new_card: None,
            filter: String::new(),
            focus_filter: false,
            error,
            confirm: None,
            auto: true,
            last_refresh: Instant::now(),
            selected: None,
            scroll_to_selected: false,
            leave_prompt: None,
            closing: false,
            send_close: false,
            preview_rect: Rect::NOTHING,
            focus_editor: false,
        }
    }

    fn refresh(&mut self) {
        match discovery::build(&self.located) {
            Ok(roadmap) => {
                self.roadmap = roadmap;
                self.error = None;
            }
            Err(err) => self.error = Some(format!("{err:#}")),
        }
        self.reconcile_editor();
    }

    /// Notices external changes to the file being edited.
    fn reconcile_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some(ticket) = self.roadmap.find(&editor.path) else {
            return;
        };
        let disk = &ticket.markdown;
        if disk == &editor.baseline {
            return;
        }
        if editor.is_dirty() {
            // Someone (or something) wrote while we have unsaved work.
            editor.conflict = Some(disk.clone());
        } else {
            // No local edits: quietly adopt the newest content.
            editor.text = disk.clone();
            editor.baseline = disk.clone();
            editor.conflict = None;
        }
    }

    /// Writes the buffer if it is dirty, renaming the file when its name was
    /// edited too. With `force`, also overwrites a detected conflict ("Keep
    /// mine").
    fn flush_autosave(&mut self, force: bool) {
        let mut renamed_to: Option<PathBuf> = None;
        let saved = {
            let Some(editor) = self.editor.as_mut() else {
                return;
            };
            if !editor.is_dirty() {
                return;
            }
            if editor.conflict.is_some() && !force {
                return;
            }

            // Work out the target path from the (possibly edited) file name.
            if editor.name.trim().is_empty() {
                editor.error = Some("the file name cannot be empty".to_string());
                return;
            }
            let desired = normalize_file_name(&editor.name);
            let target = editor.path.with_file_name(&desired);
            if target != editor.path && target.exists() {
                editor.error = Some(format!(
                    "Couldn't rename to {desired}: that file already exists. Pick another name."
                ));
                return;
            }

            // Save the content under the current name, then move the file if the
            // name changed. A refused rename leaves the saved content in place.
            if let Err(err) = atomic_write(&editor.path, &editor.text) {
                editor.error = Some(format!("Couldn't save {}: {err}", file_name(&editor.path)));
                return;
            }
            if target != editor.path {
                match std::fs::rename(&editor.path, &target) {
                    Ok(()) => {
                        editor.path = target.clone();
                        editor.baseline_name = desired.clone();
                        editor.name = desired.clone();
                        renamed_to = Some(target);
                    }
                    Err(err) => {
                        editor.error = Some(format!("Couldn't rename to {desired}: {err}"));
                        editor.baseline = editor.text.clone();
                        editor.last_edit = None;
                        editor.saved_flash = Some(Instant::now());
                        return;
                    }
                }
            }

            editor.baseline = editor.text.clone();
            if force {
                editor.conflict = None;
            }
            editor.last_edit = None;
            editor.saved_flash = Some(Instant::now());
            editor.error = None;
            true
        };
        if let Some(target) = renamed_to {
            // Keep the reader and the selection on the ticket's new file name.
            self.selected = Some(target.clone());
            if let Screen::Detail(open) = &mut self.screen
                && open != &target
            {
                *open = target;
            }
        }
        if saved {
            self.refresh();
        }
    }

    fn reload_editor_from_disk(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let Some(ticket) = self.roadmap.find(&editor.path) else {
            return;
        };
        editor.text = ticket.markdown.clone();
        editor.baseline = ticket.markdown.clone();
        editor.conflict = None;
        editor.last_edit = None;
        editor.error = None;
    }

    /// Drops unsaved edits (content and file name) and returns to the reader.
    fn discard_edits(&mut self) {
        if let Some(editor) = self.editor.as_mut() {
            editor.text = editor.baseline.clone();
            editor.name = editor.baseline_name.clone();
            editor.conflict = None;
            editor.last_edit = None;
            editor.error = None;
        }
        self.detail_mode = DetailMode::Preview;
    }

    /// Opens a ticket in the reader. Editing is a separate step, entered from
    /// the detail view.
    fn open_ticket(&mut self, path: PathBuf) {
        self.composer = None;
        self.selected = Some(path.clone());
        self.screen = Screen::Detail(path);
        self.detail_mode = DetailMode::Preview;
    }

    fn start_editing(&mut self, path: &Path) {
        if self
            .editor
            .as_ref()
            .map(|e| e.path == path)
            .unwrap_or(false)
        {
            self.detail_mode = DetailMode::Edit;
            return;
        }
        // Persist any pending edits of another ticket before switching.
        self.save_pending();

        let Some(ticket) = self.roadmap.find(path) else {
            return;
        };
        let content = ticket.markdown.clone();
        self.editor = Some(EditorState::new(path.to_path_buf(), content));
        self.screen = Screen::Detail(path.to_path_buf());
        self.detail_mode = DetailMode::Edit;
    }

    /// Saves unsaved edits unless they conflict with the file on disk.
    fn save_pending(&mut self) {
        let should_save = self
            .editor
            .as_ref()
            .map(|e| e.is_dirty() && e.conflict.is_none())
            .unwrap_or(false);
        if should_save {
            self.flush_autosave(false);
        }
    }

    fn has_unsaved_edits(&self) -> bool {
        self.editor.as_ref().is_some_and(EditorState::is_dirty)
    }

    /// Leaves the current screen for `to`. From the editor, unsaved edits are
    /// saved when Auto is on; otherwise, or when saving fails, the editor bar
    /// asks first and nothing moves until the user answers.
    fn leave(&mut self, to: Leave) {
        let editing =
            matches!(self.screen, Screen::Detail(_)) && self.detail_mode == DetailMode::Edit;
        if editing {
            if self.auto {
                self.save_pending();
            }
            if self.has_unsaved_edits() {
                self.leave_prompt = Some(to);
                return;
            }
        }
        self.go(to);
    }

    fn go(&mut self, to: Leave) {
        self.leave_prompt = None;
        match to {
            Leave::Reader => self.detail_mode = DetailMode::Preview,
            Leave::Board => {
                self.detail_mode = DetailMode::Preview;
                self.screen = Screen::Board;
            }
            Leave::Composer => {
                self.detail_mode = DetailMode::Preview;
                self.screen = Screen::Board;
                self.show_composer();
            }
            Leave::Quit => {
                self.closing = true;
                self.send_close = true;
            }
        }
    }

    /// Applies the answer to the save prompt.
    fn answer_leave(&mut self, choice: LeaveChoice) {
        let Some(to) = self.leave_prompt.take() else {
            return;
        };
        match choice {
            LeaveChoice::Save => {
                self.flush_autosave(true);
                // A failed save stays in the editor, showing why.
                if !self.has_unsaved_edits() {
                    self.go(to);
                }
            }
            LeaveChoice::Discard => {
                self.discard_edits();
                self.go(to);
            }
            LeaveChoice::KeepEditing => {}
        }
    }

    fn show_preview(&mut self) {
        self.leave(Leave::Reader);
    }

    fn back_to_board(&mut self) {
        self.leave(Leave::Board);
    }

    /// Moves a dragged ticket into another column's directory, keeping its file
    /// name apart from the Ready priority number (see
    /// [`discovery::moved_name`]). An existing file with the same name is never
    /// overwritten. Returns the ticket's new path on success.
    fn move_ticket(&mut self, path: &Path, target_dir: &Path) -> Option<PathBuf> {
        let (Some(name), Some(from_dir)) = (path.file_name(), path.parent()) else {
            return None;
        };
        if from_dir == target_dir {
            return Some(path.to_path_buf());
        }
        let name = discovery::moved_name(&name.to_string_lossy(), from_dir, target_dir);
        let target = target_dir.join(&name);
        if target.exists() {
            self.error = Some(format!(
                "Couldn't move {}: {name} already exists in that column. Rename one of them first.",
                self.relative(path),
            ));
            return None;
        }
        match std::fs::rename(path, &target) {
            Ok(()) => {
                // Keep the reader/editor pointed at the ticket's new home.
                if let Some(editor) = self.editor.as_mut()
                    && editor.path == path
                {
                    editor.path = target.clone();
                }
                if let Screen::Detail(open) = &mut self.screen
                    && open == path
                {
                    *open = target.clone();
                }
                if self.selected.as_deref() == Some(path) {
                    self.selected = Some(target.clone());
                }
                self.refresh();
                Some(target)
            }
            Err(err) => {
                self.error = Some(format!("Couldn't move {}: {err}", self.relative(path)));
                None
            }
        }
    }

    /// Handles a card drop: move it between columns when needed, then, in a
    /// file-name ordered column (Todo, Ready), renumber the files so the drop
    /// position becomes the priority. `anchor` is the card the dropped one
    /// lands in front of, or `None` for the end.
    fn place_ticket(&mut self, path: &Path, target_dir: &Path, anchor: Option<PathBuf>) {
        let same_dir = path.parent() == Some(target_dir);
        let orderable = discovery::is_orderable_column(target_dir);
        if same_dir && !orderable {
            return;
        }
        let moved = if same_dir {
            path.to_path_buf()
        } else {
            match self.move_ticket(path, target_dir) {
                Some(moved) => moved,
                None => return,
            }
        };
        if orderable {
            self.reorder_column(target_dir, &moved, anchor.as_deref());
        }
    }

    /// Renumbers an ordered column so `moved` sits in front of `anchor`, keeping
    /// every other ticket in its current order.
    fn reorder_column(&mut self, dir: &Path, moved: &Path, anchor: Option<&Path>) {
        if anchor == Some(moved) {
            return;
        }
        self.refresh();
        let mut order: Vec<PathBuf> = match self
            .roadmap
            .columns
            .iter()
            .find(|column| column.dir.as_path() == dir)
        {
            Some(column) => column.tickets.iter().map(|t| t.path.clone()).collect(),
            None => return,
        };
        order.retain(|path| path != moved);
        match anchor.and_then(|anchor| order.iter().position(|path| path == anchor)) {
            Some(index) => order.insert(index, moved.to_path_buf()),
            None => order.push(moved.to_path_buf()),
        }
        match renumber(dir, &order) {
            Ok(renames) => self.apply_renames(&renames),
            Err(err) => self.error = Some(err),
        }
        self.refresh();
    }

    /// Follows renamed files in the selection, the reader and the editor.
    fn apply_renames(&mut self, renames: &[(PathBuf, PathBuf)]) {
        for (old, new) in renames {
            if self.selected.as_deref() == Some(old.as_path()) {
                self.selected = Some(new.clone());
            }
            if let Some(editor) = self.editor.as_mut()
                && editor.path == *old
            {
                editor.path = new.clone();
            }
            if let Screen::Detail(open) = &mut self.screen
                && open == old
            {
                *open = new.clone();
            }
        }
    }

    /// The column a ticket currently belongs to.
    fn column_of(&self, path: &Path) -> Option<&Column> {
        self.roadmap
            .columns
            .iter()
            .find(|column| column.tickets.iter().any(|ticket| ticket.path == path))
    }

    /// Asks before deleting: the trash icon and the Delete key land here.
    fn request_delete(&mut self, path: PathBuf) {
        self.confirm = Some(Confirm::Delete(path));
    }

    /// Deletes a ticket's file and lets the board catch up.
    fn delete_ticket(&mut self, path: &Path) {
        self.confirm = None;
        match std::fs::remove_file(path) {
            Ok(()) => {
                if self.selected.as_deref() == Some(path) {
                    self.selected = None;
                }
                if self.editor.as_ref().is_some_and(|e| e.path == path) {
                    self.editor = None;
                }
                if matches!(&self.screen, Screen::Detail(open) if open == path) {
                    self.detail_mode = DetailMode::Preview;
                    self.screen = Screen::Board;
                }
                self.refresh();
            }
            Err(err) => {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(path)));
            }
        }
    }

    /// Moves a done ticket out of the board into `roadmap/.archive`, which
    /// discovery ignores. Never overwrites an archived file.
    fn archive_ticket(&mut self, path: &Path) {
        let archive = self.roadmap.root.join(ARCHIVE_DIR);
        if let Err(err) = std::fs::create_dir_all(&archive) {
            self.error = Some(format!(
                "Couldn't create {}: {err}",
                self.relative(&archive)
            ));
            return;
        }
        let name = unique_name(&archive, &file_name(path));
        let target = archive.join(&name);
        match std::fs::rename(path, &target) {
            Ok(()) => {
                if self.selected.as_deref() == Some(path) {
                    self.selected = None;
                }
                if self.editor.as_ref().is_some_and(|e| e.path == path) {
                    self.editor = None;
                }
                if matches!(&self.screen, Screen::Detail(open) if open == path) {
                    self.detail_mode = DetailMode::Preview;
                    self.screen = Screen::Board;
                }
                self.refresh();
            }
            Err(err) => {
                self.error = Some(format!("Couldn't archive {}: {err}", self.relative(path)));
            }
        }
    }

    /// Writes a ticket the reader changed via a checkbox.
    fn write_ticket(&mut self, path: &Path, contents: &str) {
        match atomic_write(path, contents) {
            Ok(()) => self.refresh(),
            Err(err) => {
                self.error = Some(format!("Couldn't save {}: {err}", self.relative(path)));
            }
        }
    }

    /// The column new tickets land in (Todo).
    fn todo_column(&self) -> Option<&Column> {
        self.roadmap
            .columns
            .iter()
            .find(|column| column.accepts_new_tickets())
    }

    /// Opens the quick-add composer at the top of Todo, from any screen.
    fn open_composer(&mut self) {
        if self.todo_column().is_none() {
            return;
        }
        if matches!(self.screen, Screen::Board) {
            self.show_composer();
        } else {
            self.leave(Leave::Composer);
        }
    }

    fn show_composer(&mut self) {
        let composer = self.composer.get_or_insert_with(Composer::default);
        composer.focus = true;
    }

    /// Writes `# <title>` to a new file in Todo named after the title. Returns
    /// the new file's path; the caller decides whether to open it.
    fn create_ticket(&mut self, dir: &Path, title: &str) -> Result<PathBuf, String> {
        let title = title.trim();
        let (name, _) = planned_ticket_name(dir, title);
        let path = dir.join(&name);
        match atomic_write(&path, &format!("# {title}\n")) {
            Ok(()) => {
                self.refresh();
                Ok(path)
            }
            Err(err) => Err(format!(
                "Couldn't create {name} in {}: {err}",
                self.relative(dir)
            )),
        }
    }

    /// `↩` in the composer: create and stay open for the next idea. With
    /// `open_editor` (`⌘↩`), create and open the new ticket in the editor.
    fn submit_composer(&mut self, open_editor: bool) {
        let Some(title) = self
            .composer
            .as_ref()
            .map(|c| c.title.trim().to_string())
            .filter(|t| !t.is_empty())
        else {
            return;
        };
        let Some(dir) = self.todo_column().map(|c| c.dir.clone()) else {
            return;
        };
        match self.create_ticket(&dir, &title) {
            Ok(path) => {
                if open_editor {
                    self.composer = None;
                    self.start_editing(&path);
                    self.focus_editor = true;
                } else if let Some(composer) = self.composer.as_mut() {
                    composer.title.clear();
                    composer.error = None;
                    composer.focus = true;
                    self.selected = Some(path.clone());
                    self.new_card = Some((path, Instant::now()));
                }
            }
            Err(err) => {
                if let Some(composer) = self.composer.as_mut() {
                    composer.error = Some(err);
                    composer.focus = true;
                }
            }
        }
    }

    /// The tickets the board shows, column by column, after the filter.
    fn visible_grid(&self) -> Vec<Vec<PathBuf>> {
        let filter = self.filter.trim().to_lowercase();
        self.roadmap
            .columns
            .iter()
            .map(|column| {
                column
                    .tickets
                    .iter()
                    .filter(|ticket| ticket.matches(&filter))
                    .map(|ticket| ticket.path.clone())
                    .collect()
            })
            .collect()
    }

    /// Moves the board selection one step with an arrow key.
    fn move_selection(&mut self, key: Key) {
        let grid = self.visible_grid();
        let at = self.selected.as_ref().and_then(|selected| {
            grid.iter().enumerate().find_map(|(column, paths)| {
                paths
                    .iter()
                    .position(|path| path == selected)
                    .map(|row| (column, row))
            })
        });
        if let Some((column, row)) = step_selection(&grid, at, key) {
            self.selected = Some(grid[column][row].clone());
            self.scroll_to_selected = true;
        }
    }

    /// The selected ticket, if it is still on the board.
    fn selected_ticket(&self) -> Option<PathBuf> {
        self.selected
            .clone()
            .filter(|path| self.roadmap.find(path).is_some())
    }

    /// A path relative to the roadmap folder, as the file system names it.
    fn relative(&self, path: &Path) -> String {
        match path.strip_prefix(&self.roadmap.root) {
            Ok(relative) if relative.as_os_str().is_empty() => "./".to_string(),
            Ok(relative) if path.is_dir() => format!("{}/", relative.display()),
            Ok(relative) => relative.display().to_string(),
            Err(_) => display_path(path),
        }
    }

    fn project_name(&self) -> String {
        self.roadmap
            .title
            .clone()
            .or_else(|| {
                self.roadmap
                    .project_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Roadmap".to_string())
    }

    /// Draws the whole window: header bar, the current screen, status bar.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let gutter = theme::SPACE_5 as i8;
        let header_left = if UNIFIED_TITLEBAR {
            TRAFFIC_LIGHTS_WIDTH
        } else {
            gutter
        };
        egui::Panel::top("header")
            .exact_size(theme::BAR_H)
            .resizable(false)
            .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                left: header_left,
                right: gutter,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| self.header_bar(ui));
        egui::Panel::bottom("status")
            .exact_size(theme::STATUSBAR_H)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Margin::symmetric(theme::SPACE_5 as i8, 0)),
            )
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                if let Some(error) = self.error.clone() {
                    self.error_strip(ui, &error);
                }
                if let Some(Confirm::Delete(path)) = self.confirm.clone() {
                    self.confirm_strip(ui, &path);
                }
                match self.screen.clone() {
                    Screen::Board => self.board_ui(ui),
                    Screen::Detail(path) => self.detail_ui(ui, &path),
                }
            });
    }

    /// A dismissible `danger` line naming what failed.
    fn error_strip(&mut self, ui: &mut egui::Ui, error: &str) {
        let p = theme::palette(ui);
        Frame::new()
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(error).color(p.danger));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None).clicked() {
                            self.error = None;
                        }
                    });
                });
            });
    }

    /// The confirmation bar for a delete: `↩` confirms, `esc` cancels.
    fn confirm_strip(&mut self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let message = format!("Delete {}? This cannot be undone.", self.relative(path));
        let mut confirmed = false;
        let mut cancelled = false;
        Frame::new()
            .fill(p.accent_soft)
            .inner_margin(Margin::symmetric(
                theme::SPACE_5 as i8,
                theme::SPACE_3 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(message).color(p.danger));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                        if widgets::button(ui, ButtonKind::Primary, None, "Delete", Some("↩"))
                            .clicked()
                        {
                            confirmed = true;
                        }
                        if widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc"))
                            .clicked()
                        {
                            cancelled = true;
                        }
                    });
                });
            });
        if confirmed {
            self.delete_ticket(path);
        } else if cancelled {
            self.confirm = None;
        }
    }

    /// The 40pt top bar: breadcrumb on the left, the screen's controls on the
    /// right with its main action rightmost. On macOS it is also the window's
    /// title bar: dragging it moves the window, double-clicking zooms it.
    fn header_bar(&mut self, ui: &mut egui::Ui) {
        if UNIFIED_TITLEBAR {
            // Interacted with first, so the controls drawn over it win.
            let bar = ui.interact(
                ui.max_rect(),
                egui::Id::new("header_drag"),
                Sense::click_and_drag(),
            );
            if bar.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        ui.style_mut().interaction.selectable_labels = false;
        // Controls first, so the breadcrumb truncates into the space left.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            self.header_controls(ui);
            ui.add_space(theme::SPACE_5);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                self.breadcrumb(ui);
            });
        });
    }

    /// Board: the roadmap path. Reader and editor: a
    /// `‹ Board` button, then `project / Column / Ticket title`, every segment
    /// but the last a link.
    fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let project = self.project_name();
        match self.screen.clone() {
            Screen::Board => {
                ui.add(
                    egui::Label::new(
                        RichText::new(display_path(&self.roadmap.root))
                            .font(egui::FontId::monospace(11.0))
                            .color(p.ink_muted),
                    )
                    .truncate(),
                );
            }
            Screen::Detail(path) => {
                // The visible way out, for whoever does not know the shortcuts.
                let hint = if self.detail_mode == DetailMode::Edit {
                    "Back to the board (asks if unsaved)"
                } else {
                    "Back to the board (esc)"
                };
                let mut back =
                    widgets::button(ui, ButtonKind::Ghost, Some(Icon::Back), "Board", None)
                        .on_hover_text(hint)
                        .clicked();
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                let column = self
                    .roadmap
                    .columns
                    .iter()
                    .find(|c| c.tickets.iter().any(|t| t.path == path))
                    .map(|c| c.name.clone());
                let title = self
                    .roadmap
                    .find(&path)
                    .map(|t| t.title.clone())
                    .unwrap_or_else(|| file_name(&path));
                let separator = |ui: &mut egui::Ui| {
                    ui.label(RichText::new("/").color(p.ink_faint));
                };
                back |= widgets::crumb(ui, &project, theme::ui_body()).clicked();
                if let Some(column) = column {
                    separator(ui);
                    back |= widgets::crumb(ui, &column, theme::ui_body()).clicked();
                }
                separator(ui);
                ui.add(
                    egui::Label::new(RichText::new(title).font(theme::ui_strong()).color(p.ink))
                        .truncate(),
                );
                if back {
                    self.back_to_board();
                }
            }
        }
    }

    /// The header's buttons, laid out right to left.
    fn header_controls(&mut self, ui: &mut egui::Ui) {
        match self.screen {
            Screen::Board => {
                let has_todo = self.todo_column().is_some();
                let new_ticket = ui.add_enabled_ui(has_todo, |ui| {
                    widgets::button(
                        ui,
                        ButtonKind::Primary,
                        Some(Icon::Plus),
                        "New ticket",
                        Some("⌘N"),
                    )
                    .on_hover_text("Add a ticket to Todo")
                });
                if new_ticket.inner.clicked() {
                    self.open_composer();
                }
                self.refresh_button(ui);
                widgets::checkbox(ui, &mut self.auto, "Auto")
                    .on_hover_text("Reload when files change on disk, and save edits as you type");
                self.filter_field(ui);
            }
            Screen::Detail(ref path) => {
                let path = path.clone();
                let exists = self.roadmap.find(&path).is_some();
                let done = self
                    .column_of(&path)
                    .map(|column| column.name == "Done")
                    .unwrap_or(false);
                if self.detail_mode == DetailMode::Preview
                    && exists
                    && widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(Icon::Edit),
                        "Edit",
                        Some("E"),
                    )
                    .clicked()
                {
                    self.start_editing(&path);
                }
                self.refresh_button(ui);
                if exists
                    && widgets::button(ui, ButtonKind::Ghost, Some(Icon::Trash), "", None)
                        .on_hover_text("Delete this ticket (Del)")
                        .clicked()
                {
                    self.request_delete(path.clone());
                }
                if exists
                    && done
                    && self.detail_mode == DetailMode::Preview
                    && widgets::button(ui, ButtonKind::Ghost, Some(Icon::Archive), "", None)
                        .on_hover_text(format!("Move to {ARCHIVE_DIR}/"))
                        .clicked()
                {
                    self.archive_ticket(&path);
                }
            }
        }
    }

    fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if widgets::button(
            ui,
            ButtonKind::Secondary,
            Some(Icon::Refresh),
            "Refresh",
            None,
        )
        .on_hover_text("Reload from disk (⌘R)")
        .clicked()
        {
            self.refresh();
        }
    }

    /// The 220pt filter input, with its `⌘F` hint while empty.
    fn filter_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .id(egui::Id::new("filter"))
                .hint_text(RichText::new("Filter tickets").color(p.ink_faint))
                .font(theme::ui_body())
                // Room on the right for the `⌘F` key cap.
                .margin(Margin {
                    left: theme::SPACE_3 as i8,
                    right: (theme::SPACE_3 + FILTER_KBD_WIDTH) as i8,
                    top: 3,
                    bottom: 3,
                })
                .min_size(Vec2::new(0.0, theme::CONTROL_H))
                .desired_width(220.0 - 2.0 * theme::SPACE_3 - FILTER_KBD_WIDTH),
        );
        if std::mem::take(&mut self.focus_filter) {
            response.request_focus();
        }
        if self.filter.is_empty() && !response.has_focus() {
            let rect = response.rect;
            widgets::paint_kbd(
                ui,
                "⌘F",
                Pos2::new(
                    rect.max.x - FILTER_KBD_WIDTH - theme::SPACE_2,
                    rect.center().y,
                ),
            );
        }
    }

    /// The 24pt bottom strip: where you are on the left, what you can press on
    /// the right.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.style_mut().interaction.selectable_labels = false;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_5;
            let meta = |text: String| RichText::new(text).font(theme::meta()).color(p.ink_muted);
            let hints: Vec<(&str, &str)>;
            match self.screen.clone() {
                Screen::Board => {
                    let count = self.roadmap.ticket_count();
                    let noun = if count == 1 { "ticket" } else { "tickets" };
                    if self.composer.is_some()
                        && let Some(todo) = self.todo_column()
                    {
                        ui.label(meta(self.relative(&todo.dir)));
                        hints = vec![("↩", "add"), ("⌘↩", "add & edit"), ("esc", "close")];
                    } else if let Some(selected) = self.selected_ticket() {
                        ui.label(meta(self.relative(&selected)));
                        hints = vec![("↩", "open"), ("E", "edit"), ("Del", "delete")];
                    } else {
                        hints = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
                    }
                    ui.label(meta(format!("{count} {noun}")));
                    if !self.roadmap.warnings.is_empty() {
                        let n = self.roadmap.warnings.len();
                        let noun = if n == 1 { "warning" } else { "warnings" };
                        ui.label(
                            RichText::new(format!("{n} {noun}"))
                                .font(theme::meta())
                                .color(p.status_doing),
                        )
                        .on_hover_text(self.roadmap.warnings.join("\n"));
                    }
                }
                Screen::Detail(path) => {
                    ui.label(meta(self.relative(&path)));
                    if self.detail_mode == DetailMode::Edit {
                        self.save_pill(ui, &path);
                        hints = if self.leave_prompt.is_some() {
                            vec![("esc", "keep editing")]
                        } else {
                            vec![("⌘S", "save"), ("esc", "leave")]
                        };
                    } else {
                        if let Some(modified) = self.roadmap.find(&path).and_then(|t| t.modified) {
                            ui.label(meta(format!("modified {}", relative_time(modified))));
                        }
                        hints = vec![("E", "edit"), ("Del", "delete"), ("esc", "back")];
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                for (key, action) in hints.into_iter().rev() {
                    ui.horizontal(|ui| widgets::hint(ui, key, action));
                }
            });
        });
    }

    /// Saved / Unsaved / Save failed for the ticket being edited.
    fn save_pill(&self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let Some(editor) = self.editor.as_ref().filter(|e| e.path == path) else {
            return;
        };
        if editor.error.is_some() {
            widgets::pill(ui, "Save failed", p.danger);
        } else if editor.conflict.is_some() {
            widgets::pill(ui, "Changed on disk", p.danger);
        } else if editor.is_dirty() {
            widgets::pill(ui, "Unsaved", p.status_doing);
        } else {
            widgets::pill(ui, "Saved", p.status_done);
        }
    }

    fn board_ui(&mut self, ui: &mut egui::Ui) {
        if self.roadmap.columns.is_empty() {
            Frame::new()
                .inner_margin(Margin::same(theme::SPACE_5 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("No columns found in this roadmap.")
                            .color(theme::palette(ui).ink_faint),
                    );
                });
            return;
        }

        // The board is a set of click/drag surfaces, not a text document: no
        // I-beam, no text selection anywhere on it.
        ui.style_mut().interaction.selectable_labels = false;

        let gutter = theme::SPACE_5;
        let count = self.roadmap.columns.len() as f32;
        let available = ui.available_width() - 2.0 * gutter - (count - 1.0) * gutter;
        let width = (available / count).clamp(theme::COLUMN_MIN, theme::COLUMN_MAX);
        let height = (ui.available_height() - 2.0 * gutter).max(120.0);
        let filter = self.filter.trim().to_lowercase();
        let new_card = self
            .new_card
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NEW_CARD_FLASH)
            .map(|(path, _)| path.clone());
        if new_card.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        let selected = self.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);

        let mut clicked: Option<PathBuf> = None;
        let mut dropped: Option<(PathBuf, PathBuf, Option<PathBuf>)> = None;
        let mut delete: Option<PathBuf> = None;
        let mut archive: Option<PathBuf> = None;
        let mut compose = false;
        let mut submit: Option<bool> = None;
        let mut close_composer = false;

        egui::ScrollArea::horizontal()
            .id_salt("board")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(gutter);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gutter;
                    // The window gutter on the left; item spacing after the last
                    // column gives the same on the right.
                    ui.add_space(gutter);
                    for (index, column) in self.roadmap.columns.iter().enumerate() {
                        let composer = if column.accepts_new_tickets() {
                            self.composer.as_mut()
                        } else {
                            None
                        };
                        let view = ColumnView {
                            column,
                            index,
                            filter: &filter,
                            width,
                            height,
                            new_card: new_card.as_deref(),
                            selected: selected.as_deref(),
                            scroll_to_selected,
                            root: &self.roadmap.root,
                        };
                        let actions = column_ui(ui, &view, composer);
                        if let Some(found) = actions.clicked {
                            clicked = Some(found);
                        }
                        if let Some(path) = actions.dropped {
                            dropped = Some((path, column.dir.clone(), actions.drop_anchor));
                        }
                        if delete.is_none() {
                            delete = actions.delete;
                        }
                        if archive.is_none() {
                            archive = actions.archive;
                        }
                        compose |= actions.compose;
                        submit = submit.or(actions.submit);
                        close_composer |= actions.close_composer;
                    }
                    // Ends the scrollable width one gutter after the last column.
                    ui.allocate_space(Vec2::ZERO);
                });
            });

        if let Some((path, target, anchor)) = dropped {
            self.place_ticket(&path, &target, anchor);
        }
        if let Some(path) = delete {
            self.request_delete(path);
        }
        if let Some(path) = archive {
            self.archive_ticket(&path);
        }
        if compose {
            self.open_composer();
        }
        if let Some(open_editor) = submit {
            self.submit_composer(open_editor);
        } else if close_composer {
            self.composer = None;
        }
        if let Some(path) = clicked {
            self.open_ticket(path);
        }
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        if self.roadmap.find(path).is_none() {
            page(ui, |ui| {
                ui.label(
                    RichText::new(
                        "This ticket no longer exists. It may have been moved or deleted.",
                    )
                    .color(theme::palette(ui).ink_muted),
                );
            });
            return;
        }

        match self.detail_mode {
            DetailMode::Preview => self.detail_preview(ui, path),
            DetailMode::Edit => self.detail_edit(ui, path),
        }
    }

    /// Rendered markdown on a `surface` page. Checkboxes are live: ticking one
    /// writes the file. Double-clicking the page opens the editor.
    fn detail_preview(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(ticket) = self.roadmap.find(path) else {
            return;
        };
        let mut text = ticket.markdown.clone();

        // The rendered reader is not a text surface: keep labels inert so a
        // double-click reliably opens the editor instead of selecting a word.
        ui.style_mut().interaction.selectable_labels = false;

        let mut page_rect = Rect::NOTHING;
        let changed = {
            let cache = &mut self.cache;
            egui::ScrollArea::vertical()
                .id_salt(("detail", path))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let response = page(ui, |ui| {
                        // Reading sizes: `read-body` 14, `read-h1` 20.
                        let styles = &mut ui.style_mut().text_styles;
                        styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
                        styles.insert(egui::TextStyle::Heading, theme::read_h1());
                        styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
                        ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                        CommonMarkViewer::new()
                            .show_mut(ui, cache, &mut text)
                            .response
                            .changed()
                    });
                    page_rect = response.response.rect;
                    response.inner
                })
                .inner
        };

        // Hovering the page shows that it is one edit target.
        let p = theme::palette(ui);
        if ui.rect_contains_pointer(page_rect) {
            let hint = egui::RichText::new("Double-click to edit")
                .font(egui::FontId::new(11.0, theme::ui_button().family))
                .color(p.ink_muted);
            let galley = egui::WidgetText::from(hint).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::FontSelection::Default,
            );
            let kbd_width = 18.0;
            let right = page_rect.max.x - theme::SPACE_4;
            let y = page_rect.min.y + theme::SPACE_3 + 8.0;
            let kbd_left = right - kbd_width;
            widgets::paint_kbd(ui, "E", Pos2::new(kbd_left, y));
            let text_pos = Pos2::new(
                kbd_left - theme::SPACE_2 - galley.size().x,
                y - galley.size().y / 2.0,
            );
            ui.painter().galley(text_pos, galley, p.ink_muted);
        }
        self.preview_rect = page_rect;

        if changed {
            self.write_ticket(path, &text);
        }
    }

    /// The editor: the same page as the reader, in plain text, framed in
    /// `accent`. Above it: the file name, save state, Discard and Save.
    fn detail_edit(&mut self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let focus_editor = std::mem::take(&mut self.focus_editor);
        let folder = path
            .parent()
            .map(|dir| self.relative(dir))
            .unwrap_or_default();
        let mut reload = false;
        let mut keep = false;
        let mut save = false;
        let mut discard = false;
        let mut answer: Option<LeaveChoice> = None;
        let prompting = self.leave_prompt.is_some();

        page_column(ui, |ui| {
            let Some(editor) = self.editor.as_mut() else {
                ui.label("Nothing is being edited.");
                return;
            };

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new(folder).font(theme::meta()).color(p.ink_muted));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut editor.name)
                        .font(theme::meta())
                        .text_color(p.ink)
                        .hint_text("ticket.md")
                        .char_limit(MAX_FILE_NAME_BYTES)
                        .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                        .desired_width(240.0),
                );
                if response.changed() {
                    editor.last_edit = Some(Instant::now());
                    ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    save = widgets::button(ui, ButtonKind::Primary, None, "Save", Some("⌘S"))
                        .clicked();
                    discard = widgets::button(ui, ButtonKind::Secondary, None, "Discard", None)
                        .on_hover_text("Drop unsaved changes and go back to the ticket")
                        .clicked();
                });
            });

            if prompting {
                ui.add_space(theme::SPACE_3);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    ui.label(
                        RichText::new(format!("Save changes to {}?", editor.baseline_name))
                            .font(theme::ui_strong())
                            .color(p.ink),
                    );
                    if widgets::button(ui, ButtonKind::Primary, None, "Save", None).clicked() {
                        answer = Some(LeaveChoice::Save);
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Discard", None)
                        .on_hover_text("Drop your unsaved changes and leave")
                        .clicked()
                    {
                        answer = Some(LeaveChoice::Discard);
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Keep editing", Some("esc"))
                        .clicked()
                    {
                        answer = Some(LeaveChoice::KeepEditing);
                    }
                });
            }
            if let Some(error) = &editor.error {
                ui.add_space(theme::SPACE_2);
                ui.label(RichText::new(error).color(p.danger));
            }
            if editor.conflict.is_some() {
                ui.add_space(theme::SPACE_2);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} changed on disk since you started editing.",
                            editor.baseline_name
                        ))
                        .color(p.danger),
                    );
                    reload = widgets::button(ui, ButtonKind::Secondary, None, "Reload", None)
                        .on_hover_text("Drop your edits and load the file from disk")
                        .clicked();
                    keep = widgets::button(ui, ButtonKind::Secondary, None, "Keep mine", None)
                        .on_hover_text("Overwrite the file on disk with your version")
                        .clicked();
                });
            }
            ui.add_space(theme::SPACE_3);

            let rows = ((ui.available_height() - 2.0 * theme::SPACE_5) / 20.0).max(8.0) as usize;
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(2.0, p.accent))
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .inner_margin(Margin::symmetric(20, theme::SPACE_5 as i8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("editor_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let id = egui::Id::new("editor_body");
                            let mut output = egui::TextEdit::multiline(&mut editor.text)
                                .id(id)
                                .font(theme::edit_body())
                                .code_editor()
                                .frame(Frame::NONE)
                                .margin(Margin::ZERO)
                                .desired_width(f32::INFINITY)
                                .desired_rows(rows)
                                .show(ui);
                            if focus_editor {
                                // A freshly created ticket: caret after the title.
                                let end = CCursor::new(editor.text.chars().count());
                                output
                                    .state
                                    .cursor
                                    .set_char_range(Some(CCursorRange::one(end)));
                                output.state.store(ui.ctx(), id);
                                output.response.request_focus();
                            }
                            if output.response.changed() {
                                editor.last_edit = Some(Instant::now());
                                ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                            }
                        });
                });
        });

        if reload {
            self.reload_editor_from_disk();
        }
        if keep {
            self.flush_autosave(true);
        }
        if save {
            self.flush_autosave(true);
            if self.editor.as_ref().is_some_and(|e| e.error.is_none()) {
                self.detail_mode = DetailMode::Preview;
            }
        }
        if discard {
            self.discard_edits();
        }
        if let Some(choice) = answer {
            self.answer_leave(choice);
        }
    }
}

impl eframe::App for RoadmapApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let command = |key: Key| ctx.input(|i| i.modifiers.command && i.key_pressed(key));

        // Closing the window is one more way of leaving the editor.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.leave(Leave::Quit);
            if !self.closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if std::mem::take(&mut self.send_close) && !close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // The delete confirmation owns Escape and Enter while it is up.
        let confirming = self.confirm.is_some();
        if confirming {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.confirm = None;
            } else if ctx.input(|i| i.key_pressed(Key::Enter))
                && let Some(Confirm::Delete(path)) = self.confirm.clone()
            {
                self.delete_ticket(&path);
            }
        }

        // Escape steps back one level.
        if !confirming && ctx.input(|i| i.key_pressed(Key::Escape)) {
            match self.screen.clone() {
                _ if self.leave_prompt.is_some() => self.answer_leave(LeaveChoice::KeepEditing),
                Screen::Board if self.composer.is_some() => self.composer = None,
                Screen::Board if !self.filter.is_empty() => self.filter.clear(),
                Screen::Board => self.selected = None,
                Screen::Detail(_) if self.detail_mode == DetailMode::Edit => {
                    self.show_preview();
                }
                Screen::Detail(_) => self.back_to_board(),
            }
        }

        if command(Key::N) {
            self.open_composer();
        }
        if command(Key::F) && matches!(self.screen, Screen::Board) {
            self.focus_filter = true;
        }
        if command(Key::R) {
            self.refresh();
        }
        if command(Key::OpenBracket) {
            match self.screen {
                Screen::Detail(_) if self.detail_mode == DetailMode::Edit => self.show_preview(),
                Screen::Detail(_) => self.back_to_board(),
                Screen::Board => {}
            }
        }

        // Cmd/Ctrl+S forces a save (and overwrites a conflict).
        if command(Key::S) {
            self.flush_autosave(true);
        }
        // Saved some other way while the prompt was up: carry on leaving.
        if let Some(to) = self.leave_prompt
            && !self.has_unsaved_edits()
        {
            self.go(to);
        }

        // On the board, the arrow keys select a card; `↩` opens it, `E` edits.
        if matches!(self.screen, Screen::Board) && !typing && !confirming {
            let no_modifiers = ctx.input(|i| i.modifiers.is_none());
            for key in [
                Key::ArrowUp,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowRight,
            ] {
                if no_modifiers && ctx.input(|i| i.key_pressed(key)) {
                    self.move_selection(key);
                }
            }
            if let Some(path) = self.selected_ticket() {
                if no_modifiers && ctx.input(|i| i.key_pressed(Key::Enter)) {
                    self.open_ticket(path);
                } else if no_modifiers && ctx.input(|i| i.key_pressed(Key::E)) {
                    self.start_editing(&path);
                }
            }
        }

        // `Del` (or Backspace) asks to delete the selected card, or the ticket
        // the reader is showing. In the editor the key belongs to the text.
        if !confirming && !typing && self.confirm.is_none() {
            let delete_key = ctx.input(|i| {
                (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
                    && i.modifiers.is_none()
            });
            if delete_key {
                match self.screen.clone() {
                    Screen::Board => {
                        if let Some(path) = self.selected_ticket() {
                            self.request_delete(path);
                        }
                    }
                    Screen::Detail(path) if self.detail_mode == DetailMode::Preview => {
                        self.request_delete(path);
                    }
                    _ => {}
                }
            }
        }

        if self.detail_mode == DetailMode::Preview
            && let Screen::Detail(path) = self.screen.clone()
        {
            // `E`, or a double-click on the page (not the header), edits.
            let edit_key = !typing && ctx.input(|i| i.key_pressed(Key::E) && i.modifiers.is_none());
            let double_clicked_page = ctx.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
                    && i.pointer
                        .interact_pos()
                        .map(|pos| self.preview_rect.contains(pos))
                        .unwrap_or(false)
            });
            if edit_key || double_clicked_page {
                self.start_editing(&path);
            }
        }

        // Debounced autosave, while Auto is on.
        let due = self.auto
            && self
                .editor
                .as_ref()
                .map(|e| {
                    e.is_dirty()
                        && e.conflict.is_none()
                        && e.last_edit
                            .map(|t| t.elapsed() >= AUTOSAVE_AFTER)
                            .unwrap_or(false)
                })
                .unwrap_or(false);
        if due {
            self.flush_autosave(false);
        }

        if self.auto {
            if self.last_refresh.elapsed() >= Duration::from_millis(2000) {
                self.last_refresh = Instant::now();
                self.refresh();
            }
            ctx.request_repaint_after(Duration::from_millis(2000));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

/// Everything `column_ui` needs to know about one column.
struct ColumnView<'a> {
    column: &'a Column,
    index: usize,
    filter: &'a str,
    width: f32,
    height: f32,
    /// A ticket just created by the composer, still highlighted.
    new_card: Option<&'a Path>,
    /// The card selected with the arrow keys or by opening it.
    selected: Option<&'a Path>,
    /// Scroll the selected card into view this frame.
    scroll_to_selected: bool,
    root: &'a Path,
}

/// The status color of a column, from its folder's role.
fn status_color(p: &Palette, column: &Column) -> Color32 {
    match column.name.to_lowercase().as_str() {
        "ready" => p.status_ready,
        "doing" | "in progress" | "in-progress" | "wip" | "review" => p.status_doing,
        "done" => p.status_done,
        _ => p.status_todo,
    }
}

fn column_ui(
    ui: &mut egui::Ui,
    view: &ColumnView,
    composer: Option<&mut Composer>,
) -> ColumnActions {
    let ColumnView {
        column,
        index,
        filter,
        width,
        height,
        new_card,
        selected,
        scroll_to_selected,
        root,
    } = *view;
    let p = theme::palette(ui);
    let mut actions = ColumnActions::default();
    let visible: Vec<&Ticket> = column
        .tickets
        .iter()
        .filter(|ticket| ticket.matches(filter))
        .collect();
    // `todo/`, or empty for the tickets at the roadmap root.
    let folder = match column.dir.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => format!("{}/", relative.display()),
        _ => String::new(),
    };

    // Reserve the whole column as a drop target before drawing into it, so the
    // empty space below the cards accepts a drop too.
    let origin = ui.cursor().min;
    let drop_rect = Rect::from_min_size(origin, Vec2::new(width, height));
    let drop_response = ui.interact(
        drop_rect,
        egui::Id::new(("column_drop", index)),
        Sense::hover(),
    );

    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
            let status = status_color(p, column);

            // Header: dot, uppercase name, count, Todo's `+`, 2pt status rule.
            let (head_rect, _) =
                ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::hover());
            let mut head = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(head_rect.shrink2(Vec2::new(theme::SPACE_2, 0.0)))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            head.spacing_mut().item_spacing.x = theme::SPACE_3;
            let (dot, _) = head.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            head.painter().circle_filled(dot.center(), 4.0, status);
            head.label(
                RichText::new(column.name.to_uppercase())
                    .font(theme::ui_label())
                    .extra_letter_spacing(theme::LABEL_TRACKING)
                    .color(p.ink),
            );
            head.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                if column.accepts_new_tickets()
                    && widgets::button(ui, ButtonKind::Ghost, Some(Icon::Plus), "", None)
                        .on_hover_text("New ticket in Todo (⌘N)")
                        .clicked()
                {
                    actions.compose = true;
                }
                let count = if filter.is_empty() {
                    column.tickets.len().to_string()
                } else {
                    format!("{}/{}", visible.len(), column.tickets.len())
                };
                ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
            });
            ui.painter().line_segment(
                [
                    Pos2::new(head_rect.min.x, head_rect.max.y - 1.0),
                    Pos2::new(head_rect.max.x, head_rect.max.y - 1.0),
                ],
                Stroke::new(2.0, status),
            );

            egui::ScrollArea::vertical()
                .id_salt(("column", index))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    if let Some(composer) = composer {
                        composer_ui(ui, composer, column, &folder, &mut actions);
                    }
                    if visible.is_empty() {
                        let message = if column.tickets.is_empty() {
                            if folder.is_empty() {
                                "No tickets in the roadmap folder".to_string()
                            } else {
                                format!("No tickets in {folder}")
                            }
                        } else {
                            format!("No tickets match \"{filter}\"")
                        };
                        empty_well(ui, &message, width);
                    }
                    let mut cards: Vec<(PathBuf, Rect)> = Vec::new();
                    for ticket in visible {
                        let fresh = new_card == Some(ticket.path.as_path());
                        let look = if selected == Some(ticket.path.as_path()) {
                            CardLook::Selected {
                                scroll: scroll_to_selected,
                            }
                        } else if fresh {
                            CardLook::Fresh
                        } else {
                            CardLook::Plain
                        };
                        let (action, rect) = ticket_card(ui, ticket, width, look);
                        cards.push((ticket.path.clone(), rect));
                        match action {
                            Some(CardAction::Open) => {
                                actions.clicked = Some(ticket.path.clone());
                            }
                            Some(CardAction::Delete) => {
                                actions.delete = Some(ticket.path.clone());
                            }
                            Some(CardAction::Archive) => {
                                actions.archive = Some(ticket.path.clone());
                            }
                            None => {}
                        }
                    }

                    // Where the dragged card would land: before the first card
                    // whose middle is below the pointer, else last.
                    if drop_response.dnd_hover_payload::<TicketDrag>().is_some()
                        || drop_response.dnd_release_payload::<TicketDrag>().is_some()
                    {
                        let pointer = ui
                            .ctx()
                            .pointer_interact_pos()
                            .or(ui.ctx().pointer_hover_pos());
                        let insert = pointer
                            .map(|pos| {
                                cards
                                    .iter()
                                    .position(|(_, rect)| pos.y < rect.center().y)
                                    .unwrap_or(cards.len())
                            })
                            .unwrap_or(cards.len());
                        actions.drop_anchor = cards.get(insert).map(|(path, _)| path.clone());
                        let line_y = cards
                            .get(insert)
                            .map(|(_, rect)| rect.min.y - theme::SPACE_3 / 2.0)
                            .or_else(|| {
                                cards
                                    .last()
                                    .map(|(_, rect)| rect.max.y + theme::SPACE_3 / 2.0)
                            });
                        if let (Some(y), Some((_, first))) = (line_y, cards.first()) {
                            ui.painter().line_segment(
                                [Pos2::new(first.min.x, y), Pos2::new(first.max.x, y)],
                                Stroke::new(2.0, p.accent),
                            );
                        }
                    }
                });
        },
    );

    // Highlight the column while a card from elsewhere hovers over it.
    let hovering_foreign = drop_response
        .dnd_hover_payload::<TicketDrag>()
        .filter(|drag| drag.path.parent() != Some(column.dir.as_path()))
        .is_some();
    if hovering_foreign {
        let rect = drop_rect.expand(theme::SPACE_2);
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme::RADIUS_MD),
            p.accent_soft.gamma_multiply(0.5),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::RADIUS_MD),
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    }

    if let Some(drag) = drop_response.dnd_release_payload::<TicketDrag>() {
        actions.dropped = Some(drag.path.clone());
    }

    actions
}

/// The dashed `surface-sunken` well of an empty column. It is also where a
/// dragged card lands.
fn empty_well(ui: &mut egui::Ui, message: &str, width: f32) {
    let p = theme::palette(ui);
    let response = Frame::new()
        .fill(p.surface_sunken)
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::same(theme::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(width - 2.0 * theme::SPACE_4);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(message)
                        .font(theme::ui_small())
                        .color(p.ink_faint),
                );
            });
        })
        .response;
    let r = response.rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    ui.painter().extend(Shape::dashed_line(
        &corners,
        Stroke::new(1.0, p.line_strong),
        4.0,
        3.0,
    ));
}

/// The quick-add composer: a card-shaped title field framed in `accent`, with
/// the file it will write underneath.
fn composer_ui(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    column: &Column,
    folder: &str,
    actions: &mut ColumnActions,
) {
    let p = theme::palette(ui);
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(2.0, p.accent))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            let response = ui.add(
                egui::TextEdit::singleline(&mut composer.title)
                    .id(egui::Id::new("composer"))
                    .font(theme::ui_strong())
                    .hint_text(
                        RichText::new("Ticket title")
                            .font(theme::ui_body())
                            .color(p.ink_faint),
                    )
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut composer.focus) {
                response.request_focus();
            }
            // `↩` makes the field give up focus; `⌘↩` may not, so check both.
            let (enter, command) = ui.input(|i| (i.key_pressed(Key::Enter), i.modifiers.command));
            let empty = composer.title.trim().is_empty();
            if enter && !empty && (response.has_focus() || response.lost_focus()) {
                actions.submit = Some(command);
            } else if response.lost_focus() && empty && !enter {
                // Clicking elsewhere closes an empty composer; one with text stays.
                actions.close_composer = true;
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                let title = composer.title.trim();
                if let Some(error) = &composer.error {
                    ui.add(
                        egui::Label::new(RichText::new(error).font(theme::meta()).color(p.danger))
                            .truncate(),
                    );
                    return;
                }
                let hints: &[(&str, &str)] = if title.is_empty() {
                    &[("esc", "close")]
                } else {
                    &[("↩", "add"), ("⌘↩", "edit")]
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (key, action) in hints.iter().rev() {
                        ui.horizontal(|ui| widgets::hint(ui, key, action));
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let (text, color) = if title.is_empty() {
                            (folder.to_string(), p.ink_muted)
                        } else {
                            let (name, taken) = planned_ticket_name(&column.dir, title);
                            let color = if taken { p.status_doing } else { p.ink_muted };
                            (format!("{folder}{name}"), color)
                        };
                        ui.add(
                            egui::Label::new(RichText::new(text).font(theme::meta()).color(color))
                                .truncate(),
                        );
                    });
                });
            });
        });
}

/// How a card stands out from the others.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CardLook {
    Plain,
    /// Just created by the composer: `accent-soft` for a moment.
    Fresh,
    /// Selected: `accent` edge on `accent-soft`; `scroll` brings it into view.
    Selected {
        scroll: bool,
    },
}

/// Draws one ticket card. A single click reads it; dragging the card carries it
/// to another column, with a copy following the cursor. Returns the action the
/// card asked for (if any) and the rect it occupies.
fn ticket_card(
    ui: &mut egui::Ui,
    ticket: &Ticket,
    width: f32,
    look: CardLook,
) -> (Option<CardAction>, Rect) {
    let p = theme::palette(ui);
    let card_id = egui::Id::new(("ticket_card", &ticket.path));

    // The card is one click-and-drag surface. Labels must not steal the pointer
    // to select text (which also showed an I-beam cursor and hijacked the drag).
    ui.style_mut().interaction.selectable_labels = false;

    let scope = ui.scope_builder(
        egui::UiBuilder::new()
            .id(card_id)
            .sense(Sense::click_and_drag()),
        |ui| card_body(ui, ticket, width, look != CardLook::Plain, true),
    );
    let body = scope.inner;
    let response = scope.response;

    // Dragging the card carries its file path; a drop on a column moves it.
    response.dnd_set_drag_payload(TicketDrag {
        path: ticket.path.clone(),
    });

    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        // Paint a copy of the card under the pointer, so the ticket stays
        // visible for the whole move instead of only at its ends. Anchoring it
        // to the original card position plus the total drag delta keeps the
        // pointer at the same spot on the card where it was grabbed.
        if let Some(delta) = response.total_drag_delta() {
            egui::Area::new(egui::Id::new(("ticket_ghost", &ticket.path)))
                .order(egui::Order::Tooltip)
                .fixed_pos(response.rect.min + delta)
                .constrain(false)
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    ui.style_mut().interaction.selectable_labels = false;
                    ui.set_max_width(width);
                    let ghost = card_body(ui, ticket, width, false, false);
                    ui.painter().rect_stroke(
                        ghost.response.rect,
                        CornerRadius::same(theme::RADIUS_MD),
                        Stroke::new(2.0, p.accent),
                        StrokeKind::Inside,
                    );
                });
        }
    } else if response.hovered() {
        // Hand cursor: this card can be clicked or picked up.
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let edge = match look {
        CardLook::Selected { .. } => Some(Stroke::new(2.0, p.accent)),
        _ if response.hovered() && !response.dragged() => Some(Stroke::new(1.0, p.line_strong)),
        _ => None,
    };
    if let Some(edge) = edge {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::same(theme::RADIUS_MD),
            edge,
            StrokeKind::Inside,
        );
    }
    if look == (CardLook::Selected { scroll: true }) {
        response.scroll_to_me(None);
    }

    // The action icons are their own click targets; a click on one must not
    // also open the ticket underneath it.
    let over_action = ui
        .ctx()
        .pointer_interact_pos()
        .is_some_and(|pos| body.action_rects.iter().any(|rect| rect.contains(pos)));
    let clicked_open = (response.clicked() || response.double_clicked()) && !over_action;
    let action = body.action.or(clicked_open.then_some(CardAction::Open));

    (action, response.rect)
}

/// What `card_body` drew: the frame response, the action a child button asked
/// for, and the rects those buttons cover.
struct CardBody {
    response: egui::Response,
    action: Option<CardAction>,
    action_rects: Vec<Rect>,
}

/// The visual body of a card, shared by the card in the board and the ghost
/// that follows the pointer while dragging: title, summary clamped to three
/// lines, file name. A `highlighted` card (new or selected) is `accent-soft`.
/// With `actions`, the meta row carries the delete (and, for Done, archive)
/// icon buttons.
fn card_body(
    ui: &mut egui::Ui,
    ticket: &Ticket,
    width: f32,
    highlighted: bool,
    actions: bool,
) -> CardBody {
    let p = theme::palette(ui);
    let padding = Margin::symmetric(theme::SPACE_4 as i8, 10);
    let border = 1.0;
    // The frame's border is drawn outside the padding, so take it off too.
    let inner = width - f32::from(padding.left) - f32::from(padding.right) - 2.0 * border;
    let shown = Frame::new()
        .fill(if highlighted {
            p.accent_soft
        } else {
            p.surface
        })
        .stroke(Stroke::new(border, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            ui.add(
                egui::Label::new(
                    RichText::new(&ticket.title)
                        .font(theme::ui_strong())
                        .color(p.ink),
                )
                .wrap(),
            );
            if !ticket.summary.is_empty() {
                let mut job = LayoutJob::simple(
                    ticket.summary.clone(),
                    theme::ui_small(),
                    p.ink_muted,
                    inner,
                );
                job.wrap.max_rows = 3;
                ui.add(egui::Label::new(job));
            }
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut action = None;
                    let mut action_rects = Vec::new();
                    if actions {
                        let delete = widgets::icon_button(
                            ui,
                            Icon::Trash,
                            egui::Id::new(("card_delete", &ticket.path)),
                            "Delete ticket (Del)",
                        );
                        action_rects.push(delete.rect);
                        if delete.clicked() {
                            action = Some(CardAction::Delete);
                        }
                        if ticket.done_at.is_some() {
                            let archive = widgets::icon_button(
                                ui,
                                Icon::Archive,
                                egui::Id::new(("card_archive", &ticket.path)),
                                &format!("Move to {ARCHIVE_DIR}/"),
                            );
                            action_rects.push(archive.rect);
                            if archive.clicked() {
                                action = Some(CardAction::Archive);
                            }
                        }
                    }
                    // Done tickets: the day on the right, the exact time on hover.
                    if let Some(done_at) = ticket.done_at {
                        ui.add(egui::Label::new(
                            RichText::new(done_at.format("%Y-%m-%d").to_string())
                                .font(theme::meta())
                                .color(p.ink_muted),
                        ))
                        .on_hover_text(done_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(ticket.file_name())
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                    (action, action_rects)
                })
                .inner
            })
            .inner
        });
    CardBody {
        response: shown.response,
        action: shown.inner.0,
        action_rects: shown.inner.1,
    }
}

/// Lays `add_contents` out in a column `reader-max` wide, centred, with the
/// window gutter around it.
fn page_column<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let gutter = theme::SPACE_5;
    let width = (ui.available_width() - 2.0 * gutter).min(theme::READER_MAX);
    let left = ((ui.available_width() - width) / 2.0).max(gutter);
    let rect = Rect::from_min_size(
        ui.cursor().min + Vec2::new(left, gutter),
        Vec2::new(width, (ui.available_height() - 2.0 * gutter).max(0.0)),
    );
    let result = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
            add_contents,
        )
        .inner;
    ui.add_space(gutter);
    result
}

/// The reader's `surface` page: `reader-max` wide, centred, 24×32 padding.
fn page<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    page_column(ui, |ui| {
        let p = theme::palette(ui);
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Margin::symmetric(
                theme::SPACE_7 as i8,
                theme::SPACE_6 as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_contents(ui)
            })
    })
}

/// The next selected `(column, row)` after an arrow key on the board's visible
/// tickets. With nothing selected, any arrow selects the first card. Up and down
/// stop at the column's ends; left and right skip empty columns and keep the
/// row when the next column is long enough.
fn step_selection(
    grid: &[Vec<PathBuf>],
    at: Option<(usize, usize)>,
    key: Key,
) -> Option<(usize, usize)> {
    let Some((column, row)) = at else {
        return grid
            .iter()
            .position(|paths| !paths.is_empty())
            .map(|column| (column, 0));
    };
    let land = |column: usize| (column, row.min(grid[column].len() - 1));
    match key {
        Key::ArrowUp => Some((column, row.saturating_sub(1))),
        Key::ArrowDown => Some((column, (row + 1).min(grid[column].len() - 1))),
        Key::ArrowLeft => (0..column)
            .rev()
            .find(|&c| !grid[c].is_empty())
            .map(land)
            .or(at),
        Key::ArrowRight => (column + 1..grid.len())
            .find(|&c| !grid[c].is_empty())
            .map(land)
            .or(at),
        _ => at,
    }
}

/// Turns a title into a file stem: lowercase ASCII words joined by hyphens, at
/// most 60 characters. A title with no ASCII letters or digits gives `ticket`.
fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for word in title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        let word = word.to_ascii_lowercase();
        let extra = if slug.is_empty() {
            word.len()
        } else {
            word.len() + 1
        };
        if slug.len() + extra > MAX_SLUG_LEN {
            if slug.is_empty() {
                slug = word[..MAX_SLUG_LEN].to_string();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    if slug.is_empty() {
        "ticket".to_string()
    } else {
        slug
    }
}

/// The file name the composer will write for `title` in `dir`, and whether the
/// plain slug was taken (so a `-2`, `-3`... suffix was added).
fn planned_ticket_name(dir: &Path, title: &str) -> (String, bool) {
    let slug = slugify(title);
    let mut candidate = format!("{slug}.md");
    let mut n = 2;
    while dir.join(&candidate).exists() {
        candidate = format!("{slug}-{n}.md");
        n += 1;
    }
    (candidate, n > 2)
}

/// Writes via a hidden temp file and rename, so a crash cannot leave a
/// half-written ticket. The temp name starts with `.` and is ignored by discovery.
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ticket.md");
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));

    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&temp, path)
}

/// Shortens absolute paths under the home directory to `~/...`.
fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}

/// Human-friendly "time since" for the status bar.
fn relative_time(time: std::time::SystemTime) -> String {
    match time.elapsed() {
        Ok(duration) => {
            let secs = duration.as_secs();
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86_400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86_400)
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context with the design system installed, past the first frame so
    /// the Plex font families are loaded.
    fn themed_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "roadmap-viewer-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_content_without_leaving_temp_files() {
        let dir = scratch_dir("atomic");
        let file = dir.join("ticket.md");
        std::fs::write(&file, "old").unwrap();

        atomic_write(&file, "new content").unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dirty_tracks_baseline() {
        let mut editor = EditorState::new(PathBuf::from("t.md"), "hello".to_string());
        assert!(!editor.is_dirty());
        editor.text.push_str(" world");
        assert!(editor.is_dirty());
        editor.text = "hello".to_string();
        assert!(!editor.is_dirty());
    }

    /// Drives the board, the composer, the checkbox reader and the editor through a real
    /// egui pass, so runtime mistakes surface without a display.
    #[test]
    fn renders_board_reader_and_editor() {
        let dir = scratch_dir("render");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\n- [ ] one\n- [ ] two\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        let ctx = themed_ctx();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };

        let pass = |app: &mut RoadmapApp| {
            let mut output = ctx.run_ui(raw.clone(), |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };

        pass(&mut app); // board

        app.open_ticket(file.clone());
        pass(&mut app); // rendered preview with mutable checkboxes

        app.start_editing(&file);
        pass(&mut app); // editor

        app.screen = Screen::Board;
        app.open_composer();
        app.composer.as_mut().unwrap().title = "Export board".to_string();
        pass(&mut app); // board with the quick-add composer open

        app.composer = None;
        app.move_selection(Key::ArrowDown);
        pass(&mut app); // board with a selected card

        app.auto = false;
        app.start_editing(&file);
        app.editor.as_mut().unwrap().text.push_str("more\n");
        app.show_preview();
        pass(&mut app); // editor asking before leaving

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn moving_a_ticket_reparents_the_file_and_refreshes_the_board() {
        let dir = scratch_dir("move");
        let todo = dir.join("roadmap/todo");
        let doing = dir.join("roadmap/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        assert!(app.roadmap.find(&file).is_some());

        app.move_ticket(&file, &doing);

        assert!(!file.exists(), "source should be gone");
        let moved = doing.join("01-demo.md");
        assert!(moved.exists(), "file should live in the target column");
        assert!(app.error.is_none(), "a clean move reports no error");
        assert_eq!(
            app.roadmap.find(&moved).map(|ticket| ticket.title.as_str()),
            Some("Demo"),
            "the board adopts the moved ticket"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_ticket_is_numbered_in_ready_and_unnumbered_when_taken() {
        let dir = scratch_dir("ready");
        let todo = dir.join("roadmap/todo");
        let ready = dir.join("roadmap/ready");
        let doing = dir.join("roadmap/doing");
        for d in [&todo, &ready, &doing] {
            std::fs::create_dir_all(d).unwrap();
        }
        let file = todo.join("make-colour-green.md");
        std::fs::write(&file, "# Green\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        app.selected = Some(file.clone());

        app.move_ticket(&file, &ready);
        let queued = ready.join("001-make-colour-green.md");
        assert!(queued.exists(), "entering Ready adds the priority number");
        assert_eq!(app.selected.as_deref(), Some(queued.as_path()));

        app.move_ticket(&queued, &doing);
        assert!(
            doing.join("make-colour-green.md").exists(),
            "leaving Ready drops it"
        );
        assert!(app.error.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn moving_never_overwrites_an_existing_file() {
        let dir = scratch_dir("clobber");
        let todo = dir.join("roadmap/todo");
        let doing = dir.join("roadmap/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let existing = doing.join("01-demo.md");
        std::fs::write(&existing, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.move_ticket(&file, &doing);

        assert!(file.exists(), "source must survive a refused move");
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
        assert!(app.error.is_some(), "the refusal is surfaced to the user");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Simulates a real pointer drag from a `todo/` card onto the `doing/`
    /// column, so the drag-source/drop-zone wiring is exercised, not just the
    /// move helper.
    #[test]
    fn dragging_a_card_onto_another_column_moves_the_file() {
        let dir = scratch_dir("drag");
        let todo = dir.join("roadmap/todo");
        let doing = dir.join("roadmap/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        // An existing card in the target column, so the drop lands on it.
        let other = doing.join("01-other.md");
        std::fs::write(&other, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));

        let pass = |app: &mut RoadmapApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };
        let moved = |pos: egui::Pos2| egui::Event::PointerMoved(pos);
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        pass(&mut app, vec![]);

        let card_id = egui::Id::new(("ticket_card", &file));
        let start = ctx
            .read_response(card_id)
            .expect("the card is a drag source")
            .rect
            .center();
        // Aim at the card already in the target column, to prove that a card
        // under the pointer does not swallow the column drop.
        let target_card_id = egui::Id::new(("ticket_card", &other));
        let target = ctx
            .read_response(target_card_id)
            .expect("the target column has a card")
            .rect
            .center();

        // Hover, press, then move well past the drag threshold.
        pass(&mut app, vec![moved(start)]);
        pass(&mut app, vec![moved(start), button(start, true)]);
        pass(&mut app, vec![moved(start + egui::vec2(30.0, 0.0))]);
        pass(&mut app, vec![moved(target)]);
        pass(&mut app, vec![moved(target)]);
        // Release over the target column.
        pass(&mut app, vec![moved(target), button(target, false)]);

        let moved_file = doing.join("01-demo.md");
        assert!(
            moved_file.exists(),
            "dragging the card should move the file into the target column"
        );
        assert!(!file.exists(), "the original file should be gone");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A press and release without movement is still a click, not a drag.
    #[test]
    fn clicking_a_card_still_opens_it() {
        let dir = scratch_dir("click");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut RoadmapApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };

        pass(&mut app, vec![]);
        let card_id = egui::Id::new(("ticket_card", &file));
        let start = ctx.read_response(card_id).unwrap().rect.center();

        pass(&mut app, vec![egui::Event::PointerMoved(start)]);
        pass(
            &mut app,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        pass(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );

        assert!(
            matches!(app.screen, Screen::Detail(path) if path == file),
            "a plain click should open the ticket"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_names_keep_a_markdown_extension() {
        assert_eq!(normalize_file_name("notes.md"), "notes.md");
        assert_eq!(normalize_file_name("notes.markdown"), "notes.markdown");
        assert_eq!(normalize_file_name("  notes.MD  "), "notes.MD");
        assert_eq!(normalize_file_name("notes"), "notes.md");
    }

    #[test]
    fn renaming_a_ticket_via_the_editor_moves_the_file() {
        let dir = scratch_dir("rename");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.start_editing(&file);
        app.editor.as_mut().unwrap().name = "01-renamed".to_string();
        app.flush_autosave(false);

        let renamed = todo.join("01-renamed.md");
        assert!(renamed.exists(), "the ticket is renamed on disk");
        assert!(!file.exists(), "the old file name is gone");
        assert!(app.error.is_none(), "a clean rename reports no error");
        assert!(
            matches!(&app.screen, Screen::Detail(path) if path == &renamed),
            "the detail view follows the renamed file"
        );
        assert_eq!(app.editor.as_ref().unwrap().path, renamed);
        assert_eq!(
            app.roadmap
                .find(&renamed)
                .map(|ticket| ticket.title.as_str()),
            Some("Demo"),
            "the board adopts the renamed ticket"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renaming_onto_an_existing_ticket_is_refused() {
        let dir = scratch_dir("rename-clash");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let existing = todo.join("02-other.md");
        std::fs::write(&existing, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.start_editing(&file);
        app.editor.as_mut().unwrap().name = "02-other.md".to_string();
        app.flush_autosave(false);

        assert!(file.exists(), "a refused rename leaves the file in place");
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "# Other\n");
        assert!(
            app.editor.as_ref().unwrap().error.is_some(),
            "the clash is surfaced to the user"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A glyph missing from a font stack draws as nothing, so check the key
    /// symbols of the shortcut hints against the fonts that draw them.
    #[test]
    fn shortcut_glyphs_exist_in_their_fonts() {
        let ctx = themed_ctx();
        for font in [theme::meta(), theme::ui_button()] {
            for glyph in ['⌘', '↩'] {
                assert!(
                    ctx.fonts_mut(|f| f.has_glyph(&font, glyph)),
                    "{glyph} is missing from {font:?}"
                );
            }
        }
    }

    fn grid(columns: &[&[&str]]) -> Vec<Vec<PathBuf>> {
        columns
            .iter()
            .map(|paths| paths.iter().map(PathBuf::from).collect())
            .collect()
    }

    /// At the minimum window width the three columns fit, one gutter in from
    /// each edge, without scrolling, even with the composer open.
    #[test]
    fn columns_fit_the_narrowest_window_with_gutters() {
        let dir = scratch_dir("fit");
        let mut cards = Vec::new();
        for column in ["todo", "ready", "doing", "done"] {
            let folder = dir.join("roadmap").join(column);
            std::fs::create_dir_all(&folder).unwrap();
            let file = folder.join("01-card.md");
            std::fs::write(
                &file,
                "# Card\n\nA summary long enough to wrap a few times.\n",
            )
            .unwrap();
            cards.push(file);
        }

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        // The composer's thicker frame must not widen Todo either.
        app.open_composer();
        let ctx = themed_ctx();
        let width = 880.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 480.0),
            )),
            ..Default::default()
        };
        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| app.draw(ui))
                .textures_delta
                .clear();
        }

        let gutter = theme::SPACE_5;
        for card in &cards {
            let rect = ctx
                .read_response(egui::Id::new(("ticket_card", card)))
                .expect("every card is drawn")
                .rect;
            assert!(
                rect.min.x >= gutter - 0.5 && rect.max.x <= width - gutter + 0.5,
                "{} at {rect:?} is outside the gutters",
                card.display()
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn arrows_step_through_cards_and_skip_empty_columns() {
        let board = grid(&[&["a1", "a2", "a3"], &[], &["c1"]]);
        let step = |at, key| step_selection(&board, at, key);

        assert_eq!(step(None, Key::ArrowRight), Some((0, 0)), "first card");
        assert_eq!(step(Some((0, 0)), Key::ArrowDown), Some((0, 1)));
        assert_eq!(
            step(Some((0, 2)), Key::ArrowDown),
            Some((0, 2)),
            "stops at the end"
        );
        assert_eq!(
            step(Some((0, 0)), Key::ArrowUp),
            Some((0, 0)),
            "stops at the top"
        );
        assert_eq!(
            step(Some((0, 2)), Key::ArrowRight),
            Some((2, 0)),
            "skips the empty column and clamps the row"
        );
        assert_eq!(step(Some((2, 0)), Key::ArrowLeft), Some((0, 0)));
        assert_eq!(
            step(Some((2, 0)), Key::ArrowRight),
            Some((2, 0)),
            "last column"
        );
        assert_eq!(
            step_selection(&grid(&[&[], &[]]), None, Key::ArrowDown),
            None
        );
    }

    #[test]
    fn selection_follows_the_filter_and_survives_opening() {
        let dir = scratch_dir("select");
        let todo = dir.join("roadmap/todo");
        let doing = dir.join("roadmap/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        std::fs::write(todo.join("01-alpha.md"), "# Alpha\n").unwrap();
        std::fs::write(todo.join("02-beta.md"), "# Beta\n").unwrap();
        std::fs::write(doing.join("01-gamma.md"), "# Gamma\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.move_selection(Key::ArrowDown);
        assert_eq!(app.selected, Some(todo.join("01-alpha.md")));
        app.move_selection(Key::ArrowDown);
        app.move_selection(Key::ArrowRight);
        assert_eq!(app.selected, Some(doing.join("01-gamma.md")));

        // Filtered out cards are skipped.
        app.filter = "beta".to_string();
        app.selected = None;
        app.move_selection(Key::ArrowDown);
        assert_eq!(app.selected, Some(todo.join("02-beta.md")));

        // Back from the reader, the card is still selected.
        app.open_ticket(todo.join("02-beta.md"));
        app.back_to_board();
        assert_eq!(app.selected, Some(todo.join("02-beta.md")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An app editing `todo/01-demo.md` with an unsaved change.
    fn editing_with_unsaved_change(tag: &str, auto: bool) -> (PathBuf, PathBuf, RoadmapApp) {
        let dir = scratch_dir(tag);
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        app.auto = auto;
        app.open_ticket(file.clone());
        app.start_editing(&file);
        app.editor.as_mut().unwrap().text = "# Demo\n\nmore\n".to_string();
        (dir, file, app)
    }

    #[test]
    fn leaving_with_auto_on_saves() {
        let (dir, file, mut app) = editing_with_unsaved_change("leave-auto", true);

        app.show_preview();

        assert_eq!(app.detail_mode, DetailMode::Preview);
        assert!(
            app.leave_prompt.is_none(),
            "no question when the save worked"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leaving_with_auto_off_asks_first() {
        let (dir, file, mut app) = editing_with_unsaved_change("leave-ask", false);

        app.back_to_board();
        assert_eq!(app.leave_prompt, Some(Leave::Board), "the editor asks");
        assert_eq!(app.detail_mode, DetailMode::Edit, "and stays put");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");

        app.answer_leave(LeaveChoice::KeepEditing);
        assert!(app.leave_prompt.is_none());
        assert_eq!(app.detail_mode, DetailMode::Edit);

        app.back_to_board();
        app.answer_leave(LeaveChoice::Save);
        assert!(matches!(app.screen, Screen::Board), "saved, then left");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nmore\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discarding_on_leave_keeps_the_file() {
        let (dir, file, mut app) = editing_with_unsaved_change("leave-discard", false);

        app.open_composer();
        assert_eq!(app.leave_prompt, Some(Leave::Composer));
        app.answer_leave(LeaveChoice::Discard);

        assert!(matches!(app.screen, Screen::Board));
        assert!(app.composer.is_some(), "the composer opens after leaving");
        assert!(!app.has_unsaved_edits());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_conflict_asks_even_with_auto_on() {
        let (dir, file, mut app) = editing_with_unsaved_change("leave-conflict", true);
        std::fs::write(&file, "# Changed elsewhere\n").unwrap();
        app.refresh();

        app.show_preview();

        assert_eq!(
            app.leave_prompt,
            Some(Leave::Reader),
            "autosave cannot resolve it"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# Changed elsewhere\n"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn slugs_are_lowercase_ascii_words_joined_by_hyphens() {
        assert_eq!(slugify("Fix login redirect"), "fix-login-redirect");
        assert_eq!(
            slugify("  AI assist: refine a ticket!  "),
            "ai-assist-refine-a-ticket"
        );
        assert_eq!(slugify("Café au lait"), "caf-au-lait");
        assert_eq!(slugify("¿¡!?"), "ticket");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= MAX_SLUG_LEN, "{long} is too long");
        assert!(!long.ends_with('-'));
    }

    #[test]
    fn planned_names_take_a_suffix_when_taken() {
        let dir = scratch_dir("planned");
        assert_eq!(
            planned_ticket_name(&dir, "Fix it"),
            ("fix-it.md".to_string(), false)
        );
        std::fs::write(dir.join("fix-it.md"), "").unwrap();
        assert_eq!(
            planned_ticket_name(&dir, "Fix it"),
            ("fix-it-2.md".to_string(), true)
        );
        std::fs::write(dir.join("fix-it-2.md"), "").unwrap();
        assert_eq!(
            planned_ticket_name(&dir, "Fix it"),
            ("fix-it-3.md".to_string(), true)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_composer_creates_tickets_and_stays_open() {
        let dir = scratch_dir("compose");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.open_composer();
        app.composer.as_mut().unwrap().title = "Fix login redirect".to_string();
        app.submit_composer(false);

        let created = todo.join("fix-login-redirect.md");
        assert_eq!(
            std::fs::read_to_string(&created).unwrap(),
            "# Fix login redirect\n"
        );
        let composer = app.composer.as_ref().expect("the composer stays open");
        assert!(
            composer.title.is_empty(),
            "the field clears for the next idea"
        );
        assert!(matches!(app.screen, Screen::Board));
        assert!(app.roadmap.find(&created).is_some(), "the board sees it");

        // An empty title never creates a file.
        app.submit_composer(false);
        assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn command_enter_creates_and_opens_the_editor() {
        let dir = scratch_dir("compose-edit");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.open_composer();
        app.composer.as_mut().unwrap().title = "Export board".to_string();
        app.submit_composer(true);

        let created = todo.join("export-board.md");
        assert!(created.exists(), "the new ticket is written to Todo");
        assert!(
            matches!(&app.screen, Screen::Detail(path) if path == &created),
            "the new ticket is opened"
        );
        assert_eq!(app.detail_mode, DetailMode::Edit, "it opens in the editor");
        assert!(app.focus_editor, "the editor is focused for typing");
        assert!(app.composer.is_none(), "the composer closes");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_the_todo_column_offers_new_tickets() {
        let dir = scratch_dir("new-columns");
        let todo = dir.join("roadmap/todo");
        let doing = dir.join("roadmap/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let roadmap = discovery::build(&located).unwrap();

        let find = |name: &str| {
            roadmap
                .columns
                .iter()
                .find(|column| column.name == name)
                .unwrap()
        };
        assert!(find("Todo").accepts_new_tickets());
        assert!(!find("Doing").accepts_new_tickets());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn long_ticket_names_are_capped_and_keep_their_extension() {
        let long = "word ".repeat(80);
        let name = normalize_file_name(&long);
        assert!(
            name.len() <= MAX_FILE_NAME_BYTES,
            "{name} is too long ({} bytes)",
            name.len()
        );
        assert!(name.ends_with(".md"));

        let long_markdown = format!("{}.markdown", "b".repeat(400));
        let name = normalize_file_name(&long_markdown);
        assert!(name.len() <= MAX_FILE_NAME_BYTES);
        assert!(name.ends_with(".markdown"));

        // Multi-byte characters are cut on a character boundary.
        let accented = format!("{}.md", "é".repeat(400));
        let name = normalize_file_name(&accented);
        assert!(name.len() <= MAX_FILE_NAME_BYTES);
        assert!(name.ends_with(".md"));
    }

    #[test]
    fn deleting_a_ticket_asks_first_then_removes_the_file() {
        let dir = scratch_dir("delete");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        app.selected = Some(file.clone());
        app.open_ticket(file.clone());

        app.request_delete(file.clone());
        assert!(app.confirm.is_some(), "the delete waits for confirmation");
        assert!(file.exists(), "nothing is removed until confirmed");

        app.delete_ticket(&file);
        assert!(!file.exists(), "the file is gone");
        assert!(app.confirm.is_none());
        assert!(app.selected.is_none());
        assert!(
            matches!(app.screen, Screen::Board),
            "deleting the open ticket returns to the board"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn archiving_a_done_ticket_hides_it_in_the_dot_archive() {
        let dir = scratch_dir("archive");
        let done = dir.join("roadmap/done");
        std::fs::create_dir_all(&done).unwrap();
        let file = done.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);
        assert!(app.roadmap.find(&file).is_some());

        app.archive_ticket(&file);
        let archived = dir.join("roadmap/.archive/01-demo.md");
        assert!(archived.exists(), "the ticket is kept in .archive");
        assert!(!file.exists(), "and is gone from Done");
        assert!(app.roadmap.find(&file).is_none(), "the board drops it");
        assert!(
            app.error.is_none(),
            "a clean archive reports no error: {:?}",
            app.error
        );

        // A second ticket with the same name does not overwrite the first.
        std::fs::write(&file, "# Again\n").unwrap();
        app.refresh();
        app.archive_ticket(&file);
        assert!(dir.join("roadmap/.archive/01-demo-2.md").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_a_todo_card_at_a_position_renumbers_the_column() {
        let dir = scratch_dir("reorder-todo");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let hello = todo.join("001-hello.md");
        let byebye = todo.join("002-byebye.md");
        std::fs::write(&hello, "# Hello\n").unwrap();
        std::fs::write(&byebye, "# Bye bye\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        // Drop Bye bye in front of Hello.
        app.place_ticket(&byebye, &todo, Some(hello.clone()));

        assert!(
            todo.join("001-byebye.md").exists(),
            "the dropped card becomes 001"
        );
        assert!(
            todo.join("002-hello.md").exists(),
            "the displaced card becomes 002"
        );
        assert!(!byebye.exists() && !hello.exists(), "old names are gone");
        assert!(app.error.is_none(), "{:?}", app.error);

        let names: Vec<String> = app.roadmap.columns[0]
            .tickets
            .iter()
            .map(|ticket| ticket.file_name())
            .collect();
        assert_eq!(names, ["001-byebye.md", "002-hello.md"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn moving_a_todo_card_into_ready_at_a_position_renumbers_it() {
        let dir = scratch_dir("reorder-ready");
        let todo = dir.join("roadmap/todo");
        let ready = dir.join("roadmap/ready");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&ready).unwrap();
        let moved = todo.join("later.md");
        let first = ready.join("001-first.md");
        std::fs::write(&moved, "# Later\n").unwrap();
        std::fs::write(&first, "# First\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        app.place_ticket(&moved, &ready, Some(first.clone()));

        assert!(ready.join("001-later.md").exists(), "it takes priority 1");
        assert!(ready.join("002-first.md").exists(), "the other moves down");
        assert!(app.error.is_none(), "{:?}", app.error);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A real pointer drag of the last Todo card onto the top of the column
    /// renumbers the files, exercising the drop-anchor wiring end to end.
    #[test]
    fn dragging_a_card_within_a_column_reorders_it() {
        let dir = scratch_dir("drag-reorder");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let first = todo.join("001-first.md");
        let second = todo.join("002-second.md");
        std::fs::write(&first, "# First\n").unwrap();
        std::fs::write(&second, "# Second\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut RoadmapApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };
        let moved = |pos: egui::Pos2| egui::Event::PointerMoved(pos);
        let button = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        pass(&mut app, vec![]);
        let start = ctx
            .read_response(egui::Id::new(("ticket_card", &second)))
            .expect("the card is a drag source")
            .rect
            .center();
        // Just above the first card's middle: insert before it.
        let target = ctx
            .read_response(egui::Id::new(("ticket_card", &first)))
            .expect("the target card is drawn")
            .rect
            .center()
            - egui::vec2(0.0, 6.0);

        pass(&mut app, vec![moved(start)]);
        pass(&mut app, vec![moved(start), button(start, true)]);
        pass(&mut app, vec![moved(start + egui::vec2(0.0, 30.0))]);
        pass(&mut app, vec![moved(target)]);
        pass(&mut app, vec![moved(target)]);
        pass(&mut app, vec![moved(target), button(target, false)]);

        assert!(
            todo.join("001-second.md").exists() && todo.join("002-first.md").exists(),
            "dropping the second card on top renumbers the column"
        );
        assert!(!first.exists() && !second.exists(), "old names are gone");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The delete icon is its own click target: using it must not open the
    /// ticket underneath it.
    #[test]
    fn the_card_delete_icon_asks_to_delete_without_opening() {
        let dir = scratch_dir("card-delete");
        let todo = dir.join("roadmap/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = RoadmapApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut RoadmapApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };

        pass(&mut app, vec![]);
        let icon = ctx
            .read_response(egui::Id::new(("card_delete", &file)))
            .expect("the card draws a delete icon")
            .rect
            .center();
        pass(&mut app, vec![egui::Event::PointerMoved(icon)]);
        pass(
            &mut app,
            vec![
                egui::Event::PointerMoved(icon),
                egui::Event::PointerButton {
                    pos: icon,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        pass(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: icon,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );

        assert!(app.confirm.is_some(), "the icon asks to delete");
        assert!(
            matches!(app.screen, Screen::Board),
            "and does not open the ticket"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

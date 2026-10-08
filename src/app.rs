//! The egui application: a kanban board with an inline quick-add composer, a
//! rendered markdown reader, and an editor with debounced autosave. Its look
//! follows the design system in `design/` (see `theme.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{
    Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::agent::{self, AgentConfig, Report};
use crate::commits;
use crate::discovery::{self, Board, Column, Located};
use crate::filter::{self, Filter};
use crate::general;
use crate::issue::Issue;
use crate::label_picker::{self, LabelPicker};
use crate::labels::{self, Labels};
use crate::review_settings::{self, Catalog};
use crate::theme::{self, Icon, Palette};
use crate::titles::{self, Engine};
use crate::widgets::{self, ButtonKind};
use crate::{design_ui, dialog, recents, sheet};
use iboard::design::{self, DesignSystem};
use iboard::prompts;

/// How long after the last keystroke an edit is written to disk.
const AUTOSAVE_AFTER: Duration = Duration::from_millis(800);

/// How long an issue created from the composer keeps its `accent-soft` fill.
const NEW_CARD_FLASH: Duration = Duration::from_secs(2);

/// How long the composer waits for a title before the user types one.
const NAMING_BUDGET: Duration = Duration::from_secs(3);

/// On macOS the header bar doubles as the window's title bar (the native
/// title is hidden and the content runs under the traffic lights).
pub const UNIFIED_TITLEBAR: bool = cfg!(target_os = "macos");

/// Room the macOS traffic lights take at the left of the header bar.
const TRAFFIC_LIGHTS_WIDTH: i8 = 80;

/// The filter's width at rest, and while in use or filtering.
const FILTER_REST: f32 = 220.0;
const FILTER_OPEN: f32 = 480.0;
/// How long the filter takes to widen or shrink.
const FILTER_GROW_SECS: f32 = 0.12;
/// Room the breadcrumb keeps when the filter widens on a narrow window.
const CRUMB_KEEP: f32 = 160.0;
/// Gap between the filter's icon, tokens, text and chips.
const FILTER_GAP: f32 = 6.0;
/// Narrowest the filter text may get before the chips hide.
const FILTER_TEXT_MIN: f32 = 48.0;
/// "Labels", before the suggestion chips.
fn filter_try_font() -> egui::FontId {
    egui::FontId::proportional(11.0)
}

/// The board filter's text input.
fn filter_id() -> egui::Id {
    egui::Id::new("filter")
}

/// Longest slug the composer derives from a title.
const MAX_SLUG_LEN: usize = 60;

/// Longest file name the editor accepts, in bytes, including its extension.
/// 255 bytes is the limit of APFS, ext4 and NTFS: longer names fail to write.
const MAX_FILE_NAME_BYTES: usize = 255;

/// Directory done issues are archived into, under the issues root. It starts
/// with a dot, so discovery never reads it back.
const ARCHIVE_DIR: &str = ".archive";

/// Width of the recent-projects popup, in points.
const PROJECT_MENU_WIDTH: f32 = 300.0;

/// Width of the AI review panel beside the reader or editor page.
const REVIEW_PANEL_WIDTH: f32 = 360.0;

/// Width of the Review button's menu, in points.
const REVIEW_MENU_WIDTH: f32 = 240.0;

/// A choice made in the Review button's menu.
enum ReviewMenu {
    Review,
    Settings,
}

/// A choice made in the header's project menu.
enum ProjectChoice {
    /// Switch to a remembered project.
    Recent(PathBuf),
    /// Ask the OS for a folder.
    Browse,
}

/// How a column orders its cards in ColumnFocus. The choice is remembered per
/// column for the session; only [`Sort::Board`] offers drag reordering.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sort {
    /// The order the files are in on the board.
    Board,
    /// Most recently edited first.
    Modified,
    /// Most recently created first.
    Created,
    /// Title, A to Z.
    Title,
    /// The most open checklist items first.
    MostToDo,
    /// Most recently finished first; Done's default.
    Finished,
}

impl Sort {
    /// The five sorts every column offers; Done additionally opens with
    /// [`Sort::Finished`].
    const MENU: [Sort; 5] = [
        Sort::Board,
        Sort::Modified,
        Sort::Created,
        Sort::Title,
        Sort::MostToDo,
    ];

    fn label(self) -> &'static str {
        match self {
            Sort::Board => "Board order",
            Sort::Modified => "Recently modified",
            Sort::Created => "Recently created",
            Sort::Title => "Title A to Z",
            Sort::MostToDo => "Most to do",
            Sort::Finished => "Recently finished",
        }
    }
}

/// The default sort of a focused column, chosen by what people look for there.
fn default_sort(dir: &Path) -> Sort {
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if discovery::is_ready_column(dir) {
        Sort::Board
    } else if name == "done" || name == "completed" {
        Sort::Finished
    } else {
        Sort::Modified
    }
}

/// An issue action available from a card or the detail header.
#[derive(Clone, PartialEq, Eq, Debug)]
enum CardAction {
    /// Open the issue in the reader.
    Open,
    /// Delete the issue's file.
    Delete,
    /// Move a done issue to `.archive`.
    Archive,
    /// Add the card's label to the filter.
    FilterLabel(String),
    /// Run the AI review (selected Todo card).
    Review,
    /// Reopen the last review, from the card's verdict pill.
    ShowReview,
    /// Open (or close) the LabelPicker on this card.
    AddLabel,
}

/// What a card shows beyond its file: the last review's verdict pill, and
/// whether the selected card offers Review.
#[derive(Clone, Copy)]
struct CardExtras<'a> {
    review_pill: Option<(&'a str, Color32)>,
    review_button: bool,
    /// In ColumnFocus: show more of the description.
    wide: bool,
    /// The reader's standard sections, for the wide card's facts row.
    sections: &'a [String],
}

impl Default for CardExtras<'_> {
    fn default() -> Self {
        Self {
            review_pill: None,
            review_button: false,
            wide: false,
            sections: &[],
        }
    }
}

/// A finished review kept for the session, so its card pill can reopen it.
struct PastReview {
    draft: bool,
    agent: String,
    raw: String,
    report: Option<Report>,
}

impl PastReview {
    /// The pill: the worst verdict of the three, or "Reviewed" when the
    /// answer did not parse.
    fn pill(&self, p: &Palette) -> (&'static str, Color32) {
        let verdicts = self
            .report
            .iter()
            .flat_map(|r| r.findings.iter().map(|f| f.verdict));
        let worst = verdicts.max_by_key(|v| match v {
            agent::Verdict::Pass => 0,
            agent::Verdict::Challenge => 1,
            agent::Verdict::Block => 2,
        });
        match worst {
            Some(agent::Verdict::Pass) => ("Pass", p.status_done),
            Some(agent::Verdict::Challenge) => ("Challenge", p.review),
            Some(agent::Verdict::Block) => ("Block", p.danger),
            None => ("Reviewed", p.review),
        }
    }
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

/// An issue being dragged across the board. The payload is the issue's file,
/// which a drop moves into the target column's directory, or hands to one of
/// the DropZones (archive, delete).
#[derive(Clone)]
struct IssueDrag {
    path: PathBuf,
    /// A Done issue: the Archive zone accepts it.
    archivable: bool,
}

/// What a drop on the DropZones dock asked for.
enum DockDrop {
    Archive(PathBuf),
    Delete(PathBuf),
}

/// The DropZones dock: 56pt zones with 12pt padding and a 1pt rule above.
/// Archive shows for Done cards only; otherwise Delete alone spans the dock.
const DOCK_H: f32 = 56.0 + 2.0 * 12.0 + 1.0;
/// How long the dock takes to slide in or out.
const DOCK_SLIDE: f32 = 0.14;
/// How long a status bar notice ("Archived …") stays up.
const NOTICE_FOR: Duration = Duration::from_secs(4);
/// How long `⌘Z` can bring back an issue deleted without asking.
const UNDO_FOR: Duration = Duration::from_secs(10);

/// The commits popup: the hashes whose message names a ticket's file, read
/// from git when its button is pressed.
struct CommitsPanel {
    path: PathBuf,
    title: String,
    list: Vec<commits::Commit>,
}

/// An issue deleted without asking, kept so `⌘Z` can put it back.
struct Deleted {
    path: PathBuf,
    /// Where the Trash put it, when it could say.
    trashed: Option<PathBuf>,
    /// The file as it was, for when the Trash copy can't be moved back.
    content: Vec<u8>,
    at: Instant,
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
    /// The composer asked to add its issue; `true` also closes it (a click
    /// elsewhere on the board).
    submit: Option<bool>,
    /// The composer lost focus while empty.
    close_composer: bool,
    /// The composer's engine tag was clicked: open Settings › Titles.
    title_settings: bool,
    /// A card's label was clicked: filter by it.
    filter_label: Option<String>,
    /// The selected card's Review was clicked.
    review: Option<PathBuf>,
    /// A card's verdict pill was clicked.
    show_review: Option<PathBuf>,
    /// A card's Add label was clicked.
    add_label: Option<PathBuf>,
    /// The column title was clicked, or the column double-clicked: expand it
    /// to the whole window.
    focus_column: Option<PathBuf>,
    /// "Archive all" at the foot of Done: these issues.
    archive_all: Vec<PathBuf>,
}

/// The inline quick-add composer at the top of the Todo column: you describe
/// the issue, and iboard names it from the description.
#[derive(Default)]
struct Composer {
    title: String,
    description: String,
    /// The engine that names issues, checked when the composer opened;
    /// `None` when it is Off or unavailable, so titles are typed (Manual).
    engine: Option<Engine>,
    stage: TitleStage,
    /// The engine's answer while naming.
    naming: Option<NamingRun>,
    /// The field to give keyboard focus on the next frame.
    focus: Option<ComposerField>,
    /// Select the whole title when it gets focus, so typing replaces it.
    select_title: bool,
    error: Option<String>,
}

impl Composer {
    /// Nothing typed: closing it loses nothing.
    fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.description.trim().is_empty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComposerField {
    Title,
    Description,
}

/// Where the composer's title stands while the user has not typed one.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum TitleStage {
    /// Named on save, if an engine is ready.
    #[default]
    Idle,
    /// Waiting for the engine.
    Naming,
    /// The engine's suggestion, untouched.
    Inferred,
    /// The engine failed or was too slow: the first line names the issue.
    Failed,
    /// `esc` while naming: the user types the title.
    Skipped,
}

struct NamingRun {
    answer: Receiver<Result<String, String>>,
    started: Instant,
    /// Clicked away: write and close as soon as the title is in.
    then_close: bool,
}

/// The in-progress edit of one issue.
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

/// The AI review of one issue: running, answered or failed.
struct ReviewState {
    path: PathBuf,
    /// True when the agent read the editor draft rather than the saved file.
    draft: bool,
    /// The configured tool (and model), shown in the panel header.
    agent: String,
    status: ReviewStatus,
    /// The "Ask again" follow-up, kept between renders.
    follow_up: String,
}

enum ReviewStatus {
    Running {
        answer: Receiver<Result<String, String>>,
        pid: Option<u32>,
    },
    Done {
        raw: String,
        report: Option<Report>,
    },
    Failed {
        error: String,
    },
}

/// The file name part of a path, as UTF-8 (empty when there is none).
fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Makes sure a user-typed file name keeps a markdown extension so the issue
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

pub struct IboardApp {
    located: Located,
    board: Board,
    cache: CommonMarkCache,
    screen: Screen,
    detail_mode: DetailMode,
    editor: Option<EditorState>,
    composer: Option<Composer>,
    /// The issue the composer just wrote, highlighted for a moment.
    new_card: Option<(PathBuf, Instant)>,
    filter: Filter,
    focus_filter: bool,
    /// The filter's one-click label tokens pinned in the config; `None` offers
    /// the board's three most used labels.
    suggestions: Option<Vec<String>>,
    /// What names the composer's new issues.
    title_engine: Engine,
    /// The Settings tab `⌘,` reopens.
    settings_tab: review_settings::Tab,
    /// The suggestion chip reached with `↓`, while the filter has focus.
    chip_cursor: Option<usize>,
    /// The filter is in use: focused, or being clicked (a click on a chip
    /// takes focus from the text for a frame).
    filter_open: bool,
    error: Option<String>,
    /// A destructive action waiting for the user to confirm it.
    confirm: Option<Confirm>,
    /// The LabelPicker, open on one issue.
    label_picker: Option<LabelPicker>,
    /// The commits popup, open on one issue.
    commits: Option<CommitsPanel>,
    /// A just-composed ticket to move to position 1 once the pointer is up:
    /// the Todo folder and the new file.
    pending_front: Option<(PathBuf, PathBuf)>,
    /// A short outcome for the status bar ("Archived x.md"), and when it came.
    notice: Option<(String, Instant)>,
    /// Settings › General: deleting an issue asks first.
    confirm_delete: bool,
    /// The last issue deleted without asking, for `⌘Z`.
    undo: Option<Deleted>,
    /// The last finished review of each issue this session, behind the
    /// card's verdict pill.
    past_reviews: HashMap<PathBuf, PastReview>,
    /// The column expanded to the whole window (ColumnFocus), by folder.
    focused_column: Option<PathBuf>,
    /// ColumnFocus's sort per folder, remembered for the session.
    column_sorts: HashMap<PathBuf, Sort>,
    /// ColumnFocus: split the grid into sections by label.
    group_by_label: bool,
    /// The last drag's Archive eligibility, so the dock keeps its look while
    /// it slides away after the drop.
    dock_archivable: bool,
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
    /// The rendered issue content, used to tell a double-click on the page
    /// from one on the header when deciding whether to start editing.
    preview_rect: Rect,
    /// Set when a freshly created issue should receive keyboard focus.
    focus_editor: bool,
    /// Projects opened recently, most recent first, shown in the header menu.
    recents: Vec<PathBuf>,
    /// Where `recents` is persisted; `None` in tests, so they never touch the
    /// user's cache.
    recents_file: Option<PathBuf>,
    /// A window title to apply on the next frame, after a project switch.
    pending_title: Option<String>,
    /// The user-configured AI tool and model.
    agent: AgentConfig,
    /// Where `agent` is persisted; `None` in tests, so they never touch the
    /// user's configuration.
    agent_file: Option<PathBuf>,
    /// The AI review of the open issue, if any.
    review: Option<ReviewState>,
    /// The ReviewSettings dialog, while it is open.
    settings: Option<review_settings::Dialog>,
    /// The installed agents and their models, for the settings dialog.
    catalog: Catalog,
    /// The label colours: the built-in preset, overridden by `[labels]` in the
    /// user configuration.
    labels: Labels,
    /// The Design panel, while it is open.
    design: Option<design_ui::Panel>,
    /// The project's design system, loaded when the panel first opens.
    design_system: Option<DesignSystem>,
    /// Why the design system could not be loaded, when it could not.
    design_error: Option<String>,
    /// The component preview an agent is drafting, if one is running.
    design_draft: Option<DraftJob>,
    /// A UX enrichment: an agent drafting a static mockup of an issue.
    enrichment: Option<Enrichment>,
}

/// A component whose preview an agent is drafting.
struct DraftJob {
    name: String,
    running: agent::Running,
}

/// A UX enrichment of one issue: the agent's mockup, its preview, and the
/// refinement the user is typing.
struct Enrichment {
    path: PathBuf,
    /// The drafted artifact HTML, once the agent answered.
    html: Option<String>,
    /// The rendered preview PNG, when the renderer could make one.
    png: Option<PathBuf>,
    status: EnrichStatus,
    /// The refinement instruction being typed.
    refine: String,
    /// The decoded preview, loaded once per render.
    texture: Option<egui::TextureHandle>,
    /// A short note under the artifact (where it was saved, a render warning).
    note: Option<String>,
}

enum EnrichStatus {
    /// The agent is writing the mockup.
    Running(Receiver<Result<String, String>>),
    /// The mockup is in; the renderer is turning it into a PNG.
    Rendering(Receiver<Result<PathBuf, String>>),
    Done,
    Failed(String),
}

/// One finished background step of an enrichment.
enum EnrichOutcome {
    Agent(Result<String, String>),
    Png(Result<PathBuf, String>),
}

impl IboardApp {
    /// `window_title` names the window in Mission Control and the Window menu;
    /// see `main.rs` for why it is set here rather than at startup.
    pub fn new(cc: &eframe::CreationContext<'_>, located: Located, window_title: String) -> Self {
        theme::install(&cc.egui_ctx);
        // Images in a ticket's markdown (`![shot](assets/<stem>/ui.png)`) load
        // from disk through egui's file loader.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(window_title));
        // After winit's default menu is up, so this one replaces it.
        #[cfg(target_os = "macos")]
        {
            iboard::menu::set_context(cc.egui_ctx.clone());
            iboard::menu::install("iboard", env!("CARGO_PKG_DESCRIPTION"), true);
        }
        let mut app = Self::from_located(located);
        // The user's real cache: only the running app records here, never a
        // test constructing the board directly.
        app.recents_file = recents::cache_file();
        if let Some(file) = &app.recents_file {
            app.recents = recents::read(file);
        }
        // Likewise for the AI configuration.
        app.agent_file = agent::config_file();
        app.agent = agent::load_from(app.agent_file.as_deref());
        app.suggestions = filter::load_suggestions(app.agent_file.as_deref());
        app.title_engine = titles::load_engine(app.agent_file.as_deref());
        app.confirm_delete = general::load_confirm_delete(app.agent_file.as_deref());
        app.labels = Labels::load(app.agent_file.as_deref());
        app.catalog = Catalog::probing();
        let project = app.located.project_dir.clone();
        app.remember(&project);
        app
    }

    fn from_located(located: Located) -> Self {
        let (board, error) = match discovery::build(&located) {
            Ok(board) => (board, None),
            Err(err) => (Board::empty(&located), Some(format!("{err:#}"))),
        };
        Self {
            located,
            board,
            cache: CommonMarkCache::default(),
            screen: Screen::Board,
            detail_mode: DetailMode::Preview,
            editor: None,
            composer: None,
            new_card: None,
            filter: Filter::default(),
            focus_filter: false,
            suggestions: filter::load_suggestions(None),
            title_engine: Engine::Manual,
            settings_tab: review_settings::Tab::General,
            chip_cursor: None,
            filter_open: false,
            error,
            confirm: None,
            label_picker: None,
            commits: None,
            pending_front: None,
            notice: None,
            confirm_delete: true,
            undo: None,
            past_reviews: HashMap::new(),
            focused_column: None,
            column_sorts: HashMap::new(),
            group_by_label: false,
            dock_archivable: false,
            auto: true,
            last_refresh: Instant::now(),
            selected: None,
            scroll_to_selected: false,
            leave_prompt: None,
            closing: false,
            send_close: false,
            preview_rect: Rect::NOTHING,
            focus_editor: false,
            recents: Vec::new(),
            recents_file: None,
            pending_title: None,
            agent: AgentConfig::default(),
            agent_file: None,
            review: None,
            settings: None,
            catalog: Catalog::default(),
            labels: Labels::presets(),
            design: None,
            design_system: None,
            design_error: None,
            design_draft: None,
            enrichment: None,
        }
    }

    /// Moves a project to the front of the recent list and persists it, when
    /// the cache path is known.
    fn remember(&mut self, project: &Path) {
        self.recents = recents::remember(std::mem::take(&mut self.recents), project);
        if let Some(file) = &self.recents_file {
            recents::write(file, &self.recents);
        }
    }

    /// Switches the board to another project, resolving its board and
    /// remembering it. A folder with no issues directory reports an error and leaves
    /// the current project in place. Any editor state belongs to the old
    /// project, so it is dropped.
    fn open_project(&mut self, project: PathBuf) {
        let project = recents::canonical(&project);
        match discovery::locate(&project) {
            Ok(located) => {
                self.pending_title = Some(discovery::window_title(&located));
                let project_dir = located.project_dir.clone();
                self.located = located;
                self.screen = Screen::Board;
                self.detail_mode = DetailMode::Preview;
                self.editor = None;
                self.composer = None;
                self.confirm = None;
                self.new_card = None;
                self.leave_prompt = None;
                self.cancel_review();
                self.selected = None;
                self.scroll_to_selected = false;
                self.filter.clear();
                self.error = None;
                // Remember the project root discovery settled on, not the
                // folder the user happened to pick inside it.
                self.remember(&project_dir);
                self.refresh();
            }
            Err(err) => self.error = Some(format!("{err:#}")),
        }
    }

    fn refresh(&mut self) {
        match discovery::build(&self.located) {
            Ok(board) => {
                self.board = board;
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
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        let disk = &issue.markdown;
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
            // Keep the reader and the selection on the issue's new file name.
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
        let Some(issue) = self.board.find(&editor.path) else {
            return;
        };
        editor.text = issue.markdown.clone();
        editor.baseline = issue.markdown.clone();
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

    /// Opens an issue in the reader. Editing is a separate step, entered from
    /// the detail view.
    fn open_issue(&mut self, path: PathBuf) {
        // A composer with text is an unsaved issue: it waits on the board
        // until the user adds or closes it. Only an empty one goes.
        if self.composer.as_ref().is_some_and(Composer::is_empty) {
            self.composer = None;
        }
        self.cancel_review();
        self.selected = Some(path.clone());
        self.screen = Screen::Detail(path);
        self.detail_mode = DetailMode::Preview;
    }

    fn start_editing(&mut self, path: &Path) {
        if !self.can_edit(path) {
            return;
        }
        if self
            .editor
            .as_ref()
            .map(|e| e.path == path)
            .unwrap_or(false)
        {
            self.detail_mode = DetailMode::Edit;
            return;
        }
        // Persist any pending edits of another issue before switching.
        self.save_pending();

        let Some(issue) = self.board.find(path) else {
            return;
        };
        let content = issue.markdown.clone();
        self.editor = Some(EditorState::new(path.to_path_buf(), content));
        self.screen = Screen::Detail(path.to_path_buf());
        self.detail_mode = DetailMode::Edit;
    }

    /// Opens the commits popup for `path`: the git log entries whose message
    /// names its file (the `closes <file>.md` line a commit carries).
    fn open_commits(&mut self, path: &Path) {
        let file = file_name(path);
        let dir = path.parent().unwrap_or(&self.board.root).to_path_buf();
        let list = commits::for_file(&dir, &file);
        let title = self
            .board
            .find(path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file.clone());
        self.commits = Some(CommitsPanel {
            path: path.to_path_buf(),
            title,
            list,
        });
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

    /// Ends the process once quitting has been agreed.
    ///
    /// On macOS, letting eframe close the window destroys the `NSWindow`, and
    /// AppKit's Touch Bar finder then races the teardown and aborts the process
    /// on Touch Bar Macs: a double `removeObserver` in
    /// `_NSTouchBarFinderObservation` raises `NSInternalInconsistencyException`
    /// (`SIGABRT`). Exiting here skips that teardown; unsaved edits were just
    /// saved or discarded, and writes are atomic, so there is nothing to flush.
    fn quit(&mut self, _ctx: &egui::Context) {
        #[cfg(not(target_os = "macos"))]
        _ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        #[cfg(target_os = "macos")]
        std::process::exit(0);
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

    /// Moves a dragged issue into another column's directory, keeping its file
    /// name apart from the Ready priority number (see
    /// [`discovery::moved_name`]). An existing file with the same name is never
    /// overwritten. Returns the issue's new path on success.
    fn move_issue(&mut self, path: &Path, target_dir: &Path) -> Option<PathBuf> {
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
                // Keep the reader/editor pointed at the issue's new home.
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
    fn place_issue(&mut self, path: &Path, target_dir: &Path, anchor: Option<PathBuf>) {
        let same_dir = path.parent() == Some(target_dir);
        let orderable = discovery::is_orderable_column(target_dir);
        if same_dir && !orderable {
            return;
        }
        let moved = if same_dir {
            path.to_path_buf()
        } else {
            match self.move_issue(path, target_dir) {
                Some(moved) => moved,
                None => return,
            }
        };
        if orderable {
            self.reorder_column(target_dir, &moved, anchor.as_deref());
        }
    }

    /// `⌘↩` on the board: moves the selected ticket to the next column, left
    /// to right. A ticket in the last column stays put.
    fn move_selection_to_next(&mut self) {
        let Some(path) = self.selected_issue() else {
            return;
        };
        let Some(index) = self
            .board
            .columns
            .iter()
            .position(|column| column.issues.iter().any(|issue| issue.path == path))
        else {
            return;
        };
        let Some(target) = self
            .board
            .columns
            .get(index + 1)
            .map(|column| column.dir.clone())
        else {
            return;
        };
        self.place_issue(&path, &target, None);
    }

    /// Renumbers an ordered column so `moved` sits in front of `anchor`, keeping
    /// every other issue in its current order. Returns `moved`'s path after the
    /// renumbering, which may have changed its prefix.
    fn reorder_column(
        &mut self,
        dir: &Path,
        moved: &Path,
        anchor: Option<&Path>,
    ) -> Option<PathBuf> {
        if anchor == Some(moved) {
            return Some(moved.to_path_buf());
        }
        self.refresh();
        let column = self
            .board
            .columns
            .iter()
            .find(|column| column.dir.as_path() == dir)?;
        let mut order: Vec<PathBuf> = column.issues.iter().map(|t| t.path.clone()).collect();
        order.retain(|path| path != moved);
        match anchor.and_then(|anchor| order.iter().position(|path| path == anchor)) {
            Some(index) => order.insert(index, moved.to_path_buf()),
            None => order.push(moved.to_path_buf()),
        }
        let result = match renumber(dir, &order) {
            Ok(renames) => {
                let renamed = renames
                    .iter()
                    .find(|(old, _)| old == moved)
                    .map(|(_, new)| new.clone());
                self.apply_renames(&renames);
                renamed
            }
            Err(err) => {
                self.error = Some(err);
                None
            }
        };
        self.refresh();
        Some(result.unwrap_or_else(|| moved.to_path_buf()))
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

    /// The column an issue currently belongs to.
    fn column_of(&self, path: &Path) -> Option<&Column> {
        self.board
            .columns
            .iter()
            .find(|column| column.issues.iter().any(|issue| issue.path == path))
    }

    /// Expands a column to the whole window (ColumnFocus).
    fn focus_column(&mut self, dir: PathBuf) {
        if self.board.columns.iter().any(|column| column.dir == dir) {
            self.focused_column = Some(dir);
        }
    }

    /// Leaves ColumnFocus for the board, keeping the selection.
    fn unfocus_column(&mut self) {
        self.focused_column = None;
    }

    /// The sort a focused column uses: the one the user picked, or its
    /// default.
    fn column_sort(&self, dir: &Path) -> Sort {
        self.column_sorts
            .get(dir)
            .copied()
            .unwrap_or_else(|| default_sort(dir))
    }

    /// Every delete lands here: the trash icons, the Delete key and button,
    /// and the Delete drop zone. It asks first unless Settings › General says
    /// not to; then the file goes at once and `⌘Z` can bring it back.
    fn request_delete(&mut self, path: PathBuf) {
        if self.confirm_delete {
            self.confirm = Some(Confirm::Delete(path));
        } else {
            let content = std::fs::read(&path).unwrap_or_default();
            if let Some(trashed) = self.delete_issue(&path) {
                self.notice = Some((
                    format!("Moved {} to the Trash", self.relative(&path)),
                    Instant::now(),
                ));
                self.undo = Some(Deleted {
                    path,
                    trashed,
                    content,
                    at: Instant::now(),
                });
            }
        }
    }

    /// `⌘Z` on the board: puts the last issue deleted without asking back
    /// where it was.
    fn undo_delete(&mut self) {
        let Some(deleted) = self.undo.take().filter(|d| d.at.elapsed() < UNDO_FOR) else {
            return;
        };
        if deleted.path.exists() {
            self.error = Some(format!(
                "Couldn't restore {}: a file with that name exists now.",
                self.relative(&deleted.path)
            ));
            return;
        }
        let restored = match &deleted.trashed {
            Some(trashed) if std::fs::rename(trashed, &deleted.path).is_ok() => {
                // A freedesktop Trash also holds `info/<name>.trashinfo`.
                if let (Some(files), Some(name)) = (trashed.parent(), trashed.file_name())
                    && let Some(trash) = files.parent().filter(|_| files.ends_with("files"))
                {
                    let mut info = name.to_os_string();
                    info.push(".trashinfo");
                    let _ = std::fs::remove_file(trash.join("info").join(info));
                }
                Ok(())
            }
            _ => atomic_write_bytes(&deleted.path, &deleted.content),
        };
        match restored {
            Ok(()) => {
                self.notice = Some((
                    format!("Restored {}", self.relative(&deleted.path)),
                    Instant::now(),
                ));
                self.refresh();
                self.selected = Some(deleted.path);
                self.scroll_to_selected = true;
            }
            Err(err) => {
                self.error = Some(format!(
                    "Couldn't restore {}: {err}",
                    self.relative(&deleted.path)
                ));
            }
        }
    }

    /// Whether `⌘Z` can still bring back a deleted issue.
    fn can_undo_delete(&self) -> bool {
        self.undo
            .as_ref()
            .is_some_and(|d| d.at.elapsed() < UNDO_FOR)
    }

    /// Deletes an issue's file (to the macOS Trash) and lets the board catch
    /// up. Returns where the Trash put it (`Some(None)` when it can't say),
    /// or `None` when the delete failed.
    fn delete_issue(&mut self, path: &Path) -> Option<Option<PathBuf>> {
        self.confirm = None;
        self.undo = None;
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            self.cancel_review();
        }
        match trash(path) {
            Ok(trashed) => {
                self.notice = Some((format!("Deleted {}", file_name(path)), Instant::now()));
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
                Some(trashed)
            }
            Err(err) => {
                self.error = Some(format!("Couldn't delete {}: {err}", self.relative(path)));
                None
            }
        }
    }

    /// Opens the LabelPicker on `path`, or closes it when it is already open
    /// there (the trigger again).
    fn toggle_label_picker(&mut self, path: PathBuf) {
        if self
            .label_picker
            .as_ref()
            .is_some_and(|picker| picker.path == path)
        {
            self.label_picker = None;
        } else {
            self.label_picker = Some(LabelPicker::new(path));
        }
    }

    /// Draws the open LabelPicker and writes what it adds. It closes when its
    /// issue is gone or no longer on screen (the Editor edits labels in the
    /// title instead).
    fn label_picker_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let on_screen = self.label_picker.as_ref().is_some_and(|picker| {
            self.board.find(&picker.path).is_some()
                && match &self.screen {
                    Screen::Board => true,
                    Screen::Detail(path) => {
                        *path == picker.path && self.detail_mode == DetailMode::Preview
                    }
                }
        });
        if !on_screen {
            self.label_picker = None;
        }
        label_picker::set_open(
            &ctx,
            self.label_picker
                .as_ref()
                .map(|picker| picker.path.as_path()),
        );
        let Some(picker) = self.label_picker.as_mut() else {
            return;
        };
        let has = self
            .board
            .find(&picker.path)
            .map(|issue| issue.labels.clone())
            .unwrap_or_default();
        let board = filter::top_labels(
            self.board.columns.iter().flat_map(|column| &column.issues),
            usize::MAX,
        );
        match label_picker::show(ui, picker, &has, &board, &self.labels) {
            Some(label_picker::Outcome::Close) => self.label_picker = None,
            Some(label_picker::Outcome::Add(label)) => {
                self.add_label(&label, ctx.input(|i| i.time));
            }
            Some(label_picker::Outcome::Remove(label)) => {
                self.remove_label(&label, ctx.input(|i| i.time));
            }
            None => {}
        }
    }

    /// Writes `label` into the picker's issue, then reloads so the pill shows
    /// on the card and in the Reader.
    fn add_label(&mut self, label: &str, now: f64) {
        let Some(path) = self.label_picker.as_ref().map(|picker| picker.path.clone()) else {
            return;
        };
        let shown = path
            .strip_prefix(&self.board.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match labels::add_label_to_file(&path, label) {
            Ok(_) => {
                self.refresh();
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.added(label.to_string(), shown, now);
                }
            }
            Err(message) => {
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.failed(message);
                }
            }
        }
    }

    /// Takes `label` off the picker's issue, then reloads so the pill goes
    /// from the card and the Reader.
    fn remove_label(&mut self, label: &str, now: f64) {
        let Some(path) = self.label_picker.as_ref().map(|picker| picker.path.clone()) else {
            return;
        };
        let shown = path
            .strip_prefix(&self.board.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match labels::remove_label_to_file(&path, label) {
            Ok(_) => {
                self.refresh();
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.removed(label.to_string(), shown, now);
                }
            }
            Err(message) => {
                if let Some(picker) = self.label_picker.as_mut() {
                    picker.failed(message);
                }
            }
        }
    }

    /// Rewrites every issue that carries a renamed label, in the order the
    /// renames were made so chained renames resolve.
    fn rename_labels(&mut self, renames: &[(String, String)]) {
        if renames.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = self
            .board
            .columns
            .iter()
            .flat_map(|column| column.issues.iter().map(|issue| issue.path.clone()))
            .collect();
        let mut changed = false;
        for path in paths {
            for (from, to) in renames {
                match labels::rename_label_to_file(&path, from, to) {
                    Ok(did) => changed |= did,
                    Err(message) => {
                        self.error = Some(message);
                        return;
                    }
                }
            }
        }
        if changed {
            self.refresh();
        }
    }

    /// Moves a done issue out of the board into `issues/.archive`, which
    /// discovery ignores. Never overwrites an archived file.
    fn archive_issue(&mut self, path: &Path) {
        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            self.cancel_review();
        }
        let archive = self.board.root.join(ARCHIVE_DIR);
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
                self.notice = Some((format!("Archived {}", file_name(path)), Instant::now()));
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

    /// "Archive all" at the foot of Done: moves every listed issue to
    /// `.archive/` and says how many went.
    fn archive_all(&mut self, paths: &[PathBuf]) {
        let before = self.board.issue_count();
        for path in paths {
            self.archive_issue(path);
        }
        let archived = before.saturating_sub(self.board.issue_count());
        if archived > 0 {
            let noun = if archived == 1 { "issue" } else { "issues" };
            self.notice = Some((format!("Archived {archived} {noun}"), Instant::now()));
        }
    }

    /// Writes an issue the reader changed via a checkbox.
    fn write_issue(&mut self, path: &Path, contents: &str) {
        match atomic_write(path, contents) {
            Ok(()) => self.refresh(),
            Err(err) => {
                self.error = Some(format!("Couldn't save {}: {err}", self.relative(path)));
            }
        }
    }

    /// The column new issues land in (Todo).
    fn todo_column(&self) -> Option<&Column> {
        self.board
            .columns
            .iter()
            .find(|column| column.accepts_new_issues())
    }

    /// Whether `path` is still being written, in Todo: the only issues that
    /// get the AI review and the "Not written yet" block.
    fn is_todo_issue(&self, path: &Path) -> bool {
        self.column_of(path)
            .is_some_and(|column| column.accepts_new_issues())
    }

    /// Whether `path` can be edited: Doing and Done tickets are the agent's
    /// and the finished ones, so they open read-only.
    fn can_edit(&self, path: &Path) -> bool {
        self.column_of(path)
            .is_some_and(|column| discovery::is_editable_column(&column.dir))
    }

    /// The text an agent should read: the saved file, or the editor draft when
    /// the review is of the current work.
    fn review_content(&self, path: &Path, draft: bool) -> Option<String> {
        if draft {
            self.editor
                .as_ref()
                .filter(|editor| editor.path == path)
                .map(|editor| editor.text.clone())
        } else {
            self.board.find(path).map(|issue| issue.markdown.clone())
        }
    }

    /// Stores a running review and hands the prompt to the agent.
    fn run_review(&mut self, path: PathBuf, draft: bool, prompt: String) {
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        let running = agent::start(&self.agent, &prompt, &path);
        self.review = Some(ReviewState {
            path,
            draft,
            agent: self.agent.label(),
            status: ReviewStatus::Running {
                answer: running.answer,
                pid: running.pid,
            },
            follow_up: String::new(),
        });
    }

    /// Starts a one-shot review of the open issue (or the editor draft).
    fn start_review(&mut self, path: &Path, draft: bool) {
        if !self.is_todo_issue(path) {
            return;
        }
        let Some(content) = self.review_content(path, draft) else {
            return;
        };
        self.run_review(path.to_path_buf(), draft, prompts::review_prompt(&content));
    }

    /// Sends a follow-up ("make it smaller") with the previous answer attached.
    fn ask_again(&mut self) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let path = review.path.clone();
        let draft = review.draft;
        let follow_up = review.follow_up.trim().to_string();
        if follow_up.is_empty() {
            return;
        }
        let Some(content) = self.review_content(&path, draft) else {
            return;
        };
        let previous = match &review.status {
            ReviewStatus::Done { raw, .. } => raw.clone(),
            _ => String::new(),
        };
        let prompt = if previous.is_empty() {
            format!(
                "{}\n\nFollow-up: {follow_up}",
                prompts::review_prompt(&content)
            )
        } else {
            format!(
                "{}\n\nA previous review said:\n{previous}\n\nFollow-up: {follow_up}",
                prompts::review_prompt(&content)
            )
        };
        self.run_review(path, draft, prompt);
    }

    /// Cancels a running review, or closes a finished one.
    fn cancel_review(&mut self) {
        let Some(review) = self.review.take() else {
            return;
        };
        match review.status {
            ReviewStatus::Running { pid, .. } => agent::cancel(pid),
            // A finished review stays reachable from the card's pill.
            ReviewStatus::Done { raw, report } => {
                self.past_reviews.insert(
                    review.path,
                    PastReview {
                        draft: review.draft,
                        agent: review.agent,
                        raw,
                        report,
                    },
                );
            }
            ReviewStatus::Failed { .. } => {}
        }
    }

    /// The card's verdict pill: opens the issue with its last review.
    fn show_past_review(&mut self, path: PathBuf) {
        self.open_issue(path.clone());
        if let Some(past) = self.past_reviews.remove(&path) {
            self.review = Some(ReviewState {
                path,
                draft: past.draft,
                agent: past.agent,
                status: ReviewStatus::Done {
                    raw: past.raw,
                    report: past.report,
                },
                follow_up: String::new(),
            });
        }
    }

    /// Adopts the agent's answer once it is in; called every frame.
    fn poll_review(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        let ReviewStatus::Running { answer, .. } = &review.status else {
            return;
        };
        match answer.try_recv() {
            Ok(Ok(raw)) => {
                let report = agent::parse(&raw);
                review.status = ReviewStatus::Done { raw, report };
            }
            Ok(Err(error)) => review.status = ReviewStatus::Failed { error },
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                review.status = ReviewStatus::Failed {
                    error: "the agent stopped unexpectedly".to_string(),
                };
            }
        }
    }

    /// Whether a review is running for the issue currently in the reader.
    fn review_is_running(&self, path: &Path) -> bool {
        self.review.as_ref().is_some_and(|review| {
            review.path == path && matches!(review.status, ReviewStatus::Running { .. })
        })
    }

    /// The proposed rewrite, when the agent offered one: the editor opens with
    /// it as an unsaved draft, so saving or discarding is the accept or refuse.
    fn apply_review_proposal(&mut self) {
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let path = review.path.clone();
        let proposal = match &review.status {
            ReviewStatus::Done {
                report: Some(report),
                ..
            } => report.proposal.clone(),
            _ => None,
        };
        let Some(proposal) = proposal else {
            return;
        };
        if !self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.path == path)
        {
            self.start_editing(&path);
        }
        if let Some(editor) = self.editor.as_mut()
            && editor.path == path
        {
            editor.text = proposal;
            editor.last_edit = Some(Instant::now());
        }
        self.detail_mode = DetailMode::Edit;
        self.review = None;
    }

    /// Opens the ReviewSettings dialog on the current settings.
    fn open_config(&mut self) {
        self.open_settings(review_settings::Tab::Review);
    }

    /// Settings › Titles, from the composer's engine tag.
    fn open_title_settings(&mut self) {
        self.open_settings(review_settings::Tab::Titles);
    }

    fn open_settings(&mut self, tab: review_settings::Tab) {
        let used: Vec<String> = self
            .board
            .columns
            .iter()
            .flat_map(|column| column.issues.iter())
            .flat_map(|issue| issue.labels.iter().cloned())
            .collect();
        self.settings = Some(
            review_settings::Dialog::new(&self.agent, self.title_engine, self.confirm_delete, tab)
                .with_labels(&self.labels, used),
        );
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
        // New issues belong to the board's Todo column, so leave ColumnFocus.
        self.focused_column = None;
        // Checked on every open: Apple Intelligence or an agent may have been
        // set up meanwhile.
        let engine = Some(self.title_engine).filter(|engine| {
            *engine != Engine::Manual && titles::availability(*engine, &self.agent).is_ready()
        });
        if let Some(engine) = engine {
            titles::prewarm(engine);
        }
        let composer = self.composer.get_or_insert_with(Composer::default);
        composer.engine = engine;
        composer.focus = Some(if composer.stage == TitleStage::Idle {
            ComposerField::Description
        } else {
            ComposerField::Title
        });
    }

    /// Writes `# <title>`, then the description, to a new file in Todo named
    /// after the title. Returns the new file's path; the caller decides
    /// whether to open it.
    fn create_issue(
        &mut self,
        dir: &Path,
        title: &str,
        description: &str,
    ) -> Result<PathBuf, String> {
        let title = title.trim();
        let (name, _) = planned_issue_name(dir, title);
        let path = dir.join(&name);
        let body = description.trim_end();
        let content = if body.trim().is_empty() {
            format!("# {title}\n")
        } else {
            format!("# {title}\n\n{body}\n")
        };
        match atomic_write(&path, &content) {
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

    /// `↩` or `⌘S` in the composer. With no title yet and an engine ready,
    /// it names the issue first and stops on the suggestion; otherwise it
    /// writes the issue and clears the composer for the next one. A click
    /// elsewhere (`close`) never stops: it names if needed, writes, and
    /// closes the composer.
    fn submit_composer(&mut self, close: bool) {
        let Some(composer) = self.composer.as_mut() else {
            return;
        };
        if composer.stage == TitleStage::Naming {
            if close && let Some(run) = composer.naming.as_mut() {
                run.then_close = true;
            }
            return;
        }
        let typed = composer.title.trim().to_string();
        let description = composer.description.clone();
        if typed.is_empty() && description.trim().is_empty() {
            return;
        }
        let title = if !typed.is_empty() {
            typed
        } else if let (Some(engine), TitleStage::Idle | TitleStage::Inferred) =
            (composer.engine, composer.stage)
        {
            composer.naming = Some(NamingRun {
                answer: titles::start(engine, &self.agent, &description),
                started: Instant::now(),
                then_close: close,
            });
            composer.stage = TitleStage::Naming;
            composer.error = None;
            return;
        } else {
            titles::first_line_title(&description)
        };
        self.write_composed(&title, &description, close);
    }

    /// Writes the composed issue to Todo; on success the composer clears (or
    /// closes), on failure it keeps everything and says why.
    fn write_composed(&mut self, title: &str, description: &str, close: bool) {
        let Some(dir) = self.todo_column().map(|c| c.dir.clone()) else {
            return;
        };
        match self.create_issue(&dir, title, description) {
            Ok(path) => {
                // A new ticket lands at position 1, right after the composer.
                // The renumbering is deferred to the end of the frame so a
                // click that added the ticket still reaches the card under it.
                if self.board.find(&path).is_some() {
                    self.pending_front = Some((dir.clone(), path.clone()));
                }
                if close {
                    self.composer = None;
                } else if let Some(composer) = self.composer.as_mut() {
                    *composer = Composer {
                        engine: composer.engine,
                        focus: Some(ComposerField::Description),
                        ..Composer::default()
                    };
                }
                self.selected = Some(path.clone());
                self.new_card = Some((path, Instant::now()));
            }
            Err(err) => {
                if let Some(composer) = self.composer.as_mut() {
                    composer.error = Some(err);
                    composer.focus = Some(ComposerField::Description);
                }
            }
        }
    }

    /// Adopts the engine's title when it lands, or gives up after
    /// [`NAMING_BUDGET`] and lets the user type it.
    fn poll_naming(&mut self, ctx: &egui::Context) {
        let Some(run) = self.composer.as_ref().and_then(|c| c.naming.as_ref()) else {
            return;
        };
        let result = match run.answer.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) if run.started.elapsed() < NAMING_BUDGET => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Empty) => Err("the engine took too long".to_string()),
            Err(TryRecvError::Disconnected) => Err("the engine stopped".to_string()),
        };
        let Some(composer) = self.composer.as_mut() else {
            return;
        };
        let then_close = composer.naming.take().is_some_and(|run| run.then_close);
        let description = composer.description.clone();
        match result {
            Ok(title) if then_close => self.write_composed(&title, &description, true),
            Ok(title) => {
                composer.title = title;
                composer.stage = TitleStage::Inferred;
                composer.select_title = true;
                composer.focus = Some(ComposerField::Title);
            }
            // A click elsewhere never stops: the first line names it.
            Err(_) if then_close => {
                let title = titles::first_line_title(&description);
                self.write_composed(&title, &description, true);
            }
            Err(_) => {
                composer.stage = TitleStage::Failed;
                composer.focus = Some(ComposerField::Title);
            }
        }
    }

    /// `esc` in the composer: while naming, stop and type the title instead;
    /// otherwise close it.
    fn composer_escape(&mut self) {
        match self.composer.as_mut() {
            Some(composer) if composer.stage == TitleStage::Naming => {
                composer.naming = None;
                composer.stage = TitleStage::Skipped;
                composer.focus = Some(ComposerField::Title);
            }
            _ => self.composer = None,
        }
    }

    /// The issues the board shows, column by column, after the filter.
    fn visible_grid(&self) -> Vec<Vec<PathBuf>> {
        self.board
            .columns
            .iter()
            .map(|column| {
                column
                    .issues
                    .iter()
                    .filter(|issue| self.filter.matches(issue))
                    .map(|issue| issue.path.clone())
                    .collect()
            })
            .collect()
    }

    /// Moves the board selection one step with an arrow key.
    fn move_selection(&mut self, key: Key) {
        if self.focused_column.is_some() {
            self.move_focus_selection(key);
            return;
        }
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

    /// Moves the selection inside a focused column: `↑ ↓` step through its
    /// sorted cards, `← →` switch to the neighbouring column.
    fn move_focus_selection(&mut self, key: Key) {
        match key {
            Key::ArrowLeft => self.focus_neighbor(-1),
            Key::ArrowRight => self.focus_neighbor(1),
            _ => {
                let Some(dir) = self.focused_column.clone() else {
                    return;
                };
                let Some(column) = self.board.columns.iter().find(|column| column.dir == dir)
                else {
                    return;
                };
                let paths: Vec<PathBuf> =
                    focus_issues(column, &self.filter, self.column_sort(&dir))
                        .into_iter()
                        .map(|issue| issue.path.clone())
                        .collect();
                if paths.is_empty() {
                    return;
                }
                let at = self
                    .selected
                    .as_ref()
                    .and_then(|selected| paths.iter().position(|path| path == selected));
                let next = match key {
                    Key::ArrowUp => at.map(|at| at.saturating_sub(1)).or(Some(0)),
                    _ => match at {
                        Some(at) if at + 1 < paths.len() => Some(at + 1),
                        Some(at) => Some(at),
                        None => Some(0),
                    },
                };
                if let Some(next) = next {
                    self.selected = Some(paths[next].clone());
                    self.scroll_to_selected = true;
                }
            }
        }
    }

    /// Shows the column `delta` tabs away from the focused one, wrapping.
    fn focus_neighbor(&mut self, delta: isize) {
        let Some(dir) = self.focused_column.clone() else {
            return;
        };
        let count = self.board.columns.len();
        if count == 0 {
            return;
        }
        let Some(index) = self
            .board
            .columns
            .iter()
            .position(|column| column.dir == dir)
        else {
            return;
        };
        let next = (index as isize + delta).rem_euclid(count as isize) as usize;
        self.focused_column = Some(self.board.columns[next].dir.clone());
    }

    /// The selected issue, if it is still on the board.
    fn selected_issue(&self) -> Option<PathBuf> {
        self.selected
            .clone()
            .filter(|path| self.board.find(path).is_some())
    }

    /// A path relative to the issues folder, as the file system names it.
    fn relative(&self, path: &Path) -> String {
        match path.strip_prefix(&self.board.root) {
            Ok(relative) if relative.as_os_str().is_empty() => "./".to_string(),
            Ok(relative) if path.is_dir() => format!("{}/", relative.display()),
            Ok(relative) => relative.display().to_string(),
            Err(_) => display_path(path),
        }
    }

    fn project_name(&self) -> String {
        self.board
            .title
            .clone()
            .or_else(|| {
                self.board
                    .project_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "iboard".to_string())
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
                match self.screen.clone() {
                    Screen::Board => {
                        // A focused column (ColumnFocus) is a board mode: the
                        // header and status bar stay, the board is replaced.
                        self.focused_column = self
                            .focused_column
                            .clone()
                            .filter(|dir| self.board.columns.iter().any(|c| &c.dir == dir));
                        if self.focused_column.is_some() {
                            self.column_focus_ui(ui);
                        } else {
                            self.board_ui(ui);
                        }
                        self.drop_dock(ui);
                    }
                    Screen::Detail(path) => self.detail_ui(ui, &path),
                }
            });
        self.label_picker_ui(ui);
        if let Some(Confirm::Delete(path)) = self.confirm.clone() {
            self.confirm_dialog(ui, &path);
        }
        self.config_window(ui);
        self.design_window(ui);
        self.enrichment_window(ui);
        self.commits_window(ui);
        self.flush_pending_front(ui.ctx());
    }

    /// Moves a just-composed ticket to position 1, once the pointer has settled.
    /// Renumbering changes the cards' ids, so doing it while a click is still
    /// down would steal that click from the card underneath it.
    fn flush_pending_front(&mut self, ctx: &egui::Context) {
        let Some((dir, path)) = self.pending_front.clone() else {
            return;
        };
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.pending_front = None;
        if self.board.find(&path).is_none() {
            return;
        }
        let anchor = self
            .todo_column()
            .and_then(|column| column.issues.first().map(|issue| issue.path.clone()))
            .filter(|first| first != &path);
        let Some(anchor) = anchor else {
            return;
        };
        let Some(new_path) = self.reorder_column(&dir, &path, Some(&anchor)) else {
            return;
        };
        if self
            .new_card
            .as_ref()
            .is_some_and(|(card, _)| card == &path)
        {
            self.new_card = Some((new_path, Instant::now()));
        }
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

    /// ConfirmDialog for a delete, centred over a scrim: `↩` (or the focused
    /// Delete button) confirms; `esc`, Cancel or a click on the scrim cancels.
    fn confirm_dialog(&mut self, ui: &egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let title = self
            .board
            .find(path)
            .map(|issue| issue.title.clone())
            .unwrap_or_else(|| file_name(path));
        let relative = self.relative(path);
        let width = 360.0;
        let pad = 20.0;
        let mut confirmed = false;
        let mut cancelled = false;
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .shadow(ui.visuals().popup_shadow)
            .inner_margin(Margin {
                left: pad as i8,
                right: pad as i8,
                top: pad as i8,
                bottom: theme::SPACE_5 as i8,
            });
        let modal = egui::Modal::new(egui::Id::new("confirm-delete"))
            .frame(frame)
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                let inner = width - 2.0 * pad;
                ui.set_width(inner);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                ui.label(
                    RichText::new("Delete this issue?")
                        .font(theme::ui_title())
                        .color(p.ink),
                );
                // The subject: what goes, by title and by file.
                ui.add_space(theme::SPACE_2);
                Frame::new()
                    .fill(p.surface_sunken)
                    .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                    .inner_margin(Margin::symmetric(10, theme::SPACE_3 as i8))
                    .show(ui, |ui| {
                        ui.set_width(inner - 20.0);
                        ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                        ui.add(
                            egui::Label::new(
                                RichText::new(&title).font(theme::ui_strong()).color(p.ink),
                            )
                            .truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&relative)
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                ui.label(
                    RichText::new(DELETE_CONSEQUENCE)
                        .font(theme::ui_small())
                        .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_3);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                    // Delete starts focused: `Del` then `↩` is the fast path,
                    // and the dialog itself is the safety step.
                    let delete = widgets::button(
                        ui,
                        ButtonKind::Danger,
                        Some(Icon::Trash),
                        "Delete",
                        Some("↩"),
                    );
                    ui.painter().rect_stroke(
                        delete.rect.expand(2.0),
                        CornerRadius::same(theme::RADIUS_SM + 2),
                        Stroke::new(2.0, p.accent),
                        StrokeKind::Outside,
                    );
                    confirmed = delete.clicked();
                    cancelled =
                        widgets::button(ui, ButtonKind::Secondary, None, "Cancel", Some("esc"))
                            .clicked();
                });
            });
        if confirmed {
            self.delete_issue(path);
        } else if cancelled || modal.should_close() {
            self.confirm = None;
        }
    }

    /// The commits popup: the hashes whose message names the ticket's file,
    /// read from git when its node button was pressed. `esc` or the scrim
    /// closes it.
    fn commits_window(&mut self, ui: &egui::Ui) {
        let p = theme::palette(ui);
        let Some(panel) = self.commits.as_ref() else {
            return;
        };
        let width = 460.0;
        let pad = 20.0;
        let mut close = false;
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .shadow(ui.visuals().popup_shadow)
            .inner_margin(Margin {
                left: pad as i8,
                right: pad as i8,
                top: pad as i8,
                bottom: theme::SPACE_5 as i8,
            });
        let modal = egui::Modal::new(egui::Id::new("commits"))
            .frame(frame)
            .backdrop_color(p.scrim)
            .show(ui.ctx(), |ui| {
                let inner = width - 2.0 * pad;
                ui.set_width(inner);
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                ui.label(
                    RichText::new("Commits")
                        .font(theme::ui_title())
                        .color(p.ink),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(&panel.title)
                            .font(theme::ui_strong())
                            .color(p.ink),
                    )
                    .truncate(),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        self.relative(&panel.path),
                        file_name(&panel.path)
                    ))
                    .font(theme::meta())
                    .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_2);
                if panel.list.is_empty() {
                    ui.label(
                        RichText::new("No commit names this ticket.")
                            .font(theme::ui_small())
                            .color(p.ink_muted),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("commits-list")
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
                            for commit in &panel.list {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                    ui.label(
                                        RichText::new(&commit.hash)
                                            .font(egui::FontId::monospace(12.0))
                                            .color(p.accent),
                                    );
                                    ui.label(
                                        RichText::new(commit.day())
                                            .font(theme::meta())
                                            .color(p.ink_muted),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&commit.subject)
                                                .font(theme::ui_small())
                                                .color(p.ink),
                                        )
                                        .truncate(),
                                    );
                                });
                            }
                        });
                }
                ui.add_space(theme::SPACE_2);
                ui.label(
                    RichText::new(
                        "Read from the commit messages that name this file (`closes <file>.md`).",
                    )
                    .font(theme::ui_small())
                    .color(p.ink_muted),
                );
                ui.add_space(theme::SPACE_3);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, ButtonKind::Secondary, None, "Close", Some("esc"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.commits = None;
        }
    }

    /// The DropZones dock: while a card is dragged, Delete (and, for Done
    /// cards, Archive) slides up over the bottom of the board, above the
    /// status bar.
    fn drop_dock(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx().clone();
        let drag = egui::DragAndDrop::payload::<IssueDrag>(&ctx);
        if let Some(drag) = &drag {
            self.dock_archivable = drag.archivable;
        }
        let shown =
            ctx.animate_bool_with_time(egui::Id::new("drop_dock"), drag.is_some(), DOCK_SLIDE);
        if shown <= 0.0 {
            return;
        }
        let p = theme::palette(ui);
        let area = ui.max_rect();
        let top = area.max.y - DOCK_H * shown;
        let dock = Rect::from_min_max(
            Pos2::new(area.min.x, top),
            Pos2::new(area.max.x, top + DOCK_H),
        );
        let archivable = self.dock_archivable;
        let mut dropped = None;
        egui::Area::new(egui::Id::new("drop_dock_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(dock.min)
            .movable(false)
            .constrain(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(area);
                let painter = ui.painter();
                painter.rect_filled(dock, CornerRadius::ZERO, p.bg);
                painter.hline(dock.x_range(), dock.min.y + 0.5, Stroke::new(1.0, p.line));
                // Swallow the pointer over the whole band, so a drop between
                // the zones never lands on the column underneath.
                ui.interact(dock, egui::Id::new("drop_dock_band"), Sense::hover());

                let inner = Rect::from_min_max(
                    dock.min + Vec2::new(theme::SPACE_5, 1.0 + theme::SPACE_4),
                    dock.max - Vec2::new(theme::SPACE_5, theme::SPACE_4),
                );
                // Archive is a Done-only exit: a card from another column only
                // gets Delete, which then spans the whole dock.
                let zones: Vec<(Rect, DropZone)> = if archivable {
                    let half = (inner.width() - theme::SPACE_5) / 2.0;
                    vec![
                        (
                            Rect::from_min_size(inner.min, Vec2::new(half, inner.height())),
                            DropZone::Archive,
                        ),
                        (
                            Rect::from_min_size(
                                Pos2::new(inner.max.x - half, inner.min.y),
                                Vec2::new(half, inner.height()),
                            ),
                            DropZone::Delete,
                        ),
                    ]
                } else {
                    vec![(inner, DropZone::Delete)]
                };
                for (rect, zone) in zones {
                    if let Some(path) = drop_zone(ui, rect, zone, self.confirm_delete) {
                        dropped = Some(match zone {
                            DropZone::Archive => DockDrop::Archive(path),
                            DropZone::Delete => DockDrop::Delete(path),
                        });
                    }
                }
            });
        if shown < 1.0 {
            ctx.request_repaint();
        }
        match dropped {
            Some(DockDrop::Archive(path)) => self.archive_issue(&path),
            Some(DockDrop::Delete(path)) => self.request_delete(path),
            None => {}
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

    /// Board: the issues path. Reader and editor: a
    /// `‹ Board` button, then `project / Column / Issue title`, every segment
    /// but the last a link.
    fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        let project = self.project_name();
        match self.screen.clone() {
            Screen::Board if self.focused_column.is_some() => {
                // ColumnFocus: `project / Column`, the project linking back.
                let dir = self.focused_column.clone().unwrap_or_default();
                let column = self
                    .board
                    .columns
                    .iter()
                    .find(|column| column.dir == dir)
                    .map(|column| column.name.clone())
                    .unwrap_or_default();
                let back = widgets::crumb(ui, &project, theme::ui_body()).clicked();
                ui.label(RichText::new("/").color(p.ink_faint));
                ui.add(
                    egui::Label::new(RichText::new(column).font(theme::ui_strong()).color(p.ink))
                        .truncate(),
                );
                if back {
                    self.unfocus_column();
                }
            }
            Screen::Board => {
                let path = display_path(&self.board.root);
                let response = ui
                    .horizontal(|ui| {
                        let label = ui.add(
                            egui::Label::new(
                                RichText::new(path)
                                    .font(egui::FontId::monospace(11.0))
                                    .color(p.ink_muted),
                            )
                            .truncate()
                            .sense(Sense::click()),
                        );
                        let (rect, caret) =
                            ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::click());
                        if ui.is_rect_visible(rect) {
                            theme::paint_icon(ui.painter(), Icon::ChevronDown, rect, p.ink_faint);
                        }
                        label.union(caret)
                    })
                    .inner
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Switch project");
                self.project_menu(&response);
            }
            Screen::Detail(path) => {
                // The visible way out, for whoever does not know the shortcuts.
                let hint = if self.detail_mode == DetailMode::Edit {
                    "Back to the board (asks if unsaved)"
                } else {
                    "Back to the board (esc)"
                };
                let mut back =
                    widgets::button(ui, ButtonKind::Back, Some(Icon::Back), "Board", Some("esc"))
                        .on_hover_text(hint)
                        .clicked();
                // A 16pt `line-strong` divider between the button and the trail.
                let (divider, _) =
                    ui.allocate_exact_size(Vec2::new(1.0, theme::CONTROL_H), Sense::hover());
                ui.painter().line_segment(
                    [
                        Pos2::new(divider.center().x, divider.center().y - 8.0),
                        Pos2::new(divider.center().x, divider.center().y + 8.0),
                    ],
                    Stroke::new(1.0, p.line_strong),
                );
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                let column = self
                    .board
                    .columns
                    .iter()
                    .find(|c| c.issues.iter().any(|t| t.path == path))
                    .map(|c| c.name.clone());
                let title = self
                    .board
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

    /// The recent-projects menu, opened from the path in the board header. Its
    /// last row asks the OS for a folder; both a remembered and a browsed
    /// folder go through [`Self::open_project`].
    fn project_menu(&mut self, anchor: &egui::Response) {
        let current = recents::canonical(&self.located.project_dir);
        let projects = self.recents.clone();
        let picked = egui::Popup::menu(anchor)
            .width(PROJECT_MENU_WIDTH)
            .show(|ui| {
                ui.set_min_width(PROJECT_MENU_WIDTH);
                ui.spacing_mut().item_spacing.y = theme::SPACE_1;
                let mut picked = None;
                for project in &projects {
                    let title = project
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("project");
                    let detail = display_path(project);
                    if widgets::menu_row(
                        ui,
                        PROJECT_MENU_WIDTH,
                        title,
                        &detail,
                        *project == current,
                    )
                    .clicked()
                    {
                        picked = Some(ProjectChoice::Recent(project.clone()));
                    }
                }
                if !projects.is_empty() {
                    ui.separator();
                }
                if widgets::menu_row(
                    ui,
                    PROJECT_MENU_WIDTH,
                    "Open folder…",
                    "Choose a project containing an issues/ folder",
                    false,
                )
                .clicked()
                {
                    picked = Some(ProjectChoice::Browse);
                }
                picked
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(ProjectChoice::Recent(path)) => self.open_project(path),
            Some(ProjectChoice::Browse) => {
                if let Some(dir) = dialog::pick_folder() {
                    self.open_project(dir);
                }
            }
            None => {}
        }
    }

    /// The header's buttons, laid out right to left.
    fn header_controls(&mut self, ui: &mut egui::Ui) {
        match self.screen {
            Screen::Board => {
                let has_todo = self.todo_column().is_some();
                let new_issue = ui.add_enabled_ui(has_todo, |ui| {
                    widgets::button(
                        ui,
                        ButtonKind::Primary,
                        Some(Icon::Plus),
                        "New issue",
                        Some("⌘N"),
                    )
                    .on_hover_text("Add an issue to Todo")
                });
                if new_issue.inner.clicked() {
                    self.open_composer();
                }
                self.refresh_button(ui);
                if widgets::button(ui, ButtonKind::Secondary, None, "Design", None)
                    .on_hover_text("Browse the project's design system")
                    .clicked()
                {
                    self.open_design();
                }
                widgets::checkbox(ui, &mut self.auto, "Auto")
                    .on_hover_text("Reload when files change on disk, and save edits as you type");
                self.filter_field(ui);
            }
            Screen::Detail(ref path) => {
                let path = path.clone();
                let exists = self.board.find(&path).is_some();
                let done = self
                    .column_of(&path)
                    .map(|column| column.name == "Done")
                    .unwrap_or(false);
                // Reviewing refines an issue, so only Todo issues offer it.
                if self.is_todo_issue(&path) {
                    self.review_button(ui, &path);
                }
                // In the Editor, Save and Discard take Edit's place.
                if self.detail_mode == DetailMode::Edit && exists {
                    if widgets::button(ui, ButtonKind::Primary, None, "Save", Some("⌘S")).clicked()
                    {
                        self.save_and_preview();
                    }
                    if widgets::button(ui, ButtonKind::Secondary, None, "Discard", None)
                        .on_hover_text("Drop unsaved changes and go back to the issue")
                        .clicked()
                    {
                        self.discard_edits();
                    }
                }
                if self.detail_mode == DetailMode::Preview
                    && exists
                    && self.can_edit(&path)
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
                if self.detail_mode == DetailMode::Preview
                    && exists
                    && done
                    && widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(Icon::Commit),
                        "Commits",
                        None,
                    )
                    .on_hover_text("The git commits that name this ticket")
                    .clicked()
                {
                    self.open_commits(&path);
                }
                if self.detail_mode == DetailMode::Preview
                    && exists
                    && widgets::button(ui, ButtonKind::Secondary, None, "Draft design", None)
                        .on_hover_text("Ask the agent for a static mockup of this change")
                        .clicked()
                {
                    self.start_enrichment(&path);
                }
                self.refresh_button(ui);
                // Labelled, and the one control drawn in `danger` at rest. In
                // the Editor `Del` edits text, so the key hint goes.
                let delete_key = (self.detail_mode == DetailMode::Preview).then_some("Del");
                if exists
                    && widgets::button(
                        ui,
                        ButtonKind::DangerOutline,
                        Some(Icon::Trash),
                        "Delete",
                        delete_key,
                    )
                    .on_hover_text(if self.confirm_delete {
                        "Delete this issue (asks to confirm)"
                    } else {
                        "Delete this issue"
                    })
                    .clicked()
                {
                    self.request_delete(path.clone());
                }
                // Add label sits first in the right group, before Delete.
                if exists && self.detail_mode == DetailMode::Preview {
                    let open = self
                        .label_picker
                        .as_ref()
                        .is_some_and(|picker| picker.path == path);
                    let add = widgets::button(
                        ui,
                        if open {
                            ButtonKind::Open
                        } else {
                            ButtonKind::Secondary
                        },
                        Some(Icon::TagPlus),
                        "Add label",
                        Some("L"),
                    )
                    .on_hover_text("Add a label to this issue");
                    set_label_anchor(ui.ctx(), &path, add.rect);
                    if add.clicked() {
                        self.toggle_label_picker(path.clone());
                    }
                }
                if exists
                    && done
                    && self.detail_mode == DetailMode::Preview
                    && widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(Icon::Archive),
                        "Archive",
                        None,
                    )
                    .on_hover_text(format!("Move to {ARCHIVE_DIR}/"))
                    .clicked()
                {
                    self.archive_issue(&path);
                }
            }
        }
    }

    /// The Review split button, rightmost in the Reader and Editor header: the
    /// main part runs the review (or cancels a running one), the caret opens a
    /// menu naming what runs and leading to the AI settings.
    fn review_button(&mut self, ui: &mut egui::Ui, path: &Path) {
        let running = self.review_is_running(path);
        let configured = self.agent.configured();
        let review_label = if running { "Reviewing…" } else { "Review" };
        let review_hint = if running {
            "Cancel the review (esc)"
        } else if configured {
            "Ask an agent to challenge this issue"
        } else {
            "No agent is set up yet: choose one in Settings › AI review (⌘, or the caret)"
        };
        let (main, caret) = widgets::split_button(
            ui,
            egui::Id::new("review-button"),
            ButtonKind::Review,
            Some(Icon::Review),
            review_label,
            Some(if running { "esc" } else { "⌘⇧R" }),
            running || configured,
        );
        let draft = self.detail_mode == DetailMode::Edit;
        if main.on_hover_text(review_hint).clicked() {
            if running {
                self.cancel_review();
            } else if configured {
                self.start_review(path, draft);
            } else {
                self.open_config();
            }
        }
        let caret = caret.on_hover_text("Review options");
        let p = theme::palette(ui);
        let runs_with = if configured {
            self.agent.label()
        } else {
            "No agent set up yet".to_string()
        };
        let picked = egui::Popup::menu(&caret)
            .width(REVIEW_MENU_WIDTH)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| {
                ui.set_min_width(REVIEW_MENU_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let mut picked = None;
                let option = |ui: &mut egui::Ui, name: &str, mono: bool, meta: &str| {
                    widgets::menu_option(
                        ui,
                        widgets::MenuOption {
                            name,
                            mono,
                            muted: false,
                            meta,
                            checked: false,
                            active: false,
                        },
                    )
                };
                if option(ui, "Review this issue", false, "⌘⇧R").clicked() {
                    picked = Some(ReviewMenu::Review);
                }
                ui.add_space(theme::SPACE_2);
                ui.separator();
                widgets::menu_group(ui, "Runs with");
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), theme::CONTROL_H),
                    Sense::hover(),
                );
                ui.painter().text(
                    Pos2::new(
                        rect.min.x + 2.0 * theme::SPACE_3 + theme::ICON_SIZE,
                        rect.center().y,
                    ),
                    egui::Align2::LEFT_CENTER,
                    &runs_with,
                    egui::FontId::monospace(12.0),
                    p.ink,
                );
                if option(ui, "AI settings…", false, "⌘,").clicked() {
                    picked = Some(ReviewMenu::Settings);
                }
                picked
            })
            .and_then(|inner| inner.inner);
        match picked {
            Some(ReviewMenu::Review) if running => {}
            Some(ReviewMenu::Review) if configured => self.start_review(path, draft),
            Some(ReviewMenu::Review | ReviewMenu::Settings) => self.open_config(),
            None => {}
        }
    }

    /// Only offered when Auto is off: with Auto on the board already reloads
    /// itself. `⌘R` keeps working either way.
    fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if self.auto {
            return;
        }
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

    /// The header filter: 220pt at rest, 480pt while in use or filtering,
    /// growing to the left. Focused, it offers the suggestions as chips; a
    /// picked one becomes a token at the start of the field.
    fn filter_field(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let id = filter_id();
        let focused = ctx.memory(|m| m.has_focus(id));
        if !focused {
            self.chip_cursor = None;
        }
        let open = focused || self.filter_open;
        let active = !self.filter.is_empty();

        let target = if open || active {
            FILTER_OPEN
        } else {
            FILTER_REST
        };
        let width = ctx
            .animate_value_with_time(id.with("width"), target, FILTER_GROW_SECS)
            .min(ui.available_width() - CRUMB_KEEP)
            .max(FILTER_REST);
        let (rect, frame) =
            ui.allocate_exact_size(Vec2::new(width, theme::CONTROL_H), Sense::click());
        // A press on a chip or a token's × takes focus from the text for a
        // frame; the field stays open through it.
        let pressed_inside = ctx.input(|i| {
            (i.pointer.any_down() || i.pointer.any_released())
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| rect.contains(pos))
        });
        self.filter_open = focused || (self.filter_open && pressed_inside);
        if frame.clicked() {
            self.focus_filter = true;
        }

        let painter = ui.painter();
        let edge = if active && !open {
            p.accent
        } else {
            p.line_strong
        };
        painter.rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            p.surface,
            Stroke::new(1.0, edge),
            StrokeKind::Inside,
        );
        if open {
            painter.rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(theme::RADIUS_SM + 2),
                Stroke::new(2.0, p.accent),
                StrokeKind::Outside,
            );
        }

        let chips: Vec<String> = if open {
            let issues = self.board.columns.iter().flat_map(|column| &column.issues);
            filter::suggestions(self.suggestions.as_deref(), issues)
                .into_iter()
                .filter(|word| !self.filter.tokens.contains(word))
                .collect()
        } else {
            Vec::new()
        };
        let count = active.then(|| {
            let shown: usize = self.visible_grid().iter().map(Vec::len).sum();
            format!("{shown} of {}", self.board.issue_count())
        });

        let inner = Rect::from_min_max(
            rect.min + Vec2::new(theme::SPACE_3, 0.0),
            rect.max - Vec2::new(theme::SPACE_2, 0.0),
        );
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing.x = FILTER_GAP;
        let (icon, _) = row.allocate_exact_size(Vec2::splat(theme::ICON_SIZE), Sense::hover());
        theme::paint_icon(row.painter(), Icon::Search, icon, p.ink_muted);
        let mut removed = None;
        for (index, token) in self.filter.tokens.iter().enumerate() {
            if widgets::filter_token(&mut row, token).clicked() {
                removed = Some(index);
            }
        }

        // The chips hide when they would crowd the text.
        let try_label = self.filter.is_empty();
        let chips_width: f32 = chips
            .iter()
            .map(|word| widgets::text_width(&row, word, theme::meta()) + 14.0 + FILTER_GAP)
            .sum::<f32>()
            + if try_label {
                widgets::text_width(&row, "Labels", filter_try_font()) + FILTER_GAP
            } else {
                0.0
            };
        let count_width = count
            .as_ref()
            .map(|count| widgets::text_width(&row, count, theme::meta()) + FILTER_GAP)
            .unwrap_or(0.0);
        let text_width = widgets::text_width(&row, &self.filter.text, theme::ui_body());
        let chips = if row.available_width() - count_width - chips_width
            < text_width.max(FILTER_TEXT_MIN)
        {
            Vec::new()
        } else {
            chips
        };

        if focused {
            self.filter_keys(&ctx, &chips);
        }
        let current = self.chip_cursor;
        let mut picked: Option<String> = None;
        let mut response = None;
        row.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = FILTER_GAP;
            for (index, word) in chips.iter().enumerate().rev() {
                if widgets::chip(ui, word, current == Some(index)).clicked() {
                    picked = Some(word.clone());
                }
            }
            if !chips.is_empty() && try_label {
                ui.label(
                    RichText::new("Labels")
                        .font(filter_try_font())
                        .color(p.ink_muted),
                );
            }
            if let Some(count) = &count {
                ui.label(RichText::new(count).font(theme::meta()).color(p.ink_muted));
            }
            if !open {
                let key = if active { "esc" } else { "⌘F" };
                let width = widgets::text_width(ui, key, theme::meta()) + 8.0;
                let (cap, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
                widgets::paint_kbd(ui, key, Pos2::new(cap.min.x, cap.center().y));
            }
            let hint = if self.filter.tokens.is_empty() {
                "Filter issues"
            } else {
                ""
            };
            response = Some(
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.text)
                        .id(id)
                        .frame(Frame::NONE)
                        .hint_text(RichText::new(hint).color(p.ink_faint))
                        .font(theme::ui_body())
                        .margin(Margin::ZERO)
                        .desired_width(ui.available_width()),
                ),
            );
        });

        if let Some(index) = removed {
            self.filter.tokens.remove(index);
            self.focus_filter = true;
        }
        if let Some(word) = picked {
            self.filter.pick(&word);
            self.focus_filter = true;
        }
        if let Some(response) = response
            && std::mem::take(&mut self.focus_filter)
        {
            response.request_focus();
            self.filter_open = true;
        }
    }

    /// The keys the focused filter takes before its text does: `↓` reaches
    /// the chips, `← →` move between them, `↩` picks, `↑` goes back to the
    /// text, and `⌫` in an empty field removes the last token.
    fn filter_keys(&mut self, ctx: &egui::Context, chips: &[String]) {
        let take = |key: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        match self.chip_cursor {
            Some(_) if chips.is_empty() => self.chip_cursor = None,
            Some(at) => {
                let typed =
                    ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
                if typed || take(Key::ArrowUp) {
                    self.chip_cursor = None;
                } else if take(Key::ArrowLeft) {
                    self.chip_cursor = Some(at.saturating_sub(1));
                } else if take(Key::ArrowRight) {
                    self.chip_cursor = Some((at + 1).min(chips.len() - 1));
                } else if take(Key::Enter) {
                    self.filter.pick(&chips[at.min(chips.len() - 1)]);
                    // The picked chip leaves the row; stay on its neighbour.
                    self.chip_cursor = (chips.len() > 1).then(|| at.min(chips.len() - 2));
                } else {
                    take(Key::ArrowDown);
                }
            }
            None => {
                if !chips.is_empty() && take(Key::ArrowDown) {
                    self.chip_cursor = Some(0);
                } else if self.filter.text.is_empty()
                    && !self.filter.tokens.is_empty()
                    && take(Key::Backspace)
                {
                    self.filter.tokens.pop();
                }
            }
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
                    let count = self.board.issue_count();
                    let noun = if count == 1 { "issue" } else { "issues" };
                    let notice = self
                        .notice
                        .as_ref()
                        .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
                        .map(|(text, at)| (text.clone(), *at));
                    if let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
                        ui.label(meta(self.relative(&drag.path)));
                        hints = vec![("esc", "cancel drag")];
                    } else if let Some(deleted) =
                        self.undo.as_ref().filter(|_| self.can_undo_delete())
                    {
                        ui.label(
                            RichText::new(format!(
                                "Moved {} to the Trash",
                                self.relative(&deleted.path)
                            ))
                            .font(theme::meta())
                            .color(p.ink),
                        );
                        ui.ctx()
                            .request_repaint_after(UNDO_FOR.saturating_sub(deleted.at.elapsed()));
                        hints = vec![("⌘Z", "undo"), ("⌘,", "settings")];
                    } else if let Some((text, at)) = notice {
                        ui.label(RichText::new(text).font(theme::meta()).color(p.ink));
                        ui.ctx()
                            .request_repaint_after(NOTICE_FOR.saturating_sub(at.elapsed()));
                        hints = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
                    } else if let Some(dir) = self.focused_column.clone() {
                        // ColumnFocus: what is shown, and how it is sorted.
                        let sort = self.column_sort(&dir);
                        let n = self
                            .board
                            .columns
                            .iter()
                            .find(|column| column.dir == dir)
                            .map(|column| column.issues.len())
                            .unwrap_or(0);
                        let noun = if n == 1 { "issue" } else { "issues" };
                        ui.label(meta(format!(
                            "{} · {} {} · sorted by {}",
                            self.relative(&dir),
                            n,
                            noun,
                            sort.label().to_lowercase()
                        )));
                        hints = vec![("↩", "open"), ("⌘1–4", "column"), ("esc", "board")];
                    } else if self.composer.is_some()
                        && let Some(todo) = self.todo_column()
                    {
                        ui.label(meta(self.relative(&todo.dir)));
                        hints = vec![("↩", "add"), ("⇧↩", "new line"), ("esc", "close")];
                    } else if let Some(selected) = self.selected_issue() {
                        ui.label(meta(self.relative(&selected)));
                        let done = self
                            .board
                            .find(&selected)
                            .is_some_and(|issue| issue.done_at.is_some());
                        hints = if self.can_edit(&selected) {
                            vec![
                                ("↩", "open"),
                                ("E", "edit"),
                                ("⌘↩", "move"),
                                ("Del", "delete"),
                            ]
                        } else if done {
                            vec![("↩", "open"), ("A", "archive"), ("Del", "delete")]
                        } else {
                            vec![("↩", "open"), ("Del", "delete")]
                        };
                    } else {
                        hints = vec![("⌘N", "new"), ("⌘F", "filter"), ("⌘R", "refresh")];
                    }
                    if self.focused_column.is_none() {
                        ui.label(meta(format!("{count} {noun}")));
                    }
                    if !self.board.warnings.is_empty() {
                        let n = self.board.warnings.len();
                        let noun = if n == 1 { "warning" } else { "warnings" };
                        ui.label(
                            RichText::new(format!("{n} {noun}"))
                                .font(theme::meta())
                                .color(p.status_doing),
                        )
                        .on_hover_text(self.board.warnings.join("\n"));
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
                        if let Some(modified) = self.board.find(&path).and_then(|t| t.modified) {
                            ui.label(meta(format!("modified {}", relative_time(modified))));
                        }
                        hints = if self.can_edit(&path) {
                            vec![("E", "edit"), ("Del", "delete"), ("esc", "back")]
                        } else {
                            vec![("Del", "delete"), ("esc", "back")]
                        };
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

    /// Saved / Unsaved / Save failed for the issue being edited.
    fn save_pill(&self, ui: &mut egui::Ui, path: &Path) {
        let p = theme::palette(ui);
        let Some(editor) = self.editor.as_ref().filter(|e| e.path == path) else {
            return;
        };
        let (word, color) = save_state(editor, p);
        widgets::pill(ui, word, color);
    }

    /// Save in the Editor header (`⌘S` too): writes the draft and, when it
    /// went through, shows the issue again.
    fn save_and_preview(&mut self) {
        self.flush_autosave(true);
        if self.editor.as_ref().is_some_and(|e| e.error.is_none()) {
            self.detail_mode = DetailMode::Preview;
        }
    }

    /// The review panel beside the page: the agent's three findings and the
    /// actions on them. The agent only ever proposes; Apply opens the editor.
    fn review_panel(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui);
        let Some(review) = self.review.as_ref() else {
            return;
        };
        let name = file_name(&review.path);
        let source = if review.draft { "draft" } else { "file" };
        let agent = review.agent.clone();
        let mut follow_up = review.follow_up.clone();
        let mut dismiss = false;
        let mut again = false;
        let mut apply = false;
        let mut cancel = false;
        let mut retry = false;
        let running = matches!(review.status, ReviewStatus::Running { .. });

        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.line_strong))
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                Frame::new()
                    .fill(p.review_soft)
                    // The header is flush with the panel's top edge, so its
                    // fill has to round the top corners itself: egui frames
                    // don't clip children, and a square fill would paint over
                    // the panel's rounded corners and their border. One less
                    // than RADIUS_MD matches the inside of the 1pt border.
                    .corner_radius(CornerRadius {
                        nw: theme::RADIUS_MD - 1,
                        ne: theme::RADIUS_MD - 1,
                        sw: 0,
                        se: 0,
                    })
                    .inner_margin(Margin::symmetric(
                        theme::SPACE_4 as i8,
                        theme::SPACE_3 as i8,
                    ))
                    .show(ui, |ui| {
                        // Agent first, so a long file name truncates into the
                        // space left instead of widening the panel (which
                        // flickers while the spinner repaints). The row has a
                        // fixed height: a bare right-to-left layout would
                        // stretch to fill the whole panel.
                        ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), theme::CONTROL_H),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(format!("{agent} · {source}"))
                                        .font(theme::meta())
                                        .color(p.review),
                                );
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!("Review of {name}"))
                                                .font(theme::ui_strong())
                                                .color(p.review),
                                        )
                                        .truncate(),
                                    );
                                });
                            },
                        );
                    });
                Frame::new()
                    .inner_margin(Margin::same(theme::SPACE_4 as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &review.status {
                            ReviewStatus::Running { .. } => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("The agent is reading the issue…")
                                            .color(p.ink_muted),
                                    );
                                });
                            }
                            ReviewStatus::Failed { error } => {
                                ui.label(RichText::new(error).color(p.danger));
                            }
                            ReviewStatus::Done { raw, report } => match report {
                                Some(report) => {
                                    for finding in &report.findings {
                                        let (label, color) = match finding.verdict {
                                            agent::Verdict::Pass => ("Pass", p.status_done),
                                            agent::Verdict::Challenge => ("Challenge", p.review),
                                            agent::Verdict::Block => ("Block", p.danger),
                                        };
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(&finding.axis)
                                                    .font(theme::ui_strong())
                                                    .color(p.ink),
                                            );
                                            widgets::pill(ui, label, color);
                                        });
                                        if !finding.body.is_empty() {
                                            ui.label(
                                                RichText::new(&finding.body).color(p.ink_muted),
                                            );
                                        }
                                        ui.add_space(theme::SPACE_4);
                                    }
                                }
                                None => {
                                    ui.label(RichText::new(raw).color(p.ink_muted));
                                }
                            },
                        }
                        if !running {
                            ui.add_space(theme::SPACE_2);
                            ui.add(
                                egui::TextEdit::singleline(&mut follow_up)
                                    .hint_text(
                                        RichText::new("Ask again: make it smaller…")
                                            .color(p.ink_faint),
                                    )
                                    .font(theme::ui_body())
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    });
                Frame::new()
                    .inner_margin(Margin::symmetric(
                        theme::SPACE_4 as i8,
                        theme::SPACE_3 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), theme::CONTROL_H),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = theme::SPACE_3;
                                let has_proposal = matches!(
                                    &review.status,
                                    ReviewStatus::Done { report: Some(report), .. }
                                        if report.proposal.is_some()
                                );
                                ui.add_enabled_ui(has_proposal, |ui| {
                                    if widgets::button(
                                        ui,
                                        ButtonKind::Primary,
                                        None,
                                        "Apply to draft",
                                        None,
                                    )
                                    .clicked()
                                    {
                                        apply = true;
                                    }
                                });
                                if running {
                                    if widgets::button(
                                        ui,
                                        ButtonKind::Secondary,
                                        None,
                                        "Cancel",
                                        Some("esc"),
                                    )
                                    .clicked()
                                    {
                                        cancel = true;
                                    }
                                } else {
                                    if matches!(review.status, ReviewStatus::Failed { .. })
                                        && widgets::button(
                                            ui,
                                            ButtonKind::Secondary,
                                            None,
                                            "Retry",
                                            None,
                                        )
                                        .clicked()
                                    {
                                        retry = true;
                                    }
                                    if !follow_up.trim().is_empty()
                                        && widgets::button(
                                            ui,
                                            ButtonKind::Secondary,
                                            None,
                                            "Ask again",
                                            None,
                                        )
                                        .clicked()
                                    {
                                        again = true;
                                    }
                                    if widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None)
                                        .clicked()
                                    {
                                        dismiss = true;
                                    }
                                }
                            },
                        );
                    });
            });

        if let Some(review) = self.review.as_mut() {
            review.follow_up = follow_up;
        }
        if cancel {
            self.cancel_review();
        } else if dismiss {
            self.review = None;
        } else if retry {
            let target = self
                .review
                .as_ref()
                .map(|review| (review.path.clone(), review.draft));
            if let Some((path, draft)) = target {
                self.start_review(&path, draft);
            }
        } else if again {
            self.ask_again();
        } else if apply {
            self.apply_review_proposal();
        }
    }

    /// The ReviewSettings dialog (`⌘,`): the tool and model to run, saved to
    /// the user's `iboard.toml`.
    fn config_window(&mut self, ui: &mut egui::Ui) {
        let Some(dialog) = self.settings.as_mut() else {
            return;
        };
        let saved_to = match &self.agent_file {
            Some(file) => format!("Saved to {}", display_path(file)),
            None => "Saved for this session".to_string(),
        };
        match dialog.show(ui, &mut self.catalog, &saved_to) {
            review_settings::Action::Keep => {}
            review_settings::Action::Close => {
                self.settings_tab = dialog.tab;
                self.settings = None;
            }
            review_settings::Action::Save => {
                // The AI review settings are written once they were seen, so
                // saving a title engine never marks an agent as set up.
                let save_review = dialog.review_seen || self.agent.configured();
                if save_review && !dialog.validate() {
                    dialog.tab = review_settings::Tab::Review;
                    return;
                }
                let mut config = dialog.form.clone();
                config.configured = self.agent.configured() || save_review;
                let engine = dialog.engine;
                let confirm_delete = dialog.confirm_delete;
                // `[labels]` is rewritten only once its tab was looked at.
                let label_rows = dialog.labels_seen.then(|| dialog.label_rows.clone());
                let renames = dialog.renames.clone();
                let result = match &self.agent_file {
                    Some(file) => (if save_review {
                        agent::save(file, &config)
                    } else {
                        Ok(())
                    })
                    .and_then(|()| titles::save_engine(file, engine))
                    .and_then(|()| general::save_confirm_delete(file, confirm_delete))
                    .and_then(|()| {
                        if label_rows.is_some() {
                            labels::save(file, label_rows.as_deref().unwrap_or_default())
                        } else {
                            Ok(())
                        }
                    }),
                    None => Ok(()),
                };
                match result {
                    Ok(()) => {
                        self.settings_tab = dialog.tab;
                        if save_review {
                            self.agent = config;
                        }
                        self.title_engine = engine;
                        self.confirm_delete = confirm_delete;
                        if label_rows.is_some() {
                            self.rename_labels(&renames);
                            self.labels = Labels::load(self.agent_file.as_deref());
                        }
                        self.settings = None;
                        // The composer follows the new engine at once.
                        if self.composer.is_some() {
                            self.show_composer();
                        }
                    }
                    Err(error) => dialog.save_error = Some(error),
                }
            }
        }
    }

    /// The work each frame does whatever the keyboard said: autosave, the
    /// periodic refresh, and waking up for a running review.
    /// Opens the Design panel, loading the project's design system once.
    fn open_design(&mut self) {
        match DesignSystem::discover(&self.located.project_dir) {
            Ok(system) => {
                self.design_system = Some(system);
                self.design_error = None;
            }
            Err(err) => {
                self.design_system = None;
                self.design_error = Some(err);
            }
        }
        self.design = Some(design_ui::Panel::default());
    }

    /// The Design panel, and the actions it asks for: scaffold a component, or
    /// render a preview on a background thread. Taking the panel out for the
    /// frame keeps the borrows simple and lets a `Close` drop it.
    fn design_window(&mut self, ui: &mut egui::Ui) {
        let Some(mut panel) = self.design.take() else {
            return;
        };
        let action = panel.show(
            ui.ctx(),
            self.design_system.as_ref(),
            self.design_error.as_deref(),
            design::render::available(),
            self.agent.configured(),
        );
        let keep = self.handle_design_action(&mut panel, action);
        if keep {
            self.design = Some(panel);
        }
    }

    /// Applies what the panel asked for. Returns whether the panel stays open.
    /// Factored out of `design_window` so a test can drive it without a click.
    fn handle_design_action(
        &mut self,
        panel: &mut design_ui::Panel,
        action: design_ui::Action,
    ) -> bool {
        match action {
            design_ui::Action::Keep => true,
            design_ui::Action::Close => false,
            design_ui::Action::Create {
                name,
                group,
                intent,
                draft,
            } => {
                let mut created = false;
                if let Some(system) = self.design_system.as_mut() {
                    match system.add_component(&name, &group, Some(intent.as_str())) {
                        Ok(dir) => {
                            panel.status = Some(format!("Created {}", dir.display()));
                            created = true;
                        }
                        Err(err) => panel.status = Some(err),
                    }
                }
                if created && draft && self.agent.configured() {
                    self.start_design_draft(&name, &group, &intent);
                    panel.status = Some(format!("Drafting {name}…"));
                }
                true
            }
            design_ui::Action::Render(name) => {
                if let Some(system) = self.design_system.as_ref() {
                    match design::render::component_preview(system, &name) {
                        Ok(receiver) => panel.start_render(name, receiver),
                        Err(err) => panel.fail_render(&name, err),
                    }
                }
                true
            }
            design_ui::Action::OpenCatalog => {
                if let Some(system) = self.design_system.as_ref() {
                    match design::catalog::show(system, "light") {
                        Ok(path) => panel.status = Some(format!("Opened {}", path.display())),
                        Err(err) => panel.status = Some(err),
                    }
                }
                true
            }
        }
    }

    /// Ask the configured agent to draft a component's preview fragment. The
    /// prompt is built from the just-scaffolded component and the system.
    fn start_design_draft(&mut self, name: &str, group: &str, intent: &str) {
        let Some(system) = self.design_system.as_ref() else {
            return;
        };
        let prompt = prompts::component_prompt(system, name, group, intent, "light");
        let preview = system
            .root
            .join("components")
            .join(name)
            .join("preview.html");
        let running = agent::start(&self.agent, &prompt, &preview);
        self.design_draft = Some(DraftJob {
            name: name.to_string(),
            running,
        });
    }

    /// Adopt a drafted component as soon as the agent answers. Runs every frame,
    /// so a draft started before the panel closed still lands.
    fn poll_design_draft(&mut self, ctx: &egui::Context) {
        let received = match self.design_draft.as_ref() {
            Some(job) => match job.running.answer.try_recv() {
                Ok(result) => Some((job.name.clone(), result)),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(150));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    Some((job.name.clone(), Err("the agent stopped".to_string())))
                }
            },
            None => None,
        };
        let Some((name, result)) = received else {
            return;
        };
        self.design_draft = None;

        let message = match result {
            Ok(reply) => {
                let html = prompts::clean_html_reply(&reply);
                if html.contains('<') {
                    self.write_drafted_component(&name, &html)
                } else {
                    format!("{name}: the agent returned no HTML")
                }
            }
            Err(err) => format!("{name}: {err}"),
        };
        if let Some(panel) = self.design.as_mut() {
            panel.status = Some(message);
        }
    }

    /// Write a drafted fragment into the component and reload the system.
    fn write_drafted_component(&mut self, name: &str, html: &str) -> String {
        let Some(system) = self.design_system.as_ref() else {
            return format!("{name}: no design system");
        };
        let root = system.root.clone();
        let path = root.join("components").join(name).join("preview.html");
        if let Err(err) = std::fs::write(&path, html) {
            return format!("Couldn't write {}: {err}", path.display());
        }
        if let Ok(reloaded) = DesignSystem::load(&root) {
            self.design_system = Some(reloaded);
        }
        format!("Drafted {name}.")
    }

    /// Start a UX enrichment: an agent drafts a static mockup of the issue,
    /// binding to the project's design system when there is one.
    fn start_enrichment(&mut self, path: &Path) {
        if !self.agent.configured() {
            self.open_config();
            return;
        }
        let Some(issue) = self.board.find(path).map(|issue| issue.markdown.clone()) else {
            return;
        };
        let system = self
            .design_system
            .clone()
            .or_else(|| DesignSystem::discover(&self.located.project_dir).ok());
        let prompt = prompts::enrichment_prompt(&issue, system.as_ref(), None, "light");
        let running = agent::start(&self.agent, &prompt, path);
        self.enrichment = Some(Enrichment {
            path: path.to_path_buf(),
            html: None,
            png: None,
            status: EnrichStatus::Running(running.answer),
            refine: String::new(),
            texture: None,
            note: None,
        });
    }

    /// Re-draft the artifact with the typed refinement attached.
    fn refine_enrichment(&mut self) {
        let Some(enrichment) = self.enrichment.as_ref() else {
            return;
        };
        let instruction = enrichment.refine.trim().to_string();
        let Some(html) = enrichment.html.clone() else {
            return;
        };
        if instruction.is_empty() {
            return;
        }
        let path = enrichment.path.clone();
        let Some(issue) = self.board.find(&path).map(|issue| issue.markdown.clone()) else {
            return;
        };
        let system = self
            .design_system
            .clone()
            .or_else(|| DesignSystem::discover(&self.located.project_dir).ok());
        let prompt = prompts::enrichment_prompt(
            &issue,
            system.as_ref(),
            Some((&html, &instruction)),
            "light",
        );
        let running = agent::start(&self.agent, &prompt, &path);
        if let Some(enrichment) = self.enrichment.as_mut() {
            enrichment.status = EnrichStatus::Running(running.answer);
            enrichment.refine.clear();
            enrichment.note = None;
        }
    }

    /// Adopt the agent's mockup, then its render, as they land.
    fn poll_enrichment(&mut self, ctx: &egui::Context) {
        let Some(enrichment) = self.enrichment.as_mut() else {
            return;
        };
        let outcome = match &enrichment.status {
            EnrichStatus::Running(receiver) => match receiver.try_recv() {
                Ok(result) => EnrichOutcome::Agent(result),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(150));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    EnrichOutcome::Agent(Err("the agent stopped".to_string()))
                }
            },
            EnrichStatus::Rendering(receiver) => match receiver.try_recv() {
                Ok(result) => EnrichOutcome::Png(result),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(120));
                    return;
                }
                Err(TryRecvError::Disconnected) => {
                    EnrichOutcome::Png(Err("the render stopped".to_string()))
                }
            },
            EnrichStatus::Done | EnrichStatus::Failed(_) => return,
        };

        match outcome {
            EnrichOutcome::Agent(Ok(reply)) => {
                let html = prompts::clean_html_reply(&reply);
                if !html.contains('<') {
                    enrichment.status =
                        EnrichStatus::Failed("the agent returned no HTML".to_string());
                    return;
                }
                match write_enrichment_artifact(&enrichment.path, &html) {
                    Ok(file) => {
                        let png = enrichment_dir(&enrichment.path).join("ui.png");
                        enrichment.html = Some(html);
                        enrichment.note = Some(format!("Saved {}", file.display()));
                        enrichment.status = match start_enrichment_render(&file, &png) {
                            Ok(receiver) => EnrichStatus::Rendering(receiver),
                            Err(_) => EnrichStatus::Done,
                        };
                    }
                    Err(err) => enrichment.status = EnrichStatus::Failed(err),
                }
            }
            EnrichOutcome::Agent(Err(err)) => enrichment.status = EnrichStatus::Failed(err),
            EnrichOutcome::Png(Ok(png)) => {
                enrichment.png = Some(png);
                // The file was rewritten; decode it again.
                enrichment.texture = None;
                enrichment.status = EnrichStatus::Done;
            }
            EnrichOutcome::Png(Err(err)) => {
                enrichment.note = Some(format!("preview: {err}"));
                enrichment.status = EnrichStatus::Done;
            }
        }
    }

    /// The enrichment window: the loader, the rendered mockup, and the refine
    /// field. Taking the job out for the frame keeps the borrows simple.
    fn enrichment_window(&mut self, ui: &mut egui::Ui) {
        let Some(mut enrichment) = self.enrichment.take() else {
            return;
        };
        let ctx = ui.ctx().clone();

        if matches!(enrichment.status, EnrichStatus::Done)
            && enrichment.texture.is_none()
            && let Some(png) = enrichment.png.clone()
        {
            enrichment.texture = design_ui::load_texture(&ctx, &png).ok();
        }

        let name = enrichment
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("issue")
            .to_string();
        let drafting = matches!(enrichment.status, EnrichStatus::Running(_));
        let mut close = false;
        let mut refine = false;

        egui::Window::new(format!("Design draft — {name}"))
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .default_height(620.0)
            .show(&ctx, |ui| {
                let p = theme::palette(ui);
                ui.horizontal(|ui| {
                    match &enrichment.status {
                        EnrichStatus::Running(_) => {
                            ui.spinner();
                            ui.label(RichText::new("Drafting the mockup…").color(p.ink_muted));
                        }
                        EnrichStatus::Rendering(_) => {
                            ui.spinner();
                            ui.label(RichText::new("Rendering the preview…").color(p.ink_muted));
                        }
                        EnrichStatus::Failed(error) => {
                            ui.label(RichText::new(error).color(p.danger));
                        }
                        EnrichStatus::Done => {
                            ui.label(RichText::new("Mockup").color(p.ink).strong());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, ButtonKind::Secondary, None, "Close", None).clicked()
                        {
                            close = true;
                        }
                    });
                });

                if let Some(texture) = &enrichment.texture {
                    let size = texture.size_vec2();
                    let max = ui.available_width().min(680.0);
                    let scale = if size.x > max { max / size.x } else { 1.0 };
                    ui.add_space(4.0);
                    ui.image((texture.id(), size * scale));
                } else if let Some(note) = &enrichment.note {
                    ui.label(RichText::new(note).color(p.ink_faint).size(11.0));
                }

                if enrichment.html.is_some() && !drafting {
                    ui.separator();
                    ui.label(RichText::new("Refine").color(p.ink_muted).size(11.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut enrichment.refine)
                            .hint_text("Make the header taller, use the accent for the CTA…")
                            .desired_width(f32::INFINITY),
                    );
                    let ready = !enrichment.refine.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Refine"))
                        .on_disabled_hover_text("Describe what to change")
                        .clicked()
                    {
                        refine = true;
                    }
                }
            });

        if refine {
            self.enrichment = Some(enrichment);
            self.refine_enrichment();
        } else if !close {
            self.enrichment = Some(enrichment);
        }
    }

    fn background(&mut self, ctx: &egui::Context) {
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
        // Poll a running review a few times a second.
        if self
            .review
            .as_ref()
            .is_some_and(|review| matches!(review.status, ReviewStatus::Running { .. }))
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn board_ui(&mut self, ui: &mut egui::Ui) {
        if self.board.columns.is_empty() {
            Frame::new()
                .inner_margin(Margin::same(theme::SPACE_5 as i8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("No columns found in this board.")
                            .color(theme::palette(ui).ink_faint),
                    );
                });
            return;
        }

        // The board is a set of click/drag surfaces, not a text document: no
        // I-beam, no text selection anywhere on it.
        ui.style_mut().interaction.selectable_labels = false;

        let gutter = theme::SPACE_5;
        let count = self.board.columns.len() as f32;
        let available = ui.available_width() - 2.0 * gutter - (count - 1.0) * gutter;
        let width = (available / count).clamp(theme::COLUMN_MIN, theme::COLUMN_MAX);
        let height = (ui.available_height() - 2.0 * gutter).max(120.0);
        let filter = &self.filter;
        let labels = &self.labels;
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
        let mut title_settings = false;
        let mut filter_label: Option<String> = None;
        let mut review: Option<PathBuf> = None;
        let mut show_review: Option<PathBuf> = None;
        let mut add_label: Option<PathBuf> = None;
        let mut focus_column: Option<PathBuf> = None;
        let mut archive_all: Vec<PathBuf> = Vec::new();

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
                    for (index, column) in self.board.columns.iter().enumerate() {
                        let composer = if column.accepts_new_issues() {
                            self.composer.as_mut()
                        } else {
                            None
                        };
                        let view = ColumnView {
                            column,
                            index,
                            filter,
                            width,
                            height,
                            new_card: new_card.as_deref(),
                            selected: selected.as_deref(),
                            scroll_to_selected,
                            root: &self.board.root,
                            labels,
                            past_reviews: &self.past_reviews,
                            can_review: self.agent.configured(),
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
                        title_settings |= actions.title_settings;
                        if filter_label.is_none() {
                            filter_label = actions.filter_label;
                        }
                        if review.is_none() {
                            review = actions.review;
                        }
                        if show_review.is_none() {
                            show_review = actions.show_review;
                        }
                        if add_label.is_none() {
                            add_label = actions.add_label;
                        }
                        if focus_column.is_none() {
                            focus_column = actions.focus_column;
                        }
                        if !actions.archive_all.is_empty() {
                            archive_all = actions.archive_all;
                        }
                    }
                    // Ends the scrollable width one gutter after the last column.
                    ui.allocate_space(Vec2::ZERO);
                });
            });

        if let Some((path, target, anchor)) = dropped {
            self.place_issue(&path, &target, anchor);
        }
        if let Some(path) = delete {
            self.request_delete(path);
        }
        if let Some(path) = archive {
            self.archive_issue(&path);
        }
        if let Some(path) = add_label {
            self.toggle_label_picker(path);
        }
        if compose {
            self.open_composer();
        }
        if let Some(close) = submit {
            self.submit_composer(close);
        } else if close_composer {
            self.composer = None;
        }
        if title_settings {
            self.open_title_settings();
        }
        if let Some(label) = filter_label {
            self.filter.pick(&label);
        }
        if let Some(dir) = focus_column {
            self.focus_column(dir);
        }
        if !archive_all.is_empty() {
            self.archive_all(&archive_all);
        }
        if let Some(path) = review {
            self.open_issue(path.clone());
            self.start_review(&path, false);
        } else if let Some(path) = show_review {
            self.show_past_review(path);
        }
        if let Some(path) = clicked {
            self.open_issue(path);
        }
    }

    /// ColumnFocus: one column across the whole window (the design's
    /// ColumnFocus component). The header stays; under it a focus bar holds the
    /// column tabs, the sort select, Group by label and Board, then the cards
    /// flow in a wide grid.
    fn column_focus_ui(&mut self, ui: &mut egui::Ui) {
        let Some(dir) = self.focused_column.clone() else {
            return;
        };
        let p = theme::palette(ui);

        // The focus bar, over the board's columns.
        let tabs: Vec<FocusTab> = self
            .board
            .columns
            .iter()
            .map(|column| FocusTab {
                name: column.name.clone(),
                count: column.issues.len(),
                color: status_color(p, column),
                active: column.dir == dir,
            })
            .collect();
        let done = self
            .board
            .columns
            .iter()
            .find(|column| column.dir == dir)
            .is_some_and(|column| column.name.eq_ignore_ascii_case("done"));
        let sort = self.column_sort(&dir);
        let mut group = self.group_by_label;
        let action = focus_bar(ui, &tabs, sort, &mut group, done);
        self.group_by_label = group;

        if let Some(at) = action.switch
            && let Some(column) = self.board.columns.get(at)
        {
            if column.dir == dir {
                self.unfocus_column();
            } else {
                self.focus_column(column.dir.clone());
            }
        }
        if let Some((path, at)) = action.drop
            && let Some(column) = self.board.columns.get(at)
        {
            let target = column.dir.clone();
            if path.parent() != Some(target.as_path()) {
                self.move_issue(&path, &target);
            }
        }
        if let Some(sort) = action.sort {
            self.column_sorts.insert(dir.clone(), sort);
        }
        if action.unfocus {
            self.unfocus_column();
        }

        // The bar may have switched or left the focus; redraw with the state it
        // set. When it left, the next frame draws the board.
        let Some(dir) = self.focused_column.clone() else {
            return;
        };
        let Some(column) = self.board.columns.iter().find(|column| column.dir == dir) else {
            return;
        };
        let sort = self.column_sort(&dir);
        let selected = self.selected.clone();
        let scroll_to_selected = std::mem::take(&mut self.scroll_to_selected);
        let new_card = self
            .new_card
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NEW_CARD_FLASH)
            .map(|(path, _)| path.clone());
        let view = FocusGrid {
            column,
            root: &self.board.root,
            filter: &self.filter,
            sort,
            group: self.group_by_label,
            labels: &self.labels,
            sections: &self.board.reader_sections,
            past_reviews: &self.past_reviews,
            can_review: self.agent.configured(),
            selected: selected.as_deref(),
            scroll_to_selected,
            new_card: new_card.as_deref(),
        };
        let actions = focus_grid(ui, &view);

        if let Some(path) = actions.delete {
            self.request_delete(path);
        }
        if let Some(path) = actions.add_label {
            self.toggle_label_picker(path);
        }
        if let Some(path) = actions.archive {
            self.archive_issue(&path);
        }
        if let Some(label) = actions.filter_label {
            self.filter.pick(&label);
        }
        if let Some((path, anchor)) = actions.reorder {
            self.place_issue(&path, &dir, anchor);
        }
        if let Some(path) = actions.review {
            self.open_issue(path.clone());
            self.start_review(&path, false);
        } else if let Some(path) = actions.show_review {
            self.show_past_review(path);
        }
        if let Some(path) = actions.clicked {
            self.open_issue(path);
        }
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        if self.board.find(path).is_none() {
            page(ui, |ui| {
                ui.label(
                    RichText::new(
                        "This issue no longer exists. It may have been moved or deleted.",
                    )
                    .color(theme::palette(ui).ink_muted),
                );
            });
            return;
        }

        if self
            .review
            .as_ref()
            .is_some_and(|review| review.path == path)
        {
            let p = theme::palette(ui);
            egui::Panel::right("review")
                .exact_size(REVIEW_PANEL_WIDTH)
                .resizable(false)
                .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                    left: theme::SPACE_5 as i8,
                    right: theme::SPACE_5 as i8,
                    top: theme::SPACE_5 as i8,
                    bottom: theme::SPACE_5 as i8,
                }))
                .show(ui, |ui| self.review_panel(ui));
        }

        match self.detail_mode {
            DetailMode::Preview => self.detail_preview(ui, path),
            DetailMode::Edit => self.detail_edit(ui, path),
        }
    }

    /// The reader sheet: a `surface` page that reaches down to the status bar.
    /// A meta head (column, path, modified, checklist progress or words), the
    /// rendered markdown, ghost blocks for missing standard sections, and the
    /// empty rest of the sheet, which edits at the end of the file.
    /// Checkboxes are live: ticking one writes the file. Double-clicking the
    /// text opens the editor.
    fn detail_preview(&mut self, ui: &mut egui::Ui, path: &Path) {
        let Some(issue) = self.board.find(path) else {
            return;
        };
        // The title's `[label]` groups show as tags above it, not as text.
        let (mut text, title_line) = sheet::hide_title_labels(&issue.markdown);
        let modified = issue.modified;
        let issue_labels = issue.labels.clone();
        let p = theme::palette(ui);
        let (column_name, status, todo_issue) = self
            .column_of(path)
            .map(|column| {
                (
                    column.name.clone(),
                    status_color(p, column),
                    column.accepts_new_issues(),
                )
            })
            .unwrap_or_else(|| (String::new(), p.status_todo, false));
        let review_ready = self.agent.configured();
        // Doing and Done tickets open read-only: no click-to-edit affordance.
        let editable = self.can_edit(path);
        let relative = self.relative(path);
        let (ticked, tasks) = sheet::checklist(&text);
        let words = sheet::word_count(&text);
        let sections = &self.board.reader_sections;
        // Only Todo issues are still being written: Ready, Doing and Done
        // ones never get the "Not written yet" block.
        let missing: Vec<String> = if !todo_issue {
            Vec::new()
        } else {
            sheet::missing_sections(&text, sections)
                .into_iter()
                .map(str::to_string)
                .collect()
        };
        let none_written = !missing.is_empty() && missing.len() == sections.len();

        // The rendered reader is not a text surface: keep labels inert so a
        // double-click reliably opens the editor instead of selecting a word.
        ui.style_mut().interaction.selectable_labels = false;

        // Two blocks: the sheet, and under it the "Not written yet" block,
        // docked at the bottom of the window. The sheet takes the spare
        // height; the block stays put while a long issue scrolls.
        let full = ui.available_rect_before_wrap();
        let gutter = theme::SPACE_5;
        let page_width = (full.width() - 2.0 * gutter).min(theme::READER_MAX);
        let page_left = full.min.x + ((full.width() - page_width) / 2.0).max(gutter);
        let tiles_across = if page_width >= 560.0 { 4 } else { 2 }.min(missing.len().max(1));
        let block_height = if none_written {
            let rows = missing.len().div_ceil(tiles_across) as f32;
            2.0 * theme::SPACE_4
                + theme::CONTROL_H
                + theme::SPACE_3
                + rows * GHOST_TILE_H
                + (rows - 1.0) * theme::SPACE_3
        } else if !missing.is_empty() {
            40.0
        } else {
            0.0
        };
        let block_gap = if block_height > 0.0 {
            theme::SPACE_4
        } else {
            0.0
        };
        let scroll_height = (full.height() - block_height - block_gap).max(120.0);
        // The sheet fills its share between the header and the block.
        let sheet_height = scroll_height - 2.0 * gutter;
        let mut body_rect = Rect::NOTHING;
        let mut add_section: Option<String> = None;
        let mut draft_review = false;
        let mut edit_at_end = false;
        let mut changed = false;
        let mut label_clicked: Option<String> = None;
        {
            let cache = &mut self.cache;
            let labels = &self.labels;
            egui::ScrollArea::vertical()
                .id_salt(("detail", path))
                .auto_shrink([false, false])
                .max_height(scroll_height)
                .show(ui, |ui| {
                    page(ui, |ui| {
                        let top = ui.cursor().min.y;
                        let inner_height = sheet_height - 2.0 * theme::SPACE_6 - 2.0;

                        sheet_head(
                            ui,
                            SheetHead {
                                column: &column_name,
                                status,
                                path: &relative,
                                modified,
                                checklist: (ticked, tasks),
                                words,
                            },
                        );

                        // The labels, on their own row between the head and
                        // the title; a click filters the board by one.
                        if !issue_labels.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
                                for name in &issue_labels {
                                    if widgets::label_pill(ui, name, labels.color(name))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(format!("Filter by {name}"))
                                        .clicked()
                                    {
                                        label_clicked = Some(name.clone());
                                    }
                                }
                            });
                        }

                        let body = ui.vertical(|ui| {
                            // The body spans the sheet, so a short issue still
                            // gets an outline wide enough for its legend.
                            ui.set_min_width(ui.available_width());
                            // Reading sizes: `read-body` 14, `read-h1` 20.
                            let styles = &mut ui.style_mut().text_styles;
                            styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
                            styles.insert(egui::TextStyle::Heading, theme::read_h1());
                            styles
                                .insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.5));
                            ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                            CommonMarkViewer::new()
                                .default_implicit_uri_scheme(image_base_uri(path))
                                .show_mut(ui, cache, &mut text)
                                .response
                                .changed()
                        });
                        body_rect = body.response.rect;
                        changed = body.inner;

                        // The rest of the sheet: empty, and a click edits at
                        // the end of the file.
                        let used = ui.cursor().min.y - top;
                        let rest_height = (inner_height - used).max(56.0);
                        let (rest, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), rest_height),
                            Sense::click(),
                        );
                        if editable {
                            let response = response.on_hover_cursor(egui::CursorIcon::Text);
                            edit_at_end = response.clicked();
                            paint_rest_hint(ui, rest);
                        }
                    });
                });
        }

        if block_height > 0.0 {
            let rect = Rect::from_min_size(
                Pos2::new(page_left, full.max.y - gutter - block_height),
                Vec2::new(page_width, block_height),
            );
            let mut block = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            let picked = not_written_block(
                &mut block,
                rect,
                &missing,
                none_written,
                tiles_across,
                review_ready,
            );
            match picked {
                Some(NotWritten::Section(section)) => add_section = Some(section),
                Some(NotWritten::Review) => draft_review = true,
                None => {}
            }
        }

        // Hovering the text shows that it is one edit target: a `line-strong`
        // outline with the hint on its top edge.
        let outline = body_rect.expand(theme::SPACE_3);
        if editable && ui.rect_contains_pointer(outline) {
            let painter = ui.painter();
            painter.rect_stroke(
                outline,
                CornerRadius::same(theme::RADIUS_SM),
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Outside,
            );
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
            let right = outline.max.x - theme::SPACE_3;
            let y = outline.min.y;
            let legend = Rect::from_min_max(
                Pos2::new(
                    right - kbd_width - theme::SPACE_2 - galley.size().x - theme::SPACE_2,
                    y - 8.0,
                ),
                Pos2::new(right + theme::SPACE_2, y + 8.0),
            );
            painter.rect_filled(legend, CornerRadius::ZERO, p.surface);
            let kbd_left = right - kbd_width;
            widgets::paint_kbd(ui, "E", Pos2::new(kbd_left, y));
            let text_pos = Pos2::new(
                kbd_left - theme::SPACE_2 - galley.size().x,
                y - galley.size().y / 2.0,
            );
            ui.painter().galley(text_pos, galley, p.ink_muted);
        }
        self.preview_rect = outline;

        if changed {
            if let Some((index, line)) = &title_line {
                text = sheet::restore_line(&text, *index, line);
            }
            self.write_issue(path, &text);
        }
        if let Some(label) = label_clicked {
            self.filter.pick(&label);
            self.back_to_board();
        } else if let Some(section) = add_section {
            self.add_section(path, &section);
        } else if edit_at_end {
            self.start_editing(path);
            self.focus_editor = true;
        } else if draft_review {
            self.start_review(path, false);
        }
    }

    /// Opens the editor with `## <section>` appended, the caret under it.
    /// The draft stays unsaved until the user saves or Auto does.
    fn add_section(&mut self, path: &Path, section: &str) {
        self.start_editing(path);
        if let Some(editor) = self.editor.as_mut().filter(|editor| editor.path == path) {
            editor.text = sheet::append_section(&editor.text, section);
            self.focus_editor = true;
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
                        .hint_text("issue.md")
                        .char_limit(MAX_FILE_NAME_BYTES)
                        .margin(Margin::symmetric(theme::SPACE_2 as i8, 4))
                        .desired_width(240.0),
                );
                if response.changed() {
                    editor.last_edit = Some(Instant::now());
                    ui.ctx().request_repaint_after(AUTOSAVE_AFTER);
                }
                // Save and Discard live in the header; the bar keeps the
                // file and its save state.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (word, color) = save_state(editor, p);
                    widgets::pill(ui, word, color);
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
                                // A freshly created issue: caret after the title.
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
        if let Some(choice) = answer {
            self.answer_leave(choice);
        }
    }
}

/// The `file://` base a relative image link in `issue` resolves against: the
/// issue's own folder, so `assets/<stem>/ui.png` finds the mockup written beside
/// it. Passed to the markdown viewer as its implicit URI scheme.
fn image_base_uri(issue: &Path) -> String {
    let dir = issue.parent().unwrap_or(Path::new("."));
    format!("file://{}/", dir.display())
}

/// Where a ticket's drafted artifacts live: `assets/<stem>/` beside the issue
/// (the layout issue 004 settled on).
fn enrichment_dir(issue: &Path) -> PathBuf {
    let parent = issue.parent().unwrap_or(Path::new("."));
    let stem = issue
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("issue");
    parent.join("assets").join(stem)
}

/// Write a drafted mockup to `assets/<stem>/ui.html`.
fn write_enrichment_artifact(issue: &Path, html: &str) -> Result<PathBuf, String> {
    let dir = enrichment_dir(issue);
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;
    let file = dir.join("ui.html");
    std::fs::write(&file, html)
        .map_err(|err| format!("Couldn't write {}: {err}", file.display()))?;
    Ok(file)
}

/// Render a mockup to `png` on a background thread.
fn start_enrichment_render(
    html: &Path,
    png: &Path,
) -> Result<Receiver<Result<PathBuf, String>>, String> {
    if !design::render::available() {
        return Err("no renderer".to_string());
    }
    let html = html.to_path_buf();
    let png = png.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(design::render::snapshot(&html, &png, 1100, 800));
    });
    Ok(receiver)
}

impl eframe::App for IboardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A project switch sets the title; apply it now that we have a context.
        if let Some(title) = self.pending_title.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        // Adopt an agent's answer as soon as it lands.
        self.poll_review();
        self.poll_naming(ctx);
        self.poll_design_draft(ctx);
        self.poll_enrichment(ctx);
        let typing = ctx.memory(|m| m.focused().is_some());
        let command = |key: Key| ctx.input(|i| i.modifiers.command && i.key_pressed(key));

        // A quit picked from the macOS menu arrives as a flag rather than a
        // window close, so it can run the save prompt before we exit.
        #[cfg(target_os = "macos")]
        if iboard::menu::take_quit_request() {
            self.leave(Leave::Quit);
        }
        #[cfg(target_os = "macos")]
        if iboard::menu::take_settings_request() && self.settings.is_none() {
            self.open_settings(self.settings_tab);
        }

        // Closing the window is one more way of leaving the editor.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            self.leave(Leave::Quit);
            if !self.closing {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if std::mem::take(&mut self.send_close) {
            self.quit(ctx);
        }

        // The delete confirmation owns Escape and Enter while it is up.
        let confirming = self.confirm.is_some();
        if confirming {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.confirm = None;
            } else if ctx.input(|i| i.key_pressed(Key::Enter))
                && let Some(Confirm::Delete(path)) = self.confirm.clone()
            {
                self.delete_issue(&path);
            }
        }

        // The settings dialog owns the keyboard while it is up; `⌘,` closes
        // it as it opened it.
        if self.settings.is_some() {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma)) {
                self.settings = None;
            }
            self.background(ctx);
            return;
        }

        // The LabelPicker closes first; what is left in its field is dropped.
        if !confirming
            && self.label_picker.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.label_picker = None;
        }
        // `L` adds a label to the issue the Reader shows (not in the Editor,
        // where labels are edited in the title).
        if !confirming
            && !typing
            && self.detail_mode == DetailMode::Preview
            && let Screen::Detail(path) = self.screen.clone()
            && self.board.find(&path).is_some()
            && ctx.input(|i| i.key_pressed(Key::L) && i.modifiers.is_none())
        {
            self.toggle_label_picker(path);
        }

        // In the filter, `esc` leaves the chips, then clears, then leaves the
        // field. egui drops focus on `esc` before we see it, so the field
        // asks for it back unless this is the last step.
        if !confirming
            && matches!(self.screen, Screen::Board)
            && self.filter_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            if self.chip_cursor.take().is_some() {
                self.focus_filter = true;
            } else if !self.filter.is_empty() {
                self.filter.clear();
                self.focus_filter = true;
            } else {
                ctx.memory_mut(|m| m.surrender_focus(filter_id()));
                self.filter_open = false;
            }
        }

        // Escape steps back one level, but a review consumes it first: it
        // cancels a run, or closes a finished panel.
        if !confirming && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.review.is_some() {
                self.cancel_review();
            } else {
                match self.screen.clone() {
                    _ if self.leave_prompt.is_some() => self.answer_leave(LeaveChoice::KeepEditing),
                    Screen::Board if self.composer.is_some() => self.composer_escape(),
                    Screen::Board if !self.filter.is_empty() => self.filter.clear(),
                    Screen::Board if self.focused_column.is_some() => self.unfocus_column(),
                    Screen::Board => self.selected = None,
                    Screen::Detail(_) if self.detail_mode == DetailMode::Edit => {
                        self.show_preview();
                    }
                    Screen::Detail(_) => self.back_to_board(),
                }
            }
        }

        if command(Key::N) {
            self.open_composer();
        }
        if command(Key::F) && matches!(self.screen, Screen::Board) {
            self.focus_filter = true;
        }
        // `⌘1`–`⌘4` show a column full screen (ColumnFocus); again goes back.
        if matches!(self.screen, Screen::Board) && !typing {
            for (at, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4]
                .into_iter()
                .enumerate()
            {
                if command(key)
                    && let Some(column) = self.board.columns.get(at)
                {
                    let dir = column.dir.clone();
                    if self.focused_column.as_ref() == Some(&dir) {
                        self.unfocus_column();
                    } else {
                        self.focus_column(dir);
                    }
                }
            }
        }
        if command(Key::R) && !ctx.input(|i| i.modifiers.shift) {
            self.refresh();
        }
        // `⌘Z` brings back an issue deleted without asking; in a text field
        // it stays the field's own undo.
        if command(Key::Z) && !typing && matches!(self.screen, Screen::Board) {
            self.undo_delete();
        }
        // `⌘,` opens Settings on the last tab used; `⌘⇧R` reviews the open
        // issue or draft.
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Comma)) {
            if self.settings.is_some() {
                self.settings = None;
            } else {
                self.open_settings(self.settings_tab);
            }
        }
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::R))
            && let Screen::Detail(path) = self.screen.clone()
        {
            if self.review_is_running(&path) {
                self.cancel_review();
            } else {
                let draft = self.detail_mode == DetailMode::Edit;
                self.start_review(&path, draft);
            }
        }
        if command(Key::OpenBracket) {
            match self.screen {
                Screen::Detail(_) if self.detail_mode == DetailMode::Edit => self.show_preview(),
                Screen::Detail(_) => self.back_to_board(),
                Screen::Board => {}
            }
        }

        // `⌘S` is the header's Save: it forces a save (overwriting a
        // conflict) and, when it went through, shows the issue again.
        // On the board it adds the composer's issue, as `↩` does.
        if command(Key::S) {
            if matches!(self.screen, Screen::Board) && self.composer.is_some() {
                self.submit_composer(false);
            } else if self.detail_mode == DetailMode::Edit {
                self.save_and_preview();
            } else {
                self.flush_autosave(true);
            }
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
            if let Some(path) = self.selected_issue() {
                let done = self
                    .board
                    .find(&path)
                    .is_some_and(|issue| issue.done_at.is_some());
                let command_enter = self.composer.is_none()
                    && ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
                if no_modifiers && ctx.input(|i| i.key_pressed(Key::Enter)) {
                    self.open_issue(path);
                } else if command_enter {
                    self.move_selection_to_next();
                } else if no_modifiers && ctx.input(|i| i.key_pressed(Key::E)) {
                    self.start_editing(&path);
                } else if done && no_modifiers && ctx.input(|i| i.key_pressed(Key::A)) {
                    self.archive_issue(&path);
                } else if no_modifiers && ctx.input(|i| i.key_pressed(Key::L)) {
                    self.toggle_label_picker(path);
                }
            }
        }

        // `Del` (or Backspace) asks to delete the selected card, or the issue
        // the reader is showing. In the editor the key belongs to the text.
        if !confirming && !typing && self.confirm.is_none() {
            let delete_key = ctx.input(|i| {
                (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
                    && i.modifiers.is_none()
            });
            if delete_key {
                match self.screen.clone() {
                    Screen::Board => {
                        if let Some(path) = self.selected_issue() {
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
            let editable = self.can_edit(&path);
            let edit_key = editable
                && !typing
                && ctx.input(|i| i.key_pressed(Key::E) && i.modifiers.is_none());
            let double_clicked_page = editable
                && ctx.input(|i| {
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

        self.background(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

/// Everything `column_ui` needs to know about one column.
struct ColumnView<'a> {
    column: &'a Column,
    index: usize,
    filter: &'a Filter,
    width: f32,
    height: f32,
    /// An issue just created by the composer, still highlighted.
    new_card: Option<&'a Path>,
    /// The card selected with the arrow keys or by opening it.
    selected: Option<&'a Path>,
    /// Scroll the selected card into view this frame.
    scroll_to_selected: bool,
    root: &'a Path,
    /// The label colours, for the chip on each card.
    labels: &'a Labels,
    /// The last review of each issue, for the cards' verdict pills.
    past_reviews: &'a HashMap<PathBuf, PastReview>,
    /// An agent is set up, so the selected Todo card offers Review.
    can_review: bool,
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

/// The column header's title as a button (see the Column component): at rest
/// it is just the uppercase `ui-label`, on hover it takes a `surface-hover`
/// fill and shows a small expand icon in `ink-muted`. Clicking it expands the
/// column to the whole window (ColumnFocus); the tooltip names the shortcut.
fn column_title_button(ui: &mut egui::Ui, name: &str, shortcut: Option<&str>) -> egui::Response {
    let p = theme::palette(ui);
    let label = name.to_uppercase();
    let text = widgets::text_width(ui, &label, theme::ui_label())
        + label.chars().count() as f32 * theme::LABEL_TRACKING;
    let pad = theme::SPACE_2;
    let gap = 6.0;
    let icon = 12.0;
    let width = pad * 2.0 + text + gap + icon;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
    let tooltip = match shortcut {
        Some(key) => format!("Show {name} full screen ({key})"),
        None => format!("Show {name} full screen"),
    };
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let mut job = LayoutJob::default();
        job.append(
            &label,
            0.0,
            egui::TextFormat {
                font_id: theme::ui_label(),
                color: p.ink,
                extra_letter_spacing: theme::LABEL_TRACKING,
                ..Default::default()
            },
        );
        let label_galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(
            Pos2::new(
                rect.min.x + pad,
                rect.center().y - label_galley.size().y / 2.0,
            ),
            label_galley,
            p.ink,
        );
        if response.hovered() {
            let icon_rect = Rect::from_center_size(
                Pos2::new(rect.max.x - pad - icon / 2.0, rect.center().y),
                Vec2::splat(icon),
            );
            theme::paint_icon(painter, Icon::Expand, icon_rect, p.ink_muted);
        }
    }
    response
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
        labels,
        past_reviews,
        can_review,
    } = *view;
    let p = theme::palette(ui);
    let mut actions = ColumnActions::default();
    let visible: Vec<&Issue> = column
        .issues
        .iter()
        .filter(|issue| filter.matches(issue))
        .collect();
    // `todo/`, or empty for the issues at the issues root.
    let folder = match column.dir.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => format!("{}/", relative.display()),
        _ => String::new(),
    };

    // Reserve the whole column as a drop target before drawing into it, so the
    // empty space below the cards accepts a drop too.
    let origin = ui.cursor().min;
    let drop_rect = Rect::from_min_size(origin, Vec2::new(width, height));
    // A double-click on the column (its header or empty space; cards take
    // their own clicks) expands it to the whole window, like its title.
    let drop_response = ui.interact(
        drop_rect,
        egui::Id::new(("column_drop", index)),
        Sense::click(),
    );
    if drop_response.double_clicked() {
        actions.focus_column = Some(column.dir.clone());
    }

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
            // The title is a button: it expands the column to the whole window.
            let shortcut = (index < 4).then(|| ["⌘1", "⌘2", "⌘3", "⌘4"][index]);
            if column_title_button(&mut head, &column.name, shortcut).clicked() {
                actions.focus_column = Some(column.dir.clone());
            }
            head.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                if column.accepts_new_issues()
                    && widgets::button(ui, ButtonKind::Ghost, Some(Icon::Plus), "", None)
                        .on_hover_text("New issue in Todo (⌘N)")
                        .clicked()
                {
                    actions.compose = true;
                }
                // Filtered: "2/6", the total in `ink-faint`.
                let mut count = LayoutJob::default();
                let part = |color| egui::TextFormat::simple(theme::meta(), color);
                if filter.is_empty() {
                    count.append(&column.issues.len().to_string(), 0.0, part(p.ink_muted));
                } else {
                    count.append(&visible.len().to_string(), 0.0, part(p.ink_muted));
                    count.append(&format!("/{}", column.issues.len()), 0.0, part(p.ink_faint));
                }
                ui.label(count);
            });
            ui.painter().line_segment(
                [
                    Pos2::new(head_rect.min.x, head_rect.max.y - 1.0),
                    Pos2::new(head_rect.max.x, head_rect.max.y - 1.0),
                ],
                Stroke::new(2.0, status),
            );

            // The composer stays at the top of Todo; the cards scroll under it.
            if let Some(composer) = composer {
                composer_ui(ui, composer, column, &folder, &mut actions);
            }
            egui::ScrollArea::vertical()
                .id_salt(("column", index))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                    if visible.is_empty() {
                        let message = if column.issues.is_empty() {
                            if folder.is_empty() {
                                "No issues at the root".to_string()
                            } else {
                                format!("No issues in {folder}")
                            }
                        } else {
                            filter.empty_message()
                        };
                        empty_well(ui, &message, width);
                    }
                    let mut cards: Vec<(PathBuf, Rect)> = Vec::new();
                    let visible_paths: Vec<PathBuf> =
                        visible.iter().map(|issue| issue.path.clone()).collect();
                    for issue in visible {
                        let fresh = new_card == Some(issue.path.as_path());
                        let look = if selected == Some(issue.path.as_path()) {
                            CardLook::Selected {
                                scroll: scroll_to_selected,
                            }
                        } else if fresh {
                            CardLook::Fresh
                        } else {
                            CardLook::Plain
                        };
                        let pill = past_reviews.get(&issue.path).map(|past| past.pill(p));
                        let extras = CardExtras {
                            review_pill: pill,
                            review_button: can_review
                                && column.accepts_new_issues()
                                && matches!(look, CardLook::Selected { .. }),
                            ..CardExtras::default()
                        };
                        let (action, rect) = issue_card(ui, issue, width, look, labels, extras);
                        cards.push((issue.path.clone(), rect));
                        match action {
                            Some(CardAction::FilterLabel(label)) => {
                                actions.filter_label = Some(label);
                            }
                            Some(CardAction::Review) => {
                                actions.review = Some(issue.path.clone());
                            }
                            Some(CardAction::ShowReview) => {
                                actions.show_review = Some(issue.path.clone());
                            }
                            Some(CardAction::AddLabel) => {
                                actions.add_label = Some(issue.path.clone());
                            }
                            Some(CardAction::Open) => {
                                actions.clicked = Some(issue.path.clone());
                            }
                            Some(CardAction::Delete) => {
                                actions.delete = Some(issue.path.clone());
                            }
                            Some(CardAction::Archive) => {
                                actions.archive = Some(issue.path.clone());
                            }
                            None => {}
                        }
                    }

                    // At the end of a session, Done can be cleared at once:
                    // the issues it lists (after the filter) go to `.archive/`.
                    if column.name.eq_ignore_ascii_case("done") && visible_paths.len() > 1 {
                        ui.add_space(theme::SPACE_1);
                        let label = format!("Archive the {} issues above", visible_paths.len());
                        if widgets::button(
                            ui,
                            ButtonKind::Secondary,
                            Some(Icon::Archive),
                            &label,
                            None,
                        )
                        .on_hover_text(format!("Move them all to {ARCHIVE_DIR}/"))
                        .clicked()
                        {
                            actions.archive_all = visible_paths.clone();
                        }
                    }

                    // While the DropZones dock covers the bottom of the board,
                    // the last cards can still scroll out from under it.
                    if egui::DragAndDrop::has_payload_of_type::<IssueDrag>(ui.ctx()) {
                        ui.add_space(DOCK_H);
                    }

                    // Where the dragged card would land: before the first card
                    // whose middle is below the pointer, else last.
                    if drop_response.dnd_hover_payload::<IssueDrag>().is_some()
                        || drop_response.dnd_release_payload::<IssueDrag>().is_some()
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

    // Highlight the column while a card from elsewhere hovers over it: the
    // cards' area, under the header and its status rule.
    let hovering_foreign = drop_response
        .dnd_hover_payload::<IssueDrag>()
        .filter(|drag| drag.path.parent() != Some(column.dir.as_path()))
        .is_some();
    if hovering_foreign {
        let rect = drop_rect
            .with_min_y(drop_rect.min.y + theme::CONTROL_H + theme::SPACE_3 / 2.0)
            .expand2(Vec2::new(theme::SPACE_2, 0.0))
            .with_max_y(drop_rect.max.y + theme::SPACE_2);
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

    if let Some(drag) = drop_response.dnd_release_payload::<IssueDrag>() {
        actions.dropped = Some(drag.path.clone());
    }

    actions
}

/// One column tab of ColumnFocus: its name, count and status colour, and
/// whether it is the column being shown.
struct FocusTab {
    name: String,
    count: usize,
    color: Color32,
    active: bool,
}

/// What the ColumnFocus focus bar asked for.
#[derive(Default)]
struct FocusBarAction {
    /// Another tab was clicked: show that column.
    switch: Option<usize>,
    /// The active tab was clicked (or Board): leave ColumnFocus.
    unfocus: bool,
    /// A sort was picked.
    sort: Option<Sort>,
    /// A card was dropped on a tab: its path and the tab's index.
    drop: Option<(PathBuf, usize)>,
}

/// The ColumnFocus focus bar: the segmented column tabs on the left, then the
/// Sort select, Group by label and the bordered Board button on the right,
/// over a `line` rule.
fn focus_bar(
    ui: &mut egui::Ui,
    tabs: &[FocusTab],
    sort: Sort,
    group: &mut bool,
    done: bool,
) -> FocusBarAction {
    let p = theme::palette(ui);
    let mut action = FocusBarAction::default();
    let tabs: Vec<widgets::ColumnTab> = tabs
        .iter()
        .map(|tab| widgets::ColumnTab {
            name: &tab.name,
            count: tab.count,
            color: tab.color,
            active: tab.active,
        })
        .collect();

    let bar = Frame::new()
        .inner_margin(Margin {
            left: theme::SPACE_5 as i8,
            right: theme::SPACE_5 as i8,
            top: theme::SPACE_4 as i8,
            bottom: theme::SPACE_4 as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                let shown = widgets::column_tabs(ui, &tabs);
                if let Some(at) = shown.clicked {
                    action.switch = Some(at);
                }
                // While a card is dragged, the tabs are drop targets: a drop
                // moves it to that column without leaving the view.
                if let Some(payload) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx()) {
                    for (at, rect) in shown.tab_rects.iter().enumerate() {
                        let response =
                            ui.interact(*rect, egui::Id::new(("coltab_drop", at)), Sense::hover());
                        if response.dnd_hover_payload::<IssueDrag>().is_some() {
                            ui.painter().rect_stroke(
                                *rect,
                                CornerRadius::same(3),
                                Stroke::new(2.0, p.accent),
                                StrokeKind::Outside,
                            );
                        }
                        if response.dnd_release_payload::<IssueDrag>().is_some() {
                            action.drop = Some((payload.path.clone(), at));
                        }
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE_4;
                    // Board, the visible way out (`esc`).
                    if widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(Icon::Collapse),
                        "Board",
                        Some("esc"),
                    )
                    .on_hover_text("Back to the whole board (esc)")
                    .clicked()
                    {
                        action.unfocus = true;
                    }
                    if widgets::checkbox(ui, group, "Group by label").changed() {
                        // `group` is mutated in place; nothing else to do.
                    }
                    let response = widgets::select_sized(
                        ui,
                        egui::Id::new("focus-sort"),
                        sort.label(),
                        None,
                        false,
                        170.0,
                    );
                    let options: Vec<Sort> = if done {
                        let mut options = vec![Sort::Finished];
                        options.extend(Sort::MENU);
                        options
                    } else {
                        Sort::MENU.to_vec()
                    };
                    let picked = egui::Popup::menu(&response)
                        .width(170.0)
                        .align(egui::RectAlign::BOTTOM_START)
                        .show(|ui| {
                            ui.set_min_width(170.0);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            let mut picked = None;
                            for option in &options {
                                if widgets::menu_option(
                                    ui,
                                    widgets::MenuOption {
                                        name: option.label(),
                                        mono: false,
                                        muted: false,
                                        meta: "",
                                        checked: *option == sort,
                                        active: false,
                                    },
                                )
                                .clicked()
                                {
                                    picked = Some(*option);
                                }
                            }
                            picked
                        })
                        .and_then(|inner| inner.inner);
                    if let Some(option) = picked {
                        action.sort = Some(option);
                    }
                    let cap = RichText::new("Sort").font(theme::meta()).color(p.ink_muted);
                    ui.label(cap);
                });
            });
        });
    // The `line` rule under the bar, across the whole window.
    ui.painter().hline(
        bar.response.rect.x_range(),
        bar.response.rect.max.y - 0.5,
        Stroke::new(1.0, p.line),
    );
    action
}

/// Everything `focus_grid` needs to know about the focused column.
struct FocusGrid<'a> {
    column: &'a Column,
    root: &'a Path,
    filter: &'a Filter,
    sort: Sort,
    group: bool,
    labels: &'a Labels,
    sections: &'a [String],
    past_reviews: &'a HashMap<PathBuf, PastReview>,
    can_review: bool,
    selected: Option<&'a Path>,
    scroll_to_selected: bool,
    new_card: Option<&'a Path>,
}

/// What the ColumnFocus grid asked for (it mirrors [`ColumnActions`], plus a
/// Board-order reorder).
#[derive(Default)]
struct FocusGridActions {
    clicked: Option<PathBuf>,
    delete: Option<PathBuf>,
    archive: Option<PathBuf>,
    filter_label: Option<String>,
    review: Option<PathBuf>,
    show_review: Option<PathBuf>,
    add_label: Option<PathBuf>,
    /// A card dropped in a Board-order column: it and the card to land in
    /// front of.
    reorder: Option<(PathBuf, Option<PathBuf>)>,
}

/// The focused column's issues, filtered and ordered by `sort`.
fn focus_issues<'a>(column: &'a Column, filter: &Filter, sort: Sort) -> Vec<&'a Issue> {
    let mut issues: Vec<&Issue> = column
        .issues
        .iter()
        .filter(|issue| filter.matches(issue))
        .collect();
    match sort {
        Sort::Board => {}
        Sort::Modified => issues.sort_by_key(|issue| std::cmp::Reverse(issue.modified)),
        Sort::Created => {
            issues.sort_by_key(|issue| std::cmp::Reverse(issue.created.or(issue.modified)))
        }
        Sort::Title => issues.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        Sort::MostToDo => issues.sort_by_key(|issue| {
            let (done, total) = sheet::checklist(&issue.markdown);
            std::cmp::Reverse(total.saturating_sub(done))
        }),
        Sort::Finished => issues.sort_by_key(|issue| {
            std::cmp::Reverse(issue.done_at.map(|at| at.timestamp()).unwrap_or(i64::MIN))
        }),
    }
    issues
}

/// Splits sorted issues into label sections, most-used label first with the
/// unlabelled ones last, for Group by label.
fn group_issues<'a>(issues: Vec<&'a Issue>) -> Vec<(Option<String>, Vec<&'a Issue>)> {
    let mut map: Vec<(String, Vec<&Issue>)> = Vec::new();
    let mut unlabelled: Vec<&Issue> = Vec::new();
    for issue in issues {
        // An issue with two labels shows under both.
        if issue.labels.is_empty() {
            unlabelled.push(issue);
        }
        for label in &issue.labels {
            match map.iter_mut().find(|(name, _)| name == label) {
                Some((_, group)) => group.push(issue),
                None => map.push((label.clone(), vec![issue])),
            }
        }
    }
    map.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let mut groups: Vec<(Option<String>, Vec<&Issue>)> = map
        .into_iter()
        .map(|(name, group)| (Some(name), group))
        .collect();
    if !unlabelled.is_empty() {
        groups.push((None, unlabelled));
    }
    groups
}

/// Where a card dropped at `pos` lands among `cards` (row-major): the index to
/// insert before, or `cards.len()` at the end.
fn insertion_index(cards: &[(PathBuf, Rect)], pos: Pos2) -> usize {
    for (index, (_, rect)) in cards.iter().enumerate() {
        if pos.y < rect.center().y {
            return index;
        }
        if pos.y <= rect.max.y && pos.x < rect.center().x {
            return index;
        }
    }
    cards.len()
}

/// The label heading of a Group by label section: the chip and the count over a
/// `line` rule.
fn section_heading(ui: &mut egui::Ui, label: Option<&str>, count: usize, labels: &Labels) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_4);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        match label {
            Some(label) => {
                widgets::label_pill(ui, label, labels.color(label));
            }
            None => {
                ui.label(
                    RichText::new("No label")
                        .font(theme::ui_strong())
                        .color(p.ink_muted),
                );
            }
        }
        let noun = if count == 1 { "issue" } else { "issues" };
        ui.label(
            RichText::new(format!("{count} {noun}"))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    });
    let y = ui.cursor().min.y + theme::SPACE_1;
    let left = ui.max_rect().min.x;
    let right = ui.max_rect().max.x;
    ui.painter()
        .hline(left..=right, y, Stroke::new(1.0, p.line));
    ui.add_space(theme::SPACE_3);
}

/// The ColumnFocus grid: the focused column's cards at a 300pt-minimum, in
/// `SPACE_4`-apart columns, optionally split by label. Cards are the board's
/// [`issue_card`] with the wide extras: more summary, a facts row and the
/// modified date.
fn focus_grid(ui: &mut egui::Ui, view: &FocusGrid) -> FocusGridActions {
    let p = theme::palette(ui);
    let mut actions = FocusGridActions::default();
    let issues = focus_issues(view.column, view.filter, view.sort);
    let empty = issues.is_empty();
    let groups = if view.group {
        group_issues(issues)
    } else {
        vec![(None, issues)]
    };

    let gutter = theme::SPACE_5;
    let gap = theme::SPACE_4;
    let min = 300.0;
    let available = (ui.available_width() - 2.0 * gutter).max(0.0);
    let columns = (((available + gap) / (min + gap)).floor() as usize).max(1);
    let cell = ((available - (columns as f32 - 1.0) * gap) / columns as f32).max(1.0);

    let mut cards: Vec<(PathBuf, Rect)> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("focus")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(gutter);
            ui.horizontal(|ui| {
                ui.add_space(gutter);
                ui.vertical(|ui| {
                    ui.set_width(available);
                    ui.spacing_mut().item_spacing.y = gap;
                    if empty {
                        let folder = match view.column.dir.strip_prefix(view.root) {
                            Ok(relative) if !relative.as_os_str().is_empty() => {
                                format!("{}/", relative.display())
                            }
                            _ => String::new(),
                        };
                        let message = if view.column.issues.is_empty() {
                            if folder.is_empty() {
                                "No issues at the root".to_string()
                            } else {
                                format!("No issues in {folder}")
                            }
                        } else {
                            view.filter.empty_message()
                        };
                        empty_well(ui, &message, available);
                        return;
                    }
                    for (group_index, (label, group)) in groups.iter().enumerate() {
                        if view.group {
                            if group_index > 0 {
                                ui.add_space(theme::SPACE_4);
                            }
                            section_heading(ui, label.as_deref(), group.len(), view.labels);
                        }
                        // Row by row, `columns` cards each: `horizontal_top`
                        // keeps every card in a row at the same top, and the
                        // vertical `gap` spaces the rows.
                        for row in group.chunks(columns) {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for issue in row {
                                    let fresh = view.new_card == Some(issue.path.as_path());
                                    let look = if view.selected == Some(issue.path.as_path()) {
                                        CardLook::Selected {
                                            scroll: view.scroll_to_selected,
                                        }
                                    } else if fresh {
                                        CardLook::Fresh
                                    } else {
                                        CardLook::Plain
                                    };
                                    let pill =
                                        view.past_reviews.get(&issue.path).map(|past| past.pill(p));
                                    let extras = CardExtras {
                                        review_pill: pill,
                                        review_button: view.can_review
                                            && view.column.accepts_new_issues()
                                            && matches!(look, CardLook::Selected { .. }),
                                        wide: true,
                                        sections: view.sections,
                                    };
                                    let (action, rect) = ui
                                        .allocate_ui_with_layout(
                                            Vec2::new(cell, 0.0),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                ui.set_width(cell);
                                                issue_card(
                                                    ui,
                                                    issue,
                                                    cell,
                                                    look,
                                                    view.labels,
                                                    extras,
                                                )
                                            },
                                        )
                                        .inner;
                                    cards.push((issue.path.clone(), rect));
                                    match action {
                                        Some(CardAction::FilterLabel(label)) => {
                                            actions.filter_label = Some(label);
                                        }
                                        Some(CardAction::Review) => {
                                            actions.review = Some(issue.path.clone());
                                        }
                                        Some(CardAction::ShowReview) => {
                                            actions.show_review = Some(issue.path.clone());
                                        }
                                        Some(CardAction::AddLabel) => {
                                            actions.add_label = Some(issue.path.clone());
                                        }
                                        Some(CardAction::Open) => {
                                            actions.clicked = Some(issue.path.clone());
                                        }
                                        Some(CardAction::Delete) => {
                                            actions.delete = Some(issue.path.clone());
                                        }
                                        Some(CardAction::Archive) => {
                                            actions.archive = Some(issue.path.clone());
                                        }
                                        None => {}
                                    }
                                }
                            });
                        }
                    }
                    ui.add_space(gutter);

                    // Reordering by drag is offered only where the order is
                    // the files' order on the board.
                    if view.sort == Sort::Board
                        && let Some(drag) = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx())
                    {
                        let area = ui.min_rect();
                        let response =
                            ui.interact(area, egui::Id::new("focus_drop"), Sense::hover());
                        let pointer = ui
                            .ctx()
                            .pointer_interact_pos()
                            .or(ui.ctx().pointer_hover_pos());
                        let insert = pointer
                            .map(|pos| insertion_index(&cards, pos))
                            .unwrap_or(cards.len());
                        if response.dnd_hover_payload::<IssueDrag>().is_some()
                            && let Some(pos) = pointer
                            && !cards.is_empty()
                        {
                            let (x, y0, y1) = match cards.get(insert) {
                                Some((_, rect)) => (rect.min.x - gap / 2.0, rect.min.y, rect.max.y),
                                None => {
                                    let last = cards.last().expect("not empty");
                                    (last.1.max.x + gap / 2.0, last.1.min.y, last.1.max.y)
                                }
                            };
                            ui.painter().line_segment(
                                [Pos2::new(x, y0), Pos2::new(x, y1)],
                                Stroke::new(2.0, p.accent),
                            );
                            let _ = pos;
                        }
                        if response.dnd_release_payload::<IssueDrag>().is_some() {
                            actions.reorder = Some((
                                drag.path.clone(),
                                cards.get(insert).map(|(path, _)| path.clone()),
                            ));
                        }
                    }
                });
            });
        });
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
    dashed_rect(ui.painter(), response.rect, p.line_strong);
}

/// A 1pt dashed edge just inside `rect`, as on wells and drop zones.
fn dashed_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &corners,
        Stroke::new(1.0, color),
        4.0,
        3.0,
    ));
}

/// One of the DropZones in the dock.
#[derive(Clone, Copy)]
enum DropZone {
    /// Moves the issue to `.archive/`. Only shown while a Done card is dragged.
    Archive,
    /// Asks to confirm, then deletes.
    Delete,
}

/// Draws a drop zone and returns the dragged issue if it was dropped here:
/// dashed at rest, a solid 2pt edge on a soft fill while a card is over it.
/// Every zone drawn accepts the drop; the dock simply does not draw the zones
/// that would refuse it.
fn drop_zone(ui: &mut egui::Ui, rect: Rect, zone: DropZone, asks: bool) -> Option<PathBuf> {
    let p = theme::palette(ui);
    let id = match zone {
        DropZone::Archive => "drop_zone_archive",
        DropZone::Delete => "drop_zone_delete",
    };
    let response = ui.interact(rect, egui::Id::new(id), Sense::hover());
    let over = response.dnd_hover_payload::<IssueDrag>().is_some();
    let (icon, word, hint) = match zone {
        DropZone::Archive => (Icon::Archive, "Archive", "moves to .archive/"),
        DropZone::Delete if asks => (Icon::Trash, "Delete", "asks to confirm"),
        DropZone::Delete => (Icon::Trash, "Delete", "moves to the Trash"),
    };
    let radius = CornerRadius::same(theme::RADIUS_MD);
    let painter = ui.painter();
    let ink = match zone {
        DropZone::Archive if over => {
            painter.rect(
                rect,
                radius,
                p.accent_soft,
                Stroke::new(2.0, p.accent),
                StrokeKind::Inside,
            );
            p.accent
        }
        DropZone::Archive => {
            painter.rect_filled(rect, radius, p.surface_sunken);
            dashed_rect(painter, rect, p.line_strong);
            p.ink_muted
        }
        DropZone::Delete if over => {
            painter.rect(
                rect,
                radius,
                p.danger_soft,
                Stroke::new(2.0, p.danger),
                StrokeKind::Inside,
            );
            p.danger
        }
        DropZone::Delete => {
            painter.rect_filled(rect, radius, p.surface_sunken);
            dashed_rect(painter, rect, p.danger);
            p.danger
        }
    };

    // An 18pt icon, the word and the hint, centred together.
    let word = painter.layout_no_wrap(word.to_owned(), theme::ui_strong(), ink);
    let hint = painter.layout_no_wrap(hint.to_owned(), theme::meta(), p.ink_muted);
    let icon_size = 18.0;
    let gap = theme::SPACE_3;
    let total = icon_size + gap + word.size().x + gap + hint.size().x;
    let mut x = rect.center().x - total / 2.0;
    let y = rect.center().y;
    theme::paint_icon(
        painter,
        icon,
        Rect::from_min_size(Pos2::new(x, y - icon_size / 2.0), Vec2::splat(icon_size)),
        ink,
    );
    x += icon_size + gap;
    let word_width = word.size().x;
    painter.galley(Pos2::new(x, y - word.size().y / 2.0), word, ink);
    x += word_width + gap;
    painter.galley(Pos2::new(x, y - hint.size().y / 2.0), hint, p.ink_muted);

    response
        .dnd_release_payload::<IssueDrag>()
        .map(|drag| drag.path.clone())
}

/// The quick-add composer: a card framed in `accent` with a reserved title
/// row, a description that grows from 3 to 8 lines, and the file it will
/// write underneath. See `design/components/QuickAdd`.
fn composer_ui(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    column: &Column,
    folder: &str,
    actions: &mut ColumnActions,
) {
    let p = theme::palette(ui);
    let title_id = egui::Id::new("composer");
    let description_id = egui::Id::new("composer-description");
    let naming = composer.stage == TitleStage::Naming;

    // `↩` adds, from either field. `⇧↩` and `⌘↩` are a new line: the
    // description's own return key (as `⇧↩`), or a step down from the title.
    // Taken before the fields see the key.
    let in_title = ui.memory(|m| m.has_focus(title_id));
    let in_description = ui.memory(|m| m.has_focus(description_id));
    if in_title || in_description {
        let (mut add, mut new_line) = (false, false);
        ui.input_mut(|i| {
            i.events.retain_mut(|event| match event {
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.shift || modifiers.command => {
                    new_line = true;
                    *modifiers = egui::Modifiers::SHIFT;
                    in_description
                }
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    ..
                } => {
                    add = true;
                    false
                }
                _ => true,
            })
        });
        if !naming && add {
            actions.submit = Some(false);
        }
        if in_title && new_line && !naming {
            composer.focus = Some(ComposerField::Description);
        }
    }

    let frame = Frame::new()
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
            let width = ui.available_width();

            // Title row: always 22pt, so no state makes the card jump.
            let (row, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
            let rule_y = row.max.y - 0.5;
            let rule = [Pos2::new(row.min.x, rule_y), Pos2::new(row.max.x, rule_y)];
            // A title typed by hand is Manual, whatever the engine: `↩` won't
            // infer one.
            let manual =
                composer.stage != TitleStage::Inferred && !composer.title.trim().is_empty();
            let engine = if manual { None } else { composer.engine };
            match (composer.stage, engine) {
                (TitleStage::Failed | TitleStage::Skipped, _) => {
                    ui.painter()
                        .line_segment(rule, Stroke::new(1.0, p.status_doing));
                }
                (_, None) => {
                    ui.painter().extend(Shape::dashed_line(
                        &rule,
                        Stroke::new(1.0, p.line_strong),
                        4.0,
                        3.0,
                    ));
                }
                _ => {
                    ui.painter().line_segment(rule, Stroke::new(1.0, p.line));
                }
            }
            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row.with_max_y(row.max.y - 4.0))
                    .layout(Layout::right_to_left(Align::Center)),
            );
            row_ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            let (tag, inferred) = match (composer.stage, engine) {
                (TitleStage::Inferred, _) => (Some("Inferred"), true),
                (TitleStage::Failed | TitleStage::Skipped, _) => (None, false),
                (_, Some(engine)) => (Some(engine.tag()), false),
                (_, None) => (Some(Engine::Manual.tag()), false),
            };
            if let Some(tag) = tag {
                let response = widgets::tag(&mut row_ui, tag, inferred);
                if !inferred {
                    let response = response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("Title inference (⌘, › Titles)");
                    if response.clicked() {
                        actions.title_settings = true;
                    }
                }
            }
            let chars = composer.title.chars().count();
            if chars >= titles::MAX_TITLE_CHARS - 10 {
                row_ui.label(
                    RichText::new(format!("{chars}/{}", titles::MAX_TITLE_CHARS))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }
            if naming {
                row_ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
                    let pulse = (ui.input(|i| i.time) * std::f64::consts::TAU).sin() as f32;
                    ui.painter().circle_filled(
                        dot.center(),
                        3.0,
                        p.status_doing.gamma_multiply(0.65 + 0.35 * pulse),
                    );
                    ui.label(
                        RichText::new("Naming…")
                            .font(theme::ui_body())
                            .color(p.ink_muted),
                    );
                    ui.ctx().request_repaint();
                });
            } else {
                let hint = match (composer.stage, composer.engine) {
                    (TitleStage::Failed | TitleStage::Skipped, _) => "Type a title",
                    (_, None) => "Add a title",
                    _ => "Title is named from the description",
                };
                let mut output = egui::TextEdit::singleline(&mut composer.title)
                    .id(title_id)
                    .font(theme::ui_strong())
                    .hint_text(
                        RichText::new(hint)
                            .font(theme::ui_body())
                            .color(p.ink_faint),
                    )
                    .char_limit(titles::MAX_TITLE_CHARS)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .desired_width(row_ui.available_width())
                    .show(&mut row_ui);
                if composer.focus == Some(ComposerField::Title) {
                    composer.focus = None;
                    output.response.request_focus();
                    if std::mem::take(&mut composer.select_title) {
                        let end = CCursor::new(composer.title.chars().count());
                        output
                            .state
                            .cursor
                            .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
                        output.state.store(ui.ctx(), title_id);
                    }
                }
                // Editing a suggestion makes it the user's own title.
                if output.response.changed() && composer.stage == TitleStage::Inferred {
                    composer.stage = TitleStage::Idle;
                }
            }

            // Description: 3 lines to start, 8 at most, then it scrolls with
            // its last line fading out.
            let line = 18.0;
            let scroll = egui::ScrollArea::vertical()
                .id_salt("composer-description-scroll")
                .max_height(8.0 * line)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add_enabled(
                        !naming,
                        egui::TextEdit::multiline(&mut composer.description)
                            .id(description_id)
                            .font(theme::ui_body())
                            .text_color(if naming { p.ink_muted } else { p.ink })
                            .hint_text(RichText::new("Describe the issue").color(p.ink_faint))
                            .frame(Frame::NONE)
                            .margin(Margin::ZERO)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .return_key(Some(egui::KeyboardShortcut::new(
                                egui::Modifiers::SHIFT,
                                Key::Enter,
                            ))),
                    )
                });
            let capped = scroll.content_size.y > scroll.inner_rect.height() + 1.0;
            if capped
                && scroll.state.offset.y + scroll.inner_rect.height() < scroll.content_size.y - 1.0
            {
                fade_out(ui, scroll.inner_rect, p.surface);
            }
            let description = scroll.inner;
            if composer.focus == Some(ComposerField::Description) {
                composer.focus = None;
                description.request_focus();
            }
            // Clicking elsewhere closes an empty composer; one with text stays.
            let empty = composer.title.trim().is_empty() && composer.description.trim().is_empty();
            let still_focused = ui.memory(|m| m.has_focus(title_id) || m.has_focus(description_id));
            if empty && description.lost_focus() && !still_focused && composer.focus.is_none() {
                actions.close_composer = true;
            }

            composer_meta(ui, composer, column, folder);
        });

    // A click anywhere else on the board (another card, blank space) adds
    // what was typed and closes the composer.
    let pressed = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.primary_pressed())
    });
    let clicked_away = pressed.is_some_and(|pos| {
        !frame.response.rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
    });
    if clicked_away {
        if composer.is_empty() {
            actions.close_composer = true;
        } else {
            actions.submit = Some(true);
        }
    }
}

/// The composer's last line: the file it will write (or why it can't name
/// it yet) on the left, the keys that apply on the right.
fn composer_meta(ui: &mut egui::Ui, composer: &Composer, column: &Column, folder: &str) {
    let p = theme::palette(ui);
    let meta = |text: &str, color: Color32| RichText::new(text).font(theme::meta()).color(color);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if let Some(error) = &composer.error {
            ui.add(egui::Label::new(meta(error, p.danger)).truncate());
            return;
        }
        let title = composer.title.trim();
        let described = !composer.description.trim().is_empty();
        let (hints, left): (&[(&str, &str)], Vec<RichText>) = match composer.stage {
            TitleStage::Naming => (
                &[("esc", "type it")],
                vec![meta(folder, p.ink_muted), meta("naming", p.ink_faint)],
            ),
            TitleStage::Failed if title.is_empty() => (
                &[("↩", "add")],
                vec![meta(
                    "Couldn't name it · ↩ uses the first line",
                    p.status_doing,
                )],
            ),
            _ if !title.is_empty() => {
                let (name, taken) = planned_issue_name(&column.dir, title);
                let color = if taken {
                    p.status_doing
                } else if composer.stage == TitleStage::Inferred {
                    p.status_done
                } else {
                    p.ink_muted
                };
                (
                    &[("↩", "add"), ("⇧↩", "new line")],
                    vec![meta(&format!("{folder}{name}"), color)],
                )
            }
            TitleStage::Skipped | TitleStage::Failed => (
                &[("↩", "add")],
                vec![
                    meta(folder, p.ink_muted),
                    meta("empty title uses the first line", p.ink_faint),
                ],
            ),
            _ if composer.engine.is_none() => (
                &[("↩", "add")],
                vec![
                    meta(folder, p.ink_muted),
                    meta("empty title uses the first line", p.ink_faint),
                ],
            ),
            _ => (
                if described {
                    &[("↩", "add"), ("⇧↩", "new line")]
                } else {
                    &[("↩", "add"), ("esc", "close")]
                },
                vec![
                    meta(folder, p.ink_muted),
                    meta("named on save", p.ink_faint),
                ],
            ),
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            for (key, action) in hints.iter().rev() {
                ui.horizontal(|ui| widgets::hint(ui, key, action));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for part in left {
                    ui.add(egui::Label::new(part).truncate());
                }
            });
        });
    });
}

/// Fades the bottom 22pt of `rect` into `ground`, to show more text below.
fn fade_out(ui: &egui::Ui, rect: Rect, ground: Color32) {
    let band = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 22.0), rect.max);
    let mut mesh = egui::Mesh::default();
    let clear = Color32::from_rgba_unmultiplied(ground.r(), ground.g(), ground.b(), 0);
    mesh.colored_vertex(band.left_top(), clear);
    mesh.colored_vertex(band.right_top(), clear);
    mesh.colored_vertex(band.right_bottom(), ground);
    mesh.colored_vertex(band.left_bottom(), ground);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

/// The editor's save state as a pill word and its colour.
fn save_state(editor: &EditorState, p: &Palette) -> (&'static str, Color32) {
    if editor.error.is_some() {
        ("Save failed", p.danger)
    } else if editor.conflict.is_some() {
        ("Changed on disk", p.danger)
    } else if editor.is_dirty() {
        ("Unsaved", p.status_doing)
    } else {
        ("Saved", p.status_done)
    }
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

/// Draws one issue card. A single click reads it; dragging the card carries it
/// to another column, with a copy following the cursor. Returns the action the
/// card asked for (if any) and the rect it occupies.
fn issue_card(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    look: CardLook,
    labels: &Labels,
    extras: CardExtras,
) -> (Option<CardAction>, Rect) {
    let p = theme::palette(ui);
    let card_id = egui::Id::new(("issue_card", &issue.path));

    // The card is one click-and-drag surface. Labels must not steal the pointer
    // to select text (which also showed an I-beam cursor and hijacked the drag).
    ui.style_mut().interaction.selectable_labels = false;

    // Actions show on hover or selection, never while any card is dragged.
    // Hover is read from last frame's rect, since the body is drawn first.
    let drag = egui::DragAndDrop::payload::<IssueDrag>(ui.ctx());
    let dragging_self = drag.as_ref().is_some_and(|drag| drag.path == issue.path);
    let rect_id = card_id.with("rect");
    let hovered = ui
        .ctx()
        .data(|d| d.get_temp::<Rect>(rect_id))
        .is_some_and(|rect| ui.rect_contains_pointer(rect));
    // An open LabelPicker keeps its card's actions, and so its trigger, shown.
    let picking = label_picker_open(ui.ctx()).as_deref() == Some(issue.path.as_path());
    let show_actions =
        drag.is_none() && (hovered || picking || matches!(look, CardLook::Selected { .. }));

    let scope = ui.scope_builder(
        egui::UiBuilder::new()
            .id(card_id)
            .sense(Sense::click_and_drag()),
        |ui| {
            // The card being dragged stays in place, faded.
            if dragging_self {
                ui.set_opacity(0.4);
            }
            card_body(
                ui,
                issue,
                width,
                look != CardLook::Plain,
                Some(show_actions),
                labels,
                extras,
            )
        },
    );
    let body = scope.inner;
    let response = scope.response;
    ui.ctx().data_mut(|d| d.insert_temp(rect_id, response.rect));

    // Dragging the card carries its file path; a drop on a column moves it,
    // a drop on the DropZones dock archives or deletes it.
    response.dnd_set_drag_payload(IssueDrag {
        path: issue.path.clone(),
        archivable: issue.done_at.is_some(),
    });

    // The action icons are their own click targets: while the pointer is over
    // one, the card must not override the pointer cursor the icon set with its
    // own grab cursor below (and a click there must not open the issue).
    let over_action = show_actions
        && ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|pos| body.action_rects.iter().any(|rect| rect.contains(pos)));

    // `esc` cancels a drag by dropping the payload: the card stays put.
    if response.dragged() && dragging_self {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

        // Paint a copy of the card under the pointer, so the issue stays
        // visible for the whole move instead of only at its ends. Anchoring it
        // to the original card position plus the total drag delta keeps the
        // pointer at the same spot on the card where it was grabbed.
        if let Some(delta) = response.total_drag_delta() {
            egui::Area::new(egui::Id::new(("issue_ghost", &issue.path)))
                .order(egui::Order::Tooltip)
                .fixed_pos(response.rect.min + delta)
                .constrain(false)
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    ui.style_mut().interaction.selectable_labels = false;
                    ui.set_max_width(width);
                    // The lifted copy: `shadow-popover` and a `line-strong` edge.
                    let lifted = Frame::new()
                        .shadow(ui.visuals().popup_shadow)
                        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                        .show(ui, |ui| {
                            card_body(ui, issue, width, false, None, labels, CardExtras::default())
                        });
                    ui.painter().rect_stroke(
                        lifted.response.rect,
                        CornerRadius::same(theme::RADIUS_MD),
                        Stroke::new(1.0, p.line_strong),
                        StrokeKind::Inside,
                    );
                });
        }
    } else if response.hovered() && drag.is_none() && !over_action {
        // Grab cursor: this card can be clicked or picked up, but an action
        // button under the pointer keeps its own pointing hand.
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

    let clicked_open = (response.clicked() || response.double_clicked()) && !over_action;
    let action = body.action.or(clicked_open.then_some(CardAction::Open));

    (action, response.rect)
}

/// What `card_body` drew: the action a child button asked for, and the rects
/// those buttons cover.
struct CardBody {
    action: Option<CardAction>,
    action_rects: Vec<Rect>,
}

/// One entry of a wide card's facts row: a small `meta` label, with a check
/// icon when the section is written.
fn fact(ui: &mut egui::Ui, text: &str, color: Color32, checked: bool) {
    let icon = 12.0;
    let gap = 4.0;
    let text_width = widgets::text_width(ui, text, theme::meta());
    let width = if checked { icon + gap } else { 0.0 } + text_width;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
    let cy = rect.center().y;
    let mut x = rect.min.x;
    if checked {
        let icon_rect = Rect::from_center_size(Pos2::new(x + icon / 2.0, cy), Vec2::splat(icon));
        theme::paint_icon(ui.painter(), Icon::Check, icon_rect, color);
        x += icon + gap;
    }
    ui.painter().text(
        Pos2::new(x, cy),
        egui::Align2::LEFT_CENTER,
        text,
        theme::meta(),
        color,
    );
}

/// A wide card's facts: each standard section, `true` when it is written (and
/// so gets a check; acceptance criteria carries its checklist count), `false`
/// and `Name —` when it is missing. A bare issue is one "No sections yet".
fn facts(issue: &Issue, sections: &[String]) -> Vec<(String, bool)> {
    let missing = sheet::missing_sections(&issue.markdown, sections);
    if sections
        .iter()
        .all(|section| missing.contains(&section.as_str()))
    {
        return vec![("No sections yet".to_string(), false)];
    }
    let (done, total) = sheet::checklist(&issue.markdown);
    sections
        .iter()
        .map(|section| {
            let present = !missing.contains(&section.as_str());
            if !present {
                (format!("{section} —"), false)
            } else if section.eq_ignore_ascii_case("Acceptance criteria") {
                (format!("Criteria {done}/{total}"), true)
            } else {
                (section.clone(), true)
            }
        })
        .collect()
}

/// A wide card's facts row: the written sections checked in `status-done`, the
/// missing ones as `Out of scope —` in `ink-muted`.
fn fact_row(ui: &mut egui::Ui, issue: &Issue, sections: &[String]) {
    let p = theme::palette(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 4.0);
        for (text, checked) in facts(issue, sections) {
            let color = if checked { p.status_done } else { p.ink_muted };
            fact(ui, &text, color, checked);
        }
    });
}

/// A card's modified date, friendly: "today 18:02", "yesterday", "2 Oct".
fn card_date(time: std::time::SystemTime) -> String {
    let at = chrono::DateTime::<chrono::Local>::from(time);
    let date = at.date_naive();
    let today = chrono::Local::now().date_naive();
    if date == today {
        format!("today {}", at.format("%H:%M"))
    } else if Some(date) == today.pred_opt() {
        "yesterday".to_string()
    } else {
        at.format("%-d %b").to_string()
    }
}

/// The issue whose LabelPicker is open, for its card.
fn label_picker_open(ctx: &egui::Context) -> Option<PathBuf> {
    label_picker::open_path(ctx)
}

/// Where a LabelPicker trigger sits, so the popup opens under it.
fn set_label_anchor(ctx: &egui::Context, path: &Path, rect: Rect) {
    label_picker::set_anchor(ctx, path, rect);
}

/// The visual body of a card, shared by the card in the board and the ghost
/// that follows the pointer while dragging: title, summary clamped to three
/// lines, file name. A `highlighted` card (new or selected) is `accent-soft`.
/// The meta row ends with a slot for the delete (and, for Done, archive) icon
/// buttons; `actions` says whether they show, and `None` (the lifted copy
/// while dragging) leaves it empty. The slot is always reserved so the file
/// name and date never move when the buttons appear.
fn card_body(
    ui: &mut egui::Ui,
    issue: &Issue,
    width: f32,
    highlighted: bool,
    actions: Option<bool>,
    labels: &Labels,
    extras: CardExtras,
) -> CardBody {
    let p = theme::palette(ui);
    // Wide cards (ColumnFocus) get more room: 12×14 padding and 6pt gaps.
    let padding = if extras.wide {
        Margin::symmetric(14, 12)
    } else {
        Margin::symmetric(theme::SPACE_4 as i8, 10)
    };
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
            ui.spacing_mut().item_spacing.y = if extras.wide { 6.0 } else { theme::SPACE_2 };
            // Labels sit on their own row above the title, 4pt apart and
            // wrapping; no labels, no row.
            let mut label_clicks: Vec<(String, Rect, bool)> = Vec::new();
            if !issue.labels.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
                    for label in &issue.labels {
                        let pill = widgets::label_pill(ui, label, labels.color(label))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(format!("Filter by {label}"));
                        label_clicks.push((label.clone(), pill.rect, pill.clicked()));
                    }
                });
            }
            ui.add(
                egui::Label::new(
                    RichText::new(&issue.title)
                        .font(theme::ui_strong())
                        .color(p.ink),
                )
                .wrap(),
            );
            if !issue.summary.is_empty() {
                let mut job =
                    LayoutJob::simple(issue.summary.clone(), theme::ui_small(), p.ink_muted, inner);
                // Wider cards show twice the summary: six lines, not three.
                job.wrap.max_rows = if extras.wide { 6 } else { 3 };
                ui.add(egui::Label::new(job));
            }
            if extras.wide {
                fact_row(ui, issue, extras.sections);
            }
            let meta = ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut action = None;
                    let mut action_rects = Vec::new();
                    // Add label, Review and Delete, plus Archive on Done:
                    // 70pt, or 94pt on Done.
                    let slots = if issue.done_at.is_some() { 4.0 } else { 3.0 };
                    let slot = Vec2::new(
                        slots * widgets::CARD_ACTION + (slots - 1.0) * theme::SPACE_1,
                        widgets::CARD_ACTION,
                    );
                    ui.allocate_ui_with_layout(slot, Layout::right_to_left(Align::Center), |ui| {
                        ui.set_min_size(slot);
                        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                        let Some(visible) = actions else {
                            return;
                        };
                        // Hidden actions are also disabled, so a click
                        // there opens the card instead.
                        if !visible {
                            ui.set_invisible();
                        }
                        let delete = widgets::icon_button(
                            ui,
                            Icon::Trash,
                            egui::Id::new(("card_delete", &issue.path)),
                            "Delete  Del",
                            widgets::Tone::Danger,
                        );
                        action_rects.push(delete.rect);
                        if delete.clicked() {
                            action = Some(CardAction::Delete);
                        }
                        if issue.done_at.is_some() {
                            let archive = widgets::icon_button(
                                ui,
                                Icon::Archive,
                                egui::Id::new(("card_archive", &issue.path)),
                                "Archive  A",
                                widgets::Tone::Plain,
                            );
                            action_rects.push(archive.rect);
                            if archive.clicked() {
                                action = Some(CardAction::Archive);
                            }
                        }
                        // The selected Todo card can be reviewed from here.
                        if extras.review_button {
                            let review = widgets::icon_button(
                                ui,
                                Icon::Review,
                                egui::Id::new(("card_review", &issue.path)),
                                "Review  ⌘⇧R",
                                widgets::Tone::Review,
                            );
                            action_rects.push(review.rect);
                            if review.clicked() {
                                action = Some(CardAction::Review);
                            }
                        }
                        // First of the cluster: Add label, which opens the
                        // LabelPicker under it.
                        let picking = label_picker_open(ui.ctx()).as_deref() == Some(&issue.path);
                        let add_label = widgets::icon_button(
                            ui,
                            Icon::TagPlus,
                            egui::Id::new(("card_add_label", &issue.path)),
                            "Add label  L",
                            if picking {
                                widgets::Tone::Open
                            } else {
                                widgets::Tone::Plain
                            },
                        );
                        action_rects.push(add_label.rect);
                        set_label_anchor(ui.ctx(), &issue.path, add_label.rect);
                        if add_label.clicked() {
                            action = Some(CardAction::AddLabel);
                        }
                    });
                    // The last review's verdict: a click reopens it.
                    if let Some((word, color)) = extras.review_pill {
                        let pill = widgets::pill(ui, word, color)
                            .interact(Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("Show the last review");
                        action_rects.push(pill.rect);
                        if pill.clicked() {
                            action = Some(CardAction::ShowReview);
                        }
                    }
                    // Wide cards always show the modified date, as on the
                    // board's Done cards: "today 18:02", "yesterday", "2 Oct".
                    if extras.wide {
                        if let Some(modified) = issue.modified {
                            ui.add(egui::Label::new(
                                RichText::new(card_date(modified))
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            ))
                            .on_hover_text(relative_time(modified));
                        }
                    } else if let Some(done_at) = issue.done_at {
                        // Done issues: the day on the right, the exact time on hover.
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
                                RichText::new(issue.file_name())
                                    .font(theme::meta())
                                    .color(p.ink_muted),
                            )
                            .truncate(),
                        );
                    });
                    (action, action_rects)
                })
                .inner
            });
            let (mut action, mut action_rects) = meta.inner;
            // Each label is its own click target: it filters the board.
            for (label, rect, clicked) in label_clicks {
                action_rects.push(rect);
                if clicked {
                    action = Some(CardAction::FilterLabel(label));
                }
            }
            (action, action_rects)
        });
    CardBody {
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

/// Height of a ghost tile in the "Not written yet" block.
const GHOST_TILE_H: f32 = 72.0;

/// What the "Not written yet" block asked for.
enum NotWritten {
    Section(String),
    Review,
}

/// The "Not written yet" block under the reader sheet: its own `surface`
/// card. With nothing written, a head row (label, Draft with Review) over a
/// grid of ghost tiles; once some sections exist, one line of chips.
fn not_written_block(
    ui: &mut egui::Ui,
    rect: Rect,
    missing: &[String],
    none_written: bool,
    across: usize,
    review_ready: bool,
) -> Option<NotWritten> {
    let p = theme::palette(ui);
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.surface,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
    let inner = rect.shrink2(Vec2::new(theme::SPACE_5, theme::SPACE_4));
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    let mut picked = None;
    let label = |ui: &mut egui::Ui| {
        ui.label(
            RichText::new("NOT WRITTEN YET")
                .font(theme::ui_label())
                .extra_letter_spacing(theme::LABEL_TRACKING)
                .color(p.ink_muted),
        );
    };
    if !none_written {
        // One 40pt line: the label, then a chip per missing section.
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            label(ui);
            for section in missing {
                if widgets::chip(ui, &format!("+ {section}"), false)
                    .on_hover_text(format!("Add ## {section} in the editor"))
                    .clicked()
                {
                    picked = Some(NotWritten::Section(section.clone()));
                }
            }
        });
        return picked;
    }

    ui.allocate_ui_with_layout(
        Vec2::new(inner.width(), theme::CONTROL_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            label(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let kind = if review_ready {
                    ButtonKind::Review
                } else {
                    ButtonKind::Disabled
                };
                let response = widgets::button(
                    ui,
                    kind,
                    Some(Icon::Review),
                    "Draft with Review",
                    Some("⌘⇧R"),
                );
                if review_ready {
                    if response
                        .on_hover_text("Ask the review agent to propose these sections")
                        .clicked()
                    {
                        picked = Some(NotWritten::Review);
                    }
                } else {
                    response.on_hover_text(
                        "No agent is set up yet: choose one in Settings › AI review (⌘,)",
                    );
                }
            });
        },
    );
    ui.add_space(theme::SPACE_3);
    let gap = theme::SPACE_3;
    let tile_w = (inner.width() - gap * (across as f32 - 1.0)) / across as f32;
    let top = ui.cursor().min.y;
    for (index, section) in missing.iter().enumerate() {
        let (row, col) = (index / across, index % across);
        let tile = Rect::from_min_size(
            Pos2::new(
                inner.min.x + col as f32 * (tile_w + gap),
                top + row as f32 * (GHOST_TILE_H + gap),
            ),
            Vec2::new(tile_w, GHOST_TILE_H),
        );
        let mut cell = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tile)
                .layout(Layout::top_down(Align::Min)),
        );
        if widgets::ghost_section(&mut cell, section, sheet::section_hint(section))
            .on_hover_text(format!("Add ## {section} in the editor"))
            .clicked()
        {
            picked = Some(NotWritten::Section(section.clone()));
        }
    }
    picked
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

/// What the reader sheet's head strip shows.
struct SheetHead<'a> {
    column: &'a str,
    status: Color32,
    path: &'a str,
    modified: Option<std::time::SystemTime>,
    /// `(ticked, total)` checkboxes; with none, the word count shows instead.
    checklist: (usize, usize),
    words: usize,
}

/// The sheet's `meta` head: the column as a status mark and the file path on
/// the left, "modified 1h ago" and checklist progress (or the word count) on
/// the right, over a `line` rule.
fn sheet_head(ui: &mut egui::Ui, head: SheetHead) {
    let p = theme::palette(ui);
    let meta = |text: String, color: Color32| {
        egui::Label::new(RichText::new(text).font(theme::meta()).color(color))
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if !head.column.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, head.status);
                ui.label(
                    RichText::new(head.column.to_uppercase())
                        .font(theme::ui_label())
                        .extra_letter_spacing(theme::LABEL_TRACKING)
                        .color(p.ink),
                );
            });
            ui.add(meta("·".into(), p.ink_faint));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (ticked, total) = head.checklist;
            if total > 0 {
                ui.add(meta(format!("{ticked}/{total} done"), p.ink_muted));
                ui.spacing_mut().item_spacing.x = 6.0;
                widgets::meter(ui, ticked, total);
                ui.spacing_mut().item_spacing.x = 10.0;
            } else {
                let unit = if head.words == 1 { "word" } else { "words" };
                ui.add(meta(format!("{} {unit}", head.words), p.ink_muted));
            }
            if let Some(modified) = head.modified {
                ui.add(meta("·".into(), p.ink_faint));
                ui.add(meta(
                    format!("modified {}", relative_time(modified)),
                    p.ink_muted,
                ));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(meta(head.path.to_string(), p.ink_muted).truncate());
            });
        });
    });
    ui.add_space(theme::SPACE_4);
    let (rule, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(rule.x_range(), rule.center().y, Stroke::new(1.0, p.line));
    ui.add_space(20.0);
}

/// "Click below the text to keep writing E" in `ink-faint`, at the foot of
/// the empty rest of the sheet.
fn paint_rest_hint(ui: &egui::Ui, rest: Rect) {
    let p = theme::palette(ui);
    let text = egui::WidgetText::from(
        RichText::new("Click below the text to keep writing")
            .font(theme::ui_small())
            .color(p.ink_faint),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let kbd_width = 18.0;
    let gap = 6.0;
    let width = theme::ICON_SIZE + gap + text.size().x + gap + kbd_width;
    let y = rest.max.y - 8.0;
    let mut x = rest.center().x - width / 2.0;
    let icon = Rect::from_min_size(
        Pos2::new(x, y - theme::ICON_SIZE / 2.0),
        Vec2::splat(theme::ICON_SIZE),
    );
    theme::paint_icon(ui.painter(), Icon::Edit, icon, p.ink_faint);
    x += theme::ICON_SIZE + gap;
    ui.painter().galley(
        Pos2::new(x, y - text.size().y / 2.0),
        text.clone(),
        p.ink_faint,
    );
    x += text.size().x + gap;
    widgets::paint_kbd(ui, "E", Pos2::new(x, y));
}

/// The next selected `(column, row)` after an arrow key on the board's visible
/// issues. With nothing selected, any arrow selects the first card. Up and down
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
/// most 60 characters. A title with no ASCII letters or digits gives `issue`.
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
        "issue".to_string()
    } else {
        slug
    }
}

/// The file name the composer will write for `title` in `dir`, and whether the
/// plain slug was taken (so a `-2`, `-3`... suffix was added). The slug leaves
/// the title's `[label]` groups out.
fn planned_issue_name(dir: &Path, title: &str) -> (String, bool) {
    let slug = slugify(&labels::display_title(title));
    let mut candidate = format!("{slug}.md");
    let mut n = 2;
    while dir.join(&candidate).exists() {
        candidate = format!("{slug}-{n}.md");
        n += 1;
    }
    (candidate, n > 2)
}

/// Writes via a hidden temp file and rename, so a crash cannot leave a
/// half-written issue. The temp name starts with `.` and is ignored by discovery.
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    atomic_write_bytes(path, contents.as_bytes())
}

fn atomic_write_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("issue.md");
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));

    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    std::fs::rename(&temp, path)
}

/// Where a deleted issue goes, as the confirmation dialog tells the user.
#[cfg(target_os = "macos")]
const DELETE_CONSEQUENCE: &str =
    "The file is moved to the macOS Trash. You can restore it from there.";
#[cfg(not(target_os = "macos"))]
const DELETE_CONSEQUENCE: &str = "The file is moved to the Trash. You can restore it from there.";

/// Deletes a file the way `DELETE_CONSEQUENCE` says: to the Trash on macOS,
/// for good elsewhere. Tests delete for good, so they never fill the Trash.
/// Returns where the Trash put the file, when it can say.
fn trash(path: &Path) -> std::io::Result<Option<PathBuf>> {
    #[cfg(target_os = "macos")]
    if !cfg!(test) {
        use objc2::rc::Retained;
        use objc2_foundation::{NSFileManager, NSString, NSURL};

        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let mut resulting: Option<Retained<NSURL>> = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|err| std::io::Error::other(err.localizedDescription().to_string()))?;
        return Ok(resulting
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string())));
    }
    #[cfg(not(target_os = "macos"))]
    if !cfg!(test) {
        return freedesktop_trash(path).map(Some);
    }
    std::fs::remove_file(path).map(|()| None)
}

/// Moves `path` to the freedesktop Trash (`$XDG_DATA_HOME/Trash`, by default
/// `~/.local/share/Trash`): the file under `files/`, and a `.trashinfo` under
/// `info/` so file managers can restore it. Returns where the file went.
#[cfg(not(target_os = "macos"))]
fn freedesktop_trash(path: &Path) -> std::io::Result<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| std::io::Error::other("no home folder for the Trash"))?;
    trash_into(path, &data.join("Trash"))
}

/// [`freedesktop_trash`] into a given Trash folder. Built on every OS so it is
/// tested everywhere; only Linux and the BSDs call it.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn trash_into(path: &Path, trash: &Path) -> std::io::Result<PathBuf> {
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let original = std::fs::canonicalize(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "issue.md".to_string());
    // A free name: `issue.md`, then `issue.2.md`, `issue.3.md`...
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
        None => (name.clone(), String::new()),
    };
    let mut candidate = name.clone();
    let mut n = 2;
    while files.join(&candidate).exists() || info.join(format!("{candidate}.trashinfo")).exists() {
        candidate = format!("{stem}.{n}{ext}");
        n += 1;
    }
    let encoded: String = original
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(
        info.join(format!("{candidate}.trashinfo")),
        format!("[Trash Info]\nPath={encoded}\nDeletionDate={date}\n"),
    )?;
    let target = files.join(&candidate);
    // Another file system: copy, then remove.
    if std::fs::rename(path, &target).is_err() {
        std::fs::copy(path, &target)?;
        std::fs::remove_file(path)?;
    }
    Ok(target)
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
        egui_extras::install_image_loaders(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "iboard-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_content_without_leaving_temp_files() {
        let dir = scratch_dir("atomic");
        let file = dir.join("issue.md");
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
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\n- [ ] one\n- [ ] two\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        let ctx = themed_ctx();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };

        let pass = |app: &mut IboardApp| {
            let mut output = ctx.run_ui(raw.clone(), |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
        };

        pass(&mut app); // board

        app.open_issue(file.clone());
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

    /// A labeled issue shows its chip on the card and in the reader header,
    /// with the label split off the display title, without rewriting the file.
    #[test]
    fn renders_a_labeled_issue() {
        let dir = scratch_dir("labels");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("labeled.md");
        let raw_markdown = "# [Discovery] Hey look!\n\nA labeled idea.\n";
        std::fs::write(&file, raw_markdown).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        let ctx = themed_ctx();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let pass = |app: &mut IboardApp| {
            let mut output = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
            output.textures_delta.clear();
        };

        let issue = app.board.find(&file).unwrap();
        assert_eq!(issue.title, "Hey look!");
        assert_eq!(issue.labels, ["discovery"]);

        pass(&mut app); // board: chip beside the card title

        app.open_issue(file.clone());
        pass(&mut app); // reader: chip in the head strip

        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            raw_markdown,
            "the raw file is never rewritten for a label"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The reader sheet draws ghost blocks for a bare issue and chips for a
    /// partly written one; adding a section opens the editor with the heading
    /// appended, unsaved, and leaves the file alone.
    #[test]
    fn reader_sheet_offers_missing_sections() {
        let dir = scratch_dir("sheet");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let bare = todo.join("bare.md");
        let partial = todo.join("partial.md");
        std::fs::write(&bare, "# Bare\n\nJust an idea.\n").unwrap();
        std::fs::write(
            &partial,
            "# Partial\n\n## Context\n\nWhy.\n\n- [x] a\n- [ ] b\n",
        )
        .unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        let ctx = themed_ctx();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let pass = |app: &mut IboardApp| {
            let mut output = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
            output.textures_delta.clear();
        };

        app.open_issue(bare.clone());
        pass(&mut app); // ghost blocks
        app.open_issue(partial.clone());
        pass(&mut app); // "Add a section" chips and checklist progress

        app.add_section(&partial, "Acceptance criteria");
        assert_eq!(app.detail_mode, DetailMode::Edit);
        assert!(app.focus_editor, "the caret goes under the new heading");
        let editor = app.editor.as_ref().unwrap();
        assert!(
            editor
                .text
                .ends_with("- [ ] b\n\n## Acceptance criteria\n\n- [ ] ")
        );
        assert!(editor.is_dirty());
        assert!(
            !std::fs::read_to_string(&partial)
                .unwrap()
                .contains("Acceptance")
        );
        pass(&mut app);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn moving_an_issue_reparents_the_file_and_refreshes_the_board() {
        let dir = scratch_dir("move");
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        assert!(app.board.find(&file).is_some());

        app.move_issue(&file, &doing);

        assert!(!file.exists(), "source should be gone");
        let moved = doing.join("01-demo.md");
        assert!(moved.exists(), "file should live in the target column");
        assert!(app.error.is_none(), "a clean move reports no error");
        assert_eq!(
            app.board.find(&moved).map(|issue| issue.title.as_str()),
            Some("Demo"),
            "the board adopts the moved issue"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_issue_is_numbered_in_ready_and_unnumbered_when_taken() {
        let dir = scratch_dir("ready");
        let todo = dir.join("issues/todo");
        let ready = dir.join("issues/ready");
        let doing = dir.join("issues/doing");
        for d in [&todo, &ready, &doing] {
            std::fs::create_dir_all(d).unwrap();
        }
        let file = todo.join("make-colour-green.md");
        std::fs::write(&file, "# Green\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.selected = Some(file.clone());

        app.move_issue(&file, &ready);
        let queued = ready.join("001-make-colour-green.md");
        assert!(queued.exists(), "entering Ready adds the priority number");
        assert_eq!(app.selected.as_deref(), Some(queued.as_path()));

        app.move_issue(&queued, &doing);
        assert!(
            doing.join("make-colour-green.md").exists(),
            "leaving Ready drops it"
        );
        assert!(app.error.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn opening_another_project_rebuilds_the_board_and_remembers_it() {
        let alpha = scratch_dir("switch-alpha");
        let beta = scratch_dir("switch-beta");
        std::fs::create_dir_all(alpha.join("issues/todo")).unwrap();
        std::fs::write(alpha.join("issues/todo/a.md"), "# Alpha\n").unwrap();
        std::fs::create_dir_all(beta.join("issues/ready")).unwrap();
        std::fs::write(beta.join("issues/ready/b.md"), "# Beta\n").unwrap();

        let located = discovery::locate(&alpha).unwrap();
        let mut app = IboardApp::from_located(located);

        app.open_project(beta.clone());

        assert!(app.error.is_none());
        assert_eq!(
            recents::canonical(&app.board.root),
            recents::canonical(&beta.join("issues")),
            "the board now belongs to the target project"
        );
        assert!(matches!(app.screen, Screen::Board));
        assert_eq!(app.recents, vec![recents::canonical(&beta)]);
        assert_eq!(
            app.pending_title.as_deref(),
            beta.file_name().and_then(|name| name.to_str()),
            "the window takes the new project's name"
        );

        // A folder with no issues directory reports an error and changes nothing.
        let empty = scratch_dir("switch-empty");
        let before = app.board.root.clone();
        app.open_project(empty.clone());
        assert!(app.error.is_some());
        assert_eq!(app.board.root, before);

        for dir in [&alpha, &beta, &empty] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn moving_never_overwrites_an_existing_file() {
        let dir = scratch_dir("clobber");
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let existing = doing.join("01-demo.md");
        std::fs::write(&existing, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        app.move_issue(&file, &doing);

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
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        // An existing card in the target column, so the drop lands on it.
        let other = doing.join("01-other.md");
        std::fs::write(&other, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));

        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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

        let card_id = egui::Id::new(("issue_card", &file));
        let start = ctx
            .read_response(card_id)
            .expect("the card is a drag source")
            .rect
            .center();
        // Aim at the card already in the target column, to prove that a card
        // under the pointer does not swallow the column drop.
        let target_card_id = egui::Id::new(("issue_card", &other));
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

    /// Drives a real pointer drag of `file`'s card onto the DropZones dock
    /// zone `zone` (or, with `None`, cancels it with `esc` instead), and
    /// returns the app afterwards.
    fn drag_card_to_zone(app: &mut IboardApp, file: &Path, zone: Option<&str>) {
        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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

        pass(app, vec![]);
        let start = ctx
            .read_response(egui::Id::new(("issue_card", file)))
            .expect("the card is a drag source")
            .rect
            .center();
        pass(app, vec![moved(start)]);
        pass(app, vec![moved(start), button(start, true)]);
        let lifted = start + egui::vec2(30.0, 30.0);
        // Let the dock slide all the way in.
        for _ in 0..20 {
            pass(app, vec![moved(lifted)]);
        }
        assert!(
            egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx),
            "the card is being dragged"
        );
        let Some(zone) = zone else {
            pass(
                app,
                vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert!(
                !egui::DragAndDrop::has_any_payload(&ctx),
                "esc cancels the drag"
            );
            pass(app, vec![moved(lifted), button(lifted, false)]);
            return;
        };
        let target = ctx
            .read_response(egui::Id::new(zone))
            .expect("the dock shows while dragging")
            .rect
            .center();
        pass(app, vec![moved(target)]);
        pass(app, vec![moved(target)]);
        pass(app, vec![moved(target), button(target, false)]);
    }

    #[test]
    fn dropping_a_done_card_on_archive_archives_it() {
        let dir = scratch_dir("dock-archive");
        let done = dir.join("issues/done");
        std::fs::create_dir_all(&done).unwrap();
        let file = done.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        drag_card_to_zone(&mut app, &file, Some("drop_zone_archive"));

        assert!(!file.exists(), "the card left done/");
        assert!(dir.join("issues/.archive/01-demo.md").exists());
        assert_eq!(
            app.notice.as_ref().map(|(text, _)| text.as_str()),
            Some("Archived 01-demo.md")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A card outside Done only gets the Delete zone: Archive is a Done-only
    /// exit, so the dock does not show it at all.
    #[test]
    fn a_card_outside_done_only_gets_delete() {
        let dir = scratch_dir("dock-archive-off");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(dir.join("issues/done")).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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
            .read_response(egui::Id::new(("issue_card", &file)))
            .expect("the card is a drag source")
            .rect
            .center();
        pass(&mut app, vec![moved(start)]);
        pass(&mut app, vec![moved(start), button(start, true)]);
        let lifted = start + egui::vec2(30.0, 30.0);
        for _ in 0..20 {
            pass(&mut app, vec![moved(lifted)]);
        }
        assert!(
            ctx.read_response(egui::Id::new("drop_zone_delete"))
                .is_some(),
            "Delete is always offered"
        );
        assert!(
            ctx.read_response(egui::Id::new("drop_zone_archive"))
                .is_none(),
            "Archive is only shown for Done cards"
        );
        // Cancel the drag: the card stays where it was.
        pass(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        pass(&mut app, vec![moved(lifted), button(lifted, false)]);

        assert!(file.exists(), "the card goes back to its place");
        assert!(!dir.join("issues/.archive").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_a_card_on_delete_asks_first() {
        let dir = scratch_dir("dock-delete");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        drag_card_to_zone(&mut app, &file, Some("drop_zone_delete"));

        assert!(
            matches!(&app.confirm, Some(Confirm::Delete(path)) if *path == file),
            "the drop opens the confirmation"
        );
        assert!(file.exists(), "nothing is removed until confirmed");
        app.delete_issue(&file);
        assert!(!file.exists());
        assert_eq!(
            app.notice.as_ref().map(|(text, _)| text.as_str()),
            Some("Deleted 01-demo.md")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn esc_cancels_a_drag_and_leaves_the_card() {
        let dir = scratch_dir("dock-cancel");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        drag_card_to_zone(&mut app, &file, None);

        assert!(file.exists());
        assert!(app.confirm.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_archives_the_selected_done_card() {
        let dir = scratch_dir("archive-key");
        let done = dir.join("issues/done");
        std::fs::create_dir_all(&done).unwrap();
        let file = done.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.selected = Some(file.clone());

        let ctx = themed_ctx();
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            eframe::App::logic(&mut app, ui.ctx(), &mut eframe::Frame::_new_kittest());
        });

        assert!(dir.join("issues/.archive/01-demo.md").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `⌘F` opens the filter with its suggestions; `↓ ↩` picks one as a token,
    /// `⌫` removes it, and `esc` clears before it leaves the field.
    #[test]
    fn the_filter_picks_suggestions_from_the_keyboard() {
        let dir = scratch_dir("filter-keys");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::write(todo.join("login.md"), "# [Bug] Fix login\n\nA bug.\n").unwrap();
        std::fs::write(todo.join("export.md"), "# [Feature] Export\n\nA feature.\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        let ctx = themed_ctx();
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                eframe::App::logic(app, ui.ctx(), &mut eframe::Frame::_new_kittest());
                app.draw(ui);
            })
            .textures_delta
            .clear();
        };
        let key = |key: Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let focused = |ctx: &egui::Context| ctx.memory(|m| m.has_focus(filter_id()));

        pass(&mut app, vec![]);
        // What `⌘F` sets.
        app.focus_filter = true;
        pass(&mut app, vec![]);
        pass(&mut app, vec![]);
        assert!(focused(&ctx), "the filter takes focus");

        pass(&mut app, vec![key(Key::ArrowDown, egui::Modifiers::NONE)]);
        assert_eq!(app.chip_cursor, Some(0), "↓ reaches the first chip");
        pass(&mut app, vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert_eq!(app.filter.tokens, vec!["bug"]);
        assert!(focused(&ctx), "picking keeps focus in the field");
        assert_eq!(app.visible_grid().concat().len(), 1, "only the bug matches");

        pass(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
        assert_eq!(
            app.chip_cursor, None,
            "esc goes back from the chips to the text"
        );
        assert!(focused(&ctx), "leaving the chips keeps focus");
        pass(&mut app, vec![key(Key::Backspace, egui::Modifiers::NONE)]);
        assert!(
            app.filter.tokens.is_empty(),
            "⌫ in an empty field drops the token"
        );

        app.filter.pick("feature");
        pass(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
        assert!(app.filter.is_empty(), "the first esc clears");
        assert!(focused(&ctx));
        pass(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
        assert!(!focused(&ctx), "the second esc leaves the field");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `L` opens the LabelPicker on the selected card; `↩` writes the typed
    /// label and keeps the picker open, `↓ ↩` adds a board label, and `esc`
    /// closes without writing what is left in the field.
    #[test]
    fn the_label_picker_adds_labels_from_the_keyboard() {
        let dir = scratch_dir("label-picker");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let login = todo.join("login.md");
        std::fs::write(&login, "# [Bug] Fix login\n\nA bug.\n").unwrap();
        std::fs::write(todo.join("export.md"), "# [Feature] Export\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.selected = Some(login.clone());

        let ctx = themed_ctx();
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                eframe::App::logic(app, ui.ctx(), &mut eframe::Frame::_new_kittest());
                app.draw(ui);
            })
            .textures_delta
            .clear();
        };
        let key = |key: Key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let field = egui::Id::new("label_picker_field");

        pass(&mut app, vec![]);
        pass(&mut app, vec![key(Key::L)]);
        assert!(app.label_picker.is_some(), "L opens the picker");
        pass(&mut app, vec![]);
        assert!(ctx.memory(|m| m.has_focus(field)), "the field takes focus");

        pass(&mut app, vec![egui::Event::Text("Tech debt".into())]);
        pass(&mut app, vec![key(Key::Enter)]);
        assert_eq!(
            std::fs::read_to_string(&login).unwrap(),
            "# [Bug] [tech-debt] Fix login\n\nA bug.\n"
        );
        let picker = app.label_picker.as_ref().expect("the picker stays open");
        assert!(picker.text.is_empty(), "the field is cleared");
        pass(&mut app, vec![]);
        assert!(
            ctx.memory(|m| m.has_focus(field)),
            "focus stays in the field"
        );

        // The board's labels, most used first, minus the two it has: feature.
        pass(&mut app, vec![key(Key::ArrowDown)]);
        pass(&mut app, vec![key(Key::Enter)]);
        pass(&mut app, vec![]);
        assert_eq!(
            app.board.find(&login).unwrap().labels,
            ["bug", "tech-debt", "feature"]
        );

        pass(&mut app, vec![]);
        pass(&mut app, vec![egui::Event::Text("left over".into())]);
        pass(&mut app, vec![key(Key::Escape)]);
        assert!(app.label_picker.is_none(), "esc closes the picker");
        assert!(!std::fs::read_to_string(&login).unwrap().contains("left"));
        assert!(login.exists(), "the file name never changes");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A press and release without movement is still a click, not a drag.
    #[test]
    fn clicking_a_card_still_opens_it() {
        let dir = scratch_dir("click");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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
        let card_id = egui::Id::new(("issue_card", &file));
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
            "a plain click should open the issue"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Hover actions are not drag handles: over the card the cursor is the
    /// grab hand, but over an action button it is the pointer.
    #[test]
    fn hovering_a_card_action_shows_the_pointer_cursor() {
        let dir = scratch_dir("cursor");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                app.draw(ui);
            });
            output.textures_delta.clear();
            output.platform_output.cursor_icon
        };

        pass(&mut app, vec![]);
        let center = ctx
            .read_response(egui::Id::new(("issue_card", &file)))
            .unwrap()
            .rect
            .center();

        // The card body is a drag handle.
        pass(&mut app, vec![egui::Event::PointerMoved(center)]);
        assert_eq!(
            pass(&mut app, vec![egui::Event::PointerMoved(center)]),
            egui::CursorIcon::Grab,
            "the card invites a drag"
        );

        // Its Delete action is a click target, not a drag handle.
        let delete = ctx
            .read_response(egui::Id::new(("card_delete", &file)))
            .expect("actions show on hover")
            .rect
            .center();
        pass(&mut app, vec![egui::Event::PointerMoved(delete)]);
        assert_eq!(
            pass(&mut app, vec![egui::Event::PointerMoved(delete)]),
            egui::CursorIcon::PointingHand,
            "an action button is not a drag handle"
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
    fn renaming_an_issue_via_the_editor_moves_the_file() {
        let dir = scratch_dir("rename");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        app.start_editing(&file);
        app.editor.as_mut().unwrap().name = "01-renamed".to_string();
        app.flush_autosave(false);

        let renamed = todo.join("01-renamed.md");
        assert!(renamed.exists(), "the issue is renamed on disk");
        assert!(!file.exists(), "the old file name is gone");
        assert!(app.error.is_none(), "a clean rename reports no error");
        assert!(
            matches!(&app.screen, Screen::Detail(path) if path == &renamed),
            "the detail view follows the renamed file"
        );
        assert_eq!(app.editor.as_ref().unwrap().path, renamed);
        assert_eq!(
            app.board.find(&renamed).map(|issue| issue.title.as_str()),
            Some("Demo"),
            "the board adopts the renamed issue"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renaming_onto_an_existing_issue_is_refused() {
        let dir = scratch_dir("rename-clash");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let existing = todo.join("02-other.md");
        std::fs::write(&existing, "# Other\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

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
            let folder = dir.join("issues").join(column);
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
        let mut app = IboardApp::from_located(located);
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
                .read_response(egui::Id::new(("issue_card", card)))
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
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();
        std::fs::write(todo.join("01-alpha.md"), "# Alpha\n").unwrap();
        std::fs::write(todo.join("02-beta.md"), "# Beta\n").unwrap();
        std::fs::write(doing.join("01-gamma.md"), "# Gamma\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        app.move_selection(Key::ArrowDown);
        assert_eq!(app.selected, Some(todo.join("01-alpha.md")));
        app.move_selection(Key::ArrowDown);
        app.move_selection(Key::ArrowRight);
        assert_eq!(app.selected, Some(doing.join("01-gamma.md")));

        // Filtered out cards are skipped.
        app.filter.text = "beta".to_string();
        app.selected = None;
        app.move_selection(Key::ArrowDown);
        assert_eq!(app.selected, Some(todo.join("02-beta.md")));

        // Back from the reader, the card is still selected.
        app.open_issue(todo.join("02-beta.md"));
        app.back_to_board();
        assert_eq!(app.selected, Some(todo.join("02-beta.md")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An app editing `todo/01-demo.md` with an unsaved change.
    fn editing_with_unsaved_change(tag: &str, auto: bool) -> (PathBuf, PathBuf, IboardApp) {
        let dir = scratch_dir(tag);
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.auto = auto;
        app.open_issue(file.clone());
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
            slugify("  AI assist: refine an issue!  "),
            "ai-assist-refine-an-issue"
        );
        assert_eq!(slugify("Café au lait"), "caf-au-lait");
        assert_eq!(slugify("¿¡!?"), "issue");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= MAX_SLUG_LEN, "{long} is too long");
        assert!(!long.ends_with('-'));
    }

    #[test]
    fn planned_names_take_a_suffix_when_taken() {
        let dir = scratch_dir("planned");
        assert_eq!(
            planned_issue_name(&dir, "Fix it"),
            ("fix-it.md".to_string(), false)
        );
        assert_eq!(
            planned_issue_name(&dir, "[Bug] [UI] Fix it"),
            ("fix-it.md".to_string(), false),
            "labels stay out of the file name"
        );
        std::fs::write(dir.join("fix-it.md"), "").unwrap();
        assert_eq!(
            planned_issue_name(&dir, "Fix it"),
            ("fix-it-2.md".to_string(), true)
        );
        std::fs::write(dir.join("fix-it-2.md"), "").unwrap();
        assert_eq!(
            planned_issue_name(&dir, "Fix it"),
            ("fix-it-3.md".to_string(), true)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_composer_creates_issues_and_stays_open() {
        let dir = scratch_dir("compose");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

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
        assert!(app.board.find(&created).is_some(), "the board sees it");

        // An empty title never creates a file.
        app.submit_composer(false);
        assert_eq!(std::fs::read_dir(&todo).unwrap().count(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clicking_away_adds_the_issue_and_closes_the_composer() {
        let dir = scratch_dir("compose-away");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        app.open_composer();
        app.composer.as_mut().unwrap().title = "Export board".to_string();
        app.submit_composer(true);

        let created = todo.join("export-board.md");
        assert!(created.exists(), "the new issue is written to Todo");
        assert!(app.composer.is_none(), "the composer closes");
        assert!(matches!(app.screen, Screen::Board), "the board stays");
        assert_eq!(app.selected.as_deref(), Some(created.as_path()));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The composer's keys, as typed: `⇧↩` and `⌘↩` are new lines, `↩` and
    /// `⌘S` add, and a click on another card adds and closes it.
    #[test]
    fn composer_keys_and_clicking_away() {
        let dir = scratch_dir("compose-keys");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let other = todo.join("other.md");
        std::fs::write(&other, "# Other\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.open_composer();

        let ctx = themed_ctx();
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                eframe::App::logic(app, ui.ctx(), &mut eframe::Frame::_new_kittest());
                app.draw(ui);
            })
            .textures_delta
            .clear();
        };
        let key = |key: Key, modifiers: egui::Modifiers| {
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ]
        };
        let text = |text: &str| vec![egui::Event::Text(text.into())];
        let files = || std::fs::read_dir(&todo).unwrap().count();

        pass(&mut app, vec![]);
        pass(&mut app, vec![]);
        pass(&mut app, text("First line"));
        pass(&mut app, key(Key::Enter, egui::Modifiers::SHIFT));
        pass(&mut app, text("second"));
        pass(&mut app, key(Key::Enter, egui::Modifiers::COMMAND));
        pass(&mut app, text("third"));
        assert_eq!(
            app.composer.as_ref().unwrap().description,
            "First line\nsecond\nthird"
        );
        assert_eq!(files(), 1, "new lines never add");

        pass(&mut app, key(Key::Enter, egui::Modifiers::NONE));
        assert_eq!(
            std::fs::read_to_string(todo.join("first-line.md")).unwrap(),
            "# First line\n\nFirst line\nsecond\nthird\n"
        );
        assert!(app.composer.is_some(), "↩ keeps it open for the next one");

        pass(
            &mut app,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
        pass(&mut app, text("Saved with command S"));
        pass(&mut app, key(Key::S, egui::Modifiers::COMMAND));
        // Adding a ticket at the top renumbers Todo's files.
        assert!(todo.join("001-saved-with-command-s.md").exists(), "⌘S adds");
        assert!(app.composer.is_some());
        let other = todo.join("003-other.md");
        assert!(other.exists(), "the older card was renumbered too");

        pass(
            &mut app,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
        pass(&mut app, text("Clicked away"));
        let card = ctx
            .read_response(egui::Id::new(("issue_card", &other)))
            .unwrap()
            .rect
            .center();
        let button = |pressed| egui::Event::PointerButton {
            pos: card,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        pass(&mut app, vec![egui::Event::PointerMoved(card)]);
        pass(
            &mut app,
            vec![egui::Event::PointerMoved(card), button(true)],
        );
        pass(&mut app, vec![button(false)]);
        assert!(
            todo.join("001-clicked-away.md").exists(),
            "the click adds it"
        );
        assert!(app.composer.is_none(), "and closes the composer");
        let opened = todo.join("004-other.md");
        let got = match &app.screen {
            Screen::Detail(path) => Some(path.clone()),
            _ => None,
        };
        assert_eq!(got, Some(opened), "the clicked card still opens");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A composer whose engine answers through `sender`, as the real ones do.
    fn composer_with_engine(tag: &str) -> (PathBuf, PathBuf, IboardApp) {
        let dir = scratch_dir(tag);
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.open_composer();
        let composer = app.composer.as_mut().unwrap();
        composer.engine = Some(Engine::Agent);
        composer.description =
            "When I export the board I want a checklist.\nOne per column.".into();
        (dir, todo, app)
    }

    /// Stands in for the engine: the answer is already waiting.
    fn answer_with(app: &mut IboardApp, answer: Result<String, String>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(answer).unwrap();
        let composer = app.composer.as_mut().unwrap();
        let run = composer.naming.as_mut().expect("naming started");
        run.answer = receiver;
    }

    #[test]
    fn enter_names_the_issue_then_adds_it() {
        let (dir, todo, mut app) = composer_with_engine("compose-name");
        let ctx = egui::Context::default();

        app.submit_composer(false);
        assert!(app.composer.as_ref().unwrap().stage == TitleStage::Naming);
        assert_eq!(
            std::fs::read_dir(&todo).unwrap().count(),
            0,
            "nothing written yet"
        );

        answer_with(&mut app, Ok("Export the board as checklists".into()));
        app.poll_naming(&ctx);
        let composer = app.composer.as_ref().unwrap();
        assert!(
            composer.stage == TitleStage::Inferred,
            "it stops on the suggestion"
        );
        assert_eq!(composer.title, "Export the board as checklists");
        assert!(composer.select_title, "selected, so typing replaces it");

        app.submit_composer(false);
        let created = todo.join("export-the-board-as-checklists.md");
        assert_eq!(
            std::fs::read_to_string(&created).unwrap(),
            "# Export the board as checklists\n\nWhen I export the board I want a checklist.\nOne per column.\n"
        );
        let composer = app.composer.as_ref().expect("the composer stays open");
        assert!(composer.title.is_empty() && composer.description.is_empty());
        assert!(composer.stage == TitleStage::Idle);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_name_falls_back_to_the_first_line() {
        let (dir, todo, mut app) = composer_with_engine("compose-fail");
        let ctx = egui::Context::default();

        app.submit_composer(false);
        answer_with(&mut app, Err("model unavailable".into()));
        app.poll_naming(&ctx);
        let composer = app.composer.as_ref().unwrap();
        assert!(composer.stage == TitleStage::Failed);
        assert!(
            composer.focus == Some(ComposerField::Title),
            "cursor in the title"
        );
        assert!(!composer.description.is_empty(), "nothing lost");

        app.submit_composer(false);
        assert!(
            todo.join("when-i-export-the-board-i-want-a-checklist.md")
                .exists(),
            "↩ uses the first line"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clicking_away_while_naming_writes_once_named() {
        let (dir, todo, mut app) = composer_with_engine("compose-name-away");
        let ctx = egui::Context::default();

        app.submit_composer(false);
        app.submit_composer(true);
        answer_with(&mut app, Ok("Export checklists".into()));
        app.poll_naming(&ctx);
        let created = todo.join("export-checklists.md");
        assert!(
            created.exists(),
            "a click away never stops on the suggestion"
        );
        assert!(app.composer.is_none(), "and closes the composer");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn esc_while_naming_lets_you_type_the_title() {
        let (dir, todo, mut app) = composer_with_engine("compose-skip");
        app.submit_composer(false);
        app.composer_escape();
        let composer = app
            .composer
            .as_ref()
            .expect("esc does not close while naming");
        assert!(composer.stage == TitleStage::Skipped);
        assert!(composer.naming.is_none());

        app.composer.as_mut().unwrap().title = "Board export".into();
        app.submit_composer(false);
        assert!(todo.join("board-export.md").exists());
        app.composer_escape();
        assert!(app.composer.is_none(), "esc closes it otherwise");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_an_engine_the_first_line_names_the_issue() {
        let dir = scratch_dir("compose-manual");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.open_composer();
        assert!(
            app.composer.as_ref().unwrap().engine.is_none(),
            "Off in tests"
        );
        app.composer.as_mut().unwrap().description =
            "  Fix the login redirect\nIt loses the deep link.".into();
        app.submit_composer(false);
        assert_eq!(
            std::fs::read_to_string(todo.join("fix-the-login-redirect.md")).unwrap(),
            "# Fix the login redirect\n\n  Fix the login redirect\nIt loses the deep link.\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn with_asking_off_delete_is_immediate_and_command_z_restores() {
        let dir = scratch_dir("delete-no-ask");
        let done = dir.join("issues/done");
        std::fs::create_dir_all(&done).unwrap();
        let file = done.join("sidebar.md");
        std::fs::write(&file, "# Sidebar\n\nCollapse it.\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        assert!(app.confirm_delete, "asking is on by default");

        app.request_delete(file.clone());
        assert!(app.confirm.is_some(), "on: the dialog opens");
        assert!(file.exists());
        app.confirm = None;

        app.confirm_delete = false;
        app.request_delete(file.clone());
        assert!(app.confirm.is_none(), "off: no dialog");
        assert!(!file.exists(), "off: deleted at once");
        assert!(app.can_undo_delete());

        app.undo_delete();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# Sidebar\n\nCollapse it.\n",
            "⌘Z puts it back as it was"
        );
        assert!(app.board.find(&file).is_some(), "and on the board");
        assert_eq!(app.selected.as_deref(), Some(file.as_path()));
        assert!(!app.can_undo_delete(), "once");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Bug: opening another issue dropped the composer and what was typed.
    #[test]
    fn opening_an_issue_keeps_an_unsaved_composer() {
        let dir = scratch_dir("compose-keep");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let other = todo.join("other.md");
        std::fs::write(&other, "# Other\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        app.open_composer();
        app.composer.as_mut().unwrap().description = "Half a thought".into();
        app.open_issue(other.clone());
        app.back_to_board();
        assert_eq!(
            app.composer.as_ref().map(|c| c.description.as_str()),
            Some("Half a thought"),
            "the draft waits on the board"
        );

        app.composer.as_mut().unwrap().description.clear();
        app.open_issue(other);
        assert!(app.composer.is_none(), "an empty composer still closes");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reviewing and "Not written yet" are for issues still in Todo.
    #[test]
    fn only_todo_issues_are_reviewed() {
        let dir = scratch_dir("review-todo-only");
        let ready = dir.join("issues/ready");
        std::fs::create_dir_all(&ready).unwrap();
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let file = ready.join("001-queued.md");
        std::fs::write(&file, "# Queued\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.agent.configured = true;
        assert!(!app.is_todo_issue(&file));
        app.start_review(&file, false);
        assert!(app.review.is_none(), "a Ready issue is not reviewed");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Bug: `⌘S` in the editor did not save.
    #[test]
    fn command_s_saves_the_editor() {
        let dir = scratch_dir("command-s");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.auto = false;
        app.start_editing(&file);
        app.editor.as_mut().unwrap().text = "# Demo\n\nMore.\n".into();

        let ctx = themed_ctx();
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>, _modifiers: egui::Modifiers| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                eframe::App::logic(app, ui.ctx(), &mut eframe::Frame::_new_kittest());
                app.draw(ui);
            })
            .textures_delta
            .clear();
        };
        pass(&mut app, vec![], egui::Modifiers::NONE);
        pass(&mut app, vec![], egui::Modifiers::NONE);
        let s = egui::Event::Key {
            key: Key::S,
            physical_key: Some(Key::S),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        pass(
            &mut app,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::COMMAND), s],
            egui::Modifiers::COMMAND,
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Demo\n\nMore.\n");
        assert_eq!(
            app.detail_mode,
            DetailMode::Preview,
            "like the Save ⌘S button, it shows the saved issue"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_freedesktop_trash_keeps_a_restorable_copy() {
        let dir = scratch_dir("xdg-trash");
        let file = dir.join("my issue.md");
        std::fs::write(&file, "# Mine\n").unwrap();
        let trash = dir.join("Trash");
        let first = trash_into(&file, &trash).unwrap();
        assert!(!file.exists());
        assert_eq!(first, trash.join("files/my issue.md"));
        let info = std::fs::read_to_string(trash.join("info/my issue.md.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(
            info.contains("my%20issue.md"),
            "the path is URL-encoded: {info}"
        );
        assert!(info.contains("DeletionDate="));

        std::fs::write(&file, "# Again\n").unwrap();
        let second = trash_into(&file, &trash).unwrap();
        assert_eq!(
            second,
            trash.join("files/my issue.2.md"),
            "never overwrites"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A dismissed review stays behind the card's verdict pill.
    #[test]
    fn a_dismissed_review_can_be_reopened_from_its_card() {
        let (dir, file, mut app) = open_issue_with_review("past-review");
        app.cancel_review();
        assert!(app.review.is_none());
        let past = app.past_reviews.get(&file).expect("kept for the session");
        let p = &theme::LIGHT;
        assert_eq!(past.pill(p).0, "Block", "the worst verdict names the pill");

        app.back_to_board();
        app.show_past_review(file.clone());
        assert!(matches!(&app.screen, Screen::Detail(path) if *path == file));
        let review = app.review.as_ref().expect("the panel is back");
        assert!(matches!(review.status, ReviewStatus::Done { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn archive_all_clears_done() {
        let dir = scratch_dir("archive-all");
        let done = dir.join("issues/done");
        std::fs::create_dir_all(&done).unwrap();
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let a = done.join("a.md");
        let b = done.join("b.md");
        std::fs::write(&a, "# A\n").unwrap();
        std::fs::write(&b, "# B\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        app.archive_all(&[a.clone(), b.clone()]);
        assert!(!a.exists() && !b.exists());
        assert!(dir.join("issues/.archive/a.md").exists());
        assert_eq!(
            app.notice.as_ref().map(|(text, _)| text.as_str()),
            Some("Archived 2 issues")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_the_todo_column_offers_new_issues() {
        let dir = scratch_dir("new-columns");
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&doing).unwrap();

        let located = discovery::locate(&dir).unwrap();
        let board = discovery::build(&located).unwrap();

        let find = |name: &str| {
            board
                .columns
                .iter()
                .find(|column| column.name == name)
                .unwrap()
        };
        assert!(find("Todo").accepts_new_issues());
        assert!(!find("Doing").accepts_new_issues());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn long_issue_names_are_capped_and_keep_their_extension() {
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
    fn deleting_an_issue_asks_first_then_removes_the_file() {
        let dir = scratch_dir("delete");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.selected = Some(file.clone());
        app.open_issue(file.clone());

        app.request_delete(file.clone());
        assert!(app.confirm.is_some(), "the delete waits for confirmation");
        assert!(file.exists(), "nothing is removed until confirmed");

        app.delete_issue(&file);
        assert!(!file.exists(), "the file is gone");
        assert!(app.confirm.is_none());
        assert!(app.selected.is_none());
        assert!(
            matches!(app.screen, Screen::Board),
            "deleting the open issue returns to the board"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn archiving_a_done_issue_hides_it_in_the_dot_archive() {
        let dir = scratch_dir("archive");
        let done = dir.join("issues/done");
        std::fs::create_dir_all(&done).unwrap();
        let file = done.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        assert!(app.board.find(&file).is_some());

        app.archive_issue(&file);
        let archived = dir.join("issues/.archive/01-demo.md");
        assert!(archived.exists(), "the issue is kept in .archive");
        assert!(!file.exists(), "and is gone from Done");
        assert!(app.board.find(&file).is_none(), "the board drops it");
        assert!(
            app.error.is_none(),
            "a clean archive reports no error: {:?}",
            app.error
        );

        // A second issue with the same name does not overwrite the first.
        std::fs::write(&file, "# Again\n").unwrap();
        app.refresh();
        app.archive_issue(&file);
        assert!(dir.join("issues/.archive/01-demo-2.md").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dropping_a_todo_card_at_a_position_renumbers_the_column() {
        let dir = scratch_dir("reorder-todo");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let hello = todo.join("001-hello.md");
        let byebye = todo.join("002-byebye.md");
        std::fs::write(&hello, "# Hello\n").unwrap();
        std::fs::write(&byebye, "# Bye bye\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        // Drop Bye bye in front of Hello.
        app.place_issue(&byebye, &todo, Some(hello.clone()));

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

        let names: Vec<String> = app.board.columns[0]
            .issues
            .iter()
            .map(|issue| issue.file_name())
            .collect();
        assert_eq!(names, ["001-byebye.md", "002-hello.md"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn moving_a_todo_card_into_ready_at_a_position_renumbers_it() {
        let dir = scratch_dir("reorder-ready");
        let todo = dir.join("issues/todo");
        let ready = dir.join("issues/ready");
        std::fs::create_dir_all(&todo).unwrap();
        std::fs::create_dir_all(&ready).unwrap();
        let moved = todo.join("later.md");
        let first = ready.join("001-first.md");
        std::fs::write(&moved, "# Later\n").unwrap();
        std::fs::write(&first, "# First\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        app.place_issue(&moved, &ready, Some(first.clone()));

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
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let first = todo.join("001-first.md");
        let second = todo.join("002-second.md");
        std::fs::write(&first, "# First\n").unwrap();
        std::fs::write(&second, "# Second\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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
            .read_response(egui::Id::new(("issue_card", &second)))
            .expect("the card is a drag source")
            .rect
            .center();
        // Just above the first card's middle: insert before it.
        let target = ctx
            .read_response(egui::Id::new(("issue_card", &first)))
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
    /// issue underneath it.
    #[test]
    fn the_card_delete_icon_asks_to_delete_without_opening() {
        let dir = scratch_dir("card-delete");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);

        let ctx = themed_ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let pass = |app: &mut IboardApp, events: Vec<egui::Event>| {
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
            "and does not open the issue"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An app reading `todo/01-demo.md`, with a finished review of it.
    fn open_issue_with_review(tag: &str) -> (PathBuf, PathBuf, IboardApp) {
        let dir = scratch_dir(tag);
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.open_issue(file.clone());
        let raw = "RELEVANCE: Pass\nStill needed.\nSIZE: Challenge\nSplit it.\n\
                   CLARITY: Block\nWhich tool runs it?\nPROPOSAL:\n# Demo\n\nBetter.";
        app.review = Some(ReviewState {
            path: file.clone(),
            draft: false,
            agent: "opencode · vendor/model".to_string(),
            status: ReviewStatus::Done {
                raw: raw.to_string(),
                report: agent::parse(raw),
            },
            follow_up: String::new(),
        });
        (dir, file, app)
    }

    #[test]
    fn applying_a_review_opens_the_editor_with_an_unsaved_draft() {
        let (dir, file, mut app) = open_issue_with_review("apply");

        app.apply_review_proposal();

        assert_eq!(app.detail_mode, DetailMode::Edit);
        assert_eq!(app.editor.as_ref().unwrap().path, file);
        assert_eq!(app.editor.as_ref().unwrap().text, "# Demo\n\nBetter.");
        assert!(
            app.editor.as_ref().unwrap().is_dirty(),
            "the draft is unsaved"
        );
        assert!(app.review.is_none(), "the panel closes once applied");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# Demo\n\nContext.\n",
            "the agent never writes the file"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reviewing_without_a_configured_agent_opens_the_configuration() {
        let dir = scratch_dir("review-config");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.open_issue(file);

        app.start_review(&todo.join("01-demo.md"), false);

        assert!(app.settings.is_some(), "the settings dialog opens instead");
        assert!(app.review.is_none(), "no review was started");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A configured tool (here `echo`) is spawned, polled and adopted, so the
    /// running → answered hand-off is exercised for real.
    #[test]
    fn polling_a_running_review_adopts_the_answer() {
        let dir = scratch_dir("review-poll");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\nContext.\n").unwrap();

        let located = discovery::locate(&dir).unwrap();
        let mut app = IboardApp::from_located(located);
        app.agent = AgentConfig {
            tool: agent::Tool::Custom,
            command: "cat {prompt}".to_string(),
            configured: true,
            ..AgentConfig::default()
        };

        app.start_review(&file, false);
        assert!(app.review_is_running(&file));

        for _ in 0..200 {
            app.poll_review();
            if !app.review_is_running(&file) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        match &app.review.as_ref().unwrap().status {
            ReviewStatus::Done { raw, report } => {
                assert!(raw.contains("You review software issues"), "got: {raw:?}");
                assert!(report.is_none(), "prose that is not a report stays raw");
            }
            _ => panic!("the review never finished"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `⌘↩` in the settings dialog saves; a custom command without
    /// `{prompt}` keeps it open, and `esc` there never cancels the review.
    #[test]
    fn the_settings_dialog_saves_with_command_enter() {
        let (dir, _file, mut app) = open_issue_with_review("settings-save");
        let ctx = themed_ctx();
        let frame = |app: &mut IboardApp, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
        };
        let key = |key: Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };

        app.open_config();
        app.settings.as_mut().unwrap().form = AgentConfig {
            tool: agent::Tool::Custom,
            command: "llm".to_string(),
            ..AgentConfig::default()
        };
        frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
        assert!(app.settings.is_some(), "an invalid command keeps it open");
        assert!(!app.agent.configured());

        app.settings.as_mut().unwrap().form.command = "llm < {prompt}".to_string();
        frame(&mut app, vec![key(Key::Enter, egui::Modifiers::COMMAND)]);
        assert!(app.settings.is_none(), "saved and closed");
        assert!(app.agent.configured());
        assert_eq!(app.agent.label(), "custom · default");

        app.open_config();
        frame(&mut app, vec![key(Key::Escape, egui::Modifiers::NONE)]);
        assert!(app.settings.is_none(), "esc closes the dialog");
        assert!(app.review.is_some(), "and leaves the review alone");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The panel and the settings window go through a real egui pass, so a
    /// panic or a layout mistake surfaces without a display.
    #[test]
    fn the_review_panel_and_config_window_render() {
        let (dir, _file, mut app) = open_issue_with_review("review-render");
        let ctx = themed_ctx();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };

        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| app.draw(ui))
                .textures_delta
                .clear();
        }

        app.open_config();
        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| app.draw(ui))
                .textures_delta
                .clear();
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A project with a Todo column of `(title, label)` issues, and the app.
    fn focus_app(tag: &str, cards: &[(&str, Option<&str>)]) -> (PathBuf, PathBuf, IboardApp) {
        let dir = scratch_dir(tag);
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        for (index, (title, label)) in cards.iter().enumerate() {
            let body = match label {
                Some(label) => format!("# [{label}] {title}\n\n## Context\n\nWhy.\n"),
                None => format!("# {title}\n\n## Context\n\nWhy.\n\n- [ ] one\n- [x] two\n"),
            };
            std::fs::write(todo.join(format!("card-{index}.md")), body).unwrap();
        }
        let located = discovery::locate(&dir).unwrap();
        (dir, todo, IboardApp::from_located(located))
    }

    /// Renders `app` twice and clears the texture deltas, as the other tests do.
    fn render(app: &mut IboardApp, ctx: &egui::Context, width: f32) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.0),
            )),
            ..Default::default()
        };
        for _ in 0..2 {
            ctx.run_ui(input.clone(), |ui| app.draw(ui))
                .textures_delta
                .clear();
        }
    }

    #[test]
    fn focused_columns_open_with_their_own_default_sort() {
        assert_eq!(default_sort(Path::new("/x/todo")), Sort::Modified);
        assert_eq!(default_sort(Path::new("/x/ready")), Sort::Board);
        assert_eq!(default_sort(Path::new("/x/doing")), Sort::Modified);
        assert_eq!(default_sort(Path::new("/x/done")), Sort::Finished);
    }

    /// A focused Todo column lays the wide cards out four across at 1280pt,
    /// with the fifth wrapping to a new row inside the gutters.
    #[test]
    fn column_focus_lays_wide_cards_in_a_grid() {
        let (dir, todo, mut app) = focus_app(
            "focus-grid",
            &[
                ("One", None),
                ("Two", None),
                ("Three", None),
                ("Four", None),
                ("Five", None),
            ],
        );
        // Pin the file order so the grid is deterministic.
        app.column_sorts.insert(todo.clone(), Sort::Board);
        app.focus_column(todo.clone());
        assert_eq!(app.focused_column.as_deref(), Some(todo.as_path()));

        let ctx = themed_ctx();
        render(&mut app, &ctx, 1280.0);

        let paths: Vec<PathBuf> = app
            .board
            .columns
            .iter()
            .find(|column| column.dir == todo)
            .unwrap()
            .issues
            .iter()
            .map(|issue| issue.path.clone())
            .collect();
        let rects: Vec<Rect> = paths
            .iter()
            .map(|path| {
                ctx.read_response(egui::Id::new(("issue_card", path)))
                    .expect("every card is drawn")
                    .rect
            })
            .collect();

        assert!(
            rects[0].width() >= 295.0,
            "wide cards: {:?}",
            rects[0].width()
        );
        assert!(
            (rects[0].center().y - rects[3].center().y).abs() < 1.0,
            "four cards share the first row: {rects:?}"
        );
        assert!(
            rects[4].center().y > rects[0].center().y + 10.0,
            "the fifth wraps to a new row"
        );
        assert!(rects[1].min.x > rects[0].min.x, "cards run left to right");
        for rect in &rects {
            assert!(
                rect.min.x >= 16.0 - 0.5 && rect.max.x <= 1280.0 - 16.0 + 0.5,
                "{} at {rect:?} is outside the gutters",
                rect.center()
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn focus_sorts_order_the_cards() {
        let (dir, todo, app) = focus_app(
            "focus-sort",
            &[("Zebra", None), ("Apple", None), ("Mango", None)],
        );
        let column = app
            .board
            .columns
            .iter()
            .find(|column| column.dir == todo)
            .unwrap();
        let titles = |sort| {
            focus_issues(column, &Filter::default(), sort)
                .into_iter()
                .map(|issue| issue.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(Sort::Board), ["Zebra", "Apple", "Mango"]);
        assert_eq!(titles(Sort::Title), ["Apple", "Mango", "Zebra"]);
        // Each issue has two checklist items, one open; most-to-do keeps them.
        assert_eq!(titles(Sort::MostToDo).len(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grouping_orders_labels_by_count_and_unlabelled_last() {
        let (dir, todo, app) = focus_app(
            "focus-group",
            &[
                ("A", Some("bug")),
                ("B", Some("bug")),
                ("C", Some("feature")),
                ("D", None),
            ],
        );
        let column = app
            .board
            .columns
            .iter()
            .find(|column| column.dir == todo)
            .unwrap();
        let groups = group_issues(focus_issues(column, &Filter::default(), Sort::Board));
        let names: Vec<Option<&str>> = groups.iter().map(|(label, _)| label.as_deref()).collect();
        assert_eq!(names, [Some("bug"), Some("feature"), None]);
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[2].1.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn facts_check_written_sections_and_mark_missing() {
        let sections: Vec<String> = crate::sheet::DEFAULT_SECTIONS
            .iter()
            .map(|section| section.to_string())
            .collect();
        let dir = scratch_dir("facts");
        let file = dir.join("x.md");
        std::fs::write(
            &file,
            "# T\n\n## Context\n\nWhy.\n\n## Acceptance criteria\n\n- [x] a\n- [ ] b\n",
        )
        .unwrap();
        let issue = Issue::load(&file).unwrap();
        let row = facts(&issue, &sections);
        assert!(row.iter().any(|(text, ok)| text == "Context" && *ok));
        assert!(row.iter().any(|(text, ok)| text == "Criteria 1/2" && *ok));
        assert!(
            row.iter()
                .any(|(text, ok)| text == "Out of scope —" && !*ok)
        );

        std::fs::write(&file, "# Bare\n").unwrap();
        let issue = Issue::load(&file).unwrap();
        assert_eq!(
            facts(&issue, &sections),
            vec![("No sections yet".to_string(), false)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Opening an issue from a focused column and going back returns to that
    /// column, not the whole board.
    #[test]
    fn opening_an_issue_from_focus_returns_to_the_focused_column() {
        let (dir, todo, mut app) = focus_app("focus-open", &[("One", None), ("Two", None)]);
        app.focus_column(todo.clone());
        let file = app
            .board
            .columns
            .iter()
            .find(|column| column.dir == todo)
            .unwrap()
            .issues[0]
            .path
            .clone();
        app.open_issue(file.clone());
        assert!(matches!(app.screen, Screen::Detail(_)));
        app.back_to_board();
        assert!(matches!(app.screen, Screen::Board));
        assert_eq!(
            app.focused_column.as_deref(),
            Some(todo.as_path()),
            "the focused column is kept"
        );
        assert_eq!(app.selected.as_deref(), Some(file.as_path()));

        // The active column, and `esc`, leave ColumnFocus.
        app.unfocus_column();
        assert!(app.focused_column.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A focused column still accepts a card dropped on another tab, moving the
    /// file between folders without leaving ColumnFocus.
    #[test]
    fn a_card_can_be_moved_to_another_column_from_the_tabs() {
        let dir = scratch_dir("focus-tab-drop");
        for name in ["todo", "doing"] {
            std::fs::create_dir_all(dir.join("issues").join(name)).unwrap();
        }
        let file = dir.join("issues/todo/card.md");
        std::fs::write(&file, "# Card\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        let todo = dir.join("issues/todo");
        let doing = dir.join("issues/doing");
        app.focus_column(todo.clone());

        // Simulate the tab drop the focus bar reports.
        app.move_issue(&file, &doing);
        assert!(doing.join("card.md").exists(), "moved into doing/");
        assert!(!file.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tiny `design/` so the panel tests need no display and no repo data.
    fn write_min_design(root: &Path) {
        let design = root.join("design");
        std::fs::create_dir_all(design.join("components/Button")).unwrap();
        std::fs::write(
            design.join("design-system.json"),
            r#"{ "v": 1, "title": "Fixture", "namespace": "fixture" }"#,
        )
        .unwrap();
        std::fs::write(
            design.join("tokens.json"),
            r##"{
                "name": "Fixture", "version": 1,
                "color": {
                    "themes": [ {"id":"light","name":"Light"}, {"id":"dark","name":"Dark"} ],
                    "tokens": [ {"name":"bg","value":{"light":"#ffffff","dark":"#000000"},"usage":"ground"} ]
                },
                "type": { "families": {"sans":"-apple-system"}, "groups": [] },
                "spacing": {"tokens":[]}, "radius": {"tokens":[]},
                "size": {"tokens":[]}, "shadow": {"tokens":[]}
            }"##,
        )
        .unwrap();
        std::fs::write(
            design.join("components/Button/README.md"),
            "# Button\n\nA button.\n",
        )
        .unwrap();
        std::fs::write(
            design.join("components/Button/preview.html"),
            "<!doctype html><html><head></head><body><button>Go</button></body></html>",
        )
        .unwrap();
    }

    fn panel_ctx() -> (egui::Context, egui::RawInput) {
        (
            themed_ctx(),
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                ..Default::default()
            },
        )
    }

    #[test]
    fn design_panel_loads_and_creates_a_component() {
        let dir = scratch_dir("design-panel");
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        write_min_design(&dir);
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        app.open_design();
        assert!(
            app.design_system.is_some(),
            "loaded: {:?}",
            app.design_error
        );
        assert!(app.design.is_some());

        let (ctx, raw) = panel_ctx();
        let mut output = ctx.run_ui(raw.clone(), |ui| app.draw(ui));
        output.textures_delta.clear();

        // The same path the Create button drives.
        let mut panel = app.design.take().expect("panel open");
        let keep = app.handle_design_action(
            &mut panel,
            design_ui::Action::Create {
                name: "Toast".to_string(),
                group: "Controls".to_string(),
                intent: "Transient confirmation.".to_string(),
                draft: false,
            },
        );
        assert!(keep);
        assert!(dir.join("design/components/Toast/README.md").is_file());
        assert!(
            app.design_system
                .as_ref()
                .unwrap()
                .components
                .iter()
                .any(|c| c.name == "Toast")
        );

        app.design = Some(panel);
        let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
        output.textures_delta.clear();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn design_panel_reports_a_missing_system() {
        let dir = scratch_dir("design-missing");
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        app.open_design();
        assert!(app.design_system.is_none());
        assert!(app.design.is_some(), "the panel opens even with no system");

        let (ctx, raw) = panel_ctx();
        let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
        output.textures_delta.clear();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preview_render_helper_composes_and_renders() {
        if !iboard::design::render::available() {
            return;
        }
        let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
        let system = DesignSystem::load(&design).expect("repo design");
        let receiver = design::render::component_preview(&system, "Button").expect("spawn render");
        match receiver.recv_timeout(Duration::from_secs(60)) {
            Ok(Ok(path)) => assert!(path.is_file(), "PNG written"),
            Ok(Err(err)) => panic!("render failed: {err}"),
            Err(err) => panic!("render timed out: {err}"),
        }
    }

    #[test]
    fn design_panel_drafts_a_component_with_the_agent() {
        let dir = scratch_dir("design-draft");
        std::fs::create_dir_all(dir.join("issues/todo")).unwrap();
        write_min_design(&dir);
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.open_design();

        // A fake agent: prints a fenced HTML fragment, ignores the prompt.
        app.agent = AgentConfig {
            tool: agent::Tool::Custom,
            model: String::new(),
            command: "printf '```html\\n<button class=\"ib-btn\">Hi</button>\\n```' # {prompt}"
                .to_string(),
            configured: true,
        };

        let mut panel = app.design.take().expect("panel open");
        let keep = app.handle_design_action(
            &mut panel,
            design_ui::Action::Create {
                name: "Toast".to_string(),
                group: "Controls".to_string(),
                intent: "Transient confirmation.".to_string(),
                draft: true,
            },
        );
        assert!(keep);
        app.design = Some(panel);
        assert!(app.design_draft.is_some(), "a draft was started");

        let (ctx, _raw) = panel_ctx();
        let preview = dir.join("design/components/Toast/preview.html");
        for _ in 0..300 {
            app.poll_design_draft(&ctx);
            if std::fs::read_to_string(&preview)
                .unwrap_or_default()
                .contains("ib-btn")
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let written = std::fs::read_to_string(&preview).unwrap();
        assert!(
            written.contains("ib-btn"),
            "drafted fragment written: {written}"
        );
        assert!(!written.contains("```"), "code fences stripped: {written}");
        assert!(app.design_draft.is_none(), "the draft finished");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enrichment_artifact_path_is_per_issue() {
        let path = Path::new("/p/issues/todo/01-demo.md");
        assert_eq!(
            enrichment_dir(path),
            Path::new("/p/issues/todo/assets/01-demo")
        );
    }

    #[test]
    fn enrichment_drafts_an_artifact_for_an_issue() {
        let dir = scratch_dir("enrich");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\nMake the header taller.\n").unwrap();

        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        // A fake agent: prints a whole HTML page, ignores the prompt.
        app.agent = AgentConfig {
            tool: agent::Tool::Custom,
            model: String::new(),
            command: "printf '<!doctype html><html><body><h1>Demo</h1></body></html>' # {prompt}"
                .to_string(),
            configured: true,
        };

        app.start_enrichment(&file);
        assert!(app.enrichment.is_some(), "an enrichment started");

        let (ctx, raw) = panel_ctx();
        let artifact = todo.join("assets/01-demo/ui.html");
        for _ in 0..300 {
            app.poll_enrichment(&ctx);
            if artifact.is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let html = std::fs::read_to_string(&artifact).expect("artifact written");
        assert!(html.contains("<h1>Demo</h1>"), "mockup saved: {html}");

        // The window renders without a display, loader and all.
        let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
        output.textures_delta.clear();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enrichment_without_an_agent_opens_the_settings_instead() {
        let dir = scratch_dir("enrich-noagent");
        let todo = dir.join("issues/todo");
        std::fs::create_dir_all(&todo).unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n").unwrap();
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());

        app.start_enrichment(&file);
        assert!(app.enrichment.is_none(), "nothing started");
        assert!(app.settings.is_some(), "the settings dialog opened");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_links_resolve_against_the_issue_folder() {
        let issue = Path::new("/p/issues/todo/01-demo.md");
        assert_eq!(image_base_uri(issue), "file:///p/issues/todo/");
    }

    #[test]
    fn a_ticket_can_show_an_image_from_its_asset_folder() {
        let dir = scratch_dir("image");
        let todo = dir.join("issues/todo");
        let assets = todo.join("assets/01-demo");
        std::fs::create_dir_all(&assets).unwrap();
        let png = assets.join("ui.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255]))
            .save(&png)
            .unwrap();
        let file = todo.join("01-demo.md");
        std::fs::write(&file, "# Demo\n\n![mockup](assets/01-demo/ui.png)\n").unwrap();

        // The reader renders the markdown's image link without a display.
        let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
        app.open_issue(file.clone());
        let (ctx, raw) = panel_ctx();
        let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
        output.textures_delta.clear();

        // And the loader resolves and decodes it.
        let uri = format!("file://{}", png.display());
        let mut decoded = false;
        for _ in 0..100 {
            match ctx.try_load_image(&uri, egui::load::SizeHint::default()) {
                Ok(egui::load::ImagePoll::Ready { .. }) => {
                    decoded = true;
                    break;
                }
                Ok(egui::load::ImagePoll::Pending { .. }) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) => panic!("decode failed: {err:?}"),
            }
        }
        assert!(
            decoded,
            "the ticket's image decodes through the file loader"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

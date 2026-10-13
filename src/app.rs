//! The egui application: a kanban board with an inline quick-add composer, a
//! rendered markdown reader, and an editor with debounced autosave. Its look
//! follows the design system in `design/` (see `theme.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{
    Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::agent::{self, AgentConfig, Report, ReviewMode};
use crate::commits;
use crate::discovery::{self, Board, Column, Located};
use crate::files::{self, atomic_write, file_name, normalize_file_name, unique_name};
use crate::filter::{self, Filter};
use crate::format::{card_date, display_path, relative_time};
use crate::general::{self, Appearance};
use crate::issue::Issue;
use crate::job::{self, Job};
use crate::label_picker::{self, LabelPicker};
use crate::labels::{self, Labels};
use crate::order::{Sort, default_sort, focus_issues, group_issues};
use crate::process;
use crate::reviews;
use crate::settings::{self, UserConfig};
use crate::settings_ui::{self, Catalog};
use crate::theme::{self, Icon, Palette};
use crate::titles::{self, Engine};
use crate::trash::{self, Trash};
use crate::widgets::{self, ButtonKind};
use crate::{design_ui, dialog, recents, sheet};
use iboard::design::{self, DesignSystem};
use iboard::prompts;

mod board;
mod card;
mod comments;
mod compose;
mod design_panel;
mod diff_view;
mod dock;
mod editor;
mod enrich;
mod filter_bar;
mod focus;
mod header;
mod intent;
mod keys;
mod labelling;
mod moves;
mod nav;
mod reader;
mod review;
mod split;
mod status;
mod windows;

use board::*;
use card::*;
use compose::*;
use filter_bar::*;
use focus::ColumnFocus;
use intent::Intent;
use labelling::*;
use reader::*;

/// How often Auto reads the board again.
const RESCAN_EVERY: Duration = Duration::from_secs(2);

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
    Deep,
    Settings,
}

/// A choice made in the header's project menu.
enum ProjectChoice {
    /// Switch to a remembered project.
    Recent(PathBuf),
    /// Ask the OS for a folder.
    Browse,
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
#[derive(Clone, Copy, Default)]
struct CardExtras<'a> {
    /// The last verdict, and whether the issue changed since.
    review_pill: Option<(&'a str, Color32, bool)>,
    review_button: bool,
    /// In ColumnFocus: show more of the description.
    wide: bool,
    /// The reader's standard sections, for the wide card's facts row.
    sections: &'a [String],
}

/// An issue's last review, read from its sidecar, so its card pill can
/// reopen it.
#[derive(Clone)]
struct PastReview {
    draft: bool,
    mode: ReviewMode,
    agent: String,
    raw: String,
    report: Option<Report>,
    /// The issue changed since the agent read it.
    stale: bool,
}

impl PastReview {
    fn new(stored: &reviews::StoredReview, markdown: &str) -> Self {
        Self {
            draft: stored.draft,
            mode: ReviewMode::from_key(&stored.mode),
            agent: stored.agent.clone(),
            raw: stored.raw.clone(),
            report: agent::parse(&stored.raw),
            stale: stored.is_stale(markdown),
        }
    }

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

impl PastReview {
    /// The card's pill: the verdict, and whether the issue changed since.
    fn card_pill(&self, p: &Palette) -> (&'static str, Color32, bool) {
        let (word, color) = self.pill(p);
        (word, color, self.stale)
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
    /// The commits of an issue, as a diff ([`Session::diff`]).
    Diff,
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
/// A Done card gets Archive alone, any other Delete alone: either spans it.
const DOCK_H: f32 = 56.0 + 2.0 * 12.0 + 1.0;
/// How long the dock takes to slide in or out.
const DOCK_SLIDE: f32 = 0.14;
/// How long a status bar notice ("Archived …") stays up.
const NOTICE_FOR: Duration = Duration::from_secs(4);
/// How long `⌘Z` can bring back an issue deleted without asking.
const UNDO_FOR: Duration = Duration::from_secs(10);

/// The newest commit naming one issue's file, for the board it was looked up
/// on.
struct CommitHash {
    path: PathBuf,
    generation: u64,
    lookup: Option<Job<Option<String>>>,
    hash: Option<String>,
}

/// The commits popup: the hashes whose message names a ticket's file, read
/// from git when its button is pressed.
struct CommitsPanel {
    path: PathBuf,
    title: String,
    list: Vec<commits::Commit>,
}

/// An issue deleted without asking, kept so `⌘Z` can put it back.
/// What `⌘Z` on the board can still take back.
enum Undo {
    Deleted(Deleted),
    Split(split::SplitDone),
}

impl Undo {
    fn at(&self) -> Instant {
        match self {
            Undo::Deleted(deleted) => deleted.at,
            Undo::Split(split) => split.at,
        }
    }
}

struct Deleted {
    path: PathBuf,
    /// Where the Trash put it, when it could say.
    trashed: Option<PathBuf>,
    /// The file as it was, for when the Trash copy can't be moved back.
    content: Vec<u8>,
    at: Instant,
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
    answer: Job<Result<String, String>>,
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

/// What a review reads, and how far it goes.
struct ReviewTarget {
    path: PathBuf,
    draft: bool,
    mode: ReviewMode,
}

/// The AI review of one issue: running, answered or failed.
struct ReviewState {
    path: PathBuf,
    /// True when the agent read the editor draft rather than the saved file.
    draft: bool,
    mode: ReviewMode,
    /// The configured tool (and model), shown in the panel header.
    agent: String,
    status: ReviewStatus,
    /// [`reviews::content_hash`] of the text the agent reads.
    hash: String,
    /// The "Ask again" follow-up, kept between renders.
    follow_up: String,
}

enum ReviewStatus {
    Running {
        answer: Job<Result<String, String>>,
        progress: agent::Progress,
    },
    Done {
        raw: String,
        report: Option<Report>,
    },
    Failed {
        error: String,
    },
    /// The agent is cutting the issue; `raw` and `report` are the review it
    /// came from, to go back to.
    Splitting {
        answer: Job<Result<String, String>>,
        progress: agent::Progress,
        raw: String,
        report: Option<Report>,
    },
    /// The tickets the agent proposed, waiting to be created.
    SplitReady {
        drafts: Vec<split::SplitDraft>,
        raw: String,
        report: Option<Report>,
    },
}

pub struct IboardApp {
    located: Located,
    board: Board,
    /// The reader's markdown cache, and where its text was drawn.
    reader: Reader,
    /// The filter field in the header, and the filter it holds.
    filter_bar: FilterBar,
    /// What names the composer's new issues.
    title_engine: Engine,
    /// The Settings tab `⌘,` reopens.
    settings_tab: settings_ui::Tab,
    /// The last action that failed, until it is dismissed.
    error: Option<String>,
    /// Why the board could not be read; the next good scan clears it.
    scan_error: Option<String>,
    /// Settings › General: deleting an issue asks first.
    confirm_delete: bool,
    /// Settings › General: the theme the window draws in.
    appearance: Appearance,
    /// Where deleted issues go: the user's Trash in the app, a folder of its
    /// own for a board built in tests.
    trash: Trash,
    /// The last finished review of each issue this session, behind the
    /// card's verdict pill.
    past_reviews: HashMap<PathBuf, PastReview>,
    /// ColumnFocus: the column shown full screen, its sorts and grouping.
    column_focus: ColumnFocus,
    /// The last drag's Archive eligibility, so the dock keeps its look while
    /// it slides away after the drop.
    dock_archivable: bool,
    /// Auto: reload when files change on disk, save edits as you type and when
    /// leaving the editor. Off, the editor asks before dropping unsaved edits.
    auto: bool,
    last_refresh: Instant,
    /// Counts the boards adopted, so a background scan that started before
    /// a newer board never replaces it.
    board_generation: u64,
    /// The window may close: unsaved edits were saved, discarded or absent.
    closing: bool,
    /// Ask the window to close on the next frame (after the save prompt).
    send_close: bool,
    /// Projects opened recently, most recent first, shown in the header menu.
    recents: Vec<PathBuf>,
    /// Where `recents` is persisted; `None` in tests, so they never touch the
    /// user's cache.
    recents_file: Option<PathBuf>,
    /// A window title to apply on the next frame, after a project switch.
    pending_title: Option<String>,
    /// The user-configured AI tool and model.
    agent: AgentConfig,
    /// The user configuration Settings saves to; `None` in tests, so they
    /// never touch the user's file.
    config_file: Option<PathBuf>,
    /// What the user configuration holds that iboard could not use, shown in
    /// Settings until the next save.
    config_problem: Option<String>,
    /// The ReviewSettings dialog, while it is open.
    settings: Option<settings_ui::Dialog>,
    /// The installed agents and their models, for the settings dialog.
    catalog: Catalog,
    /// The label colours: the built-in preset, overridden by `[labels]` in the
    /// user configuration.
    labels: Labels,
    /// What belongs to the open project, rebuilt when another one opens.
    session: Session,
}

/// What belongs to the open project and goes with it: the screen, the
/// editor and composer, the panels open on its issues, its undo and its
/// background work. Opening another project starts a new one.
struct Session {
    screen: Screen,
    detail_mode: DetailMode,
    editor: Option<EditorState>,
    composer: Option<Composer>,
    /// The issue the composer just wrote, highlighted for a moment.
    new_card: Option<(PathBuf, Instant)>,
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
    /// The last issue deleted without asking, for `⌘Z`.
    undo: Option<Undo>,
    /// The rescan Auto runs in the background, and the board generation it
    /// started from.
    scan: Option<(Job<Result<Scanned, String>>, u64)>,
    /// The newest commit naming the open issue's file, looked up off the UI
    /// thread.
    commit_hash: Option<CommitHash>,
    /// The card selected on the board, moved with the arrow keys.
    selected: Option<PathBuf>,
    /// Scroll the selected card into view on the next frame.
    scroll_to_selected: bool,
    /// How far a two-finger swipe to the right has gone; `None` until the
    /// fingers lift after one went back.
    swipe: Option<f32>,
    /// Leaving the editor is on hold until the user answers the save prompt.
    leave_prompt: Option<Leave>,
    /// Set when a freshly created issue should receive keyboard focus.
    focus_editor: bool,
    /// The AI review of the open issue, if any.
    review: Option<ReviewState>,
    /// The reader's comments on the open issue.
    notes: Option<comments::Notes>,
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
    /// The Diff screen's commits, while it shows.
    diff: Option<diff_view::DiffView>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            screen: Screen::Board,
            detail_mode: DetailMode::Preview,
            editor: None,
            composer: None,
            confirm: None,
            new_card: None,
            leave_prompt: None,
            notes: None,
            selected: None,
            scroll_to_selected: false,
            swipe: Some(0.0),
            focus_editor: false,
            label_picker: None,
            commits: None,
            pending_front: None,
            notice: None,
            undo: None,
            design: None,
            design_system: None,
            design_error: None,
            design_draft: None,
            enrichment: None,
            scan: None,
            commit_hash: None,
            review: None,
            diff: None,
        }
    }
}

/// A component whose preview an agent is drafting.
struct DraftJob {
    name: String,
    job: Job<Result<String, String>>,
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
    Running(Job<Result<String, String>>),
    /// The mockup is in; the renderer is turning it into a PNG.
    Rendering(Job<Result<PathBuf, String>>),
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
        app.trash = Trash::System;
        // Likewise for the user configuration.
        app.config_file = settings::config_file();
        let mut config = UserConfig::load(app.config_file.as_deref());
        app.agent = agent::load(&mut config);
        app.filter_bar.suggestions = filter::load_suggestions(&mut config);
        app.title_engine = titles::load_engine(&mut config);
        app.confirm_delete = general::load_confirm_delete(&mut config);
        app.appearance = general::load_appearance(&mut config);
        theme::apply(&cc.egui_ctx, app.appearance);
        app.labels = Labels::load(&mut config);
        if !config.problems.is_empty() {
            let problem = config.problems.join(" ");
            app.error = Some(format!("Settings: {problem}"));
            app.config_problem = Some(problem);
        }
        app.catalog = Catalog::probing();
        let project = app.located.project_dir.clone();
        app.remember(&project);
        app
    }

    fn from_located(located: Located) -> Self {
        let ((board, sidecars), scan_error) = match scan(&located) {
            Ok(scanned) => (scanned, None),
            Err(err) => ((Board::empty(&located), HashMap::new()), Some(err)),
        };
        let past_reviews = past_reviews(&board, &sidecars);
        Self {
            located,
            session: Session::default(),
            board,
            reader: Reader::default(),
            filter_bar: FilterBar::default(),
            title_engine: Engine::Manual,
            settings_tab: settings_ui::Tab::General,
            error: None,
            scan_error,
            confirm_delete: true,
            appearance: Appearance::default(),
            trash: Trash::Folder(std::env::temp_dir().join("iboard-trash")),
            past_reviews,
            column_focus: ColumnFocus::default(),
            dock_archivable: false,
            auto: true,
            last_refresh: Instant::now(),
            board_generation: 0,
            closing: false,
            send_close: false,
            recents: Vec::new(),
            recents_file: None,
            pending_title: None,
            agent: AgentConfig::default(),
            config_file: None,
            config_problem: None,
            settings: None,
            catalog: Catalog::default(),
            labels: Labels::presets(),
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
    /// remembering it. A folder with no issues directory reports an error and
    /// leaves the current project in place. Everything that belongs to the old
    /// project is dropped: the editor, the composer, reviews, undo, the Design
    /// panel, and the agents still drafting for it.
    fn open_project(&mut self, project: PathBuf) {
        let project = recents::canonical(&project);
        match discovery::locate(&project) {
            Ok(located) => {
                self.pending_title = Some(discovery::window_title(&located));
                let project_dir = located.project_dir.clone();
                self.located = located;
                self.cancel_review();
                self.session = Session::default();
                self.past_reviews.clear();
                self.column_focus.column = None;
                self.filter_bar.filter.clear();
                self.error = None;
                self.scan_error = None;
                // Remember the project root discovery settled on, not the
                // folder the user happened to pick inside it.
                self.remember(&project_dir);
                self.refresh();
            }
            Err(err) => self.error = Some(format!("{err:#}")),
        }
    }

    /// Reads the board again, now: what an action that changed files needs
    /// before it goes on.
    fn refresh(&mut self) {
        self.adopt(scan(&self.located));
    }

    /// Takes a fresh scan of the board, or why it could not be read.
    fn adopt(&mut self, scanned: Result<Scanned, String>) {
        self.board_generation += 1;
        match scanned {
            Ok((board, sidecars)) => {
                self.past_reviews = past_reviews(&board, &sidecars);
                self.board = board;
                self.scan_error = None;
            }
            Err(err) => self.scan_error = Some(err),
        }
        self.reconcile_editor();
    }

    /// Whether `path` is still being written, in Todo: the only issues that
    /// get the AI review and the "Not written yet" block.
    fn is_todo_issue(&self, path: &Path) -> bool {
        self.board
            .column_of(path)
            .is_some_and(|column| column.accepts_new_issues())
    }

    fn is_done_issue(&self, path: &Path) -> bool {
        self.board.column_of(path).is_some_and(Column::is_done)
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
                if let Some(error) = self.scan_error.clone().or_else(|| self.error.clone()) {
                    self.error_strip(ui, &error);
                }
                match self.session.screen.clone() {
                    Screen::Board => {
                        // A focused column (ColumnFocus) is a board mode: the
                        // header and status bar stay, the board is replaced.
                        let columns = &self.board.columns;
                        self.column_focus.column = self
                            .column_focus
                            .column
                            .take()
                            .filter(|dir| columns.iter().any(|c| &c.dir == dir));
                        if self.column_focus.column.is_some() {
                            self.column_focus_ui(ui);
                        } else {
                            self.board_ui(ui);
                        }
                        self.drop_dock(ui);
                    }
                    Screen::Detail(path) => self.detail_ui(ui, &path),
                    Screen::Diff => self.diff_ui(ui),
                }
            });
        self.label_picker_ui(ui);
        if let Some(Confirm::Delete(path)) = self.session.confirm.clone() {
            self.confirm_dialog(ui, &path);
        }
        self.config_window(ui);
        self.design_window(ui);
        self.enrichment_window(ui);
        self.commits_window(ui);
        self.flush_pending_front(ui.ctx());
    }
}

/// A board and the review sidecars beside it, by slug.
type Scanned = (Board, HashMap<String, reviews::Sidecar>);

/// Reads the board of `located` and its reviews, or says why it could not.
fn scan(located: &Located) -> Result<Scanned, String> {
    let board = discovery::build(located).map_err(|err| format!("{err:#}"))?;
    let sidecars = reviews::load_all(&board.root);
    Ok((board, sidecars))
}

/// The last review of every issue on `board` that has one.
fn past_reviews(
    board: &Board,
    sidecars: &HashMap<String, reviews::Sidecar>,
) -> HashMap<PathBuf, PastReview> {
    board
        .columns
        .iter()
        .flat_map(|column| &column.issues)
        .filter_map(|issue| {
            let stored = sidecars.get(&reviews::slug(&issue.path))?.review.as_ref()?;
            Some((issue.path.clone(), PastReview::new(stored, &issue.markdown)))
        })
        .collect()
}

impl eframe::App for IboardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_logic(ctx);
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

/// One column tab of ColumnFocus: its name, count and status colour, and
/// whether it is the column being shown.
struct FocusTab {
    name: String,
    dir: PathBuf,
    count: usize,
    color: Color32,
    active: bool,
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

/// One of the DropZones in the dock.
#[derive(Clone, Copy)]
enum DropZone {
    /// Moves the issue to `.archive/`. Only shown while a Done card is dragged.
    Archive,
    /// Asks to confirm, then deletes.
    Delete,
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

/// What `card_body` drew: the action a child button asked for, and the rects
/// those buttons cover.
struct CardBody {
    action: Option<CardAction>,
    action_rects: Vec<Rect>,
}

/// Height of a ghost tile in the "Not written yet" block.
const GHOST_TILE_H: f32 = 72.0;

/// What the "Not written yet" block asked for.
enum NotWritten {
    Section(String),
    Review,
}

/// What the reader sheet's head strip shows.
#[derive(Clone, Copy)]
struct SheetHead<'a> {
    column: &'a str,
    status: Color32,
    path: &'a str,
    modified: Option<std::time::SystemTime>,
    /// `(ticked, total)` checkboxes; with none, the word count shows instead.
    checklist: (usize, usize),
    words: usize,
}

#[cfg(test)]
mod tests;

//! The egui application: a kanban board with a new-issue modal, a
//! rendered markdown reader, and an editor with debounced autosave. Its look
//! follows the design system in `design/` (see `theme.rs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::epaint::MarginF32;
use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{
    Align, Color32, CornerRadius, Frame, Key, Layout, Margin, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::agent::{self, AgentConfig, ReviewMode};
use crate::build_log;
use crate::commits;
use crate::design::{self, DesignSystem};
use crate::discovery::{self, Board, Column, Located};
use crate::domains::agent::artifacts::usage_log;
use crate::domains::preferences::labels::Labels;
use crate::files::{self, atomic_write, file_name, normalize_file_name, unique_name};
use crate::filter::{self, Filter};
use crate::format::{clock, display_path, relative_time};
use crate::issue::Issue;
use crate::job::{self, Job};
use crate::labels;
use crate::onboarding;
use crate::preferences::Preferences;
use crate::priority::ARCHIVE_DIR;
use crate::process;
use crate::prompts;
use crate::reviews;
use crate::settings::{self, UserConfig};
use crate::settings_ui::{self, Catalog};
use crate::shell::card_facts::CardFacts;
use crate::shell::instance::{self, Request};
use crate::shell::label_picker::{self, LabelPicker};
use crate::shell::recents;
use crate::shell::theme::{self, Icon, Palette};
use crate::shell::widgets::{self, ButtonKind};
use crate::titles::{self, Engine};
use crate::trash::{self, Trash};
use crate::workspace::{self, Group, Workspace};
use crate::{design_ui, sheet};

mod agenda;
mod agent_log;
mod ai_button;
mod all;
mod ask;
mod board;
mod build;
mod card;
mod comments;
mod compose;
mod design_panel;
mod diff_syntax;
mod diff_view;
mod dock;
mod editor;
mod enrich;
mod filter_bar;
mod folders;
mod frame;
mod groups;
mod handover;
mod header;
mod images;
mod intent;
mod keys;
mod labelling;
mod moves;
mod naming;
mod nav;
mod new_issue_ui;
mod notifications;
mod plan;
mod projects;
mod prose;
mod question;
mod quitting;
mod reader;
mod refine;
mod release;
mod review;
mod shortcuts;
mod side_panel;
mod split;
mod stats;
mod status;
mod traffic_lights;
mod usage_popover;
mod welcome;
mod windows;

use all::Source;
use compose::Composer;
use design_panel::DraftJob;
use editor::EditorState;
use enrich::Enrichment;
use filter_bar::FilterBar;
pub use quitting::on_signals;
use reader::Reader;
use review::{PastReview, ReviewState};
use windows::{CommitHash, CommitsPanel};

#[cfg(target_os = "macos")]
const ABOUT: &str = "**iboard** is an opinionated issue tracker that lives entirely as Markdown \
files right next to your code.\n\nReport issues at https://github.com/cdesiles/iboard";

/// How often Auto reads the board again.
const RESCAN_EVERY: Duration = Duration::from_secs(2);

/// How often the subscription meters read the usage log again.
const METERS_EVERY: Duration = Duration::from_secs(30);

/// On macOS the header bar doubles as the window's title bar (the native
/// title is hidden and the content runs under the traffic lights).
pub const UNIFIED_TITLEBAR: bool = cfg!(target_os = "macos");

/// A destructive action waiting for confirmation.
#[derive(Clone)]
enum Confirm {
    Delete(Vec<PathBuf>),
    /// Removing issues an agent works on: it stops the agent first.
    Stop(Removal, Vec<PathBuf>),
    /// Quitting while this many AI runs are at work.
    Quit(usize),
}

/// How issues leave the board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Removal {
    Delete,
    Archive,
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
    Detail(PathBuf, DetailMode),
    /// The commits of an issue, as a diff ([`Session::diff`]).
    Diff,
}

/// Where the user asked to go when leaving the editor.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Leave {
    Reader,
    Board,
    /// The board with NewIssueModal open (`⌘N`).
    Composer,
    /// The board a later `iboard` asked for.
    Open(all::Target),
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

/// How long a status bar notice ("Archived …") stays up.
const NOTICE_FOR: Duration = Duration::from_secs(4);
/// What `⌘Z` on the board can still take back, until the next action.
enum Undo {
    Deleted(Vec<Deleted>),
    Split(split::SplitDone),
}

/// An issue deleted without asking, kept so `⌘Z` can put it back.
struct Deleted {
    path: PathBuf,
    /// Where the Trash put it, when it could say.
    trashed: Option<PathBuf>,
    /// The file as it was, for when the Trash copy can't be moved back.
    content: Vec<u8>,
    /// Its images, when they went to the Trash with it.
    images: Option<images::TrashedImages>,
}

pub struct IboardApp {
    located: Located,
    /// The `[[workspace.group]]` projects, when the user lists some.
    workspace: Option<Workspace>,
    /// The group of `workspace` whose board is shown rather than `located`'s.
    group: Option<String>,
    /// Where each group's board keeps its order across projects; `None`
    /// keeps none (a board built in tests).
    order_dir: Option<PathBuf>,
    board: Board,
    /// What the board's cards show beyond their files: done dates and build
    /// times, read with the board.
    facts: CardFacts,
    /// The reader's markdown cache, and where its text was drawn.
    reader: Reader,
    /// The filter field in the header, and the filter it holds.
    filter_bar: FilterBar,
    /// The Settings tab `⌘,` reopens.
    settings_tab: settings_ui::Tab,
    /// The last action that failed, until it is dismissed.
    error: Option<String>,
    /// A project that could not be opened for lack of an issues directory,
    /// and the error it raised: the strip offers to create the directory.
    uninitialised: Option<(PathBuf, String)>,
    /// Why the board could not be read; the next good scan clears it.
    scan_error: Option<String>,
    /// Where deleted issues go: the user's Trash in the app, a `Trash` folder
    /// in the project for a board built in tests.
    trash: Trash,
    /// The last finished review of each issue this session, behind the
    /// card's verdict pill.
    past_reviews: HashMap<PathBuf, PastReview>,
    /// The last drag's Archive eligibility, so the dock keeps its look while
    /// it slides away after the drop.
    dock_archivable: bool,
    /// The dragged card's column on the board, which the dock spans; `None`
    /// spans the whole window. Kept while the dock slides away.
    dock_span: Option<egui::Rangef>,
    /// The board's report, this frame, of the column a card is dragged from.
    dragged_column: Option<egui::Rangef>,
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
    /// The cache folder plans are kept in; `None` in tests, for the same
    /// reason.
    plan_dir: Option<PathBuf>,
    /// The usage log the subscription meters read; `None` in tests, so they
    /// never read the user's spend.
    usage_log_file: Option<PathBuf>,
    /// The usage log's entries, as last read, and when that was.
    usage_entries: Vec<usage_log::Entry>,
    usage_read: Option<Instant>,
    /// A window title to apply on the next frame, after a project switch.
    pending_title: Option<String>,
    /// The frame's context, so background work can wake it when it lands;
    /// `None` until the first frame.
    ctx: Option<egui::Context>,
    /// What the iboards started later ask this window to show; `None` when no
    /// socket hands them over (a board built in tests).
    handovers: Option<Receiver<Request>>,
    /// What the user configuration sets, and Settings edits.
    prefs: Preferences,
    /// The user configuration Settings saves to; `None` in tests, so they
    /// never touch the user's file.
    config_file: Option<PathBuf>,
    /// What the user configuration holds that iboard could not use, shown in
    /// Settings until the next save or until the file reads again.
    config_problem: Option<String>,
    /// The ReviewSettings dialog, while it is open.
    settings: Option<settings_ui::Dialog>,
    /// The groups dialog, while it is up.
    groups_dialog: Option<crate::shell::groups_ui::Dialog>,
    /// The folders dialog, while it is up.
    folders_dialog: Option<crate::shell::folders_ui::Dialog>,
    /// The projects this launch already offered folders to, answered or not.
    folders_offered: HashSet<PathBuf>,
    /// The projects that answered Not now, remembered across launches.
    folders_quiet: Vec<PathBuf>,
    /// The user's own order of their projects, `workspace.projects`.
    projects: Vec<PathBuf>,
    /// The ProjectMenu, while it is open.
    project_menu: Option<crate::shell::project_menu::Menu>,
    /// What the groups dialog's Choose folders… asks; tests answer
    /// without the OS.
    folder_picker: fn() -> Vec<PathBuf>,
    /// When the user configuration was last written, to notice an edit made
    /// outside iboard.
    config_stamp: Option<std::time::SystemTime>,
    config_checked: Instant,
    /// The installed agents and their models, for the settings dialog.
    catalog: Catalog,
    /// Where a pasted image comes from: the pasteboard in the app, none in
    /// tests unless they hold one.
    clipboard: images::Clipboard,
    /// What belongs to the open project, rebuilt when another one opens.
    session: Session,
}

/// What belongs to the open project and goes with it: the screen, the
/// editor and composer, the panels open on its issues, its undo and its
/// background work. Opening another project starts a new one.
struct Session {
    screen: Screen,
    editor: Option<EditorState>,
    composer: Option<Composer>,
    /// The new issues whose titles are still to land.
    namings: Vec<naming::Naming>,
    /// A new issue discarded, for `⌘Z` in the next [`compose::DISCARD_UNDO`].
    discarded: Option<(Composer, Instant)>,
    /// The issue the composer just wrote, highlighted for a moment.
    new_card: Option<(PathBuf, Instant)>,
    /// A destructive action waiting for the user to confirm it.
    confirm: Option<Confirm>,
    /// The Ask AI dialog, while it is open.
    ask_draft: Option<ask::AskDraft>,
    /// `M` asked for the Move to… menu; it opens on the next draw.
    open_move_menu: bool,
    /// The asks the agent works on, by issue.
    asks: HashMap<PathBuf, ask::AskRun>,
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
    /// What the last scan read, so an idle rescan can skip the work when the
    /// folders, files and git heads it would read are unchanged.
    scanned: Option<files::Stamps>,
    /// The newest commit naming the open issue's file, looked up off the UI
    /// thread.
    commit_hash: Option<CommitHash>,
    /// The card selected on the board, moved with the arrow keys.
    selected: Option<PathBuf>,
    /// Scroll the selected card into view on the next frame.
    scroll_to_selected: bool,
    /// The cards shift- or command-clicked together; their actions and drags apply to
    /// all of them.
    picked: Vec<PathBuf>,
    /// How far a two-finger swipe to the right has gone; `None` until the
    /// fingers lift after one went back.
    swipe: Option<f32>,
    /// Leaving the editor is on hold until the user answers the save prompt.
    leave_prompt: Option<Leave>,
    /// Set when a freshly created issue should receive keyboard focus.
    focus_editor: bool,
    /// The review panel on the open issue, if any.
    review: Option<ReviewState>,
    /// The reviews running in the background, and those just landed.
    review_runs: review::ReviewRuns,
    /// The refinement of the open issue: a conversation with a challenger.
    refine: Option<refine::Refinement>,
    /// Done's release flow, while it asks before each step.
    release: Option<release::ReleaseFlow>,
    /// The reader's comments on the open issue.
    notes: Option<comments::Notes>,
    /// The Design panel, while it is open.
    design: Option<design_ui::Panel>,
    /// The Plan panel, from Tools › Plan….
    plan: Option<plan::Panel>,
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
    /// The AI build working through Ready, while it runs.
    build: Option<build::Build>,
    /// The forms and tool approvals each service's project is waiting on.
    feeds: HashMap<(&'static str, PathBuf), (Job<()>, agent::service::Feed)>,
    /// The question open for an answer.
    answer: Option<question::AnswerDraft>,
    /// The builds the service was still running were looked for, once.
    adopted: bool,
    /// That look, while it runs off the UI thread.
    adoption: Option<Job<Result<AdoptedBuilds, String>>>,
    /// Ready, watched for new issues from Play until Stop.
    watch: Option<build::Watch>,
    /// How many build agents may work at once, set on Doing's header.
    agents: usize,
    /// The agent log open beside the reader.
    agent_log: Option<agent_log::LogView>,
    /// The project stats panel, from Tools › Project Stats….
    stats: Option<stats::StatsPanel>,
    /// The light spend feed's poll, while the board shows.
    spend_feed: Option<Job<agent::spend::Live>>,
    /// When the spend feed last asked the service.
    spend_read: Option<Instant>,
    /// What the sessions running in the board's projects cost, as the feed
    /// last read it. A test sets it to show a run.
    live_spend: agent::spend::Live,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            screen: Screen::Board,
            editor: None,
            composer: None,
            discarded: None,
            confirm: None,
            ask_draft: None,
            open_move_menu: false,
            asks: HashMap::new(),
            new_card: None,
            leave_prompt: None,
            notes: None,
            selected: None,
            scroll_to_selected: false,
            picked: Vec::new(),
            swipe: Some(0.0),
            focus_editor: false,
            label_picker: None,
            commits: None,
            pending_front: None,
            namings: Vec::new(),
            notice: None,
            undo: None,
            design: None,
            plan: None,
            design_system: None,
            design_error: None,
            design_draft: None,
            enrichment: None,
            scan: None,
            scanned: None,
            commit_hash: None,
            review: None,
            review_runs: review::ReviewRuns::default(),
            refine: None,
            release: None,
            diff: None,
            build: None,
            feeds: HashMap::new(),
            answer: None,
            adopted: false,
            adoption: None,
            watch: None,
            agents: 1,
            agent_log: None,
            stats: None,
            spend_feed: None,
            spend_read: None,
            live_spend: agent::spend::Live::default(),
        }
    }
}

impl Session {
    /// The issue open in the editor.
    fn editing(&self) -> Option<&Path> {
        match &self.screen {
            Screen::Detail(path, DetailMode::Edit) => Some(path),
            _ => None,
        }
    }

    /// The issue open in the reader.
    fn reading(&self) -> Option<&Path> {
        match &self.screen {
            Screen::Detail(path, DetailMode::Preview) => Some(path),
            _ => None,
        }
    }

    /// The issue in the editor shows in the reader again; its buffer stays.
    fn stop_editing(&mut self) {
        if let Screen::Detail(_, mode) = &mut self.screen {
            *mode = DetailMode::Preview;
        }
    }
}

impl IboardApp {
    /// `window_title` names the window in Mission Control and the Window menu;
    /// see `main.rs` for why it is set here rather than at startup.
    /// `located` is `None` for the first group's board, or for the welcome
    /// screen when there is no group. `listener` is where the iboards started
    /// later hand this window their board.
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        located: Option<Located>,
        window_title: String,
        listener: Option<instance::Listener>,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        // Images in a ticket's markdown (`![shot](assets/<stem>/ui.png)`) load
        // from disk through egui's file loader.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(window_title));
        // After winit's default menu is up, so this one replaces it.
        #[cfg(target_os = "macos")]
        {
            crate::shell::menu::set_context(cc.egui_ctx.clone());
            crate::shell::menu::install("iboard", ABOUT, true);
            crate::pasteboard::watch(cc.egui_ctx.clone());
        }
        let all = located.is_none();
        let mut app = Self::from_located(located.unwrap_or_else(welcome::nowhere));
        quitting::watch(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        app.handovers =
            listener.map(|listener| instance::serve(listener, move || ctx.request_repaint()));
        app.clipboard = images::Clipboard::System;
        // The user's real cache: only the running app records here, never a
        // test constructing the board directly.
        app.recents_file = recents::cache_file();
        if let Some(file) = &app.recents_file {
            app.recents = recents::read(file);
        }
        app.plan_dir = files::cache_dir();
        app.usage_log_file = usage_log::path();
        app.refresh_usage();
        app.trash = Trash::System;
        // Likewise for the user configuration.
        app.config_file = settings::config_file();
        app.config_stamp = app.config_file.as_deref().and_then(|file| {
            std::fs::metadata(file)
                .and_then(|meta| meta.modified())
                .ok()
        });
        let mut config = UserConfig::load(app.config_file.as_deref());
        app.load_preferences(&mut config);
        app.folders_quiet = config
            .section::<workspace::WorkspaceSection>("workspace")
            .quiet_folders();
        app.restore_splits(&cc.egui_ctx);
        agent::set_direnv(app.prefs.direnv);
        theme::apply(&cc.egui_ctx, app.prefs.appearance);
        app.workspace = app.prefs.workspace.clone();
        let projects = app.prefs.projects.clone();
        app.reload_project_order(&projects);
        app.order_dir = workspace::order::dir();
        app.catalog = Catalog::probing();
        let first = app.workspace.as_ref().and_then(|w| w.groups.first());
        if let (true, Some(first)) = (all, first.map(|group| group.name.clone())) {
            app.open_group(&first);
        } else if !app.welcoming() {
            let project = app.located.project_dir.clone();
            app.remember(&project);
        }
        app.report_config_problems(&config.problems, groups::DEFAULTS_UNTIL_FIXED);
        app.offer_folders();
        app
    }

    /// Reads the user configuration into the app, and applies what shapes the
    /// session before a project opens.
    fn load_preferences(&mut self, config: &mut UserConfig) {
        self.prefs = Preferences::load(config);
        self.apply_agents(self.prefs.agents);
    }

    /// Reads the usage log again every [`METERS_EVERY`], for the subscription
    /// rings. A test has no log, so it never reads the user's spend.
    fn refresh_usage(&mut self) {
        let Some(path) = self.usage_log_file.clone() else {
            return;
        };
        if self
            .usage_read
            .is_some_and(|read| read.elapsed() < METERS_EVERY)
        {
            return;
        }
        self.usage_entries = usage_log::read_from(&path);
        self.usage_read = Some(Instant::now());
    }

    fn from_located(located: Located) -> Self {
        let source = Source::Project(located.clone());
        let stamps = scan_stamps(&source);
        let ((board, facts, past_reviews), scan_error) = match scan(&source) {
            Ok(scanned) => (scanned, None),
            Err(err) => (
                (Board::empty(&located), CardFacts::default(), HashMap::new()),
                Some(err),
            ),
        };
        let trash = Trash::Folder(located.project_dir.join("Trash"));
        let mut app = Self {
            located,
            workspace: None,
            group: None,
            order_dir: None,
            session: Session::default(),
            board,
            facts,
            reader: Reader::default(),
            filter_bar: FilterBar::default(),
            settings_tab: settings_ui::Tab::General,
            error: None,
            uninitialised: None,
            scan_error,
            trash,
            past_reviews,
            dock_archivable: false,
            dock_span: None,
            dragged_column: None,
            last_refresh: Instant::now(),
            board_generation: 0,
            closing: false,
            send_close: false,
            recents: Vec::new(),
            recents_file: None,
            plan_dir: None,
            usage_log_file: None,
            usage_entries: Vec::new(),
            usage_read: None,
            pending_title: None,
            ctx: None,
            handovers: None,
            prefs: Preferences {
                title_engine: Engine::Off,
                ..Preferences::default()
            },
            config_file: None,
            config_problem: None,
            settings: None,
            groups_dialog: None,
            folders_dialog: None,
            folders_offered: HashSet::new(),
            folders_quiet: Vec::new(),
            projects: Vec::new(),
            project_menu: None,
            folder_picker: crate::shell::dialog::pick_folders,
            config_stamp: None,
            config_checked: Instant::now(),
            catalog: Catalog::default(),
            clipboard: images::Clipboard::Held(None),
        };
        app.session.scanned = Some(stamps);
        app
    }

    /// Moves a project to the front of the recent list and persists it, when
    /// the cache path is known.
    fn remember(&mut self, project: &Path) {
        self.recents = recents::remember(std::mem::take(&mut self.recents), project);
        if let Some(file) = &self.recents_file {
            recents::write(file, &self.recents);
        }
        self.add_to_order(project);
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
                self.group = None;
                self.leave_project();
                // Remember the project root discovery settled on, not the
                // folder the user happened to pick inside it.
                self.remember(&project_dir);
                self.reopen();
                self.offer_folders();
            }
            Err(err) => {
                self.uninitialised = Some((project, err.clone()));
                self.error = Some(err);
            }
        }
    }

    /// Creates the standard issues directory for the project that had none,
    /// then opens it.
    fn initialise(&mut self, project: PathBuf) {
        self.uninitialised = None;
        match onboarding::init(&project) {
            Ok(_) => self.open_project(project),
            Err(err) => self.error = Some(err),
        }
    }

    /// Drops everything that belongs to the board shown until now.
    fn leave_project(&mut self) {
        self.close_review();
        self.session = Session::default();
        self.apply_agents(self.prefs.agents);
        self.past_reviews.clear();
        self.filter_bar.filter.clear();
        self.error = None;
        self.uninitialised = None;
        self.scan_error = None;
        self.folders_dialog = None;
    }

    /// Reads the board again, now: what an action that changed files needs
    /// before it goes on.
    fn refresh(&mut self) {
        let source = self.source();
        self.session.scanned = Some(scan_stamps(&source));
        self.adopt(scan(&source));
    }

    /// Puts the board of what just opened on screen at once, empty until the
    /// background scan fills it: a switch must not wait on the disk.
    fn reopen(&mut self) {
        self.board = match self.shown_group() {
            Some(group) => workspace::empty(group),
            None => Board::empty(&self.located),
        };
        self.facts = CardFacts::default();
        self.scan_error = None;
        self.reconcile_editor();
        self.start_scan();
    }

    /// Reads the board on a thread of its own; the next frames take it when
    /// it lands, and its job wakes them.
    fn start_scan(&mut self) {
        let source = self.source();
        self.session.scanned = Some(scan_stamps(&source));
        let scan = Job::spawn(move |_| scan(&source));
        if let Some(ctx) = self.ctx.clone() {
            scan.wake(move || ctx.request_repaint());
        }
        self.session.scan = Some((scan, self.board_generation));
    }

    /// Takes the board's background scan once it lands, unless a newer board
    /// replaced it. Its job wakes the frame when it does.
    fn take_scan(&mut self) {
        let Some((scan, generation)) = self.session.scan.as_ref() else {
            return;
        };
        let Some(scanned) = scan.answer() else {
            return;
        };
        let current = *generation == self.board_generation;
        self.session.scan = None;
        if current {
            self.adopt(scanned);
        }
    }

    /// Takes a fresh scan of the board, or why it could not be read. An answer
    /// no different from the board on screen leaves it, and the generation
    /// that the reader's images and the build panel follow, alone.
    fn adopt(&mut self, scanned: Result<Scanned, String>) {
        match scanned {
            Ok((board, facts, reviews)) => {
                if self.scan_error.is_none()
                    && board == self.board
                    && facts == self.facts
                    && reviews == self.past_reviews
                {
                    return;
                }
                self.board_generation += 1;
                self.board = board;
                self.facts = facts;
                self.past_reviews = reviews;
                self.scan_error = None;
            }
            Err(err) => {
                self.board_generation += 1;
                self.scan_error = Some(err);
            }
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
        match path.strip_prefix(self.board.root_of(path)) {
            Ok(relative) if relative.as_os_str().is_empty() => "./".to_string(),
            Ok(relative) if path.is_dir() => format!("{}/", relative.display()),
            Ok(relative) => relative.display().to_string(),
            Err(_) => display_path(path),
        }
    }

    /// A path relative to the project folder, where the agents run.
    fn project_relative(&self, path: &Path) -> String {
        match path.strip_prefix(self.board.project_of(path)) {
            Ok(relative) if relative.as_os_str().is_empty() => "./".to_string(),
            Ok(relative) => relative.display().to_string(),
            Err(_) => display_path(path),
        }
    }

    /// Draws the whole window: header bar, the current screen, status bar.
    fn draw(&mut self, ui: &mut egui::Ui) {
        if self.welcoming() {
            self.welcome_ui(ui);
            return;
        }
        let p = theme::palette(ui);
        let gutter = theme::SPACE_5;
        let header_left = if UNIFIED_TITLEBAR {
            theme::TITLEBAR_LEAD
        } else {
            gutter
        };
        egui::Panel::top("header")
            .exact_size(theme::BAR_H)
            .resizable(false)
            .frame(Frame::new().fill(p.bg).inner_margin(MarginF32 {
                left: header_left,
                right: gutter,
                top: 0.0,
                bottom: 0.0,
            }))
            .show(ui, |ui| self.header_bar(ui));
        egui::Panel::bottom("status")
            .exact_size(theme::STATUSBAR_H)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(p.bg)
                    .inner_margin(Vec2::new(theme::SPACE_5, 0.0)),
            )
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ui, |ui| {
                if let Some(error) = self.scan_error.clone().or_else(|| self.error.clone()) {
                    self.error_strip(ui, &error);
                }
                if self.session.stats.is_some() {
                    self.stats_screen(ui);
                    return;
                }
                match self.session.screen.clone() {
                    Screen::Board => {
                        self.board_ui(ui);
                        self.drop_dock(ui);
                    }
                    Screen::Detail(path, _) => self.detail_ui(ui, &path),
                    Screen::Diff => self.diff_ui(ui),
                }
            });
        self.shortcuts_sheet(ui);
        self.label_picker_ui(ui);
        self.ask_dialog(ui);

        match self.session.confirm.clone() {
            Some(Confirm::Delete(paths)) => self.confirm_dialog(ui, &paths),
            Some(Confirm::Stop(removal, paths)) => self.stop_dialog(ui, removal, &paths),
            Some(Confirm::Quit(runs)) => self.quit_dialog(ui, runs),
            None => {}
        }
        self.config_window(ui);
        self.groups_window(ui);
        self.folders_window(ui);
        self.design_window(ui);
        self.plan_window(ui);
        self.enrichment_window(ui);
        self.commits_window(ui);
        self.release_window(ui);
        self.new_issue_modal(ui);
        widgets::settle_modals(ui.ctx());
        self.flush_pending_front(ui.ctx());
    }
}

/// A board, the facts its cards show, and the last review of each of its
/// issues that has one.
type Scanned = (Board, CardFacts, HashMap<PathBuf, PastReview>);

/// The builds the service still runs, each with the project it belongs to.
type AdoptedBuilds = Vec<(PathBuf, agent::service::Found)>;

/// Reads the board of `source` and its reviews, or says why it could not.
/// Each project's reviews are read from its own issues folder: names are only
/// unique within one.
fn scan(source: &Source) -> Result<Scanned, String> {
    let board = match source {
        Source::Project(located) => discovery::build(located)?,
        Source::Group(group, file) => {
            let order = workspace::Order::load(file.as_deref());
            workspace::build(group, &workspace::virtual_root(&group.name), &order)
        }
    };
    let roots: Vec<PathBuf> = match board.repos.as_slice() {
        [] => vec![board.root.clone()],
        repos => repos.iter().map(|repo| repo.root.clone()).collect(),
    };
    let facts = CardFacts::read(&board);
    let mut reviews = HashMap::new();
    for root in roots {
        reviews.extend(past_reviews(&board, &root, &reviews::load_all(&root)));
    }
    Ok((board, facts, reviews))
}

/// What a scan of `source` reads: the folders and files of its projects, the
/// reviews and build logs beside their issues, and the git files their done
/// dates come from. Cheap enough to check every couple of seconds.
fn scan_stamps(source: &Source) -> files::Stamps {
    match source {
        Source::Project(located) => {
            let mut stamps = discovery::stamps(located);
            stamp_sidecars(&mut stamps, &located.root);
            stamps
        }
        Source::Group(group, order) => {
            let mut stamps = workspace::stamps(group, order.as_deref());
            for root in workspace::roots(group) {
                stamp_sidecars(&mut stamps, &root);
            }
            stamps
        }
    }
}

/// Adds the reviews and build logs a scan reads beside the issues in `root`.
fn stamp_sidecars(stamps: &mut files::Stamps, root: &Path) {
    for dir in [reviews::DIR, build_log::DIR] {
        stamps.add_folder(&root.join(dir));
    }
}

/// The last review of every issue of `board` under `root` that has one.
fn past_reviews(
    board: &Board,
    root: &Path,
    sidecars: &HashMap<String, reviews::Sidecar>,
) -> HashMap<PathBuf, PastReview> {
    board
        .columns
        .iter()
        .flat_map(|column| &column.issues)
        .filter(|issue| board.root_of(&issue.path) == root)
        .filter_map(|issue| {
            let stored = sidecars.get(&reviews::slug(&issue.path))?.review.as_ref()?;
            Some((issue.path.clone(), PastReview::new(stored, &issue.markdown)))
        })
        .collect()
}

impl eframe::App for IboardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        traffic_lights::keep_placed(ctx);
        self.frame_logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

#[cfg(test)]
mod tests;

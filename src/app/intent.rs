//! What the views ask of the app: one [`Intent`] per user action, applied in
//! one place once the frame's drawing is done.

use super::*;

/// One thing a view asked for.
pub(super) enum Intent {
    Open(PathBuf),
    Select(PathBuf),
    /// Shift-click: the issue joins or leaves the multi-selection.
    Pick(PathBuf),
    /// The column header's "Clear selection".
    ClearPicked,
    /// Run the AI review on the issue, in the background.
    Review(PathBuf),
    /// Reopen the issue's last review, from its card's verdict pill.
    ShowReview(PathBuf),
    Delete(PathBuf),
    Archive(PathBuf),
    /// A done issue's diff, from its card.
    Diff(PathBuf),
    /// A done issue's agent log, from its card: the reader, the log beside it.
    AgentLog(PathBuf),
    /// Play, in Ready's hovered header: build these issues, top first.
    Build(Vec<PathBuf>),
    /// Stop, in Ready's header: every build agent.
    StopBuild,
    /// Stop, on the card a build agent works on: that agent.
    StopAgent(PathBuf),
    /// Retry, on a Doing card: a build agent starts the issue over.
    RetryBuild(PathBuf),
    /// Doing's `−` and `+`: how many build agents may work at once.
    Agents(usize),
    /// "Archive all", in Done's hovered header.
    ArchiveAll(Vec<PathBuf>),
    /// Open or close the LabelPicker on the issue.
    ToggleLabels(PathBuf),
    /// A label was clicked: filter the board by it.
    FilterBy(String),
    /// A label was clicked in the reader: the board, filtered by it.
    ShowLabel(String),
    /// The reader's "Add a section": the editor, with the heading appended.
    AddSection {
        path: PathBuf,
        section: String,
    },
    /// The empty rest of the reader's sheet: the editor, at the end.
    EditAtEnd(PathBuf),
    /// A dragged card was dropped in `column`, in front of `anchor` (last
    /// without one).
    Place {
        path: PathBuf,
        column: PathBuf,
        anchor: Option<PathBuf>,
    },
    /// A dragged card was dropped on a column tab.
    Move {
        path: PathBuf,
        to: PathBuf,
    },
    /// Show this column full screen (ColumnFocus).
    FocusColumn(PathBuf),
    UnfocusColumn,
    Sort {
        column: PathBuf,
        sort: Sort,
    },
    Compose,
    /// Add what the composer holds; `close` also closes it.
    SubmitComposer {
        close: bool,
    },
    CloseComposer,
    /// The composer's engine tag: open Settings › Titles.
    TitleSettings,
}

impl CardAction {
    /// What this card action asks for, on the card's issue.
    pub(super) fn on(self, path: &Path) -> Intent {
        let path = path.to_path_buf();
        match self {
            CardAction::Open => Intent::Open(path),
            CardAction::Select => Intent::Select(path),
            CardAction::Pick => Intent::Pick(path),
            CardAction::Delete => Intent::Delete(path),
            CardAction::Archive => Intent::Archive(path),
            CardAction::Diff => Intent::Diff(path),
            CardAction::AgentLog => Intent::AgentLog(path),
            CardAction::FilterLabel(label) => Intent::FilterBy(label),
            CardAction::Review => Intent::Review(path),
            CardAction::ShowReview => Intent::ShowReview(path),
            CardAction::AddLabel => Intent::ToggleLabels(path),
            CardAction::StopBuild => Intent::StopAgent(path),
            CardAction::RetryBuild => Intent::RetryBuild(path),
        }
    }
}

impl IboardApp {
    /// Applies the frame's intents, in the order they were asked for.
    pub(super) fn apply(&mut self, intents: Vec<Intent>) {
        // A click elsewhere both blurs the composer and adds what it holds:
        // adding wins over closing it empty.
        let submitting = intents
            .iter()
            .any(|intent| matches!(intent, Intent::SubmitComposer { .. }));
        for intent in intents {
            match intent {
                Intent::Open(path) => self.open_issue(path),
                Intent::Select(path) => self.select(path),
                Intent::Pick(path) => self.pick(path),
                Intent::ClearPicked => {
                    self.session.picked.clear();
                    self.session.selected = None;
                }
                Intent::Review(path) => self.start_review(&path, false),
                Intent::ShowReview(path) => self.show_past_review(path),
                Intent::Delete(path) => self.request_delete(path),
                Intent::Archive(path) => self.archive_issue(&path),
                Intent::Diff(path) => self.show_issue_diff(&path),
                Intent::AgentLog(path) => self.show_agent_log(path),
                Intent::ArchiveAll(paths) => self.archive_all(&paths),
                Intent::Build(paths) => self.start_build(paths),
                Intent::StopBuild => self.stop_build(),
                Intent::StopAgent(path) => self.stop_agent(&path),
                Intent::Agents(agents) => self.set_agents(agents),
                Intent::RetryBuild(path) => self.retry_build(path),
                Intent::ToggleLabels(path) => self.toggle_label_picker(path),
                Intent::FilterBy(label) => self.filter_bar.filter.pick(&label),
                Intent::ShowLabel(label) => {
                    self.filter_bar.filter.pick(&label);
                    self.back_to_board();
                }
                Intent::AddSection { path, section } => self.add_section(&path, &section),
                Intent::EditAtEnd(path) => {
                    self.start_editing(&path);
                    self.session.focus_editor = true;
                }
                Intent::Place {
                    path,
                    column,
                    anchor,
                } => self.place_issue(&path, &column, anchor),
                Intent::Move { path, to } => self.move_issues(&path, &to),
                Intent::FocusColumn(dir) => self.focus_column(dir),
                Intent::UnfocusColumn => self.unfocus_column(),
                Intent::Sort { column, sort } => {
                    self.column_focus.sorts.insert(column, sort);
                }
                Intent::Compose => self.open_composer(),
                Intent::SubmitComposer { close } => self.submit_composer(close),
                Intent::CloseComposer if !submitting => self.session.composer = None,
                Intent::CloseComposer => {}
                Intent::TitleSettings => self.open_title_settings(),
            }
        }
    }
}

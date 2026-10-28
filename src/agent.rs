//! The AI assist that challenges an issue before it is implemented.
//!
//! The agent is chosen once in the ReviewSettings dialog (`⌘,`) and stored in
//! `$XDG_CONFIG_HOME/iboard/iboard.toml` (`$HOME/.config/iboard/iboard.toml`
//! when `XDG_CONFIG_HOME` is unset):
//!
//! ```toml
//! [ai]
//! tool = "opencode"        # "opencode" | "claude" | "custom"
//! model = "default"        # or a model ID from the tool's list
//! command = ""             # custom only, e.g. "llm -m {model} < {prompt}"
//! scope = "ticket"         # "ticket" (the issue alone) | "project" (reads the repo)
//! ```
//!
//! A `[build]` section, of the same shape, names the agent that implements
//! Ready issues; without one, the review's agent does.
//!
//! `default` is the tool's first favorite model, so it follows the tool's own
//! settings. [`start`] runs the tool non-interactively against the issue text
//! and hands the answer back over a channel; the GUI polls it. The agent only
//! ever returns text: writing the file stays the user's decision.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::build_log;
use crate::files;
use crate::job::{Cancel, Job};
use crate::process;
use crate::settings::{self, Section, UserConfig};

/// What a [`AgentConfig::model`] of "the tool's first favorite" is written as.
const DEFAULT_MODEL: &str = "default";

/// Models Claude Code accepts for `--model`. It has no command that lists
/// them, so the aliases come first, then the current full IDs.
const CLAUDE_MODELS: [&str; 7] = [
    "opus",
    "sonnet",
    "haiku",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-4-5-20251001",
];

/// The agents the settings offer, in the order they list them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Tool {
    #[default]
    OpenCode,
    Claude,
    /// Any command the user writes, with `{prompt}`, `{model}` and `{issue}`.
    Custom,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::OpenCode, Tool::Claude, Tool::Custom];

    /// The name stored in the configuration and shown in "Runs with".
    pub fn key(self) -> &'static str {
        match self {
            Tool::OpenCode => "opencode",
            Tool::Claude => "claude",
            Tool::Custom => "custom",
        }
    }

    /// The name shown in the Tool select.
    pub fn name(self) -> &'static str {
        match self {
            Tool::OpenCode => "OpenCode",
            Tool::Claude => "Claude Code",
            Tool::Custom => "Custom command",
        }
    }

    /// The executable iboard looks for on `PATH`; `None` for a custom command.
    pub fn binary(self) -> Option<&'static str> {
        match self {
            Tool::OpenCode => Some("opencode"),
            Tool::Claude => Some("claude"),
            Tool::Custom => None,
        }
    }

    fn from_key(key: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|tool| tool.key() == key)
    }

    /// The arguments that run a prompt in plan mode, where the agent may read
    /// the repository but not change it; empty for a tool without one.
    fn plan_args(self) -> &'static [&'static str] {
        match self {
            Tool::OpenCode => &["--agent", "plan"],
            Tool::Claude => &["--permission-mode", "plan"],
            Tool::Custom => &[],
        }
    }

    /// The program that runs the tool: its binary, or the shell for a custom
    /// command.
    fn program(self) -> PathBuf {
        let Some(name) = self.binary() else {
            return PathBuf::from("sh");
        };
        #[cfg(test)]
        run_tools_from(Path::new(tests::FAKES));
        match TOOLS.get() {
            Some(dir) => dir.join(name),
            None => find_binary(name).unwrap_or_else(|| PathBuf::from(name)),
        }
    }
}

/// Where `opencode` and `claude` run from instead of `PATH`, once set.
static TOOLS: OnceLock<PathBuf> = OnceLock::new();

/// Runs `opencode` and `claude` from `dir` for the rest of the process, so
/// tests never start a real agent. The first call wins.
pub fn run_tools_from(dir: &Path) {
    let _ = TOOLS.set(dir.to_path_buf());
}

/// `[direnv] enabled`, read once at launch.
static DIRENV: AtomicBool = AtomicBool::new(false);

/// Runs agents through `direnv exec` in projects that have an `.envrc`, for
/// the rest of the process.
pub fn set_direnv(enabled: bool) {
    DIRENV.store(enabled, Ordering::Relaxed);
}

/// The `[ai]` section of the user's configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentConfig {
    pub tool: Tool,
    /// Model ID as the tool names it. Empty means Default: the tool's first
    /// favorite model.
    pub model: String,
    /// The custom command, with its placeholders. Unused by the other tools.
    pub command: String,
    /// Whether an agent was ever saved, so the Review button can run.
    pub configured: bool,
    /// How far a review goes unless asked otherwise. Only `[ai]` stores it.
    pub scope: ReviewMode,
}

impl AgentConfig {
    /// Whether a tool is configured, so the Review button can be enabled.
    pub fn configured(&self) -> bool {
        self.configured
    }

    /// What the Review menu and the panel say will run: `opencode · default`.
    pub fn label(&self) -> String {
        let model = self.model.trim();
        let model = if model.is_empty() {
            DEFAULT_MODEL
        } else {
            model
        };
        format!("{} · {model}", self.tool.key())
    }

    /// Checks what can only be checked on Save: a custom command must say
    /// where the issue goes.
    pub fn validate(&self) -> Result<(), String> {
        if self.tool == Tool::Custom && !self.command.contains("{prompt}") {
            return Err("The command needs {prompt} to receive the issue.".to_string());
        }
        Ok(())
    }

    fn to_raw(&self) -> RawAgent {
        let model = self.model.trim();
        RawAgent {
            tool: self.tool.key().to_string(),
            model: if model.is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                model.to_string()
            },
            command: if self.tool == Tool::Custom {
                self.command.clone()
            } else {
                String::new()
            },
            scope: self.scope.scope_key().to_string(),
        }
    }
}

/// The `[ai]` section as written on disk.
#[derive(Debug, Default, Deserialize, Serialize)]
struct RawAgent {
    #[serde(default)]
    tool: String,
    #[serde(default)]
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    scope: String,
}

/// The agent in the `[ai]` section; unconfigured when there is none, or when
/// its tool is not one iboard runs, which is reported.
pub fn load(config: &mut UserConfig) -> AgentConfig {
    load_section(config, "ai").unwrap_or_default()
}

/// The agent in the `[build]` section, or `review`'s when there is none.
pub fn load_build(config: &mut UserConfig, review: &AgentConfig) -> AgentConfig {
    load_section(config, "build").unwrap_or_else(|| review.clone())
}

fn load_section(config: &mut UserConfig, name: &str) -> Option<AgentConfig> {
    let raw: RawAgent = config.section(name);
    let tool = raw.tool.trim();
    if tool.is_empty() {
        return None;
    }
    let Some(tool) = Tool::from_key(tool) else {
        config.problem(format!(
            "[{name}]: tool \"{tool}\" is not opencode, claude or custom"
        ));
        return None;
    };
    let model = raw.model.trim();
    Some(AgentConfig {
        tool,
        model: if model.eq_ignore_ascii_case(DEFAULT_MODEL) {
            String::new()
        } else {
            model.to_string()
        },
        command: raw.command,
        configured: true,
        scope: ReviewMode::from_scope_key(&raw.scope),
    })
}

/// The `[ai]` section that stores `config`.
pub fn section(config: &AgentConfig) -> Result<Section, String> {
    Section::new("ai", &config.to_raw())
}

/// The `[build]` section that stores `config`.
pub fn build_section(config: &AgentConfig) -> Result<Section, String> {
    let raw = RawAgent {
        scope: String::new(),
        ..config.to_raw()
    };
    Section::new("build", &raw)
}

/// The folders searched for a tool: `PATH`, then where installers put them.
/// An app started from the Finder inherits a minimal `PATH`, so without the
/// extra folders a Homebrew or npm install would look missing.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let mut extra = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = settings::home() {
        for dir in [
            ".local/bin",
            ".opencode/bin",
            ".claude/local",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            extra.push(home.join(dir));
        }
    }
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The `PATH` agents run with: the search folders above.
fn search_path() -> OsString {
    std::env::join_paths(search_dirs()).unwrap_or_default()
}

/// The full path of an executable named `name`, if one is installed.
fn find_binary(name: &str) -> Option<PathBuf> {
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// What the settings know about an installed tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detected {
    /// Where the binary is; `None` when it is not on `PATH`.
    pub path: Option<PathBuf>,
    /// Its version, as `v1.2.3`, when it reports one.
    pub version: Option<String>,
}

/// Looks for `tool` and asks it for its version. Blocking: it runs the tool.
pub fn detect(tool: Tool) -> Detected {
    let Some(path) = tool.binary().and_then(find_binary) else {
        return Detected::default();
    };
    let mut command = Command::new(&path);
    command.arg("--version").env("PATH", search_path());
    let version = process::run(command, tool.key(), &Cancel::default(), PROBE_TIMEOUT)
        .and_then(|output| output.into_result(tool.key()))
        .ok()
        .and_then(|stdout| parse_version(&stdout));
    Detected {
        path: Some(path),
        version,
    }
}

/// The first version-looking word of `text` (`2.1.0 (Claude Code)`,
/// `opencode v2.0.20`), as `v2.1.0`.
fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|word| word.trim_start_matches(['v', 'V']))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()) && word.contains('.'))
        .map(|word| format!("v{word}"))
}

/// One model a tool can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    /// Marked as a favorite in the tool's own settings.
    pub favorite: bool,
}

/// The models a tool offers, and what Default resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Models {
    /// The list, or why it could not be read.
    pub list: Result<Vec<Model>, String>,
    /// The tool's first favorite model, which Default runs; `None` when the
    /// tool has none and picks for itself.
    pub default: Option<String>,
}

/// Lists the models of `tool`. Blocking: for OpenCode it runs `opencode models`.
pub fn list_models(tool: Tool) -> Models {
    let favorites = favorites(tool);
    let ids: Result<Vec<String>, String> = match tool {
        Tool::OpenCode => opencode_models(),
        Tool::Claude => Ok(CLAUDE_MODELS.iter().map(|id| id.to_string()).collect()),
        Tool::Custom => Ok(Vec::new()),
    };
    let list = ids.map(|mut ids| {
        // A favorite the list forgot is still a model the tool runs.
        for favorite in favorites.iter().rev() {
            if !ids.contains(favorite) {
                ids.insert(0, favorite.clone());
            }
        }
        ids.into_iter()
            .map(|id| Model {
                favorite: favorites.contains(&id),
                id,
            })
            .collect()
    });
    Models {
        list,
        default: favorites.into_iter().next(),
    }
}

/// The model Default stands for: the tool's first favorite.
fn default_model(tool: Tool) -> Option<String> {
    favorites(tool).into_iter().next()
}

fn favorites(tool: Tool) -> Vec<String> {
    match tool {
        Tool::OpenCode => opencode_state_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| parse_opencode_favorites(&text))
            .unwrap_or_default(),
        Tool::Claude => claude_settings_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| parse_claude_model(&text))
            .into_iter()
            .collect(),
        Tool::Custom => Vec::new(),
    }
}

fn opencode_models() -> Result<Vec<String>, String> {
    let binary =
        find_binary("opencode").ok_or_else(|| "opencode is not on your PATH".to_string())?;
    let mut command = Command::new(binary);
    command.arg("models").env("PATH", search_path());
    let what = "opencode models";
    let stdout =
        process::run(command, what, &Cancel::default(), PROBE_TIMEOUT)?.into_result(what)?;
    Ok(parse_model_list(&stdout))
}

/// One model ID per line; anything with a space is a banner, not an ID.
fn parse_model_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// OpenCode keeps favorites in its state folder, in `model.json`.
fn opencode_state_file() -> Option<PathBuf> {
    settings::dir_from_env("XDG_STATE_HOME", ".local/state")
        .map(|state| state.join("opencode/model.json"))
}

/// The `favorite` list of OpenCode's `model.json`, as `provider/model` IDs.
fn parse_opencode_favorites(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    value["favorite"]
        .as_array()
        .map(|favorites| {
            favorites
                .iter()
                .filter_map(|entry| {
                    let provider = entry["providerID"].as_str()?;
                    let model = entry["modelID"].as_str()?;
                    Some(format!("{provider}/{model}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Claude Code's user settings, where `model` is its chosen default.
fn claude_settings_file() -> Option<PathBuf> {
    settings::dir_from_env("CLAUDE_CONFIG_DIR", ".claude").map(|dir| dir.join("settings.json"))
}

fn parse_claude_model(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    value["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// How long an agent may work on an answer before it is stopped.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long an agent may work on implementing an issue.
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// How long `--version` and `opencode models` may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// The shell variables a custom command's placeholders become.
const PLACEHOLDERS: [(&str, &str); 4] = [
    ("{prompt}", "IBOARD_PROMPT"),
    ("{model}", "IBOARD_MODEL"),
    ("{issue}", "IBOARD_ISSUE"),
    ("{mode}", "IBOARD_MODE"),
];

/// How far a review goes: Quick judges the issue's text alone, without
/// tools; Deep may read the repository to check what the issue claims.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReviewMode {
    #[default]
    Quick,
    Deep,
}

impl ReviewMode {
    /// The name stored with a review and shown in its panel.
    pub fn key(self) -> &'static str {
        match self {
            ReviewMode::Quick => "quick",
            ReviewMode::Deep => "deep",
        }
    }

    pub fn from_key(key: &str) -> Self {
        if key == "deep" {
            ReviewMode::Deep
        } else {
            ReviewMode::Quick
        }
    }

    pub const ALL: [ReviewMode; 2] = [ReviewMode::Quick, ReviewMode::Deep];

    /// The review scope as Settings › AI review and the review panel name it.
    pub fn scope_name(self) -> &'static str {
        match self {
            ReviewMode::Quick => "Ticket only",
            ReviewMode::Deep => "Ticket + project context",
        }
    }

    /// The `scope` stored in `[ai]`.
    fn scope_key(self) -> &'static str {
        match self {
            ReviewMode::Quick => "ticket",
            ReviewMode::Deep => "project",
        }
    }

    fn from_scope_key(key: &str) -> Self {
        if key.trim() == "project" {
            ReviewMode::Deep
        } else {
            ReviewMode::Quick
        }
    }
}

/// What a run of the agent is for: a plain answer, a review (in plan mode
/// when `plan`), or implementing an issue, which edits the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Answer,
    Review { mode: ReviewMode, plan: bool },
    Build,
}

impl Purpose {
    /// A custom command's `{mode}`.
    fn key(self) -> &'static str {
        match self {
            Purpose::Answer => "",
            Purpose::Review { mode, .. } => mode.key(),
            Purpose::Build => "build",
        }
    }
}

/// The step a running review is on, as "Reading src/app.rs…", shared with
/// the panel that shows it.
pub type Progress = Arc<Mutex<Option<String>>>;

/// The latest step of `progress`, if the agent reported one.
pub fn step(progress: &Progress) -> Option<String> {
    progress
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Builds the command for `config`, ready to spawn. `model` is the resolved
/// one (empty for the tool's own choice); `prompt_file` holds `prompt` for a
/// custom command. Factored out so the argument shape per tool is testable
/// without running anything.
fn command(
    config: &AgentConfig,
    model: &str,
    prompt: &str,
    prompt_file: &Path,
    issue: &Path,
    purpose: Purpose,
) -> Command {
    let mut command = match config.tool {
        Tool::Custom => {
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .arg(shell_script(&config.command))
                .env("IBOARD_PROMPT", prompt_file)
                .env("IBOARD_MODEL", model)
                .env("IBOARD_ISSUE", issue)
                .env("IBOARD_MODE", purpose.key());
            command
        }
        tool => {
            let mut command = Command::new(tool.program());
            // `opencode run "…"` and `claude -p "…"` answer once and exit.
            command.arg(if tool == Tool::OpenCode { "run" } else { "-p" });
            // A review or a build streams its steps as JSON events, so iboard
            // can say what the agent is doing; a quick review gets no built-in
            // tools, a build may edit files.
            match (tool, purpose) {
                (_, Purpose::Answer) => {}
                (Tool::OpenCode, _) => {
                    command.args(["--format", "json"]);
                }
                (_, _) => {
                    command.args(["--output-format", "stream-json", "--verbose"]);
                    match purpose {
                        Purpose::Review {
                            mode: ReviewMode::Quick,
                            ..
                        } => {
                            command.args(["--tools", ""]);
                        }
                        Purpose::Build => {
                            command.args(["--permission-mode", "acceptEdits"]);
                        }
                        _ => {}
                    }
                }
            }
            if let Purpose::Review { plan: true, .. } = purpose {
                command.args(tool.plan_args());
            }
            if !model.is_empty() {
                command.arg("--model").arg(model);
            }
            command.arg(prompt);
            command
        }
    };
    command.env("PATH", search_path());
    command
}

/// A custom command with its placeholders turned into quoted shell variables,
/// so no file name or model is ever read as code. A placeholder the command
/// already wrapped in quotes keeps one pair of them.
fn shell_script(command: &str) -> String {
    let mut script = command.to_string();
    for (placeholder, variable) in PLACEHOLDERS {
        let quoted = format!("\"${variable}\"");
        for wrapped in [format!("\"{placeholder}\""), format!("'{placeholder}'")] {
            script = script.replace(&wrapped, &quoted);
        }
        script = script.replace(placeholder, &quoted);
    }
    script
}

/// Runs the agent on `prompt` and waits for its answer, on this thread. The
/// tool runs without a terminal, which is what one-shot modes need; it is
/// stopped when `cancel` says so, or after [`ANSWER_TIMEOUT`]. `issue` is the
/// file the prompt is about, a custom command's `{issue}`; the agent runs
/// from `project` when set.
pub fn run(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
    cancel: &Cancel,
) -> Result<String, String> {
    let workspace = project.map(Workspace::of);
    let purpose = Purpose::Answer;
    run_with(
        config,
        prompt,
        issue,
        cancel,
        purpose,
        None,
        workspace.as_ref(),
    )
}

/// Where an agent runs: the project it reads or edits, and for a build the
/// log its output goes to.
struct Workspace<'a> {
    dir: &'a Path,
    log: Option<&'a Path>,
}

impl<'a> Workspace<'a> {
    fn of(dir: &'a Path) -> Self {
        Workspace { dir, log: None }
    }
}

/// [`run`] for `purpose`, in `workspace` when set; the agent's steps land in
/// `progress` as they come.
fn run_with(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    cancel: &Cancel,
    purpose: Purpose,
    progress: Option<&Progress>,
    workspace: Option<&Workspace>,
) -> Result<String, String> {
    let model = match config.model.trim() {
        "" if config.tool != Tool::Custom => default_model(config.tool).unwrap_or_default(),
        model => model.to_string(),
    };
    // A custom command reads the prompt from a file: it may be too long for
    // an argument, and `{prompt}` can then go anywhere in the command.
    let custom = config.tool == Tool::Custom;
    let prompt_file = files::temp_path("prompt", "md");
    if custom {
        std::fs::write(&prompt_file, prompt)
            .map_err(|err| format!("Couldn't write {}: {err}", prompt_file.display()))?;
    }
    let what = match config.tool {
        Tool::Custom => "the custom command",
        tool => tool.key(),
    };
    let mut command = command(config, &model, prompt, &prompt_file, issue, purpose);
    if let Some(workspace) = workspace {
        command.current_dir(workspace.dir);
        if direnv::applies(DIRENV.load(Ordering::Relaxed), workspace.dir)
            && let Some(binary) = find_binary("direnv")
        {
            command = direnv::wrap(&command, &binary, workspace.dir);
        }
    }
    let mut log = workspace
        .and_then(|workspace| workspace.log)
        .and_then(|log| build_log::open(log).ok());
    let timeout = if purpose == Purpose::Build {
        BUILD_TIMEOUT
    } else {
        ANSWER_TIMEOUT
    };
    let streams = progress.filter(|_| !custom);
    let output = process::run_with(command, what, cancel, timeout, |line| {
        if let Some(progress) = streams
            && let Some(step) = step_of(line)
        {
            *progress.lock().unwrap_or_else(PoisonError::into_inner) = Some(step);
        }
        if let Some(log) = log.as_mut() {
            for entry in build_log::entries(line) {
                let _ = writeln!(log, "{entry}");
            }
        }
    });
    if custom {
        let _ = std::fs::remove_file(&prompt_file);
    }
    let output = output?;
    let failure = stream_error(&output.stdout);
    let answer = output
        .into_result("the agent")
        .map_err(|err| failure.clone().unwrap_or(err))?;
    let answer = match streams {
        Some(_) => final_answer(&answer),
        None => answer,
    };
    match answer.trim() {
        "" if purpose != Purpose::Build => {
            Err(failure.unwrap_or_else(|| "the agent returned no answer".to_string()))
        }
        answer => Ok(answer.to_string()),
    }
}

/// [`run`] on a background thread. Dropping the job stops the agent.
pub fn start(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
) -> Job<Result<String, String>> {
    let (config, prompt, issue) = (config.clone(), prompt.to_string(), issue.to_path_buf());
    let project = project.map(Path::to_path_buf);
    Job::spawn(move |cancel| run(&config, &prompt, &issue, project.as_deref(), cancel))
}

/// A review's answer, and whether the agent gave it in plan mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewed {
    pub answer: String,
    pub plan: bool,
}

/// What a review is about: the issue, and the project the agent reads.
struct Subject {
    issue: PathBuf,
    project: Option<PathBuf>,
}

impl Subject {
    fn new(issue: &Path, project: Option<&Path>) -> Self {
        Subject {
            issue: issue.to_path_buf(),
            project: project.map(Path::to_path_buf),
        }
    }
}

/// Runs a review in plan mode when the tool has one, and again without it
/// when the tool rejects the flag.
fn review(
    config: &AgentConfig,
    prompt: &str,
    subject: &Subject,
    cancel: &Cancel,
    mode: ReviewMode,
    progress: &Progress,
) -> Result<Reviewed, String> {
    let workspace = subject.project.as_deref().map(Workspace::of);
    let run = |plan| {
        let purpose = Purpose::Review { mode, plan };
        let (issue, steps) = (&subject.issue, Some(progress));
        run_with(
            config,
            prompt,
            issue,
            cancel,
            purpose,
            steps,
            workspace.as_ref(),
        )
        .map(|answer| Reviewed { answer, plan })
    };
    if config.tool.plan_args().is_empty() {
        return run(false);
    }
    match run(true) {
        Err(error) if !cancel.is_cancelled() && rejects_plan(&error) => run(false),
        result => result,
    }
}

/// Whether an agent's error says it does not know the plan-mode flag.
fn rejects_plan(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    let signs = [
        "--agent",
        "--permission-mode",
        "unknown option",
        "unknown argument",
        "unexpected argument",
        "unrecognized",
    ];
    signs.iter().any(|sign| error.contains(sign))
        || (error.contains("agent") && error.contains("not found"))
}

/// [`review`] on a background thread, with the [`Progress`] its steps land
/// in. Dropping the job stops the agent.
pub fn start_plan_review(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
    mode: ReviewMode,
) -> (Job<Result<Reviewed, String>>, Progress) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let subject = Subject::new(issue, project);
    let progress = Progress::default();
    let steps = Arc::clone(&progress);
    let job = Job::spawn(move |cancel| review(&config, &prompt, &subject, cancel, mode, &steps));
    (job, progress)
}

/// [`start_plan_review`] for its answer alone.
pub fn start_review(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
    mode: ReviewMode,
) -> (Job<Result<String, String>>, Progress) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let subject = Subject::new(issue, project);
    let progress = Progress::default();
    let steps = Arc::clone(&progress);
    let job = Job::spawn(move |cancel| {
        review(&config, &prompt, &subject, cancel, mode, &steps).map(|reviewed| reviewed.answer)
    });
    (job, progress)
}

/// Runs the agent on `prompt` to implement `issue`, from `project`, on a
/// background thread, with the [`Progress`] its steps land in. What it does
/// is appended to `log`, under a heading. Dropping the job stops the agent.
pub fn start_build(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: &Path,
    log: &Path,
) -> (Job<Result<String, String>>, Progress) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let (issue, project, log) = (
        issue.to_path_buf(),
        project.to_path_buf(),
        log.to_path_buf(),
    );
    let progress = Progress::default();
    let steps = Arc::clone(&progress);
    let job = Job::spawn(move |cancel| {
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let started = std::time::Instant::now();
        let _ = build_log::append(&log, &build_log::heading(&at, &config.label()));
        let workspace = Workspace {
            dir: &project,
            log: Some(&log),
        };
        let (purpose, steps) = (Purpose::Build, Some(&steps));
        let result = run_with(
            &config,
            &prompt,
            &issue,
            cancel,
            purpose,
            steps,
            Some(&workspace),
        );
        let _ = build_log::append(&log, &build_log::outcome(&result, started.elapsed()));
        result
    });
    (job, progress)
}

/// What one streamed event says the agent is doing: a tool call from
/// OpenCode (`tool_use`) or Claude Code (`assistant` with a `tool_use` part).
pub(crate) fn step_of(line: &str) -> Option<String> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    match event["type"].as_str()? {
        "tool_use" => describe_tool(
            event["part"]["tool"].as_str()?,
            &event["part"]["state"]["input"],
        ),
        "assistant" => event["message"]["content"]
            .as_array()?
            .iter()
            .rev()
            .find(|part| part["type"] == "tool_use")
            .and_then(|part| describe_tool(part["name"].as_str()?, &part["input"])),
        _ => None,
    }
}

/// "Reading app/review.rs…": the tool as a verb, and the tail of its target.
pub(crate) fn describe_tool(tool: &str, input: &serde_json::Value) -> Option<String> {
    let verb = match tool.to_ascii_lowercase().as_str() {
        "read" => "Reading".to_string(),
        "grep" | "glob" | "list" | "ls" | "search" => "Searching".to_string(),
        "bash" => "Running".to_string(),
        "webfetch" | "fetch" | "websearch" => "Looking up".to_string(),
        _ => format!("Using {tool}"),
    };
    let target = ["file_path", "path", "pattern", "command", "url", "query"]
        .iter()
        .find_map(|key| input[*key].as_str());
    Some(match target {
        Some(target) => {
            let parts: Vec<&str> = target.trim_end_matches('/').rsplit('/').take(2).collect();
            let tail: Vec<&str> = parts.into_iter().rev().collect();
            format!("{verb} {}…", tail.join("/"))
        }
        None => format!("{verb}…"),
    })
}

/// The answer in a stream of events: Claude Code's `result`, else the text
/// of OpenCode's last step. Plain text that is no event stream stays as is.
fn final_answer(stdout: &str) -> String {
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if events.is_empty() {
        return stdout.to_string();
    }
    let mut texts: Vec<&str> = Vec::new();
    for event in &events {
        match event["type"].as_str() {
            Some("result") if event["result"].is_string() => {
                return event["result"].as_str().unwrap_or_default().to_string();
            }
            Some("step_start") => texts.clear(),
            Some("text") => texts.extend(event["part"]["text"].as_str()),
            _ => {}
        }
    }
    texts.concat()
}

/// The error an agent reported in its event stream, where its error output
/// stays empty: OpenCode's `error` event, or Claude Code's failed `result`.
fn stream_error(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|event| match event["type"].as_str()? {
            "error" => event["error"]["message"]
                .as_str()
                .or(event["error"]["data"]["message"].as_str())
                .or(event["error"].as_str())
                .or(event["message"].as_str())
                .map(str::to_string),
            "result" if event["is_error"] == true => event["result"]
                .as_str()
                .or(event["subtype"].as_str())
                .map(str::to_string),
            _ => None,
        })
        .map(|message| match message.strip_prefix('[') {
            Some(rest) => rest
                .split_once("] ")
                .map_or(message.clone(), |(_, text)| text.to_string()),
            None => message,
        })
        .next_back()
}

/// One ticket of a split: its `# ` title (labels included) and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub body: String,
}

/// The tickets of a split answer, each after a [`crate::prompts::SPLIT_MARK`]
/// line. `None` unless there are at least two, each with a title.
pub fn parse_split(text: &str) -> Option<Vec<Draft>> {
    let drafts = text
        .split(crate::prompts::SPLIT_MARK)
        .skip(1)
        .map(|chunk| {
            let lines: Vec<&str> = chunk.lines().collect();
            let at = crate::markdown::title_line(&lines)?;
            Some(Draft {
                title: lines[at].trim_start_matches('#').trim().to_string(),
                body: lines[at + 1..].join("\n").trim().to_string(),
            })
        })
        .collect::<Option<Vec<Draft>>>()?;
    (drafts.len() >= 2).then_some(drafts)
}

/// The verdict on one axis of the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Challenge,
    Block,
}

impl Verdict {
    fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "pass" => Some(Self::Pass),
            "challenge" => Some(Self::Challenge),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

/// One finding: an axis, a verdict and the agent's explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub axis: String,
    pub verdict: Verdict,
    pub body: String,
}

/// A parsed review: the three findings and, when the issue needed it, a
/// proposed rewrite. `proposal` is `None` for the word `NONE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub proposal: Option<String>,
}

/// Parses the agent's answer. Returns `None` when the answer does not carry the
/// three findings, so the UI can show it verbatim instead.
pub fn parse(text: &str) -> Option<Report> {
    const AXES: [&str; 3] = ["RELEVANCE", "SIZE", "CLARITY"];
    let mut findings: Vec<Finding> = Vec::new();
    let mut current: Option<(String, Verdict)> = None;
    let mut body = String::new();
    let mut proposal_lines: Vec<&str> = Vec::new();
    let mut in_proposal = false;

    let flush = |findings: &mut Vec<Finding>,
                 current: &mut Option<(String, Verdict)>,
                 body: &mut String| {
        if let Some((axis, verdict)) = current.take() {
            findings.push(Finding {
                axis,
                verdict,
                body: body.trim().to_string(),
            });
        }
        body.clear();
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if in_proposal {
            proposal_lines.push(line);
            continue;
        }
        if let Some((key, rest)) = key_and_rest(trimmed) {
            if key == "PROPOSAL" {
                flush(&mut findings, &mut current, &mut body);
                in_proposal = true;
                let rest = rest.trim();
                if !rest.is_empty() {
                    proposal_lines.push(rest);
                }
                continue;
            }
            if AXES.contains(&key.as_str())
                && let Some((verdict, note)) = verdict_and_note(rest)
            {
                flush(&mut findings, &mut current, &mut body);
                let axis = match key.as_str() {
                    "RELEVANCE" => "Relevance",
                    "SIZE" => "Size",
                    _ => "Clarity",
                };
                current = Some((axis.to_string(), verdict));
                if !note.is_empty() {
                    body.push_str(note);
                    body.push('\n');
                }
                continue;
            }
        }
        if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&mut findings, &mut current, &mut body);

    if findings.len() != AXES.len() {
        return None;
    }
    let proposal = proposal_of(&proposal_lines.join("\n"));
    Some(Report { findings, proposal })
}

/// A proposed rewrite, unless it is empty or the word `NONE`.
fn proposal_of(text: &str) -> Option<String> {
    let text = unfenced(text.trim());
    (!text.is_empty() && !text.eq_ignore_ascii_case("none")).then(|| text.to_string())
}

/// `text` without the one fence around all of it (```` ``` ````,
/// ```` ```markdown ```` or ```` ```md ````): agents sometimes wrap the whole
/// rewritten issue in one.
fn unfenced(text: &str) -> &str {
    let Some((first, rest)) = text.split_once('\n') else {
        return text;
    };
    let opening = first.trim().strip_prefix("```").map(str::trim);
    match (opening, rest.trim_end().strip_suffix("```")) {
        (Some("" | "markdown" | "md"), Some(inside)) => inside.trim(),
        _ => text,
    }
}

/// One turn of a refinement, as the challenger answered it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Challenge {
    /// The global advice in a line or two, after `TLDR:`.
    pub tldr: Option<String>,
    pub text: String,
    /// Answers the author may pick, listed after `OPTIONS:`.
    pub options: Vec<String>,
    /// The refined issue once it proposes one, after `PROPOSAL:`.
    pub proposal: Option<String>,
}

/// The challenger's turn: `TLDR:` up to the first blank line, the challenge,
/// `OPTIONS:` one per line, then `PROPOSAL:` to the end. Each part may be
/// missing.
pub fn parse_refinement(text: &str) -> Challenge {
    let lines: Vec<&str> = text.lines().collect();
    let mut challenge = Challenge::default();
    let mut end = lines.len();
    if let Some((at, rest)) = key_line(&lines, &["PROPOSAL"]) {
        challenge.proposal = proposal_of(&[&[rest][..], &lines[at + 1..]].concat().join("\n"));
        end = at;
    }
    if let Some((at, rest)) = key_line(&lines[..end], &["OPTIONS"]) {
        challenge.options = [&[rest][..], &lines[at + 1..end]]
            .concat()
            .iter()
            .map(|line| list_item(line))
            .filter(|option| !option.is_empty() && !option.eq_ignore_ascii_case("none"))
            .map(str::to_string)
            .collect();
        end = at;
    }
    let mut body = lines[..end].to_vec();
    if let Some((at, rest)) = key_line(&body, &["TLDR", "TL;DR"]) {
        let after = body[at + 1..]
            .iter()
            .position(|line| line.trim().is_empty())
            .map_or(body.len(), |blank| at + 1 + blank);
        let tldr = [&[rest][..], &body[at + 1..after]].concat().join(" ");
        challenge.tldr = Some(tldr.trim().to_string()).filter(|tldr| !tldr.is_empty());
        body.drain(at..after);
    }
    challenge.text = body.join("\n").trim().to_string();
    challenge
}

/// A suggested answer as a title and its detail: split at the first `:` or
/// ` — `, the title's emphasis dropped. Without either, it is all title.
pub fn choice_parts(option: &str) -> (String, Option<&str>) {
    let split = [": ", " — "]
        .iter()
        .filter_map(|sep| option.find(sep).map(|at| (at, sep.len())))
        .min();
    let (title, detail) = match split {
        Some((at, len)) => (&option[..at], Some(option[at + len..].trim())),
        None => (option, None),
    };
    let title = title.replace("**", "").replace('`', "");
    (title.trim().to_string(), detail.filter(|d| !d.is_empty()))
}

/// A list line without its bullet or number: `- Yes` and `2. Yes` give `Yes`.
fn list_item(line: &str) -> &str {
    let line = line.trim();
    if let Some(rest) = ["- ", "* ", "+ "]
        .iter()
        .find_map(|bullet| line.strip_prefix(bullet))
    {
        return rest.trim_start();
    }
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    match line[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => line,
    }
}

/// The first line of `lines` keyed by one of `keys`, and what follows its key.
fn key_line<'a>(lines: &[&'a str], keys: &[&str]) -> Option<(usize, &'a str)> {
    lines.iter().enumerate().find_map(|(at, line)| {
        key_and_rest(line.trim())
            .filter(|(key, _)| keys.contains(&key.as_str()))
            .map(|(_, rest)| (at, rest))
    })
}

/// A line's `KEY: rest`, the markdown around the key dropped: `**Size:** Pass`
/// and `## Size: Pass` both give `SIZE`. Keys are single words.
fn key_and_rest(line: &str) -> Option<(String, &str)> {
    let (head, rest) = line.split_once(':')?;
    let key = head.trim_matches(|c: char| c.is_whitespace() || "*_#-`".contains(c));
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || "*_`".contains(c));
    Some((key.to_ascii_uppercase(), rest))
}

/// The verdict a finding's line starts with, and what follows it:
/// `**Challenge** — too big` gives Challenge and `too big`. The prompt's own
/// `Pass|Challenge|Block` is a list of choices, not a verdict.
fn verdict_and_note(rest: &str) -> Option<(Verdict, &str)> {
    let end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    if rest[end..].trim_start().starts_with(['|', '/']) {
        return None;
    }
    let verdict = Verdict::parse(&rest[..end])?;
    let note = rest[end..]
        .trim_start_matches(|c: char| c.is_whitespace() || "*_`.,;:—–-".contains(c))
        .trim_end();
    Some((verdict, note))
}

mod direnv;

#[cfg(test)]
mod tests;

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
//! worktree = true          # run opencode in a git worktree, on by default
//! ```
//!
//! A `[build]` section, of the same shape, names the agent that implements
//! Ready issues; without one, the review's agent does. Its `timeout` limits
//! builds and asks: see [`BuildTimeout`]. Its `agents` is how many build
//! agents may work at once, which Settings › AI build and Doing's header set.
//!
//! `default` is the tool's first favorite model, so it follows the tool's own
//! settings. [`start`] runs the tool non-interactively against the issue text
//! and hands the answer back over a channel; the GUI polls it. A build runs in
//! its tool's background service, so it outlives the window. The agent only
//! ever returns text: writing the file stays the user's decision.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domains::agent::artifacts::build_log;
use crate::platform::files;
use crate::platform::job::{Cancel, Job};
use crate::platform::process;

/// What a [`AgentConfig::model`] of "the tool's first favorite" is written as.
const DEFAULT_MODEL: &str = "default";

/// How many build agents work at once unless the user says otherwise.
pub const DEFAULT_AGENTS: usize = 1;

/// The most build agents that may work at once.
pub const MAX_AGENTS: usize = 16;

/// The agents the settings offer, in the order they list them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Tool {
    #[default]
    OpenCode,
    Claude,
    /// Any command the user writes, with `{prompt}`, `{model}`, `{issue}`,
    /// `{mode}` and `{session}`.
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

    /// The background service this tool builds in; `None` for a custom
    /// command, which keeps the child process.
    pub fn service(self) -> Option<&'static dyn service::Service> {
        match self {
            Tool::OpenCode => Some(opencode::service()),
            Tool::Claude => Some(claude::service()),
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

/// Every tool's background service, for the work that asks them all.
pub fn services() -> impl Iterator<Item = &'static dyn service::Service> {
    Tool::ALL.into_iter().filter_map(Tool::service)
}

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
    /// Whether `opencode run` starts in a new git worktree. `None` is on.
    pub worktree: Option<bool>,
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

    /// Whether the tool can continue a conversation itself. A custom command
    /// does only when it takes `{session}`; otherwise the plan is replayed.
    pub fn continues(&self) -> bool {
        match self.tool {
            Tool::OpenCode | Tool::Claude => true,
            Tool::Custom => self.command.contains("{session}"),
        }
    }
}

/// The `[ai]` section as written on disk.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AiSection {
    #[serde(default)]
    tool: String,
    #[serde(default)]
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<bool>,
}

impl AiSection {
    /// The agent it names; unconfigured when there is none, or when its tool
    /// is not one iboard runs, which is reported.
    pub fn agent(self) -> (AgentConfig, Vec<String>) {
        let mut problems = Vec::new();
        let agent = parse_agent(
            &self.tool,
            &self.model,
            &self.command,
            &self.scope,
            self.worktree,
            "ai",
            &mut problems,
        )
        .unwrap_or_default();
        (agent, problems)
    }
}

/// The `[build]` section as written on disk.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BuildSection {
    #[serde(default)]
    tool: String,
    #[serde(default)]
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agents: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worktree: Option<bool>,
}

/// What `[build]` holds: its agent, when it names one, its timeout and its
/// agent count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSettings {
    pub agent: Option<AgentConfig>,
    pub timeout: BuildTimeout,
    pub agents: usize,
}

impl BuildSection {
    /// Reads the section; a tool, timeout or count iboard cannot use is
    /// reported and falls back on its default.
    pub fn settings(self) -> (BuildSettings, Vec<String>) {
        let mut problems = Vec::new();
        let agent = parse_agent(
            &self.tool,
            &self.model,
            &self.command,
            &self.scope,
            self.worktree,
            "build",
            &mut problems,
        );
        let timeout = match self.timeout {
            None => BuildTimeout::default(),
            Some(key) => BuildTimeout::from_key(key.trim()).unwrap_or_else(|| {
                problems.push(format!(
                    "[build]: timeout \"{key}\" is not 5m, 10m, 20m, 30m, 60m, 180m or none"
                ));
                BuildTimeout::default()
            }),
        };
        let agents = match self.agents {
            None => DEFAULT_AGENTS,
            Some(agents) if agents > MAX_AGENTS => {
                problems.push(format!(
                    "[build]: agents {agents} is more than {MAX_AGENTS}"
                ));
                MAX_AGENTS
            }
            Some(agents) => agents,
        };
        (
            BuildSettings {
                agent,
                timeout,
                agents,
            },
            problems,
        )
    }
}

fn parse_agent(
    tool: &str,
    model: &str,
    command: &str,
    scope: &str,
    worktree: Option<bool>,
    name: &str,
    problems: &mut Vec<String>,
) -> Option<AgentConfig> {
    let tool = tool.trim();
    if tool.is_empty() {
        return None;
    }
    let Some(tool) = Tool::from_key(tool) else {
        problems.push(format!(
            "[{name}]: tool \"{tool}\" is not opencode, claude or custom"
        ));
        return None;
    };
    let model = model.trim();
    Some(AgentConfig {
        tool,
        model: if model.eq_ignore_ascii_case(DEFAULT_MODEL) {
            String::new()
        } else {
            model.to_string()
        },
        command: command.to_string(),
        configured: true,
        scope: ReviewMode::from_scope_key(scope),
        worktree,
    })
}

fn model_key(model: &str) -> String {
    let model = model.trim();
    if model.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        model.to_string()
    }
}

fn command_key(config: &AgentConfig) -> String {
    if config.tool == Tool::Custom {
        config.command.clone()
    } else {
        String::new()
    }
}

/// The `[ai]` section that stores `config`.
pub fn ai_section(config: &AgentConfig) -> AiSection {
    AiSection {
        tool: config.tool.key().to_string(),
        model: model_key(&config.model),
        command: command_key(config),
        scope: config.scope.scope_key().to_string(),
        worktree: config.worktree,
    }
}

/// The `[build]` section that stores `config` and `timeout`.
pub fn build_section(config: &AgentConfig, timeout: BuildTimeout) -> BuildSection {
    BuildSection {
        tool: config.tool.key().to_string(),
        model: model_key(&config.model),
        command: command_key(config),
        scope: String::new(),
        timeout: Some(timeout.key().to_string()),
        agents: None,
        worktree: config.worktree,
    }
}

/// The `[build]` section's `agents`, alone.
#[derive(Debug, Serialize)]
pub struct AgentsSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    agents: Option<usize>,
}

/// The `[build]` key that stores `agents`, leaving its agent and timeout be.
pub fn agents_section(agents: usize) -> AgentsSection {
    AgentsSection {
        agents: Some(agents.min(MAX_AGENTS)),
    }
}

/// How long a review may take, Quick or Deep, the Challenger's rounds and a
/// split included: `design/components/Thinking`.
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// How long a new issue's draft may take.
const DRAFT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long a design mockup or a component's preview may take: unlike a
/// review, the agent writes a whole page.
pub const DESIGN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long one plan turn may take.
const PLAN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long a build or an ask may work: Settings › AI build's Timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildTimeout(Option<u64>);

impl Default for BuildTimeout {
    fn default() -> Self {
        BuildTimeout(Some(30))
    }
}

impl BuildTimeout {
    pub const NONE: BuildTimeout = BuildTimeout(None);

    /// What the select offers, in its order.
    pub const ALL: [BuildTimeout; 7] = [
        BuildTimeout(Some(5)),
        BuildTimeout(Some(10)),
        BuildTimeout(Some(20)),
        BuildTimeout(Some(30)),
        BuildTimeout(Some(60)),
        BuildTimeout(Some(180)),
        BuildTimeout::NONE,
    ];

    /// The limit it sets; `None` lets the agent work until it is stopped.
    pub fn limit(self) -> Option<Duration> {
        self.0.map(|minutes| Duration::from_secs(minutes * 60))
    }

    /// The name in the select.
    pub fn name(self) -> String {
        match self.0 {
            Some(minutes) => format!("{minutes} minutes"),
            None => "No limit".to_string(),
        }
    }

    /// The select's note beside the name.
    pub fn hint(self) -> Option<&'static str> {
        self.0.is_none().then_some("∞")
    }

    fn key(self) -> String {
        match self.0 {
            Some(minutes) => format!("{minutes}m"),
            None => "none".to_string(),
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|timeout| timeout.key() == key)
    }
}

/// The shell variables a custom command's placeholders become.
const PLACEHOLDERS: [(&str, &str); 5] = [
    ("{prompt}", "IBOARD_PROMPT"),
    ("{model}", "IBOARD_MODEL"),
    ("{issue}", "IBOARD_ISSUE"),
    ("{mode}", "IBOARD_MODE"),
    ("{session}", "IBOARD_SESSION"),
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

/// What Ask AI lets the agent do beyond reading the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leash {
    pub edits: bool,
    pub commits: bool,
    pub deploys: bool,
}

/// Commands that deploy, or reach a server: denied while deploying is not
/// allowed. A push can start a deploy, so it is one.
const DEPLOYING: [&str; 10] = [
    "Bash(git push:*)",
    "Bash(ssh:*)",
    "Bash(scp:*)",
    "Bash(rsync:*)",
    "Bash(ansible-playbook:*)",
    "Bash(kubectl:*)",
    "Bash(gcloud:*)",
    "Bash(docker push:*)",
    "Bash(vercel:*)",
    "Bash(fly:*)",
];

impl Leash {
    /// The Claude Code tools the leash denies, as `--disallowedTools` takes
    /// them.
    fn denied(self) -> Vec<&'static str> {
        let mut tools = Vec::new();
        if !self.commits {
            tools.extend(["Bash(git commit:*)", "Bash(git push:*)"]);
        }
        if !self.deploys {
            for tool in DEPLOYING {
                if !tools.contains(&tool) {
                    tools.push(tool);
                }
            }
        }
        tools
    }
}

/// What a run of the agent is for: a plain answer, a review (in plan mode
/// when `plan`), implementing an issue, which edits the repository, what its
/// author asked on a leash, or one turn of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Answer(Duration),
    Review {
        mode: ReviewMode,
        plan: bool,
    },
    Build(BuildTimeout),
    Ask(Leash, BuildTimeout),
    /// A new issue drafted from its author's notes, its text streamed.
    Draft,
    /// One turn of a plan, read-only; `tickets` asks for the split answer.
    Plan {
        plan_mode: bool,
        tickets: bool,
    },
}

impl Purpose {
    /// A custom command's `{mode}`.
    fn key(self) -> &'static str {
        match self {
            Purpose::Answer(_) => "",
            Purpose::Review { mode, .. } => mode.key(),
            Purpose::Build(_) => "build",
            Purpose::Ask(..) => "ask",
            Purpose::Draft => "draft",
            Purpose::Plan { tickets: true, .. } => "tickets",
            Purpose::Plan { .. } => "plan",
        }
    }

    /// Whether the run works on the repository.
    fn works(self) -> bool {
        matches!(self, Purpose::Build(_) | Purpose::Ask(..))
    }

    /// When the run is stopped; `None` when only the user stops it.
    fn timeout(self) -> Option<Duration> {
        match self {
            Purpose::Answer(timeout) => Some(timeout),
            Purpose::Review { .. } => Some(REVIEW_TIMEOUT),
            Purpose::Build(timeout) | Purpose::Ask(_, timeout) => timeout.limit(),
            Purpose::Draft => Some(DRAFT_TIMEOUT),
            Purpose::Plan { .. } => Some(PLAN_TIMEOUT),
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

/// Runs the agent on `prompt` and waits for its answer, on this thread. The
/// tool runs without a terminal, which is what one-shot modes need; it is
/// stopped when `cancel` says so, or after `timeout`. `issue` is the
/// file the prompt is about, a custom command's `{issue}`; the agent runs
/// from `project` when set.
pub fn run(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
    cancel: &Cancel,
    timeout: Duration,
) -> Result<String, String> {
    let workspace = project.map(Workspace::of);
    let purpose = Purpose::Answer(timeout);
    chat::run_with(
        config,
        prompt,
        issue,
        cancel,
        purpose,
        None,
        workspace.as_ref(),
    )
    .map(|chat| chat.answer)
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

/// The file a custom command reads `prompt` from: it may be too long for an
/// argument, and `{prompt}` can then go anywhere in the command.
fn prompt_file(config: &AgentConfig, prompt: &str) -> Result<Option<files::TempFile>, String> {
    if config.tool != Tool::Custom {
        return Ok(None);
    }
    files::TempFile::write("prompt", "md", prompt)
        .map(Some)
        .map_err(|err| format!("Couldn't write the prompt to the temp folder: {err}"))
}

/// [`run`] on a background thread. Dropping the job stops the agent.
pub fn start(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    project: Option<&Path>,
    timeout: Duration,
) -> Job<Result<String, String>> {
    let (config, prompt, issue) = (config.clone(), prompt.to_string(), issue.to_path_buf());
    let project = project.map(Path::to_path_buf);
    Job::spawn(move |cancel| {
        run(
            &config,
            &prompt,
            &issue,
            project.as_deref(),
            cancel,
            timeout,
        )
    })
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
        chat::run_with(
            config,
            prompt,
            issue,
            cancel,
            purpose,
            steps,
            workspace.as_ref(),
        )
        .map(|chat| Reviewed {
            answer: chat.answer,
            plan,
        })
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

/// [`review`] on a background thread, with the [`Progress`] its steps land in.
/// Dropping the job stops the agent.
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
    timeout: BuildTimeout,
) -> (Job<Result<String, String>>, Progress, service::Handle) {
    let purpose = Purpose::Build(timeout);
    start_logged(config, prompt, issue, (project, log), purpose)
}

/// [`start_build`] for what the author of `issue` asked, within `leash`.
pub fn start_ask(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    (project, log): (&Path, &Path),
    leash: Leash,
    timeout: BuildTimeout,
) -> (Job<Result<String, String>>, Progress) {
    let purpose = Purpose::Ask(leash, timeout);
    let (job, progress, _) = start_logged(config, prompt, issue, (project, log), purpose);
    (job, progress)
}

fn start_logged(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    (project, log): (&Path, &Path),
    purpose: Purpose,
) -> (Job<Result<String, String>>, Progress, service::Handle) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let (issue, project, log) = (
        issue.to_path_buf(),
        project.to_path_buf(),
        log.to_path_buf(),
    );
    let progress = Progress::default();
    let steps = Arc::clone(&progress);
    let handle = service::Handle::new(config.tool.service());
    let mine = handle.clone();
    let job = Job::spawn(move |cancel| {
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let started = std::time::Instant::now();
        let _ = build_log::append(&log, &build_log::heading(&at, &config.label()));
        let workspace = Workspace {
            dir: &project,
            log: Some(&log),
        };
        let steps = Some(&steps);
        let child = |purpose| match chat::run_with(
            &config,
            &prompt,
            &issue,
            cancel,
            purpose,
            steps,
            Some(&workspace),
        ) {
            Ok(chat) => (Ok(chat.answer), chat.usage),
            Err(error) => (Err(error), None),
        };
        let (result, usage) = match purpose {
            // A build runs in the service, so it outlives the window.
            Purpose::Build(_) if mine.service().is_some() => {
                let build = service::Build {
                    model: &model_of(&config),
                    prompt: &prompt,
                    issue: &issue,
                    project: &project,
                    log: &log,
                    timeout: purpose.timeout(),
                };
                match service::start(&build, cancel, steps, &mine) {
                    Ok(finished) => (Ok(finished.answer), finished.usage),
                    Err(error) if config.tool == Tool::Claude && claude::unsupported(&error) => {
                        child(purpose)
                    }
                    Err(error) => (Err(error), None),
                }
            }
            purpose => child(purpose),
        };
        let _ = build_log::append(
            &log,
            &build_log::outcome(&result, started.elapsed(), usage.as_ref()),
        );
        result
    });
    (job, progress, handle)
}

/// The model a run asks for: the configured one, or the tool's own default.
fn model_of(config: &AgentConfig) -> String {
    match config.model.trim() {
        "" => default_model(config.tool).unwrap_or_default(),
        model => model.to_string(),
    }
}

mod answer;
pub mod build;
mod chat;
mod claude;
mod command;
use command::command;
mod direnv;
mod draft;
mod opencode;
mod poller;
mod reply;
pub mod service;
pub mod spend;
mod stream;
mod tools;
mod transcript;
mod usage;

pub(crate) use stream::{describe_tool, session_of, step_of};
use stream::{final_answer, stream_error};

pub use chat::{Chat, start_plan};

pub use answer::{
    Axis, Challenge, Draft, Finding, Report, Verdict, choice_parts, parse, parse_refinement,
    parse_split,
};
pub use draft::{DraftText, draft_so_far, start_draft};
pub use tools::{Detected, Model, Models, detect, list_models};
use tools::{default_model, find_binary, search_path};
pub use usage::{
    Billing, Cap, Plan, Plans, Provider, Tokens, Usage, UsageSection, Window, provider_key,
    provider_name, usage_section,
};

#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod tests;

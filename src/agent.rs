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
//! Ready issues; without one, the review's agent does. Its `timeout` limits
//! builds and asks: see [`BuildTimeout`].
//!
//! `default` is the tool's first favorite model, so it follows the tool's own
//! settings. [`start`] runs the tool non-interactively against the issue text
//! and hands the answer back over a channel; the GUI polls it. The agent only
//! ever returns text: writing the file stays the user's decision.

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
use crate::settings::{Section, UserConfig};

/// What a [`AgentConfig::model`] of "the tool's first favorite" is written as.
const DEFAULT_MODEL: &str = "default";

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
            timeout: String::new(),
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    timeout: String,
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

/// The `timeout` of the `[build]` section; an unknown one is reported.
pub fn load_build_timeout(config: &mut UserConfig) -> BuildTimeout {
    let raw: RawBuildTimeout = config.section("build");
    let Some(key) = raw.timeout else {
        return BuildTimeout::default();
    };
    BuildTimeout::from_key(key.trim()).unwrap_or_else(|| {
        config.problem(format!(
            "[build]: timeout \"{key}\" is not 5m, 10m, 20m, 30m, 60m, 180m or none"
        ));
        BuildTimeout::default()
    })
}

#[derive(Default, Deserialize)]
struct RawBuildTimeout {
    timeout: Option<String>,
}

/// The `[build]` section that stores `config` and `timeout`.
pub fn build_section(config: &AgentConfig, timeout: BuildTimeout) -> Result<Section, String> {
    let raw = RawAgent {
        scope: String::new(),
        timeout: timeout.key(),
        ..config.to_raw()
    };
    Section::new("build", &raw)
}

/// How long a review may take, Quick or Deep, the Challenger's rounds and a
/// split included: `design/components/Thinking`.
const REVIEW_TIMEOUT: Duration = Duration::from_secs(120);

/// How long a new issue's draft may take.
const DRAFT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long a design mockup or a component's preview may take: unlike a
/// review, the agent writes a whole page.
pub const DESIGN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long a build or an ask may work: Settings › AI build's Timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildTimeout(Option<u64>);

impl Default for BuildTimeout {
    fn default() -> Self {
        BuildTimeout(Some(10))
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
/// when `plan`), implementing an issue, which edits the repository, or what
/// its author asked on a leash.
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
                (_, Purpose::Answer(_)) => {}
                (Tool::OpenCode, _) => {
                    command.args(["--format", "json"]);
                }
                (_, _) => {
                    command.args(["--output-format", "stream-json", "--verbose"]);
                    match purpose {
                        Purpose::Draft => {
                            command.args(["--include-partial-messages", "--tools", ""]);
                        }
                        Purpose::Review {
                            mode: ReviewMode::Quick,
                            ..
                        } => {
                            command.args(["--tools", ""]);
                        }
                        Purpose::Build(_) | Purpose::Ask(Leash { edits: true, .. }, _) => {
                            command.args(["--permission-mode", "acceptEdits"]);
                        }
                        _ => {}
                    }
                }
            }
            if let Purpose::Review { plan: true, .. }
            | Purpose::Ask(Leash { edits: false, .. }, _) = purpose
            {
                command.args(tool.plan_args());
            }
            if let (Tool::Claude, Purpose::Ask(leash, _)) = (tool, purpose) {
                let denied = leash.denied();
                if !denied.is_empty() {
                    command.arg("--disallowedTools").arg(denied.join(","));
                }
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
    let streams = progress.filter(|_| !custom);
    let output = process::run_with(command, what, cancel, purpose.timeout(), |line| {
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
        "" if !purpose.works() => {
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
) -> (Job<Result<String, String>>, Progress) {
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
    start_logged(config, prompt, issue, (project, log), purpose)
}

fn start_logged(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    (project, log): (&Path, &Path),
    purpose: Purpose,
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
        let steps = Some(&steps);
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

mod answer;
mod direnv;
mod draft;
mod tools;

pub use answer::{
    Axis, Challenge, Draft, Finding, Report, Verdict, choice_parts, parse, parse_refinement,
    parse_split,
};
pub use draft::{DraftText, start_draft};
pub use tools::{Detected, Model, Models, detect, list_models};
use tools::{default_model, find_binary, search_path};

#[cfg(test)]
mod tests;

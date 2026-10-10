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
//! ```
//!
//! `default` is the tool's first favorite model, so it follows the tool's own
//! settings. [`start`] runs the tool non-interactively against the issue text
//! and hands the answer back over a channel; the GUI polls it. The agent only
//! ever returns text: writing the file stays the user's decision.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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

    fn from_raw(raw: RawAgent) -> Self {
        let model = raw.model.trim();
        let model = if model.eq_ignore_ascii_case(DEFAULT_MODEL) {
            String::new()
        } else {
            model.to_string()
        };
        let tool = raw.tool.trim();
        if tool.is_empty() {
            return Self::default();
        }
        if let Some(known) = Tool::from_key(tool) {
            return Self {
                tool: known,
                model,
                command: raw.command,
                configured: true,
            };
        }
        // Before the settings dialog, `tool` held a program name or path.
        let stem = Path::new(tool)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(tool);
        if let Some(known) = Tool::from_key(stem).filter(|known| *known != Tool::Custom) {
            return Self {
                tool: known,
                model,
                command: String::new(),
                configured: true,
            };
        }
        let model_flag = if model.is_empty() {
            ""
        } else {
            "--model {model} "
        };
        Self {
            tool: Tool::Custom,
            command: format!("{} {model_flag}\"$(cat {{prompt}})\"", shell_quote(tool)),
            model,
            configured: true,
        }
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
}

/// The agent in the `[ai]` section; unconfigured when there is none.
pub fn load(config: &mut UserConfig) -> AgentConfig {
    AgentConfig::from_raw(config.section("ai"))
}

/// The `[ai]` section that stores `config`.
pub fn section(config: &AgentConfig) -> Result<Section, String> {
    Section::new("ai", &config.to_raw())
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
pub fn find_binary(name: &str) -> Option<PathBuf> {
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
pub fn default_model(tool: Tool) -> Option<String> {
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

/// Quotes `text` for `sh`, so a path or model with spaces stays one word.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// How long an agent may work on an answer before it is stopped.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// How long `--version` and `opencode models` may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// The shell variables a custom command's placeholders become.
const PLACEHOLDERS: [(&str, &str); 3] = [
    ("{prompt}", "IBOARD_PROMPT"),
    ("{model}", "IBOARD_MODEL"),
    ("{issue}", "IBOARD_ISSUE"),
];

/// Builds the command for `config`, ready to spawn. `model` is the resolved
/// one (empty for the tool's own choice); `prompt_file` holds `prompt` for a
/// custom command. Factored out so the argument shape per tool is testable
/// without running anything.
pub fn command(
    config: &AgentConfig,
    model: &str,
    prompt: &str,
    prompt_file: &Path,
    issue: &Path,
) -> Command {
    let mut command = match config.tool {
        Tool::Custom => {
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .arg(shell_script(&config.command))
                .env("IBOARD_PROMPT", prompt_file)
                .env("IBOARD_MODEL", model)
                .env("IBOARD_ISSUE", issue);
            command
        }
        tool => {
            let name = tool.binary().unwrap_or_default();
            let mut command =
                Command::new(find_binary(name).unwrap_or_else(|| PathBuf::from(name)));
            // `opencode run "…"` and `claude -p "…"` answer once and exit.
            command.arg(if tool == Tool::OpenCode { "run" } else { "-p" });
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
/// file the prompt is about, a custom command's `{issue}`.
pub fn run(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    cancel: &Cancel,
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
    let command = command(config, &model, prompt, &prompt_file, issue);
    let output = process::run(command, what, cancel, ANSWER_TIMEOUT);
    if custom {
        let _ = std::fs::remove_file(&prompt_file);
    }
    let answer = output?.into_result("the agent")?;
    match answer.trim() {
        "" => Err("the agent returned no answer".to_string()),
        answer => Ok(answer.to_string()),
    }
}

/// [`run`] on a background thread. Dropping the job stops the agent.
pub fn start(config: &AgentConfig, prompt: &str, issue: &Path) -> Job<Result<String, String>> {
    let (config, prompt, issue) = (config.clone(), prompt.to_string(), issue.to_path_buf());
    Job::spawn(move |cancel| run(&config, &prompt, &issue, cancel))
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
        if let Some((head, rest)) = trimmed.split_once(':') {
            let key = head.trim().to_ascii_uppercase();
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
                && let Some(verdict) = Verdict::parse(rest)
            {
                flush(&mut findings, &mut current, &mut body);
                let axis = match key.as_str() {
                    "RELEVANCE" => "Relevance",
                    "SIZE" => "Size",
                    _ => "Clarity",
                };
                current = Some((axis.to_string(), verdict));
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
    let proposal = proposal_lines.join("\n").trim().to_string();
    let proposal = if proposal.is_empty() || proposal.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(proposal)
    };
    Some(Report { findings, proposal })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(command: &str) -> AgentConfig {
        AgentConfig {
            tool: Tool::Custom,
            model: String::new(),
            command: command.into(),
            configured: true,
        }
    }

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_custom_command_reads_the_prompt_file_and_answers() {
        let job = start(
            &custom("cat {prompt}"),
            "hello there",
            Path::new("todo/a.md"),
        );
        let answer = job.wait().expect("the job answers").expect("cat succeeds");
        assert_eq!(answer, "hello there");
    }

    #[test]
    fn a_failing_command_reports_its_error_instead_of_panicking() {
        let config = custom("echo nope >&2; exit 3 # {prompt}");
        let error = run(&config, "x", Path::new("a.md"), &Cancel::default());
        assert_eq!(error, Err("nope".to_string()));
    }

    #[test]
    fn file_names_never_run_as_shell_code() {
        let dir = std::env::temp_dir().join(format!("iboard-quoting-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let planted = dir.join("planted");
        let issue = dir.join(format!(
            "$(touch {}) `touch {}`.md",
            planted.display(),
            planted.display()
        ));
        for template in [
            "printf %s {issue} # {prompt}",
            "printf %s \"{issue}\" # {prompt}",
            "printf %s '{issue}' # {prompt}",
        ] {
            let answer = run(&custom(template), "x", &issue, &Cancel::default());
            assert_eq!(answer, Ok(issue.display().to_string()), "{template}");
        }
        assert!(!planted.exists(), "a file name ran as code");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn dropping_a_run_stops_the_agent() {
        // SAFETY: signal 0 only checks that the process exists.
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) == 0 };
        let eventually = |done: &dyn Fn() -> bool| {
            let start = std::time::Instant::now();
            while !done() && start.elapsed() < std::time::Duration::from_secs(5) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            done()
        };
        let dir = std::env::temp_dir().join(format!("iboard-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_file = dir.join("agent.pid");
        let script = format!(
            "echo $$ > '{}'; sleep 30; echo late # {{prompt}}",
            pid_file.display()
        );
        let job = start(&custom(&script), "x", Path::new("a.md"));
        let pid = || {
            std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
        };
        assert!(eventually(&|| pid().is_some()), "the agent never started");
        let pid = pid().unwrap();
        assert!(alive(pid));

        drop(job);

        assert!(eventually(&|| !alive(pid)), "the agent outlived its job");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn each_tool_gets_its_one_shot_arguments() {
        let file = Path::new("/tmp/prompt.md");
        let issue = Path::new("/p/todo/a b.md");
        let opencode = AgentConfig {
            tool: Tool::OpenCode,
            ..AgentConfig::default()
        };
        let built = command(&opencode, "vendor/model", "PROMPT", file, issue);
        assert_eq!(args(&built), ["run", "--model", "vendor/model", "PROMPT"]);

        let claude = AgentConfig {
            tool: Tool::Claude,
            ..AgentConfig::default()
        };
        let built = command(&claude, "", "PROMPT", file, issue);
        assert_eq!(args(&built), ["-p", "PROMPT"]);

        let built = command(
            &custom("llm -m {model} < {prompt} # {issue}"),
            "gpt",
            "PROMPT",
            file,
            issue,
        );
        assert_eq!(built.get_program(), "sh");
        assert_eq!(
            args(&built),
            [
                "-c",
                "llm -m \"$IBOARD_MODEL\" < \"$IBOARD_PROMPT\" # \"$IBOARD_ISSUE\""
            ]
        );
        let env = |name: &str| {
            built
                .get_envs()
                .find(|(key, _)| *key == name)
                .and_then(|(_, value)| value)
                .map(|value| value.to_string_lossy().into_owned())
        };
        assert_eq!(env("IBOARD_MODEL").as_deref(), Some("gpt"));
        assert_eq!(env("IBOARD_PROMPT").as_deref(), Some("/tmp/prompt.md"));
        assert_eq!(env("IBOARD_ISSUE").as_deref(), Some("/p/todo/a b.md"));
    }

    #[test]
    fn a_custom_command_without_prompt_does_not_validate() {
        assert!(custom("llm -m {model}").validate().is_err());
        assert!(custom("llm < {prompt}").validate().is_ok());
        assert!(AgentConfig::default().validate().is_ok());
    }

    #[test]
    fn the_label_names_the_tool_and_model() {
        let mut config = AgentConfig {
            tool: Tool::OpenCode,
            ..AgentConfig::default()
        };
        assert_eq!(config.label(), "opencode · default");
        config.model = "vendor/model".into();
        assert_eq!(config.label(), "opencode · vendor/model");
    }

    #[test]
    fn an_older_configuration_still_loads() {
        let load = |tool: &str, model: &str| {
            AgentConfig::from_raw(RawAgent {
                tool: tool.into(),
                model: model.into(),
                command: String::new(),
            })
        };
        let claude = load("/usr/local/bin/claude", "");
        assert_eq!(claude.tool, Tool::Claude);
        assert!(claude.configured());

        let other = load("my-agent", "fast");
        assert_eq!(other.tool, Tool::Custom);
        assert_eq!(
            other.command,
            "'my-agent' --model {model} \"$(cat {prompt})\""
        );
        assert_eq!(other.model, "fast");
        assert!(other.validate().is_ok());

        assert!(!load("", "").configured());
    }

    #[test]
    fn versions_and_model_lists_parse() {
        assert_eq!(
            parse_version("opencode v2.0.20\n").as_deref(),
            Some("v2.0.20")
        );
        assert_eq!(
            parse_version("2.1.287 (Claude Code)").as_deref(),
            Some("v2.1.287")
        );
        assert_eq!(parse_version("no version here"), None);

        assert_eq!(
            parse_model_list("Models available:\nanthropic/a\n\n  openai/b \n"),
            ["anthropic/a", "openai/b"]
        );
        assert_eq!(
            parse_opencode_favorites(
                r#"{"recent":[],"favorite":[{"providerID":"deepseek","modelID":"flash"}]}"#
            ),
            ["deepseek/flash"]
        );
        assert!(parse_opencode_favorites("not json").is_empty());
        assert_eq!(
            parse_claude_model(r#"{"model": "opus"}"#).as_deref(),
            Some("opus")
        );
        assert_eq!(parse_claude_model("{}"), None);
    }

    #[test]
    fn a_full_answer_parses_into_three_findings_and_a_proposal() {
        let answer = "\
RELEVANCE: Pass
Still needed, nothing else covers it.
SIZE: Challenge
Two features in one. Split them.
CLARITY: Block
An implementer would ask which tool runs it.
PROPOSAL:
# Better issue

Some context.";
        let report = parse(answer).expect("the shape is complete");
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.findings[0].axis, "Relevance");
        assert_eq!(report.findings[0].verdict, Verdict::Pass);
        assert_eq!(report.findings[1].verdict, Verdict::Challenge);
        assert_eq!(report.findings[2].verdict, Verdict::Block);
        assert!(report.findings[2].body.contains("which tool"));
        assert!(
            report
                .proposal
                .as_deref()
                .unwrap()
                .starts_with("# Better issue")
        );
    }

    #[test]
    fn a_none_proposal_is_dropped() {
        let answer = "RELEVANCE: Pass\nok\nSIZE: Pass\nok\nCLARITY: Pass\nok\nPROPOSAL:\nnone";
        let report = parse(answer).unwrap();
        assert!(report.proposal.is_none());
    }

    #[test]
    fn free_form_answers_do_not_parse() {
        assert!(parse("This is just prose the agent wrote.").is_none());
        let partial = "RELEVANCE: Pass\nok\nSIZE: Pass\nok";
        assert!(parse(partial).is_none());
    }

    #[test]
    fn the_configuration_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "iboard-agent-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("iboard.toml");
        std::fs::write(&file, "[other]\nkey = 1\n").unwrap();

        let config = AgentConfig {
            tool: Tool::OpenCode,
            model: "vendor/model".into(),
            command: String::new(),
            configured: true,
        };
        settings::save(&file, &[section(&config).unwrap()]).unwrap();
        assert_eq!(load(&mut UserConfig::load(Some(&file))), config);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[other]"), "the other section survives");
        assert!(text.contains("tool = \"opencode\""));

        // A missing file is simply an unconfigured agent.
        let absent = dir.join("absent.toml");
        assert!(!load(&mut UserConfig::load(Some(&absent))).configured());

        std::fs::remove_dir_all(&dir).ok();
    }
}

//! Claude Code's background service: builds started with `claude --bg`,
//! watched through `claude agents --json` and stopped with `claude stop`.
//!
//! Set `IBOARD_CLAUDE` to another binary to test this without one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use super::service::{Answer, Message, Part, Question, Service, Session, Status, now};
use super::{Billing, Tokens, Usage, usage};
use crate::domains::agent::artifacts::reviews;
use crate::platform::job::Cancel;
use crate::platform::process;

/// How long one call to the service may take.
const CALL: Duration = Duration::from_secs(20);

/// The mark iboard puts on the sessions it starts, before their issue's slug.
const MARK: &str = "iboard-";

/// The states in which a session is still at work.
const RUNNING: [&str; 2] = ["working", "blocked"];

/// Claude Code, the second tool to build in a background service.
pub(super) fn service() -> &'static dyn Service {
    &CLAUDE
}

static CLAUDE: Claude = Claude;

struct Claude;

impl Service for Claude {
    fn key(&self) -> &'static str {
        "claude"
    }

    fn start(
        &self,
        issue: &Path,
        project: &Path,
        model: &str,
        prompt: &str,
    ) -> Result<String, String> {
        let binary = binary().ok_or_else(|| "claude is not installed".to_string())?;
        let command = start_command(&binary, issue, project, model, prompt);
        let stdout = process::run(command, "claude --bg", &Cancel::default(), CALL)?
            .into_result("claude --bg")?;
        short_id(&stdout).ok_or_else(|| "claude --bg is not supported here".to_string())
    }

    fn sessions(&self, project: &Path) -> Result<Vec<Session>, String> {
        let project = canonical(project);
        let cwd = project.to_string_lossy().into_owned();
        let stdout = call(
            &project,
            "claude agents",
            &["agents", "--json", "--all", "--cwd", &cwd],
        )?;
        let listed: Vec<Value> =
            serde_json::from_str(&stdout).map_err(|err| format!("claude agents: {err}"))?;
        Ok(sessions_of(&listed, &project, config_dir().as_deref()))
    }

    fn read(&self, project: &Path, session: &str) -> Result<Vec<Message>, String> {
        let config =
            config_dir().ok_or_else(|| "Claude Code's config folder is missing".to_string())?;
        let job = config.join("jobs").join(session);
        let state = read_json(&job.join("state.json"));
        if let Some(messages) = transcript(&config, state.as_ref())
            && !messages.is_empty()
        {
            return Ok(messages);
        }
        let messages = timeline(&job.join("timeline.jsonl"));
        if !messages.is_empty() {
            return Ok(messages);
        }
        logs(project, session)
    }

    fn say(&self, _project: &Path, _session: &str, _text: &str) -> Result<(), String> {
        Err("A Claude build cannot take a reply while it runs in the background".to_string())
    }

    fn interrupt(&self, project: &Path, session: &str) {
        let _ = call(project, "claude stop", &["stop", session]);
    }

    fn questions(&self, _project: &Path) -> Result<Vec<Question>, String> {
        Ok(Vec::new())
    }

    fn answer(
        &self,
        _project: &Path,
        _session: &str,
        _id: &str,
        _answer: &Answer,
    ) -> Result<(), String> {
        Err("A Claude build is answered in `claude attach`, not from the board".to_string())
    }
}

/// The command that starts `issue`'s build in the background.
fn start_command(
    binary: &Path,
    issue: &Path,
    project: &Path,
    model: &str,
    prompt: &str,
) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(project)
        .arg("--bg")
        .arg(prompt)
        .arg("--name")
        .arg(name(issue))
        .args(["--permission-mode", "acceptEdits"]);
    if !model.trim().is_empty() {
        command.args(["--model", model]);
    }
    command.env("PATH", super::search_path());
    command
}

/// Runs one `claude` subcommand from `project`.
fn call(project: &Path, what: &str, args: &[&str]) -> Result<String, String> {
    let binary = binary().ok_or_else(|| "claude is not installed".to_string())?;
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(project)
        .env("PATH", super::search_path());
    process::run(command, what, &Cancel::default(), CALL)?.into_result(what)
}

/// The `claude` to call: the tests' fake, or the real one outside them.
fn binary() -> Option<PathBuf> {
    std::env::var_os("IBOARD_CLAUDE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let path = super::Tool::Claude.program();
            path.is_file().then_some(path)
        })
}

/// The name iboard gives `issue`'s session: an `iboard` mark and its slug.
fn name(issue: &Path) -> String {
    format!("{MARK}{}", reviews::slug(issue))
}

/// The short id `claude --bg` prints, as `stop`, `logs` and `attach` take it.
fn short_id(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.split_once("backgrounded")?.1;
        rest.split('·')
            .nth(1)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
    })
}

/// The iboard sessions in what `agents --json` listed, with what the finished
/// ones spent in their transcript.
fn sessions_of(listed: &[Value], project: &Path, config: Option<&Path>) -> Vec<Session> {
    listed
        .iter()
        .filter_map(|session| {
            let mut found = session_of(session, project)?;
            found.usage = config.and_then(|config| usage_of(config, &found.id));
            Some(found)
        })
        .collect()
}

/// One background session of `project`, if that is what `session` is.
fn session_of(session: &Value, project: &Path) -> Option<Session> {
    if session["kind"].as_str() != Some("background") {
        return None;
    }
    let slug = session["name"]
        .as_str()?
        .strip_prefix(MARK)
        .filter(|slug| !slug.is_empty())?;
    let cwd = session["cwd"].as_str()?;
    if !Path::new(cwd).starts_with(project) {
        return None;
    }
    let status = status_of(session);
    Some(Session {
        id: session["id"].as_str()?.to_string(),
        slug: Some(slug.to_string()),
        status,
        ended: status.updated,
        usage: None,
    })
}

/// How a background session stands, as `agents --json` keeps it.
fn status_of(session: &Value) -> Status {
    let created = session["startedAt"].as_u64().unwrap_or(0);
    let running = session["state"]
        .as_str()
        .is_some_and(|state| RUNNING.contains(&state));
    let updated = if running { now().max(created) } else { created };
    Status {
        running,
        created,
        updated,
    }
}

/// What `session` spent, from its job's transcript: its `cost-state` when the
/// run left one, else its assistant records.
fn usage_of(config: &Path, session: &str) -> Option<Usage> {
    let state = read_json(&config.join("jobs").join(session).join("state.json"));
    let path = transcript_path(config, state.as_ref())?;
    let text = std::fs::read_to_string(path).ok()?;
    spend_of(&text)
}

/// What a background transcript spent. A finished one carries a `cost-state`
/// record with its totals; the assistant records are the fallback.
fn spend_of(text: &str) -> Option<Usage> {
    let mut seen = HashSet::new();
    let mut messages = Tokens::default();
    let mut model = None;
    let mut said_anything = false;
    let mut totals = None;
    for (at, line) in text.lines().enumerate() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match event["type"].as_str() {
            Some("assistant") => {
                let message = &event["message"];
                let id = event["uuid"]
                    .as_str()
                    .or_else(|| message["id"].as_str())
                    .map_or_else(|| format!("line-{at}"), str::to_string);
                if !seen.insert(id) {
                    continue;
                }
                let usage = &message["usage"];
                if usage.is_null() {
                    continue;
                }
                said_anything = true;
                messages.input += usage["input_tokens"].as_u64().unwrap_or(0);
                messages.output += usage["output_tokens"].as_u64().unwrap_or(0);
                messages.reasoning += usage["output_tokens_details"]["thinking_tokens"]
                    .as_u64()
                    .unwrap_or(0);
                messages.cache_read += usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
                messages.cache_write += usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                let said = usage::text(&message["model"]);
                if said.as_deref() != Some("<synthetic>") {
                    model = said.or(model);
                }
            }
            Some("cost-state") => {
                let (tokens, main) = model_usage_of(&event["modelUsage"]);
                let cost = event["totalCostUSD"].as_f64();
                if tokens != Tokens::default() || cost.is_some() {
                    totals = Some((tokens, main, cost));
                }
            }
            _ => {}
        }
    }
    let (tokens, model, cost) = match totals {
        Some((tokens, main, cost)) => (tokens, main.or(model), cost),
        None if said_anything => (messages, model, None),
        None => return None,
    };
    Some(Usage {
        cost,
        tokens,
        provider: Some("claude".to_string()),
        model,
        billing: Billing::Subscription,
    })
}

/// The tokens and main model in a `cost-state`'s `modelUsage`.
fn model_usage_of(value: &Value) -> (Tokens, Option<String>) {
    let Some(models) = value.as_object() else {
        return (Tokens::default(), None);
    };
    let mut tokens = Tokens::default();
    let mut main: Option<(&str, u64)> = None;
    for (name, usage) in models {
        tokens.input += usage["inputTokens"].as_u64().unwrap_or(0);
        tokens.output += usage["outputTokens"].as_u64().unwrap_or(0);
        tokens.reasoning += usage["thinkingTokens"].as_u64().unwrap_or(0);
        tokens.cache_read += usage["cacheReadInputTokens"].as_u64().unwrap_or(0);
        tokens.cache_write += usage["cacheCreationInputTokens"].as_u64().unwrap_or(0);
        let output = usage["outputTokens"].as_u64().unwrap_or(0);
        if main.is_none_or(|(_, best)| output > best) {
            main = Some((name, output));
        }
    }
    (tokens, main.map(|(name, _)| name.to_string()))
}

/// Claude Code's configuration folder, where its jobs and transcripts live.
fn config_dir() -> Option<PathBuf> {
    crate::platform::files::dir_from_env("CLAUDE_CONFIG_DIR", ".claude")
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The messages of a background job's transcript, when it can be found.
fn transcript(config: &Path, state: Option<&Value>) -> Option<Vec<Message>> {
    let text = std::fs::read_to_string(transcript_path(config, state)?).ok()?;
    Some(messages_of(&text))
}

/// Where a background job's transcript lives: the worktree it moved to, or
/// the folder it started in.
fn transcript_path(config: &Path, state: Option<&Value>) -> Option<PathBuf> {
    let state = state?;
    let session = state["sessionId"].as_str()?;
    let cwd = state["worktreePath"]
        .as_str()
        .or_else(|| state["cwd"].as_str())?;
    let folder = cwd.replace('/', "-");
    Some(
        config
            .join("projects")
            .join(folder)
            .join(format!("{session}.jsonl")),
    )
}

/// The assistant messages of one transcript file, oldest first.
fn messages_of(text: &str) -> Vec<Message> {
    let mut seen = HashSet::new();
    let mut messages = Vec::new();
    for (at, line) in text.lines().enumerate() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["type"] != "assistant" {
            continue;
        }
        let message = &event["message"];
        let id = event["uuid"]
            .as_str()
            .or_else(|| message["id"].as_str())
            .map_or_else(|| format!("line-{at}"), str::to_string);
        if !seen.insert(id.clone()) {
            continue;
        }
        let parts: Vec<Part> = message["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(part_of)
            .collect();
        if !parts.is_empty() {
            messages.push(Message {
                id,
                closed: true,
                parts,
            });
        }
    }
    messages
}

fn part_of(value: &Value) -> Option<Part> {
    match value["type"].as_str()? {
        "text" => Some(Part::Text(
            value["text"].as_str().unwrap_or_default().to_string(),
        )),
        "tool_use" => Some(Part::Tool {
            name: value["name"].as_str().unwrap_or("tool").to_string(),
            input: value["input"].clone(),
            streaming: false,
        }),
        _ => None,
    }
}

/// The answer a job's timeline recorded, when its transcript is gone.
fn timeline(path: &Path) -> Vec<Message> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .enumerate()
        .filter_map(|(at, line)| {
            let event: Value = serde_json::from_str(line).ok()?;
            let said = event["text"]
                .as_str()
                .filter(|text| !text.trim().is_empty())?;
            Some(Message {
                id: format!("timeline-{at}"),
                closed: true,
                parts: vec![Part::Text(said.to_string())],
            })
        })
        .collect()
}

/// The session's output when neither of Claude's own files could be read.
fn logs(project: &Path, session: &str) -> Result<Vec<Message>, String> {
    let text = call(project, "claude logs", &["logs", session])?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Message {
        id: format!("logs-{session}"),
        closed: true,
        parts: vec![Part::Text(text.to_string())],
    }])
}

/// Whether `error` says this Claude Code has no background sessions.
pub(super) fn unsupported(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    let unknown = [
        "unknown",
        "unexpected",
        "unrecognized",
        "not supported",
        "does not support",
        "invalid",
        "no such",
    ];
    error.contains("--bg") && unknown.iter().any(|sign| error.contains(sign))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_background_start_names_the_issue_and_accepts_edits() {
        let command = start_command(
            Path::new("/bin/claude"),
            Path::new("doing/002-build.md"),
            Path::new("/code/app"),
            "opus",
            "PROMPT",
        );
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--bg",
                "PROMPT",
                "--name",
                "iboard-build",
                "--permission-mode",
                "acceptEdits",
                "--model",
                "opus"
            ]
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/code/app")));
    }

    #[test]
    fn the_short_id_is_the_one_the_other_commands_take() {
        let printed = "Starting background service…\n\
                       backgrounded · 8b1d2ff0 · iboard-smoke\n\
                       \x20 claude agents             list sessions\n";
        assert_eq!(short_id(printed).as_deref(), Some("8b1d2ff0"));
        assert_eq!(short_id("nothing to see"), None);
    }

    #[test]
    fn only_iboard_background_sessions_of_this_project_are_listed() {
        let project = Path::new("/code/app");
        let running = json!({
            "id": "8b1d2ff0", "cwd": "/code/app", "kind": "background",
            "startedAt": 1_000, "name": "iboard-build", "state": "working"
        });
        let session = session_of(&running, project).expect("a session");
        assert_eq!(session.id, "8b1d2ff0");
        assert_eq!(session.slug.as_deref(), Some("build"));
        assert_eq!(session.status.created, 1_000);
        assert!(session.status.running);
        for state in ["done", "failed", "stopped"] {
            let finished = json!({
                "id": "8b1d2ff0", "cwd": "/code/app", "kind": "background",
                "startedAt": 1_000, "name": "iboard-build", "state": state
            });
            let session = session_of(&finished, project).expect("a session");
            assert!(!session.status.running, "{state}");
        }
        let interactive = json!({
            "id": "x", "cwd": "/code/app", "kind": "interactive",
            "startedAt": 1_000, "name": "iboard-build", "state": "working"
        });
        assert_eq!(session_of(&interactive, project), None);
        let theirs = json!({
            "id": "x", "cwd": "/code/app", "kind": "background",
            "startedAt": 1_000, "name": "my-own-session", "state": "working"
        });
        assert_eq!(session_of(&theirs, project), None);
        let elsewhere = json!({
            "id": "x", "cwd": "/code/other", "kind": "background",
            "startedAt": 1_000, "name": "iboard-build", "state": "working"
        });
        assert_eq!(session_of(&elsewhere, project), None);
    }

    #[test]
    fn an_assistant_transcript_reads_text_and_its_tools() {
        let text = concat!(
            r#"{"type":"assistant","uuid":"m1","message":{"content":[{"type":"text","text":"Looking."},{"type":"tool_use","name":"Read","input":{"file_path":"/code/app/src/a.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","uuid":"u1","message":{"content":[{"type":"tool_result"}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"m1","message":{"content":[{"type":"text","text":"Looking."}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"m2","message":{"content":[{"type":"text","text":"Done."}]}}"#,
        );
        let messages = messages_of(text);
        assert_eq!(messages.len(), 2, "a duplicated record is read once");
        assert_eq!(messages[0].id, "m1");
        assert!(matches!(&messages[0].parts[1], Part::Tool { name, .. } if name == "Read"));
        assert_eq!(messages[1].parts, [Part::Text("Done.".into())]);
    }

    #[test]
    fn an_assistant_transcript_adds_up_what_a_session_spent() {
        let text = concat!(
            r#"{"type":"assistant","uuid":"m1","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":30,"cache_creation_input_tokens":40,"output_tokens_details":{"thinking_tokens":5}}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"m1","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":30,"cache_creation_input_tokens":40,"output_tokens_details":{"thinking_tokens":5}}}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"m2","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4,"output_tokens_details":{"thinking_tokens":6}}}}"#,
            "\n",
        );
        let usage = spend_of(text).expect("usage");
        assert_eq!(usage.cost, None, "an assistant record has no cost");
        assert_eq!(
            usage.tokens,
            Tokens {
                input: 11,
                output: 22,
                reasoning: 11,
                cache_read: 33,
                cache_write: 44,
            }
        );
        assert_eq!(usage.provider.as_deref(), Some("claude"));
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(usage.billing, Billing::Subscription);
    }

    #[test]
    fn a_cost_state_gives_the_whole_runs_cost_and_main_model() {
        let text = concat!(
            r#"{"type":"assistant","uuid":"m1","message":{"model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":2}}}"#,
            "\n",
            r#"{"type":"cost-state","totalCostUSD":0.42,"modelUsage":{"claude-haiku-5-5":{"inputTokens":100,"outputTokens":10,"thinkingTokens":1,"cacheReadInputTokens":0,"cacheCreationInputTokens":0,"costUSD":0.1},"claude-opus-5-5":{"inputTokens":20,"outputTokens":200,"thinkingTokens":30,"cacheReadInputTokens":400,"cacheCreationInputTokens":50,"costUSD":0.32}}}"#,
            "\n",
        );
        let usage = spend_of(text).expect("usage");
        assert_eq!(usage.cost, Some(0.42));
        assert_eq!(
            usage.tokens,
            Tokens {
                input: 120,
                output: 210,
                reasoning: 31,
                cache_read: 400,
                cache_write: 50,
            },
            "a cost-state holds the whole tree, subagents included"
        );
        assert_eq!(usage.model.as_deref(), Some("claude-opus-5-5"));
    }

    #[test]
    fn a_sessions_usage_is_read_from_its_job_transcript() {
        let config = tempfile::tempdir().unwrap();
        let id = "bg000001";
        let session = format!("{id}-0000-0000-0000-000000000000");
        let job = config.path().join("jobs").join(id);
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(
            job.join("state.json"),
            json!({ "sessionId": session, "cwd": "/code/app" }).to_string(),
        )
        .unwrap();
        let transcript = config
            .path()
            .join("projects/-code-app")
            .join(format!("{session}.jsonl"));
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(
            &transcript,
            r#"{"type":"assistant","uuid":"m1","message":{"model":"claude-opus-5-5","usage":{"input_tokens":7,"output_tokens":8}}}"#,
        )
        .unwrap();

        let listed = [json!({
            "id": id, "cwd": "/code/app", "kind": "background",
            "startedAt": 1_000, "name": "iboard-build", "state": "done"
        })];
        let sessions = sessions_of(&listed, Path::new("/code/app"), Some(config.path()));
        assert_eq!(sessions.len(), 1);
        let usage = sessions[0].usage.clone().expect("usage");
        assert_eq!(usage.tokens.input, 7);
        assert_eq!(usage.tokens.output, 8);
        assert_eq!(
            sessions_of(&listed, Path::new("/code/app"), None)[0].usage,
            None,
            "no config folder, nothing to read"
        );
        let entries = crate::domains::agent::artifacts::usage_log::finished(
            &sessions,
            Path::new("/code/app"),
            service().key(),
        );
        assert_eq!(entries[0].engine, "claude");
        assert_eq!(entries[0].provider.as_deref(), Some("claude"));
        assert_eq!(entries[0].tokens.input, 7);
    }

    #[test]
    fn a_jobs_timeline_gives_its_answer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"at":"1","state":"working","detail":"Reading the issue"}"#,
                "\n",
                r#"{"at":"2","state":"done","text":"Done."}"#,
                "\n",
                r#"{"at":"3","state":"done","detail":"replied Done as requested"}"#,
                "\n",
            ),
        )
        .unwrap();
        let messages = timeline(&path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts, [Part::Text("Done.".into())]);
    }

    #[test]
    fn a_claude_without_background_sessions_is_told_apart() {
        assert!(unsupported("error: unknown option '--bg'"));
        assert!(unsupported("error: unexpected argument '--bg' found"));
        assert!(!unsupported(
            "Workspace not trusted. Run `claude` in /code once"
        ));
        assert!(!unsupported("A background session is already running"));
    }

    #[test]
    fn tests_use_the_fake_claude() {
        let path = binary().expect("a fake claude");
        assert!(
            path.starts_with(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes")),
            "{path:?}"
        );
    }

    #[test]
    fn the_fake_starts_a_background_build_and_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let issue = dir.path().join("issues/doing/002-build.md");
        let session = service()
            .start(&issue, dir.path(), "", "PROMPT")
            .expect("the fake starts");
        assert!(!session.is_empty());
        let sessions = service().sessions(dir.path()).expect("the fake lists");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session);
        assert_eq!(sessions[0].slug.as_deref(), Some("smoke"));
        assert!(!sessions[0].status.running, "the fake finished");
    }

    #[test]
    fn a_claude_without_background_builds_as_a_child_process() {
        use super::super::{AgentConfig, BuildTimeout, Tool, start_build};
        let scratch = tempfile::tempdir().unwrap();
        let log = scratch.path().join("build.log");
        let config = AgentConfig {
            tool: Tool::Claude,
            model: "m".into(),
            configured: true,
            ..AgentConfig::default()
        };
        let (job, _, handle) = start_build(
            &config,
            "NO-BG",
            Path::new("a.md"),
            scratch.path(),
            &log,
            BuildTimeout::default(),
        );
        let answer = job.wait().unwrap().expect("the child builds");
        assert!(answer.starts_with("-p\n"), "{answer}");
        assert_eq!(
            super::super::service::session_of(&handle),
            None,
            "nothing runs in a service"
        );
    }
}

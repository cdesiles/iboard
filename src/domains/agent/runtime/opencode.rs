//! opencode's background service: the sessions it owns, spoken to through
//! `opencode api`. A build started here outlives the window, because the
//! service runs the turn and iboard only watches it: quitting stops the
//! watching, not the work.
//!
//! Set `IBOARD_OPENCODE` to another binary to test this without one.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use super::service::{
    Answer, Choice, Field, FieldKind, Message, Part, Permission, PermissionReply, Question,
    QuestionKind, Service, Session, Status,
};
use super::{Usage, usage};
use crate::domains::agent::artifacts::reviews;
use crate::platform::job::Cancel;
use crate::platform::process;

/// How long one call to the service may take.
const CALL: Duration = Duration::from_secs(20);

/// How many of a session's newest messages its step and answer are read from.
const PAGE: usize = 20;

/// How many times a read is retried when its reply comes back cut short.
const RETRIES: usize = 3;

/// opencode, the first tool to build in a background service.
pub(super) fn service() -> &'static dyn Service {
    &OPENCODE
}

static OPENCODE: OpenCode = OpenCode;

struct OpenCode;

impl Service for OpenCode {
    fn key(&self) -> &'static str {
        "opencode"
    }

    fn start(
        &self,
        issue: &Path,
        project: &Path,
        model: &str,
        prompt: &str,
    ) -> Result<String, String> {
        let body = create_body(issue, project, model);
        let reply = call(project, &["post", "/api/session"], Some(&body.to_string()))?;
        let value = parse(&reply)?;
        let session = value["data"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "opencode made no session".to_string())?;
        queue(project, &session, prompt)?;
        Ok(session)
    }

    fn sessions(&self, project: &Path) -> Result<Vec<Session>, String> {
        let project = canonical(project);
        let sessions = listed(&project)?;
        Ok(sessions
            .iter()
            .filter_map(|session| session_of(session, &project))
            .collect())
    }

    fn read(&self, project: &Path, session: &str) -> Result<Vec<Message>, String> {
        let path = format!("/api/session/{session}/message?order=desc&limit={PAGE}");
        let reply = call(project, &["get", &path], None)?;
        let value = parse(&reply)?;
        let mut messages = value["data"].as_array().cloned().unwrap_or_default();
        messages.reverse();
        Ok(messages.iter().filter_map(message_of).collect())
    }

    fn say(&self, project: &Path, session: &str, text: &str) -> Result<(), String> {
        queue(project, session, text)
    }

    fn interrupt(&self, project: &Path, session: &str) {
        let path = format!("/api/session/{session}/interrupt");
        let _ = call(project, &["post", &path], None);
    }

    fn questions(&self, project: &Path) -> Result<Vec<Question>, String> {
        let forms = list(project, "form.list");
        let permissions = list(project, "permission.request.list");
        match (forms, permissions) {
            (Ok(forms), Ok(permissions)) => Ok(questions_from(forms, permissions)),
            (Ok(forms), Err(_)) => Ok(questions_from(forms, Vec::new())),
            (Err(_), Ok(permissions)) => Ok(questions_from(Vec::new(), permissions)),
            (Err(forms), Err(permissions)) => Err(format!("{forms}; {permissions}")),
        }
    }

    fn answer(
        &self,
        project: &Path,
        session: &str,
        id: &str,
        answer: &Answer,
    ) -> Result<(), String> {
        match answer {
            Answer::Form(form) => answer_form(project, session, id, form),
            Answer::Permission(reply) => answer_permission(project, session, id, *reply),
        }
    }
}

/// One request to the service, from `project`, so it reads that project's
/// configuration.
fn call(project: &Path, args: &[&str], body: Option<&str>) -> Result<String, String> {
    call_with(project, args, body, CALL)
}

fn call_with(
    project: &Path,
    args: &[&str],
    body: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let binary = binary().ok_or_else(|| "opencode is not installed".to_string())?;
    request(&binary, project, args, body, timeout)
}

/// Runs `binary` for one request, its reply read from a file: the service's
/// CLI drops the tail of a reply too large for a pipe, at a write boundary,
/// and retrying only gets the same cut text back. A read whose reply is still
/// not valid JSON is tried again, in case the service was busy.
fn request(
    binary: &Path,
    project: &Path,
    args: &[&str],
    body: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let reads = args.first().is_some_and(|method| *method == "get");
    let attempts = if reads { RETRIES } else { 1 };
    let deadline = Instant::now() + timeout;
    let mut text = String::new();
    for attempt in 1..=attempts {
        let left = deadline.saturating_duration_since(Instant::now());
        let reply = tempfile::NamedTempFile::new()
            .map_err(|err| format!("Couldn't open a reply file: {err}"))?;
        let mut command = std::process::Command::new(binary);
        command.arg("api").args(args).current_dir(project);
        if let Some(body) = body {
            command.args(["-d", body]);
        }
        text = process::run_to_file(
            command,
            "opencode api",
            &Cancel::default(),
            left,
            reply.path(),
        )?
        .into_result("opencode api")?;
        if attempt == attempts || serde_json::from_str::<Value>(&text).is_ok() {
            break;
        }
    }
    Ok(text)
}

/// The `opencode` to call: the tests' fake, or the real one outside them.
fn binary() -> Option<PathBuf> {
    std::env::var_os("IBOARD_OPENCODE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let path = super::Tool::OpenCode.program();
            path.is_file().then_some(path)
        })
}

fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|err| format!("opencode api: {err}"))
}

/// The sessions the service lists for `project`, running or finished.
fn listed(project: &Path) -> Result<Vec<Value>, String> {
    let reply = call(project, &["get", "/api/session"], None)?;
    let value = parse(&reply)?;
    Ok(value["data"].as_array().cloned().unwrap_or_default())
}

/// One of the project's sessions, if that is what `session` is.
fn session_of(session: &Value, project: &Path) -> Option<Session> {
    let directory = session["location"]["directory"].as_str()?;
    if Path::new(directory) != project {
        return None;
    }
    let status = status_of(session);
    Some(Session {
        id: session["id"].as_str()?.to_string(),
        slug: session["metadata"]["iboard"]["slug"]
            .as_str()
            .map(str::to_string),
        status,
        ended: ended_of(session, status.created),
        usage: usage_of(session),
    })
}

/// How a session stands, as opencode keeps it.
fn status_of(session: &Value) -> Status {
    let millis = |key: &str| session["time"][key].as_u64().unwrap_or(0);
    let (busy, idle) = (millis("busy"), millis("idle"));
    Status {
        running: busy > 0 && idle < busy,
        created: millis("created"),
        updated: millis("updated").max(busy),
    }
}

/// The last time a finished session wrote anything, in the service's
/// milliseconds.
fn ended_of(session: &Value, created: u64) -> u64 {
    ["updated", "idle", "busy"]
        .iter()
        .filter_map(|key| session["time"][key].as_u64())
        .max()
        .unwrap_or(created)
        .max(created)
}

/// What a session's model and tokens spent, when it reports any.
fn usage_of(session: &Value) -> Option<Usage> {
    let cost = session["cost"].as_f64();
    let tokens = &session["tokens"];
    let provider = usage::text(&session["model"]["providerID"]);
    let model = usage::text(&session["model"]["id"]);
    (cost.is_some() || !tokens.is_null() || provider.is_some() || model.is_some()).then(|| Usage {
        cost,
        tokens: usage::tokens_of(tokens),
        billing: usage::billing_of(provider.as_deref()),
        provider,
        model,
    })
}

/// Creates a session for `issue` in `project`, titled after it.
fn create_body(issue: &Path, project: &Path, model: &str) -> Value {
    let title = crate::platform::files::file_name(issue);
    let directory = canonical(project);
    let mut body = json!({
        "title": title,
        "location": { "directory": directory },
        "metadata": { "iboard": { "slug": reviews::slug(issue) } },
    });
    if let Some(model) = model_ref(model) {
        body["model"] = model;
    }
    body
}

/// `provider/model#variant`, as the service wants it, or `None` when the
/// model is the service's own choice.
fn model_ref(model: &str) -> Option<Value> {
    let model = model.split('#').next()?.trim();
    let (provider, id) = model.split_once('/')?;
    Some(json!({ "providerID": provider, "id": id }))
}

/// Hands `text` to `session`; the service runs it whether or not anyone
/// watches.
fn queue(project: &Path, session: &str, text: &str) -> Result<(), String> {
    let path = format!("/api/session/{session}/prompt");
    let body = json!({ "text": text, "delivery": "queue" }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

/// The pending items of one opencode list operation. The operation ID, not a
/// raw method and path: only an operation binds `--param`, and without the
/// location the service answers for its default folder.
fn list(project: &Path, operation: &str) -> Result<Vec<Value>, String> {
    let args = list_args(project, operation);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let reply = call(project, &args, None)?;
    let value = parse(&reply)?;
    Ok(value["data"].as_array().cloned().unwrap_or_default())
}

fn list_args(project: &Path, operation: &str) -> Vec<String> {
    vec![
        operation.to_string(),
        "--param".to_string(),
        format!("location[directory]={}", canonical(project).display()),
    ]
}

fn questions_from(forms: Vec<Value>, permissions: Vec<Value>) -> Vec<Question> {
    let forms = forms.iter().filter_map(question_from_form);
    let permissions = permissions.iter().filter_map(question_from_permission);
    forms.chain(permissions).collect()
}

/// The parts of one assistant message, oldest first.
fn message_of(value: &Value) -> Option<Message> {
    if value["type"] != "assistant" {
        return None;
    }
    let parts = value["content"]
        .as_array()
        .map_or_else(Vec::new, |parts| parts.iter().map(part_of).collect());
    Some(Message {
        id: value["id"].as_str().unwrap_or_default().to_string(),
        closed: ["streamed", "completed"]
            .iter()
            .any(|key| !value["time"][*key].is_null()),
        parts,
    })
}

fn part_of(value: &Value) -> Part {
    match value["type"].as_str() {
        Some("tool" | "question") => Part::Tool {
            name: value["name"].as_str().unwrap_or("tool").to_string(),
            input: value["state"]["input"].clone(),
            streaming: value["state"]["status"] == "streaming",
        },
        Some("reasoning") => {
            let created = value["time"]["created"].as_u64().unwrap_or(0);
            let done = value["time"]["completed"].as_u64();
            Part::Thought {
                secs: done.map_or(0, |done| done.saturating_sub(created) / 1000),
                text: value["text"].as_str().unwrap_or_default().to_string(),
                streaming: done.is_none(),
            }
        }
        Some("text") => Part::Text(value["text"].as_str().unwrap_or_default().to_string()),
        _ => Part::Other,
    }
}

fn question_from_form(value: &Value) -> Option<Question> {
    let fields = value["fields"]
        .as_array()?
        .iter()
        .filter_map(field_from)
        .collect();
    Some(Question::new(
        service(),
        value["sessionID"].as_str()?,
        value["id"].as_str()?,
        value["title"].as_str()?,
        QuestionKind::Form(fields),
    ))
}

fn field_from(value: &Value) -> Option<Field> {
    let key = value["key"].as_str()?.to_string();
    let kind = match value["type"].as_str()? {
        "string" => FieldKind::Text {
            options: choices(value),
            placeholder: value["placeholder"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            default: value["default"].as_str().unwrap_or_default().to_string(),
            custom: value["custom"].as_bool().unwrap_or(false),
        },
        "number" | "integer" => FieldKind::Number {
            default: number(value["default"].as_f64()),
            integer: value["type"] == "integer",
        },
        "boolean" => FieldKind::Boolean {
            default: value["default"].as_bool().unwrap_or(false),
        },
        "multiselect" => FieldKind::Multi {
            options: choices(value),
            default: value["default"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect(),
            custom: value["custom"].as_bool().unwrap_or(false),
        },
        "external" => FieldKind::External {
            url: value["url"].as_str().unwrap_or_default().to_string(),
        },
        _ => return None,
    };
    Some(Field {
        title: value["title"].as_str().unwrap_or(&key).to_string(),
        description: value["description"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        required: value["required"].as_bool().unwrap_or(false),
        hidden: value["hidden"].as_bool().unwrap_or(false),
        key,
        kind,
    })
}

fn choices(value: &Value) -> Vec<Choice> {
    value["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let value = option["value"].as_str()?.to_string();
            Some(Choice {
                label: option["label"].as_str().unwrap_or(&value).to_string(),
                description: option["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                value,
            })
        })
        .collect()
}

fn number(value: Option<f64>) -> String {
    value.map(|value| format!("{value}")).unwrap_or_default()
}

fn question_from_permission(value: &Value) -> Option<Question> {
    let action = value["action"].as_str()?.to_string();
    Some(Question::new(
        service(),
        value["sessionID"].as_str()?,
        value["id"].as_str()?,
        format!("Allow {action}?"),
        QuestionKind::Permission(Permission {
            action,
            resources: value["resources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|resource| resource.as_str().map(str::to_string))
                .collect(),
            message: value["message"].as_str().map(str::to_string),
        }),
    ))
}

/// Answers a form `question`, whose keys are its fields' keys.
fn answer_form(
    project: &Path,
    session: &str,
    id: &str,
    answer: &Map<String, Value>,
) -> Result<(), String> {
    let path = format!("/api/session/{session}/form/{id}/reply");
    let body = json!({ "answer": answer }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

/// Answers a tool's permission request.
fn answer_permission(
    project: &Path,
    session: &str,
    id: &str,
    reply: PermissionReply,
) -> Result<(), String> {
    let path = format!("/api/session/{session}/permission/{id}/reply");
    let body = json!({ "decision": reply.key() }).to_string();
    call(project, &["post", &path], Some(&body)).map(|_| ())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_titled_and_tagged_with_its_issue() {
        let body = create_body(Path::new("doing/002-build.md"), Path::new("/code/app"), "");
        assert_eq!(body["title"], "002-build.md");
        assert_eq!(body["metadata"]["iboard"]["slug"], "build");
        assert_eq!(body["location"]["directory"], "/code/app");
        assert_eq!(service().key(), "opencode");
    }

    #[test]
    fn a_model_becomes_the_services_own_reference() {
        assert_eq!(
            model_ref("deepseek/deepseek-flash"),
            Some(json!({ "providerID": "deepseek", "id": "deepseek-flash" }))
        );
        assert_eq!(
            model_ref("anthropic/claude-sonnet#thinking")
                .as_ref()
                .unwrap()["id"],
            "claude-sonnet"
        );
        assert_eq!(model_ref(""), None, "the service picks its own");
        assert_eq!(model_ref("no-provider"), None);
    }

    #[test]
    fn a_running_session_is_not_idle_since_its_last_turn() {
        let running = json!({"id": "ses_1", "location": {"directory": "/code/app"},
            "metadata": {"iboard": {"slug": "build"}},
            "time": {"created": 1_000, "busy": 2_000}});
        let session = session_of(&running, Path::new("/code/app")).expect("a session");
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.slug.as_deref(), Some("build"));
        assert!(session.status.running);
        assert_eq!(session.ended, 2_000);
        let done = json!({"id": "ses_1", "location": {"directory": "/code/app"},
            "metadata": {"iboard": {"slug": "build"}},
            "time": {"created": 1_000, "busy": 2_000, "idle": 3_000}});
        let session = session_of(&done, Path::new("/code/app")).expect("a session");
        assert!(!session.status.running, "it finished");
        assert_eq!(session.ended, 3_000);
    }

    #[test]
    fn only_this_projects_sessions_are_listed() {
        let elsewhere = json!({"id": "ses_2", "location": {"directory": "/code/other"},
            "metadata": {"iboard": {"slug": "build"}}, "time": {"busy": 2_000}});
        assert_eq!(session_of(&elsewhere, Path::new("/code/app")), None);
        let stranger = json!({"id": "ses_3", "location": {"directory": "/code/app"},
            "time": {"busy": 2_000}});
        let session = session_of(&stranger, Path::new("/code/app")).expect("a session");
        assert_eq!(session.slug, None, "not ours");
    }

    #[test]
    fn a_reasoning_part_is_read_with_its_time() {
        let value = json!({
            "type": "assistant", "id": "msg_1",
            "time": {"completed": 1_760_000_010_000u64},
            "content": [
                {"type": "reasoning", "text": "Weighing the fix.",
                 "time": {"created": 1_760_000_000_000u64, "completed": 1_760_000_004_000u64}},
                {"type": "reasoning", "text": "Still going",
                 "time": {"created": 1_760_000_004_000u64}},
                {"type": "tool", "name": "read",
                 "state": {"input": {"path": "/p/src/a.rs"}, "status": "completed"}}
            ]
        });
        let message = message_of(&value).expect("a message");
        assert_eq!(
            message.parts[0],
            Part::Thought {
                secs: 4,
                text: "Weighing the fix.".into(),
                streaming: false
            }
        );
        assert!(matches!(
            &message.parts[1],
            Part::Thought {
                streaming: true,
                ..
            }
        ));
        assert!(matches!(&message.parts[2], Part::Tool { .. }));
    }

    #[test]
    fn a_form_becomes_a_question_with_its_typed_fields() {
        let value = json!({
            "id": "frm_1", "sessionID": "ses_1", "title": "Which approach?",
            "fields": [
                {"key": "approach", "type": "string", "title": "Approach",
                 "required": true, "options": [
                    {"value": "fast", "label": "Fast", "description": "Ship it"},
                    {"value": "careful", "label": "Careful"}
                 ]},
                {"key": "notes", "type": "string", "placeholder": "Anything else?"},
                {"key": "count", "type": "integer", "default": 3},
                {"key": "deploy", "type": "boolean", "default": true},
                {"key": "areas", "type": "multiselect", "default": ["api"],
                 "options": [{"value": "api", "label": "API"}]},
            ]
        });
        let question = question_from_form(&value).expect("a form");
        assert_eq!(question.session, "ses_1");
        assert_eq!(question.title, "Which approach?");
        assert_eq!(question.service, service());
        let QuestionKind::Form(fields) = question.kind else {
            panic!("not a form");
        };
        assert_eq!(fields.len(), 5);
        assert!(fields[0].required);
        assert!(matches!(
            &fields[0].kind,
            FieldKind::Text { options, .. } if options.len() == 2
        ));
        assert!(matches!(
            &fields[2].kind,
            FieldKind::Number { integer: true, default } if default == "3"
        ));
        assert!(matches!(
            &fields[4].kind,
            FieldKind::Multi { default, .. } if default == &["api"]
        ));
    }

    #[test]
    fn a_permission_request_becomes_a_question() {
        let value = json!({
            "id": "per_1", "sessionID": "ses_1", "action": "bash",
            "resources": ["git push origin main"],
            "message": "Push the branch?"
        });
        let question = question_from_permission(&value).expect("a request");
        assert_eq!(question.title, "Allow bash?");
        let QuestionKind::Permission(permission) = question.kind else {
            panic!("not a permission");
        };
        assert_eq!(permission.resources, ["git push origin main"]);
        assert_eq!(permission.message.as_deref(), Some("Push the branch?"));
        assert_eq!(PermissionReply::Once.key(), "once");
        assert_eq!(PermissionReply::Always.key(), "always");
        assert_eq!(PermissionReply::Reject.key(), "reject");
    }

    #[test]
    fn a_list_names_its_operation_and_the_projects_location() {
        let scratch = tempfile::tempdir().unwrap();
        let project = canonical(scratch.path());
        let args = list_args(&project, "form.list");
        assert_eq!(args[0], "form.list", "an operation binds --param");
        assert_eq!(args[1], "--param");
        assert_eq!(
            args[2],
            format!("location[directory]={}", project.display())
        );
    }

    /// The service's CLI drops the tail of a reply too large for a pipe. A
    /// reply written to a file arrives whole.
    #[test]
    fn a_large_reply_is_read_whole_from_a_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("opencode");
        std::fs::write(
            &binary,
            "#!/bin/sh\n\
             if [ -p /dev/stdout ]; then printf '{\"data\":[';\n\
             else printf '{\"data\":[{\"id\":\"ses_1\"}]}';\n\
             fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let reply = request(
            &binary,
            dir.path(),
            &["get", "/api/session/ses_1/message"],
            None,
            CALL,
        )
        .unwrap();
        assert_eq!(parse(&reply).unwrap()["data"][0]["id"], "ses_1");
    }

    #[test]
    fn tests_use_the_fake_opencode() {
        let path = binary().expect("a fake opencode");
        assert!(
            path.starts_with(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes")),
            "{path:?}"
        );
    }

    /// A session from `GET /api/session`, trimmed to what usage reads.
    const OPENCODE_SESSION: &str = r#"{
        "id": "ses_deepseek",
        "model": {"id": "deepseek-flash", "providerID": "deepseek", "variant": "default"},
        "cost": 0.42,
        "tokens": {"input": 1234, "output": 56, "reasoning": 7,
                   "cache": {"read": 890, "write": 12}},
        "location": {"directory": "/code/app"}
    }"#;

    /// A Copilot session: a subscription, which reports no money.
    const COPILOT_SESSION: &str = r#"{
        "id": "ses_copilot",
        "model": {"id": "claude-opus-4.6", "providerID": "github-copilot"},
        "cost": 0,
        "tokens": {"input": 159, "output": 14001, "reasoning": 0,
                   "cache": {"read": 1902598, "write": 383223}},
        "location": {"directory": "/code/app"}
    }"#;

    #[test]
    fn an_opencode_session_reads_its_cost_and_tokens() {
        let session: Value = serde_json::from_str(OPENCODE_SESSION).unwrap();
        let usage = usage_of(&session).expect("usage");
        assert_eq!(usage.cost, Some(0.42));
        assert_eq!(
            usage.tokens,
            super::super::Tokens {
                input: 1234,
                output: 56,
                reasoning: 7,
                cache_read: 890,
                cache_write: 12,
            }
        );
        assert_eq!(usage.provider.as_deref(), Some("deepseek"));
        assert_eq!(usage.model.as_deref(), Some("deepseek-flash"));
        assert_eq!(usage.billing, super::super::Billing::Metered);
        assert_eq!(usage.cost_label().as_deref(), Some("$0.42"));
    }

    #[test]
    fn a_copilot_session_at_cost_zero_is_a_subscription() {
        let session: Value = serde_json::from_str(COPILOT_SESSION).unwrap();
        let usage = usage_of(&session).expect("usage");
        assert_eq!(usage.cost, Some(0.0));
        assert_eq!(usage.billing, super::super::Billing::Subscription);
        assert_eq!(usage.provider.as_deref(), Some("github-copilot"));
        assert_eq!(usage.tokens.input, 159);
        assert_eq!(usage.tokens.cache_read, 1_902_598);
        assert_eq!(
            usage.cost_label().as_deref(),
            Some("$0.00 API-equivalent"),
            "a subscription's dollars are never spend"
        );
    }

    /// The only test that may reach the real service. `make third-party` runs
    /// it, with `IBOARD_OPENCODE` pointing at the real opencode.
    #[test]
    #[ignore = "third-party: run `make third-party` with the real opencode service"]
    fn third_party_the_real_service_answers() {
        let real = std::env::var_os("IBOARD_OPENCODE")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .expect("run `make third-party`, not `cargo test`");
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let reply = request(&real, project, &["get", "/api/session"], None, CALL)
            .expect("the real opencode api answers");
        let value = parse(&reply).expect("its reply is JSON");
        assert!(value["data"].is_array(), "{value}");
    }
}

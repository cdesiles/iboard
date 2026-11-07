//! One exchange with an agent, and the plan turns that keep their session.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use super::{
    AgentConfig, Cancel, DIRENV, Progress, Purpose, Tool, Workspace, build_log, command,
    default_model, direnv, files, final_answer, find_binary, process, prompt_file, rejects_plan,
    session_of, step_of, stream_error,
};
use crate::job::Job;

/// The session a plan turn lives in, filled as the tool names it.
type SessionSlot = Arc<Mutex<Option<String>>>;

/// What [`exchange`] reports while the tool works.
#[derive(Clone, Copy)]
pub(super) struct Watched<'a> {
    pub(super) progress: Option<&'a Progress>,
    pub(super) session: Option<&'a SessionSlot>,
}

/// A tool's answer and the session it gave it in, when it named one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub answer: String,
    pub session: Option<String>,
}

/// [`super::run`] for `purpose`, in `workspace` when set; the agent's steps
/// land in `progress` as they come.
pub(super) fn run_with(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    cancel: &Cancel,
    purpose: Purpose,
    progress: Option<&Progress>,
    workspace: Option<&Workspace>,
) -> Result<String, String> {
    let watched = Watched {
        progress,
        session: None,
    };
    exchange(config, prompt, issue, cancel, purpose, watched, workspace).map(|chat| chat.answer)
}

/// Runs one exchange with the agent, keeping the session id it reports.
fn exchange(
    config: &AgentConfig,
    prompt: &str,
    issue: &Path,
    cancel: &Cancel,
    purpose: Purpose,
    watched: Watched,
    workspace: Option<&Workspace>,
) -> Result<Chat, String> {
    let model = match config.model.trim() {
        "" if config.tool != Tool::Custom => default_model(config.tool).unwrap_or_default(),
        model => model.to_string(),
    };
    let custom = config.tool == Tool::Custom;
    let prompt_file = prompt_file(config, prompt)?;
    let what = match config.tool {
        Tool::Custom => "the custom command",
        tool => tool.key(),
    };
    let prompt_path = prompt_file
        .as_ref()
        .map_or(Path::new(""), files::TempFile::path);
    let session = watched
        .session
        .and_then(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).clone());
    let mut command = command(
        config,
        &model,
        prompt,
        prompt_path,
        issue,
        purpose,
        session.as_deref(),
    );
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
    let streams = watched.progress.filter(|_| !custom);
    let output = process::run_with(command, what, cancel, purpose.timeout(), |line| {
        if let Some(slot) = watched.session
            && let Some(seen) = session_of(line)
        {
            *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(seen);
        }
        if let Some(progress) = streams
            && let Some(step) = step_of(line)
        {
            *progress.lock().unwrap_or_else(PoisonError::into_inner) = Some(step);
        }
        if let Some(log) = log.as_mut() {
            for entry in build_log::entries(line) {
                let _ = build_log::write_line(log, &entry);
            }
        }
    });
    drop(prompt_file);
    let output = output?;
    let failure = stream_error(&output.stdout);
    let answer = output
        .into_result("the agent")
        .map_err(|err| failure.clone().unwrap_or(err))?;
    let answer = match streams {
        Some(_) => final_answer(&answer),
        None => answer,
    };
    let answer = match answer.trim() {
        "" if !purpose.works() => {
            return Err(failure.unwrap_or_else(|| "the agent returned no answer".to_string()));
        }
        answer => answer.to_string(),
    };
    let session = watched
        .session
        .and_then(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).clone());
    Ok(Chat { answer, session })
}

/// One turn of a plan in `project`, continuing `session` when the tool can.
/// Dropping the job stops the agent. The answer carries the session id back.
pub fn start_plan(
    config: &AgentConfig,
    prompt: &str,
    project: Option<&Path>,
    session: Option<&str>,
    tickets: bool,
) -> (Job<Result<Chat, String>>, Progress) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let project = project.map(Path::to_path_buf);
    let session = session.map(str::to_string);
    let progress = Progress::default();
    let steps = Arc::clone(&progress);
    let job = Job::spawn(move |cancel| {
        let turn = |plan_mode: bool| -> Result<Chat, String> {
            let slot = SessionSlot::default();
            if let Some(seed) = &session {
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(seed.clone());
            }
            let workspace = project.as_deref().map(Workspace::of);
            let purpose = Purpose::Plan { plan_mode, tickets };
            let watched = Watched {
                progress: Some(&steps),
                session: Some(&slot),
            };
            let mut chat = exchange(
                &config,
                &prompt,
                Path::new(""),
                cancel,
                purpose,
                watched,
                workspace.as_ref(),
            )?;
            chat.session = chat.session.or_else(|| session.clone());
            Ok(chat)
        };
        let first = turn(true);
        match first {
            Err(error)
                if !cancel.is_cancelled()
                    && !config.tool.plan_args().is_empty()
                    && rejects_plan(&error) =>
            {
                turn(false)
            }
            result => result,
        }
    });
    (job, progress)
}

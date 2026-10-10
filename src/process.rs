//! Child processes for the agents and helpers. Each runs in its own process
//! group, so stopping it also stops what it started (a `sh` pipeline, an
//! agent's own tools); each has a deadline; and the job waiting on it can
//! cancel it. A group is only signalled while its leader is not yet reaped, so
//! a recycled process id is never hit.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::job::Cancel;

/// How often a running process is checked on.
const TICK: Duration = Duration::from_millis(20);

/// How long a process asked to stop gets before it is killed, and how long a
/// finished one's output may stay open.
const GRACE: Duration = Duration::from_secs(2);

/// The processes still running, so quitting can stop them.
static RUNNING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// What a finished process printed, and how it ended.
#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// The standard output of a success; otherwise the error output, or how
    /// the process exited when it printed none.
    pub fn into_result(self, what: &str) -> Result<String, String> {
        if self.status.success() {
            return Ok(self.stdout);
        }
        let stderr = self.stderr.trim();
        Err(if stderr.is_empty() {
            format!("{what} exited with {}", self.status)
        } else {
            stderr.to_string()
        })
    }
}

/// Runs `command` to its end, with no input and its output captured. `what`
/// names it in errors ("the renderer"). It is stopped, with everything it
/// started, as soon as `cancel` says so or once `timeout` has passed.
pub fn run(
    mut command: Command,
    what: &str,
    cancel: &Cancel,
    timeout: Duration,
) -> Result<Output, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .spawn()
        .map_err(|err| format!("Couldn't run {what}: {err}"))?;
    running().push(child.id());
    let stdout = child.stdout.take().map(Drain::start);
    let stderr = child.stderr.take().map(Drain::start);
    let started = Instant::now();
    let status = loop {
        if cancel.is_cancelled() {
            stop(&mut child);
            return Err(format!("{what} was stopped"));
        }
        if started.elapsed() >= timeout {
            stop(&mut child);
            return Err(format!("{what} gave no answer within {}", span(timeout)));
        }
        match reap(&mut child) {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(TICK),
            Err(err) => {
                stop(&mut child);
                return Err(format!("Couldn't wait for {what}: {err}"));
            }
        }
    };
    Ok(Output {
        status,
        stdout: stdout.map(Drain::finish).unwrap_or_default(),
        stderr: stderr.map(Drain::finish).unwrap_or_default(),
    })
}

/// Stops every process still running, with what it started. For quitting,
/// which exits before the jobs can clean up after themselves.
pub fn stop_all() {
    #[cfg(unix)]
    for pid in running().iter() {
        if let Ok(group) = libc::pid_t::try_from(*pid) {
            // SAFETY: `killpg` only sends a signal. The lock keeps every listed
            // process unreaped, so the group is still the one it started.
            unsafe { libc::killpg(group, libc::SIGTERM) };
        }
    }
}

fn running() -> MutexGuard<'static, Vec<u32>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `try_wait`, forgetting the process once it is reaped. Holding the lock
/// keeps [`stop_all`] from signalling a group whose leader is gone.
fn reap(child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
    let mut running = running();
    let status = child.try_wait()?;
    if status.is_some() {
        running.retain(|pid| *pid != child.id());
    }
    Ok(status)
}

/// Asks the process and its group to stop, kills them when they have not after
/// [`GRACE`], and reaps the process.
fn stop(child: &mut Child) {
    signal(child, Signal::Terminate);
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        match reap(child) {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(TICK),
            Err(_) => break,
        }
    }
    signal(child, Signal::Kill);
    running().retain(|pid| *pid != child.id());
    let _ = child.wait();
}

#[derive(Clone, Copy)]
enum Signal {
    Terminate,
    Kill,
}

fn signal(child: &mut Child, signal: Signal) {
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        let number = match signal {
            Signal::Terminate => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        // SAFETY: `killpg` only sends a signal, to the child's own group; the
        // child is not reaped yet, so the group id cannot have been reused.
        unsafe { libc::killpg(group, number) };
    }
    #[cfg(not(unix))]
    {
        let _ = signal;
        let _ = child.kill();
    }
}

/// "10 minutes", "20 seconds".
fn span(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 120 && seconds.is_multiple_of(60) {
        format!("{} minutes", seconds / 60)
    } else if seconds > 0 {
        format!("{seconds} seconds")
    } else {
        format!("{} ms", duration.as_millis())
    }
}

/// Reads a pipe to its end on its own thread.
struct Drain {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Receiver<()>,
}

impl Drain {
    fn start(mut pipe: impl Read + Send + 'static) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&bytes);
        let (sender, closed) = channel();
        std::thread::spawn(move || {
            let mut chunk = [0; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(read) => sink
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .extend_from_slice(&chunk[..read]),
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = sender.send(());
        });
        Self { bytes, closed }
    }

    /// What was read, once the pipe closed. Something the process left behind
    /// may hold it open: after [`GRACE`], what is in is the answer.
    fn finish(self) -> String {
        let _ = self.closed.recv_timeout(GRACE);
        let bytes = std::mem::take(&mut *self.bytes.lock().unwrap_or_else(PoisonError::into_inner));
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[test]
    fn output_and_errors_are_captured() {
        let cancel = Cancel::default();
        let minute = Duration::from_secs(60);
        let output = run(sh("echo out; echo err >&2"), "it", &cancel, minute).unwrap();
        assert_eq!(output.stdout, "out\n");
        assert_eq!(output.stderr, "err\n");
        let failed = run(sh("echo why >&2; exit 3"), "it", &cancel, minute).unwrap();
        assert_eq!(failed.into_result("it"), Err("why".to_string()));
        let silent = run(sh("exit 4"), "it", &cancel, minute).unwrap();
        assert!(
            silent
                .into_result("it")
                .unwrap_err()
                .starts_with("it exited with")
        );
        let missing = run(Command::new("/no/such/tool"), "the tool", &cancel, minute);
        assert!(missing.unwrap_err().starts_with("Couldn't run the tool"));
    }

    #[test]
    fn a_process_past_its_deadline_is_stopped() {
        let started = Instant::now();
        let error = run(
            sh("sleep 30"),
            "it",
            &Cancel::default(),
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert_eq!(error, "it gave no answer within 200 ms");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancelling_stops_the_whole_pipeline() {
        let dir = std::env::temp_dir().join(format!("iboard-process-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid_file = dir.join("sleep.pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let cancel = Cancel::default();
        let token = cancel.clone();
        let runner =
            std::thread::spawn(move || run(sh(&script), "it", &token, Duration::from_secs(60)));
        let start = Instant::now();
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|text| text.trim().parse::<i32>().ok())
            {
                break pid;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "the script never started"
            );
            std::thread::sleep(TICK);
        };
        assert!(alive(pid));

        cancel.cancel();
        assert_eq!(runner.join().unwrap().unwrap_err(), "it was stopped");
        let start = Instant::now();
        while alive(pid) && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(TICK);
        }
        assert!(!alive(pid), "the grandchild outlived the cancel");
        std::fs::remove_dir_all(&dir).ok();
    }
}

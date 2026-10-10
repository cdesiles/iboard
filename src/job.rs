//! Work that runs off the UI thread, polled once per frame.
//!
//! A [`Job`] runs a closure on its own thread and hands back its result. The UI
//! polls it every frame and wakes up every [`POLL_EVERY`] while it waits.
//! Dropping a job tells its work to stop, so a closed window or a replaced run
//! never leaves an agent working in the background.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

/// How often the UI checks on a running job.
pub const POLL_EVERY: Duration = Duration::from_millis(100);

/// The error of a job whose thread died without answering.
pub const LOST: &str = "the background work stopped unexpectedly";

/// Tells a job's work that its result is no longer wanted.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A result being computed on another thread.
pub struct Job<T> {
    result: Receiver<T>,
    cancel: Cancel,
    started: Instant,
}

/// Where a job stands.
pub enum Poll<T> {
    Waiting,
    Ready(T),
    /// The thread died without answering.
    Lost,
}

impl<T: Send + 'static> Job<T> {
    /// Runs `work` on its own thread. Work that takes long checks its
    /// [`Cancel`], and stops early once the job is dropped.
    pub fn spawn(work: impl FnOnce(&Cancel) -> T + Send + 'static) -> Self {
        let (sender, result) = channel();
        let cancel = Cancel::default();
        let token = cancel.clone();
        std::thread::spawn(move || {
            let _ = sender.send(work(&token));
        });
        Self {
            result,
            cancel,
            started: Instant::now(),
        }
    }
}

impl<T> Job<T> {
    /// A job whose result is already known.
    pub fn done(value: T) -> Self {
        let (sender, result) = channel();
        let _ = sender.send(value);
        Self {
            result,
            cancel: Cancel::default(),
            started: Instant::now(),
        }
    }

    /// The result once it is in. Only the first poll after it lands gets it.
    pub fn poll(&self) -> Poll<T> {
        match self.result.try_recv() {
            Ok(value) => Poll::Ready(value),
            Err(TryRecvError::Empty) => Poll::Waiting,
            Err(TryRecvError::Disconnected) => Poll::Lost,
        }
    }

    /// Blocks until the result is in; `None` when the thread died.
    pub fn wait(&self) -> Option<T> {
        self.result.recv().ok()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

impl<T> Job<Result<T, String>> {
    /// The answer once it is in, a lost thread counting as an error.
    pub fn answer(&self) -> Option<Result<T, String>> {
        match self.poll() {
            Poll::Waiting => None,
            Poll::Ready(result) => Some(result),
            Poll::Lost => Some(Err(LOST.to_string())),
        }
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eventually(condition: impl Fn() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn a_job_answers_once() {
        let job = Job::spawn(|_| 6 * 7);
        assert_eq!(job.wait(), Some(42));
        assert!(matches!(job.poll(), Poll::Lost));
        assert!(matches!(Job::done(1).poll(), Poll::Ready(1)));
    }

    #[test]
    fn dropping_a_job_cancels_its_work() {
        let stopped = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&stopped);
        let job = Job::spawn(move |cancel| {
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            seen.store(true, Ordering::Relaxed);
        });
        assert!(matches!(job.poll(), Poll::Waiting));
        drop(job);
        assert!(eventually(|| stopped.load(Ordering::Relaxed)));
    }

    #[test]
    fn a_thread_that_dies_is_an_error() {
        let job: Job<Result<(), String>> = Job::spawn(|_| panic!("boom"));
        assert!(eventually(|| !matches!(job.poll(), Poll::Waiting)));
        assert_eq!(job.answer(), Some(Err(LOST.to_string())));
    }
}

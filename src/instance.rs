//! One iboard at a time. The running app answers on `iboard.sock`, in the
//! cache folder: a later `iboard` hands it the board to show, or only asks it
//! to come forward, then leaves without opening a window of its own. Outside
//! Unix there is no socket, and each iboard opens its own window.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};

use crate::settings;

const SOCKET: &str = "iboard.sock";

/// What a later `iboard` asks of the running one, sent as a line of JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Request {
    /// Come forward, on the board shown.
    Focus,
    /// Show this project.
    Project(PathBuf),
    /// Show this group's board.
    Group(String),
}

#[cfg(unix)]
pub use unix::Listener;

/// The socket an iboard answers on: there is none outside Unix.
#[cfg(not(unix))]
pub enum Listener {}

/// Where the running iboard answers, or `None` when neither `XDG_CACHE_HOME`
/// nor `HOME` is set.
pub fn socket() -> Option<PathBuf> {
    settings::cache_dir().map(|dir| dir.join(SOCKET))
}

/// Hands `request` to the iboard answering on `socket`: true once it took it,
/// false when none answers there.
pub fn hand_over(socket: &Path, request: &Request) -> bool {
    #[cfg(unix)]
    {
        unix::hand_over(socket, request)
    }
    #[cfg(not(unix))]
    {
        let _ = (socket, request);
        false
    }
}

/// Answers on `socket` from now on, taking it over from an iboard that did not
/// quit cleanly. `None` when another iboard answers there, or the socket can't
/// be made: this one then runs on its own.
pub fn listen(socket: &Path) -> Option<Listener> {
    #[cfg(unix)]
    {
        unix::listen(socket)
    }
    #[cfg(not(unix))]
    {
        let _ = socket;
        None
    }
}

/// Takes the requests that arrive on `listener`, on a thread of its own: each
/// goes down the returned channel, then `wake` has the window look at it.
pub fn serve(listener: Listener, wake: impl Fn() + Send + 'static) -> Receiver<Request> {
    #[cfg(unix)]
    {
        unix::serve(listener, wake)
    }
    #[cfg(not(unix))]
    {
        let _ = wake;
        match listener {}
    }
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, ErrorKind, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::sync::mpsc::{Receiver, channel};
    use std::time::Duration;

    pub use std::os::unix::net::UnixListener as Listener;

    use super::Request;

    /// How long a later iboard waits for the running one to take its request,
    /// and the running one for a request to arrive.
    const PATIENCE: Duration = Duration::from_secs(3);

    pub fn hand_over(socket: &Path, request: &Request) -> bool {
        let (Ok(mut stream), Ok(line)) =
            (UnixStream::connect(socket), serde_json::to_string(request))
        else {
            return false;
        };
        let mut answer = String::new();
        stream.set_read_timeout(Some(PATIENCE)).is_ok()
            && writeln!(stream, "{line}").is_ok()
            && BufReader::new(stream).read_line(&mut answer).is_ok()
            && answer == "ok\n"
    }

    pub fn listen(socket: &Path) -> Option<Listener> {
        std::fs::create_dir_all(socket.parent()?).ok()?;
        let listener = match Listener::bind(socket) {
            Ok(listener) => listener,
            Err(err)
                if err.kind() == ErrorKind::AddrInUse && UnixStream::connect(socket).is_err() =>
            {
                std::fs::remove_file(socket).ok()?;
                Listener::bind(socket).ok()?
            }
            Err(_) => return None,
        };
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600)).ok()?;
        Some(listener)
    }

    pub fn serve(listener: Listener, wake: impl Fn() + Send + 'static) -> Receiver<Request> {
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let Some(request) = read(&stream) else {
                    continue;
                };
                if sender.send(request).is_err() {
                    return;
                }
                wake();
                let _ = (&stream).write_all(b"ok\n");
            }
        });
        receiver
    }

    fn read(stream: &UnixStream) -> Option<Request> {
        stream.set_read_timeout(Some(PATIENCE)).ok()?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).ok()?;
        serde_json::from_str(&line).ok()
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    /// Where a test's iboard answers: in a fresh folder, removed when it drops.
    fn place() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("iboard").join(SOCKET);
        (dir, socket)
    }

    #[test]
    fn a_later_iboard_hands_its_request_to_the_running_one() {
        let (_dir, socket) = place();
        assert!(!hand_over(&socket, &Request::Focus), "none runs yet");

        let woken = Arc::new(AtomicUsize::new(0));
        let requests = serve(listen(&socket).unwrap(), {
            let woken = woken.clone();
            move || {
                woken.fetch_add(1, Ordering::SeqCst);
            }
        });
        let mode = std::fs::metadata(&socket).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "only its user can reach it");
        for request in [
            Request::Focus,
            Request::Project(PathBuf::from("/code/app")),
            Request::Group("Clients".to_string()),
        ] {
            assert!(hand_over(&socket, &request), "{request:?}");
            let taken = requests.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(taken, request);
        }
        assert_eq!(woken.load(Ordering::SeqCst), 3, "each one wakes the window");
        assert!(
            listen(&socket).is_none(),
            "a second iboard leaves the socket to the first"
        );
    }

    #[test]
    fn a_socket_left_by_an_iboard_gone_is_taken_over() {
        let (_dir, socket) = place();
        drop(listen(&socket).unwrap());
        assert!(socket.exists(), "quitting leaves the socket behind");
        assert!(!hand_over(&socket, &Request::Focus), "nobody answers it");

        let requests = serve(listen(&socket).unwrap(), || {});
        assert!(hand_over(&socket, &Request::Focus));
        let taken = requests.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(taken, Request::Focus);
    }
}

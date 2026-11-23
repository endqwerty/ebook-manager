//! One helper process in its sandbox, kept for as many requests as its owner
//! makes. A thread of its own starts the process and lives until the process
//! is collected, because the kernel's parent-death signal follows the thread
//! that started a child, not the process.

use crate::error::ReaderError;
use crate::host::{from_wire, Inner};
use crate::wire::{
    read_frame, FrameError, Payload, Request, Response, BOOK_IN_SANDBOX, HELPER_IN_SANDBOX,
};
use std::ffi::OsString;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Read-only system library directories the helper's loader may need.
const LIBRARY_DIRS: [&str; 4] = ["/usr/lib", "/usr/lib64", "/lib", "/lib64"];
/// The most of the helper's error stream kept for a failure's detail.
const NOTE_BYTES: usize = 8192;

/// A reply: what the helper answered, the bytes after it included.
pub type Answer = Result<(Box<Payload>, Vec<u8>), ReaderError>;

/// What became of one request.
enum Outcome {
    /// The helper answered, and is still running.
    Answered(Answer),
    /// The helper ended or was ended, with this as the reason.
    Ended(ReaderError),
}

struct Job {
    request: Request,
    done: Sender<Outcome>,
}

/// A running helper. Dropping it ends the helper.
#[derive(Debug)]
pub struct Session {
    jobs: Sender<Job>,
    /// The helper answered a request before, so a later failure may come
    /// from what the session as a whole used up rather than from one request.
    answered: bool,
}

/// A request's result, and whether the session that made it can be kept.
pub enum Reply {
    Kept(Answer),
    /// The session is gone. `retry` says a fresh one may succeed.
    Lost {
        error: ReaderError,
        retry: bool,
    },
}

impl Session {
    /// Start a helper and wait until it says its sandbox holds.
    pub fn start(inner: &Arc<Inner>, book: Option<&Path>) -> Result<Session, ReaderError> {
        let (jobs, inbox) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let inner = inner.clone();
        let book = book.map(Path::to_owned);
        std::thread::Builder::new()
            .name("ebook-reader-session".into())
            .spawn(move || own(&inner, book, &ready, &inbox))
            .map_err(|e| ReaderError::HelperFailed {
                detail: format!("cannot start a session thread: {e}"),
            })?;
        match started.recv() {
            Ok(Ok(())) => Ok(Session {
                jobs,
                answered: false,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ReaderError::HelperFailed {
                detail: "the session ended while starting".into(),
            }),
        }
    }

    pub fn ask(&mut self, request: Request) -> Reply {
        let (done, outcome) = mpsc::channel();
        let gone = || ReaderError::HelperFailed {
            detail: "the session ended".into(),
        };
        let outcome = match self.jobs.send(Job { request, done }) {
            Ok(()) => outcome.recv().unwrap_or_else(|_| Outcome::Ended(gone())),
            Err(_) => Outcome::Ended(gone()),
        };
        match outcome {
            Outcome::Answered(answer) => {
                self.answered = true;
                Reply::Kept(answer)
            }
            Outcome::Ended(error) => {
                let retry = self.answered && !matches!(error, ReaderError::Timeout);
                Reply::Lost { error, retry }
            }
        }
    }
}

/// One request in a session of its own.
pub fn once(inner: &Arc<Inner>, book: &Path, request: Request) -> Answer {
    match Session::start(inner, Some(book))?.ask(request) {
        Reply::Kept(answer) => answer,
        Reply::Lost { error, .. } => Err(error),
    }
}

fn command(inner: &Inner, book: Option<&Path>) -> Command {
    let mut cmd = Command::new(&inner.bwrap);
    cmd.args([
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
    ]);
    cmd.args(["--cap-drop", "ALL"]);
    for dir in LIBRARY_DIRS {
        cmd.args(["--ro-bind-try", dir, dir]);
    }
    cmd.arg("--ro-bind")
        .arg(&inner.helper)
        .arg(HELPER_IN_SANDBOX);
    if let Some(book) = book {
        cmd.arg("--ro-bind").arg(book).arg(BOOK_IN_SANDBOX);
    }
    cmd.args(["--remount-ro", "/", "--chdir", "/", "--", HELPER_IN_SANDBOX]);
    let cpu = inner.timeout.as_secs().saturating_add(2).to_string();
    let args: [OsString; 3] = [
        "session".into(),
        inner.limits_json.clone().into(),
        cpu.into(),
    ];
    cmd.args(args);
    cmd.env_clear();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::spawn::confine(&mut cmd);
    cmd
}

type Frame = Result<(Response, Vec<u8>), FrameError>;

/// The session thread: start the helper, report whether it is ready, then
/// pass it each job until the owner lets go or the helper ends.
fn own(
    inner: &Inner,
    book: Option<PathBuf>,
    ready: &Sender<Result<(), ReaderError>>,
    inbox: &Receiver<Job>,
) {
    let mut child = match command(inner, book.as_deref()).spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = ready.send(Err(ReaderError::SandboxUnavailable {
                reason: format!("cannot start bwrap: {e}"),
            }));
            return;
        }
    };
    let mut running = match Running::new(&mut child, inner) {
        Some(running) => running,
        None => {
            stop(&mut child);
            let _ = ready.send(Err(ReaderError::HelperFailed {
                detail: "no pipe to the helper".into(),
            }));
            return;
        }
    };
    let first = running.next(inner.timeout);
    let started = match first {
        Ok(Ok((Response::Ok(Payload::Probe(report)), blob)))
            if report.isolated && blob.is_empty() =>
        {
            Ok(())
        }
        Ok(Ok((Response::Err(e), _))) => Err(from_wire(e)),
        Ok(Ok(_)) => Err(ReaderError::SandboxUnavailable {
            reason: "unexpected probe reply".into(),
        }),
        Ok(Err(frame)) => Err(running.ended(&mut child, frame, inner.timeout)),
        Err(()) => Err(ReaderError::Timeout),
    };
    let failed = started.is_err();
    let _ = ready.send(started);
    if failed {
        stop(&mut child);
        return;
    }
    while let Ok(job) = inbox.recv() {
        let outcome = running.serve(&mut child, job.request, inner);
        let ended = matches!(outcome, Outcome::Ended(_));
        let _ = job.done.send(outcome);
        if ended {
            break;
        }
    }
    stop(&mut child);
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

struct Running {
    input: ChildStdin,
    frames: Receiver<Frame>,
    note: Arc<Mutex<Vec<u8>>>,
    note_done: Receiver<()>,
}

impl Running {
    fn new(child: &mut Child, inner: &Inner) -> Option<Running> {
        let input = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let mut stderr = child.stderr.take()?;
        let max = inner.limits.max_output_bytes;
        let (send, frames) = mpsc::channel();
        std::thread::Builder::new()
            .name("ebook-reader-replies".into())
            .spawn(move || {
                let mut stdout = BufReader::with_capacity(256 * 1024, stdout);
                loop {
                    let frame = read_frame(&mut stdout, max);
                    let last = frame.is_err();
                    if send.send(frame).is_err() || last {
                        return;
                    }
                }
            })
            .ok()?;
        let note = Arc::new(Mutex::new(Vec::new()));
        let kept = note.clone();
        let (finished, note_done) = mpsc::channel();
        std::thread::Builder::new()
            .name("ebook-reader-errors".into())
            .spawn(move || {
                let mut chunk = [0u8; 1024];
                while let Ok(n @ 1..) = stderr.read(&mut chunk) {
                    let mut note = kept.lock().unwrap_or_else(|p| p.into_inner());
                    let room = NOTE_BYTES.saturating_sub(note.len()).min(n);
                    note.extend_from_slice(&chunk[..room]);
                }
                let _ = finished.send(());
            })
            .ok()?;
        Some(Running {
            input,
            frames,
            note,
            note_done,
        })
    }

    /// The next reply, or `Err(())` when none came in time.
    fn next(&self, timeout: Duration) -> Result<Frame, ()> {
        match self.frames.recv_timeout(timeout) {
            Ok(frame) => Ok(frame),
            Err(RecvTimeoutError::Timeout) => Err(()),
            Err(RecvTimeoutError::Disconnected) => Ok(Err(FrameError::Closed)),
        }
    }

    fn serve(&mut self, child: &mut Child, request: Request, inner: &Inner) -> Outcome {
        let sent = self
            .input
            .write_all(request.line().as_bytes())
            .and_then(|()| self.input.flush());
        if sent.is_err() {
            return Outcome::Ended(self.ended(child, FrameError::Closed, inner.timeout));
        }
        match self.next(inner.timeout) {
            Ok(Ok((Response::Ok(payload), blob))) => Outcome::Answered(Ok((Box::new(payload), blob))),
            Ok(Ok((Response::Err(e), _))) => Outcome::Answered(Err(from_wire(e))),
            Ok(Err(frame)) => Outcome::Ended(self.ended(child, frame, inner.timeout)),
            Err(()) => Outcome::Ended(ReaderError::Timeout),
        }
    }

    /// The helper's replies stopped. Say why, from its exit status and the
    /// start of what it wrote on its error stream.
    fn ended(&self, child: &mut Child, frame: FrameError, timeout: Duration) -> ReaderError {
        if frame == FrameError::TooLarge {
            return ReaderError::LimitExceeded {
                what: "size of the reader's reply".into(),
            };
        }
        if frame == FrameError::Malformed {
            let _ = child.kill();
        }
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.to_string(),
                Ok(None) if Instant::now() >= deadline => return ReaderError::Timeout,
                Ok(None) => std::thread::sleep(Duration::from_millis(1)),
                Err(e) => {
                    return ReaderError::HelperFailed {
                        detail: format!("waiting for helper: {e}"),
                    }
                }
            }
        };
        let _ = self.note_done.recv_timeout(Duration::from_millis(200));
        let raw = self.note.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut note = String::from_utf8_lossy(&raw).trim().to_owned();
        note.retain(|c| !c.is_control());
        if let Some((cut, _)) = note.char_indices().nth(300) {
            note.truncate(cut);
        }
        // bwrap reports its own setup failures as "bwrap: ..." before the helper runs.
        if note.starts_with("bwrap:") {
            return ReaderError::SandboxUnavailable { reason: note };
        }
        ReaderError::HelperFailed {
            detail: format!("{status}; {note}"),
        }
    }
}

use super::*;
use ebook_reader::{Reader, ReaderConfig};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[derive(Clone, Debug)]
pub struct Config {
    pub helper: PathBuf,
    pub reader: ReaderConfig,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub source: PathBuf,
    pub identity: String,
    pub destination: PathBuf,
    pub metadata: Metadata,
    pub cover: Option<Cover>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Preparing,
    Converting,
    Checking,
    Publishing,
    Finished,
}

#[derive(Debug)]
struct State {
    stage: Stage,
    cancelled: bool,
}
type Shared = Arc<Mutex<State>>;

pub struct Job {
    state: Shared,
    worker: JoinHandle<Result<Converted, Error>>,
}
impl Job {
    pub fn stage(&self) -> Stage {
        self.state.lock().unwrap().stage
    }
    pub fn cancel(&self) {
        self.state.lock().unwrap().cancelled = true;
    }
    pub fn wait(self) -> Result<Converted, Error> {
        self.worker
            .join()
            .map_err(|_| Error::Io("job thread stopped".into()))?
    }
}
fn advance(state: &Shared, stage: Stage) -> Result<(), Error> {
    let mut state = state.lock().unwrap();
    if state.cancelled {
        return Err(Error::Cancelled);
    }
    state.stage = stage;
    Ok(())
}
pub fn start(request: Request, config: Config) -> Job {
    let state = Arc::new(Mutex::new(State {
        stage: Stage::Preparing,
        cancelled: false,
    }));
    let worker_state = state.clone();
    let worker = thread::spawn(move || {
        let result = convert(request, config, &worker_state);
        worker_state.lock().unwrap().stage = Stage::Finished;
        result
    });
    Job { state, worker }
}

fn snapshot(path: &Path, output: &Path) -> Result<(), Error> {
    let mut input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let before = input.metadata()?;
    if !before.is_file() {
        return Err(Error::Invalid("input must be a regular file".into()));
    }
    if before.len() > INPUT_LIMIT {
        return Err(Error::Limit);
    }
    let count = std::io::copy(
        &mut (&mut input).take(INPUT_LIMIT + 1),
        &mut File::create(output)?,
    )?;
    let after = input.metadata()?;
    if count != before.len()
        || count != after.len()
        || (before.ctime(), before.ctime_nsec()) != (after.ctime(), after.ctime_nsec())
    {
        return Err(Error::Invalid("source changed while copying".into()));
    }
    Ok(())
}

fn convert(req: Request, config: Config, state: &Shared) -> Result<Converted, Error> {
    let private = tempfile::tempdir()?;
    let dir = private.path();
    snapshot(&req.source, &dir.join("source"))?;
    let json = serde_json::to_vec(&req.metadata).map_err(|_| Error::Limit)?;
    if json.len() > STATUS_LIMIT {
        return Err(Error::Limit);
    }
    std::fs::write(dir.join("metadata"), json)?;
    if let Some(cover) = req.cover {
        let (name, bytes) = match cover {
            Cover::Png(b) => ("cover.png", b),
            Cover::Jpeg(b) => ("cover.jpg", b),
        };
        if bytes.len() > 16 << 20 {
            return Err(Error::Limit);
        }
        std::fs::write(dir.join(name), bytes)?;
    }
    advance(state, Stage::Converting)?;
    let reply = run_helper(dir, &config.helper, state)?;
    let prefix = reply.first_chunk::<4>().ok_or_else(bad_reply)?;
    let end = 4 + usize::try_from(u32::from_be_bytes(*prefix)).map_err(|_| bad_reply())?;
    if end > STATUS_LIMIT + 4 {
        return Err(bad_reply());
    }
    let header = reply.get(4..end).ok_or_else(bad_reply)?;
    let result: Result<Vec<String>, Error> =
        serde_json::from_slice(header).map_err(|_| bad_reply())?;
    let valid = |s: &str| s.len() <= 512 && !s.chars().any(char::is_control);
    let losses = result.map_err(|err| match &err {
        Error::Protected | Error::Limit => err,
        Error::Invalid(s) | Error::Unsupported(s) | Error::Sandbox(s) | Error::Io(s)
            if valid(s) =>
        {
            err
        }
        _ => bad_reply(),
    })?;
    if losses.len() > 128 || !losses.iter().all(|s| valid(s)) {
        return Err(bad_reply());
    }
    let bytes = &reply[end..];
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(Error::Limit);
    }
    let parent = req
        .destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    let size = bytes.len() as u64;
    drop(reply);
    advance(state, Stage::Checking)?;
    let validation = |e: ebook_reader::ReaderError| Error::Validation(e.to_string());
    let reader = Reader::new(config.reader).map_err(validation)?;
    let book = reader.open(staged.path()).map_err(validation)?;
    if book.chapters().len() > 512 {
        return Err(Error::Limit);
    }
    for chapter in book.chapters() {
        advance(state, Stage::Checking)?;
        let page = book.read_chapter(chapter.index).map_err(validation)?;
        if !page.warnings.is_empty() {
            return Err(Error::Validation(
                "reader reported a chapter warning".into(),
            ));
        }
        if page.truncated {
            return Err(Error::Validation("chapter exceeds reader limits".into()));
        }
    }
    staged.as_file().sync_all()?;
    advance(state, Stage::Publishing)?;
    let gate = state.lock().unwrap();
    if gate.cancelled {
        return Err(Error::Cancelled);
    }
    staged.persist_noclobber(&req.destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Exists
        } else {
            e.error.into()
        }
    })?;
    Ok(Converted {
        path: req.destination,
        report: Report {
            identity: req.identity,
            direction: "mobi-to-epub".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            size,
            losses,
        },
    })
}
fn bad_reply() -> Error {
    Error::Invalid("invalid converter reply".into())
}
fn run_helper(dir: &Path, helper: &Path, state: &Shared) -> Result<Vec<u8>, Error> {
    let mut cmd = Command::new("/usr/bin/bwrap");
    cmd.args([
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
        "--cap-drop",
        "ALL",
    ]);
    for lib in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        cmd.args(["--ro-bind-try", lib, lib]);
    }
    cmd.arg("--ro-bind")
        .arg(helper.canonicalize()?)
        .arg("/converter")
        .arg("--ro-bind")
        .arg(dir)
        .args([
            "/job",
            "--remount-ro",
            "/",
            "--chdir",
            "/",
            "--",
            "/converter",
        ])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    super::spawn::confine(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| Error::Sandbox(e.to_string()))?;
    let mut stdout = child.stdout.take().unwrap();
    let cap = OUTPUT_LIMIT + STATUS_LIMIT as u64 + 4;
    let collecting = thread::spawn(move || {
        let mut reply = Vec::new();
        (&mut stdout).take(cap + 1).read_to_end(&mut reply)?;
        if reply.len() as u64 > cap {
            return Err(Error::Limit);
        }
        Ok(reply)
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if state.lock().unwrap().cancelled {
            break Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            break Err(Error::Deadline);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => break Err(e.into()),
        }
    };
    if status.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let reply = collecting.join().map_err(|_| bad_reply())?;
    let status = status?;
    let reply = reply?;
    if status.code() == Some(4) {
        return Err(Error::ConversionStopped);
    }
    if !status.success() {
        return Err(Error::Sandbox(
            "conversion helper could not complete".into(),
        ));
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e3_cancel_before_conversion() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::write(&source, b"unchanged").unwrap();
        let destination = dir.path().join("output");
        let request = Request {
            source: source.clone(),
            destination: destination.clone(),
            identity: "v1".into(),
            metadata: Metadata::default(),
            cover: None,
        };
        let config = Config {
            helper: dir.path().join("never-started"),
            reader: ReaderConfig::default(),
        };
        let state = Arc::new(Mutex::new(State {
            stage: Stage::Preparing,
            cancelled: false,
        }));
        let (resume, paused) = std::sync::mpsc::channel();
        let worker_state = state.clone();
        let worker = thread::spawn(move || {
            paused.recv().unwrap();
            convert(request, config, &worker_state)
        });
        let job = Job { state, worker };
        job.cancel();
        resume.send(()).unwrap();
        assert!(matches!(job.wait(), Err(Error::Cancelled)));
        assert!(!destination.exists());
        assert_eq!(std::fs::read(source).unwrap(), b"unchanged");
    }
}

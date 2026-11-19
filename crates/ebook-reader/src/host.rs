//! The host side: find the helper, start it in a sandbox, bound it, and turn
//! its reply into checked values. No book byte is parsed in this process.

use crate::error::{ReaderError, Unsupported};
use crate::model::{Chapter, ChapterInfo, Limits, Link, Metadata, Position, TocEntry};
use crate::wire::{
    BookOutline, Payload, Response, WireError, WireErrorKind, BOOK_IN_SANDBOX, HELPER_IN_SANDBOX,
};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// File name of the helper program.
pub const HELPER_NAME: &str = "ebook-reader-helper";
/// Environment variable that names the helper by absolute path.
pub const HELPER_ENV: &str = "EBOOK_READER_HELPER";

/// Deepest contents nesting the helper produces, counted from 0.
const MAX_TOC_DEPTH: u32 = 32;
const BWRAP_PLACES: [&str; 2] = ["/usr/bin/bwrap", "/bin/bwrap"];
/// Read-only system library directories the helper's loader may need.
const LIBRARY_DIRS: [&str; 4] = ["/usr/lib", "/usr/lib64", "/lib", "/lib64"];

#[derive(Debug, Clone)]
pub struct ReaderConfig {
    /// Use this helper instead of searching the documented places.
    pub helper_path: Option<PathBuf>,
    /// Use this bubblewrap instead of `/usr/bin/bwrap` or `/bin/bwrap`.
    pub bwrap_path: Option<PathBuf>,
    pub limits: Limits,
    /// Wall-clock allowance for one call.
    pub timeout: Duration,
}

impl Default for ReaderConfig {
    fn default() -> Self {
        ReaderConfig {
            helper_path: None,
            bwrap_path: None,
            limits: Limits::default(),
            timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug)]
struct Inner {
    helper: PathBuf,
    bwrap: PathBuf,
    limits: Limits,
    limits_json: String,
    timeout: Duration,
}

/// Entry point. Cheap to clone and safe to share between threads.
#[derive(Debug, Clone)]
pub struct Reader {
    inner: Arc<Inner>,
}

impl Reader {
    /// Find the helper and bubblewrap, then prove the sandbox works by running
    /// the helper's isolation check inside it. Any failure is an error: there
    /// is no unsandboxed fallback.
    pub fn new(config: ReaderConfig) -> Result<Reader, ReaderError> {
        let helper = find_helper(config.helper_path.as_deref())?;
        let bwrap = find_bwrap(config.bwrap_path.as_deref())?;
        let limits_json =
            serde_json::to_string(&config.limits).map_err(|e| ReaderError::HelperFailed {
                detail: e.to_string(),
            })?;
        let reader = Reader {
            inner: Arc::new(Inner {
                helper,
                bwrap,
                limits: config.limits,
                limits_json,
                timeout: config.timeout,
            }),
        };
        match reader.run(None, &["probe"]) {
            Ok(Payload::Probe(report)) if report.isolated => Ok(reader),
            Ok(_) => Err(ReaderError::SandboxUnavailable {
                reason: "unexpected probe reply".into(),
            }),
            Err(ReaderError::SandboxUnavailable { reason }) => {
                Err(ReaderError::SandboxUnavailable { reason })
            }
            Err(other) => Err(ReaderError::SandboxUnavailable {
                reason: other.to_string(),
            }),
        }
    }

    /// Title, authors and identifiers for import. Reads only the package data.
    /// A protected book still returns its metadata, with `protected` set.
    pub fn inspect(&self, epub: &Path) -> Result<Metadata, ReaderError> {
        let source = Source::check(epub, &self.inner.limits)?;
        match self.run(Some(&source.path), &["metadata"])? {
            Payload::Metadata(mut m) => {
                scrub_metadata(&mut m);
                Ok(m)
            }
            _ => Err(unexpected()),
        }
    }

    /// Open a book for reading: metadata, reading order and contents.
    pub fn open(&self, epub: &Path) -> Result<Book, ReaderError> {
        let source = Source::check(epub, &self.inner.limits)?;
        match self.run(Some(&source.path), &["open"])? {
            Payload::Book(outline) => Book::new(self.clone(), source, outline),
            _ => Err(unexpected()),
        }
    }

    fn run(&self, book: Option<&Path>, op: &[&str]) -> Result<Payload, ReaderError> {
        let inner = &*self.inner;
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
        let mut args: Vec<OsString> = vec![op[0].into(), inner.limits_json.clone().into()];
        args.push(
            (inner.timeout.as_secs().saturating_add(2))
                .to_string()
                .into(),
        );
        args.extend(op[1..].iter().map(OsString::from));
        cmd.args(args);
        cmd.env_clear();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::spawn::confine(&mut cmd);

        let mut child = cmd.spawn().map_err(|e| ReaderError::SandboxUnavailable {
            reason: format!("cannot start bwrap: {e}"),
        })?;
        let outcome = collect(&mut child, inner.limits.max_output_bytes, inner.timeout);
        let _ = child.kill();
        let _ = child.wait();
        let (stdout, stderr, status) = outcome?;

        if let Ok(response) = serde_json::from_slice::<Response>(&stdout) {
            return match response {
                Response::Ok(payload) => Ok(payload),
                Response::Err(e) => Err(from_wire(e)),
            };
        }
        let mut note = String::from_utf8_lossy(&stderr).trim().to_owned();
        note.retain(|c| !c.is_control());
        if let Some((cut, _)) = note.char_indices().nth(300) {
            note.truncate(cut);
        }
        // bwrap reports its own setup failures as "bwrap: ..." before the helper runs.
        if stdout.is_empty() && note.starts_with("bwrap:") {
            return Err(ReaderError::SandboxUnavailable { reason: note });
        }
        Err(ReaderError::HelperFailed {
            detail: format!("{status}; {note}"),
        })
    }
}

type Collected = (Vec<u8>, Vec<u8>, String);

/// Read the helper's reply with a size cap and a deadline.
fn collect(
    child: &mut Child,
    max_output: u64,
    timeout: Duration,
) -> Result<Collected, ReaderError> {
    let now = Instant::now();
    let deadline = now
        .checked_add(timeout)
        .unwrap_or_else(|| now + Duration::from_secs(365 * 24 * 60 * 60));
    let failed = |what: &str| ReaderError::HelperFailed {
        detail: what.to_owned(),
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| failed("no output pipe"))?;
    let stderr = child.stderr.take().ok_or_else(|| failed("no error pipe"))?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stdout
            .take(max_output.saturating_add(1))
            .read_to_end(&mut buf)
            .map(|_| buf);
        let _ = tx.send(result);
    });
    let (etx, erx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(8192).read_to_end(&mut buf);
        let _ = etx.send(buf);
    });

    let remaining = |deadline: Instant| deadline.saturating_duration_since(Instant::now());
    let out = match rx.recv_timeout(remaining(deadline)) {
        Ok(Ok(buf)) => buf,
        Ok(Err(e)) => return Err(failed(&format!("reading reply: {e}"))),
        Err(_) => return Err(ReaderError::Timeout),
    };
    if out.len() as u64 > max_output {
        return Err(ReaderError::LimitExceeded {
            what: "size of the reader's reply".into(),
        });
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.to_string(),
            Ok(None) if Instant::now() >= deadline => return Err(ReaderError::Timeout),
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Err(e) => return Err(failed(&format!("waiting for helper: {e}"))),
        }
    };
    let err = erx
        .recv_timeout(Duration::from_millis(200))
        .unwrap_or_default();
    Ok((out, err, status))
}

fn unexpected() -> ReaderError {
    ReaderError::HelperFailed {
        detail: "unexpected reply".into(),
    }
}

fn from_wire(e: WireError) -> ReaderError {
    let mut detail = e.detail;
    scrub(&mut detail);
    match e.kind {
        WireErrorKind::NotEpub => ReaderError::Unsupported(Unsupported::NotEpub),
        WireErrorKind::Protected => ReaderError::Unsupported(Unsupported::Protected),
        WireErrorKind::Compression => ReaderError::Unsupported(Unsupported::Compression),
        WireErrorKind::Encoding => ReaderError::Unsupported(Unsupported::Encoding),
        WireErrorKind::Invalid => ReaderError::InvalidBook { detail },
        WireErrorKind::Limit => ReaderError::LimitExceeded { what: detail },
        WireErrorKind::ChapterOutOfRange => ReaderError::HelperFailed {
            detail: "chapter index".into(),
        },
        WireErrorKind::NotIsolated => ReaderError::SandboxUnavailable { reason: detail },
        WireErrorKind::Usage => ReaderError::HelperFailed { detail },
    }
}

fn find_helper(configured: Option<&Path>) -> Result<PathBuf, ReaderError> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = configured {
        candidates.push(p.to_owned());
    } else {
        // A development convenience. A release build takes its helper only
        // from configuration or from beside the installed program.
        if let Some(p) = std::env::var_os(HELPER_ENV)
            .map(PathBuf::from)
            .filter(|p| cfg!(debug_assertions) && p.is_absolute())
        {
            candidates.push(p);
        }
        if let Some(dir) = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(Path::parent)
        {
            candidates.push(dir.join(HELPER_NAME));
            candidates.push(dir.join("../libexec/ebook-manager").join(HELPER_NAME));
        }
    }
    for candidate in &candidates {
        if let Ok(real) = candidate.canonicalize() {
            if real.is_file() {
                return Ok(real);
            }
        }
    }
    Err(ReaderError::HelperNotFound {
        searched: candidates,
    })
}

fn find_bwrap(configured: Option<&Path>) -> Result<PathBuf, ReaderError> {
    let places: Vec<PathBuf> = match configured {
        Some(p) => vec![p.to_owned()],
        None => BWRAP_PLACES.iter().map(PathBuf::from).collect(),
    };
    places
        .iter()
        .find(|p| p.is_absolute() && p.is_file())
        .cloned()
        .ok_or_else(|| ReaderError::SandboxUnavailable {
            reason: "bubblewrap (bwrap) is not installed".into(),
        })
}

/// The file a book was opened from, with what is needed to notice a change.
#[derive(Debug, Clone)]
struct Source {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl Source {
    fn check(path: &Path, limits: &Limits) -> Result<Source, ReaderError> {
        let unreadable = |e: std::io::Error| ReaderError::SourceUnreadable {
            detail: e.kind().to_string(),
        };
        let path = path.canonicalize().map_err(unreadable)?;
        let meta = std::fs::metadata(&path).map_err(unreadable)?;
        if !meta.is_file() {
            return Err(ReaderError::SourceUnreadable {
                detail: "not a regular file".into(),
            });
        }
        // Fail here, with a clear reason, rather than inside the sandbox.
        std::fs::File::open(&path).map_err(unreadable)?;
        if meta.len() > limits.max_file_bytes {
            return Err(ReaderError::LimitExceeded {
                what: "file size".into(),
            });
        }
        Ok(Source {
            path,
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }

    fn unchanged(&self) -> bool {
        std::fs::metadata(&self.path)
            .map(|m| m.is_file() && m.len() == self.len && m.modified().ok() == self.modified)
            .unwrap_or(false)
    }
}

/// An opened book. Holds no file handle and no helper process: each chapter
/// read is a fresh, independent sandbox run, so a crashed read loses nothing.
#[derive(Debug, Clone)]
pub struct Book {
    reader: Reader,
    source: Source,
    metadata: Metadata,
    chapters: Vec<ChapterInfo>,
    toc: Vec<TocEntry>,
}

impl Book {
    /// The outline came from the helper, which read an untrusted file. Check
    /// it against what this crate promises its callers before handing it on.
    fn new(reader: Reader, source: Source, outline: BookOutline) -> Result<Book, ReaderError> {
        let BookOutline {
            mut metadata,
            mut chapters,
            mut toc,
        } = outline;
        let limits = &reader.inner.limits;
        if chapters.is_empty()
            || chapters.len() > limits.max_spine_items as usize
            || toc.len() > limits.max_toc_entries as usize
        {
            return Err(ReaderError::HelperFailed {
                detail: "inconsistent reply".into(),
            });
        }
        scrub_metadata(&mut metadata);
        let count = chapters.len() as u32;
        for (i, c) in chapters.iter_mut().enumerate() {
            c.index = i as u32;
            if let Some(t) = &mut c.title {
                scrub(t);
            }
        }
        for entry in &mut toc {
            entry.depth = entry.depth.min(MAX_TOC_DEPTH);
            scrub(&mut entry.title);
            if entry.target.as_ref().is_some_and(|t| t.chapter >= count) {
                entry.target = None;
            }
            if let Some(a) = entry.target.as_mut().and_then(|t| t.anchor.as_mut()) {
                scrub(a);
            }
        }
        Ok(Book {
            reader,
            source,
            metadata,
            chapters,
            toc,
        })
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Every reading-order item, in order. Never empty.
    pub fn chapters(&self) -> &[ChapterInfo] {
        &self.chapters
    }

    /// Contents entries flattened in order. Empty when the book has none; use
    /// [`Book::chapters`] to navigate then.
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    /// Read one chapter's text. Fails with [`ReaderError::SourceChanged`] when
    /// the file is no longer the one that was opened.
    pub fn read_chapter(&self, index: u32) -> Result<Chapter, ReaderError> {
        let count = self.chapters.len() as u32;
        if index >= count {
            return Err(ReaderError::ChapterOutOfRange { index, count });
        }
        if !self.source.unchanged() {
            return Err(ReaderError::SourceChanged);
        }
        let n = index.to_string();
        match self.reader.run(Some(&self.source.path), &["chapter", &n])? {
            Payload::Chapter(mut chapter) => {
                chapter.index = index;
                self.scrub_chapter(&mut chapter);
                Ok(chapter)
            }
            _ => Err(unexpected()),
        }
    }

    /// Bring a saved position into this book's chapter range: a chapter past
    /// the end becomes the start of the last chapter. The block is checked
    /// against the chapter once it is read, with [`Chapter::clamp_block`].
    pub fn clamp(&self, pos: Position) -> Position {
        let last = self.chapters.len().saturating_sub(1) as u32;
        if pos.chapter > last {
            Position {
                chapter: last,
                block: 0,
            }
        } else {
            pos
        }
    }

    fn scrub_chapter(&self, chapter: &mut Chapter) {
        let count = self.chapters.len() as u32;
        if let Some(t) = &mut chapter.title {
            scrub(t);
        }
        let max_blocks = self.reader.inner.limits.max_blocks_per_chapter as usize;
        if chapter.blocks.len() > max_blocks {
            chapter.blocks.truncate(max_blocks);
            chapter.truncated = true;
        }
        for block in &mut chapter.blocks {
            if let crate::model::BlockKind::Heading { level } = &mut block.kind {
                *level = (*level).clamp(1, 6);
            }
            block.anchors.iter_mut().for_each(scrub);
            for span in &mut block.spans {
                scrub_text(&mut span.text);
                let keep = match &mut span.link {
                    Some(Link::Internal(t)) => {
                        if let Some(a) = &mut t.anchor {
                            scrub(a);
                        }
                        t.chapter < count
                    }
                    Some(Link::External(url)) => {
                        let lower = url.to_ascii_lowercase();
                        ["http://", "https://", "mailto:"]
                            .iter()
                            .any(|p| lower.starts_with(p))
                            && !url.chars().any(|c| c.is_control() || c.is_whitespace())
                    }
                    None => true,
                };
                if !keep {
                    span.link = None;
                }
            }
        }
    }
}

/// The helper already cleans its output. The host repeats the cheap part so a
/// misbehaving helper still cannot hand control characters to the interface.
fn scrub(s: &mut String) {
    if s.chars().any(char::is_control) {
        s.retain(|c| !c.is_control());
    }
}

fn scrub_text(s: &mut String) {
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        s.retain(|c| !c.is_control() || c == '\n' || c == '\t');
    }
}

fn scrub_metadata(m: &mut Metadata) {
    if let Some(t) = &mut m.title {
        scrub(t);
    }
    for a in &mut m.authors {
        scrub(&mut a.name);
        if let Some(s) = &mut a.sort_name {
            scrub(s);
        }
    }
    for i in &mut m.identifiers {
        scrub(&mut i.value);
        if let Some(s) = &mut i.scheme {
            scrub(s);
        }
    }
    m.languages.iter_mut().for_each(scrub);
    for s in &mut m.series {
        scrub(&mut s.name);
        if let Some(p) = &mut s.position {
            scrub(p);
        }
    }
    if let Some(p) = &mut m.published {
        scrub(p);
    }
    if let Some(v) = &mut m.epub_version {
        scrub(v);
    }
}

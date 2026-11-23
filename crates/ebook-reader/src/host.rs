//! The host side: find the helper, start it in a sandbox, bound it, and turn
//! its reply into checked values. No book byte is parsed in this process.

use crate::error::{ReaderError, Unsupported};
use crate::model::{
    Chapter, ChapterContent, ChapterInfo, Limits, Link, Metadata, Position, Resource, ResourceId,
    ResourceKind, Survey, SurveyedCover, Thumbnail, TocEntry,
};
use crate::session::{self, Reply, Session};
use crate::wire::{
    markup_text_units, BookOutline, Payload, Request, WireError, WireErrorKind, WireSurvey,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// File name of the helper program.
pub const HELPER_NAME: &str = "ebook-reader-helper";
/// Environment variable that names the helper by absolute path.
pub const HELPER_ENV: &str = "EBOOK_READER_HELPER";

/// Deepest contents nesting the helper produces, counted from 0.
const MAX_TOC_DEPTH: u32 = 32;
const BWRAP_PLACES: [&str; 2] = ["/usr/bin/bwrap", "/bin/bwrap"];

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
pub(crate) struct Inner {
    pub(crate) helper: PathBuf,
    pub(crate) bwrap: PathBuf,
    pub(crate) limits: Limits,
    pub(crate) limits_json: String,
    pub(crate) timeout: Duration,
}

impl Inner {
    /// When a call that starts now must have its answer. A timeout too long
    /// to add is a year, which is as good as none.
    pub(crate) fn deadline(&self) -> Instant {
        let now = Instant::now();
        now.checked_add(self.timeout)
            .unwrap_or_else(|| now + Duration::from_secs(365 * 24 * 60 * 60))
    }
}

/// Helper, bubblewrap and limits that have already shown a working sandbox in
/// this process. Every session checks its sandbox again as it starts, so this
/// only spares a reader a second check before its first call.
/// Each configuration has its own flag, so a slow check of one holds up only
/// the readers that wait on that one.
static PROVEN: Mutex<Option<HashMap<ProbeKey, Arc<Mutex<bool>>>>> = Mutex::new(None);

type ProbeKey = (PathBuf, PathBuf, String);

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
        let key = (
            reader.inner.helper.clone(),
            reader.inner.bwrap.clone(),
            reader.inner.limits_json.clone(),
        );
        let flag = PROVEN
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_or_insert_with(HashMap::new)
            .entry(key)
            .or_default()
            .clone();
        // Held through the check, so callers that start together prove one
        // configuration once.
        let mut proven = flag.lock().unwrap_or_else(|p| p.into_inner());
        if *proven {
            return Ok(reader);
        }
        match Session::start(&reader.inner, None, reader.inner.deadline()) {
            Ok(_) => {
                *proven = true;
                Ok(reader)
            }
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
        match self.once(&source, Request::Metadata)? {
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
        let deadline = self.inner.deadline();
        let mut session = Session::start(&self.inner, Some(&source.path), deadline)?;
        match session.ask(Request::Open, deadline) {
            Reply::Kept(Ok((payload, blob))) => match *payload {
                Payload::Book(outline) if blob.is_empty() => {
                    Book::new(self.clone(), source, outline, session)
                }
                _ => Err(unexpected()),
            },
            Reply::Kept(Err(error)) | Reply::Lost { error, .. } => Err(error),
        }
    }

    /// The book's length in words and its cover as a small picture, for
    /// browsing. Reads every linear chapter, so it takes longer than
    /// [`Reader::inspect`]. A missing or unusable cover and an uncountable
    /// text are `None` in the result, not errors; an error means the helper
    /// gave no answer and asking again later may work.
    pub fn survey(&self, epub: &Path) -> Result<Survey, ReaderError> {
        let source = Source::check(epub, &self.inner.limits)?;
        let deadline = self.inner.deadline();
        let (payload, blob) = session::once(&self.inner, &source.path, Request::Survey, deadline)?;
        match *payload {
            Payload::Survey(survey) => checked_survey(survey, blob),
            _ => Err(unexpected()),
        }
    }

    /// A request that sends no bytes after its reply, in a session of its own.
    fn once(&self, source: &Source, request: Request) -> Result<Payload, ReaderError> {
        match session::once(&self.inner, &source.path, request, self.inner.deadline())? {
            (payload, blob) if blob.is_empty() => Ok(*payload),
            _ => Err(unexpected()),
        }
    }
}

/// The helper decoded the cover from an untrusted file. Its reply is held to
/// the thumbnail's shape before any caller sees the pixels.
fn checked_survey(survey: WireSurvey, pixels: Vec<u8>) -> Result<Survey, ReaderError> {
    let cover = match survey.cover {
        None if pixels.is_empty() => None,
        None => return Err(unexpected()),
        Some(c) => Some(SurveyedCover {
            thumbnail: Thumbnail::new(c.width, c.height, pixels).ok_or_else(|| {
                ReaderError::HelperFailed {
                    detail: "inconsistent cover".into(),
                }
            })?,
            average: c.average,
            spine: c.spine,
        }),
    };
    Ok(Survey {
        words: survey.words,
        cover,
    })
}

fn unexpected() -> ReaderError {
    ReaderError::HelperFailed {
        detail: "unexpected reply".into(),
    }
}

pub(crate) fn from_wire(e: WireError) -> ReaderError {
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

/// An opened book. Holds one sandboxed helper for its reads, which ends when
/// the last clone of the book is dropped. A helper that fails is ended and
/// the next read starts another, so a crashed read loses nothing. A read that
/// would wait long for that helper uses a short-lived one of its own.
#[derive(Debug, Clone)]
pub struct Book {
    reader: Reader,
    source: Source,
    metadata: Metadata,
    chapters: Vec<ChapterInfo>,
    toc: Vec<TocEntry>,
    helper: Arc<Shared>,
}

/// How long a call waits for its book's helper before using one of its own.
/// A picture or a chapter takes well under a millisecond, so this is a
/// queue of dozens; a read held up longer than this is slow, not queued.
const SHARE_WAIT: Duration = Duration::from_millis(50);
/// Most helpers of their own one book's calls run at once. Past this, a call
/// waits for the book's helper or a spare until its deadline.
const MAX_SPARES: usize = 4;

/// A book's helper, used by one call at a time.
#[derive(Debug)]
struct Shared {
    slot: Mutex<Slot>,
    freed: Condvar,
}

#[derive(Debug)]
struct Slot {
    session: Option<Session>,
    busy: bool,
    /// Calls running in helpers of their own.
    spares: usize,
}

/// What a call gets to run in.
enum Taken<'a> {
    Helper(Turn<'a>),
    /// A place for a helper of its own, given back when dropped.
    Own(Spare<'a>),
    /// Neither came free before the deadline.
    Nothing,
}

impl Shared {
    /// The book's helper once no other call is using it. After
    /// [`SHARE_WAIT`], a place for a helper of the call's own when fewer
    /// than [`MAX_SPARES`] are running, else whichever comes free first.
    fn take(&self, deadline: Instant) -> Taken<'_> {
        let share_until = (Instant::now() + SHARE_WAIT).min(deadline);
        let mut slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if Instant::now() >= deadline {
                return Taken::Nothing;
            }
            if !slot.busy {
                slot.busy = true;
                return Taken::Helper(Turn {
                    shared: self,
                    session: slot.session.take(),
                });
            }
            let now = Instant::now();
            if now >= share_until && slot.spares < MAX_SPARES {
                slot.spares += 1;
                return Taken::Own(Spare { shared: self });
            }
            let until = if now < share_until {
                share_until
            } else {
                deadline
            };
            slot = self
                .freed
                .wait_timeout(slot, until.saturating_duration_since(now))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}

/// A call's place among a book's spare helpers.
struct Spare<'a> {
    shared: &'a Shared,
}

impl Drop for Spare<'_> {
    fn drop(&mut self) {
        let mut slot = self.shared.slot.lock().unwrap_or_else(|p| p.into_inner());
        slot.spares -= 1;
        self.shared.freed.notify_all();
    }
}

/// One call's use of its book's helper. Whatever it holds when it ends is
/// the book's helper from then on.
struct Turn<'a> {
    shared: &'a Shared,
    session: Option<Session>,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        let mut slot = self.shared.slot.lock().unwrap_or_else(|p| p.into_inner());
        slot.session = self.session.take();
        slot.busy = false;
        self.shared.freed.notify_all();
    }
}

impl Book {
    /// The outline came from the helper, which read an untrusted file. Check
    /// it against what this crate promises its callers before handing it on.
    fn new(
        reader: Reader,
        source: Source,
        outline: BookOutline,
        session: Session,
    ) -> Result<Book, ReaderError> {
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
        let largest = limits.max_chapter_bytes.min(u64::from(u32::MAX)) as u32;
        for (i, c) in chapters.iter_mut().enumerate() {
            c.index = i as u32;
            c.size = c.size.min(largest);
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
            helper: Arc::new(Shared {
                slot: Mutex::new(Slot {
                    session: Some(session),
                    busy: false,
                    spares: 0,
                }),
                freed: Condvar::new(),
            }),
        })
    }

    /// Ask the book's helper, starting one if there is none. A call that
    /// finds the helper busy for longer than [`SHARE_WAIT`] uses a helper of
    /// its own, so one slow read holds up no other. When the book's helper
    /// ends by itself after having answered, one fresh helper is tried,
    /// because what ended it may be what the session as a whole used up.
    /// `check` turns the reply into what the caller gets; a reply it refuses
    /// ends the helper too.
    fn ask<T>(
        &self,
        request: Request,
        check: impl FnOnce(Payload, Vec<u8>) -> Result<T, ReaderError>,
    ) -> Result<T, ReaderError> {
        if !self.source.unchanged() {
            return Err(ReaderError::SourceChanged);
        }
        let inner = &self.reader.inner;
        let deadline = inner.deadline();
        let mut turn = match self.helper.take(deadline) {
            Taken::Helper(turn) => turn,
            Taken::Own(_spare) => {
                let (payload, blob) = session::once(inner, &self.source.path, request, deadline)?;
                return check(*payload, blob);
            }
            Taken::Nothing => return Err(ReaderError::Timeout),
        };
        let mut tried_fresh = false;
        let (payload, blob) = loop {
            let mut session = match turn.session.take() {
                Some(session) => session,
                None => {
                    tried_fresh = true;
                    Session::start(inner, Some(&self.source.path), deadline)?
                }
            };
            match session.ask(request, deadline) {
                Reply::Kept(answer) => {
                    turn.session = Some(session);
                    break answer?;
                }
                Reply::Lost { error, retry } if !retry || tried_fresh => return Err(error),
                Reply::Lost { .. } => {}
            }
        };
        let checked = check(*payload, blob);
        if matches!(checked, Err(ReaderError::HelperFailed { .. })) {
            turn.session = None;
        }
        checked
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
        self.ask(Request::Chapter(index), |payload, blob| match payload {
            Payload::Chapter(mut chapter) if blob.is_empty() => {
                chapter.index = index;
                self.scrub_chapter(&mut chapter);
                Ok(chapter)
            }
            _ => Err(unexpected()),
        })
    }

    /// Read one chapter as markup for a web view: a strict subset of XHTML
    /// the helper wrote itself, with the book's scripts, styles, frames and
    /// outside addresses left out. See [`ChapterContent::markup`].
    pub fn read_content(&self, index: u32) -> Result<ChapterContent, ReaderError> {
        let count = self.chapters.len() as u32;
        if index >= count {
            return Err(ReaderError::ChapterOutOfRange { index, count });
        }
        self.ask(Request::Content(index), |payload, blob| match payload {
            Payload::Content(mut content) if blob.is_empty() => {
                content.index = index;
                // No style sheet is served yet, so none is named.
                content.styles.clear();
                // The markup is checked to be in the writer's exact form and
                // its text is counted, because every saved place and search
                // result rests on the length being the text's own. Nothing
                // is repaired: a reply that is not in that form is refused.
                if markup_text_units(&content.markup) != Some(content.length) {
                    return Err(ReaderError::HelperFailed {
                        detail: "inconsistent reply".into(),
                    });
                }
                if let Some(t) = &mut content.title {
                    scrub(t);
                }
                if let Some(l) = &mut content.language {
                    scrub(l);
                }
                Ok(content)
            }
            _ => Err(unexpected()),
        })
    }

    /// Read one picture that a chapter's markup names as `/r/<id>`. The
    /// bytes are the book's, checked only to begin as the kind they claim.
    /// A picture that is missing, too large, not a PNG, JPEG, GIF or WebP, or
    /// not what its type says is an error; show its text alternative.
    pub fn read_resource(&self, id: ResourceId) -> Result<Resource, ReaderError> {
        let max = self.reader.inner.limits.max_resource_bytes;
        self.ask(Request::Resource(id.0), |payload, bytes| match payload {
            Payload::Resource(wire) => {
                if bytes.len() as u64 > max || ResourceKind::of(&bytes) != Some(wire.kind) {
                    return Err(ReaderError::HelperFailed {
                        detail: "inconsistent reply".into(),
                    });
                }
                Ok(Resource {
                    kind: wire.kind,
                    bytes,
                })
            }
            _ => Err(unexpected()),
        })
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

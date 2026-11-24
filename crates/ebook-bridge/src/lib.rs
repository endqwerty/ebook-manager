//! UniFFI bridge joining `ebook-library`, `ebook-reader` and `ebook-device` for native shells.

mod edit;
mod thumbs;

use ebook_device::ExportOutcome;
use ebook_library::{
    Book, BrowseQuery, BrowseReader, BrowseRow, Cancel, CatalogueReplaced, Field, ImportChoice,
    ImportOutcome, ParsedMetadata, Protection, ReadingPosition, SortOrder, SortTerm, ViewToken,
};
use ebook_reader::{
    ChapterInfo, ChapterWarning, Limits, Reader, ReaderConfig, ReaderError, ResourceId, Unsupported,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

uniffi::setup_scaffolding!();

/// Every error returned across the UniFFI boundary, with a user-facing message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum BridgeError {
    Locked {
        title: String,
        protection: BookProtection,
        message: String,
    },
    PublicationPending {
        book_id: String,
        message: String,
    },
    InvalidInput {
        message: String,
    },
    Failed {
        message: String,
    },
}

impl BridgeError {
    pub fn message(&self) -> &str {
        match self {
            BridgeError::Locked { message, .. }
            | BridgeError::PublicationPending { message, .. }
            | BridgeError::InvalidInput { message }
            | BridgeError::Failed { message } => message,
        }
    }

    fn from_library(err: anyhow::Error) -> Self {
        if let Some(pending) = err.downcast_ref::<ebook_library::PublicationPending>() {
            return BridgeError::PublicationPending {
                book_id: pending.book_id.clone(),
                message: "Saved to your library; portable backup files will finish updating when storage is writable.".into(),
            };
        }
        BridgeError::Failed {
            message: format!("{err:#}"),
        }
    }

    fn from_reader(err: ReaderError) -> Self {
        BridgeError::Failed {
            message: err.to_string(),
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BridgeError {}

/// A protection scheme declared by a locked book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BookProtection {
    Lcp,
    Adobe,
    Apple,
    Password,
    Unknown,
}

impl From<Protection> for BookProtection {
    fn from(value: Protection) -> Self {
        match value {
            Protection::Lcp => BookProtection::Lcp,
            Protection::Adobe => BookProtection::Adobe,
            Protection::Apple => BookProtection::Apple,
            Protection::Password => BookProtection::Password,
            Protection::Unknown => BookProtection::Unknown,
        }
    }
}

impl From<BookProtection> for Protection {
    fn from(value: BookProtection) -> Self {
        match value {
            BookProtection::Lcp => Protection::Lcp,
            BookProtection::Adobe => Protection::Adobe,
            BookProtection::Apple => Protection::Apple,
            BookProtection::Password => Protection::Password,
            BookProtection::Unknown => Protection::Unknown,
        }
    }
}

fn map_reader_protection(found: ebook_reader::Protection) -> Protection {
    match found {
        ebook_reader::Protection::ReadiumLcp => Protection::Lcp,
        ebook_reader::Protection::Adobe => Protection::Adobe,
        ebook_reader::Protection::Apple => Protection::Apple,
        ebook_reader::Protection::Password => Protection::Password,
        ebook_reader::Protection::Unrecognised => Protection::Unknown,
    }
}

fn locked_by_phrase(protection: BookProtection) -> &'static str {
    match protection {
        BookProtection::Lcp => "Locked by Readium LCP",
        BookProtection::Adobe => "Locked by Adobe",
        BookProtection::Apple => "Locked by Apple",
        BookProtection::Password => "Locked by a password",
        BookProtection::Unknown => "Locked by an unrecognised scheme",
    }
}

const LOCK_MEANS: &str =
    "Ebook Manager can't read, convert or change the text of books locked this way.";

/// Alert title and explanation shown when a user asks to read a locked book.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LockedBookNotice {
    pub heading: String,
    pub body: String,
}

/// Formats the standard locked-book refusal notice.
#[uniffi::export]
pub fn locked_book_summary(title: String, protection: BookProtection) -> LockedBookNotice {
    let lock = locked_by_phrase(protection);
    LockedBookNotice {
        heading: format!("Can\u{2019}t open {title}"),
        body: format!(
            "It is {}{}. {LOCK_MEANS}",
            lock[..1].to_lowercase(),
            &lock[1..]
        ),
    }
}

/// Deterministic binding colour index (`0..8`) from the book ID's SHA-256 prefix.
#[uniffi::export]
pub fn binding_palette_index(book_id: String) -> u32 {
    let hash = Sha256::digest(book_id.as_bytes());
    u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]) % 8
}

/// Custom CSS property passed when wrapping chapter markup.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StyleProperty {
    pub name: String,
    pub value: String,
}

/// Wraps sanitised body markup from `read_content` into a standalone XHTML document.
#[uniffi::export]
pub fn wrap_chapter_document(
    markup: String,
    language: Option<String>,
    right_to_left: bool,
    styles: Vec<StyleProperty>,
) -> String {
    edit::wrap_xhtml(&markup, language.as_deref(), right_to_left, &styles)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SortField {
    Title,
    Author,
    Series,
    Added,
    Published,
    LastRead,
    Progress,
}

impl From<SortField> for Field {
    fn from(field: SortField) -> Self {
        match field {
            SortField::Title => Field::Title,
            SortField::Author => Field::Author,
            SortField::Series => Field::Series,
            SortField::Added => Field::Added,
            SortField::Published => Field::Published,
            SortField::LastRead => Field::LastRead,
            SortField::Progress => Field::Progress,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SortRule {
    pub field: SortField,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BookRow {
    pub id: String,
    pub version_id: String,
    pub title: String,
    pub authors: String,
    pub author_sort: Option<String>,
    pub series_name: Option<String>,
    pub series_position: Option<String>,
    pub added: Option<i64>,
    pub published: Option<String>,
    pub last_read: Option<i64>,
    pub progress: Option<f32>,
    pub has_cover: bool,
    pub cover_digest: Option<String>,
    pub cover_rgb: Option<u32>,
    pub spine_rgb: Option<u32>,
    pub right_to_left: bool,
    pub protection: Option<BookProtection>,
}

impl From<BrowseRow> for BookRow {
    fn from(row: BrowseRow) -> Self {
        let (series_name, series_position) = match row.series {
            Some((name, pos)) => (Some(name), (!pos.is_empty()).then_some(pos)),
            None => (None, None),
        };
        let cover_digest = row.cover.map(|c| c.digest);
        Self {
            id: row.id,
            version_id: row.version_id,
            title: row.title,
            authors: row.authors,
            author_sort: row.author_sort,
            series_name,
            series_position,
            added: row.added,
            published: row.published,
            last_read: row.last_read,
            progress: row.progress,
            has_cover: cover_digest.is_some(),
            cover_digest,
            cover_rgb: row.cover_rgb,
            spine_rgb: row.spine_rgb,
            right_to_left: row.right_to_left,
            protection: row.protection.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BrowseSnapshot {
    pub view_id: u64,
    pub count: u32,
    pub rows: Vec<BookRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickJumpCount {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickJumpBook {
    pub id: String,
    pub title: String,
    pub authors: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuickJumpMatches {
    pub authors: Vec<QuickJumpCount>,
    pub series: Vec<QuickJumpCount>,
    pub books: Vec<QuickJumpBook>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SeriesEntry {
    pub name: String,
    pub position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BookDetail {
    pub id: String,
    pub version_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub sort_title: Option<String>,
    pub author_sorts: Vec<Option<String>>,
    pub series: Vec<SeriesEntry>,
    pub imported_title: String,
    pub imported_authors: Vec<String>,
    pub published: Option<String>,
    pub language: Option<String>,
    pub words: Option<u32>,
    pub added: Option<i64>,
    pub has_cover: bool,
    pub cover_digest: Option<String>,
    pub cover_rgb: Option<u32>,
    pub right_to_left: bool,
    pub protection: Option<BookProtection>,
}

impl From<Book> for BookDetail {
    fn from(book: Book) -> Self {
        let raw_sorts = book
            .author_sorts
            .as_ref()
            .unwrap_or(&book.imported.author_sorts);
        let author_sorts = (0..book.authors.len())
            .map(|n| raw_sorts.get(n).cloned().flatten())
            .collect();
        let series = book
            .series
            .as_ref()
            .unwrap_or(&book.imported.series)
            .iter()
            .map(|(name, position)| SeriesEntry {
                name: name.clone(),
                position: position.clone(),
            })
            .collect();
        let cover = book.imported.survey.as_ref().and_then(|s| s.cover.as_ref());
        let words = book.imported.survey.as_ref().and_then(|s| s.words);
        Self {
            id: book.id,
            version_id: book.version_id,
            title: book.title,
            authors: book.authors,
            sort_title: book.sort_title,
            author_sorts,
            series,
            imported_title: book.imported.title,
            imported_authors: book.imported.authors,
            published: book.imported.published,
            language: book.imported.language,
            words,
            added: book.added,
            has_cover: cover.is_some(),
            cover_digest: cover.map(|c| c.digest.clone()),
            cover_rgb: cover.map(|c| c.average),
            right_to_left: book.imported.right_to_left,
            protection: book.imported.protection.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BookEdit {
    pub title: String,
    pub authors: Vec<String>,
    pub sort_title: Option<String>,
    pub author_sorts: Vec<Option<String>>,
    pub series_name: Option<String>,
    pub series_position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReviewCandidate {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub published: Option<String>,
    pub added: Option<i64>,
    pub words: Option<u32>,
    pub has_cover: bool,
    pub cover_rgb: Option<u32>,
    pub right_to_left: bool,
}

impl From<Book> for ReviewCandidate {
    fn from(book: Book) -> Self {
        let cover = book.imported.survey.as_ref().and_then(|s| s.cover.as_ref());
        let words = book.imported.survey.as_ref().and_then(|s| s.words);
        Self {
            id: book.id,
            title: book.title,
            authors: book.authors,
            published: book.imported.published,
            added: book.added,
            words,
            has_cover: cover.is_some(),
            cover_rgb: cover.map(|c| c.average),
            right_to_left: book.imported.right_to_left,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ImportOutcomeKind {
    Added {
        book: BookDetail,
    },
    Duplicate {
        book: BookDetail,
    },
    Replaced {
        book: BookDetail,
    },
    NeedsReview {
        review_token: String,
        title: String,
        authors: Vec<String>,
        published: Option<String>,
        words: Option<u32>,
        candidates: Vec<ReviewCandidate>,
    },
    Skipped,
    Failed {
        message: String,
    },
}

/// One file's import result. `publication_pending` means the outcome is saved in the
/// catalogue but its portable record is not written yet; `refresh` retries it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ImportItemResult {
    pub source_name: String,
    pub outcome: ImportOutcomeKind,
    pub publication_pending: bool,
}

/// A saved edit. `publication_pending` means it is in the catalogue but its portable
/// record is not written yet; `refresh` or saving the same values again retries it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SavedEdit {
    pub book: BookDetail,
    pub publication_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ReviewChoice {
    KeepSeparate,
    Replace { book_id: String },
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ExportStatus {
    Copied { destination: String },
    AlreadyPresent { destination: String },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChapterSummary {
    pub index: u32,
    pub title: Option<String>,
    pub linear: bool,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TocItem {
    pub title: String,
    pub depth: u32,
    pub chapter: Option<u32>,
    pub anchor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChapterIssue {
    MalformedMarkup,
    UnsupportedContent,
    RemoteReferences,
}

impl From<ChapterWarning> for ChapterIssue {
    fn from(w: ChapterWarning) -> Self {
        match w {
            ChapterWarning::MalformedMarkup => ChapterIssue::MalformedMarkup,
            ChapterWarning::UnsupportedContent => ChapterIssue::UnsupportedContent,
            ChapterWarning::RemoteReferences => ChapterIssue::RemoteReferences,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChapterMarkup {
    pub index: u32,
    pub title: Option<String>,
    pub markup: String,
    pub length: u32,
    pub language: Option<String>,
    pub right_to_left: bool,
    pub truncated: bool,
    pub warnings: Vec<ChapterIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BookResource {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// Outcome of one background survey pass.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SurveyBatch {
    /// Books whose survey is now stored locally, including those whose portable copy is still pending.
    pub surveyed: u32,
    /// Books whose survey failed; pass them back as `skip` to reach later books.
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ReadingPlace {
    pub chapter: u32,
    pub offset: u32,
    pub fraction: Option<f32>,
    pub last_read: Option<i64>,
}

struct PendingReview {
    staged: NamedTempFile,
    source: PathBuf,
    metadata: ParsedMetadata,
}

struct BrowserState {
    reader: Option<BrowseReader>,
    next_id: u64,
    current: Option<(u64, ViewToken)>,
}

/// Shared handle to a local library directory.
#[derive(uniffi::Object)]
pub struct Library {
    thumbs_dir: PathBuf,
    reader_config: ReaderConfig,
    writer: Mutex<ebook_library::Library>,
    browser: Mutex<BrowserState>,
    reviews: Mutex<HashMap<String, PendingReview>>,
}

impl Library {
    /// Opens a library with a custom [`ReaderConfig`], used by Rust integration tests.
    pub fn open_with_config(
        directory: &Path,
        reader_config: ReaderConfig,
    ) -> Result<Arc<Self>, BridgeError> {
        let lib = ebook_library::Library::open(directory).map_err(BridgeError::from_library)?;
        Ok(Arc::new(Self {
            thumbs_dir: thumbs::folder(directory),
            reader_config,
            writer: Mutex::new(lib),
            browser: Mutex::new(BrowserState {
                reader: None,
                next_id: 0,
                current: None,
            }),
            reviews: Mutex::new(HashMap::new()),
        }))
    }

    fn with_writer<T>(
        &self,
        f: impl FnOnce(&mut ebook_library::Library) -> Result<T, BridgeError>,
    ) -> Result<T, BridgeError> {
        let mut guard = self
            .writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    fn with_browser<T>(
        &self,
        mut f: impl FnMut(&mut BrowserState) -> anyhow::Result<T>,
    ) -> Result<T, BridgeError> {
        let mut guard = self
            .browser
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for attempt in 0..2 {
            if guard.reader.is_none() {
                let reader =
                    self.with_writer(|lib| lib.browse_reader().map_err(BridgeError::from_library))?;
                guard.reader = Some(reader);
                guard.current = None;
            }
            match f(&mut guard) {
                Err(e) if e.is::<CatalogueReplaced>() => {
                    guard.reader = None;
                    guard.current = None;
                    if attempt == 1 {
                        return Err(BridgeError::from_library(e));
                    }
                }
                done => return done.map_err(BridgeError::from_library),
            }
        }
        Err(BridgeError::Failed {
            message: "The library view could not be opened.".into(),
        })
    }

    fn stage_file(&self, source: PathBuf) -> Result<PendingReview, String> {
        let mut staged = NamedTempFile::new().map_err(|e| e.to_string())?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&source)
            .map_err(|e| e.to_string())?;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("Choose a regular EPUB file.".into());
        }
        let max = Limits::default().max_file_bytes;
        let bytes =
            io::copy(&mut file.take(max + 1), staged.as_file_mut()).map_err(|e| e.to_string())?;
        if bytes > max {
            return Err("EPUB exceeds the reader’s file size limit.".into());
        }
        let reader = Reader::new(self.reader_config.clone()).map_err(|e| e.to_string())?;
        let inspected = reader.inspect(staged.path()).map_err(|e| e.to_string())?;
        let survey = reader.survey(staged.path()).ok().map(|survey| {
            let facts = thumbs::facts(&survey);
            if let (Some(cover), Some(found)) = (&facts.cover, &survey.cover) {
                let _ = thumbs::write(&self.thumbs_dir, &cover.digest, (&found.thumbnail).into());
            }
            facts
        });
        let metadata = ParsedMetadata {
            title: inspected
                .title
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| {
                    source
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                }),
            author_sorts: if inspected.authors.iter().any(|a| a.sort_name.is_some()) {
                inspected
                    .authors
                    .iter()
                    .map(|a| a.sort_name.clone())
                    .collect()
            } else {
                Vec::new()
            },
            authors: inspected.authors.into_iter().map(|a| a.name).collect(),
            series: inspected
                .series
                .into_iter()
                .map(|s| (s.name, s.position))
                .collect(),
            published: inspected.published,
            language: inspected.languages.into_iter().next(),
            right_to_left: inspected.right_to_left,
            survey,
            identifier: inspected
                .identifiers
                .into_iter()
                .find(|i| i.is_unique_id)
                .map(|i| {
                    let scheme = i.scheme.unwrap_or_default();
                    format!("{}:{scheme}{}", scheme.len(), i.value)
                }),
            protection: inspected.protection.map(map_reader_protection),
        };
        Ok(PendingReview {
            staged,
            source,
            metadata,
        })
    }

    fn import_one(&self, source: PathBuf) -> ImportItemResult {
        let source_name = display_name(&source);
        let candidate = match self.stage_file(source) {
            Ok(c) => c,
            Err(message) => {
                return ImportItemResult {
                    source_name,
                    outcome: ImportOutcomeKind::Failed { message },
                    publication_pending: false,
                };
            }
        };
        let result = self.with_writer(|lib| {
            committed_import(lib.import(
                candidate.staged.path(),
                &candidate.source,
                &candidate.metadata,
                ImportChoice::Review,
            ))
        });
        let mut publication_pending = false;
        let outcome = match result {
            Ok((ImportOutcome::NeedsReview(matches), _)) => {
                let token = Uuid::new_v4().to_string();
                let title = candidate.metadata.title.clone();
                let authors = candidate.metadata.authors.clone();
                let published = candidate.metadata.published.clone();
                let words = candidate.metadata.survey.as_ref().and_then(|s| s.words);
                let candidates = matches.into_iter().map(Into::into).collect();
                self.reviews
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(token.clone(), candidate);
                ImportOutcomeKind::NeedsReview {
                    review_token: token,
                    title,
                    authors,
                    published,
                    words,
                    candidates,
                }
            }
            Ok((outcome, pending)) => {
                publication_pending = pending;
                outcome_kind(outcome).expect("a committed import")
            }
            Err(err) => ImportOutcomeKind::Failed {
                message: err.to_string(),
            },
        };
        ImportItemResult {
            source_name,
            outcome,
            publication_pending,
        }
    }
}

/// Splits a committed import from a failure, treating a pending publication as committed.
fn committed_import(
    result: Result<ImportOutcome, anyhow::Error>,
) -> Result<(ImportOutcome, bool), BridgeError> {
    match result {
        Ok(outcome) => Ok((outcome, false)),
        Err(err) => match err.downcast::<ebook_library::PublicationPending>() {
            Ok(ebook_library::PublicationPending {
                outcome: Some(outcome),
                ..
            }) => Ok((outcome, true)),
            Ok(pending) => Err(BridgeError::from_library(pending.into())),
            Err(err) => Err(BridgeError::from_library(err)),
        },
    }
}

fn outcome_kind(outcome: ImportOutcome) -> Option<ImportOutcomeKind> {
    match outcome {
        ImportOutcome::Added(book) => Some(ImportOutcomeKind::Added { book: book.into() }),
        ImportOutcome::Duplicate(book) => Some(ImportOutcomeKind::Duplicate { book: book.into() }),
        ImportOutcome::Replaced(book) => Some(ImportOutcomeKind::Replaced { book: book.into() }),
        ImportOutcome::NeedsReview(_) => None,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

enum Discovered {
    File(PathBuf),
    Failed { name: String, message: String },
}

fn discover(paths: Vec<PathBuf>) -> Vec<Discovered> {
    let mut out = Vec::new();
    for top in paths {
        if !top.is_dir() {
            out.push(Discovered::File(top));
            continue;
        }
        let mut found = Vec::new();
        let mut stack = vec![top];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    out.push(Discovered::Failed {
                        name: display_name(&dir),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            for item in entries {
                match item.and_then(|e| Ok((e.file_type()?, e.path()))) {
                    Ok((kind, path)) if kind.is_dir() => stack.push(path),
                    Ok((kind, path))
                        if (kind.is_file() || (kind.is_symlink() && !path.is_dir()))
                            && path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("epub")) =>
                    {
                        found.push(path);
                    }
                    Err(e) => out.push(Discovered::Failed {
                        name: display_name(&dir),
                        message: e.to_string(),
                    }),
                    _ => {}
                }
            }
        }
        found.sort();
        out.extend(found.into_iter().map(Discovered::File));
    }
    out
}

#[uniffi::export]
impl Library {
    /// Opens or creates the library in `directory`.
    #[uniffi::constructor]
    pub fn open(directory: String) -> Result<Arc<Self>, BridgeError> {
        Self::open_with_config(Path::new(&directory), ReaderConfig::default())
    }

    /// Imports files or folders (recursively scanning folders for `.epub` files), returning one result per file.
    pub fn import_paths(&self, paths: Vec<String>) -> Vec<ImportItemResult> {
        let expanded = discover(paths.into_iter().map(PathBuf::from).collect());
        expanded
            .into_iter()
            .map(|entry| match entry {
                Discovered::File(path) => self.import_one(path),
                Discovered::Failed { name, message } => ImportItemResult {
                    source_name: name,
                    outcome: ImportOutcomeKind::Failed { message },
                    publication_pending: false,
                },
            })
            .collect()
    }

    /// Settles a pending import review by keeping the book separate, replacing a candidate, or skipping it.
    /// The token stays valid after a failure so the choice can be retried, and is spent once
    /// the choice is saved, even while its portable record is pending.
    pub fn settle_review(
        &self,
        review_token: String,
        choice: ReviewChoice,
    ) -> Result<ImportItemResult, BridgeError> {
        let mut reviews = self.reviews.lock().unwrap_or_else(|p| p.into_inner());
        let pending = reviews
            .get(&review_token)
            .ok_or_else(|| BridgeError::InvalidInput {
                message: "This import review has already been settled.".into(),
            })?;
        let source_name = display_name(&pending.source);
        let import_choice = match choice {
            ReviewChoice::Skip => None,
            ReviewChoice::KeepSeparate => Some(ImportChoice::KeepSeparate),
            ReviewChoice::Replace { book_id } => Some(ImportChoice::Replace(book_id)),
        };
        let (outcome, publication_pending) = if let Some(import_choice) = import_choice {
            let (outcome, pending_publication) = self.with_writer(|lib| {
                committed_import(lib.import(
                    pending.staged.path(),
                    &pending.source,
                    &pending.metadata,
                    import_choice,
                ))
            })?;
            let outcome = outcome_kind(outcome).ok_or_else(|| BridgeError::Failed {
                message: "The import still requires review.".into(),
            })?;
            (outcome, pending_publication)
        } else {
            (ImportOutcomeKind::Skipped, false)
        };
        reviews.remove(&review_token);
        Ok(ImportItemResult {
            source_name,
            outcome,
            publication_pending,
        })
    }

    /// Drops staged files for reviews that will not be settled. Unknown tokens are ignored.
    pub fn discard_reviews(&self, review_tokens: Vec<String>) {
        let dropped: Vec<PendingReview> = {
            let mut reviews = self.reviews.lock().unwrap_or_else(|p| p.into_inner());
            review_tokens
                .iter()
                .filter_map(|token| reviews.remove(token))
                .collect()
        };
        drop(dropped);
    }

    /// Opens a browse snapshot for `query` and `sort`, returning the total count and first window of up to 128 rows.
    pub fn browse(
        &self,
        query: String,
        sort: Vec<SortRule>,
    ) -> Result<BrowseSnapshot, BridgeError> {
        let sort_order = if sort.is_empty() {
            SortOrder::default()
        } else {
            let terms = sort
                .into_iter()
                .map(|s| SortTerm {
                    field: s.field.into(),
                    descending: s.descending,
                })
                .collect();
            SortOrder::new(terms).map_err(|e| BridgeError::InvalidInput {
                message: e.to_string(),
            })?
        };
        let browse_query = BrowseQuery {
            text: query,
            sort: sort_order,
        };
        self.with_browser(|state| {
            let reader = state.reader.as_mut().expect("browse reader initialized");
            let opened = reader.open(&browse_query, &Cancel::default())?;
            state.next_id += 1;
            let view_id = state.next_id;
            state.current = Some((view_id, opened.token));
            Ok(BrowseSnapshot {
                view_id,
                count: opened.count,
                rows: opened.first.into_iter().map(Into::into).collect(),
            })
        })
    }

    /// Reads a window of rows `[start .. start + limit)` from an opened browse snapshot.
    pub fn browse_window(
        &self,
        view_id: u64,
        start: u32,
        limit: u32,
    ) -> Result<Vec<BookRow>, BridgeError> {
        self.with_browser(|state| {
            let token = state
                .current
                .filter(|(id, _)| *id == view_id)
                .map(|(_, token)| token)
                .ok_or(ebook_library::StaleView)?;
            let reader = state.reader.as_mut().expect("browse reader initialized");
            let rows = reader.window(token, start, limit)?;
            Ok(rows.into_iter().map(Into::into).collect())
        })
    }

    /// Finds the current rank of each book ID in an opened browse snapshot.
    pub fn browse_locate(
        &self,
        view_id: u64,
        ids: Vec<String>,
    ) -> Result<Vec<Option<u32>>, BridgeError> {
        self.with_browser(|state| {
            let token = state
                .current
                .filter(|(id, _)| *id == view_id)
                .map(|(_, token)| token)
                .ok_or(ebook_library::StaleView)?;
            let reader = state.reader.as_mut().expect("browse reader initialized");
            reader.locate(token, &ids)
        })
    }

    /// Reports whether the catalogue has changed since the active browse snapshot opened.
    pub fn browse_changed(&self) -> Result<bool, BridgeError> {
        self.with_browser(|state| {
            let reader = state.reader.as_mut().expect("browse reader initialized");
            reader.changed()
        })
    }

    /// Searches capped quick-jump matches across authors, series and books.
    pub fn quick_jump(&self, query: String, limit: u32) -> Result<QuickJumpMatches, BridgeError> {
        self.with_browser(|state| {
            let reader = state.reader.as_mut().expect("browse reader initialized");
            let found = reader.quick_jump(&query, limit as usize, &Cancel::default())?;
            Ok(QuickJumpMatches {
                authors: found
                    .authors
                    .into_iter()
                    .map(|(name, count)| QuickJumpCount { name, count })
                    .collect(),
                series: found
                    .series
                    .into_iter()
                    .map(|(name, count)| QuickJumpCount { name, count })
                    .collect(),
                books: found
                    .books
                    .into_iter()
                    .map(|(id, title, authors)| QuickJumpBook { id, title, authors })
                    .collect(),
            })
        })
    }

    /// Loads a book's full record for details or editing.
    pub fn book(&self, book_id: String) -> Result<BookDetail, BridgeError> {
        self.with_writer(|lib| {
            lib.book(&book_id)
                .map(Into::into)
                .map_err(BridgeError::from_library)
        })
    }

    /// Saves a book's title, authors, sort title, author sort names and primary series together.
    pub fn edit_book(&self, book_id: String, edit: BookEdit) -> Result<SavedEdit, BridgeError> {
        if edit.title.trim().is_empty() {
            return Err(BridgeError::InvalidInput {
                message: "Enter a title for this book.".into(),
            });
        }
        self.with_writer(|lib| {
            let current = lib.book(&book_id).map_err(BridgeError::from_library)?;
            let changes = edit::plan_edit(&current, &edit);
            match lib.edit_book(&book_id, changes) {
                Ok(book) => Ok(SavedEdit {
                    book: book.into(),
                    publication_pending: false,
                }),
                Err(err) if err.is::<ebook_library::PublicationPending>() => Ok(SavedEdit {
                    book: lib
                        .book(&book_id)
                        .map_err(BridgeError::from_library)?
                        .into(),
                    publication_pending: true,
                }),
                Err(err) => Err(BridgeError::from_library(err)),
            }
        })
    }

    /// Returns verified PNG bytes for a book's cover thumbnail, remaking the cache file if missing.
    pub fn cover_thumbnail(&self, book_id: String) -> Result<Option<Vec<u8>>, BridgeError> {
        let (version_id, survey, managed_path) = self.with_writer(|lib| {
            let book = lib.book(&book_id).map_err(BridgeError::from_library)?;
            let path = lib
                .managed_file(&book_id)
                .map_err(BridgeError::from_library)?;
            Ok((book.version_id, book.imported.survey, path))
        })?;
        if let Some(existing) = survey {
            let Some(cover) = existing.cover else {
                return Ok(None);
            };
            if let Some(png) = thumbs::load_verified_png(&self.thumbs_dir, &cover.digest) {
                return Ok(Some(png));
            }
            let reader =
                Reader::new(self.reader_config.clone()).map_err(BridgeError::from_reader)?;
            let surveyed = reader
                .survey(&managed_path)
                .map_err(BridgeError::from_reader)?;
            let Some(remade) = surveyed.cover else {
                return Ok(None);
            };
            let pixels: thumbs::Pixels<'_> = (&remade.thumbnail).into();
            if thumbs::digest(pixels) != cover.digest {
                return Ok(None);
            }
            let bytes = thumbs::write(&self.thumbs_dir, &cover.digest, pixels)
                .map_err(BridgeError::from_library)?;
            return Ok(Some(bytes));
        }
        let reader = Reader::new(self.reader_config.clone()).map_err(BridgeError::from_reader)?;
        let surveyed = reader
            .survey(&managed_path)
            .map_err(BridgeError::from_reader)?;
        let facts = thumbs::facts(&surveyed);
        let mut out = None;
        if let (Some(cover), Some(found)) = (&facts.cover, &surveyed.cover) {
            out = thumbs::write(&self.thumbs_dir, &cover.digest, (&found.thumbnail).into()).ok();
        }
        let _ = self.with_writer(|lib| {
            lib.set_survey(&book_id, &version_id, facts)
                .map_err(BridgeError::from_library)
        });
        Ok(out)
    }

    /// Surveys up to `limit` unsurveyed books not named in `skip`, storing their word counts and cover thumbnails.
    pub fn survey_unsurveyed(
        &self,
        limit: u32,
        skip: Vec<String>,
    ) -> Result<SurveyBatch, BridgeError> {
        let mut batch = SurveyBatch {
            surveyed: 0,
            failed: Vec::new(),
        };
        if limit == 0 {
            return Ok(batch);
        }
        let skip: HashSet<String> = skip.into_iter().collect();
        let wanted = limit.saturating_add(u32::try_from(skip.len()).unwrap_or(u32::MAX));
        let books: Vec<_> = self
            .with_writer(|lib| lib.unsurveyed(wanted).map_err(BridgeError::from_library))?
            .into_iter()
            .filter(|book| !skip.contains(&book.id))
            .take(limit as usize)
            .collect();
        if books.is_empty() {
            return Ok(batch);
        }
        let reader = Reader::new(self.reader_config.clone()).map_err(BridgeError::from_reader)?;
        for book in books {
            let stored = self
                .with_writer(|lib| {
                    lib.managed_file(&book.id)
                        .map_err(BridgeError::from_library)
                })
                .and_then(|path| reader.survey(&path).map_err(BridgeError::from_reader))
                .and_then(|surveyed| {
                    let facts = thumbs::facts(&surveyed);
                    if let (Some(cover), Some(found)) = (&facts.cover, &surveyed.cover) {
                        let _ = thumbs::write(
                            &self.thumbs_dir,
                            &cover.digest,
                            (&found.thumbnail).into(),
                        );
                    }
                    self.with_writer(|lib| {
                        lib.set_survey(&book.id, &book.version_id, facts)
                            .map_err(BridgeError::from_library)
                    })
                });
            match stored {
                Ok(()) | Err(BridgeError::PublicationPending { .. }) => batch.surveyed += 1,
                Err(_) => batch.failed.push(book.id),
            }
        }
        Ok(batch)
    }

    /// Exports a book's managed EPUB to `destination_dir` through `ebook-device`.
    pub fn export_book(
        &self,
        book_id: String,
        destination_dir: String,
    ) -> Result<ExportStatus, BridgeError> {
        let (source, title) = self.with_writer(|lib| {
            let book = lib.book(&book_id).map_err(BridgeError::from_library)?;
            let path = lib
                .managed_file(&book_id)
                .map_err(BridgeError::from_library)?;
            Ok((path, book.title))
        })?;
        match ebook_device::export_epub(&source, Path::new(&destination_dir), &title) {
            Ok(ExportOutcome::Copied(path)) => Ok(ExportStatus::Copied {
                destination: path.to_string_lossy().into_owned(),
            }),
            Ok(ExportOutcome::AlreadyPresent(path)) => Ok(ExportStatus::AlreadyPresent {
                destination: path.to_string_lossy().into_owned(),
            }),
            Err(e) => Err(BridgeError::Failed {
                message: e.to_string(),
            }),
        }
    }

    /// Reconciles portable journal files with SQLite and finishes pending publications.
    pub fn refresh(&self) -> Result<(), BridgeError> {
        self.with_writer(|lib| lib.refresh().map_err(BridgeError::from_library))
    }

    /// Opens a book in its isolated reader helper session.
    pub fn open_book(self: Arc<Self>, book_id: String) -> Result<Arc<OpenBook>, BridgeError> {
        let (record, managed_path) = self.with_writer(|lib| {
            let book = lib.book(&book_id).map_err(BridgeError::from_library)?;
            let path = lib
                .managed_file(&book_id)
                .map_err(BridgeError::from_library)?;
            Ok((book, path))
        })?;
        let reader = Reader::new(self.reader_config.clone()).map_err(BridgeError::from_reader)?;
        match reader.open(&managed_path) {
            Ok(opened) => {
                let chapters = opened
                    .chapters()
                    .iter()
                    .map(|c| ChapterSummary {
                        index: c.index,
                        title: c.title.clone(),
                        linear: c.linear,
                        size: c.size,
                    })
                    .collect();
                let toc = opened
                    .toc()
                    .iter()
                    .map(|entry| TocItem {
                        title: entry.title.clone(),
                        depth: entry.depth,
                        chapter: entry.target.as_ref().map(|t| t.chapter),
                        anchor: entry.target.as_ref().and_then(|t| t.anchor.clone()),
                    })
                    .collect();
                Ok(Arc::new(OpenBook {
                    library: self,
                    book_id: record.id,
                    version_id: record.version_id,
                    title: record.title,
                    raw_chapters: opened.chapters().to_vec(),
                    chapters,
                    toc,
                    reader_book: opened,
                }))
            }
            Err(ReaderError::Unsupported(Unsupported::Protected)) => {
                let detected = reader
                    .inspect(&managed_path)
                    .ok()
                    .and_then(|m| m.protection)
                    .map(map_reader_protection)
                    .or(record.imported.protection)
                    .unwrap_or(Protection::Unknown);
                let _ = self.with_writer(|lib| {
                    lib.set_protection(&record.id, &record.version_id, detected)
                        .map_err(BridgeError::from_library)
                });
                let protection: BookProtection = detected.into();
                let notice = locked_book_summary(record.title.clone(), protection);
                Err(BridgeError::Locked {
                    title: record.title,
                    protection,
                    message: format!("{} {}", notice.heading, notice.body),
                })
            }
            Err(other) => Err(BridgeError::from_reader(other)),
        }
    }
}

/// An open book backed by a sandboxed `ebook-reader-helper` session.
#[derive(uniffi::Object)]
pub struct OpenBook {
    library: Arc<Library>,
    book_id: String,
    version_id: String,
    title: String,
    raw_chapters: Vec<ChapterInfo>,
    chapters: Vec<ChapterSummary>,
    toc: Vec<TocItem>,
    reader_book: ebook_reader::Book,
}

#[uniffi::export]
impl OpenBook {
    pub fn book_id(&self) -> String {
        self.book_id.clone()
    }

    pub fn version_id(&self) -> String {
        self.version_id.clone()
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    pub fn chapters(&self) -> Vec<ChapterSummary> {
        self.chapters.clone()
    }

    pub fn toc(&self) -> Vec<TocItem> {
        self.toc.clone()
    }

    /// Reads sanitised XHTML body markup for `chapter`.
    pub fn read_content(&self, chapter: u32) -> Result<ChapterMarkup, BridgeError> {
        let content = self
            .reader_book
            .read_content(chapter)
            .map_err(BridgeError::from_reader)?;
        Ok(ChapterMarkup {
            index: content.index,
            title: content.title,
            markup: content.markup,
            length: content.length,
            language: content.language,
            right_to_left: content.right_to_left,
            truncated: content.truncated,
            warnings: content.warnings.into_iter().map(Into::into).collect(),
        })
    }

    /// Reads undecoded image bytes for a manifest resource `/r/<id>`.
    pub fn read_resource(&self, id: u32) -> Result<BookResource, BridgeError> {
        let resource = self
            .reader_book
            .read_resource(ResourceId(id))
            .map_err(BridgeError::from_reader)?;
        Ok(BookResource {
            media_type: resource.kind.media_type().to_owned(),
            bytes: resource.bytes,
        })
    }

    /// Loads the saved reading place for this book version, clamped to the chapter count.
    pub fn load_place(&self) -> Result<Option<ReadingPlace>, BridgeError> {
        let saved = self.library.with_writer(|lib| {
            lib.load_position(&self.book_id)
                .map_err(BridgeError::from_library)
        })?;
        let last = self.chapters.len().saturating_sub(1) as u32;
        Ok(saved.filter(|p| p.version_id == self.version_id).map(|p| {
            let (chapter, offset) = if p.chapter > last {
                (last, 0)
            } else {
                (p.chapter, p.offset)
            };
            ReadingPlace {
                chapter,
                offset,
                fraction: p.fraction,
                last_read: p.last_read,
            }
        }))
    }

    /// Saves the reading place and chapter-weighted progress fraction for this book version.
    pub fn save_place(
        &self,
        chapter: u32,
        offset: u32,
        page: u32,
        pages: u32,
    ) -> Result<(), BridgeError> {
        let fraction = (pages > 0)
            .then(|| edit::progress_fraction(&self.raw_chapters, chapter as usize, page, pages));
        let last_read = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok());
        let position = ReadingPosition {
            version_id: self.version_id.clone(),
            chapter,
            offset,
            last_read,
            fraction,
        };
        self.library.with_writer(|lib| {
            lib.save_position(&self.book_id, &position)
                .map_err(BridgeError::from_library)
        })
    }
}

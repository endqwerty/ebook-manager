//! Single-device library for stable EPUB inputs already examined by the reader helper.
mod browse;
pub use browse::*;
mod catalogue;
mod record;
pub use record::*;
#[cfg(feature = "fixture")]
pub mod fixture;
mod journal;
mod sortkey;
use anyhow::{Context, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub type Result<T> = anyhow::Result<T>;

/// Downcast the error to this type to find an already-committed shelf entry.
/// After an import, `outcome` holds what was committed.
#[derive(Debug)]
pub struct PublicationPending {
    pub book_id: String,
    pub outcome: Option<ImportOutcome>,
}
impl std::fmt::Display for PublicationPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "book {} saved locally; portable publication pending",
            self.book_id
        )
    }
}
impl std::error::Error for PublicationPending {}
#[derive(Debug, Clone)]
pub enum ImportChoice {
    Review,
    KeepSeparate,
    Replace(String),
}
/// The user-editable fields of a book. `None` filing fields resume imported values.
#[derive(Debug, Clone, PartialEq)]
pub struct BookChanges {
    pub title: String,
    pub authors: Vec<String>,
    pub sort_title: Option<String>,
    pub author_sorts: Option<Vec<Option<String>>>,
    pub series: Option<Vec<(String, Option<String>)>>,
}
#[derive(Debug, Clone)]
pub enum ImportOutcome {
    Added(Book),
    Duplicate(Book),
    NeedsReview(Vec<Book>),
    Replaced(Book),
}

#[derive(Serialize, Deserialize)]
enum Change {
    Book(Box<Book>),
    Position {
        id: String,
        position: ReadingPosition,
    },
}
#[derive(Serialize, Deserialize)]
struct Entry {
    format: u32,
    id: String,
    parent: Option<String>,
    change: Change,
}

fn uid() -> String {
    Uuid::new_v4().to_string()
}
fn uuid_ok(s: &str) -> bool {
    Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s)
}
fn directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => File::open(path.parent().unwrap())?.sync_all()?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    ensure!(
        path.symlink_metadata()?.is_dir(),
        "expected directory: {}",
        path.display()
    );
    Ok(())
}
fn regular(path: &Path) -> Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let f = opts.open(path)?;
    ensure!(f.metadata()?.is_file(), "expected regular file");
    Ok(f)
}
fn check_optional_file(path: &Path) -> Result<()> {
    match path.symlink_metadata() {
        Ok(m) => ensure!(m.is_file(), "unsafe file: {}", path.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    };
    Ok(())
}
fn read_entry(path: &Path) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    regular(path)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= 8 * 1024 * 1024, "record too large");
    Ok(data)
}
fn install(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap();
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    tmp.persist_noclobber(path)?;
    File::open(dir)?.sync_all()?;
    Ok(())
}
fn digest(r: &mut impl Read) -> Result<String> {
    let mut hash = Sha256::new();
    io::copy(r, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

pub struct Library {
    keys: sortkey::Keys,
    db: Connection,
    portable: PathBuf,
    _lock: File,
}
impl Library {
    pub fn open(app_data: &Path) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(app_data)?;
        ensure!(
            app_data.symlink_metadata()?.is_dir(),
            "invalid library root"
        );
        let root = app_data.canonicalize()?;
        let portable = root.join("portable");
        directory(&portable)?;
        for part in ["changes", "books", "sidecars"] {
            directory(&portable.join(part))?;
        }
        let lock_path = root.join("library.lock");
        check_optional_file(&lock_path)?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock().context("library is already open")?;
        for name in [
            "library.sqlite",
            "library.sqlite-journal",
            "library.sqlite-wal",
            "library.sqlite-shm",
        ] {
            check_optional_file(&root.join(name))?;
        }
        let db = catalogue::open(&root.join("library.sqlite"))?;
        let keys = sortkey::Keys::new()?;
        let mut lib = Self {
            db,
            keys,
            portable,
            _lock: lock,
        };
        let tx = lib.db.transaction()?;
        if tx
            .query_row("SELECT value FROM meta WHERE key='profile'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .as_ref()
            != Some(&lib.keys.profile)
        {
            let mut rows = tx.prepare("SELECT data FROM book ORDER BY n")?;
            for data in rows.query_map([], |r| r.get::<_, String>(0))? {
                catalogue::project(&tx, &serde_json::from_str(&data?)?, &lib.keys)?;
            }
            drop(rows);
            tx.execute(
                "INSERT OR REPLACE INTO meta VALUES('profile',?)",
                [&lib.keys.profile],
            )?;
            tx.execute("UPDATE meta SET value=value+1 WHERE key='counter'", [])?;
        }
        tx.commit()?;
        lib.recover()?;
        Ok(lib)
    }
    pub fn book(&self, id: &str) -> Result<Book> {
        let data: String = self
            .db
            .query_row("SELECT data FROM book WHERE id=?", [id], |r| r.get(0))?;
        let b: Book = serde_json::from_str(&data)?;
        b.validate()?;
        Ok(b)
    }
    fn directory(&self, part: &str) -> Result<PathBuf> {
        ensure!(
            self.portable.symlink_metadata()?.is_dir(),
            "unsafe portable directory"
        );
        let dir = self.portable.join(part);
        directory(&dir)?;
        Ok(dir)
    }
    fn blob(&self, b: &Book) -> Result<PathBuf> {
        b.validate()?;
        let dir = self.directory("books")?;
        Ok(dir.join(format!("{}.epub", b.sha256)))
    }
    pub fn managed_file(&self, id: &str) -> Result<PathBuf> {
        let b = self.book(id)?;
        let path = self.blob(&b)?;
        ensure!(
            digest(&mut regular(&path)?)? == b.sha256,
            "managed bytes differ"
        );
        Ok(path)
    }
    pub fn import(
        &mut self,
        staged: &Path,
        source: &Path,
        meta: &ParsedMetadata,
        choice: ImportChoice,
    ) -> Result<ImportOutcome> {
        meta.validate()?;
        let input = regular(staged)?;
        const MAX_INPUT: u64 = 512 * 1024 * 1024;
        ensure!(
            input.metadata()?.len() <= MAX_INPUT,
            "staged book exceeds 512 MiB"
        );
        let dir = self.directory("books")?;
        let mut tmp = NamedTempFile::new_in(&dir)?;
        ensure!(
            io::copy(&mut input.take(MAX_INPUT + 1), &mut tmp)? <= MAX_INPUT,
            "staged book exceeds 512 MiB"
        );
        let sha = digest(&mut tmp.reopen()?)?;
        let key = source.as_os_str().as_encoded_bytes();
        let (prev, candidates) = self.import_candidates(key, &sha, meta.identifier.as_deref())?;
        let exact: Vec<_> = candidates
            .iter()
            .filter(|b| b.sha256 == sha)
            .cloned()
            .collect();
        let duplicate = match (&choice, exact.as_slice()) {
            (ImportChoice::Replace(id), _) => exact.iter().find(|b| &b.id == id),
            (_, [b])
                if prev.as_ref().is_none_or(|id| id == &b.id)
                    || matches!(choice, ImportChoice::KeepSeparate) =>
            {
                Some(b)
            }
            (ImportChoice::Review, _) => exact.iter().find(|b| prev.as_ref() == Some(&b.id)),
            _ => None,
        };
        if let Some(b) = duplicate {
            self.store_blob(tmp, b)?;
            self.db.execute(
                "INSERT OR REPLACE INTO sources VALUES(?,?)",
                params![key, b.id],
            )?;
            self.publish().context(PublicationPending {
                book_id: b.id.clone(),
                outcome: Some(ImportOutcome::Duplicate(b.clone())),
            })?;
            return Ok(ImportOutcome::Duplicate(b.clone()));
        }
        if !candidates.is_empty() && matches!(choice, ImportChoice::Review) {
            return Ok(ImportOutcome::NeedsReview(candidates));
        }
        let replaced = matches!(choice, ImportChoice::Replace(_));
        let mut book = match choice {
            ImportChoice::Replace(id) => candidates
                .into_iter()
                .find(|b| b.id == id)
                .context("replacement candidate changed")?,
            _ => Book {
                id: uid(),
                edition_id: uid(),
                file_id: uid(),
                version_id: uid(),
                title: meta.title.clone(),
                authors: meta.authors.clone(),
                imported: meta.clone(),
                sha256: sha.clone(),
                added: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs()
                        .try_into()?,
                ),
                sort_title: None,
                author_sorts: None,
                series: None,
            },
        };
        if replaced {
            if book.title == book.imported.title {
                book.title = meta.title.clone();
            }
            if book.authors == book.imported.authors {
                book.authors = meta.authors.clone();
            }
            book.version_id = uid();
            book.imported = meta.clone();
            book.sha256 = sha;
        }
        self.store_blob(tmp, &book)?;
        let outcome = if replaced {
            ImportOutcome::Replaced(book.clone())
        } else {
            ImportOutcome::Added(book.clone())
        };
        if let Err(mut err) =
            self.commit(Change::Book(Box::new(book.clone())), Some((key, &book.id)))
        {
            if let Some(pending) = err.downcast_mut::<PublicationPending>() {
                pending.outcome = Some(outcome);
            }
            return Err(err);
        }
        Ok(outcome)
    }
    fn import_candidates(
        &self,
        key: &[u8],
        sha: &str,
        identifier: Option<&str>,
    ) -> Result<(Option<String>, Vec<Book>)> {
        let prev: Option<String> = self
            .db
            .query_row("SELECT id FROM sources WHERE path=?", [key], |r| r.get(0))
            .optional()?;
        let mut candidates = Vec::new();
        for data in self
            .db
            .prepare_cached("SELECT data FROM book WHERE sha256=? OR id=? OR identifier=?")?
            .query_map(params![sha, prev, identifier], |r| r.get::<_, String>(0))?
        {
            let b: Book = serde_json::from_str(&data?)?;
            b.validate()?;
            candidates.push(b);
        }
        candidates.sort_by_cached_key(|b| (b.title.to_lowercase(), b.id.clone()));
        Ok((prev, candidates))
    }
    fn store_blob(&self, tmp: NamedTempFile, b: &Book) -> Result<()> {
        let path = self.blob(b)?;
        if path.try_exists()? && digest(&mut regular(&path)?)? != b.sha256 {
            preserve(&path, "damaged")?;
        }
        if !path.try_exists()? {
            tmp.as_file().sync_all()?;
            tmp.persist_noclobber(&path)?;
        }
        File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    }
    /// Replaces every user-editable field in one journal entry. Resubmitting the
    /// current values commits nothing and retries pending publication.
    pub fn edit_book(&mut self, id: &str, changes: BookChanges) -> Result<Book> {
        fields(&changes.title, &changes.authors)?;
        let mut book = self.book(id)?;
        let before = book.clone();
        book.title = changes.title;
        book.authors = changes.authors;
        book.sort_title = changes.sort_title;
        book.author_sorts = changes.author_sorts;
        book.series = changes.series;
        if book == before {
            self.publish().context(PublicationPending {
                book_id: id.into(),
                outcome: None,
            })?;
        } else {
            self.commit(Change::Book(Box::new(book.clone())), None)?;
        }
        Ok(book)
    }
    pub fn edit(&mut self, id: &str, title: &str, authors: &[String]) -> Result<Book> {
        fields(title, authors)?;
        let mut b = self.book(id)?;
        b.title = title.into();
        b.authors = authors.into();
        self.commit(Change::Book(Box::new(b.clone())), None)?;
        Ok(b)
    }
    /// Replace user-controlled filing and series overrides. None resumes imported values.
    pub fn edit_filing(
        &mut self,
        id: &str,
        sort_title: Option<String>,
        author_sorts: Option<Vec<Option<String>>>,
        series: Option<Vec<(String, Option<String>)>>,
    ) -> Result<Book> {
        let mut book = self.book(id)?;
        book.sort_title = sort_title;
        book.author_sorts = author_sorts;
        book.series = series;
        self.commit(Change::Book(Box::new(book.clone())), None)?;
        Ok(book)
    }
    pub fn unsurveyed(&self, limit: u32) -> Result<Vec<Book>> {
        self.db
            .prepare("SELECT data FROM book WHERE coalesce(survey_version,0)<1 ORDER BY n LIMIT ?")?
            .query_map([limit], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect()
    }
    pub fn set_survey(&mut self, id: &str, version_id: &str, facts: SurveyFacts) -> Result<()> {
        let mut book = self.book(id)?;
        if book.version_id == version_id && book.imported.survey.as_ref() != Some(&facts) {
            book.imported.survey = Some(facts);
            self.commit(Change::Book(Box::new(book)), None)?;
        }
        Ok(())
    }
    /// Records a lock found in a book's file after it was imported, as the
    /// reader helper now recognises more schemes. Nothing changes when the
    /// book has another version by now.
    pub fn set_protection(
        &mut self,
        id: &str,
        version_id: &str,
        protection: Protection,
    ) -> Result<()> {
        let mut book = self.book(id)?;
        if book.version_id == version_id && book.imported.protection != Some(protection) {
            book.imported.protection = Some(protection);
            self.commit(Change::Book(Box::new(book)), None)?;
        }
        Ok(())
    }
    pub fn save_position(&mut self, id: &str, pos: &ReadingPosition) -> Result<()> {
        ensure!(
            self.book(id)?.version_id == pos.version_id,
            "reading position belongs to another version"
        );
        if self.load_position(id)?.is_some_and(|old| {
            old.chapter == pos.chapter && old.offset == pos.offset && old.fraction == pos.fraction
        }) {
            return self.publish().context(PublicationPending {
                book_id: id.into(),
                outcome: None,
            });
        }
        self.commit(
            Change::Position {
                id: id.into(),
                position: pos.clone(),
            },
            None,
        )
    }
    pub fn load_position(&self, id: &str) -> Result<Option<ReadingPosition>> {
        let b = self.book(id)?;
        let raw: Option<String> = self
            .db
            .query_row("SELECT data FROM positions WHERE id=?", [id], |r| r.get(0))
            .optional()?;
        Ok(raw
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .filter(|p: &ReadingPosition| p.version_id == b.version_id))
    }
}
// Keep the prior name recoverable before installing verified replacement contents.
fn preserve(path: &Path, label: &str) -> Result<()> {
    let backup = path.with_extension(format!("{label}-{}", uid()));
    fs::rename(path, &backup)?;
    File::open(path.parent().context("missing parent directory")?)?.sync_all()?;
    Ok(())
}

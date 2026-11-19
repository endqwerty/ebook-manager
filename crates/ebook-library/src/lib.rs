//! Single-device library for stable EPUB inputs already examined by the reader helper.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub identifier: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub edition_id: String,
    pub file_id: String,
    pub version_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub imported: ParsedMetadata,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub version_id: String,
    pub chapter: u32,
    pub offset: u32,
}
#[derive(Debug, Clone)]
pub enum ImportChoice {
    Review,
    KeepSeparate,
    Replace(String),
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
    Book(Book),
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
fn fields(title: &str, authors: &[String]) -> Result<()> {
    ensure!(
        !title.trim().is_empty()
            && title.len() <= 8192
            && authors.len() <= 128
            && authors.iter().all(|a| a.len() <= 4096),
        "invalid title or author metadata"
    );
    Ok(())
}
impl ParsedMetadata {
    fn validate(&self) -> Result<()> {
        fields(&self.title, &self.authors)?;
        ensure!(
            self.identifier
                .as_ref()
                .is_none_or(|s| !s.trim().is_empty() && s.len() <= 8192),
            "invalid identifier"
        );
        Ok(())
    }
}
impl Book {
    fn validate(&self) -> Result<()> {
        ensure!(
            [&self.id, &self.edition_id, &self.file_id, &self.version_id]
                .into_iter()
                .all(|s| uuid_ok(s)),
            "invalid book identity"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid digest"
        );
        fields(&self.title, &self.authors)?;
        self.imported.validate()
    }
}
fn directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
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
    db: Connection,
    portable: PathBuf,
    _lock: File,
}
impl Library {
    pub fn open(app_data: &Path) -> Result<Self> {
        fs::create_dir_all(app_data)?;
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
        let db = Connection::open(root.join("library.sqlite"))?;
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS books(id TEXT PRIMARY KEY,data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS positions(id TEXT PRIMARY KEY,data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sources(path BLOB PRIMARY KEY,id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS journal(seq INTEGER PRIMARY KEY,id TEXT UNIQUE NOT NULL,data TEXT NOT NULL,published INTEGER NOT NULL);")?;
        let mut lib = Self {
            db,
            portable,
            _lock: lock,
        };
        lib.refresh()?;
        Ok(lib)
    }
    fn book(&self, id: &str) -> Result<Book> {
        let data: String = self
            .db
            .query_row("SELECT data FROM books WHERE id=?", [id], |r| r.get(0))?;
        let b: Book = serde_json::from_str(&data)?;
        b.validate()?;
        Ok(b)
    }
    pub fn search(&self, query: &str) -> Result<Vec<Book>> {
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        for row in self
            .db
            .prepare("SELECT data FROM books")?
            .query_map([], |r| r.get::<_, String>(0))?
        {
            let b: Book = serde_json::from_str(&row?)?;
            b.validate()?;
            if std::iter::once(&b.title)
                .chain(&b.authors)
                .any(|s| s.to_lowercase().contains(&q))
            {
                out.push(b)
            }
        }
        out.sort_by_cached_key(|b| (b.title.to_lowercase(), b.id.clone()));
        Ok(out)
    }
    fn blob(&self, b: &Book) -> Result<PathBuf> {
        b.validate()?;
        let dir = self.portable.join("books");
        directory(&dir)?;
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
        let mut input = regular(staged)?;
        let dir = self.portable.join("books");
        directory(&dir)?;
        let mut tmp = NamedTempFile::new_in(&dir)?;
        io::copy(&mut input, &mut tmp)?;
        let sha = digest(&mut tmp.reopen()?)?;
        let key = source.as_os_str().as_encoded_bytes();
        let prev: Option<String> = self
            .db
            .query_row("SELECT id FROM sources WHERE path=?", [key], |r| r.get(0))
            .optional()?;
        let mut candidates = Vec::new();
        let mut exact = None;
        for b in self.search("")? {
            if b.sha256 == sha {
                exact = Some(b.clone());
            }
            if prev.as_ref() == Some(&b.id)
                || b.sha256 == sha
                || (meta.identifier.is_some() && meta.identifier == b.imported.identifier)
            {
                candidates.push(b);
            }
        }
        if let Some(b) = exact.as_ref().filter(|_| candidates.len() == 1) {
            self.store_blob(tmp, b)?;
            self.db.execute(
                "INSERT OR REPLACE INTO sources VALUES(?,?)",
                params![key, b.id],
            )?;
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
        self.commit(Change::Book(book.clone()), Some((key, &book.id)))?;
        Ok(if replaced {
            ImportOutcome::Replaced(book)
        } else {
            ImportOutcome::Added(book)
        })
    }
    fn store_blob(&self, tmp: NamedTempFile, b: &Book) -> Result<()> {
        let path = self.blob(b)?;
        if path.try_exists()? {
            ensure!(
                digest(&mut regular(&path)?)? == b.sha256,
                "managed bytes differ"
            );
        } else {
            tmp.as_file().sync_all()?;
            tmp.persist_noclobber(&path)?;
        }
        File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    }
    pub fn edit(&mut self, id: &str, title: &str, authors: &[String]) -> Result<Book> {
        fields(title, authors)?;
        let mut b = self.book(id)?;
        b.title = title.into();
        b.authors = authors.into();
        self.commit(Change::Book(b.clone()), None)?;
        Ok(b)
    }
    pub fn save_position(&mut self, id: &str, pos: &ReadingPosition) -> Result<()> {
        ensure!(
            self.book(id)?.version_id == pos.version_id,
            "reading position belongs to another version"
        );
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
    fn commit(&mut self, change: Change, source: Option<(&[u8], &str)>) -> Result<()> {
        let tx = self.db.transaction()?;
        let parent = tip(&tx)?;
        let e = Entry {
            format: 1,
            id: uid(),
            parent,
            change,
        };
        let data = serde_json::to_string(&e)?;
        append(&tx, &e, &data, false)?;
        if let Some((path, id)) = source {
            tx.execute(
                "INSERT OR REPLACE INTO sources VALUES(?,?)",
                params![path, id],
            )?;
        }
        tx.commit()?;
        self.publish()
            .context("saved locally; portable publication pending")
    }
    fn publish(&mut self) -> Result<()> {
        let dir = self.portable.join("changes");
        directory(&dir)?;
        let pending = self
            .db
            .prepare("SELECT seq,data FROM journal WHERE published=0 ORDER BY seq")?
            .query_map([], |r| Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (seq, data) in pending {
            let path = dir.join(format!("{seq:020}.json"));
            if path.try_exists()? {
                ensure!(
                    read_entry(&path)? == data.as_bytes(),
                    "conflicting portable change"
                );
            } else {
                install(&path, data.as_bytes())?;
            }
            let e: Entry = serde_json::from_str(&data)?;
            if let Change::Book(b) = e.change {
                self.sidecar(&b)?;
            }
            File::open(&dir)?.sync_all()?;
            self.db
                .execute("UPDATE journal SET published=1 WHERE seq=?", [seq])?;
        }
        Ok(())
    }
    fn sidecar(&self, b: &Book) -> Result<()> {
        b.validate()?;
        let dir = self.portable.join("sidecars");
        directory(&dir)?;
        let path = dir.join(format!("{}-{}.json", b.id, sidecar_digest(b)?));
        let data = serde_json::to_vec_pretty(b)?;
        if path.try_exists()? {
            ensure!(read_entry(&path)? == data, "edited sidecar requires review");
        } else {
            install(&path, &data)?;
        }
        File::open(dir)?.sync_all()?;
        Ok(())
    }
    pub fn refresh(&mut self) -> Result<()> {
        let dir = self.portable.join("changes");
        directory(&dir)?;
        let mut ordered = BTreeMap::new();
        for item in dir.read_dir()? {
            let item = item?;
            let name = item.file_name();
            let name = name.to_str().context("invalid journal filename")?;
            if name.starts_with(".tmp") {
                continue;
            }
            let seq: u64 = name
                .strip_suffix(".json")
                .context("unknown journal file")?
                .parse()?;
            ensure!(
                name == format!("{seq:020}.json"),
                "unexpected journal filename"
            );
            ordered.insert(seq, item.path());
        }
        for (idx, (seq, path)) in ordered.into_iter().enumerate() {
            ensure!(seq == idx as u64 + 1, "portable journal gap");
            let data = String::from_utf8(read_entry(&path)?)?;
            let existing: Option<String> = self
                .db
                .query_row("SELECT data FROM journal WHERE seq=?", [seq], |r| r.get(0))
                .optional()?;
            if let Some(old) = existing {
                ensure!(old == data, "conflicting journal history");
                continue;
            }
            let e: Entry = serde_json::from_str(&data)?;
            let tx = self.db.transaction()?;
            append(&tx, &e, &data, false)?;
            tx.commit()?;
        }
        self.db.execute(
            "UPDATE journal SET published=0 WHERE seq > ?",
            [dir_seq(&dir)?],
        )?;
        self.publish()
    }
}
fn dir_seq(dir: &Path) -> Result<u64> {
    let mut count = 0;
    for e in dir.read_dir()? {
        if e?.path().extension().is_some_and(|s| s == "json") {
            count += 1;
        }
    }
    Ok(count)
}
fn sidecar_digest(b: &Book) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec_pretty(b)?)
    ))
}
fn tip(db: &Connection) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT id FROM journal ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}
fn append(db: &Connection, e: &Entry, data: &str, published: bool) -> Result<()> {
    ensure!(
        e.format == 1 && uuid_ok(&e.id) && e.parent == tip(db)?,
        "incompatible journal history"
    );
    match &e.change {
        Change::Book(b) => {
            b.validate()?;
            db.execute(
                "INSERT OR REPLACE INTO books VALUES(?,?)",
                params![b.id, serde_json::to_string(b)?],
            )?;
        }
        Change::Position { id, position } => {
            let raw: String =
                db.query_row("SELECT data FROM books WHERE id=?", [id], |r| r.get(0))?;
            ensure!(
                serde_json::from_str::<Book>(&raw)?.version_id == position.version_id,
                "position version mismatch"
            );
            db.execute(
                "INSERT OR REPLACE INTO positions VALUES(?,?)",
                params![id, serde_json::to_string(position)?],
            )?;
        }
    }
    db.execute(
        "INSERT INTO journal(id,data,published) VALUES(?,?,?)",
        params![e.id, data, published],
    )?;
    Ok(())
}

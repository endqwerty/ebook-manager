use crate::*;
pub(crate) fn open(path: &Path) -> Result<Connection> {
    let existing = path.try_exists()?;
    let mut db = Connection::open(path)?;
    db.execute_batch("PRAGMA trusted_schema=OFF")?;
    if existing {
        let meta: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='meta')",
            [],
            |r| r.get(0),
        )?;
        if !meta
            || db
                .query_row("SELECT value FROM meta WHERE key='version'", [], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()?
                != Some(1)
        {
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            drop(db);
            fs::rename(path, path.with_extension(format!("sqlite.old-{}", uid())))?;
            File::open(path.parent().unwrap())?.sync_all()?;
            db = Connection::open(path)?;
        }
    }
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
 BEGIN;
 CREATE TABLE IF NOT EXISTS book(n INTEGER PRIMARY KEY,id TEXT UNIQUE,sha256 TEXT,identifier TEXT,data TEXT,title TEXT,authors TEXT,author_sort TEXT,series TEXT,position TEXT,title_key BLOB,author_key BLOB,series_key BLOB,position_key BLOB,added INTEGER,published TEXT,last_read INTEGER,progress REAL,words INTEGER,language TEXT,spine_rgb INTEGER,cover_rgb INTEGER,cover_digest TEXT,version_id TEXT,rtl INTEGER,survey_version INTEGER);
 CREATE INDEX IF NOT EXISTS digest ON book(sha256);
 CREATE INDEX IF NOT EXISTS package ON book(identifier);
 CREATE TABLE IF NOT EXISTS search(n INTEGER PRIMARY KEY,folded TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS positions(id TEXT PRIMARY KEY,data TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS sources(path BLOB PRIMARY KEY,id TEXT NOT NULL);
 CREATE INDEX IF NOT EXISTS source_book ON sources(id);
 CREATE TABLE IF NOT EXISTS journal(seq INTEGER PRIMARY KEY,id TEXT UNIQUE NOT NULL,data TEXT NOT NULL,published INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value);
 INSERT OR IGNORE INTO meta VALUES('version',1),('counter',0),('tip',0);
 COMMIT;")?;
    Ok(db)
}

pub(crate) fn project(db: &Connection, b: &Book, keys: &sortkey::Keys) -> Result<()> {
    let m = &b.imported;
    let series = b.series.as_ref().unwrap_or(&m.series);
    let primary = series.first();
    let position = primary.and_then(|s| s.1.as_deref());
    let name = primary.map(|s| s.0.as_str());
    let author = b.authors.first().map(|a| {
        b.author_sorts
            .as_ref()
            .unwrap_or(&m.author_sorts)
            .first()
            .and_then(Clone::clone)
            .unwrap_or_else(|| match a.trim().rsplit_once(char::is_whitespace) {
                Some((given, last)) => format!("{last}, {}", given.trim()),
                None => a.clone(),
            })
    });
    let folded = std::iter::once(&b.title)
        .chain(&b.authors)
        .chain(series.iter().map(|s| &s.0))
        .map(|s| sortkey::fold(s))
        .collect::<Vec<_>>()
        .join("\u{0}");
    let survey = m.survey.as_ref();
    let cover = survey.and_then(|s| s.cover.as_ref());
    let data = serde_json::to_string(b)?;
    let n:i64=db.prepare_cached("INSERT INTO book(id,sha256,identifier,data,title,authors,author_sort,series,position,title_key,author_key,series_key,position_key,added,published,words,language,spine_rgb,cover_rgb,cover_digest,version_id,rtl,survey_version)
 VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET sha256=excluded.sha256,identifier=excluded.identifier,data=excluded.data,title=excluded.title,authors=excluded.authors,author_sort=excluded.author_sort,series=excluded.series,position=excluded.position,title_key=excluded.title_key,author_key=excluded.author_key,series_key=excluded.series_key,position_key=excluded.position_key,added=excluded.added,published=excluded.published,words=excluded.words,language=excluded.language,spine_rgb=excluded.spine_rgb,cover_rgb=excluded.cover_rgb,cover_digest=excluded.cover_digest,rtl=excluded.rtl,survey_version=excluded.survey_version,last_read=CASE WHEN version_id=excluded.version_id THEN last_read END,progress=CASE WHEN version_id=excluded.version_id THEN progress END,version_id=excluded.version_id RETURNING n")?.query_row(params![b.id,b.sha256,m.identifier,data,b.title,b.authors.join(", "),author,name,position,keys.key(sortkey::filing(b)),author.as_ref().map(|s|keys.key(s)),name.map(|s|keys.key(s)),position.map(sortkey::position),b.added,m.published,survey.and_then(|s|s.words),m.language,cover.map(|c|c.spine),cover.map(|c|c.average),cover.map(|c|&c.digest),b.version_id,m.right_to_left,survey.map(|s|s.version)],|r|r.get(0))?;
    db.prepare_cached("INSERT OR REPLACE INTO search VALUES(?,?)")?
        .execute(params![n, folded])?;
    Ok(())
}

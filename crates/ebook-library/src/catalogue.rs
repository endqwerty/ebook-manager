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
        if meta
            && db.query_row(
                "SELECT EXISTS(SELECT 1 FROM meta WHERE key='version' AND value=1)",
                [],
                |r| r.get::<_, bool>(0),
            )?
        {
            db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL")?;
            identify(&db)?;
            return Ok(db);
        }
        let busy: i32 = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        ensure!(busy == 0, "catalogue checkpoint is busy");
        drop(db);
        fs::rename(path, path.with_extension(format!("sqlite.old-{}", uid())))?;
        File::open(path.parent().unwrap())?.sync_all()?;
        db = Connection::open(path)?;
    }
    db.execute_batch(include_str!("schema.sql"))?;
    identify(&db)?;
    Ok(db)
}

/// Gives a catalogue file an identity of its own, once. A browse view
/// records it, so a view of one file is never read against another.
fn identify(db: &Connection) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO meta VALUES('identity',?)",
        [Uuid::new_v4().to_string()],
    )?;
    Ok(())
}

/// The identity of the catalogue file a connection is reading.
pub(crate) fn identity(db: &Connection) -> Result<String> {
    Ok(
        db.query_row("SELECT value FROM meta WHERE key='identity'", [], |r| {
            r.get(0)
        })?,
    )
}

pub(crate) fn project(db: &Connection, b: &Book, keys: &sortkey::Keys) -> Result<()> {
    let m = &b.imported;
    let series = b.series.as_ref().unwrap_or(&m.series);
    let primary = series.first();
    let position = primary.and_then(|s| s.1.as_deref());
    let name = primary.map(|s| s.0.as_str());
    let author = b.authors.first().filter(|s| !s.trim().is_empty()).map(|a| {
        b.author_sorts
            .as_ref()
            .or_else(|| (m.authors.first() == Some(a)).then_some(&m.author_sorts))
            .and_then(|sorts| sorts.first())
            .and_then(Clone::clone)
            .filter(|s| !s.trim().is_empty())
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
    let n: i64 = db.prepare_cached(include_str!("project.sql"))?.query_row(
        params![
            b.id,
            b.sha256,
            m.identifier,
            data,
            b.title,
            b.authors.join(", "),
            author,
            name,
            position,
            keys.key(sortkey::filing(b)),
            author.as_ref().map(|s| keys.key(s)),
            name.map(|s| keys.key(s)),
            position.map(sortkey::position).filter(|k| !k.is_empty()),
            b.added,
            m.published,
            survey.and_then(|s| s.words),
            m.language,
            cover.map(|c| c.spine),
            cover.map(|c| c.average),
            cover.map(|c| &c.digest),
            b.version_id,
            m.right_to_left,
            survey.map(|s| s.version)
        ],
        |r| r.get(0),
    )?;
    db.prepare_cached("INSERT OR REPLACE INTO search VALUES(?,?)")?
        .execute(params![n, folded])?;
    Ok(())
}

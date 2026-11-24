use crate::*;
impl Library {
    pub(super) fn recover(&mut self) -> Result<()> {
        let seq: u64 = self
            .db
            .query_row("SELECT value FROM meta WHERE key='tip'", [], |r| r.get(0))?;
        let changes = self.directory("changes")?;
        let matches = if seq == 0 {
            true
        } else {
            let bytes: String =
                self.db
                    .query_row("SELECT data FROM journal WHERE seq=?", [seq], |r| r.get(0))?;
            read_entry(&changes.join(format!("{seq:020}.json")))
                .is_ok_and(|v| v == bytes.as_bytes())
        };
        let successor = changes.join(format!("{:020}.json", seq + 1));
        let recovered: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key='recovered')",
            [],
            |r| r.get(0),
        )?;
        if recovered
            && matches
            && successor
                .symlink_metadata()
                .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
        {
            self.publish()?;
        } else {
            self.refresh()?;
            self.db
                .execute("INSERT OR IGNORE INTO meta VALUES('recovered',1)", [])?;
        }
        Ok(())
    }
    pub(super) fn commit(&mut self, change: Change, source: Option<(&[u8], &str)>) -> Result<()> {
        let book_id = match &change {
            Change::Book(b) => b.id.clone(),
            Change::Position { id, .. } => id.clone(),
        };
        let tx = self.db.transaction()?;
        let parent = tip(&tx)?;
        let e = Entry {
            format: 1,
            id: uid(),
            parent,
            change,
        };
        let data = serde_json::to_string(&e)?;
        append(&tx, &e, &data, &self.keys)?;
        if let Some((path, id)) = source {
            tx.execute(
                "INSERT OR REPLACE INTO sources VALUES(?,?)",
                params![path, id],
            )?;
        }
        tx.commit()?;
        self.publish().context(PublicationPending {
            book_id,
            outcome: None,
        })
    }
    pub(super) fn publish(&mut self) -> Result<()> {
        let dir = self.directory("changes")?;
        let pending = self
            .db
            .prepare("SELECT seq,data FROM journal WHERE seq>(SELECT value FROM meta WHERE key='tip') ORDER BY seq")?
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
            let tx = self.db.transaction()?;
            tx.execute("UPDATE journal SET published=1 WHERE seq=?", [seq])?;
            tx.execute("UPDATE meta SET value=? WHERE key='tip'", [seq])?;
            tx.commit()?;
        }
        Ok(())
    }
    fn sidecar(&self, b: &Book) -> Result<()> {
        b.validate()?;
        let dir = self.directory("sidecars")?;
        let path = dir.join(format!("{}-{}.json", b.id, sidecar_digest(b)?));
        let data = serde_json::to_vec_pretty(b)?;
        if path.try_exists()? {
            let edited = regular(&path)?;
            if edited.metadata()?.len() != data.len() as u64
                || digest(&mut &edited)? != format!("{:x}", Sha256::digest(&data))
            {
                preserve(&path, "proposal")?;
            }
        }
        if !path.try_exists()? {
            install(&path, &data)?;
        }
        File::open(dir)?.sync_all()?;
        Ok(())
    }
    pub fn refresh(&mut self) -> Result<()> {
        let dir = self.directory("changes")?;
        let mut ordered = BTreeMap::new();
        for item in dir.read_dir()? {
            let item = item?;
            let name = item.file_name();
            if item.path().extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let name = name.to_str().context("invalid journal filename")?;
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
        let published_count = ordered.len() as u64;
        let tx = self.db.transaction()?;
        for (idx, (seq, path)) in ordered.into_iter().enumerate() {
            ensure!(seq == idx as u64 + 1, "portable journal gap");
            let data = String::from_utf8(read_entry(&path)?)?;
            let existing: Option<String> = tx
                .query_row("SELECT data FROM journal WHERE seq=?", [seq], |r| r.get(0))
                .optional()?;
            if let Some(old) = existing {
                ensure!(old == data, "conflicting journal history");
                continue;
            }
            let e: Entry = serde_json::from_str(&data)?;
            append(&tx, &e, &data, &self.keys)?;
        }
        tx.execute(
            "UPDATE meta SET value=min(value,?) WHERE key='tip'",
            [published_count],
        )?;
        tx.execute(
            "UPDATE journal SET published=0 WHERE seq > ?",
            [published_count],
        )?;
        tx.commit()?;
        self.publish()
    }
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
pub(super) fn append(db: &Connection, e: &Entry, data: &str, keys: &sortkey::Keys) -> Result<()> {
    ensure!(
        e.format == 1 && uuid_ok(&e.id) && e.parent == tip(db)?,
        "incompatible journal history"
    );
    ensure!(data.len() <= 8 * 1024 * 1024, "record too large");
    match &e.change {
        Change::Book(b) => {
            b.validate()?;
            catalogue::project(db, b, keys)?;
        }
        Change::Position { id, position } => {
            ensure!(
                position.fraction.is_none_or(|f| (0.0..=1.0).contains(&f)),
                "invalid reading fraction"
            );
            let raw: String =
                db.query_row("SELECT data FROM book WHERE id=?", [id], |r| r.get(0))?;
            ensure!(
                serde_json::from_str::<Book>(&raw)?.version_id == position.version_id,
                "position version mismatch"
            );
            db.execute(
                "UPDATE book SET progress=?,last_read=? WHERE id=?",
                params![position.fraction, position.last_read, id],
            )?;
            db.execute(
                "INSERT OR REPLACE INTO positions VALUES(?,?)",
                params![id, serde_json::to_string(position)?],
            )?;
        }
    }
    db.execute("UPDATE meta SET value=value+1 WHERE key='counter'", [])?;
    db.execute(
        "INSERT INTO journal(id,data,published) VALUES(?,?,0)",
        params![e.id, data],
    )?;
    Ok(())
}

use crate::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Author,
    Series,
    Added,
    Published,
    LastRead,
    Progress,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortTerm {
    pub field: Field,
    pub descending: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortOrder(Vec<SortTerm>);
impl SortOrder {
    pub fn new(terms: Vec<SortTerm>) -> Result<Self> {
        ensure!(
            (1..=3).contains(&terms.len())
                && terms
                    .iter()
                    .enumerate()
                    .all(|(i, t)| !terms[..i].iter().any(|u| u.field == t.field)),
            "expected one to three distinct sort fields"
        );
        Ok(Self(terms))
    }
    fn sql(&self) -> String {
        let mut keys = Vec::new();
        for t in &self.0 {
            let cols: &[&str] = match t.field {
                Field::Title => &["title_key"],
                Field::Author => &["author_key"],
                Field::Series => &["series_key", "position_key"],
                Field::Added => &["added"],
                Field::Published => {
                    &["CASE WHEN published GLOB '[0-9][0-9][0-9][0-9]*' THEN published END"]
                }
                Field::LastRead => &["last_read"],
                Field::Progress => &["progress"],
            };
            for col in cols {
                keys.push(format!(
                    "{col} {} NULLS LAST",
                    if t.descending { "DESC" } else { "ASC" }
                ));
            }
        }
        keys.push("n".into());
        format!("SELECT n FROM book ORDER BY {}", keys.join(","))
    }
}
impl Default for SortOrder {
    fn default() -> Self {
        Self(vec![SortTerm {
            field: Field::Title,
            descending: false,
        }])
    }
}
#[derive(Default, Clone, Debug)]
pub struct BrowseQuery {
    pub text: String,
    pub sort: SortOrder,
}
#[derive(Default, Clone)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed)
    }
    fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct Cancelled;
#[derive(Debug)]
pub struct StaleView;
/// The catalogue file a reader was opened on has been replaced, as when it
/// is rebuilt from the portable folder. The reader is of no more use: ask
/// the library for a new one and open the query again.
#[derive(Debug)]
pub struct CatalogueReplaced;
impl std::fmt::Display for CatalogueReplaced {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the catalogue file was replaced")
    }
}
impl std::error::Error for CatalogueReplaced {}
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("browse cancelled")
    }
}
impl std::error::Error for Cancelled {}
impl std::fmt::Display for StaleView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stale browse view")
    }
}
impl std::error::Error for StaleView {}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ViewToken(Uuid);
#[derive(Debug)]
pub struct OpenView {
    pub token: ViewToken,
    pub count: u32,
    pub first: Vec<BrowseRow>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CoverRef {
    pub digest: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BrowseRow {
    pub id: String,
    pub version_id: String,
    pub title: String,
    pub authors: String,
    pub author_sort: Option<String>,
    pub series: Option<(String, String)>,
    pub added: Option<i64>,
    pub published: Option<String>,
    pub last_read: Option<i64>,
    pub progress: Option<f32>,
    pub cover: Option<CoverRef>,
    pub cover_rgb: Option<u32>,
    pub spine_rgb: Option<u32>,
    pub right_to_left: bool,
    pub protection: Option<Protection>,
}
pub struct ShelfStrip {
    pub thickness: Box<[u8]>,
    pub groups: Box<[(u32, String)]>,
}

/// Capped quick-jump matches across authors, series and books (#131).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickJump {
    pub authors: Vec<(String, u32)>,
    pub series: Vec<(String, u32)>,
    pub books: Vec<(String, String, String)>,
}
struct View {
    token: ViewToken,
    /// The catalogue file the view was read from.
    catalogue: String,
    counter: i64,
    sort: SortOrder,
    order: Arc<Vec<u32>>,
    visible: Arc<Vec<u32>>,
}
pub struct BrowseReader {
    db: Connection,
    path: PathBuf,
    /// The device and inode of the file the connection opened.
    file: (u64, u64),
    keys: sortkey::Keys,
    view: Option<View>,
}
impl Library {
    pub fn browse_reader(&self) -> Result<BrowseReader> {
        let path = self.portable.with_file_name("library.sqlite");
        let db = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(BrowseReader {
            file: file_at(&path)?,
            path,
            db,
            keys: sortkey::Keys::new()?,
            view: None,
        })
    }
}
fn file_at(path: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let found = fs::metadata(path)?;
    Ok((found.dev(), found.ino()))
}
fn counter(db: &Connection) -> Result<i64> {
    Ok(
        db.query_row("SELECT value FROM meta WHERE key='counter'", [], |r| {
            r.get(0)
        })?,
    )
}
fn page(db: &Connection, ids: &[u32]) -> Result<Vec<BrowseRow>> {
    let mut stmt=db.prepare_cached("SELECT id,version_id,title,authors,author_sort,series,coalesce(position,''),added,published,last_read,progress,cover_digest,cover_rgb,spine_rgb,rtl,protection FROM book WHERE n=?")?;
    ids.iter()
        .map(|n| {
            Ok(stmt.query_row([n], |r| {
                Ok(BrowseRow {
                    id: r.get(0)?,
                    version_id: r.get(1)?,
                    title: r.get(2)?,
                    authors: r.get(3)?,
                    author_sort: r.get(4)?,
                    series: r.get::<_, Option<String>>(5)?.zip(Some(r.get(6)?)),
                    added: r.get(7)?,
                    published: r.get(8)?,
                    last_read: r.get(9)?,
                    progress: r.get(10)?,
                    cover: r
                        .get::<_, Option<String>>(11)?
                        .map(|digest| CoverRef { digest }),
                    cover_rgb: r.get(12)?,
                    spine_rgb: r.get(13)?,
                    right_to_left: r.get(14)?,
                    protection: r
                        .get::<_, Option<String>>(15)?
                        .and_then(|name| Protection::from_name(&name)),
                })
            })?)
        })
        .collect()
}
impl BrowseReader {
    /// Fails when the file at the catalogue's path is no longer the one
    /// this reader opened. Its connection would go on reading the old file.
    fn same_file(&self) -> Result<()> {
        if file_at(&self.path).ok() != Some(self.file) {
            return Err(CatalogueReplaced.into());
        }
        Ok(())
    }
    pub fn open(&mut self, query: &BrowseQuery, cancel: &Cancel) -> Result<OpenView> {
        cancel.check()?;
        self.same_file()?;
        let tx = self.db.transaction()?;
        let catalogue = catalogue::identity(&tx).map_err(|_| CatalogueReplaced)?;
        let count = counter(&tx)?;
        let order = if let Some(v) = self
            .view
            .as_ref()
            .filter(|v| v.counter == count && v.sort == query.sort)
        {
            v.order.clone()
        } else {
            Arc::new(
                tx.prepare(&query.sort.sql())?
                    .query_map([], |r| r.get::<_, u32>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            )
        };
        cancel.check()?;
        let text = sortkey::fold(&query.text);
        let words: Vec<_> = text.split_whitespace().collect();
        let visible = if words.is_empty() {
            order.clone()
        } else {
            let mut matches = vec![false; order.iter().max().copied().unwrap_or(0) as usize + 1];
            let mut stmt = tx.prepare("SELECT n,folded FROM search")?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                cancel.check()?;
                let s = r.get_ref(1)?.as_str()?;
                matches[r.get::<_, u32>(0)? as usize] = words.iter().all(|w| s.contains(w));
            }
            Arc::new(
                order
                    .iter()
                    .copied()
                    .filter(|&n| matches[n as usize])
                    .collect(),
            )
        };
        let first = page(&tx, &visible[..visible.len().min(128)])?;
        cancel.check()?;
        tx.commit()?;
        let token = ViewToken(Uuid::new_v4());
        let count_rows = visible.len().try_into()?;
        self.view = Some(View {
            token,
            catalogue,
            counter: count,
            sort: query.sort.clone(),
            order,
            visible,
        });
        Ok(OpenView {
            token,
            count: count_rows,
            first,
        })
    }
    fn read<T>(
        &self,
        token: ViewToken,
        op: impl FnOnce(&Connection, &View) -> Result<T>,
    ) -> Result<T> {
        // A view stays readable after the catalogue changes: its rows are
        // fetched by row number, which never moves to another book.
        let view = self
            .view
            .as_ref()
            .filter(|v| v.token == token)
            .ok_or(StaleView)?;
        // That holds for one catalogue file. The identity is read in the
        // same transaction as the rows, so a view is never answered from a
        // file other than the one it was opened on.
        self.same_file()?;
        let tx = self
            .db
            .unchecked_transaction()
            .map_err(|_| CatalogueReplaced)?;
        if catalogue::identity(&tx).ok().as_ref() != Some(&view.catalogue) {
            return Err(CatalogueReplaced.into());
        }
        op(&tx, view)
    }
    pub fn changed(&mut self) -> Result<bool> {
        self.same_file()?;
        let count = counter(&self.db)?;
        Ok(self.view.as_ref().is_none_or(|v| v.counter != count))
    }
    pub fn window(&mut self, token: ViewToken, start: u32, len: u32) -> Result<Vec<BrowseRow>> {
        self.read(token, |db, v| {
            let rest = v.visible.get(start as usize..).unwrap_or_default();
            page(db, &rest[..rest.len().min(len as usize)])
        })
    }
    pub fn locate(&mut self, token: ViewToken, ids: &[String]) -> Result<Vec<Option<u32>>> {
        self.read(token, |db, v| {
            let mut wanted = std::collections::HashMap::new();
            let mut result = vec![None; ids.len()];
            let mut stmt = db.prepare("SELECT n FROM book WHERE id=?")?;
            for (i, id) in ids.iter().enumerate() {
                if let Some(n) = stmt.query_row([id], |r| r.get::<_, u32>(0)).optional()? {
                    wanted.entry(n).or_insert_with(Vec::new).push(i);
                }
            }
            for (rank, n) in v.visible.iter().enumerate() {
                if let Some(indices) = wanted.remove(n) {
                    for i in indices {
                        result[i] = Some(rank as u32)
                    }
                }
                if wanted.is_empty() {
                    break;
                }
            }
            Ok(result)
        })
    }
    pub fn shelf(&mut self, token: ViewToken) -> Result<ShelfStrip> {
        self.read(token, |db, v| shelf_rows(db, v, &self.keys))
    }
    /// Capped quick-jump search across authors, series and books, designed to
    /// run off the UI thread and stay fast at 100,000 books (#131).
    pub fn quick_jump(&mut self, text: &str, limit: usize, cancel: &Cancel) -> Result<QuickJump> {
        cancel.check()?;
        self.same_file()?;
        let folded = sortkey::fold(text);
        let words: Vec<&str> = folded.split_whitespace().collect();
        if words.is_empty() || limit == 0 {
            return Ok(QuickJump::default());
        }
        let tx = self.db.unchecked_transaction()?;
        let mut book_ns = Vec::with_capacity(limit);
        let mut author_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut series_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        {
            let mut stmt = tx.prepare_cached(
                "SELECT s.n, s.folded, b.authors, b.series FROM search s JOIN book b ON b.n = s.n ORDER BY b.title_key, b.n",
            )?;
            let mut rows = stmt.query([])?;
            let mut checked = 0u32;
            while let Some(r) = rows.next()? {
                checked = checked.wrapping_add(1);
                if checked.is_multiple_of(512) {
                    cancel.check()?;
                }
                let hay = r.get_ref(1)?.as_str()?;
                if !words.iter().all(|w| hay.contains(w)) {
                    continue;
                }
                if book_ns.len() < limit {
                    book_ns.push(r.get::<_, u32>(0)?);
                }
                let authors = r.get_ref(2)?.as_str()?;
                if !authors.is_empty() {
                    let a_folded = sortkey::fold(authors);
                    if words.iter().all(|w| a_folded.contains(w)) {
                        *author_counts.entry(authors.to_owned()).or_insert(0) += 1;
                    }
                }
                if let Some(series) = r.get_ref(3)?.as_str_or_null()?
                    && !series.is_empty()
                {
                    let s_folded = sortkey::fold(series);
                    if words.iter().all(|w| s_folded.contains(w)) {
                        *series_counts.entry(series.to_owned()).or_insert(0) += 1;
                    }
                }
            }
        }
        cancel.check()?;
        let mut books = Vec::with_capacity(book_ns.len());
        {
            let mut stmt = tx.prepare_cached("SELECT id, title, authors FROM book WHERE n = ?")?;
            for n in book_ns {
                if let Some(row) = stmt
                    .query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .optional()?
                {
                    books.push(row);
                }
            }
        }
        let mut authors: Vec<(String, u32)> = author_counts.into_iter().collect();
        authors.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        authors.truncate(limit.min(6));
        let mut series: Vec<(String, u32)> = series_counts.into_iter().collect();
        series.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        series.truncate(limit.min(6));
        Ok(QuickJump {
            authors,
            series,
            books,
        })
    }
    pub fn ids(&mut self, token: ViewToken, ranks: &[u32]) -> Result<Vec<String>> {
        self.read(token, |db, v| {
            let mut out = Vec::with_capacity(ranks.len());
            if ranks.len() <= 256 {
                let mut stmt = db.prepare("SELECT id FROM book WHERE n=?")?;
                for &rank in ranks {
                    if let Some(&n) = v.visible.get(rank as usize)
                        && let Some(id) =
                            stmt.query_row([n], |r| r.get::<_, String>(0)).optional()?
                    {
                        out.push(id);
                    }
                }
            } else {
                let max = v.order.iter().max().copied().unwrap_or_default() as usize;
                let mut by_n: Vec<Option<String>> = vec![None; max + 1];
                let mut stmt = db.prepare("SELECT n, id FROM book ORDER BY n")?;
                let mut rows = stmt.query([])?;
                while let Some(r) = rows.next()? {
                    let n = r.get::<_, u32>(0)? as usize;
                    if n > max {
                        break;
                    }
                    by_n[n] = Some(r.get(1)?);
                }
                for &rank in ranks {
                    if let Some(&n) = v.visible.get(rank as usize)
                        && let Some(id) = by_n.get(n as usize).and_then(|id| id.as_ref())
                    {
                        out.push(id.clone());
                    }
                }
            }
            Ok(out)
        })
    }
}
fn shelf_rows(db: &Connection, v: &View, keys: &sortkey::Keys) -> Result<ShelfStrip> {
    let by_letter = v.sort.0[0].field == Field::Author;
    let group = match v.sort.0[0].field {
        // The whole name: letters are compared as the locale files them.
        Field::Author => "coalesce(author_sort,'')",
        Field::Series => "coalesce(series,'No series')",
        Field::Added => "coalesce(strftime('%Y',added,'unixepoch'),'No date')",
        Field::LastRead => "coalesce(strftime('%Y',last_read,'unixepoch'),'No date')",
        Field::Published => {
            "CASE WHEN published GLOB '[0-9][0-9][0-9][0-9]*' THEN substr(published,1,4) ELSE 'No date' END"
        }
        _ => "NULL",
    };
    let mut stmt = db.prepare(&format!("SELECT n,words,{group} FROM book ORDER BY n"))?;
    let max = v.order.iter().max().copied().unwrap_or_default() as usize;
    let mut sizes = vec![24; max + 1];
    let mut names = vec![None; max + 1];
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let n = r.get::<_, u32>(0)? as usize;
        // A book added since the view opened is not in it.
        if n > max {
            break;
        }
        if let Some(words) = r.get::<_, Option<u32>>(1)? {
            sizes[n] = (16 + ((u64::from(words) + 6000) / 12000)).min(48) as u8;
        }
        names[n] = r.get::<_, Option<String>>(2)?;
    }
    let mut groups: Vec<(u32, String)> = vec![];
    let mut previous = None;
    let mut letters = std::collections::HashMap::new();
    let mut thickness = Vec::with_capacity(v.visible.len());
    for (rank, &n) in v.visible.iter().enumerate() {
        thickness.push(sizes[n as usize]);
        let Some(name) = &names[n as usize] else {
            continue;
        };
        if by_letter {
            // Names sharing a letter are adjacent in the view, so comparing
            // with the group before is enough.
            let (key, caption) = if name.is_empty() {
                (None, "Unknown author".into())
            } else {
                // A library has far fewer authors than books, and working
                // out a letter costs several collations.
                let (key, caption) = letters
                    .entry(sortkey::letter_head(name))
                    .or_insert_with(|| keys.letter_group(name))
                    .clone();
                (Some(key), caption)
            };
            if previous.as_ref() != Some(&key) {
                groups.push((rank as u32, caption));
                previous = Some(key);
            }
        } else if groups.last().is_none_or(|(_, last)| last != name) {
            groups.push((rank as u32, name.clone()));
        }
    }
    Ok(ShelfStrip {
        thickness: thickness.into(),
        groups: groups.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_resolves_small_and_large_rank_sets_and_rejects_stale_views() {
        let dir = tempfile::tempdir().unwrap();
        fixture::generate(dir.path(), 400).unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut reader = lib.browse_reader().unwrap();
        let view = reader
            .open(&BrowseQuery::default(), &Cancel::default())
            .unwrap();
        let first_two = reader.ids(view.token, &[0, 1]).unwrap();
        assert_eq!(
            first_two,
            vec![view.first[0].id.clone(), view.first[1].id.clone()]
        );
        let all_ranks: Vec<u32> = (0..view.count).collect();
        let all_ids = reader.ids(view.token, &all_ranks).unwrap();
        assert_eq!(all_ids.len(), 400);
        assert_eq!(all_ids[0], view.first[0].id);
        assert_eq!(all_ids[1], view.first[1].id);
        let newer = reader
            .open(
                &BrowseQuery {
                    text: "atlas".into(),
                    ..Default::default()
                },
                &Cancel::default(),
            )
            .unwrap();
        assert!(reader.ids(view.token, &[0]).is_err());
        assert!(!reader.ids(newer.token, &[0]).unwrap().is_empty());
    }

    #[test]
    fn quick_jump_returns_capped_authors_series_and_books() {
        let dir = tempfile::tempdir().unwrap();
        fixture::generate(dir.path(), 200).unwrap();
        let lib = Library::open(dir.path()).unwrap();
        let mut reader = lib.browse_reader().unwrap();
        let empty = reader.quick_jump("   ", 8, &Cancel::default()).unwrap();
        assert!(empty.books.is_empty() && empty.authors.is_empty() && empty.series.is_empty());
        let found = reader.quick_jump("atlas", 5, &Cancel::default()).unwrap();
        assert!(!found.books.is_empty());
        assert!(found.books.len() <= 5);
    }
}

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
}
pub struct ShelfStrip {
    pub thickness: Box<[u8]>,
    pub groups: Box<[(u32, String)]>,
}
struct View {
    token: ViewToken,
    counter: i64,
    sort: SortOrder,
    order: Arc<Vec<u32>>,
    visible: Arc<Vec<u32>>,
}
pub struct BrowseReader {
    db: Connection,
    keys: sortkey::Keys,
    view: Option<View>,
}
impl Library {
    pub fn browse_reader(&self) -> Result<BrowseReader> {
        Ok(BrowseReader {
            db: Connection::open_with_flags(
                self.portable.with_file_name("library.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?,
            keys: sortkey::Keys::new()?,
            view: None,
        })
    }
}
fn counter(db: &Connection) -> Result<i64> {
    Ok(
        db.query_row("SELECT value FROM meta WHERE key='counter'", [], |r| {
            r.get(0)
        })?,
    )
}
fn page(db: &Connection, ids: &[u32]) -> Result<Vec<BrowseRow>> {
    let mut stmt=db.prepare_cached("SELECT id,version_id,title,authors,author_sort,series,coalesce(position,''),added,published,last_read,progress,cover_digest,cover_rgb,spine_rgb,rtl FROM book WHERE n=?")?;
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
                })
            })?)
        })
        .collect()
}
impl BrowseReader {
    pub fn open(&mut self, query: &BrowseQuery, cancel: &Cancel) -> Result<OpenView> {
        cancel.check()?;
        let tx = self.db.transaction()?;
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
        let tx = self.db.unchecked_transaction()?;
        let count = counter(&tx)?;
        let view = self
            .view
            .as_ref()
            .filter(|v| v.token == token && v.counter == count)
            .ok_or(StaleView)?;
        op(&tx, view)
    }
    pub fn changed(&mut self) -> Result<bool> {
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
        if let Some(words) = r.get::<_, Option<u32>>(1)? {
            sizes[n] = (16 + ((u64::from(words) + 6000) / 12000)).min(48) as u8;
        }
        names[n] = r.get::<_, Option<String>>(2)?;
    }
    let mut groups: Vec<(u32, String)> = vec![];
    let mut previous = None;
    let mut thickness = Vec::with_capacity(v.visible.len());
    for (rank, &n) in v.visible.iter().enumerate() {
        thickness.push(sizes[n as usize]);
        let Some(name) = &names[n as usize] else {
            continue;
        };
        if by_letter {
            // Names sharing a letter are adjacent in the view, so comparing
            // with the group's first name is enough.
            if previous.is_none_or(|p: &String| {
                p.is_empty() != name.is_empty() || !keys.same_letter(p, name)
            }) {
                let caption = match name.chars().next() {
                    // The plain letter where the locale files this one under
                    // it, so the group reads `A` whether its first name
                    // starts with `a`, `A` or `Å`.
                    Some(c) => {
                        let own = c.to_string();
                        let plain = sortkey::fold(&own);
                        let letter = if keys.same_letter(&own, &plain) {
                            plain
                        } else {
                            own
                        };
                        letter.to_uppercase()
                    }
                    None => "Unknown author".into(),
                };
                groups.push((rank as u32, caption));
                previous = Some(name);
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

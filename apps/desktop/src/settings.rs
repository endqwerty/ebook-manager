//! The choices the window remembers: the view, each view's sort, the cover
//! size and the table's columns. They live in `settings.json` in the app's
//! config folder. A file that cannot be read, or holds something this build
//! does not know, gives the defaults for that part.
use ebook_library::{Field, SortTerm};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewKind {
    #[default]
    Grid,
    List,
    Table,
}

impl ViewKind {
    /// The views in the order the switch shows them, with each one's
    /// action target, name and icon. A fourth view is a fourth row.
    pub const ALL: [(ViewKind, &'static str, &'static str, &'static str); 3] = [
        (ViewKind::Grid, "grid", "Grid", "view-grid-symbolic"),
        (ViewKind::List, "list", "List", "view-list-symbolic"),
        (ViewKind::Table, "table", "Table", "view-dual-symbolic"),
    ];
    pub fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|v| v.0 == self)
            .map_or("grid", |v| v.1)
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().find(|v| v.1 == id).map(|v| v.0)
    }
    /// The view shown for a saved choice. The table needs a window at
    /// least 640 wide, and at a text scale of 1.5 or more the grid gives
    /// way to the list. The saved choice is not changed.
    pub fn shown(self, narrow: bool, text_scale: f64) -> Self {
        match self {
            ViewKind::Table if narrow => ViewKind::List,
            ViewKind::Grid if text_scale >= 1.5 => ViewKind::List,
            kind => kind,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CoverSize {
    pub const ALL: [(CoverSize, &'static str, &'static str); 3] = [
        (CoverSize::Small, "small", "_Small Covers"),
        (CoverSize::Medium, "medium", "_Medium Covers"),
        (CoverSize::Large, "large", "_Large Covers"),
    ];
    pub fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|s| s.0 == self)
            .map_or("medium", |s| s.1)
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().find(|s| s.1 == id).map(|s| s.0)
    }
    /// The width of a cover in the grid.
    pub fn grid(self) -> i32 {
        match self {
            CoverSize::Small => 96,
            CoverSize::Medium => 128,
            CoverSize::Large => 160,
        }
    }
    /// The width of a thumbnail in the list.
    pub fn list(self) -> i32 {
        match self {
            CoverSize::Small => 40,
            CoverSize::Medium => 48,
            CoverSize::Large => 60,
        }
    }
}

/// A fact a book can be sorted by, which is also a column of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortField {
    Title,
    Author,
    Series,
    Added,
    Published,
    LastRead,
    Progress,
}

impl SortField {
    /// Every field, with its action target and its name.
    pub const ALL: [(SortField, &'static str, &'static str); 7] = [
        (SortField::Title, "title", "Title"),
        (SortField::Author, "author", "Author"),
        (SortField::Series, "series", "Series"),
        (SortField::Added, "added", "Date Added"),
        (SortField::Published, "published", "Date Published"),
        (SortField::LastRead, "last-read", "Last Read"),
        (SortField::Progress, "progress", "Progress"),
    ];
    pub fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|f| f.0 == self)
            .map_or("title", |f| f.1)
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().find(|f| f.1 == id).map(|f| f.0)
    }
    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|f| f.0 == self).map_or("", |f| f.2)
    }
    fn field(self) -> Field {
        match self {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    pub field: SortField,
    #[serde(default)]
    pub descending: bool,
}

/// One to three keys, each a different field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Vec<SortKey>", into = "Vec<SortKey>")]
pub struct Sort(Vec<SortKey>);

impl Default for Sort {
    fn default() -> Self {
        Self(vec![SortKey {
            field: SortField::Title,
            descending: false,
        }])
    }
}

impl From<Vec<SortKey>> for Sort {
    /// Keeps the first three keys that name different fields. No keys at
    /// all is the default sort.
    fn from(keys: Vec<SortKey>) -> Self {
        let mut kept: Vec<SortKey> = vec![];
        for key in keys {
            if kept.len() < 3 && kept.iter().all(|k| k.field != key.field) {
                kept.push(key);
            }
        }
        if kept.is_empty() {
            Self::default()
        } else {
            Self(kept)
        }
    }
}

impl From<Sort> for Vec<SortKey> {
    fn from(sort: Sort) -> Self {
        sort.0
    }
}

impl Sort {
    pub fn keys(&self) -> &[SortKey] {
        &self.0
    }
    pub fn first(&self) -> SortKey {
        self.0[0]
    }
    pub fn terms(&self) -> Vec<SortTerm> {
        self.0
            .iter()
            .map(|key| SortTerm {
                field: key.field.field(),
                descending: key.descending,
            })
            .collect()
    }
    /// Sorting by one field alone, as choosing it in the Sort menu or
    /// activating its table header does. Choosing the first key again
    /// turns it round.
    pub fn by(&self, field: SortField) -> Self {
        let first = self.first();
        Self(vec![SortKey {
            field,
            descending: first.field == field && self.0.len() == 1 && !first.descending,
        }])
    }
    /// Adding a field as the next key, as activating its table header
    /// with Control held does. A field already a key is turned round, and
    /// a fourth is not added.
    pub fn then_by(&self, field: SortField) -> Self {
        let mut keys = self.0.clone();
        match keys.iter_mut().find(|key| key.field == field) {
            Some(key) => key.descending = !key.descending,
            None => keys.push(SortKey {
                field,
                descending: false,
            }),
        }
        keys.into()
    }
    pub fn with_direction(&self, descending: bool) -> Self {
        let mut keys = self.0.clone();
        keys[0].descending = descending;
        Self(keys)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub field: SortField,
    #[serde(default = "shown")]
    pub visible: bool,
    /// The width the reader gave it. Absent, the table decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
}

fn shown() -> bool {
    true
}

/// The table's columns in order: Title, Author, Series, Added and Progress
/// to start with, and the rest hidden after them.
fn default_columns() -> Vec<Column> {
    use SortField::*;
    [
        (Title, true),
        (Author, true),
        (Series, true),
        (Added, true),
        (Progress, true),
        (Published, false),
        (LastRead, false),
    ]
    .into_iter()
    .map(|(field, visible)| Column {
        field,
        visible,
        width: None,
    })
    .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sorts {
    pub grid: Sort,
    pub list: Sort,
    pub table: Sort,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub view: ViewKind,
    pub sort: Sorts,
    pub cover_size: CoverSize,
    pub columns: Vec<Column>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            view: ViewKind::default(),
            sort: Sorts::default(),
            cover_size: CoverSize::default(),
            columns: default_columns(),
        }
    }
}

impl Column {
    pub const MIN_WIDTH: i32 = 16;
    pub const MAX_WIDTH: i32 = 4096;
}

impl Settings {
    pub fn sort(&self, view: ViewKind) -> &Sort {
        match view {
            ViewKind::Grid => &self.sort.grid,
            ViewKind::List => &self.sort.list,
            ViewKind::Table => &self.sort.table,
        }
    }
    pub fn set_sort(&mut self, view: ViewKind, sort: Sort) {
        match view {
            ViewKind::Grid => self.sort.grid = sort,
            ViewKind::List => self.sort.list = sort,
            ViewKind::Table => self.sort.table = sort,
        }
    }

    /// Every field has one column, and Title is always shown.
    fn mend(mut self) -> Self {
        let mut columns: Vec<Column> = vec![];
        for column in self.columns {
            if columns.iter().all(|c| c.field != column.field) {
                columns.push(column);
            }
        }
        for column in default_columns() {
            if columns.iter().all(|c| c.field != column.field) {
                columns.push(Column {
                    visible: false,
                    ..column
                });
            }
        }
        for column in &mut columns {
            column.visible |= column.field == SortField::Title;
            column.width = column
                .width
                .filter(|width| (Column::MIN_WIDTH..=Column::MAX_WIDTH).contains(width));
        }
        self.columns = columns;
        self
    }

    /// Reads the file. Each part that is missing or not understood takes
    /// its default, and so does everything when the file is not JSON.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(serde_json::Value::Object(parts)) = serde_json::from_str(&text) else {
            return Self::default();
        };
        let defaults = Self::default();
        fn read<T: serde::de::DeserializeOwned>(value: Option<&serde_json::Value>) -> Option<T> {
            T::deserialize(value?).ok()
        }
        let part = |name: &str| parts.get(name);
        let sort =
            |view: &str| read(part("sort").and_then(|sort| sort.get(view))).unwrap_or_default();
        Self {
            view: read(part("view")).unwrap_or(defaults.view),
            sort: Sorts {
                grid: sort("grid"),
                list: sort("list"),
                table: sort("table"),
            },
            cover_size: read(part("cover_size")).unwrap_or(defaults.cover_size),
            columns: part("columns")
                .and_then(|value| value.as_array())
                .map(|columns| {
                    columns
                        .iter()
                        .filter_map(|column| read(Some(column)))
                        .collect()
                })
                .unwrap_or(defaults.columns),
        }
        .mend()
    }

    /// Writes the file through a temporary file and a rename.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let folder = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(folder)?;
        let mut staged = tempfile::NamedTempFile::new_in(folder)?;
        staged.write_all(serde_json::to_string_pretty(self)?.as_bytes())?;
        staged.persist(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(field: SortField, descending: bool) -> SortKey {
        SortKey { field, descending }
    }

    #[test]
    fn what_is_saved_is_what_is_loaded() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("nested/settings.json");
        let mut settings = Settings {
            view: ViewKind::Table,
            cover_size: CoverSize::Large,
            ..Default::default()
        };
        settings.set_sort(
            ViewKind::Table,
            vec![key(SortField::Series, true), key(SortField::Author, false)].into(),
        );
        settings.columns.swap(0, 1);
        settings.columns[2].visible = false;
        settings.columns[0].width = Some(180);
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
    }

    #[test]
    fn a_missing_or_damaged_file_gives_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let defaults = Settings::default();
        assert_eq!(defaults.view, ViewKind::Grid);
        assert_eq!(defaults.cover_size, CoverSize::Medium);
        assert_eq!(
            defaults.sort(ViewKind::List).keys(),
            [key(SortField::Title, false)]
        );
        let visible: Vec<_> = defaults
            .columns
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.field)
            .collect();
        assert_eq!(
            visible,
            [
                SortField::Title,
                SortField::Author,
                SortField::Series,
                SortField::Added,
                SortField::Progress
            ]
        );
    }

    #[test]
    fn a_part_this_build_does_not_know_leaves_the_others() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"view": "shelf", "cover_size": "large",
                "sort": {"grid": [{"field": "added", "descending": true}],
                         "list": [{"field": "future"}],
                         "table": [{"field": "progress"}]},
                "columns": [{"field": "future"}, {"field": "author", "width": 9}, {"field": "author"},
                            {"field": "title", "visible": false}]}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.view, ViewKind::Grid);
        assert_eq!(settings.cover_size, CoverSize::Large);
        assert_eq!(
            settings.sort(ViewKind::Grid).keys(),
            [key(SortField::Added, true)]
        );
        assert_eq!(settings.sort(ViewKind::List), &Sort::default());
        assert_eq!(
            settings.sort(ViewKind::Table).keys(),
            [key(SortField::Progress, false)]
        );
        // One column a field, in the saved order, Title always shown, an
        // unusable width dropped, and the fields not named added hidden.
        assert_eq!(
            settings.columns[..2],
            [
                Column {
                    field: SortField::Author,
                    visible: true,
                    width: None
                },
                Column {
                    field: SortField::Title,
                    visible: true,
                    width: None
                }
            ]
        );
        assert_eq!(settings.columns.len(), 7);
        assert!(settings.columns[2..].iter().all(|c| !c.visible));
    }

    #[test]
    fn a_sort_has_one_to_three_different_fields() {
        let sort: Sort = vec![
            key(SortField::Author, true),
            key(SortField::Author, false),
            key(SortField::Title, false),
            key(SortField::Series, false),
            key(SortField::Added, false),
        ]
        .into();
        assert_eq!(
            sort.keys(),
            [
                key(SortField::Author, true),
                key(SortField::Title, false),
                key(SortField::Series, false)
            ]
        );
        assert_eq!(Sort::from(vec![]), Sort::default());
    }

    #[test]
    fn choosing_a_field_sorts_by_it_alone_and_again_turns_it_round() {
        let sort = Sort::default().by(SortField::Author);
        assert_eq!(sort.keys(), [key(SortField::Author, false)]);
        let sort = sort.by(SortField::Author);
        assert_eq!(sort.keys(), [key(SortField::Author, true)]);
        assert_eq!(
            sort.by(SortField::Author).keys(),
            [key(SortField::Author, false)]
        );
        // From several keys, the field alone, ascending.
        let several = sort.then_by(SortField::Title);
        assert_eq!(
            several.by(SortField::Author).keys(),
            [key(SortField::Author, false)]
        );
    }

    #[test]
    fn adding_a_field_makes_it_the_next_key_up_to_three() {
        let sort = Sort::default()
            .then_by(SortField::Author)
            .then_by(SortField::Series);
        assert_eq!(
            sort.keys(),
            [
                key(SortField::Title, false),
                key(SortField::Author, false),
                key(SortField::Series, false)
            ]
        );
        assert_eq!(sort.then_by(SortField::Added), sort);
        assert_eq!(
            sort.then_by(SortField::Author).keys()[1],
            key(SortField::Author, true)
        );
    }

    #[test]
    fn the_view_shown_follows_the_window_and_the_text_size() {
        assert_eq!(ViewKind::Table.shown(false, 1.0), ViewKind::Table);
        assert_eq!(ViewKind::Table.shown(true, 1.0), ViewKind::List);
        assert_eq!(ViewKind::Grid.shown(true, 1.49), ViewKind::Grid);
        assert_eq!(ViewKind::Grid.shown(false, 1.5), ViewKind::List);
        assert_eq!(ViewKind::List.shown(true, 2.0), ViewKind::List);
    }
}

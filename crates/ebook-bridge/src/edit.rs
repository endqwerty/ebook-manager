//! Pure filing reconciliation, chapter progress calculation, and XHTML document wrapping.

use crate::{BookEdit, StyleProperty};
use ebook_library::Book;
use ebook_reader::ChapterInfo;

type SeriesList = Vec<(String, Option<String>)>;

#[derive(Debug, PartialEq)]
pub struct FilingChanges {
    pub sort_title: Option<String>,
    pub author_sorts: Option<Vec<Option<String>>>,
    pub series: Option<SeriesList>,
}

#[derive(Debug, PartialEq)]
pub struct PlannedEdit {
    pub names: Option<(String, Vec<String>)>,
    pub filing: Option<FilingChanges>,
}

fn filled(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn series_of(book: &Book) -> &SeriesList {
    book.series.as_ref().unwrap_or(&book.imported.series)
}

pub fn plan_edit(book: &Book, edit: &BookEdit) -> PlannedEdit {
    let title = edit.title.trim().to_owned();
    let authors: Vec<String> = edit
        .authors
        .iter()
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty())
        .collect();
    let mut sorts: Vec<Option<String>> = edit
        .author_sorts
        .iter()
        .map(|line| filled(line.as_deref()))
        .collect();
    sorts.resize(authors.len(), None);
    let mut imported_sorts = book.imported.author_sorts.clone();
    imported_sorts.resize(authors.len(), None);
    let mut series: SeriesList = filled(edit.series_name.as_deref())
        .map(|name| (name, filled(edit.series_position.as_deref())))
        .into_iter()
        .collect();
    series.extend(series_of(book).iter().skip(1).cloned());
    let filing = FilingChanges {
        sort_title: filled(edit.sort_title.as_deref()),
        author_sorts: (sorts != imported_sorts).then_some(sorts),
        series: (series != book.imported.series).then_some(series),
    };
    let current = FilingChanges {
        sort_title: book.sort_title.clone(),
        author_sorts: book.author_sorts.clone(),
        series: book.series.clone(),
    };
    PlannedEdit {
        names: (title != book.title || authors != book.authors).then_some((title, authors)),
        filing: (filing != current).then_some(filing),
    }
}

pub fn progress_fraction(chapters: &[ChapterInfo], chapter: usize, page: u32, pages: u32) -> f32 {
    let even = chapters.iter().all(|c| !c.linear || c.size == 0);
    let weight = |c: &ChapterInfo| match (c.linear, even) {
        (false, _) => 0.0,
        (true, true) => 1.0,
        (true, false) => f64::from(c.size),
    };
    let total: f64 = chapters.iter().map(weight).sum();
    let Some(current) = chapters.get(chapter) else {
        return 1.0;
    };
    if total <= 0.0 {
        return 0.0;
    }
    let before: f64 = chapters[..chapter].iter().map(weight).sum();
    let through = if !current.linear {
        0.0
    } else if pages == 0 || page + 1 >= pages {
        1.0
    } else {
        f64::from(page) / f64::from(pages)
    };
    let read = before + weight(current) * through;
    if read >= total {
        1.0
    } else {
        (read / total) as f32
    }
}

fn escape_attribute(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

fn property_name(name: &str) -> bool {
    name.strip_prefix("--").is_some_and(|rest| {
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    })
}

fn property_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " #.,%()-'".contains(c))
}

pub fn wrap_xhtml(
    markup: &str,
    language: Option<&str>,
    right_to_left: bool,
    styles: &[StyleProperty],
) -> String {
    let style: String = styles
        .iter()
        .filter(|p| property_name(&p.name) && property_value(&p.value))
        .map(|p| format!("{}:{};", p.name, p.value))
        .collect();
    let language = language
        .map(|l| format!(r#" lang="{}""#, escape_attribute(l)))
        .unwrap_or_default();
    let direction = if right_to_left { "rtl" } else { "ltr" };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"{language} dir=\"{direction}\" style=\"{}\"><head><title>Chapter</title></head><body>{markup}</body></html>",
        escape_attribute(&style)
    )
}

//! The list: a `GtkListView` whose row is a small cover, three lines of
//! text and one short fact at the trailing edge.
use super::{
    BookCell, Shared, fact, finished, finished_mark, item_factory, percent, series, spoken,
    truncating,
};
use crate::{
    browse::BookListModel,
    cover::{Cover, Kind},
    settings::SortField,
    views::grid::EDGE,
};
use ebook_library::BrowseRow;
use gtk::prelude::*;
use gtk4 as gtk;

/// What the trailing slot says: how far a book has been read, or the fact
/// the list is sorted by when no line of the row shows it. A finished book
/// gets the mark, which is not text.
fn trailing(book: &BrowseRow, sorted_by: SortField) -> String {
    match sorted_by {
        SortField::Added | SortField::Published | SortField::LastRead => fact(book, sorted_by),
        _ => percent(book.progress).unwrap_or_default(),
    }
}

fn line(class: &str) -> gtk::Label {
    let label = gtk::Label::builder().xalign(0.0).build();
    label.add_css_class(class);
    truncating(&label);
    label
}

pub fn build(model: &BookListModel, shared: &Shared) -> gtk::ListView {
    let shared = shared.clone();
    let factory = item_factory(move |item| {
        let cover = Cover::new(Kind::List, shared.cover_size.list(), &shared.thumbs);
        cover.set_valign(gtk::Align::Center);
        cover.set_margin_top(12);
        cover.set_margin_bottom(12);
        let title = line("heading");
        let author = line("body");
        author.add_css_class("dimmed");
        let in_series = line("caption");
        in_series.add_css_class("dimmed");
        // The three lines are centred against the cover as a block.
        let lines = gtk::Box::new(gtk::Orientation::Vertical, 2);
        lines.set_valign(gtk::Align::Center);
        lines.set_hexpand(true);
        lines.append(&title);
        lines.append(&author);
        lines.append(&in_series);
        let said = gtk::Label::new(None);
        said.add_css_class("caption");
        said.add_css_class("dimmed");
        said.add_css_class("numeric");
        let done = finished_mark();
        // The separator starts at the text's edge and not under the cover.
        let text = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        text.add_css_class("book-row-text");
        text.append(&lines);
        text.append(&said);
        text.append(&done);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(EDGE);
        content.append(&cover);
        content.append(&text);
        let weak = item.downgrade();
        let sorted_by = shared.sorted_by.clone();
        BookCell::new(&content, &shared, move |_, book| {
            cover.set_book(book);
            title.set_text(book.map_or("", |book| &book.title));
            author.set_text(book.map_or("", |book| &book.authors));
            in_series.set_text(&book.map(|book| series(book)).unwrap_or_default());
            let dated = !matches!(
                sorted_by.get(),
                SortField::Title | SortField::Author | SortField::Series | SortField::Progress
            );
            said.set_text(
                &book
                    .map(|book| trailing(book, sorted_by.get()))
                    .unwrap_or_default(),
            );
            done.set_visible(!dated && book.is_some_and(|book| finished(book.progress)));
            if let Some(item) = weak.upgrade() {
                item.set_accessible_label(&book.map(|book| spoken(book)).unwrap_or_default());
            }
        })
    });
    let list = gtk::ListView::new(Some(model.clone()), Some(factory));
    list.add_css_class("books");
    list.set_enable_rubberband(true);
    list.update_property(&[gtk::accessible::Property::Label("Books")]);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> BrowseRow {
        BrowseRow {
            id: String::new(),
            version_id: String::new(),
            title: "Salt and Iron".into(),
            authors: "Tomas Reyes".into(),
            author_sort: None,
            series: Some(("Orchard".into(), "2".into())),
            added: None,
            published: Some("2019-04".into()),
            last_read: None,
            progress: Some(0.62),
            cover: None,
            cover_rgb: None,
            spine_rgb: None,
            right_to_left: false,
            protection: None,
        }
    }

    #[test]
    fn the_trailing_slot_gives_the_percentage_read() {
        assert_eq!(trailing(&book(), SortField::Title), "62%");
        assert_eq!(trailing(&book(), SortField::Series), "62%");
        let unread = BrowseRow {
            progress: None,
            ..book()
        };
        assert_eq!(trailing(&unread, SortField::Author), "");
    }

    #[test]
    fn a_sort_by_a_fact_no_line_shows_puts_it_in_the_trailing_slot() {
        assert_eq!(trailing(&book(), SortField::Published), "2019-04");
        // A book with no such fact has an empty slot.
        assert_eq!(trailing(&book(), SortField::LastRead), "");
    }

    #[test]
    fn a_series_line_gives_the_name_and_the_position() {
        assert_eq!(series(&book()), "Orchard · 2");
        let unnumbered = BrowseRow {
            series: Some(("Orchard".into(), String::new())),
            ..book()
        };
        assert_eq!(series(&unnumbered), "Orchard");
    }
}

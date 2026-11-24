//! The cover grid: a `GtkGridView` whose cell is a cover over a caption.
use super::{
    BookCell, Shared, finished, finished_mark, item_factory, lock_mark, show_lock, spoken,
    truncating,
};
use crate::{
    browse::BookListModel,
    cover::{Cover, Kind},
};
use gtk::{pango, prelude::*};
use gtk4 as gtk;

/// The leading edge of the content: where the screen title and the first
/// cover in each row start.
pub const EDGE: i32 = 24;
/// Between columns.
const GAP: i32 = 20;
/// Between a caption block and the next row.
const ROW_GAP: i32 = 28;
/// What the grid's own item adds around a cell, set in `style.css`.
const ITEM_PADDING: i32 = 6;

/// How many columns a grid `width` wide has for covers of the chosen width,
/// and how much of the trailing side stays empty.
///
/// With a row W wide and a cover width c, multiplied by the text scale, there
/// are floor((W + 20) / (c + 20)) columns and never fewer than one. Columns
/// share the row until a cover would be wider than 1.25 c. Past that the
/// grid keeps to the leading edge.
pub fn layout(width: i32, cover: i32, text_scale: f64) -> (u32, i32) {
    let row = (width - 2 * EDGE).max(1);
    let cover = (f64::from(cover) * text_scale).round() as i32;
    let columns = ((row + GAP) / (cover + GAP)).max(1);
    let widest = columns * (cover * 5 / 4 + GAP);
    (columns as u32, (row + GAP - widest).max(0))
}

/// Sets the grid's columns and margins for the width it has.
pub fn fit(grid: &gtk::GridView, width: i32, cover: i32, text_scale: f64) {
    let (columns, spare) = layout(width, cover, text_scale);
    // Each column carries half a gap on both sides, so the grid starts half
    // a gap before the leading edge.
    grid.set_margin_start(EDGE - GAP / 2);
    grid.set_margin_end(EDGE - GAP / 2 + spare);
    grid.set_min_columns(columns);
    grid.set_max_columns(columns);
}

fn caption_label(class: &str, lines: i32) -> gtk::Label {
    let label = gtk::Label::builder()
        .xalign(0.0)
        .yalign(0.0)
        .lines(lines)
        .wrap(lines > 1)
        .wrap_mode(pango::WrapMode::WordChar)
        .build();
    label.add_css_class(class);
    truncating(&label);
    label
}

pub fn build(model: &BookListModel, shared: &Shared) -> gtk::GridView {
    let shared = shared.clone();
    let factory = item_factory(move |item| {
        let cover = Cover::new(Kind::Grid, shared.cover_size.grid(), &shared.thumbs);
        let title = caption_label("heading", 2);
        let author = caption_label("caption", 1);
        author.add_css_class("dimmed");
        author.set_hexpand(true);
        // The mark has a slot of its own, so only the name is cut short.
        let done = finished_mark();
        let lock = lock_mark();
        let byline = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        byline.append(&author);
        byline.append(&done);
        byline.append(&lock);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_valign(gtk::Align::Start);
        text.append(&title);
        text.append(&byline);
        // The caption keeps the height of two title lines and an author
        // line whatever the title needs, so the next row starts level.
        let sizer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (class, lines) in [("heading", "\n"), ("caption", "")] {
            let line = gtk::Label::builder()
                .label(lines)
                .accessible_role(gtk::AccessibleRole::Presentation)
                .build();
            line.add_css_class(class);
            sizer.append(&line);
        }
        sizer.set_opacity(0.0);
        sizer.set_can_target(false);
        let caption = gtk::Overlay::new();
        caption.set_child(Some(&sizer));
        caption.add_overlay(&text);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_margin_start(GAP / 2 - ITEM_PADDING);
        content.set_margin_end(GAP / 2 - ITEM_PADDING);
        content.set_margin_top(GAP / 2 - ITEM_PADDING);
        content.set_margin_bottom(ROW_GAP - GAP / 2 - ITEM_PADDING);
        content.append(&cover);
        content.append(&caption);
        let weak = item.downgrade();
        BookCell::new(&content, &shared, move |_, book| {
            cover.set_book(book);
            title.set_text(book.map_or("", |book| &book.title));
            author.set_text(book.map_or("", |book| &book.authors));
            done.set_visible(book.is_some_and(|book| finished(book.progress)));
            show_lock(&lock, book);
            if let Some(item) = weak.upgrade() {
                item.set_accessible_label(&book.map(|book| spoken(book)).unwrap_or_default());
            }
        })
    });
    let grid = gtk::GridView::new(Some(model.clone()), Some(factory));
    grid.add_css_class("books");
    grid.set_enable_rubberband(true);
    grid.update_property(&[gtk::accessible::Property::Label("Books")]);
    grid
}

#[cfg(test)]
mod tests {
    use super::layout;

    #[test]
    fn columns_come_from_the_row_and_the_cover_width() {
        // A row of 1052: floor(1072 / 148) is 7, and each cover is 133.
        assert_eq!(layout(1100, 128, 1.0), (7, 0));
        // One unit short of an eighth column.
        assert_eq!(layout(8 * 148 - 20 + 48 - 1, 128, 1.0), (7, 0));
        assert_eq!(layout(8 * 148 - 20 + 48, 128, 1.0), (8, 0));
        // Small and large covers.
        assert_eq!(layout(1100, 96, 1.0).0, 9);
        assert_eq!(layout(1100, 160, 1.0).0, 5);
    }

    #[test]
    fn a_cover_never_grows_past_a_quarter_over_its_width() {
        // A row of 352 has 2 columns, which would make covers 166 wide.
        // They stay at 160, and 12 is left at the trailing side.
        assert_eq!(layout(400, 128, 1.0), (2, 12));
        // One column: the cover is 160 in a row of 252.
        assert_eq!(layout(300, 128, 1.0), (1, 92));
    }

    #[test]
    fn a_narrow_row_still_has_one_column() {
        assert_eq!(layout(120, 128, 1.0), (1, 0));
        assert_eq!(layout(10, 128, 1.0), (1, 0));
    }

    #[test]
    fn larger_text_takes_columns_away() {
        // 128 at a text scale of 1.5 is 192: floor(1072 / 212) is 5.
        assert_eq!(layout(1100, 128, 1.5).0, 5);
        assert_eq!(layout(1100, 128, 3.0).0, 2);
    }
}

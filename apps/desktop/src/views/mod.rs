//! What the ways of showing books share: the cell that ties a row widget to
//! its place in the model, and finding cells again on screen.
pub mod grid;
pub mod list;
pub mod table;

use crate::{
    browse::BookObject,
    settings::{CoverSize, SortField},
    thumbs::Thumbs,
};
use ebook_library::BrowseRow;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type OpenMenu = dyn Fn(&BookCell, f64, f64);

/// What every view is built with.
#[derive(Clone)]
pub struct Shared {
    pub thumbs: Thumbs,
    /// Opens the context menu for a cell, at a point in the cell.
    pub menu: Rc<OpenMenu>,
    pub cover_size: CoverSize,
    /// The first sort key, which the list shows when no line of a row does.
    pub sorted_by: Rc<Cell<SortField>>,
}

/// The view on screen.
pub enum View {
    Grid(gtk::GridView),
    List(gtk::ListView),
    Table(table::Table),
}

impl View {
    pub fn widget(&self) -> &gtk::Widget {
        match self {
            View::Grid(view) => view.upcast_ref(),
            View::List(view) => view.upcast_ref(),
            View::Table(table) => table.view.upcast_ref(),
        }
    }
    /// The part of the view that scrolls: all of a grid or a list, and a
    /// table without its headers. Places on screen are measured in it.
    pub fn body(&self) -> gtk::Widget {
        if let View::Table(table) = self {
            let mut child = table.view.first_child();
            while let Some(widget) = child {
                if widget.is::<gtk::ListView>() {
                    return widget;
                }
                child = widget.next_sibling();
            }
        }
        self.widget().clone()
    }
    pub fn scroll_to(&self, rank: u32, flags: gtk::ListScrollFlags) {
        match self {
            View::Grid(view) => view.scroll_to(rank, flags, None),
            View::List(view) => view.scroll_to(rank, flags, None),
            View::Table(table) => table.view.scroll_to(rank, None, flags, None),
        }
    }
    pub fn connect_activate(&self, activated: impl Fn(u32) + 'static) {
        match self {
            View::Grid(view) => view.connect_activate(move |_, rank| activated(rank)),
            View::List(view) => view.connect_activate(move |_, rank| activated(rank)),
            View::Table(table) => table.view.connect_activate(move |_, rank| activated(rank)),
        };
    }
}

/// A factory whose items each hold a `BookCell` made by `build` and follow
/// the place in the model they are given.
fn cell_factory<T: IsA<glib::Object>>(
    build: impl Fn(&T) -> BookCell + 'static,
    set_child: fn(&T, &BookCell),
    child: fn(&T) -> Option<gtk::Widget>,
    item: fn(&T) -> Option<glib::Object>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, slot| {
        if let Some(slot) = slot.downcast_ref::<T>() {
            set_child(slot, &build(slot));
        }
    });
    factory.connect_bind(move |_, slot| {
        if let Some(slot) = slot.downcast_ref::<T>()
            && let (Some(cell), Some(book)) = (
                child(slot).and_downcast::<BookCell>(),
                item(slot).and_downcast::<BookObject>(),
            )
        {
            cell.bind(&book);
        }
    });
    factory.connect_unbind(move |_, slot| {
        if let Some(cell) = slot
            .downcast_ref::<T>()
            .and_then(child)
            .and_downcast::<BookCell>()
        {
            cell.unbind();
        }
    });
    factory
}

/// For a grid or a list.
pub fn item_factory(
    build: impl Fn(&gtk::ListItem) -> BookCell + 'static,
) -> gtk::SignalListItemFactory {
    cell_factory::<gtk::ListItem>(
        build,
        |item, cell| item.set_child(Some(cell)),
        |item| item.child(),
        |item| item.item(),
    )
}

/// For a column of the table.
pub fn column_factory(build: impl Fn() -> BookCell + 'static) -> gtk::SignalListItemFactory {
    cell_factory::<gtk::ColumnViewCell>(
        move |_| build(),
        |cell, child| cell.set_child(Some(child)),
        |cell| cell.child(),
        |cell| cell.item(),
    )
}

/// A label that shows its whole text in a tooltip when it is cut short.
pub fn truncating(label: &gtk::Label) {
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_has_tooltip(true);
    label.connect_query_tooltip(|label, _, _, _, tooltip| {
        let cut = label.layout().is_ellipsized();
        if cut {
            tooltip.set_text(Some(label.text().as_str()));
        }
        cut
    });
}

/// The mark after a finished book's author.
pub fn finished_mark() -> gtk::Image {
    let done = gtk::Image::from_icon_name("object-select-symbolic");
    done.set_pixel_size(12);
    done.add_css_class("dimmed");
    done.set_accessible_role(gtk::AccessibleRole::Presentation);
    done
}

/// A day, from seconds since 1970 in UTC, in the reader's time zone.
pub fn day(seconds: i64) -> String {
    glib::DateTime::from_unix_local(seconds)
        .and_then(|time| time.format("%-e %b %Y"))
        .map(Into::into)
        .unwrap_or_default()
}

/// The series and the book's place in it.
pub fn series(book: &BrowseRow) -> String {
    match &book.series {
        Some((name, position)) if position.is_empty() => name.clone(),
        Some((name, position)) => format!("{name} · {position}"),
        None => String::new(),
    }
}

/// What a column of the table, or the list's trailing slot, says of a book.
/// A finished book's progress is a mark and not text.
pub fn fact(book: &BrowseRow, field: SortField) -> String {
    match field {
        SortField::Title => book.title.clone(),
        SortField::Author => book.authors.clone(),
        SortField::Series => series(book),
        SortField::Added => book.added.map(day).unwrap_or_default(),
        SortField::Published => book.published.clone().unwrap_or_default(),
        SortField::LastRead => book.last_read.map(day).unwrap_or_default(),
        SortField::Progress => percent(book.progress).unwrap_or_default(),
    }
}

type Update = Box<dyn Fn(u32, Option<&Rc<BrowseRow>>)>;

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct BookCell {
        pub book: RefCell<Option<(BookObject, glib::SignalHandlerId)>>,
        pub update: RefCell<Option<Update>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for BookCell {
        const NAME: &'static str = "EbookManagerBookCell";
        type Type = super::BookCell;
        type ParentType = adw::Bin;
    }
    impl ObjectImpl for BookCell {}
    impl WidgetImpl for BookCell {}
    impl BinImpl for BookCell {}
}

glib::wrapper! {
    /// The widget a list item shows. It follows one place in the model and
    /// redraws when that place's book arrives.
    pub struct BookCell(ObjectSubclass<imp::BookCell>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl BookCell {
    /// `update` is given the rank and, once loaded, the book.
    pub fn new(
        child: &impl IsA<gtk::Widget>,
        shared: &Shared,
        update: impl Fn(u32, Option<&Rc<BrowseRow>>) + 'static,
    ) -> Self {
        let cell: Self = glib::Object::new();
        cell.set_child(Some(child));
        cell.imp().update.replace(Some(Box::new(update)));
        let secondary = gtk::GestureClick::builder().button(3).build();
        let menu = shared.menu.clone();
        let weak = cell.downgrade();
        secondary.connect_pressed(move |gesture, _, x, y| {
            if let Some(cell) = weak.upgrade() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                menu(&cell, x, y);
            }
        });
        cell.add_controller(secondary);
        cell
    }

    pub fn bind(&self, book: &BookObject) {
        self.unbind();
        let weak = self.downgrade();
        let handler = book.connect_loaded_notify(move |book| {
            if let Some(cell) = weak.upgrade() {
                cell.show(book);
            }
        });
        self.show(book);
        self.imp().book.replace(Some((book.clone(), handler)));
    }

    pub fn unbind(&self) {
        if let Some((book, handler)) = self.imp().book.take() {
            book.disconnect(handler);
        }
    }

    fn show(&self, book: &BookObject) {
        if let Some(update) = &*self.imp().update.borrow() {
            update(book.rank(), book.row().as_ref());
        }
    }

    pub fn book(&self) -> Option<BookObject> {
        self.imp()
            .book
            .borrow()
            .as_ref()
            .map(|(book, _)| book.clone())
    }
}

/// Every cell under `root` that is showing a place in the model.
pub fn cells(root: &gtk::Widget) -> Vec<BookCell> {
    fn walk(widget: &gtk::Widget, depth: u32, found: &mut Vec<BookCell>) {
        if let Some(cell) = widget.downcast_ref::<BookCell>() {
            found.push(cell.clone());
            return;
        }
        // From the scrolled window, a table's cell is a table, a list, a row
        // and a column cell away.
        if depth == 0 {
            return;
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            walk(&widget, depth - 1, found);
            child = widget.next_sibling();
        }
    }
    let mut found = vec![];
    walk(root, 6, &mut found);
    found
}

/// The first cell on screen in a view's body, with where its top is from
/// the top of the body: zero or less when it starts above.
pub fn top_cell(body: &gtk::Widget) -> Option<(BookCell, f32)> {
    cells(body)
        .into_iter()
        .filter(|cell| cell.is_mapped() && cell.book().is_some())
        .filter_map(|cell| {
            let bounds = cell.compute_bounds(body)?;
            (bounds.y() + bounds.height() > 0.0).then_some((cell, bounds))
        })
        .min_by(|(_, a), (_, b)| {
            (a.y(), a.x())
                .partial_cmp(&(b.y(), b.x()))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(cell, bounds)| (cell, bounds.y()))
}

/// The cell that holds the keyboard focus, when a book does.
pub fn focused_cell(scrolled: &gtk::ScrolledWindow) -> Option<BookCell> {
    let focus = scrolled.root()?.focus()?;
    if !focus.is_ancestor(scrolled) {
        return None;
    }
    cells(&focus).into_iter().next().or_else(|| {
        // Focus inside a cell rather than on its row.
        focus.ancestor(BookCell::static_type()).and_downcast()
    })
}

/// What a screen reader says for a book: its title, authors and, in words,
/// how far it has been read.
pub fn spoken(book: &BrowseRow) -> String {
    let mut parts = vec![book.title.clone()];
    if !book.authors.is_empty() {
        parts.push(book.authors.clone());
    }
    parts.extend(reading_state(book.progress));
    parts.join(", ")
}

pub fn finished(progress: Option<f32>) -> bool {
    progress.is_some_and(|read| read >= 1.0)
}

/// The percentage read, for a book that is partly read.
pub fn percent(progress: Option<f32>) -> Option<String> {
    let read = progress.filter(|read| *read > 0.0 && *read < 1.0)?;
    // Never 0% for a started book or 100% for an unfinished one.
    Some(format!("{}%", ((read * 100.0).round() as u32).clamp(1, 99)))
}

fn reading_state(progress: Option<f32>) -> Option<String> {
    if finished(progress) {
        Some("Finished".into())
    } else {
        percent(progress).map(|percent| format!("{percent} read"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_state_is_said_in_words() {
        assert_eq!(reading_state(None), None);
        assert_eq!(reading_state(Some(0.0)), None);
        assert_eq!(reading_state(Some(0.004)).as_deref(), Some("1% read"));
        assert_eq!(reading_state(Some(0.62)).as_deref(), Some("62% read"));
        assert_eq!(reading_state(Some(0.999)).as_deref(), Some("99% read"));
        assert_eq!(reading_state(Some(1.0)).as_deref(), Some("Finished"));
    }
}

//! What the ways of showing books share: the cell that ties a row widget to
//! its place in the model, and finding cells again on screen.
pub mod grid;

use crate::{browse::BookObject, thumbs::Thumbs};
use ebook_library::BrowseRow;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::{cell::RefCell, rc::Rc};

type OpenMenu = dyn Fn(&BookCell, f64, f64);

/// What every view is built with.
#[derive(Clone)]
pub struct Shared {
    pub thumbs: Thumbs,
    /// Opens the context menu for a cell, at a point in the cell.
    pub menu: Rc<OpenMenu>,
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
        // A cell is at most a row, a column cell and a list away.
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
    walk(root, 4, &mut found);
    found
}

/// The first cell on screen in `scrolled`, with where its top is from the
/// top of the visible area: zero or less when it starts above.
pub fn top_cell(scrolled: &gtk::ScrolledWindow) -> Option<(BookCell, f32)> {
    cells(scrolled.upcast_ref())
        .into_iter()
        .filter(|cell| cell.is_mapped() && cell.book().is_some())
        .filter_map(|cell| {
            let bounds = cell.compute_bounds(scrolled)?;
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

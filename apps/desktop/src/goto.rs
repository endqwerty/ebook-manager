//! Keyboard "Go to" quick jump dialog (#131): searches a capped, indexed set
//! of books, authors and series off the GTK thread. Return reveals the book or
//! navigates to the author/series; Shift+Return opens a book in the reader.

use crate::store::Store;
use ebook_library::{Cancel, QuickJump};
use gtk::{gdk, glib};
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub enum Destination {
    Author(String),
    Series(String),
    Book { id: String, open_reader: bool },
}

enum EntryKind {
    Author(String),
    Series(String),
    Book(String),
}

pub fn open(parent: &impl IsA<gtk::Widget>, store: Store, on_pick: impl Fn(Destination) + 'static) {
    let dialog = adw::Dialog::builder()
        .title("Go to")
        .content_width(460)
        .content_height(380)
        .build();

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Book, author or series")
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label("Go to")]);
    outer.append(&entry);

    let status = gtk::Label::new(Some("Type a title, author or series name."));
    status.add_css_class("caption");
    status.add_css_class("dimmed");
    status.set_xalign(0.0);
    status.set_margin_start(14);
    status.set_margin_end(14);
    status.set_margin_bottom(6);
    outer.append(&status);

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();
    list.add_css_class("navigation-sidebar");
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();
    outer.append(&scrolled);

    let footer = gtk::Label::new(Some("Return: show in library · Shift+Return: open reader"));
    footer.add_css_class("caption");
    footer.add_css_class("dimmed");
    footer.set_xalign(0.0);
    footer.set_margin_top(6);
    footer.set_margin_bottom(10);
    footer.set_margin_start(14);
    footer.set_margin_end(14);
    outer.append(&footer);

    dialog.set_child(Some(&outer));

    let items: Rc<RefCell<Vec<Option<EntryKind>>>> = Rc::new(RefCell::new(Vec::new()));
    let on_pick = Rc::new(on_pick);
    let cancel_slot: Rc<RefCell<Cancel>> = Rc::new(RefCell::new(Cancel::default()));
    let serial = Rc::new(Cell::new(0u64));
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    let activate = {
        let (items, on_pick, dialog) = (items.clone(), on_pick.clone(), dialog.downgrade());
        Rc::new(move |index: usize, open_reader: bool| {
            let picked = items.borrow().get(index).and_then(|k| match k {
                Some(EntryKind::Author(a)) => Some(Destination::Author(a.clone())),
                Some(EntryKind::Series(s)) => Some(Destination::Series(s.clone())),
                Some(EntryKind::Book(id)) => Some(Destination::Book {
                    id: id.clone(),
                    open_reader,
                }),
                None => None,
            });
            if let Some(dest) = picked {
                if let Some(d) = dialog.upgrade() {
                    d.close();
                }
                on_pick(dest);
            }
        })
    };

    {
        let activate = activate.clone();
        list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx >= 0 {
                activate(idx as usize, false);
            }
        });
    }

    let populate = {
        let (list, items, status) = (list.clone(), items.clone(), status.clone());
        Rc::new(move |query_empty: bool, found: QuickJump| {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            items.borrow_mut().clear();
            if query_empty {
                status.set_text("Type a title, author or series name.");
                return;
            }
            let total = found.authors.len() + found.series.len() + found.books.len();
            if total == 0 {
                status.set_text("No books, authors or series match.");
                return;
            }
            status.set_text(&format!(
                "{total} {}",
                if total == 1 { "match" } else { "matches" }
            ));

            let add_header = |title: &str| {
                let lbl = gtk::Label::new(Some(title));
                lbl.add_css_class("caption");
                lbl.add_css_class("dimmed");
                lbl.set_xalign(0.0);
                lbl.set_margin_top(6);
                lbl.set_margin_start(6);
                let row = gtk::ListBoxRow::builder()
                    .selectable(false)
                    .activatable(false)
                    .child(&lbl)
                    .build();
                list.append(&row);
                items.borrow_mut().push(None);
            };
            let add_row = |primary: &str, secondary: &str, accessible: &str, kind: EntryKind| {
                let h = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                h.set_margin_top(4);
                h.set_margin_bottom(4);
                h.set_margin_start(6);
                h.set_margin_end(6);
                let left = gtk::Label::new(Some(primary));
                left.set_xalign(0.0);
                left.set_hexpand(true);
                left.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let right = gtk::Label::new(Some(secondary));
                right.add_css_class("caption");
                right.add_css_class("dimmed");
                right.add_css_class("numeric");
                right.set_ellipsize(gtk::pango::EllipsizeMode::End);
                h.append(&left);
                h.append(&right);
                let row = gtk::ListBoxRow::builder().child(&h).build();
                row.update_property(&[gtk::accessible::Property::Label(accessible)]);
                list.append(&row);
                items.borrow_mut().push(Some(kind));
            };

            if !found.authors.is_empty() {
                add_header("Authors");
                for (author, count) in found.authors {
                    let c = format!("{count} {}", if count == 1 { "book" } else { "books" });
                    let a11y = format!("Author, {author}, {c}");
                    add_row(&author, &c, &a11y, EntryKind::Author(author.clone()));
                }
            }
            if !found.series.is_empty() {
                add_header("Series");
                for (series, count) in found.series {
                    let c = format!("{count} {}", if count == 1 { "book" } else { "books" });
                    let a11y = format!("Series, {series}, {c}");
                    add_row(&series, &c, &a11y, EntryKind::Series(series.clone()));
                }
            }
            if !found.books.is_empty() {
                add_header("Books");
                for (id, title, authors) in found.books {
                    let a11y = if authors.is_empty() {
                        format!("Book, {title}")
                    } else {
                        format!("Book, {title}, {authors}")
                    };
                    add_row(&title, &authors, &a11y, EntryKind::Book(id));
                }
            }
            // Select the first selectable row so Return works immediately.
            let mut idx = 0;
            while let Some(row) = list.row_at_index(idx) {
                if row.is_selectable() {
                    list.select_row(Some(&row));
                    break;
                }
                idx += 1;
            }
        })
    };

    // Debounced off-thread quick jump query.
    {
        let (store, cancel_slot, serial, debounce, populate) = (
            store.clone(),
            cancel_slot.clone(),
            serial.clone(),
            debounce.clone(),
            populate.clone(),
        );
        entry.connect_search_changed(move |e| {
            let text = e.text().to_string();
            if let Some(id) = debounce.borrow_mut().take() {
                id.remove();
            }
            cancel_slot.borrow().cancel();
            let next_serial = serial.get().wrapping_add(1);
            serial.set(next_serial);
            if text.trim().is_empty() {
                populate(true, QuickJump::default());
                return;
            }
            let (store, cancel_slot, serial, debounce_slot, populate) = (
                store.clone(),
                cancel_slot.clone(),
                serial.clone(),
                debounce.clone(),
                populate.clone(),
            );
            let timer =
                glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
                    debounce_slot.borrow_mut().take();
                    if serial.get() != next_serial {
                        return;
                    }
                    let cancel = Cancel::default();
                    cancel_slot.replace(cancel.clone());
                    let query = text.clone();
                    let store = store.clone();
                    let serial = serial.clone();
                    let populate = populate.clone();
                    glib::spawn_future_local(async move {
                        let Ok(mut reader) = store.run(|lib| lib.browse_reader()).await else {
                            return;
                        };
                        let res =
                            gtk::gio::spawn_blocking(move || reader.quick_jump(&query, 8, &cancel))
                                .await;
                        if serial.get() != next_serial {
                            return;
                        }
                        if let Ok(Ok(found)) = res {
                            populate(false, found);
                        }
                    });
                });
            debounce.replace(Some(timer));
        });
    }

    // Keyboard navigation from the search entry: Up/Down move selection,
    // Return activates, Shift+Return opens reader directly.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let (list, activate) = (list.clone(), activate.clone());
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let step = |dir: i32| {
                let mut cur = list.selected_row().map_or(-1, |r| r.index());
                loop {
                    cur += dir;
                    let Some(row) = list.row_at_index(cur) else {
                        break;
                    };
                    if row.is_selectable() {
                        list.select_row(Some(&row));
                        break;
                    }
                }
            };
            match key {
                gdk::Key::Down | gdk::Key::KP_Down => {
                    step(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Up | gdk::Key::KP_Up => {
                    step(-1);
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                    if let Some(row) = list.selected_row() {
                        let idx = row.index();
                        if idx >= 0 {
                            let open_reader = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
                            activate(idx as usize, open_reader);
                            return glib::Propagation::Stop;
                        }
                    }
                    glib::Propagation::Proceed
                }
                _ => glib::Propagation::Proceed,
            }
        });
    }
    outer.add_controller(keys);

    dialog.present(Some(parent));
    entry.grab_focus();
}

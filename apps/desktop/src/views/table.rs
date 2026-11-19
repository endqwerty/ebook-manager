//! The table: a `GtkColumnView` with a column for each fact. Its headers
//! change the model's sort, and the table never sorts rows itself.
use super::{BookCell, Shared, column_factory, fact, finished, finished_mark, truncating};
use crate::{
    browse::BookListModel,
    cover::{Cover, Kind},
    settings::{Column, Sort, SortField},
    views::grid::EDGE,
};
use gtk::{gio, glib, prelude::*};
use gtk4 as gtk;
use std::{cell::Cell, rc::Rc};

/// The small cover in the title column. A row is this cover and 8 more.
const COVER: i32 = 20;

/// What the reader does to the table that the window has to know of.
type HeaderMenu = dyn Fn(&gtk::Widget, &gio::Menu);

pub struct Events {
    pub menu: Box<HeaderMenu>,
    /// A header was activated: its field, and whether Control was held,
    /// which adds the field as the next sort key.
    pub sort: Box<dyn Fn(SortField, bool)>,
    /// The columns' order or widths changed.
    pub layout: Box<dyn Fn(Vec<Column>)>,
}

fn cell(shared: &Shared, field: SortField) -> BookCell {
    let label = gtk::Label::builder().xalign(0.0).build();
    truncating(&label);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    match field {
        SortField::Title => {
            let cover = Cover::new(Kind::Table, COVER, &shared.thumbs);
            cover.set_margin_top(4);
            cover.set_margin_bottom(4);
            label.add_css_class("heading");
            content.append(&cover);
            content.append(&label);
            BookCell::new(&content, shared, move |_, book| {
                cover.set_book(book);
                label.set_text(book.map_or("", |book| &book.title));
            })
        }
        SortField::Progress => {
            // A number column keeps to the trailing edge.
            label.add_css_class("dimmed");
            label.add_css_class("numeric");
            label.set_xalign(1.0);
            label.set_hexpand(true);
            let done = finished_mark();
            content.append(&label);
            content.append(&done);
            BookCell::new(&content, shared, move |_, book| {
                label.set_text(&book.map(|book| fact(book, field)).unwrap_or_default());
                done.set_visible(book.is_some_and(|book| finished(book.progress)));
            })
        }
        _ => {
            label.add_css_class("dimmed");
            // Dates are text and keep to the leading edge, in tabular figures.
            if matches!(
                field,
                SortField::Added | SortField::Published | SortField::LastRead
            ) {
                label.add_css_class("numeric");
            }
            content.append(&label);
            BookCell::new(&content, shared, move |_, book| {
                label.set_text(&book.map(|book| fact(book, field)).unwrap_or_default());
            })
        }
    }
}

/// A header's menu: which columns are shown, and moving this one. It is
/// how the keyboard reorders columns, which a pointer does by dragging.
fn header_menu(field: SortField) -> gio::Menu {
    let menu = gio::Menu::new();
    let shown = gio::Menu::new();
    for (other, id, name) in SortField::ALL {
        // Title cannot be removed.
        if other != SortField::Title {
            shown.append(Some(name), Some(&format!("win.column-{id}")));
        }
    }
    menu.append_section(Some("Columns"), &shown);
    for commands in [
        [
            ("Move Toward the _Start", "win.column-start"),
            ("Move Toward the _End", "win.column-end"),
        ],
        [
            ("Make _Wider", "win.column-wider"),
            ("Make _Narrower", "win.column-narrower"),
        ],
    ] {
        let section = gio::Menu::new();
        for (label, action) in commands {
            let item = gio::MenuItem::new(Some(label), None);
            item.set_action_and_target_value(Some(action), Some(&field.id().to_variant()));
            section.append_item(&item);
        }
        menu.append_section(None, &section);
    }
    menu
}

fn field_of(column: &gtk::ColumnViewColumn) -> Option<SortField> {
    SortField::from_id(column.id()?.as_str())
}

fn columns_of(table: &gtk::ColumnView) -> Vec<gtk::ColumnViewColumn> {
    table
        .columns()
        .iter::<gtk::ColumnViewColumn>()
        .flatten()
        .collect()
}

fn header_field(table: &gtk::ColumnView, title: &gtk::Widget) -> Option<SortField> {
    // GTK keeps the header and column sibling orders in sync.
    let place = std::iter::successors(title.prev_sibling(), gtk::Widget::prev_sibling).count();
    columns_of(table).get(place).and_then(field_of)
}

/// The table and what it remembers of the sort it shows.
pub struct Table {
    pub view: gtk::ColumnView,
    state: Rc<State>,
}

#[derive(Default)]
struct State {
    /// `show_sort` is setting the table's own sorter, which is not the
    /// reader activating a header.
    showing: Cell<bool>,
    /// The first sort key shown.
    first: Cell<Option<SortField>>,
}

pub fn build(
    model: &BookListModel,
    shared: &Shared,
    columns: &[Column],
    sort: &Sort,
    events: Events,
) -> Table {
    let view = gtk::ColumnView::new(Some(model.clone()));
    view.add_css_class("books");
    view.set_reorderable(true);
    view.set_enable_rubberband(true);
    // The title's small cover starts at the screen's leading edge. A cell
    // has 6 of its own before its content.
    view.set_margin_start(EDGE - 6);
    view.set_margin_end(EDGE - 6);
    view.update_property(&[gtk::accessible::Property::Label("Books")]);
    let events = Rc::new(events);
    for saved in columns {
        let field = saved.field;
        let shared = shared.clone();
        let column = gtk::ColumnViewColumn::new(
            Some(field.name()),
            Some(column_factory(move || cell(&shared, field))),
        );
        column.set_id(Some(field.id()));
        column.set_visible(saved.visible);
        column.set_resizable(true);
        column.set_expand(field == SortField::Title && saved.width.is_none());
        column.set_fixed_width(saved.width.unwrap_or(-1));
        // A sorter makes the header one that can be activated. It never
        // orders anything: the model's rows come sorted from the catalogue.
        column.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
        view.append_column(&column);
    }
    let table = Table {
        view: view.clone(),
        state: Rc::default(),
    };
    table.show_sort(sort);

    // The table's own sorter records which header was activated. The
    // window answers by calling `show_sort`, which puts it back to the
    // model's sort.
    if let Some(sorter) = view.sorter().and_downcast::<gtk::ColumnViewSorter>() {
        let weak = view.downgrade();
        let (state, events) = (table.state.clone(), events.clone());
        sorter.connect_changed(move |sorter, _| {
            let Some(view) = weak.upgrade() else { return };
            if state.showing.get() {
                return;
            }
            // Activating the sorted header a third time clears the
            // table's sorter, and then that header is the one meant.
            let activated = sorter
                .primary_sort_column()
                .as_ref()
                .and_then(field_of)
                .or(state.first.get());
            let control = view
                .display()
                .default_seat()
                .and_then(|seat| seat.keyboard())
                .is_some_and(|keyboard| {
                    keyboard
                        .modifier_state()
                        .contains(gtk::gdk::ModifierType::CONTROL_MASK)
                });
            // Answered once the sorter has finished telling of its change.
            if let Some(field) = activated {
                let events = events.clone();
                glib::idle_add_local_once(move || (events.sort)(field, control));
            }
        });
    }
    // GTK's column headers answer a pointer only, and Tab passes them by.
    // Here Up from the first row reaches them. On a header, Left and Right
    // move along the row, Return sorts by it, with Control adds it as the
    // next key, the Menu key or Shift+F10 opens its menu, and Down goes
    // back to the books.
    let up = gtk::EventControllerKey::new();
    up.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = view.downgrade();
    up.connect_key_pressed(move |_, key, _, _| {
        let Some(view) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let on_first_row = view
            .root()
            .and_then(|root| root.focus())
            .filter(|focus| focus.is_ancestor(&view))
            .is_some_and(|focus| {
                super::cells(&focus)
                    .first()
                    .and_then(|cell| cell.book())
                    .is_some_and(|book| book.rank() == 0)
            });
        let first = view
            .first_child()
            .and_then(|row| row.first_child())
            .filter(|_| key == gtk::gdk::Key::Up && on_first_row);
        match first {
            Some(title) if title.grab_focus() => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        }
    });
    view.add_controller(up);
    // The header row refuses the focus for all its headers.
    if let Some(row) = view.first_child() {
        row.set_can_focus(true);
        row.set_focusable(false);
    }
    let mut header = view.first_child().and_then(|row| row.first_child());
    while let Some(title) = header {
        title.set_focusable(true);
        let keys = gtk::EventControllerKey::new();
        let (weak, on_key) = (view.downgrade(), events.clone());
        keys.connect_key_pressed(move |keys, key, _, modifiers| {
            use gtk::gdk::{Key, ModifierType};
            let (Some(view), Some(title)) = (weak.upgrade(), keys.widget()) else {
                return glib::Propagation::Proceed;
            };
            let Some(field) = header_field(&view, &title) else {
                return glib::Propagation::Proceed;
            };
            match key {
                Key::Return | Key::KP_Enter | Key::space => {
                    (on_key.sort)(field, modifiers.contains(ModifierType::CONTROL_MASK));
                    glib::Propagation::Stop
                }
                Key::Left | Key::Right => {
                    // The next header shown on that side.
                    let step = |widget: &gtk::Widget| {
                        if (key == Key::Left) != (title.direction() == gtk::TextDirection::Rtl) {
                            widget.prev_sibling()
                        } else {
                            widget.next_sibling()
                        }
                    };
                    let mut other = step(&title);
                    while other.as_ref().is_some_and(|widget| !widget.is_visible()) {
                        other = other.as_ref().and_then(step);
                    }
                    if let Some(other) = other {
                        other.grab_focus();
                    }
                    glib::Propagation::Stop
                }
                Key::Down => {
                    if let Some(rows) = view.last_child() {
                        rows.grab_focus();
                    }
                    glib::Propagation::Stop
                }
                // Nothing in the table is above its headers.
                Key::Up => glib::Propagation::Stop,
                Key::Menu | Key::F10
                    if key == Key::Menu || modifiers.contains(ModifierType::SHIFT_MASK) =>
                {
                    (on_key.menu)(&title, &header_menu(field));
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        let secondary = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_SECONDARY)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let (weak, on_click) = (view.downgrade(), events.clone());
        secondary.connect_pressed(move |gesture, _, _, _| {
            if let (Some(view), Some(title)) = (weak.upgrade(), gesture.widget())
                && let Some(field) = header_field(&view, &title)
            {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                (on_click.menu)(&title, &header_menu(field));
            }
        });
        title.add_controller(secondary);
        title.add_controller(keys);
        header = title.next_sibling();
    }
    let layout = {
        let weak = view.downgrade();
        move || {
            if let Some(view) = weak.upgrade() {
                (events.layout)(
                    columns_of(&view)
                        .iter()
                        .filter_map(|column| {
                            Some(Column {
                                field: field_of(column)?,
                                visible: column.is_visible(),
                                width: Some(column.fixed_width()).filter(|width| *width > 0),
                            })
                        })
                        .collect(),
                );
            }
        }
    };
    let layout = Rc::new(layout);
    for column in columns_of(&view) {
        let resized = layout.clone();
        column.connect_fixed_width_notify(move |column| {
            if field_of(column) == Some(SortField::Title) {
                // The title fills spare space only until the reader sets a width.
                column.set_expand(column.fixed_width() < 0);
            }
            resized();
        });
        let changed = layout.clone();
        column.connect_visible_notify(move |_| changed());
    }
    view.columns().connect_items_changed(move |_, _, _, _| {
        // Moving a column emits separate removal and insertion signals.
        let layout = layout.clone();
        glib::idle_add_local_once(move || layout());
    });
    table
}

#[derive(Clone, Copy)]
pub enum Change {
    Toggle,
    Move(bool),
    Resize(i32),
}

impl Table {
    pub fn change(&self, field: SortField, change: Change) {
        let columns = columns_of(&self.view);
        let Some(at) = columns
            .iter()
            .position(|column| field_of(column) == Some(field))
        else {
            return;
        };
        let column = &columns[at];
        match change {
            Change::Toggle => column.set_visible(!column.is_visible()),
            Change::Move(toward_end) => {
                let shown = |n: &usize| columns[*n].is_visible();
                let other = if toward_end {
                    (at + 1..columns.len()).find(shown)
                } else {
                    (0..at).rev().find(shown)
                };
                if let Some(other) = other {
                    self.view.insert_column(other as u32, column);
                }
            }
            Change::Resize(delta) => {
                let header = self.view.first_child().and_then(|row| row.first_child());
                if let Some(bounds) = std::iter::successors(header, gtk::Widget::next_sibling)
                    .nth(at)
                    .and_then(|header| header.compute_bounds(&self.view))
                {
                    let width = bounds.width().round() as i32;
                    column.set_fixed_width(
                        (width + delta).clamp(Column::MIN_WIDTH, Column::MAX_WIDTH),
                    );
                }
            }
        }
    }

    /// Shows the model's sort on the headers: the arrow on the first key,
    /// and a small 2 or 3 on a second and third.
    pub fn show_sort(&self, sort: &Sort) {
        self.state.showing.set(true);
        self.state.first.set(Some(sort.first().field));
        // The table's sorter keeps the headers activated before as lesser
        // keys of its own, each with an arrow. Only the model's sort shows.
        self.view.sort_by_column(None, gtk::SortType::Ascending);
        for column in columns_of(&self.view) {
            let Some(field) = field_of(&column) else {
                continue;
            };
            let place = sort.keys().iter().position(|key| key.field == field);
            column.set_title(Some(&match place {
                Some(1) => format!("{} ²", field.name()),
                Some(2) => format!("{} ³", field.name()),
                _ => field.name().to_owned(),
            }));
            if place == Some(0) {
                let order = if sort.first().descending {
                    gtk::SortType::Descending
                } else {
                    gtk::SortType::Ascending
                };
                self.view.sort_by_column(Some(&column), order);
            }
        }
        self.state.showing.set(false);
    }
}

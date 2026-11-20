//! The viewport scrolls a full-height canvas. Only rows around the viewport
//! have children, and their book cells are rebound as the reader moves.
mod back;
mod layout;
mod spine;
use super::{BookCell, Shared};
use crate::browse::{BookListModel, BookObject};
use ebook_library::ShelfStrip;
use gtk::{gdk, glib, prelude::*};
use gtk4 as gtk;
use libadwaita::prelude::BinExt;
pub(crate) use spine::state_label;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

pub struct Shelf {
    pub widget: gtk::Viewport,
    state: Rc<State>,
    connections: Vec<(glib::Object, glib::SignalHandlerId)>,
}
struct State {
    canvas: gtk::Fixed,
    model: BookListModel,
    shared: Shared,
    scroll: gtk::Adjustment,
    width: Cell<i32>,
    height: i32,
    strip: RefCell<Option<Arc<ShelfStrip>>>,
    rows: RefCell<Vec<layout::Row>>,
    visible: RefCell<BTreeMap<usize, Board>>,
    anchor: Cell<u32>,
}

/// A row keeps its children attached when it moves to a new set of ranks.
struct Board {
    widget: gtk::Box,
    caption: gtk::Label,
    books: back::Back,
    shade: gtk::Box,
    cells: Vec<BookCell>,
}

impl Board {
    fn new(canvas: &gtk::Fixed) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let caption = gtk::Label::builder()
            .xalign(0.)
            .height_request(24)
            .css_classes(["caption", "dimmed"])
            .build();
        widget.append(&caption);
        let books: back::Back = glib::Object::new();
        books.set_spacing(1);
        books.add_css_class("shelf-back");
        widget.append(&books);
        let shade = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        shade.add_css_class("standing-shade");
        for (class, height) in [("shelf-top", 7), ("shelf-board", 12), ("shelf-shadow", 10)] {
            let face = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            face.add_css_class(class);
            face.set_height_request(height);
            if class == "shelf-top" {
                face.append(&shade);
            }
            widget.append(&face);
        }
        canvas.put(&widget, 0., 0.);
        Self {
            widget,
            caption,
            books,
            shade,
            cells: Vec::new(),
        }
    }

    fn bind(&mut self, s: &Rc<State>, strip: &ShelfStrip, row: &layout::Row, end: u32) {
        self.widget.set_width_request(s.width.get());
        self.caption.set_visible(row.caption.is_some());
        if let Some(group) = row.caption {
            self.caption.set_label(&strip.groups[group].1);
        }
        let count = (end - row.first) as usize;
        for old in self.cells.drain(count.min(self.cells.len())..) {
            old.unbind();
            self.books.remove(&old);
        }
        let mut occupied = 0;
        for (index, rank) in (row.first..end).enumerate() {
            if index == self.cells.len() {
                let cell = s.cell();
                self.books.append(&cell);
                self.cells.push(cell);
            }
            let cell = &self.cells[index];
            let width = i32::from(strip.thickness[rank as usize]);
            occupied += width + 1;
            cell.set_size_request(width, s.height + 14);
            if let Some(book) = s.model.item(rank).and_downcast::<BookObject>() {
                cell.bind(&book);
            }
            s.describe(cell);
        }
        self.shade.set_width_request(occupied - 1);
        s.canvas.move_(&self.widget, 0., row.top as f64);
    }
}
impl Shelf {
    pub fn new(model: &BookListModel, shared: &Shared, scrolled: &gtk::ScrolledWindow) -> Self {
        let scroll = scrolled.vadjustment();
        let widget = gtk::Viewport::new(None::<&gtk::Adjustment>, Some(&scroll));
        widget.set_scroll_to_focus(false);
        let style = libadwaita::StyleManager::default();
        let appearance = |w: &gtk::Viewport, style: &libadwaita::StyleManager| {
            for (class, enabled) in [
                ("shelf-dark", style.is_dark()),
                ("shelf-contrast", style.is_high_contrast()),
            ] {
                if enabled {
                    w.add_css_class(class);
                } else {
                    w.remove_css_class(class);
                }
            }
            w.queue_draw();
        };
        appearance(&widget, &style);
        let mut connections = Vec::new();
        for name in ["dark", "high-contrast"] {
            let handler = style.connect_notify_local(
                Some(name),
                glib::clone!(
                    #[weak]
                    widget,
                    move |style, _| appearance(&widget, style)
                ),
            );
            connections.push((style.clone().upcast(), handler));
        }

        let canvas = gtk::Fixed::new();
        canvas.set_accessible_role(gtk::AccessibleRole::Grid);
        canvas.update_property(&[gtk::accessible::Property::Label("Bookshelf")]);
        widget.set_child(Some(&canvas));
        let state = Rc::new(State {
            canvas,
            model: model.clone(),
            shared: shared.clone(),
            scroll,
            width: Cell::new(0),
            height: shared.cover_size.grid() * 3 / 2,
            rows: Default::default(),
            visible: Default::default(),
            strip: Default::default(),
            anchor: Default::default(),
        });
        // GTK adjustments report scrolling and changes to either page size.
        // Coalesce them with model replacements before the next paint.
        let request_layout = Rc::new({
            let widget = widget.downgrade();
            let state = Rc::downgrade(&state);
            let pending = Rc::new(Cell::new(false));
            move || {
                if let Some(widget) = widget.upgrade()
                    && !pending.replace(true)
                {
                    let state = state.clone();
                    let pending = pending.clone();
                    widget.add_tick_callback(move |widget, _| {
                        pending.set(false);
                        if let Some(s) = state.upgrade() {
                            s.update(widget.width(), widget.height());
                        }
                        glib::ControlFlow::Break
                    });
                }
            }
        });
        for adjustment in [scrolled.hadjustment(), scrolled.vadjustment()] {
            let layout = request_layout.clone();
            let handler = adjustment.connect_changed(move |_| layout());
            connections.push((adjustment.upcast(), handler));
        }
        let layout = request_layout.clone();
        let handler = state.scroll.connect_value_changed(move |_| layout());
        connections.push((state.scroll.clone().upcast(), handler));
        let layout = request_layout.clone();
        let model_state = Rc::downgrade(&state);
        let handler = model.connect_items_changed(move |_, _, _, _| {
            // A repeated token can reuse its strip, but every replacement
            // hands out new page objects, including for unloaded spines.
            if let Some(state) = model_state.upgrade() {
                state.strip.take();
            }
            layout();
        });
        connections.push((model.clone().upcast(), handler));
        request_layout();
        if std::env::var_os("EBOOK_SHELF_FRAME_TIMES").is_some() {
            let scroll = state.scroll.clone();
            let previous = Cell::new(None);
            widget.add_tick_callback(move |_, clock| {
                let y = scroll.value();
                let time = clock.frame_time();
                if let Some((last_y, last_time)) = previous.replace(Some((y, time)))
                    && last_y != y
                {
                    eprintln!("shelf-scroll-frame-us {}", time - last_time);
                }
                glib::ControlFlow::Continue
            });
        }
        let s = Rc::downgrade(&state);
        let selection_handler = model.connect_selection_changed(move |_, _, _| {
            if let Some(s) = s.upgrade() {
                for cell in super::cells(s.canvas.upcast_ref()) {
                    s.describe(&cell);
                }
            }
        });
        connections.push((model.clone().upcast(), selection_handler));
        Self {
            widget,
            state,
            connections,
        }
    }
    pub fn scroll_to(&self, rank: u32, flags: gtk::ListScrollFlags) {
        self.state.update(self.widget.width(), self.widget.height());
        if flags.contains(gtk::ListScrollFlags::SELECT) {
            self.state.choose(rank, gdk::ModifierType::empty());
        }
        self.state
            .reveal(rank, flags.contains(gtk::ListScrollFlags::FOCUS));
    }
}
impl Drop for Shelf {
    fn drop(&mut self) {
        for (obj, handler) in self.connections.drain(..) {
            obj.disconnect(handler);
        }
    }
}
impl State {
    fn stride(&self) -> u64 {
        self.height as u64 + 71
    }
    fn update(self: &Rc<Self>, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        let current = self.model.strip();
        let changed = match (&*self.strip.borrow(), &current) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        let repack = changed || self.width.get() != width;
        let mut focus = None;
        if repack {
            if !changed {
                focus = self
                    .canvas
                    .root()
                    .and_then(|r| r.focus())
                    .and_downcast::<BookCell>()
                    .and_then(|c| c.book())
                    .map(|b| b.rank());
            }
            let old_y = self.scroll.value().ceil() as u64;
            let old_rank = {
                let rows = self.rows.borrow();
                rows.get(rows.partition_point(|r| r.top < old_y))
                    .map(|r| r.first)
            };
            let rows = current
                .as_ref()
                .map(|s| {
                    layout::pack(
                        &s.thickness,
                        &s.groups,
                        width as u32,
                        self.height as u32,
                        24,
                    )
                })
                .unwrap_or_default();
            let total = rows.last().map_or(0, |r| {
                r.top + self.stride() + if r.caption.is_some() { 24 } else { 0 }
            });
            self.canvas
                .set_height_request(total.min(i32::MAX as u64) as i32);
            if !changed && let Some(rank) = old_rank {
                let index = rows.partition_point(|r| r.first <= rank).saturating_sub(1);
                if let Some(row) = rows.get(index) {
                    self.scroll.set_value(row.top as f64);
                }
            }
            self.rows.replace(rows);
            self.strip.replace(current);
            self.width.set(width);
        }
        self.fill(height, repack);
        if let Some(focus) = focus
            && let Some(cell) = super::cells(self.canvas.upcast_ref())
                .into_iter()
                .find(|c| c.book().is_some_and(|b| b.rank() == focus))
        {
            cell.grab_focus();
        }
    }
    fn fill(self: &Rc<Self>, height: i32, repack: bool) {
        let rows = self.rows.borrow();
        let y = self.scroll.value() as u64;
        let start = rows.partition_point(|r| r.top <= y).saturating_sub(2);
        let end = (rows.partition_point(|r| r.top < y + height as u64) + 1).min(rows.len());
        let mut visible = self.visible.borrow_mut();
        let mut spare: Vec<_> = visible
            .extract_if(.., |i, _| repack || !(start..end).contains(i))
            .map(|(_, b)| b)
            .collect();
        for board in &spare {
            for cell in &board.cells {
                (self.shared.preview)(cell, false);
                if cell.has_focus()
                    && let Some(root) = cell.root()
                {
                    root.set_focus(gtk::Widget::NONE);
                }
            }
        }
        if let Some(strip) = self.strip.borrow().as_ref() {
            for index in start..end {
                visible.entry(index).or_insert_with(|| {
                    let mut board = spare.pop().unwrap_or_else(|| Board::new(&self.canvas));
                    let next = rows
                        .get(index + 1)
                        .map_or(strip.thickness.len() as u32, |r| r.first);
                    board.bind(self, strip, &rows[index], next);
                    board
                });
            }
        }
        for board in spare {
            self.canvas.remove(&board.widget);
        }
    }
    fn describe(&self, cell: &BookCell) {
        let Some(book) = cell.book() else { return };
        let selected = self.model.is_selected(book.rank());
        cell.update_state(&[gtk::accessible::State::Selected(Some(selected))]);
        if let Some(spine) = cell.child().and_downcast::<spine::Spine>() {
            spine.select(selected);
        }
    }
    fn reveal(self: &Rc<Self>, rank: u32, focus: bool) {
        let rows = self.rows.borrow();
        let i = rows.partition_point(|r| r.first <= rank).saturating_sub(1);
        let Some(r) = rows.get(i) else { return };
        self.scroll
            .clamp_page(r.top as f64, (r.top + self.stride() - 28) as f64);
        drop(rows);
        self.fill(self.scroll.page_size() as i32, false);
        if focus
            && let Some(cell) = super::cells(self.canvas.upcast_ref())
                .into_iter()
                .find(|c| c.book().is_some_and(|b| b.rank() == rank))
        {
            cell.grab_focus();
        }
    }
    fn choose(&self, rank: u32, mods: gdk::ModifierType) {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        if mods.contains(gdk::ModifierType::SHIFT_MASK) {
            let a = self
                .anchor
                .get()
                .min(self.model.n_items().saturating_sub(1));
            self.model
                .select_range(a.min(rank), a.abs_diff(rank) + 1, !ctrl);
        } else {
            self.anchor.set(rank);
            if ctrl && self.model.is_selected(rank) {
                self.model.unselect_item(rank);
            } else {
                self.model.select_item(rank, !ctrl);
            }
        }
    }
    fn adjacent(&self, rank: u32, down: bool) -> u32 {
        let rows = self.rows.borrow();
        let strip = self.strip.borrow();
        let Some(strip) = strip.as_ref() else {
            return rank;
        };
        let i = rows.partition_point(|r| r.first <= rank).saturating_sub(1);
        let j = if down { i + 1 } else { i.saturating_sub(1) };
        let Some(row) = rows.get(j) else { return rank };
        let widths = &strip.thickness;
        let x = widths[rows[i].first as usize..rank as usize]
            .iter()
            .map(|&w| 2 * (i32::from(w) + 1))
            .sum::<i32>()
            + i32::from(widths[rank as usize]);
        let end = rows.get(j + 1).map_or(widths.len() as u32, |r| r.first);
        let mut left = 0;
        (row.first..end)
            .min_by_key(|&n| {
                let w = i32::from(widths[n as usize]);
                let distance = (left + w - x).abs();
                left += 2 * (w + 1);
                distance
            })
            .unwrap_or(rank)
    }
    fn cell(self: &Rc<Self>) -> BookCell {
        let spine = spine::Spine::new();
        let s = spine.clone();
        let cell = BookCell::new(&spine, &self.shared, move |_, book| s.show_book(book));
        cell.set_focusable(true);
        cell.add_css_class("shelf-cell");
        cell.set_overflow(gtk::Overflow::Visible);
        let focus = gtk::EventControllerFocus::new();
        let preview = self.shared.preview.clone();
        let c = cell.downgrade();
        focus.connect_enter(move |_| {
            if let Some(cell) = c.upgrade() {
                preview(&cell, true);
            }
        });
        let preview = self.shared.preview.clone();
        let c = cell.downgrade();
        focus.connect_leave(move |_| {
            if let Some(cell) = c.upgrade() {
                preview(&cell, false);
            }
        });
        cell.add_controller(focus);
        let preview = self.shared.preview.clone();
        cell.set_has_tooltip(true);
        cell.connect_query_tooltip(move |cell, _, _, _, _| {
            preview(cell, true);
            false
        });
        let motion = gtk::EventControllerMotion::new();
        let preview = self.shared.preview.clone();
        let c = cell.downgrade();
        motion.connect_leave(move |_| {
            if let Some(cell) = c.upgrade()
                && !cell.has_focus()
            {
                preview(&cell, false);
            }
        });
        cell.add_controller(motion);
        cell.set_accessible_role(gtk::AccessibleRole::GridCell);
        let click = gtk::GestureClick::builder().button(1).build();
        let weak = Rc::downgrade(self);
        let c = cell.downgrade();
        click.connect_pressed(move |g, n, _, _| {
            if let (Some(s), Some(cell)) = (weak.upgrade(), c.upgrade())
                && let Some(book) = cell.book()
            {
                cell.grab_focus();
                s.choose(book.rank(), g.current_event_state());
                if n == 2 {
                    let _ = WidgetExt::activate_action(&cell, "win.read", None);
                }
            }
        });
        cell.add_controller(click);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        let c = cell.downgrade();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let (Some(s), Some(cell)) = (weak.upgrade(), c.upgrade()) else {
                return glib::Propagation::Proceed;
            };
            let Some(book) = cell.book() else {
                return glib::Propagation::Proceed;
            };
            let rank = book.rank();
            let last = s.model.n_items().saturating_sub(1);
            let target = match key {
                gdk::Key::Right => rank.saturating_add(1).min(last),
                gdk::Key::Left => rank.saturating_sub(1),
                gdk::Key::Down => s.adjacent(rank, true),
                gdk::Key::Up => s.adjacent(rank, false),
                gdk::Key::Home => 0,
                gdk::Key::End => last,
                gdk::Key::space => rank,
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let _ = WidgetExt::activate_action(&cell, "win.read", None);
                    return glib::Propagation::Stop;
                }
                _ => return glib::Propagation::Proceed,
            };
            if !mods.contains(gdk::ModifierType::CONTROL_MASK)
                || mods.contains(gdk::ModifierType::SHIFT_MASK)
                || key == gdk::Key::space
            {
                s.choose(target, mods);
            }
            s.reveal(target, true);
            glib::Propagation::Stop
        });
        cell.add_controller(keys);
        cell
    }
}

//! The read thread and the one list model every view shares.
//!
//! The read thread owns a `BrowseReader`. The model asks it to open a view
//! (a search and a sort) and for pages of that view, and never waits: an
//! item is handed out at once and filled in when its page arrives. Books
//! are tracked across views by ID, because a rank means nothing once the
//! view is replaced. The selection is a set of book IDs for that reason,
//! and a new view is shown without finding the selected books in it.
mod core;
mod worker;

use self::core::{Core, Effect, Event, Phase};
use crate::store::Store;
use ebook_library::{BrowseRow, OpenView, SortTerm};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk4 as gtk;
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use worker::Request;

/// Rows come from the read thread in pages of this many.
pub const PAGE: u32 = 128;
const CACHED_PAGES: usize = 48;
/// A view is reopened for catalogue changes at most this often.
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// What the reader asked to see. An empty sort is the library's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub shelf: bool,
    pub text: String,
    pub sort: Vec<SortTerm>,
}

/// Rank at the viewport and an upper bound on the number of spines it can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShelfViewport {
    pub first: u32,
    pub capacity: u32,
}

/// Where the reader is in the view on screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anchors {
    /// The book commands act on: the focused one, else the selected one.
    pub current: Option<String>,
    /// The first book on screen.
    pub top: Option<String>,
    /// Where that book starts, from the top of the view.
    pub top_offset: f32,
    pub shelf: Option<ShelfViewport>,
    /// The shelf remembers its book while another widget or window has focus.
    pub shelf_focus: Option<String>,
    /// A book holds the keyboard focus.
    pub focused: bool,
}

/// Why the open needs to place the reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Command,
    Refresh,
    Layout,
}

/// Where the reader was, and where those books are in the view now shown.
pub struct Placed {
    pub anchors: Anchors,
    pub current: Option<u32>,
    pub top: Option<u32>,
    pub reason: Placement,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    /// No view has opened yet.
    #[default]
    Loading,
    Ready,
    /// No view can be read. The text says why.
    Failed(String),
}

/// How the window takes part in replacing a view.
pub struct Hooks {
    /// Where the reader is at this moment.
    pub anchors: Box<dyn Fn() -> Anchors>,
    /// Close menus anchored to the old books before GTK removes them.
    pub before_replace: Box<dyn Fn()>,
    /// The status or the selection changed, or, when true, the books shown
    /// were replaced.
    pub changed: Box<dyn Fn(bool)>,
    /// Put the reader back where they were.
    pub place: Box<dyn Fn(Placed)>,
}

mod object {
    use super::*;
    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::BookObject)]
    pub struct BookObject {
        #[property(get, construct_only)]
        pub rank: Cell<u32>,
        /// Becomes true once, when the book's row has arrived.
        #[property(get)]
        pub loaded: Cell<bool>,
        /// Becomes true once, when the view was replaced or lost before
        /// the row arrived. It never will.
        #[property(get)]
        pub gone: Cell<bool>,
        pub row: RefCell<Option<Rc<BrowseRow>>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for BookObject {
        const NAME: &'static str = "EbookManagerBook";
        type Type = super::BookObject;
    }
    #[glib::derived_properties]
    impl ObjectImpl for BookObject {}
}

glib::wrapper! {
    /// One place in the view. It knows its rank at once and its book when
    /// the page holding it has loaded.
    pub struct BookObject(ObjectSubclass<object::BookObject>);
}

impl BookObject {
    fn new(rank: u32, row: Option<Rc<BrowseRow>>) -> Self {
        let object: Self = glib::Object::builder().property("rank", rank).build();
        object.imp().loaded.set(row.is_some());
        object.imp().row.replace(row);
        object
    }
    pub fn row(&self) -> Option<Rc<BrowseRow>> {
        self.imp().row.borrow().clone()
    }
    fn fill(&self, row: Rc<BrowseRow>) {
        self.imp().row.replace(Some(row));
        self.imp().loaded.set(true);
        self.notify_loaded();
    }
    /// The view this place was in has gone. If its row had not arrived, a
    /// command waiting for it is told so.
    fn leave(&self) {
        if !self.loaded() && !self.imp().gone.replace(true) {
            self.notify_gone();
        }
    }
    /// The book's row, once its page has arrived. `None` when the view was
    /// replaced or lost first, and the place no longer stands for a book.
    pub async fn loaded_row(&self) -> Option<Rc<BrowseRow>> {
        if self.row().is_some() || self.gone() {
            return self.row();
        }
        let (tx, rx) = async_channel::bounded(2);
        let arrived = tx.clone();
        let loaded = self.connect_loaded_notify(move |_| {
            let _ = arrived.try_send(());
        });
        let gone = self.connect_gone_notify(move |_| {
            let _ = tx.try_send(());
        });
        let _ = rx.recv().await;
        self.disconnect(loaded);
        self.disconnect(gone);
        self.row()
    }
}

/// What the GObject keeps beside the core: the rows in memory, the places
/// handed out, and the selection.
#[derive(Default)]
struct State {
    strip: Option<std::sync::Arc<ebook_library::ShelfStrip>>,
    count: u32,
    pages: HashMap<u32, Vec<Rc<BrowseRow>>>,
    /// Loaded pages, oldest first.
    recent: VecDeque<u32>,
    live: HashMap<u32, glib::WeakRef<BookObject>>,
    live_prune_at: usize,
    /// The selected books, by ID.
    selected: HashSet<String>,
    /// Places selected before their rows arrived. Each becomes an ID in
    /// `selected` when its row does.
    pending: BTreeSet<u32>,
    last_refresh: Option<Instant>,
    refresh_waiting: bool,
}

fn ranks(bits: &gtk::Bitset) -> impl Iterator<Item = u32> {
    gtk::BitsetIter::init_first(bits)
        .into_iter()
        .flat_map(|(iter, first)| std::iter::once(first).chain(iter))
}

impl State {
    fn row(&self, rank: u32) -> Option<Rc<BrowseRow>> {
        self.pages
            .get(&(rank / PAGE))?
            .get((rank % PAGE) as usize)
            .cloned()
    }
    /// Keeps a page, and returns whether any of its books is selected.
    fn store_page(&mut self, page: u32, rows: Vec<BrowseRow>) -> bool {
        self.store_page_with_limit(page, rows, CACHED_PAGES)
    }

    fn store_page_with_limit(&mut self, page: u32, rows: Vec<BrowseRow>, limit: usize) -> bool {
        let mut any = false;
        for (n, row) in rows.iter().enumerate() {
            let rank = page * PAGE + n as u32;
            if self.pending.remove(&rank) {
                self.selected.insert(row.id.clone());
            }
            any |= self.selected.contains(&row.id);
        }
        self.pages
            .insert(page, rows.into_iter().map(Rc::new).collect());
        self.recent.push_back(page);
        while self.recent.len() > limit {
            if let Some(old) = self.recent.pop_front() {
                self.pages.remove(&old);
            }
        }
        any
    }
    /// Whether the book at a place is selected: by its ID once its row is
    /// in memory, and until then by the place.
    fn is_selected(&self, rank: u32) -> bool {
        match self.row(rank) {
            Some(row) => self.selected.contains(&row.id),
            None => self.pending.contains(&rank),
        }
    }
    /// The selected places among those whose rows are in memory, and the
    /// places selected before their rows came.
    fn selection(&self) -> Vec<u32> {
        let mut places: Vec<u32> = self.pending.iter().copied().collect();
        if !self.selected.is_empty() {
            for (page, rows) in &self.pages {
                for (n, row) in rows.iter().enumerate() {
                    if self.selected.contains(&row.id) {
                        places.push(page * PAGE + n as u32);
                    }
                }
            }
        }
        places
    }
    /// Selected places within `position..position + n_items`.
    fn selection_in_range(&self, position: u32, n_items: u32) -> Vec<u32> {
        if n_items == 0 {
            return Vec::new();
        }
        let end = position.saturating_add(n_items);
        let mut places: Vec<u32> = self.pending.range(position..end).copied().collect();
        if !self.selected.is_empty() {
            let first_page = position / PAGE;
            let last_page = (end - 1) / PAGE;
            for page in first_page..=last_page {
                if let Some(rows) = self.pages.get(&page) {
                    for (n, row) in rows.iter().enumerate() {
                        let rank = page * PAGE + n as u32;
                        if rank >= position && rank < end && self.selected.contains(&row.id) {
                            places.push(rank);
                        }
                    }
                }
            }
        }
        places
    }
    /// The places still waiting for a row.
    fn waiting(&self) -> Vec<BookObject> {
        self.live
            .values()
            .filter_map(|weak| weak.upgrade())
            .filter(|place| !place.loaded())
            .collect()
    }
    /// Empties the view, and returns how many items there were.
    fn clear(&mut self) -> u32 {
        self.strip = None;
        self.live.clear();
        self.pages.clear();
        self.recent.clear();
        self.pending.clear();
        std::mem::take(&mut self.count)
    }
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct BookListModel {
        pub(super) store: OnceCell<Store>,
        /// The way to the read thread, while there is one.
        pub(super) requests: RefCell<Option<mpsc::Sender<Request>>>,
        /// Counts read threads, so the end of an old one is not taken for
        /// the end of its replacement.
        pub(super) started: Cell<u64>,
        /// The protocol with the read thread. `None` until the first query.
        pub(super) core: RefCell<Option<Core>>,
        pub(super) state: RefCell<State>,
        pub(super) parked: RefCell<Option<BookObject>>,
        pub(super) hooks: RefCell<Option<Rc<Hooks>>>,
        pub(super) id_tag: Cell<u64>,
        pub(super) id_waiters: RefCell<HashMap<u64, async_channel::Sender<Option<Vec<String>>>>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for BookListModel {
        const NAME: &'static str = "EbookManagerBookListModel";
        type Type = super::BookListModel;
        type Interfaces = (gio::ListModel, gtk::SelectionModel, gtk::SectionModel);
    }
    impl ObjectImpl for BookListModel {}
    impl ListModelImpl for BookListModel {
        fn item_type(&self) -> glib::Type {
            BookObject::static_type()
        }
        fn n_items(&self) -> u32 {
            self.state.borrow().count
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            let (object, wanted) = {
                let mut state = self.state.borrow_mut();
                if position >= state.count {
                    return None;
                }
                if let Some(object) = state.live.get(&position).and_then(|weak| weak.upgrade()) {
                    return Some(object.upcast());
                }
                let row = state.row(position);
                let wanted = row.is_none();
                let object = BookObject::new(position, row);
                let prune_at = state.live_prune_at.max(4096);
                if state.live.len() >= prune_at {
                    state.live.retain(|_, weak| weak.upgrade().is_some());
                    state.live_prune_at = (state.live.len() * 2).max(4096);
                }
                state.live.insert(position, object.downgrade());
                (object, wanted)
            };
            // The core asks for the page, or says no row will come.
            if wanted {
                self.obj().run(Event::Want(position));
            }
            Some(object.upcast())
        }
    }
    impl BookListModel {
        /// Changes which places are selected and reports the span touched.
        fn select(&self, clear: bool, add: &gtk::Bitset, remove: &gtk::Bitset) -> bool {
            let span = {
                let mut state = self.state.borrow_mut();
                let before = bits(&state.selection());
                if clear {
                    state.selected.clear();
                    state.pending.clear();
                }
                for rank in ranks(remove) {
                    match state.row(rank) {
                        Some(row) => {
                            state.selected.remove(&row.id);
                        }
                        None => {
                            state.pending.remove(&rank);
                        }
                    }
                }
                let count = state.count;
                for rank in ranks(add).filter(|rank| *rank < count) {
                    match state.row(rank) {
                        Some(row) => {
                            state.selected.insert(row.id.clone());
                        }
                        None => {
                            state.pending.insert(rank);
                        }
                    }
                }
                before.difference(&bits(&state.selection()));
                (!before.is_empty()).then(|| (before.minimum(), before.maximum()))
            };
            if let Some((first, last)) = span {
                self.obj().selection_changed(first, last - first + 1);
            }
            true
        }
    }
    fn bits(places: &[u32]) -> gtk::Bitset {
        let bits = gtk::Bitset::new_empty();
        for place in places {
            bits.add(*place);
        }
        bits
    }
    fn range(position: u32, n_items: u32) -> gtk::Bitset {
        let bits = gtk::Bitset::new_empty();
        bits.add_range(position, n_items);
        bits
    }
    impl SelectionModelImpl for BookListModel {
        fn is_selected(&self, position: u32) -> bool {
            self.state.borrow().is_selected(position)
        }
        fn selection_in_range(&self, position: u32, n_items: u32) -> gtk::Bitset {
            bits(&self.state.borrow().selection_in_range(position, n_items))
        }
        fn select_item(&self, position: u32, unselect_rest: bool) -> bool {
            self.select(
                unselect_rest,
                &range(position, 1),
                &gtk::Bitset::new_empty(),
            )
        }
        fn select_range(&self, position: u32, n_items: u32, unselect_rest: bool) -> bool {
            self.select(
                unselect_rest,
                &range(position, n_items),
                &gtk::Bitset::new_empty(),
            )
        }
        fn select_all(&self) -> bool {
            let count = self.state.borrow().count;
            self.select(false, &range(0, count), &gtk::Bitset::new_empty())
        }
        fn unselect_all(&self) -> bool {
            self.select(true, &gtk::Bitset::new_empty(), &gtk::Bitset::new_empty())
        }
        fn unselect_item(&self, position: u32) -> bool {
            self.select(false, &gtk::Bitset::new_empty(), &range(position, 1))
        }
        fn unselect_range(&self, position: u32, n_items: u32) -> bool {
            self.select(false, &gtk::Bitset::new_empty(), &range(position, n_items))
        }
        fn set_selection(&self, selected: &gtk::Bitset, mask: &gtk::Bitset) -> bool {
            let add = selected.copy();
            add.intersect(mask);
            let remove = mask.copy();
            remove.subtract(selected);
            self.select(false, &add, &remove)
        }
    }
    // One section: the whole view.
    impl SectionModelImpl for BookListModel {}
}

glib::wrapper! {
    pub struct BookListModel(ObjectSubclass<imp::BookListModel>)
        @implements gio::ListModel, gtk::SelectionModel, gtk::SectionModel;
}

/// The books a command was invoked on: those whose IDs were known then,
/// and any unloaded ranks resolved off the GTK thread by the read thread.
pub struct Targets {
    ids: Vec<String>,
    pending: Option<async_channel::Receiver<Option<Vec<String>>>>,
    failed: bool,
}

impl Targets {
    /// The unique IDs, once every selected rank has been resolved. `None` when
    /// the view went first or the read thread stopped.
    pub async fn ids(mut self) -> Option<Vec<String>> {
        if self.failed {
            return None;
        }
        if let Some(rx) = self.pending {
            let resolved = rx.recv().await.ok()??;
            self.ids.extend(resolved);
        }
        let mut seen = HashSet::with_capacity(self.ids.len());
        self.ids.retain(|id| seen.insert(id.clone()));
        Some(self.ids)
    }
}

impl BookListModel {
    pub fn new(store: Store) -> Self {
        let model: Self = glib::Object::new();
        let _ = model.imp().store.set(store);
        model.start();
        model
    }

    /// Starts a read thread and listens for its replies.
    fn start(&self) {
        let Some(store) = self.imp().store.get().cloned() else {
            return;
        };
        let started = self.imp().started.get() + 1;
        self.imp().started.set(started);
        let (requests, received) = mpsc::channel();
        let (send, replies) = async_channel::unbounded();
        std::thread::spawn(move || worker::serve(store, received, send));
        self.imp().requests.replace(Some(requests));
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(reply) = replies.recv().await {
                let Some(model) = weak.upgrade() else { return };
                // A reply from a thread that has been replaced means
                // nothing to the one that replaced it.
                if model.imp().started.get() == started {
                    if let worker::Reply::Ids { tag, ids } = reply {
                        if let Some(tx) = model.imp().id_waiters.borrow_mut().remove(&tag) {
                            let _ = tx.try_send(ids);
                        }
                    } else {
                        model.run(Event::Reply(reply));
                    }
                }
            }
            // The thread ended, which it does not do while the model
            // lives. Try Again starts another.
            if let Some(model) = weak.upgrade()
                && model.imp().started.get() == started
            {
                model.imp().requests.replace(None);
                model.imp().id_waiters.borrow_mut().clear();
                model.run(Event::Stopped);
            }
        });
    }

    pub fn set_hooks(&self, hooks: Hooks) {
        self.imp().hooks.replace(Some(Rc::new(hooks)));
    }
    fn hooks(&self) -> Option<Rc<Hooks>> {
        self.imp().hooks.borrow().clone()
    }
    fn changed(&self, replaced: bool) {
        if let Some(hooks) = self.hooks() {
            (hooks.changed)(replaced);
        }
    }
    fn anchors(&self) -> Anchors {
        self.hooks()
            .map(|hooks| (hooks.anchors)())
            .unwrap_or_default()
    }

    pub fn status(&self) -> Status {
        match self.imp().core.borrow().as_ref().map(|core| &core.phase) {
            None | Some(Phase::Loading { .. }) => Status::Loading,
            Some(Phase::Showing { .. } | Phase::Lost { .. }) => Status::Ready,
            Some(Phase::Error { message }) => Status::Failed(message.clone()),
        }
    }
    pub fn query(&self) -> Query {
        self.imp()
            .core
            .borrow()
            .as_ref()
            .map(|core| core.query.clone())
            .unwrap_or_default()
    }

    /// Gives the core an event and carries out what it asks for.
    fn run(&self, event: Event) {
        let effects = match self.imp().core.borrow_mut().as_mut() {
            Some(core) => core.step(event, &|| self.anchors()),
            None => return,
        };
        self.perform(effects);
    }

    fn perform(&self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send(request) => {
                    let sent = self
                        .imp()
                        .requests
                        .borrow()
                        .as_ref()
                        .is_some_and(|requests| requests.send(request).is_ok());
                    // The thread has gone. The model says so once whoever
                    // is being answered has returned.
                    if !sent {
                        let weak = self.downgrade();
                        glib::idle_add_local_once(move || {
                            if let Some(model) = weak.upgrade() {
                                model.run(Event::Stopped);
                            }
                        });
                    }
                }
                Effect::Gone => {
                    let waiting = self.imp().state.borrow().waiting();
                    for place in waiting {
                        place.leave();
                    }
                }
                Effect::Swap {
                    view,
                    strip,
                    background,
                    pages,
                    shelf_focus,
                } => self.swap(view, strip, background, pages, shelf_focus),
                Effect::Fill { page, rows } => {
                    let (arrived, selected) = {
                        let mut state = self.imp().state.borrow_mut();
                        let selected = state.store_page(page, rows);
                        let arrived: Vec<_> = (page * PAGE..(page + 1) * PAGE)
                            .filter_map(|rank| {
                                Some((state.live.get(&rank)?.upgrade()?, state.row(rank)?))
                            })
                            .collect();
                        (arrived, selected)
                    };
                    for (object, row) in arrived {
                        object.fill(row);
                    }
                    // Books selected in an earlier view show as selected
                    // now that their rows say which they are.
                    if selected {
                        let first = page * PAGE;
                        self.selection_changed(first, PAGE.min(self.n_items() - first));
                    }
                }
                Effect::Place(placed) => {
                    if let Some(hooks) = self.hooks() {
                        (hooks.place)(placed);
                    }
                }
                Effect::Absent(ids) => {
                    {
                        let mut state = self.imp().state.borrow_mut();
                        for id in &ids {
                            state.selected.remove(id);
                        }
                    }
                    // None of them was on screen, so no row changes its
                    // look. The commands that count the selection are told.
                    self.changed(false);
                }
                Effect::Empty => {
                    self.park_focus(None);
                    self.before_replace();
                    // With an error shown there are no books, so there is
                    // no selection either: a command must not reach a book
                    // the reader can no longer see.
                    let before = {
                        let mut state = self.imp().state.borrow_mut();
                        state.selected.clear();
                        state.clear()
                    };
                    if before > 0 {
                        self.items_changed(0, before, 0);
                    }
                    self.changed(true);
                }
            }
        }
    }

    /// Geometry for the view now on screen, when opened for a shelf.
    pub fn strip(&self) -> Option<std::sync::Arc<ebook_library::ShelfStrip>> {
        self.imp().state.borrow().strip.clone()
    }

    /// Shows the books of a view in place of whatever is shown. The
    /// selection is by ID and needs nothing found: each row says whether
    /// its book is selected as it arrives.
    fn swap(
        &self,
        view: OpenView,
        strip: Option<std::sync::Arc<ebook_library::ShelfStrip>>,
        background: bool,
        pages: worker::Pages,
        shelf_focus: Option<u32>,
    ) {
        self.before_replace();
        let before = {
            let mut state = self.imp().state.borrow_mut();
            // The interval runs from when a refresh is shown.
            if background {
                if strip.is_some() && std::env::var_os("EBOOK_SHELF_REFRESH_TRACE").is_some() {
                    eprintln!("shelf-refresh");
                }
                state.last_refresh = Some(Instant::now());
            }
            state.clear()
        };
        // Two signals, not one. Given a removal and an addition together,
        // a GTK list asks for every added item to see whether a row it is
        // showing can stay: 100,000 calls. Between the two the model has
        // no items, and so nothing selected.
        self.items_changed(0, before, 0);
        let selected: Vec<String> = {
            let mut state = self.imp().state.borrow_mut();
            state.count = view.count;
            state.strip = strip;
            // Even a viewport larger than the normal cache must bind its
            // first boards from the pages delivered with the swap.
            let limit = CACHED_PAGES.max(1 + pages.len());
            state.store_page_with_limit(0, view.first, limit);
            for (page, rows) in pages {
                state.store_page_with_limit(page, rows, limit);
            }
            state.selected.iter().cloned().collect()
        };
        self.park_focus(shelf_focus.and_then(|rank| self.item(rank)?.downcast().ok()));
        self.items_changed(0, 0, view.count);
        self.changed(true);
        self.run(Event::Swapped);
        // A selected book the view does not hold leaves the selection.
        // Asking which those are can only remove what is not there, so its
        // answer is safe whenever it comes.
        if !selected.is_empty() {
            self.run(Event::Prune(selected));
        }
    }

    fn before_replace(&self) {
        if let Some(hooks) = self.hooks() {
            (hooks.before_replace)();
        }
    }

    /// Shows another search or sort. The open in flight, if any, is dropped.
    pub fn set_query(&self, query: Query) {
        if self.imp().core.borrow().is_some() {
            self.run(Event::Query(query));
            return;
        }
        let (core, effects) = Core::new(query);
        self.imp().core.replace(Some(core));
        self.perform(effects);
    }

    /// Tries again to open the library after it could not be read, with a
    /// new read thread if the last one stopped.
    pub fn retry(&self) {
        if self.imp().requests.borrow().is_none() {
            self.start();
        }
        self.run(Event::Retry);
    }

    /// Opens the query for the new layout, even if its sort is unchanged.
    pub fn change_layout(&self, query: Query) {
        self.run(Event::Layout(query));
    }

    /// The write thread finished a job, which may have changed a book. The
    /// view is refreshed for it at most once a second.
    pub fn catalogue_changed(&self) {
        let wait = {
            let mut state = self.imp().state.borrow_mut();
            if state.refresh_waiting {
                return;
            }
            let wait = state
                .last_refresh
                .and_then(|last| REFRESH_INTERVAL.checked_sub(last.elapsed()));
            state.refresh_waiting = wait.is_some();
            if wait.is_none() {
                state.last_refresh = Some(Instant::now());
            }
            wait
        };
        match wait {
            None => self.run(Event::Refresh),
            Some(wait) => {
                let weak = self.downgrade();
                glib::timeout_add_local_once(wait, move || {
                    if let Some(model) = weak.upgrade() {
                        model.imp().state.borrow_mut().refresh_waiting = false;
                        model.catalogue_changed();
                    }
                });
            }
        }
    }

    /// How many books are selected.
    pub fn selected_count(&self) -> u64 {
        let state = self.imp().state.borrow();
        (state.selected.len() + state.pending.len()) as u64
    }

    /// Retain the book (including a pending page read) when its cell is reused.
    pub fn park_focus(&self, book: Option<BookObject>) {
        self.imp().parked.replace(book);
    }

    pub fn parked_focus(&self) -> Option<BookObject> {
        self.imp().parked.borrow().clone()
    }

    /// The selected book, when there is one and only one.
    pub fn selected_one(&self) -> Option<String> {
        let state = self.imp().state.borrow();
        (state.selected.len() == 1 && state.pending.is_empty())
            .then(|| state.selected.iter().next().cloned())
            .flatten()
    }

    /// The books selected at this moment, for a command to act on whatever
    /// is selected by the time their IDs have arrived.
    pub fn targets(&self) -> Targets {
        let (ids, pending): (Vec<String>, Vec<u32>) = {
            let state = self.imp().state.borrow();
            (
                state.selected.iter().cloned().collect(),
                state.pending.iter().copied().collect(),
            )
        };
        if pending.is_empty() {
            return Targets {
                ids,
                pending: None,
                failed: false,
            };
        }
        let token = match self.imp().core.borrow().as_ref().map(|c| &c.phase) {
            Some(Phase::Showing { view, .. }) => view.token,
            _ => {
                return Targets {
                    ids,
                    pending: None,
                    failed: true,
                };
            }
        };
        let (tx, rx) = async_channel::bounded(1);
        let tag = self.imp().id_tag.get() + 1;
        self.imp().id_tag.set(tag);
        let sent = self
            .imp()
            .requests
            .borrow()
            .as_ref()
            .is_some_and(|requests| {
                requests
                    .send(Request::Ids {
                        tag,
                        token,
                        ranks: pending,
                    })
                    .is_ok()
            });
        if !sent {
            return Targets {
                ids,
                pending: None,
                failed: true,
            };
        }
        self.imp().id_waiters.borrow_mut().insert(tag, tx);
        Targets {
            ids,
            pending: Some(rx),
            failed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> BrowseRow {
        BrowseRow {
            id: id.into(),
            version_id: String::new(),
            title: id.into(),
            authors: String::new(),
            author_sort: None,
            series: None,
            added: None,
            published: None,
            last_read: None,
            progress: None,
            cover: None,
            cover_rgb: None,
            spine_rgb: None,
            right_to_left: false,
            protection: None,
        }
    }

    fn state(count: u32) -> State {
        State {
            count,
            ..Default::default()
        }
    }

    #[test]
    fn a_command_keeps_the_books_it_was_invoked_on() {
        // What is selected later is not what the command acts on: the
        // targets are taken when it is invoked.
        let targets = Targets {
            ids: vec!["first".into()],
            pending: None,
            failed: false,
        };
        let ids = glib::MainContext::new().block_on(targets.ids());
        assert_eq!(ids, Some(vec!["first".to_owned()]));
    }

    #[test]
    fn a_selected_book_is_selected_wherever_the_next_view_puts_it() {
        let mut state = state(300);
        state.store_page(0, vec![row("a"), row("b")]);
        state.selected.insert("b".into());
        assert!(state.is_selected(1) && !state.is_selected(0));
        // Another view: nothing is found or carried over by rank. The
        // book says it is selected when its row arrives at its new place.
        state.clear();
        state.count = 300;
        assert!(!state.is_selected(1));
        assert!(state.store_page(1, vec![row("x"), row("b")]));
        assert!(state.is_selected(PAGE + 1));
        assert_eq!(state.selection(), [PAGE + 1]);
    }

    #[test]
    fn a_selected_book_stays_selected_when_its_page_leaves_memory() {
        let mut state = state(100_000);
        state.store_page(0, vec![row("a")]);
        state.selected.insert("a".into());
        for page in 1..=CACHED_PAGES as u32 {
            state.store_page(page, vec![row(&format!("p{page}"))]);
        }
        assert!(state.row(0).is_none(), "the first page was dropped");
        assert!(state.selected.contains("a"));
        // When the page is read again the book is selected as before.
        assert!(state.store_page(0, vec![row("a")]));
        assert!(state.is_selected(0));
    }

    #[test]
    fn a_place_selected_before_its_row_arrives_becomes_that_book() {
        let mut state = state(1000);
        state.pending.insert(700);
        assert!(state.is_selected(700));
        let page = 700 / PAGE;
        let rows = (0..PAGE).map(|n| row(&format!("r{n}"))).collect();
        assert!(state.store_page(page, rows));
        assert!(state.pending.is_empty());
        assert!(state.selected.contains(&format!("r{}", 700 % PAGE)));
        assert!(state.is_selected(700));
    }

    #[test]
    fn selection_in_range_only_returns_ranks_inside_the_requested_window() {
        let mut state = state(100_000);
        state.store_page(0, vec![row("a"), row("b")]);
        state.selected.insert("a".into());
        for rank in 2..100_000 {
            state.pending.insert(rank);
        }
        let window = state.selection_in_range(500, 10);
        assert_eq!(window, (500..510).collect::<Vec<_>>());
        assert!(state.selection_in_range(500, 0).is_empty());
        let first = state.selection_in_range(0, 4);
        assert_eq!(first, vec![2, 3, 0]);
    }

    #[test]
    fn targets_deduplicates_ids_present_in_both_selected_and_pending() {
        let (tx, rx) = async_channel::bounded(1);
        tx.try_send(Some(vec!["a".into(), "b".into(), "a".into()]))
            .unwrap();
        let targets = Targets {
            ids: vec!["a".into(), "c".into()],
            pending: Some(rx),
            failed: false,
        };
        let ids = glib::MainContext::new().block_on(targets.ids());
        assert_eq!(
            ids,
            Some(vec!["a".to_owned(), "c".to_owned(), "b".to_owned()])
        );
    }
}

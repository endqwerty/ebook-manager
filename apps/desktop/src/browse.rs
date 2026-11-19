//! The read thread and the one list model every view shares.
//!
//! The read thread owns a `BrowseReader`. The model asks it to open a view
//! (a search and a sort) and for pages of that view, and never waits: an
//! item is handed out at once and filled in when its page arrives. Books
//! are tracked across views by ID, because a rank means nothing once the
//! view is replaced.
mod worker;

use crate::store::Store;
use ebook_library::{BrowseRow, Cancel, OpenView, SortTerm, ViewToken};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk4 as gtk;
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use worker::{Reply, Request};

/// Rows come from the read thread in pages of this many.
pub const PAGE: u32 = 128;
const CACHED_PAGES: usize = 48;
/// A view is reopened for catalogue changes at most this often.
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// What the reader asked to see. An empty sort is the library's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub text: String,
    pub sort: Vec<SortTerm>,
}

/// The books the window wants found again in the next view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anchors {
    /// The book commands act on: the focused one, else the selected one.
    pub current: Option<String>,
    /// The first book on screen.
    pub top: Option<String>,
}

/// Where the anchors are in the view that has just replaced the last.
pub struct Replaced {
    pub current: Option<u32>,
    pub top: Option<u32>,
    /// The catalogue changed under the reader, who asked for nothing.
    pub background: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    /// No view has opened yet.
    #[default]
    Loading,
    Ready,
    /// No view could be opened. The text says why.
    Failed(String),
}

type OnReplaced = dyn Fn(Option<&Replaced>);

/// How the window takes part in replacing a view.
pub struct Hooks {
    /// Asked when a view has opened and is about to be shown, so the
    /// answer is where the reader is then and not where they were when the
    /// view was asked for.
    pub anchors: Box<dyn Fn() -> Anchors>,
    /// Told after the model has swapped views, with where the anchors went,
    /// and with nothing when no view could be opened.
    pub replaced: Box<OnReplaced>,
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

/// The IDs of selected books, by rank. The selection itself is a bitset of
/// ranks, which mean nothing in the next view, and a page that held a
/// selected book can leave the cache before the view is replaced.
#[derive(Default)]
struct Chosen(HashMap<u32, String>);

impl Chosen {
    /// Takes the ID of each selected book among `rows`, and forgets books
    /// no longer selected.
    fn remember<'a>(
        &mut self,
        selected: impl Fn(u32) -> bool,
        rows: impl Iterator<Item = (u32, &'a str)>,
    ) {
        self.0.retain(|rank, _| selected(*rank));
        for (rank, id) in rows {
            if selected(rank) {
                self.0.entry(rank).or_insert_with(|| id.to_owned());
            }
        }
    }
    /// The selected books in view order.
    fn ids(&self) -> Vec<String> {
        let mut ranked: Vec<_> = self.0.iter().collect();
        ranked.sort();
        ranked.into_iter().map(|(_, id)| id.clone()).collect()
    }
}

/// An open the read thread has been asked for.
struct Opening {
    serial: u64,
    cancel: Cancel,
    background: bool,
    /// The view it made, waiting for its books to be found.
    arrived: Option<Arrived>,
}

struct Arrived {
    view: OpenView,
    /// What was asked to be found in it.
    asked: Asked,
    /// How many times the reader had moved on by the time the answer came.
    again: u32,
}

/// Where the reader is: the selected books and the window's anchors.
#[derive(Clone, PartialEq)]
struct Asked {
    selected: Vec<String>,
    anchors: Anchors,
}

struct State {
    query: Query,
    serial: u64,
    opening: Option<Opening>,
    view: Option<ViewToken>,
    count: u32,
    status: Status,
    pages: HashMap<u32, Vec<Rc<BrowseRow>>>,
    /// Loaded pages, oldest first.
    recent: VecDeque<u32>,
    requested: HashSet<u32>,
    live: HashMap<u32, glib::WeakRef<BookObject>>,
    selection: gtk::Bitset,
    chosen: Chosen,
    /// The catalogue changed while a view was opening.
    dirty: bool,
    last_refresh: Option<Instant>,
    refresh_waiting: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: Query::default(),
            serial: 0,
            opening: None,
            view: None,
            count: 0,
            status: Status::Loading,
            pages: HashMap::new(),
            recent: VecDeque::new(),
            requested: HashSet::new(),
            live: HashMap::new(),
            selection: gtk::Bitset::new_empty(),
            chosen: Chosen::default(),
            dirty: false,
            last_refresh: None,
            refresh_waiting: false,
        }
    }
}

impl State {
    fn row(&self, rank: u32) -> Option<Rc<BrowseRow>> {
        self.pages
            .get(&(rank / PAGE))?
            .get((rank % PAGE) as usize)
            .cloned()
    }
    fn store_page(&mut self, page: u32, rows: Vec<BrowseRow>) {
        self.pages
            .insert(page, rows.into_iter().map(Rc::new).collect());
        self.recent.push_back(page);
        while self.recent.len() > CACHED_PAGES {
            if let Some(old) = self.recent.pop_front() {
                self.pages.remove(&old);
            }
        }
        self.remember_selected();
    }
    /// Notes the ID of every selected book whose row is in memory.
    fn remember_selected(&mut self) {
        let selection = &self.selection;
        self.chosen.remember(
            |rank| selection.contains(rank),
            self.pages.iter().flat_map(|(page, rows)| {
                rows.iter()
                    .enumerate()
                    .map(move |(n, row)| (page * PAGE + n as u32, row.id.as_str()))
            }),
        );
    }
    /// Empties the view, and returns the places that were waiting for a
    /// row with how many items there were.
    fn clear(&mut self) -> (Vec<BookObject>, u32) {
        let left = self
            .live
            .drain()
            .filter_map(|(_, weak)| weak.upgrade())
            .collect();
        self.pages.clear();
        self.recent.clear();
        self.requested.clear();
        self.selection = gtk::Bitset::new_empty();
        self.chosen = Chosen::default();
        (left, std::mem::take(&mut self.count))
    }
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct BookListModel {
        pub(super) requests: OnceCell<mpsc::Sender<Request>>,
        pub(super) state: RefCell<State>,
        pub(super) hooks: RefCell<Option<Rc<Hooks>>>,
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
            let mut state = self.state.borrow_mut();
            if position >= state.count {
                return None;
            }
            if let Some(object) = state.live.get(&position).and_then(|weak| weak.upgrade()) {
                return Some(object.upcast());
            }
            let row = state.row(position);
            let page = position / PAGE;
            if row.is_none()
                && let Some(token) = state.view
                && state.requested.insert(page)
            {
                self.send(Request::Page { token, page });
            }
            let object = BookObject::new(position, row);
            if state.live.len() > 4096 {
                state.live.retain(|_, weak| weak.upgrade().is_some());
            }
            state.live.insert(position, object.downgrade());
            Some(object.upcast())
        }
    }
    impl BookListModel {
        /// Sends a request to the read thread. If the thread has gone, the
        /// model says so once the caller has returned.
        pub(super) fn send(&self, request: Request) {
            let sent = self
                .requests
                .get()
                .is_some_and(|requests| requests.send(request).is_ok());
            if !sent {
                let weak = self.obj().downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(model) = weak.upgrade() {
                        model.lose("The library reader has stopped.".into());
                    }
                });
            }
        }

        /// Applies a change to the selection and reports the span it touched.
        fn select(&self, change: impl FnOnce(&gtk::Bitset, u32)) -> bool {
            let span = {
                let mut state = self.state.borrow_mut();
                let before = state.selection.copy();
                change(&state.selection, state.count);
                before.difference(&state.selection);
                state.remember_selected();
                (!before.is_empty()).then(|| (before.minimum(), before.maximum()))
            };
            if let Some((first, last)) = span {
                self.obj().selection_changed(first, last - first + 1);
            }
            true
        }
    }
    impl SelectionModelImpl for BookListModel {
        fn is_selected(&self, position: u32) -> bool {
            self.state.borrow().selection.contains(position)
        }
        fn selection_in_range(&self, _position: u32, _n_items: u32) -> gtk::Bitset {
            self.state.borrow().selection.copy()
        }
        fn select_item(&self, position: u32, unselect_rest: bool) -> bool {
            self.select(|selection, _| {
                if unselect_rest {
                    selection.remove_all();
                }
                selection.add(position);
            })
        }
        fn select_range(&self, position: u32, n_items: u32, unselect_rest: bool) -> bool {
            self.select(|selection, _| {
                if unselect_rest {
                    selection.remove_all();
                }
                selection.add_range(position, n_items);
            })
        }
        fn select_all(&self) -> bool {
            self.select(|selection, count| selection.add_range(0, count))
        }
        fn unselect_all(&self) -> bool {
            self.select(|selection, _| selection.remove_all())
        }
        fn unselect_item(&self, position: u32) -> bool {
            self.select(|selection, _| {
                selection.remove(position);
            })
        }
        fn unselect_range(&self, position: u32, n_items: u32) -> bool {
            self.select(|selection, _| selection.remove_range(position, n_items))
        }
        fn set_selection(&self, selected: &gtk::Bitset, mask: &gtk::Bitset) -> bool {
            self.select(|selection, _| {
                let chosen = selected.copy();
                chosen.intersect(mask);
                selection.subtract(mask);
                selection.union(&chosen);
            })
        }
    }
    // One section: the whole view.
    impl SectionModelImpl for BookListModel {}
}

glib::wrapper! {
    pub struct BookListModel(ObjectSubclass<imp::BookListModel>)
        @implements gio::ListModel, gtk::SelectionModel, gtk::SectionModel;
}

impl BookListModel {
    pub fn new(store: Store) -> Self {
        let model: Self = glib::Object::new();
        let (requests, received) = mpsc::channel();
        let (send, replies) = async_channel::unbounded();
        std::thread::spawn(move || worker::serve(store, received, send));
        let _ = model.imp().requests.set(requests);
        let weak = model.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(reply) = replies.recv().await {
                let Some(model) = weak.upgrade() else { return };
                model.handle(reply);
            }
            // The read thread ended, which it does not do while it is asked
            // for anything.
            if let Some(model) = weak.upgrade() {
                model.lose("The library reader has stopped.".into());
            }
        });
        model
    }

    pub fn set_hooks(&self, hooks: Hooks) {
        self.imp().hooks.replace(Some(Rc::new(hooks)));
    }
    fn hooks(&self) -> Option<Rc<Hooks>> {
        self.imp().hooks.borrow().clone()
    }

    pub fn status(&self) -> Status {
        self.imp().state.borrow().status.clone()
    }
    pub fn query(&self) -> Query {
        self.imp().state.borrow().query.clone()
    }

    /// Shows another search or sort. The open in flight, if any, is dropped.
    pub fn set_query(&self, query: Query) {
        {
            let mut state = self.imp().state.borrow_mut();
            if state.query == query && (state.view.is_some() || state.opening.is_some()) {
                return;
            }
            state.query = query;
        }
        self.open(false);
    }

    /// Tries again to open the library after it could not be read.
    pub fn retry(&self) {
        {
            let mut state = self.imp().state.borrow_mut();
            state.status = Status::Loading;
        }
        if let Some(hooks) = self.hooks() {
            (hooks.replaced)(None);
        }
        self.open(false);
    }

    /// The write thread finished a job, which may have changed a book.
    pub fn catalogue_changed(&self) {
        let wait = {
            let mut state = self.imp().state.borrow_mut();
            if state.opening.is_some() {
                state.dirty = true;
                return;
            }
            if state.refresh_waiting {
                return;
            }
            let wait = state
                .last_refresh
                .and_then(|last| REFRESH_INTERVAL.checked_sub(last.elapsed()));
            state.refresh_waiting = wait.is_some();
            wait
        };
        match wait {
            None => self.open(true),
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

    fn open(&self, background: bool) {
        let request = {
            let mut state = self.imp().state.borrow_mut();
            if let Some(earlier) = state.opening.take() {
                earlier.cancel.cancel();
            }
            if background {
                state.last_refresh = Some(Instant::now());
            }
            state.serial += 1;
            let cancel = Cancel::default();
            state.opening = Some(Opening {
                serial: state.serial,
                cancel: cancel.clone(),
                background,
                arrived: None,
            });
            Request::Open {
                serial: state.serial,
                query: state.query.clone(),
                cancel,
                only_if_changed: background,
            }
        };
        self.imp().send(request);
    }

    /// Where the reader is now: what is selected, and the window's anchors.
    fn asked(&self) -> Asked {
        let anchors = self
            .hooks()
            .map(|hooks| (hooks.anchors)())
            .unwrap_or_default();
        Asked {
            selected: self.imp().state.borrow().chosen.ids(),
            anchors,
        }
    }

    /// Asks the read thread where the reader's books are in the view that
    /// has just opened.
    fn locate(&self, view: OpenView, again: u32) {
        let asked = self.asked();
        let request = {
            let mut state = self.imp().state.borrow_mut();
            let Some(opening) = state.opening.as_mut() else {
                return;
            };
            let mut ids = asked.selected.clone();
            ids.push(asked.anchors.current.clone().unwrap_or_default());
            ids.push(asked.anchors.top.clone().unwrap_or_default());
            let request = Request::Locate {
                serial: opening.serial,
                token: view.token,
                ids,
            };
            opening.arrived = Some(Arrived { view, asked, again });
            request
        };
        self.imp().send(request);
    }

    /// The open with this serial has been answered. `None` for an answer to
    /// an open that a later one replaced.
    fn finish(&self, serial: u64) -> Option<Opening> {
        let mut state = self.imp().state.borrow_mut();
        if state.opening.as_ref()?.serial != serial {
            return None;
        }
        state.opening.take()
    }

    fn opening(&self, serial: u64) -> bool {
        let state = self.imp().state.borrow();
        state.opening.as_ref().is_some_and(|o| o.serial == serial)
    }

    /// Replaces what the model holds with a view whose books have been found.
    fn swap(&self, opening: Opening, view: OpenView, asked: &Asked, ranks: &[Option<u32>]) {
        let (left, before) = {
            let mut state = self.imp().state.borrow_mut();
            state.view = Some(view.token);
            state.status = Status::Ready;
            // The interval runs from when a refresh is shown.
            if opening.background {
                state.last_refresh = Some(Instant::now());
            }
            state.clear()
        };
        for place in left {
            place.leave();
        }
        // Two signals, not one. Given a removal and an addition together,
        // a GTK list asks for every added item to see whether a row it is
        // showing can stay: 100,000 calls. Between the two the model is
        // empty in every answer it gives: no items and nothing selected.
        self.items_changed(0, before, 0);
        {
            let mut state = self.imp().state.borrow_mut();
            state.count = view.count;
            for (id, rank) in asked.selected.iter().zip(ranks) {
                if let Some(rank) = rank {
                    state.selection.add(*rank);
                    state.chosen.0.insert(*rank, id.clone());
                }
            }
            state.store_page(0, view.first);
        }
        self.items_changed(0, 0, view.count);
        let replaced = Replaced {
            current: ranks[asked.selected.len()],
            top: ranks[asked.selected.len() + 1],
            background: opening.background,
        };
        if let Some(hooks) = self.hooks() {
            (hooks.replaced)(Some(&replaced));
        }
        self.settle();
    }

    /// The view can no longer be read. The window shows why, and every
    /// place waiting for its row is told it will not come.
    fn lose(&self, message: String) {
        let (left, before) = {
            let mut state = self.imp().state.borrow_mut();
            if let Some(opening) = state.opening.take() {
                opening.cancel.cancel();
            }
            state.view = None;
            state.status = Status::Failed(message);
            state.clear()
        };
        for place in left {
            place.leave();
        }
        if before > 0 {
            self.items_changed(0, before, 0);
        }
        if let Some(hooks) = self.hooks() {
            (hooks.replaced)(None);
        }
    }

    fn handle(&self, reply: Reply) {
        match reply {
            Reply::Opened { serial, view } => {
                if self.opening(serial) {
                    self.locate(view, 0);
                }
            }
            Reply::Located { serial, ranks } => {
                if !self.opening(serial) {
                    return;
                }
                let now = self.asked();
                let arrived = {
                    let mut state = self.imp().state.borrow_mut();
                    state.opening.as_mut().and_then(|o| o.arrived.take())
                };
                let Some(arrived) = arrived else { return };
                // The reader scrolled or selected while their books were
                // being found. Find them again, a few times at most.
                if arrived.asked != now && arrived.again < 3 {
                    self.locate(arrived.view, arrived.again + 1);
                    return;
                }
                if let Some(opening) = self.finish(serial) {
                    self.swap(opening, arrived.view, &arrived.asked, &ranks);
                }
            }
            Reply::Unchanged { serial } => {
                if self.finish(serial).is_some() {
                    self.settle();
                }
            }
            Reply::Failed { serial, message } => {
                if self.finish(serial).is_none() {
                    return;
                }
                // A view that is open stays readable when its refresh
                // fails, and the next change to the catalogue tries again.
                if self.imp().state.borrow().view.is_none() {
                    self.lose(message);
                }
                self.settle();
            }
            Reply::Page { token, page, rows } => {
                let arrived: Vec<_> = {
                    let mut state = self.imp().state.borrow_mut();
                    if state.view != Some(token) {
                        return;
                    }
                    state.requested.remove(&page);
                    state.store_page(page, rows);
                    (page * PAGE..(page + 1) * PAGE)
                        .filter_map(|rank| {
                            Some((state.live.get(&rank)?.upgrade()?, state.row(rank)?))
                        })
                        .collect()
                };
                for (object, row) in arrived {
                    object.fill(row);
                }
            }
            Reply::Lost { token, message } => {
                let (current, arriving) = {
                    let state = self.imp().state.borrow();
                    let arriving = state
                        .opening
                        .as_ref()
                        .and_then(|o| o.arrived.as_ref())
                        .is_some_and(|arrived| arrived.view.token == token);
                    (state.view == Some(token), arriving)
                };
                if current || arriving {
                    self.lose(message);
                }
            }
        }
    }

    /// Runs the refresh that was put off while a view was opening.
    fn settle(&self) {
        let dirty = std::mem::take(&mut self.imp().state.borrow_mut().dirty);
        if dirty {
            self.catalogue_changed();
        }
    }

    /// The first selected place.
    pub fn selected_one(&self) -> Option<BookObject> {
        let rank = {
            let state = self.imp().state.borrow();
            (!state.selection.is_empty()).then(|| state.selection.minimum())?
        };
        self.item(rank).and_downcast()
    }

    /// The selected places, in view order.
    pub fn selected(&self) -> Vec<BookObject> {
        let ranks: Vec<u32> = {
            let state = self.imp().state.borrow();
            gtk::BitsetIter::init_first(&state.selection)
                .into_iter()
                .flat_map(|(iter, first)| std::iter::once(first).chain(iter))
                .collect()
        };
        ranks
            .into_iter()
            .filter_map(|rank| self.item(rank).and_downcast())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Chosen;

    #[test]
    fn a_selected_book_is_known_by_id_after_its_page_leaves_memory() {
        let mut chosen = Chosen::default();
        let selected = |rank: u32| rank == 3 || rank == 200;
        // The page holding rank 3 is in memory, and the book is selected.
        chosen.remember(selected, [(2, "b"), (3, "c")].into_iter());
        // That page is dropped and another loads.
        chosen.remember(selected, [(200, "x"), (201, "y")].into_iter());
        assert_eq!(chosen.ids(), ["c", "x"]);
    }

    #[test]
    fn a_book_no_longer_selected_is_forgotten() {
        let mut chosen = Chosen::default();
        chosen.remember(|rank| rank == 3, [(3, "c")].into_iter());
        chosen.remember(|_| false, std::iter::empty());
        assert!(chosen.ids().is_empty());
    }
}

//! The read thread and the one list model every view shares.
//!
//! The read thread owns a `BrowseReader`. The model asks it to open a view
//! (a search and a sort) and for pages of that view, and never waits: an
//! item is handed out at once and filled in when its page arrives. Books
//! are tracked across views by ID, because a rank means nothing once the
//! view is replaced. The selection is a set of book IDs for that reason,
//! and a new view is shown without finding the selected books in it.
mod worker;

use crate::store::Store;
use ebook_library::{BrowseRow, Cancel, OpenView, SortTerm, ViewToken};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk4 as gtk;
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use worker::{Reply, Request, Why};

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

/// Where the reader is in the view on screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anchors {
    /// The book commands act on: the focused one, else the selected one.
    pub current: Option<String>,
    /// The first book on screen.
    pub top: Option<String>,
    /// Where that book starts, from the top of the view.
    pub top_offset: f32,
    /// A book holds the keyboard focus.
    pub focused: bool,
}

/// Where the reader was, and where those books are in the view now shown.
pub struct Placed {
    pub anchors: Anchors,
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
    /// No view can be read. The text says why.
    Failed(String),
}

/// How the window takes part in replacing a view.
pub struct Hooks {
    /// Where the reader is at this moment.
    pub anchors: Box<dyn Fn() -> Anchors>,
    /// The books or the status changed.
    pub changed: Box<dyn Fn()>,
    /// Put the reader back where they were.
    pub place: Box<dyn Fn(&Placed)>,
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

/// An open the read thread has been asked for.
struct Opening {
    serial: u64,
    cancel: Cancel,
    background: bool,
    /// The view it made, waiting for the reader's place to be found in it.
    arrived: Option<(OpenView, Anchors)>,
}

/// Why the read thread was asked where some books are.
enum Locating {
    /// A view has opened, and will be shown when this is answered.
    Place { serial: u64 },
    /// The reader moved while their place was being found. The view is
    /// shown, and this is where they had moved to. `since` is where the
    /// new view stood when it was asked.
    Follow {
        token: ViewToken,
        anchors: Anchors,
        since: Anchors,
        background: bool,
    },
    /// Which of the selected books the view holds at all.
    Prune { token: ViewToken, ids: Vec<String> },
}

struct State {
    query: Query,
    serial: u64,
    opening: Option<Opening>,
    view: Option<ViewToken>,
    /// The view on screen can no longer be read, and another is on its way.
    lost: bool,
    count: u32,
    status: Status,
    pages: HashMap<u32, Vec<Rc<BrowseRow>>>,
    /// Loaded pages, oldest first.
    recent: VecDeque<u32>,
    requested: HashSet<u32>,
    live: HashMap<u32, glib::WeakRef<BookObject>>,
    /// The selected books, by ID.
    selected: HashSet<String>,
    /// Places selected before their rows arrived. Each becomes an ID in
    /// `selected` when its row does.
    pending: BTreeSet<u32>,
    locating: HashMap<u64, Locating>,
    tag: u64,
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
            lost: false,
            count: 0,
            status: Status::Loading,
            pages: HashMap::new(),
            recent: VecDeque::new(),
            requested: HashSet::new(),
            live: HashMap::new(),
            selected: HashSet::new(),
            pending: BTreeSet::new(),
            locating: HashMap::new(),
            tag: 0,
            dirty: false,
            last_refresh: None,
            refresh_waiting: false,
        }
    }
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
        while self.recent.len() > CACHED_PAGES {
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
    fn tag(&mut self, why: Locating) -> u64 {
        self.tag += 1;
        self.locating.insert(self.tag, why);
        self.tag
    }
    /// The places still waiting for a row.
    fn waiting(&self) -> Vec<BookObject> {
        self.live
            .values()
            .filter_map(|weak| weak.upgrade())
            .filter(|place| !place.loaded())
            .collect()
    }
    /// Empties the view, and returns the places that were waiting for a
    /// row with how many items there were.
    fn clear(&mut self) -> (Vec<BookObject>, u32) {
        let left = self.waiting();
        self.live.clear();
        self.pages.clear();
        self.recent.clear();
        self.requested.clear();
        self.pending.clear();
        self.lost = false;
        (left, std::mem::take(&mut self.count))
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
            let object = BookObject::new(position, row.clone());
            if state.live.len() > 4096 {
                state.live.retain(|_, weak| weak.upgrade().is_some());
            }
            state.live.insert(position, object.downgrade());
            // A lost view gives no more rows. Its replacement will.
            let lost = state.lost;
            let ask = match state.view {
                Some(token) if row.is_none() && !lost => state
                    .requested
                    .insert(page)
                    .then_some(Request::Page { token, page }),
                _ => None,
            };
            drop(state);
            if row.is_none() && lost {
                object.leave();
            }
            if let Some(request) = ask {
                self.send(request);
            }
            Some(object.upcast())
        }
    }
    impl BookListModel {
        /// Sends a request to the read thread. If the thread has gone, the
        /// model says so once the caller has returned.
        pub(super) fn send(&self, request: Request) {
            let sent = self
                .requests
                .borrow()
                .as_ref()
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
        fn selection_in_range(&self, _position: u32, _n_items: u32) -> gtk::Bitset {
            bits(&self.state.borrow().selection())
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
                model.handle(reply);
            }
            // The thread ended, which it does not do while the model
            // lives. Try Again starts another.
            if let Some(model) = weak.upgrade()
                && model.imp().started.get() == started
            {
                model.imp().requests.replace(None);
                model.lose("The library reader has stopped.".into());
            }
        });
    }

    pub fn set_hooks(&self, hooks: Hooks) {
        self.imp().hooks.replace(Some(Rc::new(hooks)));
    }
    fn hooks(&self) -> Option<Rc<Hooks>> {
        self.imp().hooks.borrow().clone()
    }
    fn changed(&self) {
        if let Some(hooks) = self.hooks() {
            (hooks.changed)();
        }
    }
    fn anchors(&self) -> Anchors {
        self.hooks()
            .map(|hooks| (hooks.anchors)())
            .unwrap_or_default()
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

    /// Tries again to open the library after it could not be read, with a
    /// new read thread if the last one stopped.
    pub fn retry(&self) {
        if self.imp().requests.borrow().is_none() {
            self.start();
        }
        self.imp().state.borrow_mut().status = Status::Loading;
        self.changed();
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
                // A view that is lost is opened again whatever the
                // catalogue says.
                only_if_changed: background && !state.lost,
            }
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

    /// Asks where the reader's two books are in a view.
    fn locate(&self, token: ViewToken, anchors: &Anchors, why: Locating) {
        let tag = self.imp().state.borrow_mut().tag(why);
        self.imp().send(Request::Locate {
            tag,
            token,
            ids: vec![
                anchors.current.clone().unwrap_or_default(),
                anchors.top.clone().unwrap_or_default(),
            ],
        });
    }

    /// Replaces what the model holds with a view that has opened. The
    /// selection is by ID and needs nothing found: each row says whether
    /// its book is selected as it arrives.
    fn swap(&self, view: OpenView, background: bool) {
        let (left, before) = {
            let mut state = self.imp().state.borrow_mut();
            state.view = Some(view.token);
            state.status = Status::Ready;
            // The interval runs from when a refresh is shown.
            if background {
                state.last_refresh = Some(Instant::now());
            }
            state.clear()
        };
        for place in left {
            place.leave();
        }
        // Two signals, not one. Given a removal and an addition together,
        // a GTK list asks for every added item to see whether a row it is
        // showing can stay: 100,000 calls. Between the two the model has
        // no items, and so nothing selected.
        self.items_changed(0, before, 0);
        let prune = {
            let mut state = self.imp().state.borrow_mut();
            state.count = view.count;
            state.store_page(0, view.first);
            // A selected book the view does not hold leaves the selection.
            // Asking which those are can only remove what is not there, so
            // its answer is safe whenever it comes.
            (!state.selected.is_empty()).then(|| {
                let ids: Vec<String> = state.selected.iter().cloned().collect();
                let tag = state.tag(Locating::Prune {
                    token: view.token,
                    ids: ids.clone(),
                });
                Request::Locate {
                    tag,
                    token: view.token,
                    ids,
                }
            })
        };
        self.items_changed(0, 0, view.count);
        if let Some(request) = prune {
            self.imp().send(request);
        }
        self.changed();
    }

    fn place(&self, anchors: Anchors, ranks: &[Option<u32>], background: bool) {
        if let Some(hooks) = self.hooks() {
            (hooks.place)(&Placed {
                anchors,
                current: ranks.first().copied().flatten(),
                top: ranks.get(1).copied().flatten(),
                background,
            });
        }
    }

    /// No view can be read. The window shows why, and every place waiting
    /// for its row is told it will not come.
    fn lose(&self, message: String) {
        let (left, before) = {
            let mut state = self.imp().state.borrow_mut();
            if let Some(opening) = state.opening.take() {
                opening.cancel.cancel();
            }
            state.view = None;
            state.status = Status::Failed(message);
            state.locating.clear();
            state.clear()
        };
        for place in left {
            place.leave();
        }
        if before > 0 {
            self.items_changed(0, before, 0);
        }
        self.changed();
    }

    /// The view on screen can no longer be read. Its loaded rows stay, the
    /// places waiting for rows are told none will come, and the query is
    /// opened again.
    fn view_lost(&self) {
        let (left, opening) = {
            let mut state = self.imp().state.borrow_mut();
            state.lost = true;
            state.requested.clear();
            (state.waiting(), state.opening.is_some())
        };
        for place in left {
            place.leave();
        }
        if !opening {
            self.open(false);
        }
    }

    fn handle(&self, reply: Reply) {
        match reply {
            Reply::Opened { serial, view } => {
                // Its place is found before it is shown, so the window
                // does not show the top of it first.
                let anchors = self.anchors();
                let token = view.token;
                {
                    let mut state = self.imp().state.borrow_mut();
                    match state.opening.as_mut() {
                        Some(opening) if opening.serial == serial => {
                            opening.arrived = Some((view, anchors.clone()));
                        }
                        // A later open replaced this one.
                        _ => return,
                    }
                }
                self.locate(token, &anchors, Locating::Place { serial });
            }
            Reply::Located { tag, ranks } => self.located(tag, Ok(ranks)),
            Reply::NotLocated { tag, why } => self.located(tag, Err(why)),
            Reply::Unchanged { serial } => {
                if self.finish(serial).is_some() {
                    self.settle();
                }
            }
            Reply::Cancelled => {}
            Reply::Failed { serial, message } => {
                if self.finish(serial).is_none() {
                    return;
                }
                // A view that is still readable stays when its refresh
                // fails, and the next change to the catalogue tries again.
                let none = {
                    let state = self.imp().state.borrow();
                    state.view.is_none() || state.lost
                };
                if none {
                    self.lose(message);
                }
                self.settle();
            }
            Reply::Page { token, page, rows } => {
                let (arrived, selected) = {
                    let mut state = self.imp().state.borrow_mut();
                    // A page of a view the model has replaced.
                    if state.view != Some(token) {
                        return;
                    }
                    state.requested.remove(&page);
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
                // Books selected in an earlier view show as selected now
                // that their rows say which they are.
                if selected {
                    self.selection_changed(page * PAGE, PAGE);
                }
            }
            Reply::NoPage { token, why, .. } => {
                // For a view the model has replaced there is nothing to
                // do. For the one on screen, it is lost, whatever the
                // reason: another view took its place in the reader, or it
                // could not be read.
                if self.imp().state.borrow().view != Some(token) {
                    return;
                }
                if let Why::Failed(message) = why {
                    glib::g_warning!(
                        "ebook-manager",
                        "The books shown could not be read: {message}"
                    );
                }
                self.view_lost();
            }
        }
    }

    /// The read thread has said where some books are, or that it cannot.
    fn located(&self, tag: u64, ranks: Result<Vec<Option<u32>>, Why>) {
        let why = self.imp().state.borrow_mut().locating.remove(&tag);
        match why {
            Some(Locating::Place { serial }) => {
                let Some(opening) = self.finish(serial) else {
                    return;
                };
                let Some((view, asked)) = opening.arrived else {
                    return;
                };
                // The reader's view is another already, so this one could
                // not be read once shown. The query is opened again.
                if matches!(ranks, Err(Why::NotCurrent)) {
                    self.open(opening.background);
                    return;
                }
                let token = view.token;
                let now = self.anchors();
                self.swap(view, opening.background);
                match ranks {
                    Ok(ranks) if now == asked => self.place(asked, &ranks, opening.background),
                    // The reader moved while their place was being found,
                    // so that place is stale and is not applied. Where they
                    // are now is found once more.
                    _ => {
                        let since = self.anchors();
                        let follow = Locating::Follow {
                            token,
                            anchors: now.clone(),
                            since,
                            background: opening.background,
                        };
                        self.locate(token, &now, follow);
                    }
                }
                self.settle();
            }
            Some(Locating::Follow {
                token,
                anchors,
                since,
                background,
            }) => {
                // If the reader has moved again, the view stays where they
                // put it.
                let showing = self.imp().state.borrow().view == Some(token);
                if let Ok(ranks) = ranks
                    && showing
                    && self.anchors() == since
                {
                    self.place(anchors, &ranks, background);
                }
            }
            Some(Locating::Prune { token, ids }) => {
                let Ok(ranks) = ranks else { return };
                let removed = {
                    let mut state = self.imp().state.borrow_mut();
                    if state.view != Some(token) {
                        return;
                    }
                    let before = state.selected.len();
                    for (id, rank) in ids.iter().zip(&ranks) {
                        if rank.is_none() {
                            state.selected.remove(id);
                        }
                    }
                    before != state.selected.len()
                };
                // None of them was on screen, so no row changes its look.
                // The commands that count the selection are told.
                if removed {
                    self.changed();
                }
            }
            None => {}
        }
    }

    /// Runs the refresh that was put off while a view was opening.
    fn settle(&self) {
        let dirty = std::mem::take(&mut self.imp().state.borrow_mut().dirty);
        if dirty {
            self.catalogue_changed();
        }
    }

    /// How many books are selected.
    pub fn selected_count(&self) -> u64 {
        let state = self.imp().state.borrow();
        (state.selected.len() + state.pending.len()) as u64
    }

    /// The selected book, when there is one and only one.
    pub fn selected_one(&self) -> Option<String> {
        let state = self.imp().state.borrow();
        (state.selected.len() == 1 && state.pending.is_empty())
            .then(|| state.selected.iter().next().cloned())
            .flatten()
    }

    /// The IDs of the selected books. A place selected before its row
    /// arrived is waited for. `None` when the view went first, and the
    /// place no longer stands for a book.
    pub async fn selected(&self) -> Option<Vec<String>> {
        let waiting: Vec<BookObject> = {
            let pending: Vec<u32> = self.imp().state.borrow().pending.iter().copied().collect();
            pending
                .into_iter()
                .filter_map(|rank| self.item(rank).and_downcast())
                .collect()
        };
        for place in waiting {
            place.loaded_row().await?;
        }
        Some(self.imp().state.borrow().selected.iter().cloned().collect())
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
        }
    }

    fn state(count: u32) -> State {
        State {
            count,
            ..Default::default()
        }
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
}

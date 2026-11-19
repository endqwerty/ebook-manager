//! The read thread and the one list model every view shares.
//!
//! The read thread owns a `BrowseReader`. The model asks it to open a view
//! (a search and a sort) and for pages of that view, and never waits: an
//! item is handed out at once and filled in when its page arrives. Books
//! are tracked across views by ID, because a rank means nothing once the
//! view is replaced.
use crate::store::Store;
use ebook_library::{
    BrowseQuery, BrowseReader, BrowseRow, Cancel, Cancelled, OpenView, SortOrder, SortTerm,
    ViewToken,
};
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk4 as gtk;
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

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
#[derive(Default)]
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
    /// Asked just before a view is requested.
    pub anchors: Box<dyn Fn() -> Anchors>,
    /// Told after the model has swapped views, with where the anchors went,
    /// and with nothing when no view could be opened.
    pub replaced: Box<OnReplaced>,
}

enum Request {
    Open {
        serial: u64,
        query: Query,
        cancel: Cancel,
        /// IDs to find in the new view.
        keep: Vec<String>,
        /// Answer `Unchanged` when the open view still matches the catalogue.
        only_if_changed: bool,
    },
    Page {
        token: ViewToken,
        page: u32,
    },
}

enum Reply {
    Opened {
        serial: u64,
        view: OpenView,
        kept: Vec<Option<u32>>,
    },
    Unchanged {
        serial: u64,
    },
    Failed {
        serial: u64,
        message: String,
    },
    Page {
        token: ViewToken,
        page: u32,
        rows: Vec<BrowseRow>,
    },
}

fn open(
    reader: &mut BrowseReader,
    query: &Query,
    cancel: &Cancel,
    keep: &[String],
) -> anyhow::Result<(OpenView, Vec<Option<u32>>)> {
    let query = BrowseQuery {
        text: query.text.clone(),
        sort: if query.sort.is_empty() {
            SortOrder::default()
        } else {
            SortOrder::new(query.sort.clone())?
        },
    };
    let view = reader.open(&query, cancel)?;
    let kept = reader.locate(view.token, keep)?;
    Ok((view, kept))
}

fn serve(store: Store, requests: mpsc::Receiver<Request>, replies: async_channel::Sender<Reply>) {
    let mut reader: Option<BrowseReader> = None;
    for request in requests {
        let reply = match request {
            Request::Open {
                serial,
                query,
                cancel,
                keep,
                only_if_changed,
            } => {
                let result = (|| {
                    if reader.is_none() {
                        reader = Some(store.call(|library| library.browse_reader())?);
                    }
                    let reader = reader.as_mut().expect("reader was just opened");
                    if only_if_changed && !reader.changed()? {
                        return Ok(None);
                    }
                    open(reader, &query, &cancel, &keep).map(Some)
                })();
                match result {
                    Ok(Some((view, kept))) => Reply::Opened { serial, view, kept },
                    Ok(None) => Reply::Unchanged { serial },
                    // A newer request replaced this one, and its reply follows.
                    Err(e) if e.is::<Cancelled>() => continue,
                    Err(e) => {
                        reader = None;
                        Reply::Failed {
                            serial,
                            message: format!("{e:#}"),
                        }
                    }
                }
            }
            Request::Page { token, page } => {
                match reader
                    .as_mut()
                    .map(|reader| reader.window(token, page * PAGE, PAGE))
                {
                    Some(Ok(rows)) => Reply::Page { token, page, rows },
                    // A newer open replaced the view, and its reply says so.
                    _ => continue,
                }
            }
        };
        if replies.send_blocking(reply).is_err() {
            return;
        }
    }
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
    /// The book's row, once its page has arrived. Never resolves if the
    /// view is replaced first, so a command on a vanished row does nothing.
    pub async fn loaded_row(&self) -> Rc<BrowseRow> {
        if let Some(row) = self.row() {
            return row;
        }
        let (tx, rx) = async_channel::bounded(1);
        let handler = self.connect_loaded_notify(move |_| {
            let _ = tx.try_send(());
        });
        let arrived = rx.recv().await;
        self.disconnect(handler);
        match (arrived, self.row()) {
            (Ok(()), Some(row)) => row,
            _ => std::future::pending().await,
        }
    }
}

struct Opening {
    serial: u64,
    cancel: Cancel,
    selected: usize,
    background: bool,
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
                && let Some(requests) = self.requests.get()
            {
                let _ = requests.send(Request::Page { token, page });
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
        /// Applies a change to the selection and reports the span it touched.
        fn select(&self, change: impl FnOnce(&gtk::Bitset, u32)) -> bool {
            let span = {
                let state = self.state.borrow();
                let before = state.selection.copy();
                change(&state.selection, state.count);
                before.difference(&state.selection);
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
        std::thread::spawn(move || serve(store, received, send));
        let _ = model.imp().requests.set(requests);
        let weak = model.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(reply) = replies.recv().await {
                let Some(model) = weak.upgrade() else { break };
                model.handle(reply);
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
        let anchors = self
            .hooks()
            .map(|hooks| (hooks.anchors)())
            .unwrap_or_default();
        let mut state = self.imp().state.borrow_mut();
        if let Some(earlier) = state.opening.take() {
            earlier.cancel.cancel();
        }
        if background {
            state.last_refresh = Some(Instant::now());
        }
        // A selected row that never loaded has no ID to find it by.
        let mut keep: Vec<String> = gtk::BitsetIter::init_first(&state.selection)
            .into_iter()
            .flat_map(|(iter, first)| std::iter::once(first).chain(iter))
            .filter_map(|rank| Some(state.row(rank)?.id.clone()))
            .collect();
        let selected = keep.len();
        keep.push(anchors.current.unwrap_or_default());
        keep.push(anchors.top.unwrap_or_default());
        state.serial += 1;
        let cancel = Cancel::default();
        state.opening = Some(Opening {
            serial: state.serial,
            cancel: cancel.clone(),
            selected,
            background,
        });
        if let Some(requests) = self.imp().requests.get() {
            let _ = requests.send(Request::Open {
                serial: state.serial,
                query: state.query.clone(),
                cancel,
                keep,
                only_if_changed: background,
            });
        }
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

    fn handle(&self, reply: Reply) {
        match reply {
            Reply::Opened { serial, view, kept } => {
                let Some(opening) = self.finish(serial) else {
                    return;
                };
                let (before, replaced) = {
                    let mut state = self.imp().state.borrow_mut();
                    let before = state.count;
                    state.view = Some(view.token);
                    state.count = 0;
                    state.status = Status::Ready;
                    // The interval runs from when a refresh is shown.
                    if opening.background {
                        state.last_refresh = Some(Instant::now());
                    }
                    state.pages.clear();
                    state.recent.clear();
                    state.requested.clear();
                    state.live.clear();
                    state.store_page(0, view.first);
                    state.selection = gtk::Bitset::new_empty();
                    for rank in kept[..opening.selected].iter().flatten() {
                        state.selection.add(*rank);
                    }
                    let replaced = Replaced {
                        current: kept[opening.selected],
                        top: kept[opening.selected + 1],
                        background: opening.background,
                    };
                    (before, replaced)
                };
                // Two signals, not one. Given a removal and an addition
                // together, a GTK list asks for every added item to see
                // whether a row it is showing can stay: 100,000 calls.
                self.items_changed(0, before, 0);
                self.imp().state.borrow_mut().count = view.count;
                self.items_changed(0, 0, view.count);
                if let Some(hooks) = self.hooks() {
                    (hooks.replaced)(Some(&replaced));
                }
                self.settle();
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
                let shown = {
                    let mut state = self.imp().state.borrow_mut();
                    // A view that is open stays usable when its refresh fails.
                    let first = state.view.is_none();
                    if first {
                        state.status = Status::Failed(message);
                    }
                    first
                };
                if shown && let Some(hooks) = self.hooks() {
                    (hooks.replaced)(None);
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

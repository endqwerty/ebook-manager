//! The model's state as data, and what it does with each thing that can
//! happen to it, as one function with no GTK in it. The GObject in
//! `browse.rs` calls `Core::step` and carries out the effects it returns.
//!
//! The states, and what the enum cannot hold on purpose:
//!
//! - `Loading` always has an open in flight. There is no loading with
//!   nothing asked for.
//! - `Showing` has a view and may have an open in flight for a newer query
//!   or a refresh.
//! - `Lost` has the view that can no longer be read and always an open in
//!   flight to replace it. There is no lost view with nothing coming.
//! - `Error` has its text and nothing else: no view, so no rows of an
//!   older query stay on screen, and no open in flight.
//!
//! A view on screen is always one the model was given by the read thread
//! for a query. The model never shows a view and an error together.
use super::worker::{Pages, Reply, Request, Why};
use super::{Anchors, Placed, Placement, Query};
use ebook_library::{BrowseRow, Cancel, OpenView, ViewToken};
use std::collections::{HashMap, HashSet};

/// What can happen to the model.
pub enum Event {
    /// The reader asked for another search or sort.
    Query(Query),
    /// Open the query for a view built anew.
    Layout(Query),
    /// The catalogue may have changed.
    Refresh,
    /// Try Again, from the error state.
    Retry,
    /// A place on screen needs the row at this rank.
    Want(u32),
    /// Which of these books does the view on screen hold?
    Prune(Vec<String>),
    /// The books of a `Swap` are on screen.
    Swapped,
    /// The read thread answered.
    Reply(Reply),
    /// The read thread ended.
    Stopped,
}

/// What the model's owner must do, in order.
pub enum Effect {
    Send(Request),
    /// Show these books in place of whatever is shown.
    Swap {
        view: OpenView,
        strip: Option<std::sync::Arc<ebook_library::ShelfStrip>>,
        background: bool,
        pages: Pages,
        shelf_focus: Option<u32>,
    },
    /// A page of the view on screen arrived.
    Fill {
        page: u32,
        rows: Vec<BrowseRow>,
    },
    /// Every place waiting for a row is told none will come.
    Gone,
    /// Put the reader here.
    Place(Placed),
    /// These selected books are not in the view on screen.
    Absent(Vec<String>),
    /// Nothing can be shown, or loading began again: remove the books.
    Empty,
}

/// An open the read thread has been asked for.
pub struct Opening {
    serial: u64,
    cancel: Cancel,
    query: Query,
    reason: Placement,
    /// The view it made, waiting for the reader's place to be found in it.
    arrived: Option<(
        OpenView,
        Option<std::sync::Arc<ebook_library::ShelfStrip>>,
        Anchors,
    )>,
}

/// A view on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub token: ViewToken,
    pub count: u32,
    /// The query it was opened for.
    pub query: Query,
}

pub enum Phase {
    Loading {
        opening: Opening,
    },
    Showing {
        view: Shown,
        opening: Option<Opening>,
    },
    Lost {
        view: Shown,
        opening: Opening,
    },
    Error {
        message: String,
    },
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
        /// Unknown until the new view is on screen.
        since: Option<Anchors>,
        reason: Placement,
    },
    /// Which of the selected books the view holds at all.
    Prune { token: ViewToken, ids: Vec<String> },
}

pub struct Core {
    pub phase: Phase,
    /// What the reader last asked to see.
    pub query: Query,
    serial: u64,
    tag: u64,
    locating: HashMap<u64, Locating>,
    /// Pages of the view on screen that have been asked for.
    requested: HashSet<u32>,
    /// The catalogue changed while a view was opening.
    dirty: bool,
}

impl Core {
    /// A model that has asked for its first view. The effects carry the
    /// request.
    pub fn new(query: Query) -> (Self, Vec<Effect>) {
        let mut core = Self {
            phase: Phase::Error {
                message: String::new(),
            },
            query,
            serial: 0,
            tag: 0,
            locating: HashMap::new(),
            requested: HashSet::new(),
            dirty: false,
        };
        let mut effects = vec![];
        core.open(Placement::Command, &mut effects);
        (core, effects)
    }

    fn opening(&mut self) -> Option<&mut Opening> {
        match &mut self.phase {
            Phase::Loading { opening } | Phase::Lost { opening, .. } => Some(opening),
            Phase::Showing { opening, .. } => opening.as_mut(),
            Phase::Error { .. } => None,
        }
    }

    fn is_opening(&mut self, serial: u64) -> bool {
        self.opening().is_some_and(|o| o.serial == serial)
    }

    /// Whether the view on screen is this one and can be read.
    fn showing(&self, token: ViewToken) -> bool {
        matches!(&self.phase, Phase::Showing { view, .. } if view.token == token)
    }

    /// Asks the read thread for the current query, in place of any open in
    /// flight. A refresh of a view that can be read may be answered
    /// `Unchanged`. Anything else is an open that must give a view.
    fn open(&mut self, reason: Placement, effects: &mut Vec<Effect>) {
        if let Some(earlier) = self.opening() {
            earlier.cancel.cancel();
        }
        self.serial += 1;
        self.dirty = false;
        let cancel = Cancel::default();
        let opening = Opening {
            serial: self.serial,
            cancel: cancel.clone(),
            query: self.query.clone(),
            reason,
            arrived: None,
        };
        let phase = std::mem::replace(
            &mut self.phase,
            Phase::Error {
                message: String::new(),
            },
        );
        let (showing, refresh) = match &phase {
            Phase::Showing { view, .. } => (Some(view.token), reason == Placement::Refresh),
            Phase::Lost { view, .. } => (Some(view.token), false),
            _ => (None, false),
        };
        self.phase = match phase {
            Phase::Showing { view, .. } => Phase::Showing {
                view,
                opening: Some(opening),
            },
            Phase::Lost { view, .. } => Phase::Lost { view, opening },
            Phase::Loading { .. } => Phase::Loading { opening },
            Phase::Error { .. } => {
                // Loading begins again with nothing on screen.
                effects.push(Effect::Empty);
                Phase::Loading { opening }
            }
        };
        effects.push(Effect::Send(Request::Open {
            serial: self.serial,
            query: self.query.clone(),
            cancel,
            showing,
            refresh,
        }));
    }

    /// Nothing can be shown.
    fn fail(&mut self, message: String, effects: &mut Vec<Effect>) {
        if let Some(opening) = self.opening() {
            opening.cancel.cancel();
        }
        self.phase = Phase::Error { message };
        self.locating.clear();
        self.requested.clear();
        self.dirty = false;
        effects.push(Effect::Gone);
        effects.push(Effect::Empty);
    }

    fn locate(
        &mut self,
        token: ViewToken,
        ids: Vec<String>,
        why: Locating,
        effects: &mut Vec<Effect>,
    ) {
        self.tag += 1;
        let shelf = match (&why, self.opening()) {
            (Locating::Place { .. }, Some(o))
                if o.reason == Placement::Refresh && o.query.shelf =>
            {
                o.arrived.as_ref().and_then(|(_, _, anchors)| anchors.shelf)
            }
            _ => None,
        };
        self.locating.insert(self.tag, why);
        effects.push(Effect::Send(Request::Locate {
            tag: self.tag,
            token,
            ids,
            shelf,
        }));
    }

    fn place_ids(anchors: &Anchors) -> Vec<String> {
        let mut ids = vec![
            anchors.current.clone().unwrap_or_default(),
            anchors.top.clone().unwrap_or_default(),
        ];
        ids.extend(anchors.shelf_focus.iter().cloned());
        ids
    }

    /// The open in flight has been answered. Runs the refresh that was put
    /// off while it was.
    fn settle(&mut self, effects: &mut Vec<Effect>) {
        if std::mem::take(&mut self.dirty)
            && matches!(self.phase, Phase::Showing { opening: None, .. })
        {
            self.open(Placement::Refresh, effects);
        }
    }

    /// Takes one event and returns what to do about it. `anchors` says
    /// where the reader is at this moment.
    pub fn step(&mut self, event: Event, anchors: &dyn Fn() -> Anchors) -> Vec<Effect> {
        let mut effects = vec![];
        match event {
            Event::Query(query) => {
                let same = query == self.query && !matches!(self.phase, Phase::Error { .. });
                if !same {
                    self.query = query;
                    self.open(Placement::Command, &mut effects);
                }
            }
            Event::Layout(query) => {
                self.query = query;
                self.open(Placement::Layout, &mut effects);
            }
            Event::Retry => {
                if matches!(self.phase, Phase::Error { .. }) {
                    self.open(Placement::Command, &mut effects);
                }
            }
            Event::Refresh => match &self.phase {
                Phase::Showing { opening: None, .. } => self.open(Placement::Refresh, &mut effects),
                // The open in flight may have read the catalogue before
                // the change. A refresh follows it.
                Phase::Showing { .. } | Phase::Loading { .. } | Phase::Lost { .. } => {
                    self.dirty = true
                }
                // The change may be what was wrong.
                Phase::Error { .. } => self.open(Placement::Command, &mut effects),
            },
            Event::Want(rank) => match &self.phase {
                Phase::Showing { view, .. } => {
                    let page = rank / super::PAGE;
                    if rank < view.count && self.requested.insert(page) {
                        effects.push(Effect::Send(Request::Page {
                            token: view.token,
                            page,
                        }));
                    }
                }
                // A lost view gives no more rows. Its replacement will.
                _ => effects.push(Effect::Gone),
            },
            Event::Swapped => {
                let now = anchors();
                for why in self.locating.values_mut() {
                    if let Locating::Follow { since, .. } = why {
                        since.get_or_insert_with(|| now.clone());
                    }
                }
            }
            Event::Prune(ids) => {
                if let Phase::Showing { view, .. } = &self.phase {
                    let token = view.token;
                    let why = Locating::Prune {
                        token,
                        ids: ids.clone(),
                    };
                    self.locate(token, ids, why, &mut effects);
                }
            }
            Event::Stopped => self.fail("The library reader has stopped.".into(), &mut effects),
            Event::Reply(reply) => self.reply(reply, anchors, &mut effects),
        }
        effects
    }

    fn reply(&mut self, reply: Reply, anchors: &dyn Fn() -> Anchors, effects: &mut Vec<Effect>) {
        match reply {
            Reply::Opened {
                serial,
                view,
                strip,
            } => {
                if !self.is_opening(serial) {
                    return;
                }
                // Its place is found before it is shown, so the window
                // does not show the top of it first.
                let now = anchors();
                let token = view.token;
                let ids = Self::place_ids(&now);
                if let Some(opening) = self.opening() {
                    opening.arrived = Some((view, strip, now));
                }
                self.locate(token, ids, Locating::Place { serial }, effects);
            }
            Reply::Located { tag, ranks, pages } => {
                self.located(tag, Ok(ranks), pages, anchors, effects)
            }
            Reply::NotLocated { tag, why } => {
                self.located(tag, Err(why), Vec::new(), anchors, effects)
            }
            Reply::Unchanged { serial } => {
                if !self.is_opening(serial) {
                    return;
                }
                match &mut self.phase {
                    // The view on screen is the reader's own and is as the
                    // catalogue has it.
                    Phase::Showing { opening, .. } => {
                        *opening = None;
                        self.settle(effects);
                    }
                    // It was lost after the refresh was asked for. A
                    // refresh cannot bring it back.
                    Phase::Lost { .. } => self.open(Placement::Command, effects),
                    Phase::Loading { .. } | Phase::Error { .. } => {}
                }
            }
            Reply::Cancelled { serial } => {
                // Only an open a newer one replaced is cancelled. If the
                // newest was, nothing is coming.
                if self.is_opening(serial) {
                    self.fail("The library was not opened.".into(), effects);
                }
            }
            Reply::Failed { serial, message } => {
                // The newest open failed, so there is nothing to show for
                // what the reader asked. Rows of an older query do not stay.
                if self.is_opening(serial) {
                    self.fail(message, effects);
                }
            }
            Reply::Page { token, page, rows } => {
                if self.showing(token) && self.requested.remove(&page) {
                    effects.push(Effect::Fill { page, rows });
                }
            }
            Reply::NoPage { token, page, .. } => {
                // For a view the model has replaced there is nothing to
                // do. The view on screen is lost, whatever the reason.
                if !self.showing(token) || !self.requested.contains(&page) {
                    return;
                }
                self.requested.clear();
                effects.push(Effect::Gone);
                let phase = std::mem::replace(
                    &mut self.phase,
                    Phase::Error {
                        message: String::new(),
                    },
                );
                match phase {
                    // The open in flight named this view as the one on
                    // screen, so its answer is about this view: a new one,
                    // a failure, or `Unchanged`, which `Lost` answers with
                    // an open of its own.
                    Phase::Showing {
                        view,
                        opening: Some(opening),
                    } => self.phase = Phase::Lost { view, opening },
                    Phase::Showing {
                        view,
                        opening: None,
                    } => {
                        self.phase = Phase::Showing {
                            view,
                            opening: None,
                        };
                        self.open(Placement::Command, effects);
                        self.lose();
                    }
                    other => self.phase = other,
                }
            }
            Reply::Ids { .. } => {}
        }
    }

    /// Turns a showing view with an open in flight into a lost one.
    fn lose(&mut self) {
        let phase = std::mem::replace(
            &mut self.phase,
            Phase::Error {
                message: String::new(),
            },
        );
        self.phase = match phase {
            Phase::Showing {
                view,
                opening: Some(opening),
            } => Phase::Lost { view, opening },
            other => other,
        };
    }

    fn located(
        &mut self,
        tag: u64,
        ranks: Result<Vec<Option<u32>>, Why>,
        pages: Pages,
        anchors: &dyn Fn() -> Anchors,
        effects: &mut Vec<Effect>,
    ) {
        match self.locating.remove(&tag) {
            Some(Locating::Place { serial }) => {
                if !self.is_opening(serial) {
                    return;
                }
                // The reader's view is another already, so this one could
                // not be read once shown. The query is opened again.
                if matches!(ranks, Err(Why::NotCurrent)) {
                    let reason = self.opening().map_or(Placement::Command, |o| o.reason);
                    self.open(reason, effects);
                    return;
                }
                // The view could not be read between opening and placing,
                // so its books cannot be shown. That is an error to show,
                // with Try Again, and not a view to publish.
                if matches!(ranks, Err(Why::Unreadable)) {
                    self.fail("The library could not be read.".into(), effects);
                    return;
                }
                let now = anchors();
                if let Some(opening) = self.opening()
                    && opening.reason == Placement::Refresh
                    && opening.query.shelf
                    && let Some((view, _, asked)) = &mut opening.arrived
                    && *asked != now
                {
                    // Keep showing the old books while the thread prepares
                    // the viewport the reader actually moved to.
                    let token = view.token;
                    let ids = Self::place_ids(&now);
                    *asked = now;
                    self.locate(token, ids, Locating::Place { serial }, effects);
                    return;
                }
                let phase = std::mem::replace(
                    &mut self.phase,
                    Phase::Error {
                        message: String::new(),
                    },
                );
                let opening = match phase {
                    Phase::Loading { opening } | Phase::Lost { opening, .. } => opening,
                    Phase::Showing {
                        opening: Some(opening),
                        ..
                    } => opening,
                    other => {
                        self.phase = other;
                        return;
                    }
                };
                let Some((view, strip, asked)) = opening.arrived else {
                    self.fail("The view arrived without its books.".into(), effects);
                    return;
                };
                let token = view.token;
                let shelf = opening.query.shelf;
                self.requested.clear();
                self.locating
                    .retain(|_, why| matches!(why, Locating::Place { .. }));
                self.phase = Phase::Showing {
                    view: Shown {
                        token,
                        count: view.count,
                        query: opening.query,
                    },
                    opening: None,
                };
                effects.push(Effect::Gone);
                effects.push(Effect::Swap {
                    strip,
                    view,
                    background: opening.reason == Placement::Refresh,
                    pages,
                    shelf_focus: if shelf && asked.shelf_focus == now.shelf_focus {
                        ranks
                            .as_ref()
                            .ok()
                            .and_then(|rs| rs.get(2).copied().flatten())
                    } else {
                        None
                    },
                });
                match ranks {
                    Ok(ranks) if now == asked => effects.push(Effect::Place(Placed {
                        anchors: asked,
                        current: ranks.first().copied().flatten(),
                        top: ranks.get(1).copied().flatten(),
                        reason: opening.reason,
                    })),
                    // The reader moved while their place was being found,
                    // so that place is stale and is not applied. Where
                    // they are now is found once more, and applied only if
                    // the new view has not been moved by then.
                    _ => {
                        let follow = Locating::Follow {
                            token,
                            anchors: now.clone(),
                            since: None,
                            reason: opening.reason,
                        };
                        self.locate(token, Self::place_ids(&now), follow, effects);
                    }
                }
                self.settle(effects);
            }
            Some(Locating::Follow {
                token,
                anchors: was,
                since,
                reason,
            }) => {
                if let Ok(ranks) = ranks
                    && self.showing(token)
                    && since.is_some_and(|since| anchors() == since)
                {
                    effects.push(Effect::Place(Placed {
                        anchors: was,
                        current: ranks.first().copied().flatten(),
                        top: ranks.get(1).copied().flatten(),
                        reason,
                    }));
                }
            }
            Some(Locating::Prune { token, ids }) => {
                if let Ok(ranks) = ranks
                    && self.showing(token)
                {
                    let absent: Vec<String> = ids
                        .into_iter()
                        .zip(ranks)
                        .filter_map(|(id, rank)| rank.is_none().then_some(id))
                        .collect();
                    if !absent.is_empty() {
                        effects.push(Effect::Absent(absent));
                    }
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{browse::PAGE, store::Store};
    use ebook_library::{Field, SortTerm, fixture};
    use std::{
        cell::RefCell,
        collections::HashMap,
        sync::mpsc,
        time::{Duration, Instant},
    };

    const BOOKS: u32 = 700;

    /// What a request is known by when its reply comes.
    fn open_key(serial: u64) -> String {
        format!("open {serial}")
    }
    fn locate_key(tag: u64) -> String {
        format!("locate {tag}")
    }
    fn page_key(token: ViewToken, page: u32) -> String {
        format!("page {page} of {token:?}")
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum OpenOutcome {
        Succeeded,
        Unchanged,
        FailedOrCancelled,
        UnreadablePlace,
    }

    /// The model's core, the real read thread, and a stand-in for the
    /// window: places waiting for rows, and where the reader is.
    struct Harness {
        core: Core,
        store: Store,
        requests: Option<mpsc::Sender<Request>>,
        replies: Option<async_channel::Receiver<Reply>>,
        /// How many replies each request sent is still owed.
        owed: HashMap<String, u32>,
        /// Ranks a place is waiting for a row at.
        waiting: Vec<u32>,
        /// Pages whose rows the window holds.
        filled: Vec<u32>,
        /// Where the reader is, which the steps move about.
        anchors: RefCell<Anchors>,
        /// The view the reader last gave: token, count and first-page book IDs.
        readers: Option<(ViewToken, u32, Vec<String>)>,
        /// First-page book IDs currently shown by the window.
        shown_first: Vec<String>,
        latest_open_serial: u64,
        latest_open_outcome: Option<OpenOutcome>,
        thread_stopped: bool,
        steps: u64,
        replies_seen: u64,
        unchanged: u64,
        lost: u64,
        stops: u64,
        evictions: u64,
        seed: u64,
    }

    impl Harness {
        fn new(app: &std::path::Path, seed: u64) -> Self {
            let store = Store::open(app.to_owned(), || {});
            let (core, effects) = Core::new(Query::default());
            let mut harness = Self {
                core,
                store,
                requests: None,
                replies: None,
                owed: HashMap::new(),
                waiting: vec![],
                filled: vec![],
                anchors: RefCell::new(Anchors::default()),
                readers: None,
                shown_first: vec![],
                latest_open_serial: 0,
                latest_open_outcome: None,
                thread_stopped: false,
                steps: 0,
                replies_seen: 0,
                unchanged: 0,
                lost: 0,
                stops: 0,
                evictions: 0,
                seed,
            };
            harness.start_thread();
            harness.perform(effects);
            harness
        }

        fn start_thread(&mut self) {
            let (requests, received) = mpsc::channel();
            let (send, replies) = async_channel::unbounded();
            let for_thread = self.store.clone();
            std::thread::spawn(move || crate::browse::worker::serve(for_thread, received, send));
            self.requests = Some(requests);
            self.replies = Some(replies);
            self.owed.clear();
            self.readers = None;
            self.thread_stopped = false;
        }

        fn stop_thread(&mut self) {
            self.requests = None;
            self.replies = None;
            self.owed.clear();
            self.thread_stopped = true;
            self.stops += 1;
            self.event(Event::Stopped);
        }

        fn random(&mut self, below: u64) -> u64 {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            self.seed % below
        }

        fn event(&mut self, event: Event) {
            let effects = {
                let anchors = &self.anchors;
                self.core.step(event, &|| anchors.borrow().clone())
            };
            self.perform(effects);
        }

        fn perform(&mut self, effects: Vec<Effect>) {
            for effect in effects {
                match effect {
                    Effect::Send(request) => {
                        if let Request::Open { serial, .. } = &request {
                            self.latest_open_serial = *serial;
                            self.latest_open_outcome = None;
                        }
                        if let Some(requests) = &self.requests {
                            let sent = match &request {
                                Request::Open { serial, .. } => open_key(*serial),
                                Request::Locate { tag, .. } => locate_key(*tag),
                                Request::Page { token, page } => page_key(*token, *page),
                                Request::Ids { tag, .. } => format!("ids {tag}"),
                            };
                            *self.owed.entry(sent).or_default() += 1;
                            requests.send(request).unwrap();
                        } else {
                            self.event(Event::Stopped);
                        }
                    }
                    // Every waiting place is told.
                    Effect::Gone => self.waiting.clear(),
                    Effect::Swap {
                        ref strip,
                        ref view,
                        ref pages,
                        background,
                        shelf_focus,
                    } => {
                        assert_eq!(
                            strip.as_ref().map(|s| s.thickness.len()),
                            self.core.query.shelf.then_some(view.count as usize)
                        );
                        assert!(self.waiting.is_empty(), "a swap left places untold");
                        self.shown_first = view.first.iter().map(|r| r.id.clone()).collect();
                        self.filled = vec![0];
                        self.filled.extend(pages.iter().map(|(page, _)| *page));
                        if background
                            && strip.is_some()
                            && let Some(window) = self.anchors.borrow().shelf
                        {
                            for rank in
                                window.first..(window.first + window.capacity).min(view.count)
                            {
                                assert!(
                                    self.filled.contains(&(rank / PAGE)),
                                    "refresh lost a visible page"
                                );
                            }
                            if let Some(rank) = shelf_focus {
                                let page = rank / PAGE;
                                let rows = if page == 0 {
                                    &view.first
                                } else {
                                    &pages.iter().find(|(p, _)| *p == page).unwrap().1
                                };
                                assert_eq!(
                                    Some(&rows[(rank % PAGE) as usize].id),
                                    self.anchors.borrow().shelf_focus.as_ref()
                                );
                            }
                        }
                        self.event(Event::Swapped);
                    }
                    Effect::Fill { page, .. } => {
                        self.filled.push(page);
                        self.waiting.retain(|rank| rank / PAGE != page);
                    }
                    Effect::Empty => {
                        assert!(self.waiting.is_empty(), "an error left places untold");
                        self.filled.clear();
                        self.shown_first.clear();
                    }
                    Effect::Place(_) | Effect::Absent(_) => {}
                }
            }
        }

        /// Takes one reply from the read thread, checks that it is owed,
        /// and gives it to the core.
        fn deliver(&mut self, reply: Reply) {
            let answered = match &reply {
                Reply::Opened {
                    serial,
                    view,
                    strip,
                } => {
                    if let Some(strip) = strip {
                        assert_eq!(strip.thickness.len(), view.count as usize);
                    }
                    self.readers = Some((
                        view.token,
                        view.count,
                        view.first.iter().map(|r| r.id.clone()).collect(),
                    ));
                    if *serial == self.latest_open_serial {
                        self.latest_open_outcome = Some(OpenOutcome::Succeeded);
                    }
                    open_key(*serial)
                }
                Reply::Unchanged { serial } => {
                    self.unchanged += 1;
                    if *serial == self.latest_open_serial {
                        self.latest_open_outcome = Some(OpenOutcome::Unchanged);
                    }
                    open_key(*serial)
                }
                Reply::Failed { serial, .. } | Reply::Cancelled { serial } => {
                    if *serial == self.latest_open_serial {
                        self.latest_open_outcome = Some(OpenOutcome::FailedOrCancelled);
                    }
                    open_key(*serial)
                }
                Reply::Located { tag, .. } => locate_key(*tag),
                Reply::NotLocated { tag, why } => {
                    if *why == Why::Unreadable {
                        self.latest_open_outcome = Some(OpenOutcome::UnreadablePlace);
                    }
                    locate_key(*tag)
                }
                Reply::Page { token, page, .. } => page_key(*token, *page),
                Reply::NoPage { token, page } => {
                    self.lost += 1;
                    page_key(*token, *page)
                }
                Reply::Ids { tag, .. } => format!("ids {tag}"),
            };
            match self.owed.get_mut(&answered) {
                Some(count) if *count > 0 => *count -= 1,
                _ => panic!(
                    "step {}: a reply nothing asked for: {answered:?}",
                    self.steps
                ),
            }
            self.replies_seen += 1;
            self.event(Event::Reply(reply));
        }

        fn outstanding(&self) -> u32 {
            self.owed.values().sum()
        }

        /// One thing the reader or the library does.
        fn act(&mut self) {
            self.steps += 1;
            match self.random(14) {
                // Another search or sort, one of which cannot be opened.
                0 | 1 => {
                    let query = match self.random(5) {
                        0 => Query::default(),
                        1 => Query {
                            shelf: false,
                            text: "atlas".into(),
                            sort: vec![],
                        },
                        2 => Query {
                            shelf: true,
                            text: "book 4".into(),
                            sort: vec![SortTerm {
                                field: Field::Author,
                                descending: true,
                            }],
                        },
                        3 => Query {
                            shelf: false,
                            text: "matches nothing".into(),
                            sort: vec![],
                        },
                        _ => {
                            let term = SortTerm {
                                field: Field::Title,
                                descending: false,
                            };
                            Query {
                                shelf: false,
                                text: String::new(),
                                sort: vec![term, term],
                            }
                        }
                    };
                    self.event(Event::Query(query));
                }
                // The store finished a job, which changed a book or did not.
                2 | 3 => {
                    if self.random(2) == 0 {
                        let book = fixture::id(self.random(u64::from(BOOKS)) as u32);
                        let title = format!("Book {} A distant atlas", self.random(4096));
                        self.store
                            .call(move |library| {
                                library.edit(&book, &title, &["Author 1".to_owned()])?;
                                Ok(())
                            })
                            .unwrap();
                    }
                    self.event(Event::Refresh);
                }
                // A place comes on screen and needs its row.
                4..=7 => {
                    let rank = self.random(u64::from(BOOKS) + 40) as u32;
                    let shown = match &self.core.phase {
                        Phase::Showing { view, .. } | Phase::Lost { view, .. } => Some(view.count),
                        _ => None,
                    };
                    // The window only has places for the books it shows,
                    // and asks only for rows it does not hold.
                    if shown.is_some_and(|count| rank < count)
                        && !self.filled.contains(&(rank / PAGE))
                    {
                        self.waiting.push(rank);
                        self.event(Event::Want(rank));
                    }
                }
                8 => {
                    let ids = vec![
                        fixture::id(self.random(u64::from(BOOKS)) as u32),
                        "none".into(),
                    ];
                    self.event(Event::Prune(ids));
                }
                9 => {
                    if self.thread_stopped {
                        self.start_thread();
                    }
                    self.event(Event::Retry);
                }
                // A page leaves the cache, so a later Want asks for it again.
                10 => {
                    if !self.filled.is_empty() {
                        let idx = self.random(self.filled.len() as u64) as usize;
                        self.filled.swap_remove(idx);
                        self.evictions += 1;
                    }
                }
                // Occasionally the read thread stops; a later Retry starts a new one.
                11 if !self.thread_stopped && self.random(12) == 0 => {
                    self.stop_thread();
                }
                // The reader scrolls or moves the focus.
                _ => {
                    let current = fixture::id(self.random(u64::from(BOOKS)) as u32);
                    let top_offset = -(self.random(40) as f32);
                    *self.anchors.borrow_mut() = Anchors {
                        shelf: Some(super::super::ShelfViewport {
                            first: self.random(BOOKS as u64) as u32,
                            capacity: 62,
                        }),
                        shelf_focus: Some(current.clone()),
                        current: Some(current.clone()),
                        top: Some(current),
                        top_offset,
                        focused: true,
                    };
                }
            }
        }

        /// Delivers replies until none is owed. A reply that never comes
        /// fails here and does not hang.
        fn drain(&mut self) {
            let started = Instant::now();
            let mut delivered = 0;
            while self.outstanding() > 0 {
                let Some(replies) = &self.replies else {
                    break;
                };
                delivered += 1;
                assert!(
                    delivered < 10_000,
                    "step {}: replies without end",
                    self.steps
                );
                let reply = loop {
                    match replies.try_recv() {
                        Ok(reply) => break reply,
                        Err(_) => {
                            assert!(
                                started.elapsed() < Duration::from_secs(20),
                                "step {}: {} replies never came: {:?}",
                                self.steps,
                                self.outstanding(),
                                self.owed
                                    .iter()
                                    .filter(|(_, n)| **n > 0)
                                    .collect::<Vec<_>>()
                            );
                            std::thread::sleep(Duration::from_micros(200));
                        }
                    }
                };
                self.deliver(reply);
            }
            if let Some(replies) = &self.replies {
                assert!(
                    replies.is_empty(),
                    "step {}: a reply nothing asked for",
                    self.steps
                );
            }
        }

        /// What must hold once the read thread has nothing left to say.
        fn check(&self) {
            let step = self.steps;
            match &self.core.phase {
                Phase::Showing {
                    view,
                    opening: None,
                } => {
                    assert_eq!(
                        view.query, self.core.query,
                        "step {step}: an older query's books"
                    );
                    let Some((expected_token, expected_count, expected_first)) = &self.readers
                    else {
                        panic!("step {step}: the model shows a view the reader never opened");
                    };
                    assert_eq!(
                        view.token, *expected_token,
                        "step {step}: the model shows a view the reader does not hold"
                    );
                    assert_eq!(
                        view.count, *expected_count,
                        "step {step}: shown book count differs from reader"
                    );
                    assert_eq!(
                        &self.shown_first, expected_first,
                        "step {step}: shown first page differs from reader"
                    );
                }
                Phase::Error { .. } => {
                    assert!(
                        self.shown_first.is_empty(),
                        "step {step}: error phase left first page shown"
                    );
                    assert!(
                        self.thread_stopped
                            || matches!(
                                self.latest_open_outcome,
                                Some(OpenOutcome::FailedOrCancelled | OpenOutcome::UnreadablePlace)
                            ),
                        "step {step}: error shown although newest open succeeded ({:?})",
                        self.latest_open_outcome
                    );
                }
                Phase::Showing { .. } => {
                    panic!("step {step}: an open in flight with no reply owed")
                }
                Phase::Loading { .. } => panic!("step {step}: loading with no reply owed"),
                Phase::Lost { .. } => panic!("step {step}: a lost view with no reply owed"),
            }
            assert!(
                self.waiting.is_empty(),
                "step {step}: places waiting for rows nothing will bring: {:?}",
                self.waiting
            );
        }
    }

    /// A core showing a view the library really gave, with no open in flight.
    fn showing(folder: &std::path::Path) -> (Core, ViewToken) {
        let app = folder.join("app");
        fixture::generate(&app, BOOKS).unwrap();
        let library = ebook_library::Library::open(&app).unwrap();
        let view = library
            .browse_reader()
            .unwrap()
            .open(&Default::default(), &Cancel::default())
            .unwrap();
        let token = view.token;
        let (mut core, _) = Core::new(Query::default());
        let here = || Anchors::default();
        core.step(
            Event::Reply(Reply::Opened {
                serial: 1,
                view,
                strip: None,
            }),
            &here,
        );
        let placed = core.step(
            Event::Reply(Reply::Located {
                tag: 1,
                ranks: vec![None, None],
                pages: vec![],
            }),
            &here,
        );
        assert!(placed.iter().any(|e| matches!(e, Effect::Swap { .. })));
        assert!(matches!(core.phase, Phase::Showing { opening: None, .. }));
        (core, token)
    }

    /// The open an effect list asks for: the view it names and whether it
    /// may be answered `Unchanged`.
    fn asked(effects: &[Effect]) -> Option<(u64, Option<ViewToken>, bool)> {
        effects.iter().find_map(|effect| match effect {
            Effect::Send(Request::Open {
                serial,
                showing,
                refresh,
                ..
            }) => Some((*serial, *showing, *refresh)),
            _ => None,
        })
    }

    #[test]
    fn a_refused_page_of_the_view_on_screen_opens_the_query_again() {
        let folder = tempfile::tempdir().unwrap();
        let (mut core, token) = showing(folder.path());
        let here = || Anchors::default();
        let page = 3;
        let wanted = core.step(Event::Want(page * PAGE), &here);
        assert!(matches!(wanted[..], [Effect::Send(Request::Page { .. })]));
        let refused = core.step(Event::Reply(Reply::NoPage { token, page }), &here);
        // Places are told, and an open that cannot be answered `Unchanged`
        // is on its way.
        assert!(matches!(refused[0], Effect::Gone));
        assert_eq!(
            asked(&refused).map(|(_, showing, refresh)| (showing, refresh)),
            Some((Some(token), false))
        );
        assert!(matches!(core.phase, Phase::Lost { .. }));
        // A place that comes on screen meanwhile is told at once.
        assert!(matches!(
            core.step(Event::Want(5), &here)[..],
            [Effect::Gone]
        ));
    }

    #[test]
    fn a_view_that_cannot_be_read_when_placed_is_an_error_and_not_shown() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        fixture::generate(&app, BOOKS).unwrap();
        let library = ebook_library::Library::open(&app).unwrap();
        let view = library
            .browse_reader()
            .unwrap()
            .open(&Default::default(), &Cancel::default())
            .unwrap();
        let (mut core, _) = Core::new(Query::default());
        let here = || Anchors::default();
        core.step(
            Event::Reply(Reply::Opened {
                serial: 1,
                view,
                strip: None,
            }),
            &here,
        );
        let effects = core.step(
            Event::Reply(Reply::NotLocated {
                tag: 1,
                why: Why::Unreadable,
            }),
            &here,
        );
        assert!(matches!(core.phase, Phase::Error { .. }));
        assert!(effects.iter().any(|e| matches!(e, Effect::Empty)));
        assert!(!effects.iter().any(|e| matches!(e, Effect::Swap { .. })));
    }

    #[test]
    fn a_view_lost_while_a_refresh_is_in_flight_is_not_left_lost() {
        // The page is refused, then the refresh asked for before that is
        // answered `Unchanged`. The view must still be replaced.
        let folder = tempfile::tempdir().unwrap();
        let (mut core, token) = showing(folder.path());
        let here = || Anchors::default();
        let page = 2;
        core.step(Event::Want(page * PAGE), &here);
        let refresh = core.step(Event::Refresh, &here);
        let (serial, showing, may_be_unchanged) = asked(&refresh).unwrap();
        assert_eq!((showing, may_be_unchanged), (Some(token), true));
        let refused = core.step(Event::Reply(Reply::NoPage { token, page }), &here);
        assert!(
            asked(&refused).is_none(),
            "the refresh in flight will answer"
        );
        assert!(matches!(core.phase, Phase::Lost { .. }));
        let unchanged = core.step(Event::Reply(Reply::Unchanged { serial }), &here);
        let again = asked(&unchanged).expect("an open in place of the lost view");
        assert_eq!((again.1, again.2), (Some(token), false));
        assert!(matches!(core.phase, Phase::Lost { .. }));
        // If that open fails there is an error to show, with no rows of
        // the lost view left on screen.
        let failed = core.step(
            Event::Reply(Reply::Failed {
                serial: again.0,
                message: "no".into(),
            }),
            &here,
        );
        assert!(failed.iter().any(|e| matches!(e, Effect::Empty)));
        assert!(matches!(&core.phase, Phase::Error { message } if message == "no"));
        // Try Again, or a change to the catalogue, opens it afresh.
        let retried = core.step(Event::Retry, &here);
        assert_eq!(
            asked(&retried).map(|(_, showing, refresh)| (showing, refresh)),
            Some((None, false))
        );
        assert!(matches!(core.phase, Phase::Loading { .. }));
    }

    #[test]
    fn a_reader_who_moved_is_placed_once_more_and_then_left_alone() {
        let folder = tempfile::tempdir().unwrap();
        let (mut core, _) = showing(folder.path());
        let at = RefCell::new(Anchors::default());
        let here = || at.borrow().clone();
        let moved = |to: f32| Anchors {
            top_offset: to,
            ..Default::default()
        };
        let another = |core: &mut Core, event, here: &dyn Fn() -> Anchors| {
            let library = ebook_library::Library::open(&folder.path().join("app")).unwrap();
            let view = library
                .browse_reader()
                .unwrap()
                .open(&Default::default(), &Cancel::default())
                .unwrap();
            let serial = asked(&core.step(event, here)).unwrap().0;
            let located = core.step(
                Event::Reply(Reply::Opened {
                    serial,
                    view,
                    strip: None,
                }),
                here,
            );
            match located[..] {
                [Effect::Send(Request::Locate { tag, .. })] => tag,
                _ => panic!("a view that opened asks where the reader is"),
            }
        };
        let found = |tag| {
            Event::Reply(Reply::Located {
                tag,
                ranks: vec![Some(4), Some(9)],
                pages: vec![],
            })
        };
        // The reader does not move: they are placed with the swap.
        let tag = another(&mut core, Event::Refresh, &here);
        let effects = core.step(found(tag), &here);
        assert!(effects.iter().any(|e| matches!(e, Effect::Place(_))));
        // They scroll while their place is being found: the stale place
        // is not applied, and where they are now is asked for.
        let tag = another(&mut core, Event::Refresh, &here);
        *at.borrow_mut() = moved(-10.0);
        let effects = core.step(found(tag), &here);
        assert!(!effects.iter().any(|e| matches!(e, Effect::Place(_))));
        let follow = effects
            .iter()
            .find_map(|e| match e {
                Effect::Send(Request::Locate { tag, .. }) => Some(*tag),
                _ => None,
            })
            .expect("one more look for the reader's place");
        // The new view is on screen, and they do not move again.
        *at.borrow_mut() = moved(0.0);
        core.step(Event::Swapped, &here);
        let effects = core.step(found(follow), &here);
        assert!(matches!(&effects[..], [Effect::Place(placed)] if placed.anchors == moved(-10.0)));
        // The same, but they move again before the answer: nothing is
        // applied and nothing more is asked.
        let tag = another(&mut core, Event::Layout(Query::default()), &here);
        *at.borrow_mut() = moved(-10.0);
        let effects = core.step(found(tag), &here);
        let follow = effects
            .iter()
            .find_map(|e| match e {
                Effect::Send(Request::Locate { tag, .. }) => Some(*tag),
                _ => None,
            })
            .unwrap();
        *at.borrow_mut() = moved(0.0);
        core.step(Event::Swapped, &here);
        *at.borrow_mut() = moved(-25.0);
        assert!(core.step(found(follow), &here).is_empty());
        let tag = another(
            &mut core,
            Event::Query(Query {
                sort: vec![ebook_library::SortTerm {
                    field: ebook_library::Field::Author,
                    descending: false,
                }],
                ..Default::default()
            }),
            &here,
        );
        assert!(core.step(found(tag), &here).iter().any(|effect| matches!(
            effect,
            Effect::Place(placed) if placed.reason == Placement::Command
        )));
    }

    #[test]
    fn the_model_and_the_read_thread_always_settle() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        fixture::generate(&app, BOOKS).unwrap();
        let started = Instant::now();
        let mut harness = Harness::new(&app, 0x9e37_79b9_7f4a_7c15);
        harness.drain();
        harness.check();
        for _ in 0..2000 {
            // A burst of what the reader and the library do, with the read
            // thread's replies taken at any point between.
            for _ in 0..1 + harness.random(9) {
                if harness.random(3) == 0 {
                    if let Some(reply) = harness.replies.as_ref().and_then(|r| r.try_recv().ok()) {
                        harness.deliver(reply);
                    }
                } else {
                    harness.act();
                }
            }
            harness.drain();
            harness.check();
        }
        eprintln!(
            "{} steps and {} replies in {:.1} s, {} Unchanged, {} refused pages, {} stops, {} evictions",
            harness.steps,
            harness.replies_seen,
            started.elapsed().as_secs_f64(),
            harness.unchanged,
            harness.lost,
            harness.stops,
            harness.evictions
        );
        assert!(
            harness.steps > 5000
                && harness.unchanged > 50
                && harness.lost > 20
                && harness.stops > 5
                && harness.evictions > 20,
            "the run was too thin"
        );
    }
}

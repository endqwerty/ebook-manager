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
use super::worker::{Reply, Request, Why};
use super::{Anchors, Placed, Query};
use ebook_library::{BrowseRow, Cancel, OpenView, ViewToken};
use std::collections::{HashMap, HashSet};

/// What can happen to the model.
pub enum Event {
    /// The reader asked for another search or sort.
    Query(Query),
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
        background: bool,
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
    background: bool,
    /// The view it made, waiting for the reader's place to be found in it.
    arrived: Option<(OpenView, Anchors)>,
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
        background: bool,
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
        core.open(false, &mut effects);
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
    fn open(&mut self, background: bool, effects: &mut Vec<Effect>) {
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
            background,
            arrived: None,
        };
        let phase = std::mem::replace(
            &mut self.phase,
            Phase::Error {
                message: String::new(),
            },
        );
        let (showing, refresh) = match &phase {
            Phase::Showing { view, .. } => (Some(view.token), background),
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
        self.locating.insert(self.tag, why);
        effects.push(Effect::Send(Request::Locate {
            tag: self.tag,
            token,
            ids,
        }));
    }

    fn place_ids(anchors: &Anchors) -> Vec<String> {
        vec![
            anchors.current.clone().unwrap_or_default(),
            anchors.top.clone().unwrap_or_default(),
        ]
    }

    /// The open in flight has been answered. Runs the refresh that was put
    /// off while it was.
    fn settle(&mut self, effects: &mut Vec<Effect>) {
        if std::mem::take(&mut self.dirty)
            && matches!(self.phase, Phase::Showing { opening: None, .. })
        {
            self.open(true, effects);
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
                    self.open(false, &mut effects);
                }
            }
            Event::Retry => {
                if matches!(self.phase, Phase::Error { .. }) {
                    self.open(false, &mut effects);
                }
            }
            Event::Refresh => match &self.phase {
                Phase::Showing { opening: None, .. } => self.open(true, &mut effects),
                // The open in flight may have read the catalogue before
                // the change. A refresh follows it.
                Phase::Showing { .. } | Phase::Loading { .. } | Phase::Lost { .. } => {
                    self.dirty = true
                }
                // The change may be what was wrong.
                Phase::Error { .. } => self.open(false, &mut effects),
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
            Reply::Opened { serial, view } => {
                if !self.is_opening(serial) {
                    return;
                }
                // Its place is found before it is shown, so the window
                // does not show the top of it first.
                let now = anchors();
                let token = view.token;
                let ids = Self::place_ids(&now);
                if let Some(opening) = self.opening() {
                    opening.arrived = Some((view, now));
                }
                self.locate(token, ids, Locating::Place { serial }, effects);
            }
            Reply::Located { tag, ranks } => self.located(tag, Ok(ranks), anchors, effects),
            Reply::NotLocated { tag, why } => self.located(tag, Err(why), anchors, effects),
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
                    Phase::Lost { .. } => self.open(false, effects),
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
                        self.open(false, effects);
                        self.lose();
                    }
                    other => self.phase = other,
                }
            }
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
                    let background = self.opening().is_some_and(|o| o.background);
                    self.open(background, effects);
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
                let Some((view, asked)) = opening.arrived else {
                    self.fail("The view arrived without its books.".into(), effects);
                    return;
                };
                let token = view.token;
                let now = anchors();
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
                    view,
                    background: opening.background,
                });
                match ranks {
                    Ok(ranks) if now == asked => effects.push(Effect::Place(Placed {
                        anchors: asked,
                        current: ranks.first().copied().flatten(),
                        top: ranks.get(1).copied().flatten(),
                        background: opening.background,
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
                            background: opening.background,
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
                background,
            }) => {
                if let Ok(ranks) = ranks
                    && self.showing(token)
                    && since.is_some_and(|since| anchors() == since)
                {
                    effects.push(Effect::Place(Placed {
                        anchors: was,
                        current: ranks.first().copied().flatten(),
                        top: ranks.get(1).copied().flatten(),
                        background,
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

    /// The model's core, the real read thread, and a stand-in for the
    /// window: places waiting for rows, and where the reader is.
    struct Harness {
        core: Core,
        store: Store,
        requests: mpsc::Sender<Request>,
        replies: async_channel::Receiver<Reply>,
        /// How many replies each request sent is still owed.
        owed: HashMap<String, u32>,
        /// Ranks a place is waiting for a row at.
        waiting: Vec<u32>,
        /// Pages whose rows the window holds.
        filled: Vec<u32>,
        /// Where the reader is, which the steps move about.
        anchors: RefCell<Anchors>,
        /// The view the reader last gave, whoever it was for.
        readers: Option<ViewToken>,
        steps: u64,
        replies_seen: u64,
        unchanged: u64,
        lost: u64,
        seed: u64,
    }

    impl Harness {
        fn new(app: &std::path::Path, seed: u64) -> Self {
            let (requests, received) = mpsc::channel();
            let (send, replies) = async_channel::unbounded();
            let store = Store::open(app.to_owned(), || {});
            let for_thread = store.clone();
            std::thread::spawn(move || crate::browse::worker::serve(for_thread, received, send));
            let (core, effects) = Core::new(Query::default());
            let mut harness = Self {
                core,
                store,
                requests,
                replies,
                owed: HashMap::new(),
                waiting: vec![],
                filled: vec![],
                anchors: RefCell::new(Anchors::default()),
                readers: None,
                steps: 0,
                replies_seen: 0,
                unchanged: 0,
                lost: 0,
                seed,
            };
            harness.perform(effects);
            harness
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
                        let sent = match &request {
                            Request::Open { serial, .. } => open_key(*serial),
                            Request::Locate { tag, .. } => locate_key(*tag),
                            Request::Page { token, page } => page_key(*token, *page),
                        };
                        *self.owed.entry(sent).or_default() += 1;
                        self.requests.send(request).unwrap();
                    }
                    // Every waiting place is told.
                    Effect::Gone => self.waiting.clear(),
                    Effect::Swap { .. } => {
                        assert!(self.waiting.is_empty(), "a swap left places untold");
                        self.filled = vec![0];
                        self.event(Event::Swapped);
                    }
                    Effect::Fill { page, .. } => {
                        self.filled.push(page);
                        self.waiting.retain(|rank| rank / PAGE != page);
                    }
                    Effect::Empty => {
                        assert!(self.waiting.is_empty(), "an error left places untold");
                        self.filled.clear();
                    }
                    Effect::Place(_) | Effect::Absent(_) => {}
                }
            }
        }

        /// Takes one reply from the read thread, checks that it is owed,
        /// and gives it to the core.
        fn deliver(&mut self, reply: Reply) {
            let answered = match &reply {
                Reply::Opened { serial, view } => {
                    self.readers = Some(view.token);
                    open_key(*serial)
                }
                Reply::Unchanged { serial } => {
                    self.unchanged += 1;
                    open_key(*serial)
                }
                Reply::Failed { serial, .. } | Reply::Cancelled { serial } => open_key(*serial),
                Reply::Located { tag, .. } | Reply::NotLocated { tag, .. } => locate_key(*tag),
                Reply::Page { token, page, .. } => page_key(*token, *page),
                Reply::NoPage { token, page } => {
                    self.lost += 1;
                    page_key(*token, *page)
                }
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
            match self.random(12) {
                // Another search or sort, one of which cannot be opened.
                0 | 1 => {
                    let query = match self.random(5) {
                        0 => Query::default(),
                        1 => Query {
                            text: "atlas".into(),
                            sort: vec![],
                        },
                        2 => Query {
                            text: "book 4".into(),
                            sort: vec![SortTerm {
                                field: Field::Author,
                                descending: true,
                            }],
                        },
                        3 => Query {
                            text: "matches nothing".into(),
                            sort: vec![],
                        },
                        _ => {
                            let term = SortTerm {
                                field: Field::Title,
                                descending: false,
                            };
                            Query {
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
                9 => self.event(Event::Retry),
                // The reader scrolls or moves the focus.
                _ => {
                    let current = fixture::id(self.random(u64::from(BOOKS)) as u32);
                    let top_offset = -(self.random(40) as f32);
                    *self.anchors.borrow_mut() = Anchors {
                        current: Some(current),
                        top: None,
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
                delivered += 1;
                assert!(
                    delivered < 10_000,
                    "step {}: replies without end",
                    self.steps
                );
                let reply = loop {
                    match self.replies.try_recv() {
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
            assert!(
                self.replies.is_empty(),
                "step {}: a reply nothing asked for",
                self.steps
            );
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
                    assert_eq!(
                        Some(view.token),
                        self.readers,
                        "step {step}: the model shows a view the reader does not hold"
                    );
                }
                Phase::Error { .. } => {}
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
        core.step(Event::Reply(Reply::Opened { serial: 1, view }), &here);
        let placed = core.step(
            Event::Reply(Reply::Located {
                tag: 1,
                ranks: vec![None, None],
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
        let another = |core: &mut Core, here: &dyn Fn() -> Anchors| {
            let library = ebook_library::Library::open(&folder.path().join("app")).unwrap();
            let view = library
                .browse_reader()
                .unwrap()
                .open(&Default::default(), &Cancel::default())
                .unwrap();
            let serial = asked(&core.step(Event::Refresh, here)).unwrap().0;
            let located = core.step(Event::Reply(Reply::Opened { serial, view }), here);
            match located[..] {
                [Effect::Send(Request::Locate { tag, .. })] => tag,
                _ => panic!("a view that opened asks where the reader is"),
            }
        };
        let found = |tag| {
            Event::Reply(Reply::Located {
                tag,
                ranks: vec![Some(4), Some(9)],
            })
        };
        // The reader does not move: they are placed with the swap.
        let tag = another(&mut core, &here);
        let effects = core.step(found(tag), &here);
        assert!(effects.iter().any(|e| matches!(e, Effect::Place(_))));
        // They scroll while their place is being found: the stale place
        // is not applied, and where they are now is asked for.
        let tag = another(&mut core, &here);
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
        let tag = another(&mut core, &here);
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
                    if let Ok(reply) = harness.replies.try_recv() {
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
            "{} steps and {} replies in {:.1} s, {} of them Unchanged and {} refused pages",
            harness.steps,
            harness.replies_seen,
            started.elapsed().as_secs_f64(),
            harness.unchanged,
            harness.lost
        );
        assert!(
            harness.steps > 5000 && harness.unchanged > 50 && harness.lost > 20,
            "the run was too thin"
        );
    }
}

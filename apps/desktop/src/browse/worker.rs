//! The read thread. It owns the `BrowseReader` and answers the model's
//! requests in order: one reply to every request, with what was asked for
//! or with the reason it cannot be given. It never drops one. What a reason
//! means is the model's to decide.
use super::{PAGE, Query, ShelfViewport};
use crate::store::Store;
use ebook_library::{
    BrowseQuery, BrowseReader, BrowseRow, Cancel, Cancelled, CatalogueReplaced, OpenView,
    SortOrder, StaleView, ViewToken,
};
use std::collections::BTreeSet;
use std::sync::mpsc;

pub type Pages = Vec<(u32, Vec<BrowseRow>)>;

pub enum Request {
    Open {
        serial: u64,
        query: Query,
        cancel: Cancel,
        /// The view the model is showing, if any.
        showing: Option<ViewToken>,
        /// The model asks only because the catalogue may have changed. If
        /// the reader's view is the one the model is showing and nothing
        /// has changed, the answer is `Unchanged`.
        refresh: bool,
    },
    /// Where these books are in a view. `tag` comes back with the answer.
    Locate {
        tag: u64,
        token: ViewToken,
        ids: Vec<String>,
        shelf: Option<ShelfViewport>,
    },
    Page {
        token: ViewToken,
        page: u32,
    },
}

/// Why the books asked for could not be found in a view.
#[derive(Debug, PartialEq)]
pub enum Why {
    /// The view is not the reader's any more: a later open replaced it.
    NotCurrent,
    /// The view could not be read.
    Unreadable,
}

/// One for every request, whatever became of it.
pub enum Reply {
    Opened {
        serial: u64,
        view: OpenView,
        strip: Option<std::sync::Arc<ebook_library::ShelfStrip>>,
    },
    /// The view the model named as the one it is showing is the reader's
    /// own, and the catalogue has not changed since it opened. Never said
    /// of any other view.
    Unchanged {
        serial: u64,
    },
    /// The open failed. The reader's view is the one it had before.
    Failed {
        serial: u64,
        message: String,
    },
    /// A newer open made this one pointless before it finished.
    Cancelled {
        serial: u64,
    },
    Located {
        tag: u64,
        ranks: Vec<Option<u32>>,
        pages: Pages,
    },
    NotLocated {
        tag: u64,
        why: Why,
    },
    Page {
        token: ViewToken,
        page: u32,
        rows: Vec<BrowseRow>,
    },
    /// The page cannot be given: the view is not the reader's any more, or
    /// it could not be read. Either way the view has no more rows to give.
    NoPage {
        token: ViewToken,
        page: u32,
    },
}

/// The library's reader, made when first needed and made again when the
/// catalogue file it was reading is replaced.
struct Reader {
    store: Store,
    reader: Option<BrowseReader>,
    /// The reader's own view: what it was opened for, its token and count.
    held: Option<(Query, ViewToken, u32)>,
}

impl Reader {
    fn run<T>(
        &mut self,
        mut call: impl FnMut(&mut BrowseReader, &Option<(Query, ViewToken, u32)>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        for attempt in 0..2 {
            if self.reader.is_none() {
                self.reader = Some(self.store.call(|library| library.browse_reader())?);
                self.held = None;
            }
            let reader = self.reader.as_mut().expect("the reader was just made");
            match call(reader, &self.held) {
                Err(e) if e.is::<CatalogueReplaced>() => {
                    self.reader = None;
                    self.held = None;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
                done => return done,
            }
        }
        unreachable!("the second attempt returns")
    }
}

fn open(reader: &mut BrowseReader, query: &Query, cancel: &Cancel) -> anyhow::Result<OpenView> {
    let query = BrowseQuery {
        text: query.text.clone(),
        sort: if query.sort.is_empty() {
            SortOrder::default()
        } else {
            SortOrder::new(query.sort.clone())?
        },
    };
    reader.open(&query, cancel)
}

fn why(error: Option<anyhow::Error>) -> Why {
    match error {
        Some(e) if e.is::<StaleView>() => Why::NotCurrent,
        _ => Why::Unreadable,
    }
}

/// Answers each request with exactly one reply, until either channel closes.
pub fn serve(
    store: Store,
    requests: mpsc::Receiver<Request>,
    replies: async_channel::Sender<Reply>,
) {
    let mut reader = Reader {
        store,
        reader: None,
        held: None,
    };
    let mut cached_strip = None;
    for request in requests {
        let reply = match request {
            Request::Open {
                serial,
                query,
                cancel,
                showing,
                refresh,
            } => {
                let opened = reader.run(|reader, held| {
                    let fresh = held
                        .as_ref()
                        .filter(|(asked, ..)| *asked == query)
                        .map(|(_, token, count)| (*token, *count));
                    match fresh {
                        Some((token, count)) if !reader.changed()? => {
                            if refresh && showing == Some(token) {
                                return Ok(None);
                            }
                            // The reader holds this very view, and the
                            // model is not showing it. It is given again.
                            let first = reader.window(token, 0, PAGE)?;
                            Ok(Some(OpenView {
                                token,
                                count,
                                first,
                            }))
                        }
                        _ => open(reader, &query, &cancel).map(Some),
                    }
                });
                match opened {
                    Ok(Some(view)) => {
                        let strip = if query.shelf {
                            if reader.held.as_ref().map(|held| held.1) != Some(view.token)
                                || cached_strip.is_none()
                            {
                                cached_strip = Some(
                                    reader
                                        .run(|r, _| r.shelf(view.token))
                                        .map(std::sync::Arc::new),
                                );
                            }
                            cached_strip
                                .as_ref()
                                .map(|result| result.as_ref().map(std::sync::Arc::clone))
                        } else {
                            cached_strip = None;
                            None
                        };
                        reader.held = Some((query, view.token, view.count));
                        match strip.transpose() {
                            Ok(strip) => Reply::Opened {
                                serial,
                                view,
                                strip,
                            },
                            Err(e) => {
                                reader.held = None;
                                Reply::Failed {
                                    serial,
                                    message: format!("{e:#}"),
                                }
                            }
                        }
                    }
                    Ok(None) => Reply::Unchanged { serial },
                    Err(e) if e.is::<Cancelled>() => Reply::Cancelled { serial },
                    Err(e) => Reply::Failed {
                        serial,
                        message: format!("{e:#}"),
                    },
                }
            }
            Request::Locate {
                tag,
                token,
                ids,
                shelf,
            } => {
                let located = reader.run(|reader, held| {
                    let ranks = reader.locate(token, &ids)?;
                    let mut pages = Vec::new();
                    if let Some(viewport) = shelf {
                        let count = held.as_ref().map_or(0, |v| v.2);
                        let top = ranks.get(1).copied().flatten().unwrap_or(viewport.first);
                        // The old scroll offset can paint once before placement. Load
                        // both windows, including a row on either side of the anchor.
                        let mut needed = BTreeSet::new();
                        for first in [viewport.first, top] {
                            let start = first.saturating_sub(viewport.capacity);
                            let end = first.saturating_add(viewport.capacity).min(count);
                            if start < end {
                                needed.extend(start / PAGE..=(end - 1) / PAGE);
                            }
                        }
                        needed.extend(ranks.get(2).copied().flatten().map(|r| r / PAGE));
                        needed.remove(&0); // OpenView already includes it.
                        for page in needed {
                            pages.push((page, reader.window(token, page * PAGE, PAGE)?));
                        }
                    }
                    Ok((ranks, pages))
                });
                match located {
                    Ok((ranks, pages)) => Reply::Located { tag, ranks, pages },
                    Err(e) => Reply::NotLocated {
                        tag,
                        why: why(Some(e)),
                    },
                }
            }
            Request::Page { token, page } => {
                match reader
                    .reader
                    .as_mut()
                    .map(|reader| reader.window(token, page * PAGE, PAGE))
                {
                    Some(Ok(rows)) => Reply::Page { token, page, rows },
                    _ => Reply::NoPage { token, page },
                }
            }
        };
        if replies.send_blocking(reply).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebook_library::{ImportChoice, Library, ParsedMetadata};
    use std::path::Path;

    struct Thread {
        requests: mpsc::Sender<Request>,
        replies: async_channel::Receiver<Reply>,
        serial: u64,
    }

    fn add(app: &Path, title: &str) {
        let staged = app.with_extension("stage");
        std::fs::write(&staged, title).unwrap();
        let facts = ParsedMetadata {
            title: title.into(),
            ..Default::default()
        };
        Library::open(app)
            .unwrap()
            .import(&staged, &staged, &facts, ImportChoice::KeepSeparate)
            .unwrap();
    }

    impl Thread {
        fn start(app: &Path) -> Self {
            let (requests, received) = mpsc::channel();
            let (send, replies) = async_channel::unbounded();
            let store = Store::open(app.to_owned(), || {});
            std::thread::spawn(move || serve(store, received, send));
            Self {
                requests,
                replies,
                serial: 0,
            }
        }
        fn ask(&mut self, query: Query, cancel: Cancel) -> Reply {
            self.serial += 1;
            self.requests
                .send(Request::Open {
                    serial: self.serial,
                    query,
                    cancel,
                    showing: None,
                    refresh: false,
                })
                .unwrap();
            self.replies.recv_blocking().unwrap()
        }
        fn open(&mut self) -> Reply {
            self.ask(Query::default(), Cancel::default())
        }
        /// An open that fails: a sort cannot name a field twice.
        fn open_badly(&mut self) -> Reply {
            let term = ebook_library::SortTerm {
                field: ebook_library::Field::Title,
                descending: false,
            };
            let query = Query {
                shelf: false,
                text: String::new(),
                sort: vec![term, term],
            };
            self.ask(query, Cancel::default())
        }
        fn page(&self, token: ViewToken) -> Reply {
            self.requests
                .send(Request::Page { token, page: 0 })
                .unwrap();
            self.replies.recv_blocking().unwrap()
        }
        fn locate(&self, token: ViewToken, ids: Vec<String>) -> Reply {
            self.requests
                .send(Request::Locate {
                    tag: 7,
                    token,
                    ids,
                    shelf: None,
                })
                .unwrap();
            self.replies.recv_blocking().unwrap()
        }
    }

    fn opened(reply: Reply) -> OpenView {
        match reply {
            Reply::Opened { view, .. } => view,
            Reply::Failed { message, .. } => panic!("open failed: {message}"),
            _ => panic!("expected a view"),
        }
    }

    #[test]
    fn a_page_of_a_view_that_can_no_longer_be_read_is_answered_and_says_so() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        let view = opened(thread.open());
        assert!(matches!(thread.page(view.token), Reply::Page { rows, .. } if rows.len() == 1));
        // The catalogue file goes, as when it is being rebuilt.
        std::fs::rename(app.join("library.sqlite"), app.join("aside")).unwrap();
        assert!(
            matches!(thread.page(view.token), Reply::NoPage { token, .. } if token == view.token),
            "the page request went unanswered or was served"
        );
        assert!(matches!(
            thread.locate(view.token, vec![]),
            Reply::NotLocated {
                why: Why::Unreadable,
                ..
            }
        ));
    }

    #[test]
    fn an_open_after_the_catalogue_file_is_replaced_reads_the_new_file() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        assert_eq!(opened(thread.open()).count, 1);
        // The write thread's library is opened lazily and holds the lock,
        // so the replacement is a second catalogue swapped into place.
        let other = folder.path().join("other");
        add(&other, "Second");
        add(&other, "Third");
        std::fs::rename(other.join("library.sqlite"), app.join("library.sqlite")).unwrap();
        let view = opened(thread.open());
        assert_eq!(view.count, 2);
        assert_eq!(view.first[0].title, "Second");
    }

    #[test]
    fn a_page_of_a_view_replaced_behind_a_failed_open_is_answered() {
        // The window shows A. B opens and is replaced before the window
        // takes it, and C fails to open, so the window still shows A. The
        // reader's view is B: a page of A is not served, and is answered.
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        let a = opened(thread.open());
        let other = Query {
            shelf: false,
            text: "first".into(),
            sort: vec![],
        };
        let b = opened(thread.ask(other, Cancel::default()));
        assert_ne!(a.token, b.token);
        assert!(matches!(thread.open_badly(), Reply::Failed { .. }));
        assert!(matches!(thread.page(a.token), Reply::NoPage { .. }));
        assert!(matches!(
            thread.locate(a.token, vec![]),
            Reply::NotLocated {
                tag: 7,
                why: Why::NotCurrent
            }
        ));
        // The failed open left the reader's own view readable.
        assert!(matches!(thread.page(b.token), Reply::Page { .. }));
        assert!(matches!(
            thread.locate(b.token, vec![b.first[0].id.clone(), "missing".into()]),
            Reply::Located { tag: 7, ranks, .. } if ranks == [Some(0), None]
        ));
    }

    #[test]
    fn unchanged_is_only_ever_said_of_the_view_the_model_names() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        let a = opened(thread.open());
        let refresh = |thread: &mut Thread, showing| {
            thread.serial += 1;
            thread
                .requests
                .send(Request::Open {
                    serial: thread.serial,
                    query: Query::default(),
                    cancel: Cancel::default(),
                    showing,
                    refresh: true,
                })
                .unwrap();
            thread.replies.recv_blocking().unwrap()
        };
        // The model shows the reader's own view and nothing has changed.
        assert!(matches!(
            refresh(&mut thread, Some(a.token)),
            Reply::Unchanged { .. }
        ));
        // The model shows nothing, or shows some other view: it is given
        // the reader's view, which is the same one while nothing changed.
        assert_eq!(opened(refresh(&mut thread, None)).token, a.token);
        let other = Query {
            shelf: false,
            text: "first".into(),
            sort: vec![],
        };
        let b = opened(thread.ask(other, Cancel::default()));
        // The reader now holds B. A model still showing A asks whether A
        // changed, and is not told it is unchanged.
        let again = opened(refresh(&mut thread, Some(a.token)));
        assert_ne!(again.token, b.token);
        assert_eq!(again.count, 1);
    }

    #[test]
    fn every_request_gets_exactly_one_reply_whatever_came_before() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        let mut tokens = vec![];
        // A fixed seed, so a failure can be run again.
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = |below: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % below
        };
        for step in 0..400 {
            match next(5) {
                0 => tokens.push(opened(thread.open()).token),
                1 => assert!(
                    matches!(thread.open_badly(), Reply::Failed { .. }),
                    "step {step}"
                ),
                2 => {
                    let cancel = Cancel::default();
                    cancel.cancel();
                    assert!(
                        // Cancelled, unless the reader already holds the
                        // view asked for and gives it again.
                        matches!(
                            thread.ask(Query::default(), cancel),
                            Reply::Cancelled { .. } | Reply::Opened { .. }
                        ),
                        "step {step}"
                    );
                }
                _ if tokens.is_empty() => {}
                kind => {
                    // Any view ever opened: the last is the reader's, and
                    // every earlier one is not.
                    let token = tokens[next(tokens.len() as u64) as usize];
                    let current = token == *tokens.last().unwrap();
                    let reply = if kind == 3 {
                        thread.page(token)
                    } else {
                        thread.locate(token, vec![])
                    };
                    let served = matches!(reply, Reply::Page { .. } | Reply::Located { .. });
                    let refused = matches!(
                        reply,
                        Reply::NoPage { .. }
                            | Reply::NotLocated {
                                why: Why::NotCurrent,
                                ..
                            }
                    );
                    assert!(
                        if current { served } else { refused },
                        "step {step}: a request for a {} view",
                        if current { "current" } else { "replaced" }
                    );
                }
            }
            // One reply to each request, and no more.
            assert!(thread.replies.is_empty(), "step {step}: a second reply");
        }
    }
}

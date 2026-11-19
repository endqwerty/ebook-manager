//! The read thread. It owns the `BrowseReader` and answers the model's
//! requests in order: one reply to every request, with what was asked for
//! or with the reason it cannot be given. It never drops one. What a reason
//! means is the model's to decide.
use super::{PAGE, Query};
use crate::store::Store;
use ebook_library::{
    BrowseQuery, BrowseReader, BrowseRow, Cancel, Cancelled, CatalogueReplaced, OpenView,
    SortOrder, StaleView, ViewToken,
};
use std::sync::mpsc;

pub enum Request {
    Open {
        serial: u64,
        query: Query,
        cancel: Cancel,
        /// Answer `Unchanged` when the open view still matches the catalogue.
        only_if_changed: bool,
    },
    /// Where these books are in a view. `tag` comes back with the answer.
    Locate {
        tag: u64,
        token: ViewToken,
        ids: Vec<String>,
    },
    Page {
        token: ViewToken,
        page: u32,
    },
}

/// Why a view's rows or ranks could not be given.
#[derive(Debug)]
pub enum Why {
    /// The view is not the reader's any more: a later open replaced it.
    NotCurrent,
    /// The view could not be read, and the text says why.
    Failed(String),
}

/// One for every request, whatever became of it.
pub enum Reply {
    Opened {
        serial: u64,
        view: OpenView,
    },
    Unchanged {
        serial: u64,
    },
    /// The open failed. The reader's view is the one it had before.
    Failed {
        serial: u64,
        message: String,
    },
    /// A newer open made this one pointless before it finished.
    Cancelled,
    Located {
        tag: u64,
        ranks: Vec<Option<u32>>,
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
    NoPage {
        token: ViewToken,
        why: Why,
    },
}

/// The library's reader, made when first needed and made again when the
/// catalogue file it was reading is replaced.
struct Reader {
    store: Store,
    reader: Option<BrowseReader>,
}

impl Reader {
    fn run<T>(
        &mut self,
        mut call: impl FnMut(&mut BrowseReader) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        for attempt in 0..2 {
            if self.reader.is_none() {
                self.reader = Some(self.store.call(|library| library.browse_reader())?);
            }
            let reader = self.reader.as_mut().expect("the reader was just made");
            match call(reader) {
                Err(e) if e.is::<CatalogueReplaced>() => {
                    self.reader = None;
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
        Some(e) => Why::Failed(format!("{e:#}")),
        None => Why::Failed("The library is not open.".into()),
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
    };
    for request in requests {
        let reply = match request {
            Request::Open {
                serial,
                query,
                cancel,
                only_if_changed,
            } => {
                let opened = reader.run(|reader| {
                    if only_if_changed && !reader.changed()? {
                        return Ok(None);
                    }
                    open(reader, &query, &cancel).map(Some)
                });
                match opened {
                    Ok(Some(view)) => Reply::Opened { serial, view },
                    Ok(None) => Reply::Unchanged { serial },
                    Err(e) if e.is::<Cancelled>() => Reply::Cancelled,
                    Err(e) => Reply::Failed {
                        serial,
                        message: format!("{e:#}"),
                    },
                }
            }
            Request::Locate { tag, token, ids } => {
                match reader
                    .reader
                    .as_mut()
                    .map(|reader| reader.locate(token, &ids))
                {
                    Some(Ok(ranks)) => Reply::Located { tag, ranks },
                    failed => Reply::NotLocated {
                        tag,
                        why: why(failed.and_then(Result::err)),
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
                    failed => Reply::NoPage {
                        token,
                        why: why(failed.and_then(Result::err)),
                    },
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
                    only_if_changed: false,
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
                .send(Request::Locate { tag: 7, token, ids })
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
        match thread.page(view.token) {
            Reply::NoPage {
                token,
                why: Why::Failed(message),
                ..
            } => {
                assert_eq!(token, view.token);
                assert!(!message.is_empty());
            }
            _ => panic!("the page request went unanswered or was served"),
        }
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
        let b = opened(thread.open());
        assert!(matches!(thread.open_badly(), Reply::Failed { .. }));
        assert!(matches!(
            thread.page(a.token),
            Reply::NoPage {
                why: Why::NotCurrent,
                ..
            }
        ));
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
            Reply::Located { tag: 7, ranks } if ranks == [Some(0), None]
        ));
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
                        matches!(thread.ask(Query::default(), cancel), Reply::Cancelled),
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
                        Reply::NoPage {
                            why: Why::NotCurrent,
                            ..
                        } | Reply::NotLocated {
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

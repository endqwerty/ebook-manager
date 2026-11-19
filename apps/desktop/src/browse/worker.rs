//! The read thread. It owns the `BrowseReader` and answers the model's
//! requests in order. Every request gets one answer, except one that a
//! newer open has made pointless, whose answer is the newer open's.
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
    /// Where these books are in the view the open with this serial made.
    Locate {
        serial: u64,
        token: ViewToken,
        ids: Vec<String>,
    },
    Page {
        token: ViewToken,
        page: u32,
    },
}

pub enum Reply {
    Opened {
        serial: u64,
        view: OpenView,
    },
    Located {
        serial: u64,
        ranks: Vec<Option<u32>>,
    },
    Unchanged {
        serial: u64,
    },
    /// The open failed. The view before it, if any, is still readable.
    Failed {
        serial: u64,
        message: String,
    },
    Page {
        token: ViewToken,
        page: u32,
        rows: Vec<BrowseRow>,
    },
    /// The view can no longer be read, and the text says why.
    Lost {
        token: ViewToken,
        message: String,
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
                    // A newer open replaced this one, and its reply follows.
                    Err(e) if e.is::<Cancelled>() => continue,
                    Err(e) => Reply::Failed {
                        serial,
                        message: format!("{e:#}"),
                    },
                }
            }
            Request::Locate { serial, token, ids } => {
                match reader
                    .reader
                    .as_mut()
                    .map(|reader| reader.locate(token, &ids))
                {
                    Some(Ok(ranks)) => Reply::Located { serial, ranks },
                    // A newer open replaced the view, and its reply follows.
                    Some(Err(e)) if e.is::<StaleView>() => continue,
                    // The view is shown without its books found.
                    Some(Err(e)) if !e.is::<CatalogueReplaced>() => Reply::Located {
                        serial,
                        ranks: vec![None; ids.len()],
                    },
                    lost => Reply::Lost {
                        token,
                        message: lost_message(lost.and_then(Result::err)),
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
                    // A newer open replaced the view, and its reply follows.
                    Some(Err(e)) if e.is::<StaleView>() => continue,
                    lost => Reply::Lost {
                        token,
                        message: lost_message(lost.and_then(Result::err)),
                    },
                }
            }
        };
        if replies.send_blocking(reply).is_err() {
            return;
        }
    }
}

fn lost_message(error: Option<anyhow::Error>) -> String {
    error.map_or_else(|| "The library is not open.".into(), |e| format!("{e:#}"))
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
        fn open(&mut self) -> Reply {
            self.serial += 1;
            self.requests
                .send(Request::Open {
                    serial: self.serial,
                    query: Query::default(),
                    cancel: Cancel::default(),
                    only_if_changed: false,
                })
                .unwrap();
            self.replies.recv_blocking().unwrap()
        }
        fn page(&self, token: ViewToken) -> Reply {
            self.requests
                .send(Request::Page { token, page: 0 })
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
            Reply::Lost { token, message } => {
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
    fn locating_in_a_view_a_newer_open_replaced_is_left_to_the_newer_open() {
        let folder = tempfile::tempdir().unwrap();
        let app = folder.path().join("app");
        add(&app, "First");
        let mut thread = Thread::start(&app);
        let old = opened(thread.open());
        let new = opened(thread.open());
        thread
            .requests
            .send(Request::Locate {
                serial: 1,
                token: old.token,
                ids: vec![old.first[0].id.clone()],
            })
            .unwrap();
        thread
            .requests
            .send(Request::Locate {
                serial: 2,
                token: new.token,
                ids: vec![new.first[0].id.clone(), "missing".into()],
            })
            .unwrap();
        // The only answer is the one for the newer view.
        match thread.replies.recv_blocking().unwrap() {
            Reply::Located { serial, ranks } => {
                assert_eq!((serial, ranks), (2, vec![Some(0), None]));
            }
            _ => panic!("expected the newer view's books"),
        }
    }
}

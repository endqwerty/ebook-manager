//! The thumbnail cache: one PNG per cover under the app data folder, named
//! by the digest the book record stores. The folder is a cache and any file
//! in it can be made again from the managed EPUB.
use crate::store::Store;
use anyhow::Result;
use ebook_library::{CoverFacts, SurveyFacts};
use ebook_reader::{Reader, ReaderConfig, Survey, Thumbnail};
use gtk::{gdk, glib};
use gtk4 as gtk;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
};

/// The survey this build asks the helper for.
pub const SURVEY_VERSION: u32 = 1;

pub fn folder(app_data: &Path) -> PathBuf {
    app_data.join("cache/thumbs")
}

pub fn file(folder: &Path, digest: &str) -> PathBuf {
    folder.join(format!("{digest}.png"))
}

/// SHA-256 of the width and height as big-endian 32-bit numbers, then the pixels.
pub fn digest(thumbnail: &Thumbnail) -> String {
    let mut hash = Sha256::new();
    hash.update(thumbnail.width().to_be_bytes());
    hash.update(thumbnail.height().to_be_bytes());
    hash.update(thumbnail.pixels());
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What the book record keeps of a survey.
pub fn facts(survey: &Survey) -> SurveyFacts {
    SurveyFacts {
        version: SURVEY_VERSION,
        words: survey.words,
        cover: survey.cover.as_ref().map(|c| CoverFacts {
            digest: digest(&c.thumbnail),
            average: c.average.to_u32(),
            spine: c.spine.to_u32(),
        }),
    }
}

/// Writes the thumbnail as `<digest>.png` through a temporary file and a
/// rename, so a reader of the folder never sees half a file.
pub fn write(folder: &Path, digest: &str, thumbnail: &Thumbnail) -> Result<()> {
    std::fs::create_dir_all(folder)?;
    let texture = gdk::MemoryTexture::new(
        thumbnail.width() as i32,
        thumbnail.height() as i32,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from(thumbnail.pixels()),
        thumbnail.width() as usize * 3,
    );
    let mut staged = tempfile::NamedTempFile::new_in(folder)?;
    staged.write_all(&gdk::prelude::TextureExt::save_to_png_bytes(&texture))?;
    staged.persist(file(folder, digest))?;
    Ok(())
}

/// Asks the helper about one managed EPUB.
fn survey(store: &Store, id: &str) -> Result<Survey> {
    let id = id.to_owned();
    let epub = store.call(move |library| library.managed_file(&id))?;
    Ok(Reader::new(ReaderConfig::default())?.survey(&epub)?)
}

enum Job {
    /// Make this book's thumbnail again: its ID and the digest on record.
    Remake(String, String),
    /// Survey the books that have no survey yet.
    Unsurveyed,
}

enum Event {
    Loaded(String, Option<gdk::Texture>),
    Remade(String, bool),
}

/// The survey thread. It makes missing thumbnails again when asked, and
/// between those surveys the books that lack a survey, one at a time. A book
/// whose survey fails is left unmarked and tried at the next start.
fn work(
    store: Store,
    folder: PathBuf,
    jobs: mpsc::Receiver<Job>,
    events: async_channel::Sender<Event>,
) {
    let mut surveying = false;
    let mut failed = HashSet::new();
    let mut queue = VecDeque::new();
    loop {
        let job = if surveying {
            match jobs.try_recv() {
                Ok(job) => Some(job),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match jobs.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            }
        };
        match job {
            Some(Job::Remake(id, wanted)) => {
                // Installed only when it is the cover the record names.
                let made = survey(&store, &id).is_ok_and(|survey| {
                    survey.cover.is_some_and(|cover| {
                        digest(&cover.thumbnail) == wanted
                            && write(&folder, &wanted, &cover.thumbnail).is_ok()
                    })
                });
                if events.send_blocking(Event::Remade(wanted, made)).is_err() {
                    return;
                }
            }
            Some(Job::Unsurveyed) => surveying = true,
            None => {
                if queue.is_empty() {
                    let limit = 16 + failed.len() as u32;
                    queue = store
                        .call(move |library| library.unsurveyed(limit))
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|book| !failed.contains(&book.id))
                        .collect();
                }
                let Some(book) = queue.pop_front() else {
                    surveying = false;
                    continue;
                };
                let stored = survey(&store, &book.id).and_then(|survey| {
                    let found = facts(&survey);
                    if let (Some(cover), Some(surveyed)) = (&found.cover, &survey.cover) {
                        let _ = write(&folder, &cover.digest, &surveyed.thumbnail);
                    }
                    let (id, version) = (book.id.clone(), book.version_id.clone());
                    store.call(move |library| library.set_survey(&id, &version, found))
                });
                if stored.is_err() {
                    failed.insert(book.id);
                }
            }
        }
    }
}

type Arrived = Box<dyn Fn(&gdk::Texture)>;

struct Shared {
    loads: mpsc::Sender<String>,
    jobs: mpsc::Sender<Job>,
    textures: RefCell<HashMap<String, gdk::Texture>>,
    /// Digests of the cached textures, oldest first.
    order: RefCell<VecDeque<String>>,
    /// Who wants each digest that has not arrived: a book that has it, and
    /// what to call.
    waiting: RefCell<HashMap<String, (String, Vec<Arrived>)>>,
    remade: RefCell<HashSet<String>>,
    /// Digests that could not be loaded or made again in this run.
    hopeless: RefCell<HashSet<String>>,
}

/// Cover images for the GTK thread. Files are read on a loader thread, and
/// a missing one is made again on the survey thread.
#[derive(Clone)]
pub struct Thumbs(Rc<Shared>);

const CACHED_TEXTURES: usize = 192;

impl Thumbs {
    pub fn new(store: &Store, app_data: &Path) -> Self {
        let folder = folder(app_data);
        let (loads, wanted) = mpsc::channel::<String>();
        let (jobs, received) = mpsc::channel();
        let (send, events) = async_channel::unbounded();
        let (loader_folder, loaded) = (folder.clone(), send.clone());
        std::thread::spawn(move || {
            for digest in wanted {
                let texture = gdk::Texture::from_filename(file(&loader_folder, &digest)).ok();
                if loaded
                    .send_blocking(Event::Loaded(digest, texture))
                    .is_err()
                {
                    return;
                }
            }
        });
        let store = store.clone();
        std::thread::spawn(move || work(store, folder, received, send));
        let thumbs = Self(Rc::new(Shared {
            loads,
            jobs,
            textures: RefCell::default(),
            order: RefCell::default(),
            waiting: RefCell::default(),
            remade: RefCell::default(),
            hopeless: RefCell::default(),
        }));
        let weak = Rc::downgrade(&thumbs.0);
        glib::spawn_future_local(async move {
            while let Ok(event) = events.recv().await {
                let Some(shared) = weak.upgrade() else { break };
                Thumbs(shared).handle(event);
            }
        });
        thumbs
    }

    /// Starts the low-priority survey of books that have none.
    pub fn survey_unsurveyed(&self) {
        let _ = self.0.jobs.send(Job::Unsurveyed);
    }

    pub fn get(&self, digest: &str) -> Option<gdk::Texture> {
        self.0.textures.borrow().get(digest).cloned()
    }

    /// Calls `arrived` when the image is in memory, which may be never.
    /// `book` is a book whose cover it is, in case the file has to be made.
    pub fn want(&self, digest: &str, book: &str, arrived: impl Fn(&gdk::Texture) + 'static) {
        if self.0.hopeless.borrow().contains(digest) {
            return;
        }
        let mut waiting = self.0.waiting.borrow_mut();
        let first = !waiting.contains_key(digest);
        waiting
            .entry(digest.to_owned())
            .or_insert_with(|| (book.to_owned(), vec![]))
            .1
            .push(Box::new(arrived));
        if first {
            let _ = self.0.loads.send(digest.to_owned());
        }
    }

    fn handle(&self, event: Event) {
        let shared = &self.0;
        match event {
            Event::Loaded(digest, Some(texture)) => {
                shared
                    .textures
                    .borrow_mut()
                    .insert(digest.clone(), texture.clone());
                let mut order = shared.order.borrow_mut();
                order.push_back(digest.clone());
                while order.len() > CACHED_TEXTURES {
                    if let Some(old) = order.pop_front() {
                        shared.textures.borrow_mut().remove(&old);
                    }
                }
                drop(order);
                let waiters = shared.waiting.borrow_mut().remove(&digest);
                for arrived in waiters.into_iter().flat_map(|(_, waiters)| waiters) {
                    arrived(&texture);
                }
            }
            Event::Loaded(digest, None) => {
                let book = shared
                    .waiting
                    .borrow()
                    .get(&digest)
                    .map(|(book, _)| book.clone());
                match book {
                    Some(book) if shared.remade.borrow_mut().insert(digest.clone()) => {
                        let _ = shared.jobs.send(Job::Remake(book, digest));
                    }
                    _ => self.give_up(digest),
                }
            }
            Event::Remade(digest, true) => {
                let _ = shared.loads.send(digest);
            }
            Event::Remade(digest, false) => self.give_up(digest),
        }
    }

    fn give_up(&self, digest: String) {
        self.0.waiting.borrow_mut().remove(&digest);
        self.0.hopeless.borrow_mut().insert(digest);
    }
}

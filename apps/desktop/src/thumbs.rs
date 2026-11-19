//! The thumbnail cache: one PNG per cover under the app data folder, named
//! by the digest the book record stores. The folder is a cache and any file
//! in it can be made again from the managed EPUB.
use crate::store::Store;
use anyhow::Result;
use ebook_library::{CoverFacts, SurveyFacts};
use ebook_reader::{Reader, ReaderConfig, Survey, Thumbnail};
use gtk::{gdk, gdk::prelude::*, glib};
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

/// A cover as rows of RGB pixels.
#[derive(Clone, Copy)]
pub struct Pixels<'a> {
    pub width: u32,
    pub height: u32,
    pub rgb: &'a [u8],
}

impl<'a> From<&'a Thumbnail> for Pixels<'a> {
    fn from(thumbnail: &'a Thumbnail) -> Self {
        Self {
            width: thumbnail.width(),
            height: thumbnail.height(),
            rgb: thumbnail.pixels(),
        }
    }
}

/// SHA-256 of the width and height as big-endian 32-bit numbers, then the pixels.
pub fn digest(cover: Pixels) -> String {
    let mut hash = Sha256::new();
    hash.update(cover.width.to_be_bytes());
    hash.update(cover.height.to_be_bytes());
    hash.update(cover.rgb);
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What the book record keeps of a survey.
pub fn facts(survey: &Survey) -> SurveyFacts {
    SurveyFacts {
        version: SURVEY_VERSION,
        words: survey.words,
        cover: survey.cover.as_ref().map(|c| CoverFacts {
            digest: digest((&c.thumbnail).into()),
            average: c.average.to_u32(),
            spine: c.spine.to_u32(),
        }),
    }
}

/// Writes the thumbnail as `<digest>.png` through a temporary file and a
/// rename, so a reader of the folder never sees half a file.
pub fn write(folder: &Path, digest: &str, cover: Pixels) -> Result<()> {
    std::fs::create_dir_all(folder)?;
    let texture = gdk::MemoryTexture::new(
        cover.width as i32,
        cover.height as i32,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from(cover.rgb),
        cover.width as usize * 3,
    );
    let mut staged = tempfile::NamedTempFile::new_in(folder)?;
    staged.write_all(&texture.save_to_png_bytes())?;
    staged.persist(file(folder, digest))?;
    Ok(())
}

/// No thumbnail file is larger: 256 by 384 pixels of RGB, and room for a
/// PNG that compresses badly.
const LARGEST_FILE: u64 = 512 * 1024;

/// The width and height a PNG's header gives, if it is the one kind of
/// file `write` makes: eight bits a channel, red, green and blue with no
/// alpha and no palette, not interlaced, and no larger than a thumbnail.
/// Read from the first bytes, before anything is decoded.
fn header(png: &[u8]) -> Option<(u32, u32)> {
    let rest = png.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
    // The first chunk: a length of 13, `IHDR`, then the 13 bytes.
    let chunk = rest.get(..8 + 13)?;
    if chunk[..8] != *b"\0\0\0\x0dIHDR" {
        return None;
    }
    let number =
        |at: usize| u32::from_be_bytes([chunk[at], chunk[at + 1], chunk[at + 2], chunk[at + 3]]);
    let (width, height) = (number(8), number(12));
    // Bit depth, colour type, compression, filter, interlace.
    let plain = chunk[16..21] == [8, 2, 0, 0, 0];
    let fits = (1..=Thumbnail::MAX_WIDTH).contains(&width)
        && (1..=Thumbnail::MAX_HEIGHT).contains(&height);
    (plain && fits).then_some((width, height))
}

/// Reads the thumbnail with this digest, if the file there is one. The
/// folder is only a cache, so the file is checked before it is shown: a
/// regular file and not a link, no larger than a thumbnail file can be, a
/// PNG of the one kind this app writes, whose pixels hash to the digest it
/// is named by. The texture is made from those hashed pixels. Anything
/// else counts as missing, and the thumbnail is made again.
pub fn load(folder: &Path, wanted: &str) -> Option<gdk::Texture> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let found = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(file(folder, wanted))
        .ok()?;
    if !found.metadata().ok()?.is_file() {
        return None;
    }
    // One byte past the limit shows a file that is too large, however
    // large it has grown since it was opened.
    let mut bytes = vec![];
    found.take(LARGEST_FILE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > LARGEST_FILE {
        return None;
    }
    let (width, height) = header(&bytes)?;
    let decoded = gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)).ok()?;
    if (decoded.width() as u32, decoded.height() as u32) != (width, height) {
        return None;
    }
    let mut download = gdk::TextureDownloader::new(&decoded);
    download.set_format(gdk::MemoryFormat::R8g8b8);
    let (rows, stride) = download.download_bytes();
    let across = width as usize * 3;
    let rgb: Vec<u8> = rows
        .chunks(stride)
        .take(height as usize)
        .flat_map(|row| &row[..across])
        .copied()
        .collect();
    let cover = Pixels {
        width,
        height,
        rgb: &rgb,
    };
    if digest(cover) != wanted {
        return None;
    }
    // What is shown is what was hashed: opaque pixels, whatever else the
    // file carried.
    Some(
        gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::R8g8b8,
            &glib::Bytes::from_owned(rgb),
            across,
        )
        .upcast(),
    )
}

/// Which books have been tried as the source of each cover that had to be
/// made again.
#[derive(Default)]
struct Repairs(HashMap<String, HashSet<String>>);

impl Repairs {
    /// The books among those asking for a cover that have not been tried,
    /// now counted as tried. `None` when every one of them has been.
    fn next(&mut self, cover: &str, asking: &[String]) -> Option<Vec<String>> {
        let tried = self.0.entry(cover.to_owned()).or_default();
        let fresh: Vec<String> = asking
            .iter()
            .filter(|book| tried.insert((*book).clone()))
            .cloned()
            .collect();
        (!fresh.is_empty()).then_some(fresh)
    }
    fn tried(&self, cover: &str, book: &str) -> bool {
        self.0.get(cover).is_some_and(|tried| tried.contains(book))
    }
    /// The cover has loaded, so its books are sources again if it goes.
    fn forget(&mut self, cover: &str) {
        self.0.remove(cover);
    }
}

/// Makes the thumbnail with this digest again from the first of `books`
/// whose EPUB still yields that cover. `cover_of` surveys one book.
fn remake<C: AsRef<[u8]>>(
    folder: &Path,
    wanted: &str,
    books: &[String],
    cover_of: impl Fn(&str) -> Option<(u32, u32, C)>,
) -> bool {
    books.iter().any(|book| {
        cover_of(book).is_some_and(|(width, height, rgb)| {
            let cover = Pixels {
                width,
                height,
                rgb: rgb.as_ref(),
            };
            // Installed only when it is the cover the record names.
            digest(cover) == wanted && write(folder, wanted, cover).is_ok()
        })
    })
}

/// Asks the helper about one managed EPUB.
fn survey(store: &Store, id: &str) -> Result<Survey> {
    let id = id.to_owned();
    let epub = store.call(move |library| library.managed_file(&id))?;
    Ok(Reader::new(ReaderConfig::default())?.survey(&epub)?)
}

enum Job {
    /// Make a thumbnail again: the books that have it and the digest on
    /// record.
    Remake(Vec<String>, String),
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
            Some(Job::Remake(books, wanted)) => {
                let made = remake(&folder, &wanted, &books, |book| {
                    let cover = survey(&store, book).ok()?.cover?.thumbnail;
                    Some((cover.width(), cover.height(), cover.pixels().to_vec()))
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
                        let _ = write(&folder, &cover.digest, (&surveyed.thumbnail).into());
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
/// The books that asked for a cover, and what to call when it arrives.
type Wanted = (Vec<String>, Vec<Arrived>);

struct Shared {
    loads: mpsc::Sender<String>,
    jobs: mpsc::Sender<Job>,
    textures: RefCell<HashMap<String, gdk::Texture>>,
    /// Digests of the cached textures, oldest first.
    order: RefCell<VecDeque<String>>,
    /// Who wants each digest that has not arrived: every book that asked,
    /// any of which can supply it, and what to call.
    waiting: RefCell<HashMap<String, Wanted>>,
    repairs: RefCell<Repairs>,
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
                let texture = load(&loader_folder, &digest);
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
            repairs: RefCell::default(),
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
    /// Two books can share a cover, and each that asks is remembered.
    pub fn want(&self, digest: &str, book: &str, arrived: impl Fn(&gdk::Texture) + 'static) {
        let mut waiting = self.0.waiting.borrow_mut();
        let first = !waiting.contains_key(digest);
        // This book's file has been tried for this cover and did not give
        // it. Another book that has the cover still can.
        if first && self.0.repairs.borrow().tried(digest, book) {
            return;
        }
        let wanted = waiting.entry(digest.to_owned()).or_default();
        if !wanted.0.iter().any(|asked| asked == book) {
            wanted.0.push(book.to_owned());
        }
        wanted.1.push(Box::new(arrived));
        if first {
            let _ = self.0.loads.send(digest.to_owned());
        }
    }

    fn handle(&self, event: Event) {
        let shared = &self.0;
        match event {
            Event::Loaded(digest, Some(texture)) => {
                shared.repairs.borrow_mut().forget(&digest);
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
            // The file is missing or is not the thumbnail, or the books
            // tried could not supply it. Each book that has asked, up to
            // this moment, is tried before the cover is given up.
            Event::Loaded(digest, None) | Event::Remade(digest, false) => {
                let asking = shared
                    .waiting
                    .borrow()
                    .get(&digest)
                    .map(|(books, _)| books.clone())
                    .unwrap_or_default();
                let fresh = shared.repairs.borrow_mut().next(&digest, &asking);
                match fresh {
                    Some(books) => {
                        let _ = shared.jobs.send(Job::Remake(books, digest));
                    }
                    None => {
                        shared.waiting.borrow_mut().remove(&digest);
                    }
                }
            }
            Event::Remade(digest, true) => {
                let _ = shared.loads.send(digest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four by two pixels, each a different colour.
    fn cover() -> (u32, u32, Vec<u8>) {
        (4, 2, (0..24).map(|n| n * 10).collect())
    }

    fn pixels(cover: &(u32, u32, Vec<u8>)) -> Pixels<'_> {
        Pixels {
            width: cover.0,
            height: cover.1,
            rgb: &cover.2,
        }
    }

    #[test]
    fn a_written_thumbnail_loads_with_the_pixels_it_was_given() {
        let folder = tempfile::tempdir().unwrap();
        let cover = cover();
        let name = digest(pixels(&cover));
        write(folder.path(), &name, pixels(&cover)).unwrap();
        let texture = load(folder.path(), &name).expect("the thumbnail just written");
        assert_eq!((texture.width(), texture.height()), (4, 2));
    }

    #[test]
    fn a_file_that_is_not_the_named_thumbnail_counts_as_missing() {
        let folder = tempfile::tempdir().unwrap();
        let cover = cover();
        let name = digest(pixels(&cover));
        assert!(load(folder.path(), &name).is_none(), "no file");
        // Another picture under this name.
        let mut other = cover.clone();
        other.2[5] ^= 0xff;
        write(folder.path(), &name, pixels(&other)).unwrap();
        assert!(load(folder.path(), &name).is_none(), "other pixels");
        // Not a PNG.
        std::fs::write(file(folder.path(), &name), b"GIF89a not a thumbnail").unwrap();
        assert!(load(folder.path(), &name).is_none(), "not a PNG");
        // A link to the right picture, which is not followed.
        let elsewhere = tempfile::tempdir().unwrap();
        write(elsewhere.path(), &name, pixels(&cover)).unwrap();
        std::fs::remove_file(file(folder.path(), &name)).unwrap();
        std::os::unix::fs::symlink(file(elsewhere.path(), &name), file(folder.path(), &name))
            .unwrap();
        assert!(load(folder.path(), &name).is_none(), "a link");
        assert!(load(elsewhere.path(), &name).is_some());
    }

    #[test]
    fn only_the_kind_of_png_this_app_writes_is_read() {
        let folder = tempfile::tempdir().unwrap();
        let cover = cover();
        let name = digest(pixels(&cover));
        write(folder.path(), &name, pixels(&cover)).unwrap();
        let written = std::fs::read(file(folder.path(), &name)).unwrap();
        assert_eq!(header(&written), Some((4, 2)));
        // Byte 24 is the bit depth, 25 the colour type, 28 the interlace.
        for (at, value) in [(24, 16), (25, 6), (25, 3), (25, 0), (28, 1)] {
            let mut other = written.clone();
            other[at] = value;
            assert_eq!(header(&other), None, "byte {at} set to {value}");
        }
        assert_eq!(header(&written[..20]), None, "cut short");
        assert_eq!(header(b"GIF89a"), None);
        // A picture with an alpha channel whose colours are the cover's.
        // Shown as it is, the window would show through it.
        let clear: Vec<u8> = cover
            .2
            .chunks(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 0])
            .collect();
        let texture = gdk::MemoryTexture::new(
            4,
            2,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(clear),
            16,
        );
        std::fs::write(file(folder.path(), &name), texture.save_to_png_bytes()).unwrap();
        assert!(load(folder.path(), &name).is_none(), "alpha");
    }

    #[test]
    fn a_file_past_the_size_limit_is_not_read_to_its_end() {
        let folder = tempfile::tempdir().unwrap();
        let cover = cover();
        let name = digest(pixels(&cover));
        write(folder.path(), &name, pixels(&cover)).unwrap();
        // The right picture, with more after it than a thumbnail can be.
        let mut bytes = std::fs::read(file(folder.path(), &name)).unwrap();
        bytes.resize(LARGEST_FILE as usize + 1, 0);
        std::fs::write(file(folder.path(), &name), bytes).unwrap();
        assert!(load(folder.path(), &name).is_none());
    }

    #[test]
    fn a_book_that_asks_after_a_repair_began_is_tried_before_giving_up() {
        let mut repairs = Repairs::default();
        let first = ["one".to_owned()];
        assert_eq!(repairs.next("cover", &first), Some(first.to_vec()));
        // That book's file could not be read. Meanwhile another book with
        // the same cover came into view.
        let both = ["one".to_owned(), "two".to_owned()];
        assert_eq!(repairs.next("cover", &both), Some(vec!["two".to_owned()]));
        // Both have failed: the cover is given up, for those two books.
        assert_eq!(repairs.next("cover", &both), None);
        assert!(repairs.tried("cover", "two") && !repairs.tried("cover", "three"));
        assert!(!repairs.tried("other", "one"));
        // Once the cover loads, its books can be asked again if it goes.
        repairs.forget("cover");
        assert_eq!(repairs.next("cover", &first), Some(first.to_vec()));
    }

    #[test]
    fn a_picture_larger_than_a_thumbnail_counts_as_missing() {
        let folder = tempfile::tempdir().unwrap();
        let wide = (257, 1, vec![7; 257 * 3]);
        let name = digest(pixels(&wide));
        write(folder.path(), &name, pixels(&wide)).unwrap();
        assert!(load(folder.path(), &name).is_none());
    }

    #[test]
    fn a_shared_cover_is_made_again_from_any_book_that_has_it() {
        let folder = tempfile::tempdir().unwrap();
        let cover = cover();
        let name = digest(pixels(&cover));
        let books = [
            "unreadable".to_owned(),
            "changed".to_owned(),
            "good".to_owned(),
        ];
        let made = remake(folder.path(), &name, &books, |book| match book {
            // One book's file cannot be read, and another now has a
            // different cover.
            "unreadable" => None,
            "changed" => Some((1, 1, vec![1, 2, 3])),
            _ => Some(cover.clone()),
        });
        assert!(made);
        assert!(load(folder.path(), &name).is_some());
        // With no book that still has it, nothing is installed.
        let other = tempfile::tempdir().unwrap();
        assert!(!remake(other.path(), &name, &books[..2], |_| None::<(
            u32,
            u32,
            Vec<u8>
        )>));
        assert!(load(other.path(), &name).is_none());
    }
}

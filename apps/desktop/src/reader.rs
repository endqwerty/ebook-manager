//! The reader's window: a web view on one chapter, the chapter controls, the
//! keys, and saving the place. What keeps book content inside the view is in
//! `reader/view.rs`. See docs/design/epub-reading.md.

mod place;
mod progress;
mod view;

use super::*;
use ebook_library::ReadingPosition;
use ebook_reader::{Reader, ReaderConfig, ResourceId};
use progress::Weight;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::Arc,
};
use view::{Config, Document, Event, Landing, Picture, Source, Zone};

/// A page colour from docs/design/interface-reader.md: page, text, secondary.
const PAPER: [&str; 3] = ["#f4ecd8", "#3b2f21", "#736858"];
const GREY: [&str; 3] = ["#2b2b2e", "#d6d6d8", "#929294"];

/// The reader's faces in the order the design prefers them. Literata is the
/// design's default; bundling it is a later task, so it is used when the
/// system has it.
const FACES: [&str; 4] = ["Literata", "Noto Serif", "Liberation Serif", "DejaVu Serif"];

/// The first of `FACES` the system has, as a CSS font list. A name the
/// system lacks is not sent to the page: fontconfig would answer it with
/// its default face, which is often not a serif.
fn reading_font(w: &gtk::Window) -> String {
    let installed = w.pango_context().list_families();
    FACES
        .iter()
        .find(|face| installed.iter().any(|f| f.name() == **face))
        .map_or_else(|| "serif".to_owned(), |face| format!("'{face}', serif"))
}

pub fn open_in(parent: &impl IsA<gtk::Window>, store: &store::Store, b: &Rc<Book>) {
    let w = gtk::Window::builder()
        .title(&b.title)
        .transient_for(parent)
        // Closing the library closes the reader, and so lets its view go.
        .destroy_with_parent(true)
        .modal(true)
        .default_width(760)
        .default_height(640)
        .build();
    w.add_css_class("reader-window");
    let layout = column(0);
    w.set_child(Some(&layout));
    let status = label("Opening book…", "heading");
    inset(&status, 16);
    layout.append(&status);
    w.present();
    let store = store.clone();
    let b = b.clone();
    glib::spawn_future_local(async move {
        let id = b.id.clone();
        let result = async {
            let (path, saved) = store
                .run(move |lib| Ok((lib.managed_file(&id)?, lib.load_position(&id)?)))
                .await?;
            let book = gio::spawn_blocking(move || -> Result<_> {
                Ok(Reader::new(ReaderConfig::default())?.open(&path)?)
            })
            .await
            .map_err(|_| anyhow::anyhow!("Reader interrupted"))??;
            anyhow::ensure!(!book.chapters().is_empty(), "No readable chapters");
            let last = book.chapters().len() as u32 - 1;
            // A book with no saved place opens on its first page, whatever
            // is on it. A saved place opens the page holding its offset.
            let place =
                saved
                    .filter(|p| p.version_id == b.version_id)
                    .map_or((0, Landing::Start), |p| {
                        if p.chapter > last {
                            (last, Landing::Start)
                        } else {
                            (p.chapter, Landing::Offset(p.offset))
                        }
                    });
            Ok::<_, anyhow::Error>((Arc::new(book), place))
        }
        .await;
        let (book, (chapter, landing)) = match result {
            Ok(x) => x,
            Err(e) => {
                status.set_text(&format!("Couldn’t open book · {e:#}"));
                return;
            }
        };
        let facts = Rc::new(RefCell::new(HashMap::new()));
        let source = Rc::new(BookSource {
            book: book.clone(),
            facts: facts.clone(),
        });
        let config = Config::new(
            glib::user_cache_dir()
                .join("ebook-manager")
                .join("content-filters"),
        );
        let (ready, built) = async_channel::bounded(1);
        // The view tells the reader what the page did in the same step the
        // page's message is handled, so the place the reader holds is never
        // behind a message that has arrived.
        let hears: Rc<RefCell<std::rc::Weak<Reading>>> = Rc::default();
        let tells = hears.clone();
        view::View::open(
            config,
            source,
            move |event| {
                let reading = tells.borrow().upgrade();
                if let Some(reading) = reading {
                    reading.event(event);
                }
            },
            move |view| {
                let _ = ready.send_blocking(view);
            },
        );
        let view = match built.recv().await {
            Ok(Ok(view)) => view,
            Ok(Err(e)) => {
                status.set_text(&format!("Couldn’t open book · {e}"));
                return;
            }
            Err(_) => return,
        };
        if !w.is_visible() {
            return;
        }
        layout.remove(&status);
        inset(&status, 0);
        let reading = Reading::build(&w, &layout, status, view, book, facts, b, store);
        hears.replace(Rc::downgrade(&reading));
        reading.load(chapter, landing);
    });
}

/// What the helper said about a chapter beyond its markup.
#[derive(Clone, Copy, Default)]
struct Facts {
    /// The length of the chapter's text, as the helper counted it.
    length: u32,
    right_to_left: bool,
    /// Something in the chapter could not be shown.
    partial: bool,
}

/// Chapters and pictures from the sandboxed helper, read off the GTK thread.
struct BookSource {
    book: Arc<ebook_reader::Book>,
    facts: Rc<RefCell<HashMap<u32, Facts>>>,
}

impl Source for BookSource {
    fn chapter(&self, index: u32, done: Box<dyn FnOnce(Result<Document, String>)>) {
        let book = self.book.clone();
        let facts = self.facts.clone();
        glib::spawn_future_local(async move {
            let read = gio::spawn_blocking(move || book.read_content(index)).await;
            done(match read {
                Ok(Ok(content)) => {
                    facts.borrow_mut().insert(
                        index,
                        Facts {
                            length: content.length,
                            right_to_left: content.right_to_left,
                            partial: content.truncated || !content.warnings.is_empty(),
                        },
                    );
                    Ok(Document {
                        markup: content.markup,
                        language: content.language,
                        right_to_left: content.right_to_left,
                    })
                }
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("Chapter loading interrupted".into()),
            })
        });
    }

    fn picture(&self, id: u32, done: Box<dyn FnOnce(Option<Picture>)>) {
        let book = self.book.clone();
        glib::spawn_future_local(async move {
            let read = gio::spawn_blocking(move || book.read_resource(ResourceId(id))).await;
            // A picture that cannot be read shows its text alternative.
            done(read.ok().and_then(Result::ok).map(|resource| Picture {
                media_type: resource.kind.media_type(),
                bytes: resource.bytes,
            }))
        });
    }
}

fn toggle_full_screen(w: &gtk::Window) {
    if w.is_fullscreen() {
        w.unfullscreen()
    } else {
        w.fullscreen()
    }
}

fn position_error(w: &gtk::Window, e: &anyhow::Error) {
    if let Some(detail) = publication_message(e) {
        notice(w, "Position saved locally", &detail);
    } else {
        notice(w, "Couldn’t save reading position", &e.to_string());
    }
}

/// The page colour for the window's appearance: Paper in light, Grey in dark.
fn page_colour() -> [&'static str; 3] {
    if libadwaita::StyleManager::default().is_dark() {
        GREY
    } else {
        PAPER
    }
}

use place::{Effect, Input, Keeper, Place};

struct Reading {
    book: Arc<ebook_reader::Book>,
    window: glib::WeakRef<gtk::Window>,
    model: Rc<Book>,
    store: store::Store,
    view: view::View,
    facts: Rc<RefCell<HashMap<u32, Facts>>>,
    picker: gtk::DropDown,
    /// Where to land when the picker next changes chapter.
    arrival: RefCell<Option<Landing>>,
    previous: gtk::Button,
    next: gtk::Button,
    status: gtk::Label,
    /// The place to save, the place showing and whether the reader has
    /// left. Everything that happens goes through `step`, and this file
    /// only does what the keeper answers. See `place.rs`.
    keeper: RefCell<Keeper>,
    /// The timer that saves the place once turning settles.
    pending_save: Cell<Option<glib::SourceId>>,
    /// The answer to the save a leave submitted, until the leave reads it.
    submitted: RefCell<Option<Saving>>,
    /// The window's own colours, replaced when the appearance changes.
    colours: gtk::CssProvider,
}

impl Reading {
    #[allow(clippy::too_many_arguments)]
    fn build(
        w: &gtk::Window,
        layout: &gtk::Box,
        status: gtk::Label,
        view: view::View,
        book: Arc<ebook_reader::Book>,
        facts: Rc<RefCell<HashMap<u32, Facts>>>,
        model: Rc<Book>,
        store: store::Store,
    ) -> Rc<Reading> {
        let names: Vec<_> = book
            .chapters()
            .iter()
            .enumerate()
            .map(|(n, c)| {
                c.title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| format!("Chapter {}", n + 1))
            })
            .collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        let picker = gtk::DropDown::from_strings(&names);
        picker.set_hexpand(true);
        let bar = row(8);
        inset(&bar, 12);
        layout.append(&bar);
        let previous = gtk::Button::with_label("Previous");
        let next = gtk::Button::with_label("Next");
        bar.append(&previous);
        bar.append(&picker);
        bar.append(&next);
        let page = view.widget();
        page.set_vexpand(true);
        page.update_property(&[gtk::accessible::Property::Label("Book text")]);
        layout.append(page);
        let footer = row(12);
        inset(&footer, 12);
        layout.append(&footer);
        status.set_hexpand(true);
        footer.append(&status);
        let weak = w.downgrade();
        let full_screen = button("Full screen", move || {
            if let Some(w) = weak.upgrade() {
                toggle_full_screen(&w)
            }
        });
        full_screen.set_tooltip_text(Some("F11"));
        footer.append(&full_screen);
        w.connect_fullscreened_notify(move |w| {
            full_screen.set_label(if w.is_fullscreen() {
                "Leave full screen"
            } else {
                "Full screen"
            })
        });
        let weak = w.downgrade();
        footer.append(&button("Back to library", move || {
            if let Some(w) = weak.upgrade() {
                w.close()
            }
        }));
        let colours = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(w),
            &colours,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        let reading = Rc::new(Reading {
            book,
            window: w.downgrade(),
            model,
            store,
            view,
            facts,
            picker: picker.clone(),
            arrival: RefCell::new(None),
            previous: previous.clone(),
            next: next.clone(),
            status,
            keeper: RefCell::default(),
            pending_save: Cell::new(None),
            submitted: RefCell::new(None),
            colours,
        });
        reading.colour();
        reading.view.set_style(&[("--font", &reading_font(w))]);
        let weak = Rc::downgrade(&reading);
        libadwaita::StyleManager::default().connect_dark_notify(move |_| {
            if let Some(r) = weak.upgrade() {
                r.colour();
            }
        });
        let weak = Rc::downgrade(&reading);
        picker.connect_selected_notify(move |p| {
            if let Some(r) = weak.upgrade() {
                let landing = r.arrival.take().unwrap_or(Landing::Start);
                r.show(p.selected(), landing);
            }
        });
        let weak = Rc::downgrade(&reading);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, key, _, modifiers| match weak.upgrade() {
            Some(r) if r.key(key, modifiers) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        w.add_controller(keys);
        let p = picker.downgrade();
        previous.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.set_selected(p.selected().saturating_sub(1))
            }
        });
        next.connect_clicked(move |_| picker.set_selected(picker.selected() + 1));
        // The window can also go without being asked, when the library
        // closes under it. A place still waiting to be saved is sent then.
        let weak = Rc::downgrade(&reading);
        w.connect_unrealize(move |_| {
            if let Some(r) = weak.upgrade() {
                let _ = r.leave();
            }
        });
        // The window the reader was opened from can close first, and can do
        // so without this window ever being unrealized: what it holds can
        // keep it from being disposed, which is what would close this one.
        // So the reader also leaves when that window is asked to close or
        // goes off screen.
        if let Some(parent) = w.transient_for() {
            let weak = Rc::downgrade(&reading);
            parent.connect_close_request(move |_| {
                if let Some(r) = weak.upgrade() {
                    let _ = r.leave();
                }
                glib::Propagation::Proceed
            });
            let weak = Rc::downgrade(&reading);
            parent.connect_unrealize(move |_| {
                if let Some(r) = weak.upgrade() {
                    let _ = r.leave();
                }
            });
        }
        // The application can also quit with no window being closed, as it
        // does when the session ends.
        if let Some(app) = gio::Application::default() {
            let weak = Rc::downgrade(&reading);
            app.connect_shutdown(move |_| {
                if let Some(r) = weak.upgrade() {
                    let _ = r.leave();
                }
            });
        }
        let r = reading.clone();
        w.connect_close_request(move |w| {
            // Leaving is the same whoever asks: save the place the reader
            // holds, wait for the library to have it, end the page.
            if let Err(e) = r.leave()
                && let Some(parent) = w.transient_for()
            {
                position_error(&parent, &e);
            }
            // The window the reader was opened from may have been destroyed
            // from outside, as when a window manager ends. GTK before 4.24
            // tells that window nothing while this one is modal, so it
            // would stay, unseen, and the app would run on with no window.
            // It goes with the reader, and the app ends with its last window.
            if let Some(parent) = w.transient_for()
                && parent.surface().is_none_or(|s| s.is_destroyed())
            {
                glib::idle_add_local_once(move || parent.destroy());
            }
            glib::Propagation::Proceed
        });
        reading
    }

    /// Give the page and the window the page colour for the appearance.
    fn colour(&self) {
        let [page, ink, secondary] = page_colour();
        self.colours.load_from_string(&format!(
            ".reader-window {{ background-color: {page}; color: {ink}; }}"
        ));
        self.view
            .set_style(&[("--page", page), ("--ink", ink), ("--secondary", secondary)]);
    }

    /// Select a chapter in the picker and show it at `landing`.
    fn load(self: &Rc<Self>, chapter: u32, landing: Landing) {
        if self.picker.selected() == chapter {
            self.show(chapter, landing);
        } else {
            self.arrival.replace(Some(landing));
            self.picker.set_selected(chapter);
        }
    }

    fn show(self: &Rc<Self>, chapter: u32, landing: Landing) {
        if self.keeper.borrow().gone() {
            return;
        }
        self.previous.set_sensitive(chapter > 0);
        self.next
            .set_sensitive((chapter as usize) + 1 < self.book.chapters().len());
        let number = self.view.load(chapter, landing);
        let _ = self.step(Input::Asked { number });
        // A chapter opens in the page colour, with no flash of another.
        let [page, ink, secondary] = page_colour();
        self.view
            .set_style(&[("--page", page), ("--ink", ink), ("--secondary", secondary)]);
    }

    fn event(self: &Rc<Self>, event: Event) {
        // A reader that has left has saved its place and ended its page.
        // Nothing that arrives after that changes anything.
        if self.keeper.borrow().gone() {
            return;
        }
        match event {
            Event::Ready { load } => {
                self.view.widget().grab_focus();
                // The page and the helper count the same text. If they ever
                // differ, places still work, since the page makes and reads
                // its own, but search results would land off. Say so.
                let counted = self.facts.borrow().get(&load.chapter).map(|f| f.length);
                if counted.is_some_and(|c| c != load.length) {
                    glib::g_warning!(
                        "ebook-manager",
                        "chapter {}: the page counts {} units of text, the helper {}",
                        load.chapter,
                        load.length,
                        counted.unwrap_or_default()
                    );
                }
                let _ = self.step(Input::Ready { load: record(load) });
            }
            Event::Place {
                load,
                offset,
                page,
                pages,
                count,
                moved,
            } => {
                let _ = self.step(Input::Report(place::Report {
                    load: record(load),
                    offset,
                    page,
                    pages,
                    count,
                    moved,
                }));
            }
            Event::Jump { chapter, anchor } => {
                if (chapter as usize) < self.book.chapters().len() {
                    self.load(chapter, anchor.map_or(Landing::Start, Landing::Anchor));
                }
            }
            Event::External(address) => self.offer(address),
            Event::Tap(Zone::Forward) => self.turn(1),
            Event::Tap(Zone::Back) => self.turn(-1),
            // The controls stay visible for now, so the middle does nothing.
            Event::Tap(Zone::Middle) => {}
            Event::Failed(why) => {
                let _ = self.step(Input::Failed);
                self.status.set_text(&why);
            }
        }
    }

    /// Tell the keeper what happened and do what it answers, in the order
    /// it answers. The error, if any, is the library's for a save a leave
    /// made.
    fn step(self: &Rc<Self>, input: Input) -> Result<()> {
        // The keeper is let go before anything is done, so an effect that
        // leads back here finds it free.
        let effects = self.keeper.borrow_mut().step(input);
        let mut result = Ok(());
        for effect in effects {
            match effect {
                Effect::Show(place) => self.describe(place),
                Effect::Blank => self.status.set_text("Loading chapter…"),
                Effect::StartTimer => self.start_timer(),
                Effect::StopTimer => {
                    if let Some(timer) = self.pending_save.take() {
                        timer.remove();
                    }
                }
                Effect::Submit(place) => {
                    let saving = submit(&self.store, &self.model.id, self.position(place));
                    if input == Input::Leave {
                        self.submitted.replace(Some(saving));
                    } else {
                        self.report_later(saving);
                    }
                }
                Effect::AwaitLibrary => {
                    result = await_library(&self.store, self.submitted.take());
                }
                Effect::EndPage => self.view.shut(),
            }
        }
        result
    }

    /// The line under the page: the chapter, the page, and a note when some
    /// of the chapter could not be shown.
    fn describe(&self, place: Place) {
        let partial = self
            .facts
            .borrow()
            .get(&place.chapter)
            .is_some_and(|f| f.partial);
        self.status.set_text(&format!(
            "Chapter {} of {} · Page {} of {}{}",
            place.chapter + 1,
            self.book.chapters().len(),
            place.page + 1,
            place.pages,
            if partial {
                " · Some content could not be displayed"
            } else {
                ""
            }
        ));
    }

    /// Ask before an address outside the book goes to the system.
    fn offer(&self, address: String) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let dialog = gtk::AlertDialog::builder()
            .message("Open this link outside the book?")
            .detail(&address)
            .buttons(["Cancel", "Open"])
            .cancel_button(0)
            .default_button(0)
            .modal(true)
            .build();
        let parent = w.clone();
        dialog.choose(Some(&w), gio::Cancellable::NONE, move |choice| {
            if choice == Ok(1) {
                gtk::UriLauncher::new(&address).launch(
                    Some(&parent),
                    gio::Cancellable::NONE,
                    |_| {},
                );
            }
        });
    }

    /// Handles a reader shortcut. The chapter list keeps its own keys, and
    /// Space still presses a focused button.
    fn key(self: &Rc<Self>, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
        use gtk::gdk::{Key, ModifierType};
        let Some(w) = self.window.upgrade() else {
            return false;
        };
        if modifiers.intersects(
            ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK,
        ) {
            return false;
        }
        if key == Key::F11 {
            toggle_full_screen(&w);
            return true;
        }
        let focus = gtk::prelude::GtkWindowExt::focus(&w);
        let in_popover = focus
            .as_ref()
            .is_some_and(|f| f.ancestor(gtk::Popover::static_type()).is_some());
        if key == Key::Escape && w.is_fullscreen() && !in_popover {
            w.unfullscreen();
            return true;
        }
        if focus
            .as_ref()
            .is_some_and(|f| f == &self.picker || f.is_ancestor(&self.picker))
        {
            return false;
        }
        let page = self.view.widget();
        let reading = focus.is_none_or(|f| &f == page || f.is_ancestor(page));
        let back = modifiers.contains(ModifierType::SHIFT_MASK);
        // The side that goes forward follows the book, not the interface.
        let towards_end = if self.right_to_left() { -1 } else { 1 };
        match key {
            Key::Right | Key::KP_Right => self.turn(towards_end),
            Key::Left | Key::KP_Left => self.turn(-towards_end),
            Key::Page_Down | Key::KP_Page_Down => self.turn(1),
            Key::Page_Up | Key::KP_Page_Up => self.turn(-1),
            Key::space if reading && back => self.turn(-1),
            Key::space if reading => self.turn(1),
            _ => return false,
        }
        true
    }

    fn right_to_left(&self) -> bool {
        let chapter = self.keeper.borrow().displayed().map(|(p, _)| p.chapter);
        chapter.is_some_and(|c| self.facts.borrow().get(&c).is_some_and(|f| f.right_to_left))
    }

    /// Turns one page, moving to the neighbouring chapter at either end of
    /// this one: forward to its start, back to its last page.
    fn turn(self: &Rc<Self>, by: i32) {
        // A turn is made from the place showing, in the load showing it.
        let Some((place, load)) = self.keeper.borrow().displayed() else {
            return;
        };
        // The answer must not keep the reader alive after its window closes.
        let weak = Rc::downgrade(self);
        self.view.turn(by, false, move |moved| {
            let Some(reading) = weak.upgrade() else {
                return;
            };
            // A turn that moved has already told its place. One that did
            // not is the chapter's edge only if the same load is still
            // showing: a page ended while a turn was unanswered also says
            // it did not move, and so does one the reader has since left.
            let keeper = reading.keeper.borrow();
            let same = keeper.displayed().is_some_and(|(_, shown)| shown == load) && !keeper.gone();
            drop(keeper);
            if moved || !same {
                return;
            }
            let chapters = reading.book.chapters().len() as u32;
            if by > 0 && place.chapter + 1 < chapters {
                reading.load(place.chapter + 1, Landing::Start);
            } else if by < 0 && place.chapter > 0 {
                reading.load(place.chapter - 1, Landing::End);
            }
        });
    }

    /// How far through the book the place is, weighing chapters by size.
    fn fraction(&self, place: Place) -> f32 {
        let weights: Vec<Weight> = self
            .book
            .chapters()
            .iter()
            .map(|c| Weight {
                size: c.size,
                linear: c.linear,
            })
            .collect();
        progress::fraction(&weights, place.chapter as usize, place.page, place.pages)
    }

    /// The place as the library stores it, with when it was read and how
    /// far through the book it is. This is the one function that knows the
    /// record's shape.
    fn position(&self, place: Place) -> ReadingPosition {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok());
        ReadingPosition {
            version_id: self.model.version_id.clone(),
            chapter: place.chapter,
            offset: place.offset,
            last_read: now,
            fraction: Some(self.fraction(place)),
        }
    }

    /// Start the timer that saves once turning settles, or start it again.
    fn start_timer(self: &Rc<Self>) {
        if let Some(timer) = self.pending_save.take() {
            timer.remove();
        }
        let weak = Rc::downgrade(self);
        let timer =
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                if let Some(r) = weak.upgrade() {
                    // This timer has fired and is gone, so it is forgotten.
                    r.pending_save.take();
                    let _ = r.step(Input::Timer);
                }
            });
        self.pending_save.set(Some(timer));
    }

    /// Tell the reader if a save made while reading failed. The save is
    /// already with the library: this only waits for its answer.
    fn report_later(&self, saving: Saving) {
        let window = self.window.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = saving.await
                && let Some(w) = window.upgrade()
            {
                position_error(&w, &e);
            }
        });
    }

    /// Leave the reader. Every way of leaving does this one thing, whether
    /// it is the reader's own close, its window going off screen, the window
    /// it was opened from closing, or the application quitting: save the
    /// place the reader holds, wait for the library to have it, because the
    /// process may end as soon as this returns, and end the page's process
    /// before anything can leave it to shut itself down. Leaving twice does
    /// nothing the second time.
    fn leave(self: &Rc<Self>) -> Result<()> {
        self.step(Input::Leave)
    }
}

/// A save the library has been handed, and will answer.
type Saving = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>;

fn record(load: view::Load) -> place::Load {
    place::Load {
        number: load.number,
        chapter: load.chapter,
        length: load.length,
    }
}

/// Ask a future once, with nothing to wake.
fn poll_once<T>(
    future: std::pin::Pin<&mut (impl std::future::Future<Output = T> + ?Sized)>,
) -> std::task::Poll<T> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    future.poll(&mut context)
}

/// Hand a place to the library. This is the one way a place is saved, for
/// the timer and for leaving alike.
fn submit(store: &store::Store, id: &str, position: ReadingPosition) -> Saving {
    let store = store.clone();
    let id = id.to_owned();
    queue(async move { store.save_position(id, position).await })
}

/// Put a job in the library's queue now. The store queues a job when its
/// future is first asked, not when it is made, so this asks once. The
/// library's thread takes its queue in order, so jobs are done in the order
/// this is called. What comes back only waits for the library's answer.
fn queue(job: impl std::future::Future<Output = Result<()>> + 'static) -> Saving {
    let mut saving: Saving = Box::pin(job);
    if let std::task::Poll::Ready(answer) = poll_once(saving.as_mut()) {
        return Box::pin(std::future::ready(answer));
    }
    saving
}

/// Wait until the library has written everything submitted so far, or two
/// seconds have passed, and give the answer to `last` if there is one. The
/// process may end as soon as a leave returns, so a leave waits here.
///
/// This can run inside a signal handler, where the main loop cannot be
/// waited on. The library answers from its own thread, so asking again
/// until it has answered is enough. With no save of its own to wait for, a
/// leave queues a job that does nothing: when that is answered, every save
/// queued before it has been written.
fn await_library(store: &store::Store, last: Option<Saving>) -> Result<()> {
    let mut last = last.unwrap_or_else(|| {
        let store = store.clone();
        Box::pin(async move { store.run(|_| Ok(())).await })
    });
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let std::task::Poll::Ready(result) = poll_once(last.as_mut()) {
            return result;
        }
        if std::time::Instant::now() >= limit {
            anyhow::bail!("The library did not answer in time");
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn job(store: &store::Store, log: &Arc<Mutex<Vec<u32>>>, n: u32) -> Saving {
        let (store, log) = (store.clone(), log.clone());
        Box::pin(async move {
            store
                .run(move |_| {
                    log.lock().unwrap().push(n);
                    Ok(())
                })
                .await
        })
    }

    /// A save made by the timer is in the library's queue before the timer
    /// returns, so a leave that follows it at once is written after it,
    /// though nothing ever waits for the timer's save.
    #[test]
    fn a_queued_save_is_written_before_a_later_one_without_being_waited_for() {
        let dir = tempfile::tempdir().unwrap();
        let store = store::Store::open(dir.path().to_owned(), || {});
        let log = Arc::new(Mutex::new(Vec::new()));
        let timed = queue(job(&store, &log, 1));
        let leaving = queue(job(&store, &log, 2));
        await_library(&store, Some(leaving)).unwrap();
        assert_eq!(*log.lock().unwrap(), [1, 2]);
        drop(timed);
        // With no save of its own, a leave still waits for what was queued.
        let _unwaited = queue(job(&store, &log, 3));
        await_library(&store, None).unwrap();
        assert_eq!(*log.lock().unwrap(), [1, 2, 3]);
    }

    /// The fault this replaced: a save that is made and only later asked
    /// lands after one made later. The test above would see it.
    #[test]
    fn a_save_that_is_not_queued_at_once_can_land_late() {
        let dir = tempfile::tempdir().unwrap();
        let store = store::Store::open(dir.path().to_owned(), || {});
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut late = job(&store, &log, 1);
        let leaving = queue(job(&store, &log, 2));
        await_library(&store, Some(leaving)).unwrap();
        assert_eq!(*log.lock().unwrap(), [2]);
        let _ = poll_once(late.as_mut());
        await_library(&store, None).unwrap();
        assert_eq!(*log.lock().unwrap(), [2, 1]);
    }
}

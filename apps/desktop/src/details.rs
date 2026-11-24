//! Book details utility pane (#128): cover, title, plain label-colour author
//! and series navigation links, prominent Read action, Send to device, and
//! leading-aligned facts. Loaded off the GTK thread via `Store`.

use crate::{
    cover::{Cover, Kind},
    store::Store,
    thumbs::Thumbs,
};
use ebook_library::{Book, BrowseRow, ReadingPosition};
use gtk::{glib, prelude::*};
use gtk4 as gtk;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub struct Callbacks {
    pub read: Box<dyn Fn(String)>,
    pub send: Box<dyn Fn(Vec<String>)>,
    pub edit: Box<dyn Fn(String)>,
    pub filter: Box<dyn Fn(String)>,
    pub close: Box<dyn Fn()>,
}

pub struct Details {
    root: gtk::ScrolledWindow,
    body: gtk::Box,
    store: Store,
    thumbs: Thumbs,
    callbacks: Rc<Callbacks>,
    serial: Rc<Cell<u64>>,
    shown: RefCell<Vec<String>>,
}

fn grouped_u64(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_date(unix: i64) -> Option<String> {
    glib::DateTime::from_unix_local(unix)
        .ok()
        .and_then(|dt| dt.format("%-d %B %Y").ok())
        .map(|s| s.to_string())
}

fn nav_button(label: &str, accessible: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    let btn = gtk::Button::with_label(label);
    btn.add_css_class("flat");
    btn.add_css_class("details-link");
    btn.set_halign(gtk::Align::Start);
    btn.update_property(&[gtk::accessible::Property::Label(accessible)]);
    btn.connect_clicked(move |_| on_click());
    btn
}

fn browse_row_from(book: &Book, pos: Option<&ReadingPosition>) -> BrowseRow {
    let cover = book.imported.survey.as_ref().and_then(|s| s.cover.as_ref());
    let series = book
        .series
        .as_ref()
        .unwrap_or(&book.imported.series)
        .first()
        .map(|(n, p)| (n.clone(), p.clone().unwrap_or_default()));
    BrowseRow {
        id: book.id.clone(),
        version_id: book.version_id.clone(),
        title: book.title.clone(),
        authors: book.authors.join(", "),
        author_sort: None,
        series,
        added: book.added,
        published: book.imported.published.clone(),
        last_read: pos.and_then(|p| p.last_read),
        progress: pos.and_then(|p| p.fraction),
        cover: cover.map(|c| ebook_library::CoverRef {
            digest: c.digest.clone(),
        }),
        cover_rgb: cover.map(|c| c.average),
        spine_rgb: cover.map(|c| c.spine),
        right_to_left: book.imported.right_to_left,
        protection: book.imported.protection,
    }
}

impl Details {
    pub fn new(store: Store, thumbs: Thumbs, callbacks: Callbacks) -> Rc<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.set_margin_top(16);
        body.set_margin_bottom(24);
        body.set_margin_start(18);
        body.set_margin_end(18);
        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .child(&body)
            .build();
        root.add_css_class("details-pane");
        Rc::new(Self {
            root,
            body,
            store,
            thumbs,
            callbacks: Rc::new(callbacks),
            serial: Rc::new(Cell::new(0)),
            shown: RefCell::new(Vec::new()),
        })
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    fn clear(&self) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
    }

    pub fn show(self: &Rc<Self>, ids: Vec<String>) {
        if *self.shown.borrow() == ids {
            return;
        }
        self.shown.replace(ids.clone());
        let serial = self.serial.get().wrapping_add(1);
        self.serial.set(serial);
        if ids.is_empty() {
            self.clear();
            let empty = gtk::Label::new(Some("Select a book to see its details."));
            empty.add_css_class("dimmed");
            empty.set_xalign(0.0);
            empty.set_wrap(true);
            self.body.append(&empty);
            return;
        }
        if ids.len() > 1 {
            self.render_multi(ids);
            return;
        }
        let id = ids[0].clone();
        let store = self.store.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = store
                .run(move |lib| {
                    let book = lib.book(&id)?;
                    let pos = lib.load_position(&id).ok().flatten();
                    let size = lib
                        .managed_file(&id)
                        .ok()
                        .and_then(|p| std::fs::metadata(p).ok())
                        .map(|m| m.len());
                    Ok((book, pos, size))
                })
                .await;
            let Some(details) = weak.upgrade() else {
                return;
            };
            if details.serial.get() != serial {
                return;
            }
            match result {
                Ok((book, pos, size)) => details.render_book(&book, pos.as_ref(), size),
                Err(e) => {
                    details.clear();
                    let msg = gtk::Label::new(Some(&format!("Couldn’t load details: {e}")));
                    msg.add_css_class("dimmed");
                    msg.set_xalign(0.0);
                    msg.set_wrap(true);
                    details.body.append(&msg);
                }
            }
        });
    }

    pub fn invalidate(&self) {
        self.shown.borrow_mut().clear();
    }

    fn render_multi(&self, ids: Vec<String>) {
        self.clear();
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some(&format!("{} books", grouped_u64(ids.len() as u64))));
        title.add_css_class("title-1");
        title.add_css_class("screen-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close details"));
        let cb = self.callbacks.clone();
        close.connect_clicked(move |_| (cb.close)());
        top.append(&title);
        top.append(&close);
        self.body.append(&top);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let send = gtk::Button::with_label("Send to device");
        send.add_css_class("pill");
        let cb = self.callbacks.clone();
        send.connect_clicked(move |_| (cb.send)(ids.clone()));
        actions.append(&send);
        self.body.append(&actions);
    }

    fn render_book(&self, book: &Book, pos: Option<&ReadingPosition>, file_size: Option<u64>) {
        self.clear();

        // Top bar with close button.
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close details"));
        let cb = self.callbacks.clone();
        close.connect_clicked(move |_| (cb.close)());
        top.append(&spacer);
        top.append(&close);
        self.body.append(&top);

        // Cover at leading edge, 160 wide.
        let cover = Cover::new(Kind::List, 160, &self.thumbs);
        cover.set_halign(gtk::Align::Start);
        let row = Rc::new(browse_row_from(book, pos));
        cover.set_book(Some(&row));
        self.body.append(&cover);

        // Serif book title (never truncated in Book details).
        let title = gtk::Label::new(Some(&book.title));
        title.add_css_class("title-1");
        title.add_css_class("screen-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        self.body.append(&title);

        // Author button(s): plain label-colour navigation buttons.
        if !book.authors.is_empty() {
            let authors_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
            for author in &book.authors {
                let query = author.clone();
                let cb = self.callbacks.clone();
                let btn = nav_button(author, &format!("Show books by {author}"), move || {
                    (cb.filter)(query.clone());
                });
                authors_box.append(&btn);
            }
            self.body.append(&authors_box);
        }

        // Series link(s), when present.
        let series_list = book.series.as_ref().unwrap_or(&book.imported.series);
        if let Some((name, num)) = series_list.first() {
            let label = match num.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(n) => format!("{name}, {n}"),
                None => name.clone(),
            };
            let query = name.clone();
            let cb = self.callbacks.clone();
            let btn = nav_button(&label, &format!("Show the {name} series"), move || {
                (cb.filter)(query.clone());
            });
            self.body.append(&btn);
        }

        // Prominent action (Read / Continue reading / Read again) + Send to device.
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_margin_top(4);
        actions.set_margin_bottom(6);
        let read_label = match pos.and_then(|p| p.fraction) {
            Some(f) if f >= 0.999 => "Read again",
            Some(f) if f > 0.0 => "Continue reading",
            _ => "Read",
        };
        let read_btn = gtk::Button::with_label(read_label);
        read_btn.add_css_class("pill");
        read_btn.add_css_class("suggested-action");
        {
            let id = book.id.clone();
            let cb = self.callbacks.clone();
            read_btn.connect_clicked(move |_| (cb.read)(id.clone()));
        }
        let send_btn = gtk::Button::with_label("Send to device");
        send_btn.add_css_class("pill");
        {
            let id = book.id.clone();
            let cb = self.callbacks.clone();
            send_btn.connect_clicked(move |_| (cb.send)(vec![id.clone()]));
        }
        let edit_btn = gtk::Button::with_label("Edit");
        edit_btn.add_css_class("pill");
        {
            let id = book.id.clone();
            let cb = self.callbacks.clone();
            edit_btn.connect_clicked(move |_| (cb.edit)(id.clone()));
        }
        actions.append(&read_btn);
        actions.append(&send_btn);
        actions.append(&edit_btn);
        self.body.append(&actions);

        // Two-column facts grid: leading-aligned labels in secondary colour.
        let grid = gtk::Grid::builder()
            .column_spacing(16)
            .row_spacing(6)
            .build();
        let mut r = 0;
        let mut add_fact = |name: &str, value: String, numeric: bool| {
            let k = gtk::Label::new(Some(name));
            k.set_xalign(0.0);
            k.add_css_class("dimmed");
            let v = gtk::Label::new(Some(&value));
            v.set_xalign(0.0);
            v.set_wrap(true);
            v.set_hexpand(true);
            if numeric {
                v.add_css_class("numeric");
            }
            grid.attach(&k, 0, r, 1, 1);
            grid.attach(&v, 1, r, 1, 1);
            r += 1;
        };
        if let Some(published) = book
            .imported
            .published
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            add_fact("Published", published.to_owned(), true);
        }
        if let Some(language) = book
            .imported
            .language
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            add_fact("Language", language.to_owned(), false);
        }
        if let Some(words) = book.imported.survey.as_ref().and_then(|s| s.words) {
            add_fact(
                "Length",
                format!("{} words", grouped_u64(u64::from(words))),
                true,
            );
        }
        let format_val = match file_size {
            Some(sz) => format!("EPUB, {}", format_bytes(sz)),
            None => "EPUB".to_owned(),
        };
        add_fact("Format", format_val, true);
        if let Some(added) = book.added.and_then(format_date) {
            add_fact("Added", added, true);
        }
        self.body.append(&grid);
    }
}

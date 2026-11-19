mod imports;
mod reader;
mod store;

use anyhow::Result;
use ebook_device::ExportOutcome;
use ebook_library::Book;
use gtk::{gio, glib, prelude::*};
use gtk4 as gtk;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone)]
struct Shelf {
    window: gtk::ApplicationWindow,
    store: store::Store,
    rows: gtk::Box,
    search: gtk::SearchEntry,
    count: gtk::Label,
}

fn column(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}
fn row(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}
fn inset(w: &impl IsA<gtk::Widget>, n: i32) {
    w.set_margin_top(n);
    w.set_margin_bottom(n);
    w.set_margin_start(n);
    w.set_margin_end(n);
}
fn label(text: &str, class: &str) -> gtk::Label {
    let w = gtk::Label::new(Some(text));
    w.set_xalign(0.0);
    w.add_css_class(class);
    w.set_wrap(true);
    w
}
fn button(text: &str, f: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.connect_clicked(move |_| f());
    b
}
fn notice(parent: &impl IsA<gtk::Window>, title: &str, detail: &str) {
    gtk::AlertDialog::builder()
        .message(title)
        .detail(detail)
        .build()
        .show(Some(parent));
}
fn publication_message(e: &anyhow::Error) -> Option<String> {
    e.downcast_ref::<ebook_library::PublicationPending>()?;
    Some(format!(
        "{e:#}. When storage is available, choose Refresh library."
    ))
}
fn choose(
    parent: &gtk::ApplicationWindow,
    title: &str,
    folder: bool,
    f: impl Fn(Vec<PathBuf>) + 'static,
) {
    let parent = parent.clone();
    let d = gtk::FileDialog::builder()
        .title(title)
        .accept_label("Choose")
        .build();
    glib::spawn_future_local(async move {
        let result = if folder {
            d.select_folder_future(Some(&parent))
                .await
                .map(|file| file.path().into_iter().collect())
        } else {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("EPUB books"));
            filter.add_suffix("epub");
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            d.set_filters(Some(&filters));
            d.open_multiple_future(Some(&parent)).await.map(|files| {
                files
                    .iter::<gio::File>()
                    .filter_map(|v| v.ok()?.path())
                    .collect()
            })
        };
        match result {
            Ok(paths) => f(paths),
            Err(e) if e.matches(gtk::DialogError::Dismissed) => {}
            Err(e) => notice(&parent, "Couldn’t choose a location", &e.to_string()),
        }
    });
}

impl Shelf {
    async fn refresh(&self) -> Result<()> {
        let query = self.search.text().to_string();
        let q = query.clone();
        let books = self.store.run(move |lib| lib.search(&q)).await?;
        if self.search.text().as_str() != query {
            return Ok(());
        }
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.count.set_text(&format!(
            "{} {}",
            books.len(),
            if books.len() == 1 { "book" } else { "books" }
        ));
        if books.is_empty() {
            let empty = column(10);
            empty.add_css_class("card");
            inset(&empty, 4);
            let content = column(8);
            inset(&content, 30);
            empty.append(&content);
            let searching = !self.search.text().is_empty();
            content.append(&label(
                if searching {
                    "No matching books"
                } else {
                    "Your first book belongs here"
                },
                "title-2",
            ));
            content.append(&label(
                if searching {
                    "Try a different title or author."
                } else {
                    "Choose an EPUB or a folder to start your library."
                },
                "dim-label",
            ));
            self.rows.append(&empty);
        }
        for book in books {
            self.add_book(book);
        }
        Ok(())
    }
    fn reload(&self) {
        let s = self.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = s.refresh().await {
                notice(&s.window, "Couldn’t load the shelf", &e.to_string());
            }
        });
    }
    fn add_book(&self, book: Book) {
        let card = row(12);
        card.add_css_class("card");
        let inside = row(14);
        inset(&inside, 14);
        inside.set_hexpand(true);
        card.append(&inside);
        let initial = book
            .title
            .chars()
            .find(|c| c.is_alphanumeric())
            .unwrap_or('•')
            .to_uppercase()
            .to_string();
        let cover = gtk::Label::new(Some(&initial));
        cover.add_css_class("book-cover");
        cover.set_valign(gtk::Align::Center);
        inside.append(&cover);
        let info = column(3);
        info.set_hexpand(true);
        info.append(&label(&book.title, "heading"));
        info.append(&label(&book.authors.join(", "), "dim-label"));
        inside.append(&info);
        let book = Rc::new(book);
        for (text, action) in [("Read", 0), ("Edit", 1), ("Export", 2)] {
            let s = self.clone();
            let b = book.clone();
            let btn = button(text, move || match action {
                0 => reader::open(&s, &b),
                1 => s.edit(&b),
                _ => s.export(&b),
            });
            if action == 2 {
                btn.set_tooltip_text(Some(
                    "Copy this EPUB to a folder on your computer or mounted reader",
                ));
            }
            btn.set_valign(gtk::Align::Center);
            inside.append(&btn);
        }
        self.rows.append(&card);
    }
    fn edit(&self, b: &Rc<Book>) {
        let d = gtk::Window::builder()
            .title("Edit book")
            .transient_for(&self.window)
            .modal(true)
            .build();
        let form = column(8);
        inset(&form, 20);
        d.set_child(Some(&form));
        let title = gtk::Entry::new();
        title.update_property(&[gtk::accessible::Property::Label("Book title")]);
        title.set_text(&b.title);
        title.set_width_chars(40);
        let authors = gtk::TextView::new();
        authors.update_property(&[gtk::accessible::Property::Label("Book authors")]);
        authors.buffer().set_text(&b.authors.join("\n"));
        authors.set_height_request(64);
        form.append(&label("Title", "heading"));
        form.append(&title);
        form.append(&label("Authors · one per line", "heading"));
        form.append(&authors);
        let actions = row(8);
        form.append(&actions);
        let weak = d.downgrade();
        actions.append(&button("Cancel", move || {
            if let Some(d) = weak.upgrade() {
                d.close()
            }
        }));
        let s = self.clone();
        let id = b.id.clone();
        let weak = d.downgrade();
        let save = button("Save", move || {
            let Some(d) = weak.upgrade() else { return };
            let buf = authors.buffer();
            let (a, z) = buf.bounds();
            let names = buf
                .text(&a, &z, false)
                .lines()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let title = title.text().trim().to_string();
            if title.is_empty() {
                notice(
                    &d,
                    "Enter a title",
                    "A title helps you find this book again.",
                );
                return;
            }
            let s = s.clone();
            let id = id.clone();
            d.set_sensitive(false);
            glib::spawn_future_local(async move {
                match s.store.run(move |lib| lib.edit(&id, &title, &names)).await {
                    Ok(_) => {
                        d.close();
                        s.reload();
                    }
                    Err(e) => {
                        if let Some(detail) = publication_message(&e) {
                            d.close();
                            s.reload();
                            notice(&s.window, "Changes saved locally", &detail);
                        } else {
                            d.set_sensitive(true);
                            notice(&d, "Couldn’t save changes", &e.to_string());
                        }
                    }
                }
            });
        });
        save.add_css_class("suggested-action");
        actions.append(&save);
        d.present();
    }
    fn export(&self, b: &Rc<Book>) {
        let s = self.clone();
        let b = b.clone();
        choose(
            &self.window,
            "Choose an export folder",
            true,
            move |paths| {
                let Some(dir) = paths.first() else { return };
                let dir = dir.clone();
                let store = s.store.clone();
                let id = b.id.clone();
                let title = b.title.clone();
                let window = s.window.clone();
                glib::spawn_future_local(async move {
                    let result = store
                        .run(move |lib| {
                            let source = lib.managed_file(&id)?;
                            Ok(ebook_device::export_epub(&source, &dir, &title)?)
                        })
                        .await;
                    match result {
                        Ok(ExportOutcome::Copied(p)) => {
                            notice(&window, "Book exported", &p.display().to_string())
                        }
                        Ok(ExportOutcome::AlreadyPresent(p)) => {
                            notice(&window, "Already exported", &p.display().to_string())
                        }
                        Err(e) => notice(&window, "Couldn’t export this book", &e.to_string()),
                    }
                });
            },
        );
    }
}

fn activate(app: &gtk::Application) {
    if let Some(w) = app.active_window() {
        w.present();
        return;
    }
    let w = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Ebook Manager")
        .default_width(840)
        .default_height(620)
        .build();
    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&label("Ebook Manager", "heading")));
    w.set_titlebar(Some(&header));
    let main = column(16);
    inset(&main, 26);
    w.set_child(Some(&main));
    let top = row(12);
    main.append(&top);
    let heading = label("Your library", "shelf-title");
    heading.set_hexpand(true);
    top.append(&heading);
    let search = gtk::SearchEntry::new();
    search.update_property(&[gtk::accessible::Property::Label("Search library")]);
    search.set_placeholder_text(Some("Search by title or author"));
    main.append(&search);
    let count = label("", "dim-label");
    main.append(&count);
    let rows = column(8);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&rows)
        .build();
    main.append(&scroll);
    let shelf = Shelf {
        window: w.clone(),
        store: store::Store::open(glib::user_data_dir().join(Path::new("ebook-manager"))),
        rows,
        search: search.clone(),
        count,
    };
    let sync = gtk::Button::from_icon_name("view-refresh-symbolic");
    sync.set_tooltip_text(Some("Refresh library"));
    sync.update_property(&[gtk::accessible::Property::Label("Refresh library")]);
    header.pack_start(&sync);
    let s = shelf.clone();
    sync.connect_clicked(move |_| {
        let s = s.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = s.store.run(|lib| lib.refresh()).await {
                notice(&s.window, "Couldn’t refresh the library", &format!("{e:#}"));
            }
            s.reload();
        });
    });
    for (text, folder) in [("Add folder", true), ("Add files", false)] {
        let s = shelf.clone();
        let b = button(text, move || {
            let shelf = s.clone();
            choose(
                &s.window,
                if folder {
                    "Choose a folder of EPUBs"
                } else {
                    "Choose EPUB files"
                },
                folder,
                move |p| imports::start(&shelf, p),
            );
        });
        if !folder {
            b.add_css_class("suggested-action");
        }
        top.append(&b);
    }
    let s = shelf.clone();
    search.connect_search_changed(move |_| s.reload());
    w.present();
    shelf.reload();
}
fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.github.endqwerty.EbookManager")
        .build();
    app.connect_startup(|_| {
        libadwaita::init().expect("GTK display");
        let css = gtk::CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("GTK display"),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    app.connect_activate(activate);
    app.run()
}

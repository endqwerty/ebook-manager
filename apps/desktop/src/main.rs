mod browse;
mod cover;
mod details;
mod edit;
mod goto;
mod imports;
mod reader;
mod settings;
mod store;
mod thumbs;
mod views;
mod window;
mod x11_errors;

use anyhow::Result;
use ebook_library::Book;
use gtk::{gio, glib, prelude::*};
use gtk4 as gtk;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

fn data_folder() -> PathBuf {
    glib::user_data_dir().join("ebook-manager")
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
    parent: &impl IsA<gtk::Window>,
    title: &str,
    folder: bool,
    f: impl Fn(Vec<PathBuf>) + 'static,
) {
    let parent: gtk::Window = parent.clone().upcast();
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

fn main() -> glib::ExitCode {
    let app = libadwaita::Application::builder()
        .application_id("io.github.endqwerty.EbookManager")
        .build();
    app.connect_startup(|app| {
        let display = gtk::gdk::Display::default().expect("GTK display");
        x11_errors::install(&display);
        let css = gtk::CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        window::accelerators(app);
    });
    app.connect_activate(|app| match app.active_window() {
        Some(window) => window.present(),
        None => window::open(app),
    });
    app.run()
}

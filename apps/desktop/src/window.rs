//! The library window: the header bar, the search bar, the screen title and
//! the books, with every command as an action on the window.
use super::*;
use crate::{
    browse::{Anchors, BookListModel, Hooks, Query, Replaced, Status},
    thumbs::Thumbs,
    views::{self, BookCell, Shared, grid},
};
use ebook_device::ExportOutcome;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::Cell;

/// The medium cover width.
const COVER: i32 = 128;

/// Where the view was when its replacement was asked for.
#[derive(Clone, Copy, Default)]
struct Place {
    /// Where the first book on screen started, from the top of the view.
    top_offset: f32,
    /// A book held the keyboard focus.
    focused: bool,
}

type Command = Box<dyn Fn(&Rc<Library>)>;

struct Library {
    window: adw::ApplicationWindow,
    store: store::Store,
    model: BookListModel,
    thumbs: Thumbs,
    scrolled: gtk::ScrolledWindow,
    /// As wide as the books' area and no taller than nothing: it reports
    /// each change of width, which a scrolled window does not.
    width: gtk::DrawingArea,
    grid: gtk::GridView,
    pages: gtk::Stack,
    count: gtk::Label,
    search_bar: gtk::SearchBar,
    search: gtk::SearchEntry,
    no_match: adw::StatusPage,
    failed: adw::StatusPage,
    place: Cell<Place>,
    /// Counts replacements, so a late scroll correction can tell it is stale.
    replaced: Cell<u64>,
    surveying: Cell<bool>,
}

fn books(count: u32) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (n, digit) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if count == 1 { "book" } else { "books" })
}

/// The desktop's text size setting, as a multiple of the usual size.
fn text_scale() -> f64 {
    gtk::Settings::default()
        .map(|settings| settings.gtk_xft_dpi())
        .filter(|dpi| *dpi > 0)
        .map_or(1.0, |dpi| f64::from(dpi) / (96.0 * 1024.0))
}

fn status_page(title: &str) -> adw::StatusPage {
    let page = adw::StatusPage::builder().title(title).build();
    page.add_css_class("library-state");
    page
}

fn add_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("_Files…"), Some("win.add-files"));
    menu.append(Some("F_older…"), Some("win.add-folder"));
    menu
}

impl Library {
    fn refresh_state(&self) {
        let count = self.model.n_items();
        let searched = self.model.query().text;
        let page = match self.model.status() {
            Status::Loading => "loading",
            Status::Failed(message) => {
                self.failed.set_description(Some(&message));
                "failed"
            }
            Status::Ready if count > 0 => "books",
            Status::Ready if searched.trim().is_empty() => "empty",
            Status::Ready => {
                self.no_match.set_description(Some(&format!(
                    "Nothing in the library matches “{}”.",
                    glib::markup_escape_text(searched.trim())
                )));
                "none"
            }
        };
        self.pages.set_visible_child_name(page);
        self.count.set_visible(page == "books");
        self.count.set_text(&books(count));
        self.refresh_actions();
    }

    fn refresh_actions(&self) {
        let selected = self.model.selection().size();
        for (name, enabled) in [
            ("read", selected == 1),
            ("edit", selected == 1),
            ("send", selected >= 1),
        ] {
            if let Some(action) = self
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }

    /// The books to find again in the view about to open.
    fn anchors(&self) -> Anchors {
        let focused = views::focused_cell(&self.scrolled);
        let top = views::top_cell(&self.scrolled);
        self.place.set(Place {
            top_offset: top.as_ref().map_or(0.0, |(_, offset)| *offset),
            focused: focused.is_some(),
        });
        let id = |cell: &BookCell| Some(cell.book()?.row()?.id.clone());
        Anchors {
            current: focused
                .as_ref()
                .and_then(id)
                .or_else(|| Some(self.model.selected_one()?.row()?.id.clone())),
            top: top.as_ref().and_then(|(cell, _)| id(cell)),
        }
    }

    /// Puts the reader back where they were in the view that has just
    /// replaced the last: on the book they were on if it is still there,
    /// else at the first book that was on screen, else at the top.
    fn restore(self: &Rc<Self>, replaced: &Replaced) {
        let place = self.place.get();
        let serial = self.replaced.get() + 1;
        self.replaced.set(serial);
        if let Some(rank) = replaced.current {
            if place.focused || !replaced.background {
                self.grid.scroll_to(rank, gtk::ListScrollFlags::FOCUS, None);
            }
            // The book that held the keyboard focus went with the old view.
            if place.focused {
                self.grid.grab_focus();
            }
            // A change the reader did not ask for leaves the page where it was.
            if !replaced.background {
                return;
            }
        }
        let Some(rank) = replaced.top else { return };
        self.grid.scroll_to(rank, gtk::ListScrollFlags::NONE, None);
        // `scroll_to` only brings the book on screen. Once the grid has laid
        // itself out, move it to where it was.
        let weak = Rc::downgrade(self);
        let frames = Cell::new(0);
        self.grid.add_tick_callback(move |_, _| {
            let Some(library) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            frames.set(frames.get() + 1);
            if library.replaced.get() != serial || frames.get() > 10 {
                return glib::ControlFlow::Break;
            }
            if frames.get() < 2 {
                return glib::ControlFlow::Continue;
            }
            let found = views::cells(library.scrolled.upcast_ref())
                .into_iter()
                .find(|cell| cell.book().is_some_and(|book| book.rank() == rank))
                .and_then(|cell| cell.compute_bounds(&library.scrolled));
            let Some(bounds) = found else {
                return glib::ControlFlow::Continue;
            };
            let adjustment = library.scrolled.vadjustment();
            adjustment.set_value(adjustment.value() + f64::from(bounds.y() - place.top_offset));
            glib::ControlFlow::Break
        });
    }

    fn fit(&self) {
        let width = self.width.width();
        if width > 0 {
            grid::fit(&self.grid, width, COVER, text_scale());
        }
    }

    fn search_for(&self, text: &str) {
        let mut query = self.model.query();
        query.text = text.to_owned();
        self.model.set_query(query);
    }

    /// Runs `command` with the ID and title of each selected book, once
    /// their rows have loaded.
    fn with_selection(
        self: &Rc<Self>,
        command: impl FnOnce(&Rc<Self>, Vec<(String, String)>) + 'static,
    ) {
        let selected = self.model.selected();
        let library = self.clone();
        glib::spawn_future_local(async move {
            let mut books = vec![];
            for book in selected {
                let row = book.loaded_row().await;
                books.push((row.id.clone(), row.title.clone()));
            }
            if !books.is_empty() {
                command(&library, books);
            }
        });
    }

    fn read(self: &Rc<Self>, id: String) {
        let library = self.clone();
        glib::spawn_future_local(async move {
            match library.store.run(move |lib| lib.book(&id)).await {
                Ok(book) => reader::open_in(&library.window, &library.store, &Rc::new(book)),
                Err(e) => notice(
                    &library.window,
                    "Couldn’t open this book",
                    &format!("{e:#}"),
                ),
            }
        });
    }

    fn edit(self: &Rc<Self>, id: String) {
        let library = self.clone();
        glib::spawn_future_local(async move {
            match library.store.run(move |lib| lib.book(&id)).await {
                Ok(book) => edit::open(&library.window, &library.store, book),
                Err(e) => notice(
                    &library.window,
                    "Couldn’t open this book",
                    &format!("{e:#}"),
                ),
            }
        });
    }

    /// Copies each book's EPUB to a folder the reader chooses, such as one
    /// on a mounted e-reader.
    fn send(self: &Rc<Self>, books: Vec<(String, String)>) {
        let library = self.clone();
        choose(
            &self.window,
            "Choose an export folder",
            true,
            move |paths| {
                let Some(folder) = paths.into_iter().next() else {
                    return;
                };
                let books = books.clone();
                let library = library.clone();
                glib::spawn_future_local(async move {
                    let result = library
                        .store
                        .run(move |lib| {
                            books
                                .iter()
                                .map(|(id, title)| {
                                    let source = lib.managed_file(id)?;
                                    Ok(ebook_device::export_epub(&source, &folder, title)?)
                                })
                                .collect::<Result<Vec<_>>>()
                        })
                        .await;
                    let window = &library.window;
                    match result.as_deref() {
                        Ok([ExportOutcome::Copied(path)]) => {
                            notice(window, "Book exported", &path.display().to_string())
                        }
                        Ok([ExportOutcome::AlreadyPresent(path)]) => {
                            notice(window, "Already exported", &path.display().to_string())
                        }
                        Ok(outcomes) => {
                            let present = outcomes
                                .iter()
                                .filter(|o| matches!(o, ExportOutcome::AlreadyPresent(_)))
                                .count();
                            notice(
                                window,
                                &format!("{} exported", books_exported(outcomes.len() - present)),
                                &if present > 0 {
                                    format!("{} already in the folder.", books_exported(present))
                                } else {
                                    String::new()
                                },
                            )
                        }
                        Err(e) => notice(window, "Couldn’t export", &format!("{e:#}")),
                    }
                });
            },
        );
    }

    fn context_menu(self: &Rc<Self>, cell: &BookCell, x: f64, y: f64) {
        let Some(id) = cell.book().and_then(|book| Some(book.row()?.id.clone())) else {
            return;
        };
        let item = |label: &str, action: &str| {
            let item = gio::MenuItem::new(Some(label), None);
            item.set_action_and_target_value(Some(action), Some(&id.to_variant()));
            item
        };
        let menu = gio::Menu::new();
        let first = gio::Menu::new();
        first.append_item(&item("_Read", "win.read-book"));
        menu.append_section(None, &first);
        let rest = gio::Menu::new();
        rest.append_item(&item("_Edit…", "win.edit-book"));
        rest.append_item(&item("_Send to Device…", "win.send-book"));
        menu.append_section(None, &rest);
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(cell);
        popover.set_has_arrow(false);
        popover.set_halign(gtk::Align::Start);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|popover| {
            // After the chosen item's action has run.
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }

    fn install_actions(self: &Rc<Self>) {
        let simple = |name: &str, run: Command| {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(library) = weak.upgrade() {
                    run(&library);
                }
            });
            self.window.add_action(&action);
        };
        let on_book = |name: &str, run: fn(&Rc<Self>, String)| {
            let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, id| {
                if let (Some(library), Some(id)) = (weak.upgrade(), id.and_then(|id| id.str())) {
                    run(&library, id.to_owned());
                }
            });
            self.window.add_action(&action);
        };
        for (name, folder) in [("add-files", false), ("add-folder", true)] {
            simple(
                name,
                Box::new(move |library| {
                    let (window, store) = (library.window.clone(), library.store.clone());
                    choose(
                        &library.window,
                        if folder {
                            "Choose a folder of EPUBs"
                        } else {
                            "Choose EPUB files"
                        },
                        folder,
                        move |paths| imports::start(&window, &store, paths),
                    );
                }),
            );
        }
        simple(
            "search",
            Box::new(|library| {
                library.search_bar.set_search_mode(true);
                library.search.grab_focus();
            }),
        );
        simple(
            "refresh",
            Box::new(|library| {
                let library = library.clone();
                glib::spawn_future_local(async move {
                    if let Err(e) = library.store.run(|lib| lib.refresh()).await {
                        notice(
                            &library.window,
                            "Couldn’t refresh the library",
                            &format!("{e:#}"),
                        );
                    }
                });
            }),
        );
        simple(
            "read",
            Box::new(|library| {
                library.with_selection(|library, mut books| library.read(books.remove(0).0))
            }),
        );
        simple(
            "edit",
            Box::new(|library| {
                library.with_selection(|library, mut books| library.edit(books.remove(0).0))
            }),
        );
        simple(
            "send",
            Box::new(|library| library.with_selection(|library, books| library.send(books))),
        );
        on_book("read-book", |library, id| library.read(id));
        on_book("edit-book", |library, id| library.edit(id));
        on_book("send-book", |library, id| {
            let library = library.clone();
            glib::spawn_future_local(async move {
                let found = id.clone();
                match library.store.run(move |lib| lib.book(&found)).await {
                    Ok(book) => library.send(vec![(id, book.title)]),
                    Err(e) => notice(&library.window, "Couldn’t export", &format!("{e:#}")),
                }
            });
        });
    }
}

fn books_exported(count: usize) -> String {
    books(count as u32)
}

pub fn accelerators(app: &adw::Application) {
    for (action, keys) in [
        ("win.add-files", &["<Control>o"][..]),
        ("win.add-folder", &["<Control><Shift>o"]),
        ("win.search", &["<Control>f"]),
        ("win.refresh", &["F5", "<Control>r"]),
        ("win.edit", &["F2"]),
        ("win.send", &["<Control>d"]),
        ("window.close", &["<Control>w", "<Control>q"]),
    ] {
        app.set_accels_for_action(action, keys);
    }
}

pub fn open(app: &adw::Application) {
    let (changed, changes) = async_channel::bounded(1);
    let store = store::Store::open(data_folder(), move || {
        // One waiting message is enough: the listener asks what changed.
        let _ = changed.try_send(());
    });
    let model = BookListModel::new(store.clone());
    let thumbs = Thumbs::new(&store, &data_folder());

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Ebook Manager")
        .default_width(900)
        .default_height(680)
        .width_request(360)
        .height_request(294)
        .build();
    let header = adw::HeaderBar::builder().show_title(false).build();
    let add = gtk::MenuButton::builder()
        .label("_Add")
        .use_underline(true)
        .menu_model(&add_menu())
        .tooltip_text("Add Books")
        .build();
    header.pack_start(&add);
    let menu = gio::Menu::new();
    menu.append(Some("_Refresh Library"), Some("win.refresh"));
    header.pack_end(
        &gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .tooltip_text("Main Menu")
            .build(),
    );
    let search_button = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search")
        .build();
    search_button.update_property(&[gtk::accessible::Property::Label("Search")]);
    header.pack_end(&search_button);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Title, author or series")
        .hexpand(true)
        .build();
    search.update_property(&[gtk::accessible::Property::Label("Search library")]);
    let search_bar = gtk::SearchBar::builder()
        .child(
            &adw::Clamp::builder()
                .maximum_size(480)
                .child(&search)
                .build(),
        )
        .build();
    search_bar.connect_entry(&search);
    search_bar.set_key_capture_widget(Some(&window));
    search_bar
        .bind_property("search-mode-enabled", &search_button, "active")
        .bidirectional()
        .sync_create()
        .build();

    // The screen title is the first line of the content, in the serif, with
    // the count on its baseline. The header bar shows no title.
    let title = gtk::Label::builder()
        .label("All books")
        .valign(gtk::Align::Baseline)
        .build();
    title.add_css_class("title-1");
    title.add_css_class("screen-title");
    let count = gtk::Label::builder().valign(gtk::Align::Baseline).build();
    count.add_css_class("numeric");
    count.add_css_class("dimmed");
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    heading.set_margin_start(grid::EDGE);
    heading.set_margin_end(grid::EDGE);
    heading.set_margin_top(3);
    heading.set_margin_bottom(9);
    heading.append(&title);
    heading.append(&count);

    let library = Rc::<Library>::new_cyclic(|weak: &std::rc::Weak<Library>| {
        let for_menu = weak.clone();
        let shared = Shared {
            thumbs: thumbs.clone(),
            menu: Rc::new(move |cell, x, y| {
                if let Some(library) = for_menu.upgrade() {
                    library.context_menu(cell, x, y);
                }
            }),
        };
        let grid = grid::build(&model, &shared, COVER);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&grid)
            .vexpand(true)
            .build();
        let pages = gtk::Stack::new();
        pages.set_vexpand(true);
        Library {
            window: window.clone(),
            store: store.clone(),
            model: model.clone(),
            thumbs,
            scrolled,
            width: gtk::DrawingArea::builder().hexpand(true).build(),
            grid,
            pages,
            count,
            search_bar: search_bar.clone(),
            search: search.clone(),
            no_match: status_page("No books match"),
            failed: status_page("The library could not be opened"),
            place: Cell::default(),
            replaced: Cell::new(0),
            surveying: Cell::new(false),
        }
    });

    // Nothing for 300 ms, then the progress indicator.
    let spinner = gtk::Spinner::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .width_request(32)
        .height_request(32)
        .build();
    let weak = spinner.downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        if let Some(spinner) = weak.upgrade() {
            spinner.start();
        }
    });
    let empty = status_page("No books yet");
    empty.set_description(Some("Add EPUB files or a folder to start your library."));
    let add_first = gtk::MenuButton::builder()
        .label("_Add")
        .use_underline(true)
        .menu_model(&add_menu())
        .halign(gtk::Align::Center)
        .build();
    add_first.add_css_class("pill");
    add_first.add_css_class("suggested-action");
    empty.set_child(Some(&add_first));
    let clear = gtk::Button::builder()
        .label("_Clear Search")
        .use_underline(true)
        .halign(gtk::Align::Center)
        .build();
    clear.add_css_class("pill");
    let bar = search_bar.clone();
    clear.connect_clicked(move |_| bar.set_search_mode(false));
    library.no_match.set_child(Some(&clear));
    library.pages.add_named(&spinner, Some("loading"));
    library.pages.add_named(&library.scrolled, Some("books"));
    library.pages.add_named(&empty, Some("empty"));
    library.pages.add_named(&library.no_match, Some("none"));
    library.pages.add_named(&library.failed, Some("failed"));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&heading);
    content.append(&library.width);
    content.append(&library.pages);
    let frame = adw::ToolbarView::new();
    frame.add_top_bar(&header);
    frame.add_top_bar(&search_bar);
    frame.set_content(Some(&content));
    window.set_content(Some(&frame));

    library.install_actions();
    let (for_anchors, for_replaced) = (Rc::downgrade(&library), Rc::downgrade(&library));
    model.set_hooks(Hooks {
        anchors: Box::new(move || {
            for_anchors
                .upgrade()
                .map(|library| library.anchors())
                .unwrap_or_default()
        }),
        replaced: Box::new(move |replaced| {
            let Some(library) = for_replaced.upgrade() else {
                return;
            };
            library.refresh_state();
            if let Some(replaced) = replaced {
                library.restore(replaced);
                // Books imported before covers were read get theirs now,
                // after the window has shown what it has.
                if !library.surveying.replace(true) {
                    library.thumbs.survey_unsurveyed();
                }
            }
        }),
    });
    let weak = Rc::downgrade(&library);
    model.connect_selection_changed(move |_, _, _| {
        if let Some(library) = weak.upgrade() {
            library.refresh_actions();
        }
    });
    let weak = Rc::downgrade(&library);
    library.grid.connect_activate(move |grid, position| {
        let (Some(library), Some(book)) = (
            weak.upgrade(),
            grid.model()
                .and_then(|model| model.item(position))
                .and_downcast::<browse::BookObject>(),
        ) else {
            return;
        };
        glib::spawn_future_local(async move {
            let id = book.loaded_row().await.id.clone();
            library.read(id);
        });
    });
    let weak = Rc::downgrade(&library);
    library.width.connect_resize(move |_, _, _| {
        if let Some(library) = weak.upgrade() {
            library.fit();
        }
    });
    if let Some(settings) = gtk::Settings::default() {
        let weak = Rc::downgrade(&library);
        settings.connect_gtk_xft_dpi_notify(move |_| {
            if let Some(library) = weak.upgrade() {
                library.fit();
            }
        });
    }
    // The Menu key and Shift+F10 open the menu for the focused book.
    let keys = gtk::ShortcutController::new();
    let weak = Rc::downgrade(&library);
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Menu|<Shift>F10"),
        Some(gtk::CallbackAction::new(move |_, _| {
            let Some(library) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match views::focused_cell(&library.scrolled) {
                Some(cell) => {
                    let (x, y) = (
                        f64::from(cell.width()) / 2.0,
                        f64::from(cell.height()) / 2.0,
                    );
                    library.context_menu(&cell, x, y);
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        })),
    ));
    window.add_controller(keys);
    // Escape closes the search from anywhere in the window, as it does from
    // the search field.
    let keys = gtk::ShortcutController::new();
    let weak = Rc::downgrade(&library);
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::CallbackAction::new(move |_, _| match weak.upgrade() {
            Some(library) if library.search_bar.is_search_mode() => {
                library.search_bar.set_search_mode(false);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        })),
    ));
    window.add_controller(keys);
    let weak = Rc::downgrade(&library);
    search.connect_search_changed(move |search| {
        if let Some(library) = weak.upgrade() {
            library.search_for(search.text().as_str());
        }
    });
    // Down leaves the search field for the books it found.
    let leave = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(&library);
    leave.connect_key_pressed(move |_, key, _, _| match weak.upgrade() {
        Some(library) if key == gtk::gdk::Key::Down && library.model.n_items() > 0 => {
            // Arriving from the search field selects, as moving between
            // books with the arrow keys does.
            if library.model.selection().is_empty() {
                let to = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
                library.grid.scroll_to(0, to, None);
            }
            library.grid.grab_focus();
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    });
    search.add_controller(leave);

    let weak = model.downgrade();
    glib::spawn_future_local(async move {
        while changes.recv().await.is_ok() {
            let Some(model) = weak.upgrade() else { break };
            model.catalogue_changed();
        }
    });
    // A window opened from this one closes first, through its own close, so
    // a reader saves its place before the process ends.
    window.connect_close_request(|window| {
        let open: Vec<_> = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .filter(|other| {
                other.is_visible() && other.transient_for().as_ref() == Some(window.upcast_ref())
            })
            .collect();
        if open.is_empty() {
            return glib::Propagation::Proceed;
        }
        let left = Rc::new(Cell::new(open.len()));
        for other in open {
            let (left, window) = (left.clone(), window.downgrade());
            other.connect_destroy(move |_| {
                left.set(left.get() - 1);
                if left.get() == 0 {
                    // Once this window has finished going.
                    let window = window.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(window) = window.upgrade() {
                            window.close();
                        }
                    });
                }
            });
            other.close();
        }
        glib::Propagation::Stop
    });
    // The window owns the library's state for as long as it is open.
    window.connect_destroy(move |_| {
        let _ = &library;
    });
    model.set_query(Query::default());
    window.present();
}

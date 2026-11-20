//! The library window: the header bar, the search bar, the screen title and
//! the books, with every command as an action on the window.
mod controls;

use super::*;
use crate::{
    browse::{Anchors, BookListModel, Hooks, Placed, Placement, Query, Status},
    settings::{Settings, SortField, ViewKind},
    thumbs::Thumbs,
    views::{self, BookCell, Shared, View, grid, list, table},
};
use ebook_device::ExportOutcome;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};

type Command = Box<dyn Fn(&Rc<Library>)>;

struct Library {
    window: adw::ApplicationWindow,
    store: store::Store,
    model: BookListModel,
    thumbs: Thumbs,
    scrolled: gtk::ScrolledWindow,
    overlay: gtk::Overlay,
    context_popover: gtk::PopoverMenu,
    preview: gtk::Popover,
    preview_cell: RefCell<Option<glib::WeakRef<BookCell>>>,
    context_focus: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    menu_buttons: [gtk::MenuButton; 3],
    /// As wide as the books' area and no taller than nothing: it reports
    /// each change of width, which a scrolled window does not.
    width: gtk::DrawingArea,
    /// The view on screen: the saved choice, or the list standing in for it.
    view: RefCell<Option<(ViewKind, View)>>,
    /// The window is under 640 wide, where the table is not offered.
    narrow: Cell<bool>,
    settings: RefCell<Settings>,
    settings_file: PathBuf,
    saving: Cell<bool>,
    saved: RefCell<Settings>,
    /// The view is to be built anew when the model next has its books.
    rebuild: Cell<bool>,
    sorted_by: Rc<Cell<SortField>>,
    controls: controls::Controls,
    pages: gtk::Stack,
    count: gtk::Label,
    search_bar: gtk::SearchBar,
    search: gtk::SearchEntry,
    no_match: adw::StatusPage,
    failed: adw::StatusPage,
    /// Counts placements, so a late scroll correction can tell it is stale.
    placed: Cell<u64>,
    /// Counts what the reader does to the books with the wheel, the
    /// pointer or a key, any of which can move them.
    moved: Rc<Cell<u64>>,
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
        let selected = self.model.selected_count();
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

    /// Where the reader is in the books on screen.
    fn anchors(&self) -> Anchors {
        let focused = views::focused_cell(&self.scrolled);
        let top = self
            .with_view(|view| views::top_cell(&view.body()))
            .flatten();
        let id = |cell: &BookCell| Some(cell.book()?.row()?.id.clone());
        Anchors {
            current: focused
                .as_ref()
                .and_then(id)
                .or_else(|| self.model.selected_one()),
            top: top.as_ref().and_then(|(cell, _)| id(cell)),
            top_offset: top.as_ref().map_or(0.0, |(_, offset)| *offset),
            focused: focused.is_some(),
        }
    }

    fn viewport_ready(&self) -> bool {
        self.with_view(|view| {
            let body = view.body();
            body.is_mapped() && body.width() > 0 && body.height() > 0
        }) == Some(true)
            && self.scrolled.hadjustment().page_size() > 0.0
            && self.scrolled.vadjustment().page_size() > 0.0
    }

    /// Puts the reader back after GTK has allocated the new viewport.
    fn restore(self: &Rc<Self>, placed: Placed) {
        let serial = self.placed.get() + 1;
        self.placed.set(serial);
        if placed.current.is_none() && placed.top.is_none() {
            return;
        }
        if self.viewport_ready() {
            self.place_books(&placed, serial);
            return;
        }
        // Older GTK computes a scroll alignment by dividing by the viewport
        // size. A newly attached list can be mapped before it has any size.
        let weak = Rc::downgrade(self);
        let moved = self.moved.get();
        self.scrolled.add_tick_callback(move |_, _| {
            let Some(library) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if library.placed.get() != serial || library.moved.get() != moved {
                return glib::ControlFlow::Break;
            }
            if !library.viewport_ready() {
                return glib::ControlFlow::Continue;
            }
            library.place_books(&placed, serial);
            glib::ControlFlow::Break
        });
    }

    fn place_books(self: &Rc<Self>, placed: &Placed, serial: u64) {
        let was = &placed.anchors;
        let rebuilt = placed.reason == Placement::Layout;
        let keep_top = placed.reason != Placement::Command;
        let focused = placed.current.filter(|_| rebuilt && was.focused);
        if let Some(rank) = placed.current {
            if was.focused || !keep_top {
                self.with_view(|view| view.scroll_to(rank, gtk::ListScrollFlags::FOCUS));
            }
            if was.focused {
                self.with_view(|view| view.focus_books());
            }
        }
        if !keep_top {
            return;
        }
        let Some(rank) = placed.top else { return };
        self.with_view(|view| view.scroll_to(rank, gtk::ListScrollFlags::NONE));
        // `scroll_to` only brings the book on screen. Once the view has laid
        // itself out, move it to where it was. An offset measured in one
        // kind of view means nothing in another.
        let weak = Rc::downgrade(self);
        let frames = Cell::new(0);
        let top_offset = if rebuilt { 0.0 } else { was.top_offset };
        let moved = self.moved.get();
        self.scrolled.add_tick_callback(move |_, _| {
            let Some(library) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            frames.set(frames.get() + 1);
            // The correction gives way to a later placement and to the
            // reader: if they have scrolled, clicked or pressed a key in
            // the books since, the view stays where they put it.
            if library.placed.get() != serial || library.moved.get() != moved {
                return glib::ControlFlow::Break;
            }
            if frames.get() < 2 {
                return glib::ControlFlow::Continue;
            }
            library
                .with_view(|view| {
                    let body = view.body();
                    let cells = views::cells(&body);
                    let bounds_of = |rank| {
                        cells
                            .iter()
                            .find(|cell| cell.book().is_some_and(|book| book.rank() == rank))
                            .and_then(|cell| views::row_bounds(cell, &body))
                    };
                    let focus_book = |rank| {
                        view.scroll_to(rank, gtk::ListScrollFlags::FOCUS);
                        view.focus_books();
                    };
                    let Some(top) = bounds_of(rank) else {
                        if frames.get() <= 10 {
                            return glib::ControlFlow::Continue;
                        }
                        if let Some(focused) = focused {
                            focus_book(focused);
                        }
                        return glib::ControlFlow::Break;
                    };
                    let adjustment = library.scrolled.vadjustment();
                    let mut offset = f64::from(top.y() - top_offset);
                    if rebuilt {
                        offset = offset.ceil();
                    }
                    if let Some(focused) = focused {
                        let Some(bounds) = bounds_of(focused) else {
                            // The top row and focus are too far apart for GTK
                            // to have both rows allocated at once.
                            focus_book(focused);
                            return glib::ControlFlow::Break;
                        };
                        let start = f64::from(bounds.y());
                        let end = f64::from(bounds.y() + bounds.height());
                        offset = offset.clamp((end - adjustment.page_size()).min(start), start);
                    }
                    adjustment.set_value(adjustment.value() + offset);
                    if let Some(focused) = focused {
                        focus_book(focused);
                    }
                    glib::ControlFlow::Break
                })
                .unwrap_or(glib::ControlFlow::Break)
        });
    }

    fn with_view<T>(&self, act: impl FnOnce(&View) -> T) -> Option<T> {
        self.view.borrow().as_ref().map(|(_, view)| act(view))
    }

    /// The view to show for the saved choice in this window.
    fn shown(&self) -> ViewKind {
        self.settings
            .borrow()
            .view
            .shown(self.narrow.get(), text_scale())
    }

    /// Gives the grid the columns its width has room for.
    fn fit(&self) {
        let width = self.width.width();
        if width > 0
            && let Some((_, View::Grid(grid))) = &*self.view.borrow()
        {
            let cover = self.settings.borrow().cover_size.grid();
            grid::fit(grid, width, cover, text_scale());
        }
    }

    /// Builds the view for the saved choices and puts it on screen.
    fn build_view(self: &Rc<Self>) {
        self.close_menus();
        let kind = self.shown();
        let settings = self.settings.borrow().clone();
        let menu = Rc::downgrade(self);
        let shared = Shared {
            thumbs: self.thumbs.clone(),
            preview: {
                let weak = Rc::downgrade(self);
                Rc::new(move |cell, show| {
                    if let Some(lib) = weak.upgrade() {
                        lib.preview(cell, show);
                    }
                })
            },
            menu: Rc::new(move |cell| {
                if let Some(library) = menu.upgrade() {
                    library.context_menu(cell);
                }
            }),
            cover_size: settings.cover_size,
            sorted_by: self.sorted_by.clone(),
        };
        let view = match kind {
            ViewKind::Shelf => View::Shelf(views::shelf::Shelf::new(
                &self.model,
                &shared,
                &self.scrolled,
            )),
            ViewKind::Grid => View::Grid(grid::build(&self.model, &shared)),
            ViewKind::List => View::List(list::build(&self.model, &shared)),
            ViewKind::Table => {
                let (sorts, layouts, menus) = (
                    Rc::downgrade(self),
                    Rc::downgrade(self),
                    Rc::downgrade(self),
                );
                View::Table(table::build(
                    &self.model,
                    &shared,
                    &settings.columns,
                    settings.sort(kind),
                    table::Events {
                        menu: Box::new(move |header, menu| {
                            if let Some(library) = menus.upgrade() {
                                library.show_context_menu(header, menu);
                            }
                        }),
                        sort: Box::new(move |field, add| {
                            if let Some(library) = sorts.upgrade() {
                                let sort = library.sort();
                                library.set_sort(if add {
                                    sort.then_by(field)
                                } else {
                                    sort.by(field)
                                });
                            }
                        }),
                        layout: Box::new(move |columns| {
                            if let Some(library) = layouts.upgrade() {
                                library.settings.borrow_mut().columns = columns;
                                library.show_choices();
                                library.save_soon();
                            }
                        }),
                    },
                ))
            }
        };
        // Return and a double click read the selected book, as every
        // command from the keyboard acts on the selection. A click selects
        // the book it lands on, so for a pointer the two are the same book.
        let weak = self.window.downgrade();
        view.connect_activate(move |_| {
            if let Some(window) = weak.upgrade() {
                gio::prelude::ActionGroupExt::activate_action(&window, "read", None);
            }
        });
        self.scrolled
            .set_hscrollbar_policy(if kind == ViewKind::Table {
                gtk::PolicyType::Automatic
            } else if kind == ViewKind::Shelf {
                gtk::PolicyType::External
            } else {
                gtk::PolicyType::Never
            });
        self.scrolled.set_child(Some(view.widget()));
        self.view.replace(Some((kind, view)));
        self.fit();
    }

    fn search_for(&self, text: &str) {
        let mut query = self.model.query();
        query.text = text.to_owned();
        self.model.set_query(query);
    }

    /// Writes the remembered choices shortly, once for a run of changes
    /// such as a column being dragged wider.
    fn save_soon(self: &Rc<Self>) {
        if *self.settings.borrow() == *self.saved.borrow() || self.saving.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            if let Some(library) = weak.upgrade() {
                library.save();
            }
        });
    }

    fn save(&self) {
        self.saving.set(false);
        // The choices are a convenience. Losing them is not worth a message.
        let settings = self.settings.borrow();
        if *settings != *self.saved.borrow() && settings.save(&self.settings_file).is_ok() {
            self.saved.replace(settings.clone());
        }
    }

    /// Runs `command` with the ID of each selected book.
    fn with_selection(self: &Rc<Self>, command: impl FnOnce(&Rc<Self>, Vec<String>) + 'static) {
        // The books are those selected now, when the command is given,
        // whatever is selected by the time their rows have arrived.
        let targets = self.model.targets();
        let library = self.clone();
        glib::spawn_future_local(async move {
            match targets.ids().await {
                Some(books) if !books.is_empty() => command(&library, books),
                Some(_) => {}
                // A selected row was still loading when the list was
                // replaced, so its place no longer stands for a book.
                None => notice(
                    &library.window,
                    "The list changed",
                    "A selected book was still loading when the list changed. Select it again.",
                ),
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
    fn send(self: &Rc<Self>, books: Vec<String>) {
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
                                .map(|id| {
                                    let title = lib.book(id)?.title;
                                    let source = lib.managed_file(id)?;
                                    Ok(ebook_device::export_epub(&source, &folder, &title)?)
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

    fn context_menu(self: &Rc<Self>, cell: &BookCell) {
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
        self.show_context_menu(cell, &menu);
    }

    fn restore_context_focus(&self) {
        if let Some(focus) = self.context_focus.take().and_then(|focus| focus.upgrade())
            && focus.is_mapped()
        {
            focus.grab_focus();
        }
    }

    fn preview(&self, cell: &BookCell, show: bool) {
        if !show {
            if self
                .preview_cell
                .borrow()
                .as_ref()
                .and_then(|c| c.upgrade())
                .as_ref()
                == Some(cell)
            {
                self.preview.popdown();
                self.preview_cell.take();
            }
            return;
        }
        if self.context_popover.is_visible() {
            return;
        }
        let Some(row) = cell.book().and_then(|book| book.row()) else {
            return;
        };
        let Some(bounds) = cell.compute_bounds(&self.overlay) else {
            return;
        };
        let cover = crate::cover::Cover::new(crate::cover::Kind::List, 96, &self.thumbs);
        cover.set_halign(gtk::Align::Center);
        cover.set_book(Some(&row));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&cover);
        for text in [&row.title, &row.authors, &views::shelf::state_label(&row)] {
            let label = gtk::Label::new(Some(text));
            label.set_max_width_chars(25);
            label.set_wrap(true);
            content.append(&label);
        }
        self.preview.set_child(Some(&content));
        self.preview.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            bounds.x() as i32,
            bounds.y() as i32 + 14,
            bounds.width() as i32,
            1,
        )));
        self.preview_cell.replace(Some(cell.downgrade()));
        self.preview.popup();
    }

    fn close_context_menu(&self) {
        self.preview.popdown();
        self.preview_cell.take();
        if self.context_popover.is_visible() {
            self.context_popover.popdown();
        }
        self.restore_context_focus();
    }

    fn close_menus(&self) {
        self.close_context_menu();
        for button in &self.menu_buttons {
            button.popdown();
        }
        self.controls.close_menus();
    }

    /// The anchor supplies geometry only. The overlay owns transient menus
    /// because GTK can recycle a book cell or replace its entire view.
    fn show_context_menu(&self, anchor: &impl IsA<gtk::Widget>, menu: &gio::Menu) {
        let Some(bounds) = anchor.compute_bounds(&self.overlay) else {
            return;
        };
        self.close_menus();
        let popover = &self.context_popover;
        popover.set_menu_model(Some(menu));
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            bounds.x() as i32,
            bounds.y() as i32,
            bounds.width() as i32,
            bounds.height() as i32,
        )));
        let previous_focus =
            gtk::prelude::RootExt::focus(&self.window).map(|focus| focus.downgrade());
        self.context_focus.replace(previous_focus);
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
                library.with_selection(|library, mut books| library.read(books.remove(0)))
            }),
        );
        simple(
            "edit",
            Box::new(|library| {
                library.with_selection(|library, mut books| library.edit(books.remove(0)))
            }),
        );
        simple(
            "send",
            Box::new(|library| library.with_selection(|library, books| library.send(books))),
        );
        on_book("read-book", |library, id| library.read(id));
        on_book("edit-book", |library, id| library.edit(id));
        on_book("send-book", |library, id| library.send(vec![id]));
    }
}

/// Whether a key moves about or commands something, and so is not text
/// for the search field. With the search field focused and not yet on
/// screen, or a dead key waiting for its letter, Space and the editing keys
/// are text too, and so is Escape for the dead key it cancels.
fn commands(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType, text: bool) -> bool {
    use glib::translate::IntoGlib;
    use gtk::gdk::{Key, ModifierType};
    let held = ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK;
    if modifiers.intersects(held) {
        return true;
    }
    let moves = matches!(
        key,
        Key::Return
            | Key::KP_Enter
            | Key::ISO_Enter
            | Key::Tab
            | Key::ISO_Left_Tab
            | Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::Page_Up
            | Key::Page_Down
            | Key::Menu
            | Key::Insert
    ) || (Key::F1.into_glib()..=Key::F35.into_glib()).contains(&key.into_glib());
    let edits = matches!(key, Key::Escape | Key::space | Key::BackSpace | Key::Delete);
    moves || (edits && !text)
}

fn books_exported(count: usize) -> String {
    books(count as u32)
}

pub fn accelerators(app: &adw::Application) {
    for (action, keys) in [
        ("win.add-files", &["<Control>o"][..]),
        ("win.add-folder", &["<Control><Shift>o"]),
        ("win.search", &["<Control>f"]),
        ("win.view('grid')", &["<Control>1"]),
        ("win.view('list')", &["<Control>2"]),
        ("win.view-table", &["<Control>3"]),
        ("win.view('shelf')", &["<Control>4"]),
        ("win.sort-more", &["<Control><Shift>s"]),
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
    let menu = controls::main_menu();
    let main_menu = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .primary(true)
        .tooltip_text("Main Menu")
        .build();
    header.pack_end(&main_menu);
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

    let add_first = gtk::MenuButton::builder()
        .label("_Add")
        .use_underline(true)
        .menu_model(&add_menu())
        .halign(gtk::Align::Center)
        .build();
    add_first.add_css_class("pill");
    add_first.add_css_class("suggested-action");

    let settings_file = glib::user_config_dir().join("ebook-manager/settings.json");
    let settings = Settings::load(&settings_file);
    let library = Rc::new({
        let scrolled = gtk::ScrolledWindow::builder().vexpand(true).build();
        let pages = gtk::Stack::new();
        pages.set_vexpand(true);
        let context_popover = gtk::PopoverMenu::from_model(gio::MenuModel::NONE);
        context_popover.set_visible(false);
        context_popover.set_has_arrow(false);
        context_popover.set_halign(gtk::Align::Start);
        Library {
            window: window.clone(),
            store: store.clone(),
            model: model.clone(),
            thumbs,
            scrolled,
            overlay: gtk::Overlay::new(),
            context_popover,
            preview: gtk::Popover::builder()
                .visible(false)
                .position(gtk::PositionType::Top)
                .focusable(false)
                .can_target(false)
                .autohide(false)
                .halign(gtk::Align::Start)
                .build(),
            preview_cell: Default::default(),
            context_focus: RefCell::new(None),
            menu_buttons: [add, main_menu, add_first.clone()],
            width: gtk::DrawingArea::builder().hexpand(true).build(),
            view: RefCell::new(None),
            narrow: Cell::new(false),
            sorted_by: Rc::new(Cell::new(settings.sort(settings.view).first().field)),
            saved: RefCell::new(settings.clone()),
            settings: RefCell::new(settings),
            settings_file,
            saving: Cell::new(false),
            rebuild: Cell::new(false),
            controls: controls::Controls::new(&header),
            pages,
            count,
            search_bar: search_bar.clone(),
            search: search.clone(),
            no_match: status_page("No books match"),
            failed: status_page("The library could not be opened"),
            placed: Cell::new(0),
            moved: Rc::default(),
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
    let again = gtk::Button::builder()
        .label("_Try Again")
        .use_underline(true)
        .halign(gtk::Align::Center)
        .build();
    again.add_css_class("pill");
    let weak = model.downgrade();
    again.connect_clicked(move |_| {
        if let Some(model) = weak.upgrade() {
            model.retry();
        }
    });
    library.failed.set_child(Some(&again));
    library.pages.add_named(&library.failed, Some("failed"));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&heading);
    content.append(&library.width);
    content.append(&library.pages);
    let frame = adw::ToolbarView::new();
    frame.add_top_bar(&header);
    frame.add_top_bar(&search_bar);
    frame.set_content(Some(&content));
    library.overlay.set_child(Some(&frame));
    // Reuse the native popup surface. Older GTK can still have a tooltip
    // query pending when the menu closes.
    library.overlay.add_overlay(&library.preview);
    library.overlay.add_overlay(&library.context_popover);
    let weak = Rc::downgrade(&library);
    library.context_popover.connect_closed(move |_| {
        let weak = weak.clone();
        glib::idle_add_local_once(move || {
            if let Some(library) = weak.upgrade()
                && !library.context_popover.is_visible()
            {
                library.restore_context_focus();
            }
        });
    });
    window.set_content(Some(&library.overlay));

    library.install_actions();
    library.install_controls();
    library.build_view();
    let (for_anchors, for_changed, for_place) = (
        Rc::downgrade(&library),
        Rc::downgrade(&library),
        Rc::downgrade(&library),
    );
    let for_replace = Rc::downgrade(&library);
    model.set_hooks(Hooks {
        before_replace: Box::new(move || {
            if let Some(library) = for_replace.upgrade() {
                // Only a menu that points at a book or a column has to go
                // when the books are replaced. The header's menus point at
                // nothing in the view, and closing them here would shut
                // them once a second for as long as an import runs.
                library.close_context_menu();
                library.placed.set(library.placed.get() + 1);
            }
        }),
        anchors: Box::new(move || {
            for_anchors
                .upgrade()
                .map(|library| library.anchors())
                .unwrap_or_default()
        }),
        changed: Box::new(move |replaced| {
            let Some(library) = for_changed.upgrade() else {
                return;
            };
            // A view to be built anew goes on screen with its books.
            if replaced && library.rebuild.take() {
                library.build_view();
                library.show_sort();
            }
            library.refresh_state();
            // Books imported before covers were read get theirs once the
            // window has shown what it has.
            if library.model.status() == Status::Ready && !library.surveying.replace(true) {
                library.thumbs.survey_unsurveyed();
            }
        }),
        place: Box::new(move |placed| {
            if let Some(library) = for_place.upgrade() {
                library.restore(placed);
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
    library.width.connect_resize(move |_, _, _| {
        let weak = weak.clone();
        glib::idle_add_local_once(move || {
            if let Some(library) = weak.upgrade() {
                library.close_menus();
                library.set_narrow(library.width.width() < 640);
                library.fit();
            }
        });
    });
    if let Some(settings) = gtk::Settings::default() {
        let weak = Rc::downgrade(&library);
        settings.connect_gtk_xft_dpi_notify(move |_| {
            if let Some(library) = weak.upgrade() {
                // Larger text takes columns from the grid and, from one
                // and a half times, shows the list in its place.
                library.show(false);
            }
        });
    }
    // Whatever the reader does to the books that can move them is counted,
    // so a scroll correction still to come can give way to it.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    let moved = library.moved.clone();
    wheel.connect_scroll(move |_, _, _| {
        moved.set(moved.get() + 1);
        glib::Propagation::Proceed
    });
    library.scrolled.add_controller(wheel);
    let press = gtk::GestureClick::builder()
        .button(0)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let moved = library.moved.clone();
    press.connect_pressed(move |_, _, _, _| moved.set(moved.get() + 1));
    library.scrolled.add_controller(press);
    let pressed = gtk::EventControllerKey::new();
    pressed.set_propagation_phase(gtk::PropagationPhase::Capture);
    let moved = library.moved.clone();
    pressed.connect_key_pressed(move |_, _, _, _| {
        moved.set(moved.get() + 1);
        glib::Propagation::Proceed
    });
    library.scrolled.add_controller(pressed);
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
                    library.context_menu(&cell);
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
    // Typing anywhere but in a text field goes to the search field, and
    // opens the search if it is closed. The window passes the keys on
    // itself. GTK's search bar and search field pass on only the key that
    // opens the search, and the field, which takes the focus at once, takes
    // keys only when it is on screen a frame later: a key typed in between
    // was lost. Every press and release goes through the field's own key
    // handling, and so through its input method, so a dead key or a
    // composed letter starts a search as a plain one does. Only the keys
    // that move about or command something are left where they are.
    let typing = gtk::EventControllerKey::new();
    typing.set_propagation_phase(gtk::PropagationPhase::Capture);
    // A dead key has been passed on and its letter has not come yet.
    let composing = Rc::new(Cell::new(false));
    let pass = {
        let weak = Rc::downgrade(&library);
        let composing = composing.clone();
        move |typing: &gtk::EventControllerKey, key: gtk::gdk::Key, modifiers| {
            let library = weak.upgrade()?;
            let field = {
                let mut child = library.search.first_child();
                while child.as_ref().is_some_and(|c| !c.is::<gtk::Text>()) {
                    child = child.and_then(|c| c.next_sibling());
                }
                child.and_downcast::<gtk::Text>()?
            };
            let focus = gtk::prelude::GtkWindowExt::focus(&library.window);
            let in_field = focus.as_ref() == Some(field.upcast_ref());
            let in_text = focus
                .as_ref()
                .is_some_and(|focus| focus.is::<gtk::Text>() || focus.is::<gtk::TextView>());
            // Once the search is open and its field is on screen with the
            // focus, the field needs no help. While the search is closed
            // the field can still hold the focus, unseen, and then its
            // keys come this way so that they open the search. Another
            // text field keeps its own keys.
            let ready = in_field && field.is_mapped() && library.search_bar.is_search_mode();
            if ready || (in_text && !in_field) {
                return None;
            }
            // While a dead key waits, Escape and Backspace cancel it and
            // Space completes it, as they do in a text field.
            let text = in_field || composing.get();
            (!commands(key, modifiers, text) && typing.forward(&field)).then_some((library, field))
        }
    };
    let pressed = pass.clone();
    typing.connect_key_pressed(move |typing, key, _, modifiers| {
        use gtk::gdk::Key;
        let ends = matches!(key, Key::Escape | Key::BackSpace | Key::space);
        match pressed(typing, key, modifiers) {
            Some((library, field)) => {
                composing.set(field.text().is_empty() && !ends);
                // The search opens when there is text. A dead key gives
                // none yet, and moving the focus now would make the input
                // method forget it.
                if !field.text().is_empty() {
                    library.search_bar.set_search_mode(true);
                    // Taking the focus the ordinary way would select what
                    // has been typed, and the next key would replace it.
                    if !field.has_focus() {
                        field.grab_focus_without_selecting();
                    }
                }
                glib::Propagation::Stop
            }
            None => {
                if ends {
                    composing.set(false);
                }
                glib::Propagation::Proceed
            }
        }
    });
    typing.connect_key_released(move |typing, key, _, modifiers| {
        pass(typing, key, modifiers);
    });
    window.add_controller(typing);
    // Down leaves the search field for the books it found.
    let leave = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(&library);
    leave.connect_key_pressed(move |_, key, _, _| match weak.upgrade() {
        Some(library) if key == gtk::gdk::Key::Down && library.model.n_items() > 0 => {
            // Arriving from the search field selects, as moving between
            // books with the arrow keys does.
            if library.model.selected_count() == 0 {
                let to = gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT;
                library.with_view(|view| view.scroll_to(0, to));
            }
            library.with_view(|view| view.focus_books());
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
    let weak = Rc::downgrade(&library);
    window.connect_close_request(move |window| {
        if let Some(library) = weak.upgrade() {
            library.close_menus();
            library.save();
        }
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
    let query = Query {
        shelf: library.shown() == ViewKind::Shelf,
        text: String::new(),
        sort: library.sort().terms(),
    };
    // The window owns the library's state for as long as it is open.
    window.connect_destroy(move |_| {
        let _ = &library;
    });
    model.set_query(query);
    window.present();
}

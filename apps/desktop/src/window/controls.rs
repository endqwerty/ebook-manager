//! The controls above the books: the view switch, the Sort menu and its
//! editor, the cover sizes and the table's columns. Each is an action on
//! the window whose state is the remembered choice.
use super::*;
use crate::settings::{CoverSize, Sort, SortKey};

/// The three rows of the sort editor: a field and a direction each.
struct Editor {
    dialog: adw::Dialog,
    rows: Vec<(gtk::DropDown, gtk::DropDown)>,
    /// The rows are being set from the sort, which is not the reader
    /// choosing.
    setting: Rc<Cell<bool>>,
}

pub struct Controls {
    /// The views as linked buttons, on a window with room for them.
    switch: gtk::Box,
    /// The views as a menu, on a narrow window.
    switch_menu: gtk::MenuButton,
    sort_menu: gtk::MenuButton,
    editor: RefCell<Option<Editor>>,
}

pub fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let sizes = gio::Menu::new();
    for (_, id, label) in CoverSize::ALL {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.cover-size"), Some(&id.to_variant()));
        sizes.append_item(&item);
    }
    menu.append_section(None, &sizes);
    let rest = gio::Menu::new();
    rest.append(Some("_Go to…"), Some("win.go-to"));
    rest.append(Some("_Refresh Library"), Some("win.refresh"));
    menu.append_section(None, &rest);
    menu
}

fn choice(label: &str, action: &str, target: &str) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), Some(&target.to_variant()));
    item
}

fn sort_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let fields = gio::Menu::new();
    // Progress is a choice in the editor and not in the menu.
    for (field, id, name) in SortField::ALL {
        if field != SortField::Progress {
            fields.append_item(&choice(name, "win.sort-field", id));
        }
    }
    menu.append_section(None, &fields);
    let directions = gio::Menu::new();
    directions.append_item(&choice("_Ascending", "win.sort-direction", "ascending"));
    directions.append_item(&choice("_Descending", "win.sort-direction", "descending"));
    menu.append_section(None, &directions);
    let more = gio::Menu::new();
    more.append(Some("_More…"), Some("win.sort-more"));
    menu.append_section(None, &more);
    menu
}

impl Controls {
    /// Adds the controls to the end of the header bar, before what is
    /// already there: the view switch, then Sort.
    pub fn new(header: &adw::HeaderBar) -> Self {
        let sort = gtk::MenuButton::builder()
            .icon_name("view-sort-descending-symbolic")
            .menu_model(&sort_menu())
            .tooltip_text("Sort")
            .build();
        sort.update_property(&[gtk::accessible::Property::Label("Sort")]);
        header.pack_end(&sort);
        let switch = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        switch.add_css_class("linked");
        let narrow = gio::Menu::new();
        for (kind, id, name, icon) in ViewKind::ALL {
            let button = gtk::ToggleButton::builder()
                .icon_name(icon)
                .tooltip_text(name)
                .action_name("win.view")
                .action_target(&id.to_variant())
                .build();
            button.update_property(&[gtk::accessible::Property::Label(name)]);
            switch.append(&button);
            // A narrow window does not offer the table.
            if kind != ViewKind::Table {
                narrow.append_item(&choice(name, "win.view", id));
            }
        }
        let switch_menu = gtk::MenuButton::builder()
            .icon_name("view-grid-symbolic")
            .menu_model(&narrow)
            .tooltip_text("View")
            .visible(false)
            .build();
        switch_menu.update_property(&[gtk::accessible::Property::Label("View")]);
        header.pack_end(&switch_menu);
        header.pack_end(&switch);
        Self {
            switch,
            switch_menu,
            sort_menu: sort,
            editor: RefCell::new(None),
        }
    }

    pub(super) fn close_menus(&self) {
        self.sort_menu.popdown();
        self.switch_menu.popdown();
    }
}

impl Library {
    /// The sort of the view on screen.
    pub(super) fn sort(&self) -> Sort {
        self.settings.borrow().sort(self.shown()).clone()
    }

    pub(super) fn set_sort(self: &Rc<Self>, sort: Sort) {
        let kind = self.shown();
        self.settings.borrow_mut().set_sort(kind, sort);
        self.show(false);
    }

    /// Brings the window in line with the remembered choices. `rebuild`
    /// makes the view again, for a choice its rows were built with.
    pub(super) fn show(self: &Rc<Self>, rebuild: bool) {
        let kind = self.shown();
        let sort = self.sort();
        let rebuild = rebuild || self.view.borrow().as_ref().map(|(shown, _)| *shown) != Some(kind);
        self.sorted_by.set(sort.first().field);
        let mut query = self.model.query();
        query.sort = sort.terms();
        query.shelf = kind == ViewKind::Shelf;
        // A view built anew is put on screen when the model has the books
        // for it, in `shown_again`. Until then the old one stays, so the
        // model can ask it where the reader is.
        if rebuild {
            self.rebuild.set(true);
            self.model.change_layout(query);
        } else if query != self.model.query() {
            self.model.set_query(query);
        }
        self.fit();
        self.show_sort();
        self.show_choices();
        self.save_soon();
    }

    /// Shows the sort on the table's headers.
    pub(super) fn show_sort(&self) {
        if let Some((_, View::Table(table))) = &*self.view.borrow() {
            table.show_sort(&self.sort());
        }
    }

    /// Sets each control to the choice it stands for.
    pub(super) fn show_choices(&self) {
        let settings = self.settings.borrow();
        let sort = settings.sort(self.shown());
        let state = |name: &str, value: glib::Variant| {
            if let Some(action) = self
                .window
                .lookup_action(name)
                .and_downcast::<gio::SimpleAction>()
            {
                action.set_state(&value);
            }
        };
        state("view", settings.view.id().to_variant());
        state("cover-size", settings.cover_size.id().to_variant());
        state("sort-field", sort.first().field.id().to_variant());
        state(
            "sort-direction",
            if sort.first().descending {
                "descending"
            } else {
                "ascending"
            }
            .to_variant(),
        );
        for column in &settings.columns {
            state(
                &format!("column-{}", column.field.id()),
                column.visible.to_variant(),
            );
        }
        if let Some(editor) = &*self.controls.editor.borrow() {
            editor.setting.set(true);
            for (n, (field, direction)) in editor.rows.iter().enumerate() {
                let key = sort.keys().get(n);
                let at = key.map_or(0, |key| {
                    SortField::ALL
                        .iter()
                        .position(|f| f.0 == key.field)
                        .unwrap_or(0)
                        + usize::from(n > 0)
                });
                field.set_selected(at as u32);
                direction.set_selected(u32::from(key.is_some_and(|key| key.descending)));
                direction.set_sensitive(key.is_some());
            }
            editor.setting.set(false);
        }
    }

    /// The window crossed the width under which the table is not offered.
    pub(super) fn set_narrow(self: &Rc<Self>, narrow: bool) {
        if self.narrow.replace(narrow) == narrow {
            return;
        }
        if let Some(action) = self
            .window
            .lookup_action("view-table")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(!narrow);
        }
        self.controls.switch.set_visible(!narrow);
        self.controls.switch_menu.set_visible(narrow);
        self.show(false);
    }

    /// Opens the sort editor: three rows, each a field and a direction.
    fn edit_sort(self: &Rc<Self>) {
        if self.controls.editor.borrow().is_none() {
            let setting = Rc::new(Cell::new(false));
            let grid = gtk::Grid::builder()
                .row_spacing(8)
                .column_spacing(8)
                .margin_top(12)
                .margin_bottom(18)
                .margin_start(18)
                .margin_end(18)
                .build();
            let names: Vec<&str> = SortField::ALL.iter().map(|f| f.2).collect();
            let mut rows = vec![];
            for (n, (label, spoken)) in [
                ("Sort by", "First"),
                ("Then by", "Second"),
                ("Then by", "Third"),
            ]
            .into_iter()
            .enumerate()
            {
                let label = gtk::Label::builder().label(label).xalign(0.0).build();
                // A second and third key are optional.
                let fields: Vec<&str> = if n == 0 {
                    names.clone()
                } else {
                    std::iter::once("None")
                        .chain(names.iter().copied())
                        .collect()
                };
                let field = gtk::DropDown::from_strings(&fields);
                field.update_property(&[gtk::accessible::Property::Label(&format!(
                    "{spoken} sort field"
                ))]);
                let direction = gtk::DropDown::from_strings(&["Ascending", "Descending"]);
                direction.update_property(&[gtk::accessible::Property::Label(&format!(
                    "{spoken} sort direction"
                ))]);
                grid.attach(&label, 0, n as i32, 1, 1);
                grid.attach(&field, 1, n as i32, 1, 1);
                grid.attach(&direction, 2, n as i32, 1, 1);
                rows.push((field, direction));
            }
            for (field, direction) in &rows {
                for dropdown in [field, direction] {
                    let (weak, setting) = (Rc::downgrade(self), setting.clone());
                    dropdown.connect_selected_notify(move |_| {
                        if !setting.get()
                            && let Some(library) = weak.upgrade()
                        {
                            library.sort_from_editor();
                        }
                    });
                }
            }
            // A dialog and not a popover: a drop-down's own list is a
            // popover, and choosing from it closes a popover it sits in.
            let page = adw::ToolbarView::new();
            page.add_top_bar(&adw::HeaderBar::new());
            page.set_content(Some(&grid));
            let dialog = adw::Dialog::builder().title("Sort").child(&page).build();
            self.controls.editor.replace(Some(Editor {
                dialog,
                rows,
                setting,
            }));
        }
        self.show_choices();
        if let Some(editor) = &*self.controls.editor.borrow() {
            editor.dialog.present(Some(&self.window));
            editor.rows[0].0.grab_focus();
        }
    }

    fn sort_from_editor(self: &Rc<Self>) {
        let keys: Vec<SortKey> = {
            let editor = self.controls.editor.borrow();
            let Some(editor) = &*editor else { return };
            editor
                .rows
                .iter()
                .enumerate()
                .filter_map(|(n, (field, direction))| {
                    // After the first row, the first choice is None.
                    let at = (field.selected() as usize).checked_sub(usize::from(n > 0))?;
                    Some(SortKey {
                        field: SortField::ALL.get(at)?.0,
                        descending: direction.selected() == 1,
                    })
                })
                .collect()
        };
        self.set_sort(keys.into());
    }

    fn choose_view(self: &Rc<Self>, id: &str) {
        if let Some(kind) = ViewKind::from_id(id)
            && (kind != ViewKind::Table || !self.narrow.get())
        {
            self.settings.borrow_mut().view = kind;
            self.show(false);
        }
    }

    fn with_table(&self, act: impl FnOnce(&table::Table)) {
        if let Some((_, View::Table(table))) = &*self.view.borrow() {
            act(table);
        }
    }

    pub(super) fn install_controls(self: &Rc<Self>) {
        let settings = self.settings.borrow().clone();
        let chosen = |name: &str, initial: &str, choose: fn(&Rc<Self>, &str)| {
            let action = gio::SimpleAction::new_stateful(
                name,
                Some(glib::VariantTy::STRING),
                &initial.to_variant(),
            );
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, target| {
                if let (Some(library), Some(target)) =
                    (weak.upgrade(), target.and_then(|target| target.str()))
                {
                    choose(&library, target);
                }
            });
            self.window.add_action(&action);
        };
        chosen("view", settings.view.id(), Self::choose_view);
        let table = gio::SimpleAction::new("view-table", None);
        let weak = Rc::downgrade(self);
        table.connect_activate(move |_, _| {
            if let Some(library) = weak.upgrade() {
                library.choose_view(ViewKind::Table.id());
            }
        });
        self.window.add_action(&table);
        chosen("cover-size", settings.cover_size.id(), |library, id| {
            if let Some(size) = CoverSize::from_id(id) {
                library.settings.borrow_mut().cover_size = size;
                library.show(true);
            }
        });
        chosen("sort-field", "title", |library, id| {
            // The field alone, in the direction now chosen.
            if let Some(field) = SortField::from_id(id) {
                let descending = library.sort().first().descending;
                library.set_sort(vec![SortKey { field, descending }].into());
            }
        });
        chosen("sort-direction", "ascending", |library, id| {
            let sort = library.sort().with_direction(id == "descending");
            library.set_sort(sort);
        });
        for (name, action) in [
            ("column-start", table::Change::Move(false)),
            ("column-end", table::Change::Move(true)),
            ("column-wider", table::Change::Resize(24)),
            ("column-narrower", table::Change::Resize(-24)),
        ] {
            let command = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let weak = Rc::downgrade(self);
            command.connect_activate(move |_, target| {
                if let (Some(library), Some(field)) = (
                    weak.upgrade(),
                    target
                        .and_then(|target| target.str())
                        .and_then(SortField::from_id),
                ) {
                    library.with_table(|table| table.change(field, action));
                }
            });
            self.window.add_action(&command);
        }
        let more = gio::SimpleAction::new("sort-more", None);
        let weak = Rc::downgrade(self);
        more.connect_activate(move |_, _| {
            if let Some(library) = weak.upgrade() {
                library.edit_sort();
            }
        });
        self.window.add_action(&more);
        for (field, id, _) in SortField::ALL {
            // Title cannot be removed.
            if field == SortField::Title {
                continue;
            }
            let action =
                gio::SimpleAction::new_stateful(&format!("column-{id}"), None, &true.to_variant());
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(library) = weak.upgrade() {
                    library.with_table(|table| table.change(field, table::Change::Toggle));
                }
            });
            self.window.add_action(&action);
        }
        self.show_choices();
    }
}

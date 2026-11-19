//! The Edit window: the title and authors a book is shown by, and how it is
//! filed: its sort title, each author's sort name, and its series.
use super::*;
use ebook_library::Book;

type Series = Vec<(String, Option<String>)>;

/// What the form holds, as text.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    title: String,
    authors: String,
    sort_title: String,
    author_sorts: String,
    series: String,
    position: String,
}

/// What saving a form changes in the record. `None` leaves a part alone.
#[derive(Debug, PartialEq)]
struct Changes {
    names: Option<(String, Vec<String>)>,
    filing: Option<Filing>,
}

/// The reader's own filing. A `None` field files by what the file says.
#[derive(Debug, PartialEq)]
struct Filing {
    sort_title: Option<String>,
    author_sorts: Option<Vec<Option<String>>>,
    series: Option<Series>,
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(|line| line.trim().to_owned()).collect()
}

fn filled(text: &str) -> Option<String> {
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}

/// The series a book is filed under: the reader's, else the file's.
fn series_of(book: &Book) -> &Series {
    book.series.as_ref().unwrap_or(&book.imported.series)
}

fn form_for(book: &Book) -> Form {
    let sorts = book
        .author_sorts
        .as_ref()
        .unwrap_or(&book.imported.author_sorts);
    let primary = series_of(book).first();
    Form {
        title: book.title.clone(),
        authors: book.authors.join("\n"),
        sort_title: book.sort_title.clone().unwrap_or_default(),
        author_sorts: (0..book.authors.len())
            .map(|n| sorts.get(n).cloned().flatten().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_owned(),
        series: primary.map(|(name, _)| name.clone()).unwrap_or_default(),
        position: primary
            .and_then(|(_, position)| position.clone())
            .unwrap_or_default(),
    }
}

/// Compares the form with the record. A filing field that matches what the
/// file says is stored as absent, so it follows the file again.
fn changes(book: &Book, form: &Form) -> Changes {
    let authors: Vec<String> = lines(&form.authors)
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
    let title = form.title.trim().to_owned();
    let mut sorts: Vec<Option<String>> = lines(&form.author_sorts)
        .iter()
        .map(|line| filled(line))
        .collect();
    sorts.resize(authors.len(), None);
    let mut imported_sorts = book.imported.author_sorts.clone();
    imported_sorts.resize(authors.len(), None);
    // The form edits the primary series and keeps any others.
    let mut series: Series = filled(&form.series)
        .map(|name| (name, filled(&form.position)))
        .into_iter()
        .collect();
    series.extend(series_of(book).iter().skip(1).cloned());
    let filing = Filing {
        sort_title: filled(&form.sort_title),
        author_sorts: (sorts != imported_sorts).then_some(sorts),
        series: (series != book.imported.series).then_some(series),
    };
    let current = Filing {
        sort_title: book.sort_title.clone(),
        author_sorts: book.author_sorts.clone(),
        series: book.series.clone(),
    };
    Changes {
        names: (title != book.title || authors != book.authors).then_some((title, authors)),
        filing: (filing != current).then_some(filing),
    }
}

fn field(
    form: &gtk::Box,
    name: &str,
    hint: &str,
    widget: &(impl IsA<gtk::Widget> + IsA<gtk::Accessible>),
) {
    let heading = label(name, "heading");
    heading.set_margin_top(6);
    form.append(&heading);
    form.append(widget);
    widget.update_property(&[gtk::accessible::Property::Label(name)]);
    if !hint.is_empty() {
        let hint = label(hint, "caption");
        hint.add_css_class("dimmed");
        form.append(&hint);
    }
}

fn entry(text: &str) -> gtk::Entry {
    gtk::Entry::builder()
        .text(text)
        .width_chars(40)
        .activates_default(true)
        .build()
}

fn several_lines(text: &str) -> gtk::TextView {
    // Tab moves on to the next field.
    let view = gtk::TextView::builder()
        .accepts_tab(false)
        .height_request(56)
        .build();
    view.add_css_class("card");
    view.set_left_margin(8);
    view.set_right_margin(8);
    view.set_top_margin(6);
    view.set_bottom_margin(6);
    view.buffer().set_text(text);
    view
}

fn text_of(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

pub fn open(parent: &impl IsA<gtk::Window>, store: &store::Store, book: Book) {
    let d = gtk::Window::builder()
        .title("Edit book")
        .transient_for(parent)
        .modal(true)
        .build();
    let form = column(4);
    inset(&form, 20);
    d.set_child(Some(&form));
    let shown = form_for(&book);
    let title = entry(&shown.title);
    let authors = several_lines(&shown.authors);
    let sort_title = entry(&shown.sort_title);
    let author_sorts = several_lines(&shown.author_sorts);
    let series = entry(&shown.series);
    let position = entry(&shown.position);
    field(&form, "Title", "", &title);
    field(&form, "Authors", "One on each line.", &authors);
    field(
        &form,
        "Sort title",
        "Where the book is filed by title. Leave empty to file by the title.",
        &sort_title,
    );
    field(
        &form,
        "Author sort names",
        "One on each line, in the order of the authors, such as “Melville, Herman”. An empty line files by the name as written.",
        &author_sorts,
    );
    field(&form, "Series", "", &series);
    field(
        &form,
        "Position in series",
        "A number or other text, such as 2, 1.5 or TPB1.",
        &position,
    );
    let actions = row(8);
    actions.set_margin_top(12);
    actions.set_halign(gtk::Align::End);
    form.append(&actions);
    let weak = d.downgrade();
    actions.append(&button("Cancel", move || {
        if let Some(d) = weak.upgrade() {
            d.close()
        }
    }));
    let store = store.clone();
    let parent: gtk::Window = parent.clone().upcast();
    let weak = d.downgrade();
    let save = button("Save", move || {
        let Some(d) = weak.upgrade() else { return };
        let form = Form {
            title: title.text().to_string(),
            authors: text_of(&authors),
            sort_title: sort_title.text().to_string(),
            author_sorts: text_of(&author_sorts),
            series: series.text().to_string(),
            position: position.text().to_string(),
        };
        if form.title.trim().is_empty() {
            notice(
                &d,
                "Enter a title",
                "A title helps you find this book again.",
            );
            return;
        }
        let changes = changes(&book, &form);
        let id = book.id.clone();
        let store = store.clone();
        let parent = parent.clone();
        d.set_sensitive(false);
        glib::spawn_future_local(async move {
            let saved = store
                .run(move |library| {
                    if let Some((title, authors)) = &changes.names {
                        library.edit(&id, title, authors)?;
                    }
                    if let Some(filing) = changes.filing {
                        library.edit_filing(
                            &id,
                            filing.sort_title,
                            filing.author_sorts,
                            filing.series,
                        )?;
                    }
                    Ok(())
                })
                .await;
            match saved {
                Ok(()) => d.close(),
                Err(e) => {
                    if let Some(detail) = publication_message(&e) {
                        d.close();
                        notice(&parent, "Changes saved locally", &detail);
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
    d.set_default_widget(Some(&save));
    let escape = gtk::ShortcutController::new();
    escape.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::NamedAction::new("window.close")),
    ));
    d.add_controller(escape);
    d.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebook_library::ParsedMetadata;

    fn book() -> Book {
        Book {
            id: String::new(),
            edition_id: String::new(),
            file_id: String::new(),
            version_id: String::new(),
            title: "The Whale".into(),
            authors: vec!["Herman Melville".into(), "A. Editor".into()],
            imported: ParsedMetadata {
                title: "The Whale".into(),
                authors: vec!["Herman Melville".into(), "A. Editor".into()],
                author_sorts: vec![Some("Melville, Herman".into()), None],
                series: vec![("Sea".into(), Some("2".into())), ("Classics".into(), None)],
                ..Default::default()
            },
            sha256: String::new(),
            added: None,
            sort_title: None,
            author_sorts: None,
            series: None,
        }
    }

    #[test]
    fn a_form_saved_untouched_changes_nothing() {
        let book = book();
        assert_eq!(
            changes(&book, &form_for(&book)),
            Changes {
                names: None,
                filing: None
            }
        );
    }

    #[test]
    fn the_form_shows_what_the_file_says_until_the_reader_files_it_otherwise() {
        let mut book = book();
        let form = form_for(&book);
        assert_eq!(form.author_sorts, "Melville, Herman");
        assert_eq!((form.series.as_str(), form.position.as_str()), ("Sea", "2"));
        book.series = Some(vec![("Ocean".into(), Some("TPB1".into()))]);
        book.sort_title = Some("Whale".into());
        let form = form_for(&book);
        assert_eq!(
            (form.series.as_str(), form.position.as_str()),
            ("Ocean", "TPB1")
        );
        assert_eq!(form.sort_title, "Whale");
    }

    #[test]
    fn filing_fields_are_stored_only_where_they_differ_from_the_file() {
        let book = book();
        let mut form = form_for(&book);
        form.sort_title = " Whale ".into();
        form.author_sorts = "Melville, H.\nEditor, A.".into();
        form.series = "Ocean".into();
        form.position = "1.5".into();
        assert_eq!(
            changes(&book, &form).filing,
            Some(Filing {
                sort_title: Some("Whale".into()),
                author_sorts: Some(vec![Some("Melville, H.".into()), Some("Editor, A.".into())]),
                // The second series the file names is kept.
                series: Some(vec![
                    ("Ocean".into(), Some("1.5".into())),
                    ("Classics".into(), None)
                ]),
            })
        );
    }

    #[test]
    fn restoring_the_files_values_removes_the_readers_filing() {
        let mut book = book();
        book.sort_title = Some("Whale".into());
        book.author_sorts = Some(vec![None, None]);
        book.series = Some(vec![]);
        let mut form = form_for(&book);
        form.sort_title.clear();
        form.author_sorts = "Melville, Herman".into();
        form.series = "Sea".into();
        form.position = "2".into();
        // The file's second series comes back only through its first, so
        // the reader's list is still their own.
        assert_eq!(
            changes(&book, &form).filing,
            Some(Filing {
                sort_title: None,
                author_sorts: None,
                series: Some(vec![("Sea".into(), Some("2".into()))]),
            })
        );
    }

    #[test]
    fn clearing_the_series_takes_the_book_out_of_it() {
        let book = book();
        let mut form = form_for(&book);
        form.series.clear();
        assert_eq!(
            changes(&book, &form)
                .filing
                .and_then(|filing| filing.series),
            Some(vec![("Classics".into(), None)])
        );
    }

    #[test]
    fn a_changed_title_or_author_list_is_an_edit_of_its_own() {
        let book = book();
        let mut form = form_for(&book);
        form.title = "Moby-Dick ".into();
        form.authors = "Herman Melville\n\n".into();
        let changes = changes(&book, &form);
        assert_eq!(
            changes.names,
            Some(("Moby-Dick".into(), vec!["Herman Melville".into()]))
        );
        // With one author left, the file's first sort name still applies.
        assert_eq!(changes.filing, None);
    }
}

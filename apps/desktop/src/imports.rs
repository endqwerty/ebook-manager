use super::*;
use anyhow::{Context, ensure};
use ebook_library::{ImportChoice, ImportOutcome, ParsedMetadata};
use ebook_reader::{Reader, ReaderConfig};
use std::{
    cell::Cell,
    fs,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
};
use tempfile::NamedTempFile;

struct Candidate {
    staged: NamedTempFile,
    source: PathBuf,
    metadata: ParsedMetadata,
}

fn stage(source: PathBuf) -> Result<Candidate> {
    let mut staged = NamedTempFile::new()?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&source)?;
    ensure!(file.metadata()?.is_file(), "Choose a regular EPUB file");
    let max = ebook_reader::Limits::default().max_file_bytes;
    let bytes = io::copy(&mut file.take(max + 1), staged.as_file_mut())?;
    ensure!(bytes <= max, "EPUB exceeds the reader’s file size limit");
    let reader = Reader::new(ReaderConfig::default())?;
    let m = reader.inspect(staged.path())?;
    ensure!(
        !m.protected,
        "This EPUB is protected. Choose a DRM-free book."
    );
    let metadata = ParsedMetadata {
        title: m.title.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
            source
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        authors: m.authors.into_iter().map(|a| a.name).collect(),
        identifier: m.identifiers.into_iter().find(|i| i.is_unique_id).map(|i| {
            let scheme = i.scheme.unwrap_or_default();
            format!("{}:{scheme}{}", scheme.len(), i.value)
        }),
    };
    Ok(Candidate {
        staged,
        source,
        metadata,
    })
}

fn discover(paths: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut dirs = Vec::new();
    for p in paths {
        if p.is_dir() {
            dirs.push(p)
        } else {
            files.push(p)
        }
    }
    while let Some(p) = dirs.pop() {
        let entries = match fs::read_dir(&p) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("{} · {e}", p.display()));
                continue;
            }
        };
        for item in entries {
            let item = item.and_then(|e| Ok((e.file_type()?, e.path())));
            match item {
                Ok((kind, path)) if kind.is_dir() => dirs.push(path),
                Ok((kind, path))
                    if (kind.is_file() || kind.is_symlink() && !path.is_dir())
                        && path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("epub")) =>
                {
                    files.push(path)
                }
                Err(e) => errors.push(format!("{} · {e}", p.display())),
                _ => {}
            }
        }
    }
    files.sort();
    (files, errors)
}

async fn review(parent: &gtk::Window, source: &Path, books: Vec<Book>) -> Option<ImportChoice> {
    use libadwaita::prelude::{AlertDialogExt, AlertDialogExtManual};
    let d = libadwaita::AlertDialog::builder()
        .heading("Review changed copy")
        .body(format!(
            "{} may be another version of a book on your shelf.",
            source.file_name()?.to_string_lossy()
        ))
        .build();
    d.add_responses(&[
        ("skip", "Skip"),
        ("keep", "Keep separately"),
        ("replace", "Replace selected"),
    ]);
    d.set_close_response("skip");
    let names: Vec<_> = books
        .iter()
        .map(|b| {
            format!(
                "{} · {} · {}",
                b.title,
                b.authors.join(", "),
                &b.sha256[..8]
            )
        })
        .collect();
    let refs: Vec<_> = names.iter().map(String::as_str).collect();
    let list = gtk::DropDown::from_strings(&refs);
    d.set_extra_child(Some(&list));
    let response = d.choose_future(Some(parent)).await;
    match response.as_str() {
        "keep" => Some(ImportChoice::KeepSeparate),
        "replace" => Some(ImportChoice::Replace(
            books.get(list.selected() as usize)?.id.clone(),
        )),
        _ => None,
    }
}

async fn import(s: &Shelf, parent: &gtk::Window, path: PathBuf) -> Result<String> {
    let candidate = gio::spawn_blocking(move || stage(path))
        .await
        .map_err(|_| anyhow::anyhow!("Inspection interrupted"))??;
    let mut choice = ImportChoice::Review;
    let candidate = std::sync::Arc::new(candidate);
    loop {
        let c = candidate.clone();
        match s
            .store
            .run(move |lib| lib.import(c.staged.path(), &c.source, &c.metadata, choice))
            .await?
        {
            ImportOutcome::Added(b) => return Ok(format!("Added · {}", b.title)),
            ImportOutcome::Duplicate(b) => return Ok(format!("Duplicate · {}", b.title)),
            ImportOutcome::Replaced(b) => return Ok(format!("Replaced · {}", b.title)),
            ImportOutcome::NeedsReview(matches) => {
                match review(parent, &candidate.source, matches).await {
                    Some(c) => choice = c,
                    None => return Ok("Skipped".into()),
                }
            }
        }
    }
}

pub fn start(s: &Shelf, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let d = gtk::Window::builder()
        .title("Import results")
        .transient_for(&s.window)
        .default_width(540)
        .default_height(390)
        .build();
    let content = column(12);
    inset(&content, 20);
    d.set_child(Some(&content));
    let progress = label("Finding EPUBs…", "title-2");
    content.append(&progress);
    let list = column(8);
    content.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build(),
    );
    let stop = gtk::Button::with_label("Stop after current book");
    content.append(&stop);
    let cancelled = Rc::new(Cell::new(false));
    let c = cancelled.clone();
    stop.connect_clicked(move |b| {
        c.set(true);
        b.set_sensitive(false);
    });
    d.set_deletable(false);
    d.present();
    let s = s.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || discover(paths))
            .await
            .ok()
            .context("Folder scan interrupted");
        if let Ok((paths, errors)) = result {
            for err in errors {
                list.append(&label(&err, "error"));
            }
            for (index, path) in paths.iter().enumerate() {
                if cancelled.get() {
                    break;
                }
                progress.set_text(&format!("Importing {} of {}", index + 1, paths.len()));
                let res = import(&s, &d, path.clone()).await.unwrap_or_else(|e| {
                    publication_message(&e).unwrap_or_else(|| format!("Failed · {e:#}"))
                });
                list.append(&label(
                    &format!(
                        "{}\n{}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        res
                    ),
                    "body",
                ));
            }
            if paths.is_empty() {
                list.append(&label("No EPUB files found.", "dim-label"));
            }
        } else {
            list.append(&label("Folder scan interrupted", "error"));
        }
        progress.set_text(if cancelled.get() {
            "Import stopped"
        } else {
            "Import complete"
        });
        s.reload();
        content.remove(&stop);
        let window = d.downgrade();
        content.append(&button("Done", move || {
            if let Some(w) = window.upgrade() {
                w.close();
            }
        }));
        d.set_deletable(true);
    });
}

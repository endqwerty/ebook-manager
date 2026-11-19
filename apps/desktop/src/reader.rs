use super::*;
use ebook_library::ReadingPosition;
use ebook_reader::{Position, Reader, ReaderConfig};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

pub fn open(s: &Shelf, b: &Rc<Book>) {
    let w = gtk::Window::builder()
        .title(&b.title)
        .transient_for(&s.window)
        .modal(true)
        .default_width(760)
        .default_height(640)
        .build();
    let layout = column(12);
    inset(&layout, 16);
    w.set_child(Some(&layout));
    let status = label("Opening book…", "heading");
    layout.append(&status);
    w.present();
    let s = s.clone();
    let b = b.clone();
    glib::spawn_future_local(async move {
        let id = b.id.clone();
        let result = async {
            let (path, saved) = s
                .store
                .run(move |lib| Ok((lib.managed_file(&id)?, lib.load_position(&id)?)))
                .await?;
            let book = gio::spawn_blocking(move || -> Result<_> {
                Ok(Reader::new(ReaderConfig::default())?.open(&path)?)
            })
            .await
            .map_err(|_| anyhow::anyhow!("Reader interrupted"))??;
            anyhow::ensure!(!book.chapters().is_empty(), "No readable chapters");
            let pos = saved.filter(|p| p.version_id == b.version_id).map_or(
                Position {
                    chapter: 0,
                    block: 0,
                },
                |p| Position {
                    chapter: p.chapter,
                    block: p.offset,
                },
            );
            let pos = book.clamp(pos);
            Ok::<_, anyhow::Error>((book, pos))
        }
        .await;
        let (book, pos) = match result {
            Ok(x) => x,
            Err(e) => {
                status.set_text(&format!("Couldn’t open book · {e:#}"));
                return;
            }
        };
        if !w.is_visible() {
            return;
        }
        layout.remove(&status);
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
        picker.set_selected(pos.chapter);
        picker.set_hexpand(true);
        let bar = row(8);
        layout.append(&bar);
        let previous = gtk::Button::with_label("Previous");
        let next = gtk::Button::with_label("Next");
        bar.append(&previous);
        bar.append(&picker);
        bar.append(&next);
        let text = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::WordChar)
            .left_margin(32)
            .right_margin(32)
            .top_margin(24)
            .bottom_margin(28)
            .pixels_below_lines(8)
            .build();
        text.add_css_class("reading-text");
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&text)
            .build();
        layout.append(&scroll);
        let footer = row(12);
        layout.append(&footer);
        status.set_hexpand(true);
        footer.append(&status);
        let weak = w.downgrade();
        footer.append(&button("Back to library", move || {
            if let Some(w) = weak.upgrade() {
                w.close()
            }
        }));
        let view = Rc::new(View {
            book: Arc::new(book),
            window: w.downgrade(),
            model: b,
            store: s.store.clone(),
            text,
            picker: picker.clone(),
            previous: previous.clone(),
            next: next.clone(),
            status,
            offsets: RefCell::new(vec![]),
            shown: Cell::new(None),
            generation: Cell::new(0),
        });
        let weak = Rc::downgrade(&view);
        picker.connect_selected_notify(move |p| {
            if let Some(v) = weak.upgrade() {
                v.load(p.selected(), 0);
            }
        });
        let p = picker.downgrade();
        previous.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.set_selected(p.selected().saturating_sub(1))
            }
        });
        next.connect_clicked(move |_| picker.set_selected(picker.selected() + 1));
        let v = view.clone();
        w.connect_close_request(move |w| {
            let Some(pos) = v.position() else {
                return glib::Propagation::Proceed;
            };
            let w = w.clone();
            let store = v.store.clone();
            let id = v.model.id.clone();
            w.set_sensitive(false);
            glib::spawn_future_local(async move {
                let result = store.save_position(id, pos).await;
                w.set_sensitive(true);
                match result {
                    Ok(()) => w.destroy(),
                    Err(e) => {
                        let parent = w.transient_for();
                        w.destroy();
                        if let Some(parent) = parent {
                            position_error(&parent, &e);
                        }
                    }
                }
            });
            glib::Propagation::Stop
        });
        view.load(pos.chapter, pos.block);
    });
}

fn position_error(w: &gtk::Window, e: &anyhow::Error) {
    if let Some(detail) = publication_message(e) {
        notice(w, "Position saved locally", &detail);
    } else {
        notice(w, "Couldn’t save reading position", &e.to_string());
    }
}

struct View {
    book: Arc<ebook_reader::Book>,
    window: glib::WeakRef<gtk::Window>,
    model: Rc<Book>,
    store: store::Store,
    text: gtk::TextView,
    picker: gtk::DropDown,
    previous: gtk::Button,
    next: gtk::Button,
    status: gtk::Label,
    offsets: RefCell<Vec<i32>>,
    shown: Cell<Option<u32>>,
    generation: Cell<u64>,
}
impl View {
    fn position(&self) -> Option<ReadingPosition> {
        let chapter = self.shown.get()?;
        let (iter, _) = self.text.line_at_y(self.text.visible_rect().y());
        let block = self
            .offsets
            .borrow()
            .partition_point(|p| *p <= iter.offset())
            .saturating_sub(1) as u32;
        Some(ReadingPosition {
            version_id: self.model.version_id.clone(),
            chapter,
            offset: block,
        })
    }
    fn persist(&self, chapter: u32, offset: u32) {
        let id = self.model.id.clone();
        let pos = ReadingPosition {
            version_id: self.model.version_id.clone(),
            chapter,
            offset,
        };
        let store = self.store.clone();
        let window = self.window.clone();
        glib::spawn_future_local(async move {
            if let Err(e) = store.save_position(id, pos).await
                && let Some(w) = window.upgrade()
            {
                position_error(&w, &e);
            }
        });
    }
    fn load(self: &Rc<Self>, index: u32, block: u32) {
        self.shown.set(None);
        let seq = self.generation.get() + 1;
        self.generation.set(seq);
        self.previous.set_sensitive(index > 0);
        self.next
            .set_sensitive((index as usize) + 1 < self.book.chapters().len());
        self.status.set_text("Loading chapter…");
        self.text.buffer().set_text("");
        let book = self.book.clone();
        let v = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || -> Result<_> {
                let chapter = book.read_chapter(index)?;
                let partial = chapter.truncated || !chapter.warnings.is_empty();
                let mut body = String::new();
                let mut offsets = vec![];
                let mut at = 0;
                for block in chapter.blocks {
                    offsets.push(at);
                    for span in block.spans {
                        at += span.text.chars().count() as i32;
                        body.push_str(&span.text);
                    }
                    body.push('\n');
                    at += 1;
                }
                Ok((body, offsets, partial))
            })
            .await;
            if v.generation.get() != seq || v.window.upgrade().is_none_or(|w| !w.is_visible()) {
                return;
            }
            match result {
                Ok(Ok((body, offsets, partial))) => {
                    let block = (block as usize).min(offsets.len().saturating_sub(1));
                    let buf = v.text.buffer();
                    buf.set_text(&body);
                    let iter = buf.iter_at_offset(offsets.get(block).copied().unwrap_or(0));
                    buf.move_mark(&buf.get_insert(), &iter);
                    v.text
                        .scroll_to_mark(&buf.get_insert(), 0.0, true, 0.0, 0.0);
                    v.offsets.replace(offsets);
                    v.shown.set(Some(index));
                    v.persist(index, block as u32);
                    v.status.set_text(&format!(
                        "Chapter {} of {}{}",
                        v.picker.selected() + 1,
                        v.book.chapters().len(),
                        if partial {
                            " · Some content could not be displayed"
                        } else {
                            ""
                        }
                    ));
                }
                Ok(Err(e)) => v.status.set_text(&format!("Couldn’t read chapter · {e:#}")),
                Err(_) => v.status.set_text("Chapter loading interrupted"),
            }
        });
    }
}

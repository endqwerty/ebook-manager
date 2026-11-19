use super::*;
use ebook_library::ReadingPosition;
use ebook_reader::{Position, Reader, ReaderConfig};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

pub fn open_in(parent: &impl IsA<gtk::Window>, store: &store::Store, b: &Rc<Book>) {
    let w = gtk::Window::builder()
        .title(&b.title)
        .transient_for(parent)
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
        let scroll = gtk::ScrolledWindow::builder().child(&text).build();
        // Full screen would otherwise stretch each line across the monitor.
        let measure = libadwaita::Clamp::builder()
            .maximum_size(760)
            .tightening_threshold(760)
            .vexpand(true)
            .child(&scroll)
            .build();
        layout.append(&measure);
        let footer = row(12);
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
        let view = Rc::new(View {
            book: Arc::new(book),
            window: w.downgrade(),
            model: b,
            store,
            text,
            picker: picker.clone(),
            arrival: Cell::default(),
            previous: previous.clone(),
            next: next.clone(),
            status,
            offsets: RefCell::new(vec![]),
            shown: Cell::new(None),
            pending_save: Cell::new(None),
            generation: Cell::new(0),
        });
        let weak = Rc::downgrade(&view);
        scroll.vadjustment().connect_value_changed(move |_| {
            if let Some(v) = weak.upgrade() {
                v.queue_scroll_save();
            }
        });
        let weak = Rc::downgrade(&view);
        picker.connect_selected_notify(move |p| {
            if let Some(v) = weak.upgrade() {
                v.load(p.selected(), v.arrival.take());
            }
        });
        let weak = Rc::downgrade(&view);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, key, _, modifiers| match weak.upgrade() {
            Some(v) if v.key(key, modifiers) => glib::Propagation::Stop,
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
        view.load(pos.chapter, Landing::Block(pos.block));
    });
}

fn toggle_full_screen(w: &gtk::Window) {
    if w.is_fullscreen() {
        w.unfullscreen()
    } else {
        w.fullscreen()
    }
}

/// Where a chapter opens.
#[derive(Clone, Copy)]
enum Landing {
    /// With this block at the top of the page.
    Block(u32),
    /// On its last page.
    End,
}
impl Default for Landing {
    fn default() -> Self {
        Self::Block(0)
    }
}

#[derive(Clone, Copy)]
enum Turn {
    Back,
    Forward,
}

/// The top of the page after the one showing `top..top + height`: the first
/// line that does not fit, so no line is skipped or cut. `None` when every
/// line from `lines` (top and height of each, in order) already fits.
fn next_page_top(
    lines: impl IntoIterator<Item = (i32, i32)>,
    top: i32,
    height: i32,
) -> Option<i32> {
    let (y, _) = lines.into_iter().find(|(y, h)| y + h > top + height)?;
    Some(if y > top { y } else { top + height })
}

/// The top of the page before the one showing from `top`. It ends with the
/// last line above the first whole line now showing, so a line cut by `top`
/// is whole on it. It starts on a whole line, or at `start`, the top of the
/// chapter above its margin. `lines` must reach one line past `top`.
fn previous_page_top(lines: &[(i32, i32)], start: i32, top: i32, height: i32) -> i32 {
    let tops = || lines.iter().map(|(y, _)| *y);
    let edge = tops().find(|y| *y >= top).unwrap_or(i32::MAX);
    // The page must reach the bottom of the last line above that one. The
    // space between paragraphs below it can fall off the page.
    let reach = lines
        .iter()
        .take_while(|(y, _)| *y < edge)
        .last()
        .map_or(top, |(y, h)| y + h);
    std::iter::once(start)
        .chain(tops())
        .find(|y| *y >= reach - height)
        .filter(|y| *y < top)
        .unwrap_or(top - height)
}

/// Linear chapters before `chapter` plus the part of it scrolled past, over
/// the number of linear chapters. A chapter that is not linear counts as
/// the start of the next linear one. Exactly 1 at the end of the last.
fn interim_fraction(linear: &[bool], chapter: usize, scrolled: f64) -> f32 {
    let total = linear.iter().filter(|l| **l).count();
    let before = linear.iter().take(chapter).filter(|l| **l).count();
    if total == 0 {
        return 0.0;
    }
    let within = if linear.get(chapter) == Some(&true) {
        scrolled.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if before + 1 == total && within >= 1.0 - 1e-6 {
        return 1.0;
    }
    ((before as f64 + within) / total as f64).min(1.0) as f32
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
    /// Where to land when the picker next changes chapter.
    arrival: Cell<Landing>,
    previous: gtk::Button,
    next: gtk::Button,
    status: gtk::Label,
    offsets: RefCell<Vec<i32>>,
    shown: Cell<Option<u32>>,
    pending_save: Cell<Option<glib::SourceId>>,
    generation: Cell<u64>,
}
impl View {
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
        let reading = focus.is_none_or(|f| f == self.text);
        let back = modifiers.contains(ModifierType::SHIFT_MASK);
        match key {
            Key::Right | Key::KP_Right | Key::Page_Down | Key::KP_Page_Down => {
                self.turn(Turn::Forward)
            }
            Key::Left | Key::KP_Left | Key::Page_Up | Key::KP_Page_Up => self.turn(Turn::Back),
            Key::space if reading && back => self.turn(Turn::Back),
            Key::space if reading => self.turn(Turn::Forward),
            Key::Down | Key::KP_Down if reading => self.nudge(1.0),
            Key::Up | Key::KP_Up if reading => self.nudge(-1.0),
            _ => return false,
        }
        true
    }

    fn nudge(&self, direction: f64) {
        let Some(a) = self.text.vadjustment() else {
            return;
        };
        let max = (a.upper() - a.page_size()).max(a.lower());
        a.set_value((a.value() + direction * a.step_increment().max(40.0)).clamp(a.lower(), max));
    }

    /// Tops and heights of the wrapped lines that touch `from..=to`, in
    /// buffer coordinates.
    fn lines(&self, from: i32, to: i32) -> Vec<(i32, i32)> {
        let (mut iter, _) = self.text.line_at_y(from);
        let mut lines = vec![];
        loop {
            let r = self.text.iter_location(&iter);
            if r.y() > to {
                break;
            }
            if r.y() + r.height() > from {
                lines.push((r.y(), r.height()));
            }
            if !self.text.forward_display_line(&mut iter) {
                break;
            }
        }
        lines
    }

    /// Turns one page, moving to the neighbouring chapter at either end of
    /// this one: forward to its start, back to its end.
    fn turn(&self, turn: Turn) {
        if self.shown.get().is_none() {
            return;
        }
        let Some(a) = self.text.vadjustment() else {
            return;
        };
        let rect = self.text.visible_rect();
        let (top, height) = (rect.y(), rect.height());
        let max = (a.upper() - a.page_size()).max(a.lower());
        let first = self
            .text
            .iter_location(&self.text.buffer().start_iter())
            .y();
        let chapter = self.picker.selected();
        match turn {
            Turn::Forward => {
                let lines = self.lines(top + height - 1, top + 2 * height);
                match next_page_top(lines, top, height) {
                    Some(target) if a.value() < max - 0.5 => {
                        a.set_value((a.value() + f64::from(target - top)).min(max))
                    }
                    _ if (chapter as usize) + 1 < self.book.chapters().len() => {
                        self.picker.set_selected(chapter + 1)
                    }
                    _ => {}
                }
            }
            Turn::Back if top <= first => {
                if chapter > 0 {
                    self.arrival.set(Landing::End);
                    self.picker.set_selected(chapter - 1);
                }
            }
            Turn::Back => {
                let start = top - (a.value() - a.lower()).round() as i32;
                let lines = self.lines(top - height, top + height);
                let target = previous_page_top(&lines, start, top, height);
                a.set_value((a.value() + f64::from(target - top)).max(a.lower()));
            }
        }
    }

    /// How far through the book the page on screen is. Interim: every
    /// linear chapter counts the same, whatever its length.
    fn fraction(&self, chapter: u32) -> f32 {
        let linear: Vec<_> = self.book.chapters().iter().map(|c| c.linear).collect();
        let scrolled = self.text.vadjustment().map_or(0.0, |a| {
            let range = a.upper() - a.lower() - a.page_size();
            if range < 0.5 {
                1.0
            } else {
                (a.value() - a.lower()) / range
            }
        });
        interim_fraction(&linear, chapter as usize, scrolled)
    }
    fn position(&self) -> Option<ReadingPosition> {
        let chapter = self.shown.get()?;
        let (iter, _) = self.text.line_at_y(self.text.visible_rect().y());
        let block = self
            .offsets
            .borrow()
            .partition_point(|p| *p <= iter.offset())
            .saturating_sub(1) as u32;
        Some(ReadingPosition {
            last_read: Some(glib::DateTime::now_utc().ok()?.to_unix()),
            fraction: Some(self.fraction(chapter)),
            version_id: self.model.version_id.clone(),
            chapter,
            offset: block,
        })
    }
    fn queue_scroll_save(self: &Rc<Self>) {
        if let Some(timer) = self.pending_save.take() {
            timer.remove();
        }
        if self.shown.get().is_none() {
            return;
        }
        let weak = Rc::downgrade(self);
        let timer =
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                if let Some(v) = weak.upgrade() {
                    v.pending_save.take();
                    if let Some(p) = v.position() {
                        v.persist(p.chapter, p.offset);
                    }
                }
            });
        self.pending_save.set(Some(timer));
    }

    fn persist(&self, chapter: u32, offset: u32) {
        let id = self.model.id.clone();
        let pos = ReadingPosition {
            last_read: glib::DateTime::now_utc().ok().map(|t| t.to_unix()),
            fraction: Some(self.fraction(chapter)),
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
    // A freshly filled TextView can still have the empty buffer's scroll range.
    // Set the adjustment after layout, then verify the viewport on a later frame.
    /// `target` is the block to put at the top and its buffer offset, or
    /// `None` for the chapter's last page.
    fn restore(self: &Rc<Self>, seq: u64, chapter: u32, target: Option<(u32, i32)>, partial: bool) {
        let weak = Rc::downgrade(self);
        self.text.add_tick_callback(move |text, _| {
            let Some(v) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if v.generation.get() != seq {
                return glib::ControlFlow::Break;
            }
            let adjustment = text
                .vadjustment()
                .expect("scrolled text has a vertical adjustment");
            let rect = text.visible_rect();
            let buffer = text.buffer();
            let anchor = target.map_or_else(|| buffer.end_iter(), |t| buffer.iter_at_offset(t.1));
            let (y, height) = text.line_yrange(&anchor);
            if rect.height() <= 0 || adjustment.page_size() <= 0.0 || height <= 0 {
                return glib::ControlFlow::Continue;
            }
            let max = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
            let Some((block, _)) = target else {
                if adjustment.value() < max - 0.5 {
                    adjustment.set_value(max);
                    return glib::ControlFlow::Continue;
                }
                if text.iter_location(&anchor).y() > rect.y() + rect.height() {
                    return glib::ControlFlow::Continue;
                }
                v.shown.set(Some(chapter));
                if let Some(p) = v.position() {
                    v.persist(chapter, p.offset);
                }
                v.show_chapter(chapter, partial);
                return glib::ControlFlow::Break;
            };
            let delta = f64::from(y - rect.y());
            let aligned = delta.abs() <= 1.0;
            let last_page = adjustment.value() >= max - 0.5
                && y >= rect.y()
                && y < rect.y() + rect.height()
                && text.line_yrange(&text.buffer().end_iter()).0 <= rect.y() + rect.height();
            if block > 0 && !aligned && !last_page {
                adjustment.set_value((adjustment.value() + delta).clamp(adjustment.lower(), max));
                return glib::ControlFlow::Continue;
            }
            if block == 0 && adjustment.value() > adjustment.lower() + 0.5 {
                adjustment.set_value(adjustment.lower());
                return glib::ControlFlow::Continue;
            }
            v.shown.set(Some(chapter));
            v.persist(chapter, block);
            v.show_chapter(chapter, partial);
            glib::ControlFlow::Break
        });
    }

    fn show_chapter(&self, chapter: u32, partial: bool) {
        self.status.set_text(&format!(
            "Chapter {} of {}{}",
            chapter + 1,
            self.book.chapters().len(),
            if partial {
                " · Some content could not be displayed"
            } else {
                ""
            }
        ));
    }

    fn load(self: &Rc<Self>, index: u32, landing: Landing) {
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
                // A newline after the last block would add an empty line, and
                // a short window's last page would show only that.
                for block in chapter.blocks {
                    if !offsets.is_empty() {
                        body.push('\n');
                        at += 1;
                    }
                    offsets.push(at);
                    for span in block.spans {
                        at += span.text.chars().count() as i32;
                        body.push_str(&span.text);
                    }
                }
                Ok((body, offsets, partial))
            })
            .await;
            if v.generation.get() != seq || v.window.upgrade().is_none_or(|w| !w.is_visible()) {
                return;
            }
            match result {
                Ok(Ok((body, offsets, partial))) => {
                    let target = match landing {
                        Landing::Block(block) => {
                            let block = (block as usize).min(offsets.len().saturating_sub(1));
                            Some((block as u32, offsets.get(block).copied().unwrap_or(0)))
                        }
                        Landing::End => None,
                    };
                    v.text.buffer().set_text(&body);
                    v.offsets.replace(offsets);
                    v.text.grab_focus();
                    v.restore(seq, index, target, partial);
                }
                Ok(Err(e)) => v.status.set_text(&format!("Couldn’t read chapter · {e:#}")),
                Err(_) => v.status.set_text("Chapter loading interrupted"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{interim_fraction, next_page_top, previous_page_top};

    #[test]
    fn the_fraction_read_counts_linear_chapters_and_ends_at_exactly_one() {
        let linear = [true, false, true, true];
        assert_eq!(interim_fraction(&linear, 0, 0.0), 0.0);
        assert_eq!(interim_fraction(&linear, 0, 0.5), 0.5 / 3.0);
        // A chapter outside the reading line is the start of the next.
        assert_eq!(interim_fraction(&linear, 1, 0.7), 1.0 / 3.0);
        assert_eq!(interim_fraction(&linear, 3, 0.5), 2.5 / 3.0);
        assert_eq!(interim_fraction(&linear, 3, 1.0), 1.0);
        assert_eq!(interim_fraction(&[false], 0, 1.0), 0.0);
    }

    /// Twenty-pixel lines from the top of the chapter.
    fn lines(count: i32) -> Vec<(i32, i32)> {
        (0..count).map(|n| (n * 20, 20)).collect()
    }

    #[test]
    fn the_next_page_starts_with_the_first_line_that_did_not_fit() {
        // A 50-pixel page from 0 shows lines 0 and 1 whole and cuts line 2.
        assert_eq!(next_page_top(lines(10), 0, 50), Some(40));
        // A line that ends exactly at the bottom edge was read in full.
        assert_eq!(next_page_top(lines(10), 0, 60), Some(60));
    }

    #[test]
    fn the_last_page_has_no_next_page() {
        assert_eq!(next_page_top(lines(3), 0, 60), None);
    }

    #[test]
    fn a_line_taller_than_the_page_still_advances() {
        assert_eq!(next_page_top([(0, 200)], 0, 50), Some(50));
    }

    #[test]
    fn the_previous_page_starts_on_a_whole_line_and_reaches_the_old_top() {
        // One 50-pixel page above 100 would start at 50, inside line 2.
        let top = previous_page_top(&lines(10), -24, 100, 50);
        assert_eq!(top, 60);
        assert!(top + 50 >= 100);
        assert_eq!(previous_page_top(&lines(10), -24, 100, 60), 40);
    }

    #[test]
    fn a_line_cut_at_the_top_is_whole_on_the_previous_page() {
        // The page starts at 65, inside the line from 60 to 80.
        let top = previous_page_top(&lines(10), -24, 65, 50);
        assert_eq!(top, 40);
        assert!(top + 50 >= 80);
    }

    #[test]
    fn paging_back_to_the_first_line_shows_the_margin_above_it() {
        assert_eq!(previous_page_top(&lines(10), -24, 20, 50), -24);
    }

    #[test]
    fn the_first_page_drops_its_margin_when_that_keeps_a_cut_line_whole() {
        // The page starts at 45, inside the line from 40 to 60. From the
        // margin at -24 a 50-pixel page would end at 26, short of that line.
        let top = previous_page_top(&lines(10), -24, 45, 50);
        assert_eq!(top, 20);
        assert!(top + 50 >= 60);
    }

    #[test]
    fn the_space_between_paragraphs_does_not_cost_the_first_page_its_margin() {
        // Lines of 23 pixels, 31 apart. Sixteen fit from the margin in 516,
        // and the next page starts with the seventeenth at 496.
        let spaced: Vec<_> = (0..40).map(|n| (n * 31, 23)).collect();
        assert_eq!(previous_page_top(&spaced, -24, 496, 516), -24);
    }

    #[test]
    fn a_cut_last_line_is_whole_on_the_previous_page() {
        let top = previous_page_top(&lines(3), -24, 45, 35);
        assert!(top < 45 && top + 35 >= 60, "{top}");
    }

    #[test]
    fn a_line_taller_than_the_page_still_goes_back() {
        assert_eq!(previous_page_top(&[(0, 200)], -24, 100, 50), 50);
    }
}

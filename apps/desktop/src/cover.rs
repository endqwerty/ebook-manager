//! A book's cover, drawn as the cover table in `docs/design/interface.md`
//! says: a 2:3 frame with the image, the edge line, the spine shade and two
//! shadows, or a binding set in type for a book that has no cover image.
use crate::thumbs::Thumbs;
use ebook_library::BrowseRow;
use gtk::{gdk, glib, graphene, gsk, pango, subclass::prelude::*};
use gtk4 as gtk;
use libadwaita as adw;
use libadwaita::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

/// Which of the cover's parts a view draws.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Kind {
    /// Everything, the progress line under the cover and the rise under the
    /// pointer.
    #[default]
    Grid,
    /// The list's thumbnail: the edge line and the contact shadow.
    List,
    /// The table's small cover: the edge line alone.
    Table,
}

/// The gap between a grid cover and its caption, which holds the progress line.
const CAPTION_GAP: i32 = 12;

/// Binding colours and the colour of the type on each, from the interface page.
const BINDINGS: [(u32, u32); 8] = [
    (0x24405c, 0xf1e6cc),
    (0x6f2a2a, 0xf1e6cc),
    (0x315047, 0xf1e6cc),
    (0x7a3b22, 0xf1e6cc),
    (0x51446b, 0xf1e6cc),
    (0xb98a3c, 0x23190a),
    (0x465366, 0xf1e6cc),
    (0x2f3a2b, 0xf1e6cc),
];

/// The binding for a book with no cover image: the first four bytes of the
/// SHA-256 of its identifier, as a big-endian number, modulo 8.
fn binding(id: &str) -> (u32, u32) {
    let hash = Sha256::digest(id.as_bytes());
    BINDINGS[(u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]) % 8) as usize]
}

fn rgb(colour: u32, alpha: f32) -> gdk::RGBA {
    let channel = |shift: u32| ((colour >> shift) & 0xff) as f32 / 255.0;
    gdk::RGBA::new(channel(16), channel(8), channel(0), alpha)
}

fn black(alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(0.0, 0.0, 0.0, alpha)
}

/// Where an image of this size goes in a frame, and whether it fills it. An
/// image whose width is 0.56 to 0.80 of its height fills the frame, cropped
/// equally from both sides or from top and bottom. Any other is shown whole
/// and centred.
fn place(image: (f32, f32), frame: (f32, f32)) -> (graphene::Rect, bool) {
    let fills = (0.56..=0.80).contains(&(image.0 / image.1));
    let (across, down) = (frame.0 / image.0, frame.1 / image.1);
    let scale = if fills {
        across.max(down)
    } else {
        across.min(down)
    };
    let (width, height) = (image.0 * scale, image.1 * scale);
    (
        graphene::Rect::new(
            (frame.0 - width) / 2.0,
            (frame.1 - height) / 2.0,
            width,
            height,
        ),
        fills,
    )
}

/// The desktop asks for less motion.
fn reduced_motion() -> bool {
    // The setting arrived in GTK 4.20, and the app runs on 4.12.
    gtk::Settings::default().is_some_and(|settings| {
        settings
            .find_property("gtk-interface-reduced-motion")
            .is_some_and(|_| {
                let value = settings.property_value("gtk-interface-reduced-motion");
                glib::EnumValue::from_value(&value).is_some_and(|(_, v)| v.nick() == "reduce")
            })
    })
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Cover {
        pub kind: Cell<Kind>,
        pub width: Cell<i32>,
        pub thumbs: OnceCell<Thumbs>,
        pub book: RefCell<Option<Rc<BrowseRow>>>,
        pub texture: RefCell<Option<gdk::Texture>>,
        /// How much of the image shows over the loading colour, 0 to 1.
        pub shown: Cell<f64>,
        /// How far the cover has risen under the pointer, 0 to 1.
        pub rise: Cell<f64>,
        pub fade: RefCell<Option<adw::TimedAnimation>>,
        pub lift: RefCell<Option<adw::TimedAnimation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Cover {
        const NAME: &'static str = "EbookManagerCover";
        type Type = super::Cover;
        type ParentType = gtk::Widget;
        fn class_init(class: &mut Self::Class) {
            class.set_css_name("cover");
            class.set_accessible_role(gtk::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Cover {}

    impl WidgetImpl for Cover {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            match self.kind.get() {
                Kind::Grid => gtk::SizeRequestMode::HeightForWidth,
                Kind::List | Kind::Table => gtk::SizeRequestMode::ConstantSize,
            }
        }
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let width = self.width.get();
            // A grid cover is as wide as its column. A thumbnail is the
            // width it was made with.
            let stretches = self.kind.get() == Kind::Grid;
            if orientation == gtk::Orientation::Horizontal {
                let least = if stretches { width.min(16) } else { width };
                return (least, width, -1, -1);
            }
            let across = if for_size < 0 || !stretches {
                width
            } else {
                for_size
            };
            let height = self.obj().frame_height(across as f32) as i32 + self.below();
            (height, height, -1, -1)
        }
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }

    impl Cover {
        /// Room under the frame that belongs to the cover.
        pub fn below(&self) -> i32 {
            match self.kind.get() {
                Kind::Grid => CAPTION_GAP,
                Kind::List | Kind::Table => 0,
            }
        }
    }
}

glib::wrapper! {
    pub struct Cover(ObjectSubclass<imp::Cover>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Cover {
    /// A cover `width` units wide when its container does not stretch it.
    pub fn new(kind: Kind, width: i32, thumbs: &Thumbs) -> Self {
        let cover: Self = glib::Object::new();
        let imp = cover.imp();
        imp.kind.set(kind);
        imp.width.set(width);
        let _ = imp.thumbs.set(thumbs.clone());
        if kind == Kind::Grid {
            let pointer = gtk::EventControllerMotion::new();
            let weak = cover.downgrade();
            pointer.connect_enter(move |_, _, _| {
                if let Some(cover) = weak.upgrade() {
                    cover.lift(1.0);
                }
            });
            let weak = cover.downgrade();
            pointer.connect_leave(move |_| {
                if let Some(cover) = weak.upgrade() {
                    cover.lift(0.0);
                }
            });
            cover.add_controller(pointer);
            // A press settles the cover back to rest.
            let press = gtk::GestureClick::new();
            press.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = cover.downgrade();
            press.connect_pressed(move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::None);
                if let Some(cover) = weak.upgrade() {
                    cover.lift(0.0);
                }
            });
            cover.add_controller(press);
        }
        cover
    }

    pub fn set_width(&self, width: i32) {
        if self.imp().width.replace(width) != width {
            self.queue_resize();
        }
    }

    fn frame_height(&self, width: f32) -> f32 {
        (width * 1.5).round()
    }

    /// Shows this book's cover, or the loading colour for a row that has
    /// not arrived.
    pub fn set_book(&self, book: Option<&Rc<BrowseRow>>) {
        let imp = self.imp();
        let same = match (&*imp.book.borrow(), book) {
            (Some(old), Some(new)) => old.id == new.id && old.cover == new.cover,
            (None, None) => true,
            _ => false,
        };
        imp.book.replace(book.cloned());
        if same {
            self.queue_draw();
            return;
        }
        if let Some(fade) = imp.fade.take() {
            fade.pause();
        }
        imp.texture.replace(None);
        imp.shown.set(0.0);
        self.queue_draw();
        let (Some(book), Some(thumbs)) = (book, imp.thumbs.get()) else {
            return;
        };
        let Some(cover) = &book.cover else { return };
        // An image already in memory is shown at once. One that has to be
        // read fades in over the loading colour.
        if let Some(texture) = thumbs.get(&cover.digest) {
            imp.texture.replace(Some(texture));
            imp.shown.set(1.0);
            return;
        }
        let weak = self.downgrade();
        let digest = cover.digest.clone();
        thumbs.want(&cover.digest, &book.id, move |texture| {
            let Some(cover) = weak.upgrade() else { return };
            let wanted = cover
                .imp()
                .book
                .borrow()
                .as_ref()
                .and_then(|book| book.cover.as_ref())
                .is_some_and(|c| c.digest == digest);
            if wanted {
                cover.fade_in(texture);
            }
        });
    }

    fn fade_in(&self, texture: &gdk::Texture) {
        let imp = self.imp();
        imp.texture.replace(Some(texture.clone()));
        let weak = self.downgrade();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            if let Some(cover) = weak.upgrade() {
                cover.imp().shown.set(value);
                cover.queue_draw();
            }
        });
        // Jumps to the end when the desktop has animations turned off.
        let fade = adw::TimedAnimation::new(self, 0.0, 1.0, 200, target);
        fade.play();
        imp.fade.replace(Some(fade));
    }

    fn lift(&self, to: f64) {
        let imp = self.imp();
        if to > 0.0 && reduced_motion() {
            return;
        }
        let weak = self.downgrade();
        let target = adw::CallbackAnimationTarget::new(move |value| {
            if let Some(cover) = weak.upgrade() {
                cover.imp().rise.set(value);
                cover.queue_draw();
            }
        });
        let lift = adw::TimedAnimation::new(self, imp.rise.get(), to, 150, target);
        lift.play();
        if let Some(earlier) = imp.lift.replace(Some(lift)) {
            earlier.pause();
        }
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let width = self.width() as f32;
        if width < 1.0 {
            return;
        }
        let height = self.frame_height(width);
        let book = imp.book.borrow();
        let texture = imp.texture.borrow();
        let style = adw::StyleManager::default();
        let (dark, contrast) = (style.is_dark(), style.is_high_contrast());
        let ink = self.color();
        let rise = imp.rise.get() as f32;
        let kind = imp.kind.get();
        let corner = match kind {
            Kind::Table => 1.0,
            _ if width >= 200.0 => 3.0,
            _ => 2.0,
        };
        let frame =
            gsk::RoundedRect::from_rect(graphene::Rect::new(0.0, 0.0, width, height), corner);

        self.draw_progress(snapshot, book.as_deref(), width, height, &ink);

        snapshot.save();
        snapshot.translate(&graphene::Point::new(0.0, -2.0 * rise));
        let shade = black(if dark { 0.28 } else { 0.16 });
        if !contrast && kind != Kind::Table {
            snapshot.append_outset_shadow(&frame, &shade, 0.0, 1.0 + rise, 0.0, 2.0 + rise);
        }
        if !contrast && kind == Kind::Grid {
            snapshot.append_outset_shadow(
                &frame,
                &shade,
                0.0,
                6.0 + 3.0 * rise,
                -4.0,
                14.0 + 4.0 * rise,
            );
        }
        snapshot.push_rounded_clip(&frame);
        let bounds = graphene::Rect::new(0.0, 0.0, width, height);
        let fill = gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), 0.1);
        let mut spine = false;
        match book.as_deref() {
            Some(book) if book.cover.is_none() => {
                self.draw_binding(snapshot, book, width, height);
                spine = true;
            }
            Some(book) => {
                let placed = texture.as_ref().map(|texture| {
                    place(
                        (texture.width() as f32, texture.height() as f32),
                        (width, height),
                    )
                });
                let loading = book.cover_rgb.map_or(fill, |colour| rgb(colour, 1.0));
                let fills = placed.as_ref().is_none_or(|(_, fills)| *fills);
                let shown = imp.shown.get();
                // An image shown whole leaves the rest of the frame in the
                // fill colour, so the loading colour fades out under it.
                if !fills {
                    snapshot.append_color(&fill, &bounds);
                }
                if fills || shown < 1.0 {
                    snapshot.push_opacity(if fills { 1.0 } else { 1.0 - shown });
                    snapshot.append_color(&loading, &bounds);
                    snapshot.pop();
                }
                if let (Some(texture), Some((area, _))) = (texture.as_ref(), placed) {
                    snapshot.push_opacity(shown);
                    snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, &area);
                    snapshot.pop();
                }
                spine = fills;
            }
            None => snapshot.append_color(&fill, &bounds),
        }
        if !contrast {
            // The shade on the side the book is bound, whatever the
            // interface's direction.
            if spine && kind == Kind::Grid && width >= 64.0 {
                let across = width * 0.05;
                let right = book.as_deref().is_some_and(|book| book.right_to_left);
                let (near, far) = if right {
                    (width, width - across)
                } else {
                    (0.0, across)
                };
                snapshot.append_linear_gradient(
                    &graphene::Rect::new(near.min(far), 0.0, across, height),
                    &graphene::Point::new(near, 0.0),
                    &graphene::Point::new(far, 0.0),
                    &[
                        gsk::ColorStop::new(0.0, black(0.14)),
                        gsk::ColorStop::new(1.0, black(0.0)),
                    ],
                );
            }
            let line = if dark {
                gdk::RGBA::new(1.0, 1.0, 1.0, 0.18)
            } else {
                black(0.12)
            };
            snapshot.append_inset_shadow(&frame, &line, 0.0, 0.0, 1.0, 0.0);
        }
        snapshot.pop();
        if contrast {
            // Outside the cover's edge, where artwork cannot hide it.
            let outline = gsk::RoundedRect::from_rect(
                graphene::Rect::new(-1.0, -1.0, width + 2.0, height + 2.0),
                corner + 1.0,
            );
            snapshot.append_border(&outline, &[1.0; 4], &[ink; 4]);
        }
        snapshot.restore();
    }

    /// The line in the gap under a cover, only while the book is partly read.
    fn draw_progress(
        &self,
        snapshot: &gtk::Snapshot,
        book: Option<&BrowseRow>,
        width: f32,
        height: f32,
        ink: &gdk::RGBA,
    ) {
        let read = match book.and_then(|book| book.progress) {
            Some(read) if read > 0.0 && read < 1.0 && self.imp().below() > 0 => read,
            _ => return,
        };
        let top = height + (CAPTION_GAP as f32 - 2.0) / 2.0;
        let tint = |alpha| gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), alpha);
        snapshot.append_color(&tint(0.15), &graphene::Rect::new(0.0, top, width, 2.0));
        let filled = (width * read).round().max(1.0);
        let start = if self.direction() == gtk::TextDirection::Rtl {
            width - filled
        } else {
            0.0
        };
        snapshot.append_color(&tint(0.55), &graphene::Rect::new(start, top, filled, 2.0));
    }

    /// A flat colour with the title at the top in the serif, a hairline
    /// rule, and the author at the bottom. Its type is artwork and scales
    /// with the cover, not with the text size setting.
    fn draw_binding(&self, snapshot: &gtk::Snapshot, book: &BrowseRow, width: f32, height: f32) {
        let (cloth, type_colour) = binding(&book.id);
        let type_colour = rgb(type_colour, 1.0);
        snapshot.append_color(
            &rgb(cloth, 1.0),
            &graphene::Rect::new(0.0, 0.0, width, height),
        );
        // On a thumbnail the type would be too small to make out.
        if width < 64.0 {
            return;
        }
        let pad = (width * 0.12).round();
        let across = width - 2.0 * pad;
        let layout = |text: &str, family: Option<&str>, size: f32, weight, lines: i32| {
            let layout = self.create_pango_layout(Some(text));
            let mut font = layout.context().font_description().unwrap_or_default();
            if let Some(family) = family {
                font.set_family(family);
            }
            font.set_weight(weight);
            font.set_absolute_size(f64::from(size) * f64::from(pango::SCALE));
            layout.set_font_description(Some(&font));
            layout.set_width((across * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
            layout.set_ellipsize(pango::EllipsizeMode::End);
            // A negative height is a number of lines.
            layout.set_height(-lines);
            layout
        };
        let title = layout(
            &book.title,
            Some("serif"),
            (width * 0.11).max(6.0),
            pango::Weight::Semibold,
            4,
        );
        snapshot.save();
        snapshot.translate(&graphene::Point::new(pad, pad));
        snapshot.append_layout(&title, &type_colour);
        snapshot.restore();
        let rule = pad + title.pixel_size().1 as f32 + (width * 0.05).round();
        snapshot.append_color(&type_colour, &graphene::Rect::new(pad, rule, across, 1.0));
        if !book.authors.is_empty() && width >= 110.0 {
            let author = layout(&book.authors, None, width * 0.09, pango::Weight::Normal, 2);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                pad,
                height - pad - author.pixel_size().1 as f32,
            ));
            snapshot.append_layout(&author, &type_colour);
            snapshot.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_takes_its_colour_from_the_books_identifier() {
        // SHA-256 of "abc" begins ba 78 16 bf, and 0xba7816bf modulo 8 is 7: Moss.
        assert_eq!(binding("abc"), (0x2f3a2b, 0xf1e6cc));
        // SHA-256 of the empty text begins e3 b0 c4 42: 2, Forest.
        assert_eq!(binding(""), (0x315047, 0xf1e6cc));
    }

    #[test]
    fn an_image_near_two_by_three_fills_the_frame_and_is_cropped_evenly() {
        // 0.8 wide for its height: scaled to the frame's height, 12 over
        // on each side.
        let (area, fills) = place((240.0, 300.0), (96.0, 150.0));
        assert!(fills);
        assert_eq!(
            (area.x(), area.y(), area.width(), area.height()),
            (-12.0, 0.0, 120.0, 150.0)
        );
        // 0.56: scaled to the width, cropped top and bottom.
        let (area, fills) = place((56.0, 100.0), (112.0, 168.0));
        assert!(fills);
        assert_eq!(
            (area.x(), area.y(), area.width(), area.height()),
            (0.0, -16.0, 112.0, 200.0)
        );
    }

    #[test]
    fn any_other_image_is_shown_whole_and_centred() {
        let (area, fills) = place((300.0, 300.0), (100.0, 150.0));
        assert!(!fills);
        assert_eq!(
            (area.x(), area.y(), area.width(), area.height()),
            (0.0, 25.0, 100.0, 100.0)
        );
        let (_, fills) = place((50.0, 100.0), (100.0, 150.0));
        assert!(!fills);
    }
}

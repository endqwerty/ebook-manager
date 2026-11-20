use crate::cover::{binding, binding_hash, reduced_motion, rgb};
use adw::{prelude::*, subclass::prelude::*};
use ebook_library::BrowseRow;
use gtk::{
    gdk, glib,
    graphene::{Point, Rect, Vec3},
    gsk, pango,
};
use gtk4 as gtk;
use libadwaita as adw;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Spine {
        pub book: RefCell<Option<Rc<BrowseRow>>>,
        pub selected: Cell<bool>,
        pub hover: Cell<bool>,
        pub angle: Cell<f64>,
        pub animation: RefCell<Option<adw::SpringAnimation>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Spine {
        const NAME: &'static str = "EbookManagerSpine";
        type Type = super::Spine;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Spine {}
    impl WidgetImpl for Spine {
        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
        }
    }
}
glib::wrapper! {
    pub struct Spine(ObjectSubclass<imp::Spine>) @extends gtk::Widget,
    @implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget;
}

pub(crate) fn state_label(book: &BrowseRow) -> String {
    if super::super::finished(book.progress) {
        "Finished".into()
    } else {
        super::super::percent(book.progress)
            .map_or_else(|| "Unread".into(), |p| format!("{p} read"))
    }
}

fn luminance(rgb: &gdk::RGBA) -> f32 {
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    linear(rgb.red()) * 0.2126 + linear(rgb.green()) * 0.7152 + linear(rgb.blue()) * 0.0722
}
fn colours(base: u32) -> (gdk::RGBA, gdk::RGBA, bool) {
    let base = rgb(base, 1.);
    let contrast = |a: f32, b: f32| (a.max(b) + 0.05) / (a.min(b) + 0.05);
    let light = rgb(0xf1e6cc, 1.);
    let dark = rgb(0x23190a, 1.);
    let l = luminance(&base);
    let pale = contrast(l, luminance(&light)) >= contrast(l, luminance(&dark));
    let ink = if pale { light } else { dark };
    let mut cloth = base;
    for step in 1..=20 {
        if contrast(luminance(&cloth), luminance(&ink)) >= 4.5 {
            break;
        }
        let t = step as f32 / 20.;
        let mix = |v: f32| (v * (1. - t) + if pale { 0. } else { t }) * 255.;
        cloth = gdk::RGBA::new(
            mix(base.red()).round() / 255.,
            mix(base.green()).round() / 255.,
            mix(base.blue()).round() / 255.,
            1.,
        );
    }
    (cloth, ink, pale)
}
pub(super) enum Grain {
    Back,
    Cloth,
}

fn grain_texture(strength: f32) -> gdk::MemoryTexture {
    let mut seed = 71237u32;
    let mut pixels = Vec::with_capacity(128 * 128 * 3);
    for _ in 0..128 * 128 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        // Mid-grey is neutral in Overlay. Mixing the noise toward it gives
        // the same opacity on Cairo and GL, including on repeated tiles.
        let level = 127.5 + (f32::from(seed as u8) - 127.5) * strength;
        pixels.extend_from_slice(&[level.round() as u8; 3]);
    }
    gdk::MemoryTexture::new(
        128,
        128,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from_owned(pixels),
        384,
    )
}

impl Grain {
    pub(super) fn draw(self, s: &gtk::Snapshot, bounds: &Rect) {
        thread_local! {
            static BACK: gdk::MemoryTexture = grain_texture(0.20);
            static CLOTH: gdk::MemoryTexture = grain_texture(0.35);
        }
        let texture = match self {
            Self::Back => &BACK,
            Self::Cloth => &CLOTH,
        };
        let tile = Rect::new(0., 0., 128., 128.);
        s.push_repeat(bounds, Some(&tile));
        texture.with(|texture| s.append_texture(texture, &tile));
        s.pop();
    }
}

pub(super) fn gradient(s: &gtk::Snapshot, r: Rect, horizontal: bool, stops: &[(f32, f32)]) {
    let stops = stops
        .iter()
        .map(|&(at, a)| gsk::ColorStop::new(at, rgb(if a > 0. { 0xffffff } else { 0 }, a.abs())))
        .collect::<Vec<_>>();
    s.append_linear_gradient(
        &r,
        &Point::new(r.x(), r.y()),
        &Point::new(
            r.x() + if horizontal { r.width() } else { 0. },
            r.y() + if horizontal { 0. } else { r.height() },
        ),
        &stops,
    );
}
fn cloth_face(s: &gtk::Snapshot, rect: Rect, cloth: &gdk::RGBA) {
    let w = rect.width();
    let h = rect.height();
    s.push_blend(gsk::BlendMode::Overlay);
    s.append_color(cloth, &rect);
    gradient(
        s,
        rect,
        true,
        &[
            (0., -0.30),
            (0.14, 0.14),
            (0.38, -0.06),
            (0.5, -0.10),
            (0.62, -0.06),
            (0.86, 0.14),
            (1., -0.30),
        ],
    );
    gradient(s, rect, false, &[(0., 0.06), (0.4, 0.), (1., -0.08)]);
    gradient(
        s,
        Rect::new(0., 14., w, 6.),
        false,
        &[(0., -0.12), (1., 0.)],
    );
    gradient(
        s,
        Rect::new(0., 14. + h - 6., w, 6.),
        false,
        &[(0., 0.), (1., -0.12)],
    );
    s.pop();
    Grain::Cloth.draw(s, &rect);
    s.pop();
}

#[derive(PartialEq)]
struct ClothKey {
    scale: f32,
    rgb: u32,
    width: i32,
    height: i32,
}

// GTK keeps each widget's snapshot. Share just the cloth across titles and
// bound the cache even when every book in the catalogue has a different colour.
fn software_cloth(renderer: &gsk::Renderer, key: ClothKey, colour: &gdk::RGBA) -> gdk::Texture {
    thread_local! {
        static CLOTH: RefCell<std::collections::VecDeque<(ClothKey, gdk::Texture)>> = const { RefCell::new(std::collections::VecDeque::new()) };
    }
    CLOTH.with_borrow_mut(|cache| {
        if let Some(entry) = cache
            .iter()
            .position(|(old, _)| old == &key)
            .and_then(|i| cache.remove(i))
        {
            let texture = entry.1.clone();
            cache.push_front(entry);
            return texture;
        }
        let area = Rect::new(0., 14., key.width as f32, key.height as f32);
        let snapshot = gtk::Snapshot::new();
        let scale = key.scale;
        snapshot.scale(scale, scale);
        cloth_face(&snapshot, area, colour);
        // The caller supplies a non-empty rectangle with an opaque base.
        let node = snapshot
            .to_node()
            .unwrap_or_else(|| gsk::ColorNode::new(colour, &area).upcast());
        let texture = renderer.render_texture(
            &node,
            Some(&Rect::new(
                0.,
                14. * scale,
                area.width() * scale,
                area.height() * scale,
            )),
        );
        cache.truncate(63);
        cache.push_front((key, texture.clone()));
        texture
    })
}

impl Spine {
    pub fn new() -> Self {
        let spine: Self = glib::Object::new();
        let pointer = gtk::EventControllerMotion::new();
        let w = spine.downgrade();
        pointer.connect_enter(move |_, _, _| {
            if let Some(s) = w.upgrade() {
                s.imp().hover.set(true);
                s.animate();
            }
        });
        let w = spine.downgrade();
        pointer.connect_leave(move |_| {
            if let Some(s) = w.upgrade() {
                s.imp().hover.set(false);
                s.animate();
            }
        });
        spine.add_controller(pointer);
        spine
    }
    pub fn show_book(&self, book: Option<&Rc<BrowseRow>>) {
        self.imp().book.replace(book.cloned());
        let name = book
            .map(|b| {
                [
                    Some(b.title.clone()),
                    (!b.authors.is_empty()).then(|| b.authors.clone()),
                    Some(state_label(b)),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ")
            })
            .unwrap_or_else(|| "Loading book".into());
        if let Some(cell) = self.parent() {
            cell.update_property(&[gtk::accessible::Property::Label(&name)]);
        }

        self.queue_draw();
    }
    pub fn select(&self, yes: bool) {
        if self.imp().selected.replace(yes) != yes {
            self.animate();
        }
    }
    fn animate(&self) {
        let imp = self.imp();
        if let Some(a) = imp.animation.take() {
            a.pause();
        }
        let to = if imp.selected.get() {
            10.
        } else if imp.hover.get() {
            4.
        } else {
            0.
        };
        if reduced_motion() || !self.is_mapped() {
            imp.angle.set(to);
            self.queue_draw();
            return;
        }
        let w = self.downgrade();
        let target = adw::CallbackAnimationTarget::new(move |v| {
            if let Some(s) = w.upgrade() {
                s.imp().angle.set(v);
                s.queue_draw();
            }
        });
        let anim = adw::SpringAnimation::new(
            self,
            imp.angle.get(),
            to,
            adw::SpringParams::new(1., 1., 450.),
            target,
        );
        anim.set_clamp(true);
        anim.play();
        imp.animation.replace(Some(anim));
    }
    fn draw(&self, target: &gtk::Snapshot) {
        let w = self.width() as f32;
        let h = self.height() as f32 - 14.;
        if w < 1. || h < 1. {
            return;
        }
        let book = self.imp().book.borrow();
        let Some(b) = book.as_ref() else {
            if std::env::var_os("EBOOK_SHELF_REFRESH_TRACE").is_some()
                && let Some(viewport) = self.ancestor(gtk::Viewport::static_type())
                && let Some(bounds) = self.compute_bounds(&viewport)
                && bounds.y() < viewport.height() as f32
                && bounds.y() + bounds.height() > 0.
            {
                eprintln!("shelf-visible-placeholder");
            }
            target.append_color(&rgb(0x808080, 1.), &Rect::new(0., 14., w, h));
            return;
        };
        let base = b.spine_rgb.unwrap_or_else(|| binding(&b.id).0);
        let (cloth, ink, pale) = colours(base);
        let style = adw::StyleManager::default();
        let hc = style.is_high_contrast();
        let angle = self.imp().angle.get() as f32;
        let software = self
            .native()
            .and_then(|n| n.renderer())
            .filter(|r| r.is::<gsk::CairoRenderer>());
        let display_scale = self
            .native()
            .and_then(|n| n.surface())
            .map_or(1., |surface| surface.scale() as f32);
        let lift = if software.is_some() { angle * 0.4 } else { 0. };
        let h = h + lift;
        let rect = Rect::new(0., 14., w, h);
        let corner = gtk::graphene::Size::new(1., 1.);
        let square = gtk::graphene::Size::new(0., 0.);
        let frame = gsk::RoundedRect::new(rect, corner, corner, square, square);
        let drawing = gtk::Snapshot::new();
        let s = &drawing;
        s.translate(&Point::new(0., -lift));
        if !hc {
            let trailing = if self.direction() == gtk::TextDirection::Rtl {
                -5.
            } else {
                5.
            };
            s.append_outset_shadow(&frame, &rgb(0, 0.30), trailing, 0., -2., 8.);
            if angle > 0. {
                s.append_outset_shadow(
                    &frame,
                    &rgb(0, if style.is_dark() { 0.28 } else { 0.16 }),
                    0.,
                    6.,
                    -4.,
                    14.,
                );
            }
        }
        let top = 14. - 4. - angle / 2.;
        let path = gsk::PathBuilder::new();
        path.move_to(0., 14.);
        path.line_to(w * 0.06, top);
        path.line_to(w * 0.94, top);
        path.line_to(w, 14.);
        path.close();
        s.push_fill(&path.to_path(), gsk::FillRule::Winding);
        s.append_color(
            &gdk::RGBA::new(
                cloth.red() * 0.75,
                cloth.green() * 0.75,
                cloth.blue() * 0.75,
                1.,
            ),
            &Rect::new(0., top, w, 14. - top),
        );
        let pages = if style.is_dark() { 0xbfb8a9 } else { 0xece5d6 };
        s.append_color(&rgb(pages, 1.), &Rect::new(2., top, w - 4., 14. - top));
        let mut x = 4.;
        while x < w - 2. {
            s.append_color(&rgb(0, 0.18), &Rect::new(x, top, 1., 14. - top));
            x += 3.;
        }
        s.pop();
        s.push_rounded_clip(&frame);
        if hc {
            s.append_color(&cloth, &rect);
        } else if let Some(renderer) = software.as_ref() {
            let key = ClothKey {
                rgb: base,
                width: self.width(),
                height: h.ceil() as i32,
                scale: display_scale,
            };
            s.append_texture(
                &software_cloth(renderer, key, &cloth),
                &Rect::new(0., 14., w, h.ceil()),
            );
        } else {
            cloth_face(s, rect, &cloth);
        }
        let stamp = rgb(
            if pale { 0 } else { 0xffffff },
            if pale { 0.4 } else { 0.3 },
        );
        let variant = binding_hash(&b.id) % 3;
        if variant == 1 {
            let mut rule = ink;
            rule.set_alpha(0.45);
            for y in [8., 12., h - 13., h - 9.] {
                s.append_color(&stamp, &Rect::new(0., 15. + y, w, 1.));
                s.append_color(&rule, &Rect::new(0., 14. + y, w, 1.));
            }
        }
        let layout = |text: &str, size: f64, serif: bool| {
            let vertical = serif
                && text.chars().any(|c| {
                    matches!(
                        glib::Unichar::script(c),
                        glib::UnicodeScript::Han
                            | glib::UnicodeScript::Hiragana
                            | glib::UnicodeScript::Katakana
                            | glib::UnicodeScript::Hangul
                    )
                });
            let context = if vertical {
                self.create_pango_context()
            } else {
                self.pango_context()
            };
            if vertical {
                context.set_base_gravity(pango::Gravity::East);
                context.set_gravity_hint(pango::GravityHint::Natural);
            }
            let l = pango::Layout::new(&context);
            l.set_text(text);
            let mut f = l.context().font_description().unwrap_or_default();
            if serif {
                f.set_family("serif");
                f.set_weight(pango::Weight::Semibold);
            }
            f.set_absolute_size(size * f64::from(pango::SCALE));
            l.set_font_description(Some(&f));
            l.set_single_paragraph_mode(true);
            l
        };
        let title = layout(&b.title, 12., true);
        let labelled = variant == 2 && w >= 26. && w - 12. >= title.pixel_size().1 as f32;
        let name = b
            .author_sort
            .as_deref()
            .unwrap_or("")
            .split(',')
            .next()
            .unwrap_or("");
        let author = layout(name, 10., false);
        let print_author =
            w >= 24. && title.pixel_size().0 + author.pixel_size().0 + 44 <= h as i32;
        if title.pixel_size().0 > (h - 20.) as i32 {
            title.set_width((h - 20.) as i32 * pango::SCALE);
            title.set_ellipsize(pango::EllipsizeMode::End);
        }
        let print = |l: &pango::Layout, y: f32, col: &gdk::RGBA| {
            s.save();
            let (width, height) = l.pixel_size();
            if l.direction(0) == pango::Direction::Rtl {
                s.translate(&Point::new((w - height as f32) / 2., y + width as f32));
                s.rotate(-90.);
            } else {
                s.translate(&Point::new((w + height as f32) / 2., y));
                s.rotate(90.);
            }
            s.save();
            s.translate(&Point::new(1., 0.));
            s.append_layout(l, &stamp);
            s.restore();
            s.append_layout(l, col);
            s.restore();
        };
        if labelled {
            let bounds = Rect::new(
                6.,
                18.,
                w - 12.,
                title.pixel_extents().1.width() as f32 + 12.,
            );
            let label = gsk::RoundedRect::from_rect(bounds, 1.);
            s.append_outset_shadow(&label, &stamp, 0., 1., 0., 0.);
            s.push_rounded_clip(&label);
            s.append_color(&ink, &bounds);
            s.pop();
        }
        print(&title, 24., if labelled { &cloth } else { &ink });
        if print_author {
            print(&author, 14. + h - 10. - author.pixel_size().0 as f32, &ink);
        }
        s.pop();
        let selected = self.imp().selected.get();
        let line = if hc && selected {
            style.accent_color_rgba()
        } else if hc {
            self.color()
        } else if style.is_dark() {
            rgb(0xffffff, 0.18)
        } else {
            rgb(0, 0.12)
        };
        s.append_border(
            &frame,
            &[if hc && selected { 2. } else { 1. }; 4],
            &[line; 4],
        );
        let Some(node) = drawing.to_node() else {
            return;
        };
        let foot = 14. + h;
        if let Some(renderer) = software {
            // Keep the composed title and decoration as one Cairo node while
            // other rows scroll. Replaying their shadows every frame is costly.
            let bounds = node.bounds();
            let layer = gtk::Snapshot::new();
            layer.scale(display_scale, display_scale);
            layer.append_node(&node);
            if let Some(scaled) = layer.to_node() {
                let pixels = scaled.bounds();
                target.append_texture(&renderer.render_texture(&scaled, Some(&pixels)), &bounds);
            }
        } else if angle == 0. {
            target.append_node(&node);
        } else {
            target.save();
            target.translate(&Point::new(w / 2., foot));
            target.perspective(800.);
            target.rotate_3d(-angle, &Vec3::new(1., 0., 0.));
            target.translate(&Point::new(-w / 2., -foot));
            target.append_node(&node);
            target.restore();
        }
    }
}

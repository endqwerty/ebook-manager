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
thread_local! {
    static GRAIN: gdk::MemoryTexture = {
        let mut seed = 71237u32;
        let mut pixels = Vec::with_capacity(128 * 128 * 3);
        for _ in 0..128*128 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            pixels.extend_from_slice(&[seed as u8; 3]);
        }
        gdk::MemoryTexture::new(128,128,gdk::MemoryFormat::R8g8b8,&glib::Bytes::from_owned(pixels),384)
    };
}

pub(super) fn grain(s: &gtk::Snapshot, bounds: &Rect) {
    let tile = Rect::new(0., 0., 128., 128.);
    s.push_repeat(bounds, Some(&tile));
    GRAIN.with(|texture| s.append_texture(texture, &tile));
    s.pop();
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
            target.append_color(&rgb(0x808080, 1.), &Rect::new(0., 14., w, h));
            return;
        };
        let (cloth, ink, pale) = colours(b.spine_rgb.unwrap_or_else(|| binding(&b.id).0));
        let style = adw::StyleManager::default();
        let hc = style.is_high_contrast();
        let angle = self.imp().angle.get() as f32;
        let rect = Rect::new(0., 14., w, h);
        let corner = gtk::graphene::Size::new(1., 1.);
        let square = gtk::graphene::Size::new(0., 0.);
        let frame = gsk::RoundedRect::new(rect, corner, corner, square, square);
        let drawing = gtk::Snapshot::new();
        let s = &drawing;
        if !hc {
            s.append_outset_shadow(&frame, &rgb(0, 0.30), 5., 0., -2., 8.);
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
        if !hc {
            s.push_blend(gsk::BlendMode::Overlay);
        }
        s.append_color(&cloth, &rect);
        if !hc {
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
        }
        if !hc {
            s.pop();
            s.push_opacity(0.35);
            grain(s, &rect);
            s.pop();
            s.pop();
        }
        let stamp = rgb(
            if pale { 0 } else { 0xffffff },
            if pale { 0.4 } else { 0.3 },
        );
        let variant = binding_hash(&b.id) % 3;
        if variant == 1 {
            let mut rule = ink;
            rule.set_alpha(0.45);
            for y in [8., 12., h - 12., h - 8.] {
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
        title.set_width((h - 20.) as i32 * pango::SCALE);
        title.set_ellipsize(pango::EllipsizeMode::End);
        let print = |l: &pango::Layout, y: f32, col: &gdk::RGBA| {
            s.save();
            s.translate(&Point::new((w + l.pixel_size().1 as f32) / 2., y));
            s.rotate(90.);
            s.save();
            s.translate(&Point::new(1., 0.));
            s.append_layout(l, &stamp);
            s.restore();
            s.append_layout(l, col);
            s.restore();
        };
        if variant == 2 {
            s.append_color(
                &ink,
                &Rect::new(
                    6.,
                    18.,
                    w - 12.,
                    title.pixel_extents().1.width() as f32 + 12.,
                ),
            );
        }
        print(&title, 24., if variant == 2 { &cloth } else { &ink });
        if print_author {
            print(&author, 14. + h - 10. - author.pixel_size().0 as f32, &ink);
        }
        s.pop();
        let selected = self.imp().selected.get();
        let line = if hc && selected {
            style.accent_color_rgba()
        } else if hc {
            self.color()
        } else {
            if style.is_dark() {
                rgb(0xffffff, 0.18)
            } else {
                rgb(0, 0.12)
            }
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
        if let Some(renderer) = self.native().and_then(|n| n.renderer())
            && renderer.is::<gsk::CairoRenderer>()
        {
            let bounds = node.bounds();
            let texture = renderer.render_texture(&node, Some(&bounds));
            if angle == 0. {
                target.append_texture(&texture, &bounds);
                return;
            }
            // Cairo accepts only affine transforms. Project each horizontal
            // pixel band about the foot with the same pinhole camera.
            let sin = angle.to_radians().sin();
            let cos = angle.to_radians().cos();
            let project = |y: f32| {
                let scale = 800. / (800. - (foot - y) * sin);
                (foot + (y - foot) * cos * scale, scale)
            };
            for y in bounds.y().floor() as i32..(bounds.y() + bounds.height()).ceil() as i32 {
                let y = y as f32;
                let (top, scale) = project(y);
                let (bottom, _) = project(y + 1.);
                target.save();
                target.translate(&Point::new(w * (1. - scale) / 2., top));
                target.scale(scale, bottom - top);
                target.translate(&Point::new(0., -y));
                target.push_clip(&Rect::new(bounds.x(), y, bounds.width(), 1.));
                target.append_texture(&texture, &bounds);
                target.pop();
                target.restore();
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

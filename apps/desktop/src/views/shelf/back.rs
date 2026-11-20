//! The recess behind a row of books. GTK draws its children over the back.
use super::spine::{Grain, gradient};
use crate::cover::rgb;
use gtk::{gdk, glib, graphene::Rect, gsk};

type BackTexture = ((i32, i32, bool), gdk::Texture);
thread_local! { static LAST_BACK:std::cell::RefCell<Option<BackTexture>>=const { std::cell::RefCell::new(None) }; }
use gtk4 as gtk;
use libadwaita::{self as adw, prelude::*, subclass::prelude::*};

fn draw_recess(s: &gtk::Snapshot, r: Rect, dark: bool) {
    s.push_blend(gsk::BlendMode::Overlay);
    s.append_color(&rgb(if dark { 0x2a292e } else { 0xd5cec2 }, 1.), &r);
    s.pop();
    Grain::Back.draw(s, &r);
    s.pop();
    gradient(
        s,
        r,
        false,
        &[(0., if dark { -0.45 } else { -0.24 }), (0.5, 0.), (1., 0.)],
    );
    let side = if dark { -0.30 } else { -0.14 };
    gradient(
        s,
        r,
        true,
        &[(0., side), (0.08, 0.), (0.92, 0.), (1., side)],
    );
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Back;
    #[glib::object_subclass]
    impl ObjectSubclass for Back {
        const NAME: &'static str = "EbookManagerShelfBack";
        type Type = super::Back;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for Back {}
    impl BoxImpl for Back {}
    impl WidgetImpl for Back {
        fn snapshot(&self, s: &gtk::Snapshot) {
            let style = adw::StyleManager::default();
            if !style.is_high_contrast() {
                let w = self.obj().width() as f32;
                let h = self.obj().height() as f32;
                let r = Rect::new(0., 0., w, h);
                let dark = style.is_dark();
                let key = (w as i32, h as i32, dark);
                if let Some(renderer) = self.obj().native().and_then(|n| n.renderer()) {
                    LAST_BACK.with_borrow_mut(|cache| {
                        if cache.as_ref().is_none_or(|(old, _)| *old != key) {
                            let layer = gtk::Snapshot::new();
                            draw_recess(&layer, r, dark);
                            if let Some(node) = layer.to_node() {
                                *cache = Some((key, renderer.render_texture(&node, Some(&r))));
                            }
                        }
                        if let Some((_, texture)) = cache {
                            s.append_texture(texture, &r);
                        }
                    });
                } else {
                    draw_recess(s, r, dark);
                }
            }
            if let (Some(first), Some(last)) = (self.obj().first_child(), self.obj().last_child())
                && let (Some(a), Some(b)) = (
                    first.compute_bounds(&*self.obj()),
                    last.compute_bounds(&*self.obj()),
                )
            {
                let start = a.x().min(b.x());
                let end = (a.x() + a.width()).max(b.x() + b.width());
                s.append_color(
                    &rgb(0, 0.35),
                    &Rect::new(start, 14., end - start, (a.height() - 14.).max(0.)),
                );
            }
            self.parent_snapshot(s);
        }
    }
}
glib::wrapper! {
    pub struct Back(ObjectSubclass<imp::Back>) @extends gtk::Box,gtk::Widget,
    @implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget,gtk::Orientable;
}

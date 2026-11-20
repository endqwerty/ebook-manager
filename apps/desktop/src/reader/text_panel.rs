//! The text settings popover: four rows first, and every other setting one
//! level down. Each control is a stepper, a list or a switch, never a slider.
//! See docs/design/interface-reader.md, "Text settings".

use super::settings::{
    BLACK, Colour, GREY, LETTERS, LINES, Margin, PAPER, Rgb, SIZES, Settings, Turn, WEIGHTS, WHITE,
    WORDS, contrast,
};
use gtk::gdk;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The faces offered when the system has them, in the design's order: the
/// reader's serif, the system's serifs and sans serifs, then faces made for
/// dyslexic readers.
const FACES: [&str; 10] = [
    "Literata",
    "Noto Serif",
    "Liberation Serif",
    "DejaVu Serif",
    "Adwaita Sans",
    "Noto Sans",
    "Liberation Sans",
    "DejaVu Sans",
    "OpenDyslexic",
    "Atkinson Hyperlegible",
];

/// The faces of `FACES` that are installed, in that order.
pub fn installed_faces(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    let installed = widget.pango_context().list_families();
    FACES
        .iter()
        .filter(|face| installed.iter().any(|f| f.name() == **face))
        .map(|face| (*face).to_owned())
        .collect()
}

/// Give a button a face of a letter or two and a name in words. A button
/// made with a label is named by that label, whatever name it is given
/// after, so a screen reader would say "A" for both size buttons. The face
/// here is a plain child, and the name is the button's own.
fn named(
    button: &(impl IsA<gtk::Button> + IsA<gtk::Widget> + IsA<gtk::Accessible>),
    face: &str,
    words: &str,
) {
    button.set_child(Some(&gtk::Label::new(Some(face))));
    button.set_tooltip_text(Some(words));
    button.update_property(&[gtk::accessible::Property::Label(words)]);
}

fn row(label: &str, control: &(impl IsA<gtk::Widget> + IsA<gtk::Accessible>)) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    row.append(&name);
    control.set_valign(gtk::Align::Center);
    control.update_property(&[gtk::accessible::Property::Label(label)]);
    row.append(control);
    row
}

/// A pair of buttons that step an index down and up between 0 and `last`.
fn stepper(
    less: (&str, &str),
    more: (&str, &str),
    last: usize,
    get: impl Fn() -> usize + 'static,
    set: impl Fn(usize) + 'static,
) -> gtk::Box {
    let pair = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pair.add_css_class("linked");
    let down = gtk::Button::new();
    let up = gtk::Button::new();
    for (button, (face, words)) in [(&down, less), (&up, more)] {
        named(button, face, words);
        pair.append(button);
    }
    let get = Rc::new(get);
    let set = Rc::new(set);
    let refresh = {
        let (down, up, get) = (down.clone(), up.clone(), get.clone());
        Rc::new(move || {
            down.set_sensitive(get() > 0);
            up.set_sensitive(get() < last);
        })
    };
    refresh();
    for (button, by) in [(&down, -1i32), (&up, 1)] {
        let (get, set, refresh) = (get.clone(), set.clone(), refresh.clone());
        button.connect_clicked(move |_| {
            let next = (get() as i32 + by).clamp(0, last as i32) as usize;
            set(next);
            refresh();
        });
    }
    // The popover can be reset while it is open.
    pair.connect_map(move |_| refresh());
    pair
}

fn rgba(colour: Rgb) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(colour.0) / 255.0,
        f32::from(colour.1) / 255.0,
        f32::from(colour.2) / 255.0,
        1.0,
    )
}

fn rgb(colour: &gdk::RGBA) -> Rgb {
    let part = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(
        part(colour.red()),
        part(colour.green()),
        part(colour.blue()),
    )
}

/// The tallest the second level grows before it scrolls, in pixels. A
/// reader window at its default height of 640 has about 540 under its
/// controls, less the popover's arrow and padding.
const MORE_HEIGHT: i32 = 460;

/// Build the popover. `changed` is called after every change, with the
/// settings already updated.
pub fn build(
    settings: Rc<RefCell<Settings>>,
    faces: Vec<String>,
    changed: Rc<dyn Fn()>,
) -> gtk::Popover {
    let popover = gtk::Popover::new();
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    stack.set_vhomogeneous(false);
    popover.set_child(Some(&stack));
    // Controls set from code must not answer as if the reader had set them.
    let syncing = Rc::new(Cell::new(false));
    let change = {
        let (settings, changed, syncing) = (settings.clone(), changed.clone(), syncing.clone());
        Rc::new(move |edit: &dyn Fn(&mut Settings)| {
            if syncing.get() {
                return;
            }
            edit(&mut settings.borrow_mut());
            changed();
        })
    };

    let first = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let second = gtk::Box::new(gtk::Orientation::Vertical, 10);
    for page in [&first, &second] {
        page.set_margin_top(12);
        page.set_margin_bottom(12);
        page.set_margin_start(12);
        page.set_margin_end(12);
        page.set_width_request(300);
    }
    stack.add_named(&first, Some("first"));
    // The second level is taller than a small screen, so it scrolls. Its
    // most is what fits under the controls of a reader window at its
    // default height of 640. Focus moving through it scrolls the focused
    // control into view.
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .max_content_height(MORE_HEIGHT)
        .child(
            &gtk::Viewport::builder()
                .scroll_to_focus(true)
                .child(&second)
                .build(),
        )
        .build();
    stack.add_named(&scroller, Some("more"));

    let scope = gtk::Label::new(Some("For every book"));
    scope.set_xalign(0.0);
    scope.add_css_class("dim-label");
    first.append(&scope);

    // 1. Page colour: four swatches, and one for colours of the reader's own.
    let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    swatches.update_property(&[gtk::accessible::Property::Label("Page colour")]);
    let css = gtk::CssProvider::new();
    css.load_from_string(&format!(
        ".page-swatch {{ min-width: 36px; min-height: 36px; border-radius: 18px; padding: 0; font-weight: 600; }}\n{}",
        [("white", WHITE), ("paper", PAPER), ("grey", GREY), ("black", BLACK)]
            .iter()
            .map(|(name, page)| format!(
                ".page-swatch.{name} {{ background: {}; color: {}; }}\n",
                page.page.hex(),
                page.ink.hex()
            ))
            .collect::<String>()
    ));
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&popover),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let mut group: Option<gtk::ToggleButton> = None;
    let mut swatch_buttons = Vec::new();
    for (name, class, colour) in [
        ("White", "white", Colour::White),
        ("Paper", "paper", Colour::Paper),
        ("Grey", "grey", Colour::Grey),
        ("Black", "black", Colour::Black),
        ("A colour of your own", "custom", Colour::Custom),
    ] {
        let button = gtk::ToggleButton::new();
        named(
            &button,
            if colour == Colour::Custom { "+" } else { "Aa" },
            name,
        );
        button.add_css_class("page-swatch");
        button.add_css_class(class);
        if let Some(first) = &group {
            button.set_group(Some(first));
        } else {
            group = Some(button.clone());
        }
        let (change, stack) = (change.clone(), stack.clone());
        button.connect_toggled(move |b| {
            if b.is_active() {
                change(&|s| s.colour = colour);
                if colour == Colour::Custom {
                    stack.set_visible_child_name("more");
                }
            }
        });
        swatches.append(&button);
        swatch_buttons.push((colour, button));
    }
    first.append(&swatches);

    // 2. Font.
    let mut names = vec!["Default".to_owned()];
    names.extend(faces.iter().cloned());
    let font = gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
    {
        let (change, faces) = (change.clone(), faces.clone());
        font.connect_selected_notify(move |d| {
            let chosen = (d.selected() as usize)
                .checked_sub(1)
                .and_then(|i| faces.get(i).cloned());
            change(&|s| s.face = chosen.clone());
        });
    }
    first.append(&row("Font", &font));

    // 3. Size: a small and a large letter A, and no number.
    let size = {
        let (get, set) = (settings.clone(), change.clone());
        stepper(
            ("A", "Smaller text"),
            ("A", "Larger text"),
            SIZES.len() - 1,
            move || get.borrow().size,
            move |n| set(&|s| s.size = n),
        )
    };
    if let Some(small) = size.first_child() {
        small.add_css_class("caption");
    }
    if let Some(large) = size.last_child() {
        large.add_css_class("title-3");
    }
    first.append(&row("Size", &size));

    // 4. More.
    let more = gtk::Button::with_label("More");
    {
        let stack = stack.clone();
        more.connect_clicked(move |_| stack.set_visible_child_name("more"));
    }
    first.append(&more);

    // The second level.
    let back = gtk::Button::with_label("Back");
    back.set_halign(gtk::Align::Start);
    {
        let stack = stack.clone();
        back.connect_clicked(move |_| stack.set_visible_child_name("first"));
    }
    second.append(&back);

    let weight = gtk::DropDown::from_strings(&WEIGHTS.map(|(_, name)| name));
    {
        let change = change.clone();
        weight.connect_selected_notify(move |d| {
            let n = d.selected() as usize;
            change(&|s| s.weight = n);
        });
    }
    second.append(&row("Weight", &weight));

    for (label, less, more, last, field) in [
        (
            "Line spacing",
            "Tighter lines",
            "Looser lines",
            LINES.len() - 1,
            0u8,
        ),
        (
            "Letter spacing",
            "Closer letters",
            "Wider letters",
            LETTERS.len() - 1,
            1,
        ),
        (
            "Word spacing",
            "Closer words",
            "Wider words",
            WORDS.len() - 1,
            2,
        ),
    ] {
        let (get, set) = (settings.clone(), change.clone());
        let control = stepper(
            ("−", less),
            ("+", more),
            last,
            move || {
                let s = get.borrow();
                [s.line, s.letter, s.word][usize::from(field)]
            },
            move |n| {
                set(&|s| match field {
                    0 => s.line = n,
                    1 => s.letter = n,
                    _ => s.word = n,
                })
            },
        );
        second.append(&row(label, &control));
    }

    let margin = gtk::DropDown::from_strings(&["Narrow", "Medium", "Wide"]);
    {
        let change = change.clone();
        margin.connect_selected_notify(move |d| {
            let chosen =
                [Margin::Narrow, Margin::Medium, Margin::Wide][d.selected().min(2) as usize];
            change(&|s| s.margin = chosen);
        });
    }
    second.append(&row("Margins", &margin));

    let switch = |label: &str, set: fn(&mut Settings, bool)| -> gtk::Switch {
        let control = gtk::Switch::new();
        let change = change.clone();
        control.connect_active_notify(move |c| {
            let on = c.is_active();
            change(&|s| set(s, on));
        });
        second.append(&row(label, &control));
        control
    };
    let justify = switch("Justified", |s, on| {
        s.justify = on;
        // Justified text without hyphenation opens rivers of white space.
        if on {
            s.hyphenate = true;
        }
    });
    let hyphenate = switch("Hyphenation", |s, on| s.hyphenate = on);
    let columns = switch("Two columns", |s, on| s.two_columns = on);
    let scrolling = switch("Continuous scrolling", |s, on| s.scrolling = on);
    {
        // Justifying turns hyphenation on with it, and the switch shows it.
        let (hyphenate, settings, syncing) = (hyphenate.clone(), settings.clone(), syncing.clone());
        justify.connect_active_notify(move |_| {
            let on = settings.borrow().hyphenate;
            if hyphenate.is_active() != on {
                syncing.set(true);
                hyphenate.set_active(on);
                syncing.set(false);
            }
        });
    }

    let turn = gtk::DropDown::from_strings(&["Slide", "Cross-fade", "None"]);
    {
        let change = change.clone();
        turn.connect_selected_notify(move |d| {
            let chosen = [Turn::Slide, Turn::Fade, Turn::None][d.selected().min(2) as usize];
            change(&|s| s.turn = chosen);
        });
    }
    second.append(&row("Page turn", &turn));

    let follow = gtk::Switch::new();
    second.append(&row("Page colour follows light and dark", &follow));

    // A page colour of the reader's own: two colours, and their contrast.
    let dialog = gtk::ColorDialog::builder().with_alpha(false).build();
    let page_colour = gtk::ColorDialogButton::new(Some(dialog.clone()));
    let text_colour = gtk::ColorDialogButton::new(Some(dialog));
    let verdict = gtk::Label::new(None);
    verdict.set_xalign(0.0);
    verdict.set_wrap(true);
    verdict.add_css_class("caption");
    let judge = {
        let (settings, verdict) = (settings.clone(), verdict.clone());
        Rc::new(move || {
            let (page, ink) = settings.borrow().custom;
            let ratio = contrast(page, ink);
            verdict.set_text(&if ratio < 4.5 {
                format!("Contrast {ratio:.1}:1. This is under 4.5:1 and may be hard to read.")
            } else {
                format!("Contrast {ratio:.1}:1")
            });
        })
    };
    for (button, is_page) in [(&page_colour, true), (&text_colour, false)] {
        let (change, judge) = (change.clone(), judge.clone());
        button.connect_rgba_notify(move |b| {
            let chosen = rgb(&b.rgba());
            change(&|s| {
                if is_page {
                    s.custom.0 = chosen;
                } else {
                    s.custom.1 = chosen;
                }
                s.colour = Colour::Custom;
            });
            judge();
        });
    }
    second.append(&row("Your page colour", &page_colour));
    second.append(&row("Your text colour", &text_colour));
    second.append(&verdict);

    let reset = gtk::Button::with_label("Reset");
    reset.set_halign(gtk::Align::Start);
    second.append(&reset);

    // Show the settings in the controls. Run when the popover opens and
    // after a reset, so the controls never show something else.
    let sync = {
        let (settings, syncing, judge) = (settings.clone(), syncing.clone(), judge.clone());
        let (font, weight, margin, turn, follow) = (
            font.clone(),
            weight.clone(),
            margin.clone(),
            turn.clone(),
            follow.clone(),
        );
        let (justify, hyphenate, columns, scrolling) = (
            justify.clone(),
            hyphenate.clone(),
            columns.clone(),
            scrolling.clone(),
        );
        let (page_colour, text_colour) = (page_colour.clone(), text_colour.clone());
        Rc::new(move || {
            syncing.set(true);
            let s = settings.borrow().clone();
            for (colour, button) in &swatch_buttons {
                button.set_active(*colour == s.colour);
            }
            if s.colour == Colour::System {
                // No swatch is the reader's choice while the system decides.
                for (_, button) in &swatch_buttons {
                    button.set_active(false);
                }
            }
            let at = s
                .face
                .as_ref()
                .and_then(|f| faces.iter().position(|n| n == f))
                .map_or(0, |i| i as u32 + 1);
            font.set_selected(at);
            weight.set_selected(s.weight as u32);
            margin.set_selected(match s.margin {
                Margin::Narrow => 0,
                Margin::Medium => 1,
                Margin::Wide => 2,
            });
            turn.set_selected(match s.turn {
                Turn::Slide => 0,
                Turn::Fade => 1,
                Turn::None => 2,
            });
            justify.set_active(s.justify);
            hyphenate.set_active(s.hyphenate);
            columns.set_active(s.two_columns);
            scrolling.set_active(s.scrolling);
            follow.set_active(s.colour == Colour::System);
            page_colour.set_rgba(&rgba(s.custom.0));
            text_colour.set_rgba(&rgba(s.custom.1));
            judge();
            syncing.set(false);
        })
    };
    {
        let (change, sync) = (change.clone(), sync.clone());
        follow.connect_active_notify(move |f| {
            let on = f.is_active();
            change(&|s| {
                if on {
                    s.colour = Colour::System;
                } else if s.colour == Colour::System {
                    s.colour = Colour::Paper;
                }
            });
            // The swatches show the change.
            if !syncing.get() {
                sync();
            }
        });
    }
    {
        let (settings, changed, sync) = (settings.clone(), changed.clone(), sync.clone());
        reset.connect_clicked(move |_| {
            settings.replace(Settings::default());
            sync();
            changed();
        });
    }
    {
        let (sync, stack) = (sync.clone(), stack.clone());
        popover.connect_show(move |_| {
            stack.set_visible_child_name("first");
            sync();
        });
    }
    popover
}

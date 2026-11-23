//! A window that another program destroys must not end the app when GTK
//! still uses it. Run under a private display: see apps/desktop/README.md.

#[allow(unused_imports)]
#[path = "../src/x11_errors.rs"]
mod x11_errors;

use gtk::{gdk, glib, glib::translate::ToGlibPtr, prelude::*};
use gtk4 as gtk;
use libtest_mimic::{Arguments, Failed, Trial};
use std::process::Command;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn gdk_x11_display_error_trap_push(display: *mut gdk::ffi::GdkDisplay);
    fn gdk_x11_display_error_trap_pop_ignored(display: *mut gdk::ffi::GdkDisplay);
}

fn settle(until: impl Fn() -> bool) -> Result<(), Failed> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let context = glib::MainContext::default();
    while !until() {
        if Instant::now() > deadline {
            return Err("timed out".into());
        }
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

type Test = fn() -> Result<(), Failed>;

/// A mapped window of our own that another program has just destroyed, or
/// `None` when the display is not X11.
fn destroyed_window(name: &str) -> Result<Option<(gdk::Display, gtk::Window)>, Failed> {
    let display = gdk::Display::default().ok_or("no display")?;
    if !display.type_().name().starts_with("GdkX11") {
        return Ok(None);
    }
    x11_errors::install(&display);
    let title = format!("{name}-{}", std::process::id());
    let window = gtk::Window::builder().title(&title).build();
    window.present();
    settle(|| window.is_mapped())?;
    display.sync();
    let found = Command::new("xdotool")
        .args(["search", "--sync", "--name", &format!("^{title}$")])
        .output()?;
    let id = String::from_utf8(found.stdout)?;
    let id = id.lines().next().ok_or("window not found")?.to_owned();
    let closed = Command::new("xdotool")
        .args(["windowclose", &id])
        .status()?;
    if !closed.success() {
        return Err("xdotool could not destroy the window".into());
    }
    Ok(Some((display, window)))
}

fn run_a_while() {
    let context = glib::MainContext::default();
    for _ in 0..50 {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// GDK reads the destroy notice only when the main loop next runs, so a
/// title set before then is a request about a window that is gone.
fn a_window_destroyed_by_another_program_is_survived() -> Result<(), Failed> {
    if !x11_errors::interposed() {
        return Err("libraries do not reach the program's XSetErrorHandler".into());
    }
    let Some((display, window)) = destroyed_window("destroyed-elsewhere")? else {
        return Ok(());
    };
    window.set_title(Some("after"));
    display.sync();
    window.queue_draw();
    run_a_while();
    display.sync();
    Ok(())
}

/// GDK sets its own error handler while an error trap is open. An error
/// about a request made before the trap, read while it is open, must still
/// reach the filter.
fn a_late_error_read_inside_a_gdk_trap_is_survived() -> Result<(), Failed> {
    let Some((display, window)) = destroyed_window("destroyed-before-trap")? else {
        return Ok(());
    };
    window.set_title(Some("after"));
    let raw: *mut gdk::ffi::GdkDisplay = display.to_glib_none().0;
    // SAFETY: the running X11 display; the trap is pushed and popped here.
    unsafe { gdk_x11_display_error_trap_push(raw) };
    display.sync();
    // SAFETY: pops the trap pushed above.
    unsafe { gdk_x11_display_error_trap_pop_ignored(raw) };
    run_a_while();
    Ok(())
}

/// The library window destroyed while the reader opens: GDK asks for the
/// pointer inside an error trap as the new window is made, and reads the
/// late error about the destroyed one there.
fn a_window_made_after_another_is_destroyed_is_survived() -> Result<(), Failed> {
    let Some((_display, _gone)) = destroyed_window("destroyed-then-new")? else {
        return Ok(());
    };
    let reader = gtk::Window::builder().title("reader").build();
    reader.present();
    run_a_while();
    Ok(())
}

/// A destroyed window whose content changes is painted by the next frame,
/// and GDK checks the window manager's hints inside an error trap there.
fn a_destroyed_window_that_changes_is_survived() -> Result<(), Failed> {
    let Some((_display, window)) = destroyed_window("destroyed-and-changed")? else {
        return Ok(());
    };
    let label = gtk::Label::new(Some("before"));
    window.set_child(Some(&label));
    label.set_text("after");
    window.set_title(Some("after"));
    window.queue_draw();
    run_a_while();
    Ok(())
}

fn main() {
    let mut arguments = Arguments::from_args();
    arguments.test_threads = Some(1);
    let tests: [(&str, Test); 4] = [
        (
            "a_window_destroyed_by_another_program_is_survived",
            a_window_destroyed_by_another_program_is_survived,
        ),
        (
            "a_late_error_read_inside_a_gdk_trap_is_survived",
            a_late_error_read_inside_a_gdk_trap_is_survived,
        ),
        (
            "a_window_made_after_another_is_destroyed_is_survived",
            a_window_made_after_another_is_destroyed_is_survived,
        ),
        (
            "a_destroyed_window_that_changes_is_survived",
            a_destroyed_window_that_changes_is_survived,
        ),
    ];
    if arguments.list {
        let names = tests
            .iter()
            .map(|(name, _)| Trial::test(*name, || Ok(())))
            .collect();
        libtest_mimic::run(&arguments, names).exit();
    }
    gtk::disable_portals();
    gtk::init().expect("a display: run under apps/desktop/tests/private-session.sh");
    let trials = tests
        .iter()
        .map(|(name, test)| Trial::test(*name, *test))
        .collect();
    libtest_mimic::run(&arguments, trials).exit();
}

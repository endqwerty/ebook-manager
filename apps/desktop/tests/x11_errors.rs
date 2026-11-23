//! A window that another program destroys must not end the app when GTK
//! still uses it. Run under a private display: see apps/desktop/README.md.

#[allow(unused_imports)]
#[path = "../src/x11_errors.rs"]
mod x11_errors;

use gtk::{gdk, glib, prelude::*};
use gtk4 as gtk;
use libtest_mimic::{Arguments, Failed, Trial};
use std::process::Command;
use std::time::{Duration, Instant};

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

/// GDK reads the destroy notice only when the main loop next runs, so a
/// title set before then is a request about a window that is gone.
fn a_window_destroyed_by_another_program_is_survived() -> Result<(), Failed> {
    let display = gdk::Display::default().ok_or("no display")?;
    if !display.type_().name().starts_with("GdkX11") {
        return Ok(());
    }
    x11_errors::install(&display);
    let title = format!("destroyed-elsewhere-{}", std::process::id());
    let window = gtk::Window::builder().title(&title).build();
    window.present();
    settle(|| window.is_mapped())?;
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
    window.set_title(Some("after"));
    display.sync();
    window.queue_draw();
    let context = glib::MainContext::default();
    for _ in 0..50 {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    display.sync();
    Ok(())
}

fn main() {
    let mut arguments = Arguments::from_args();
    arguments.test_threads = Some(1);
    let name = "a_window_destroyed_by_another_program_is_survived";
    if arguments.list {
        libtest_mimic::run(&arguments, vec![Trial::test(name, || Ok(()))]).exit();
    }
    gtk::disable_portals();
    gtk::init().expect("a display: run under apps/desktop/tests/private-session.sh");
    let trials = vec![Trial::test(
        name,
        a_window_destroyed_by_another_program_is_survived,
    )];
    libtest_mimic::run(&arguments, trials).exit();
}

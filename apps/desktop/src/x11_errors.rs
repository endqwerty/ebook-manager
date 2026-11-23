//! Keep an X error about one of the app's own windows, destroyed by another
//! program, from ending the app.
//!
//! When another client destroys one of our toplevel X windows, GTK can still
//! paint it or set a property on it before GDK reads the destroy notice. The
//! server answers that request with `BadWindow` or `BadDrawable`, and GDK's
//! handler ends the process with status 1. Such an error names a window that
//! is already gone, so nothing is lost by ignoring it. Every other error goes
//! to GDK's handler as before.
//!
//! [`install`] does nothing on a display that is not X11.

use gtk::{gdk, glib, glib::translate::ToGlibPtr, prelude::*};
use gtk4 as gtk;
use std::collections::HashSet;
use std::ffi::{c_int, c_uchar, c_ulong, c_void};
use std::sync::{Mutex, OnceLock};

const BAD_WINDOW: c_uchar = 3;
const BAD_DRAWABLE: c_uchar = 9;

#[repr(C)]
struct XErrorEvent {
    kind: c_int,
    display: *mut c_void,
    resource: c_ulong,
    serial: c_ulong,
    error_code: c_uchar,
    request_code: c_uchar,
    minor_code: c_uchar,
}

type Handler = unsafe extern "C" fn(*mut c_void, *mut XErrorEvent) -> c_int;

#[link(name = "X11")]
unsafe extern "C" {
    fn XSetErrorHandler(handler: Option<Handler>) -> Option<Handler>;
}

unsafe extern "C" {
    fn gdk_x11_surface_get_xid(surface: *mut gdk::ffi::GdkSurface) -> c_ulong;
}

/// GDK's handler, which every error not ignored here goes to.
static GDK_HANDLER: OnceLock<Option<Handler>> = OnceLock::new();
/// Every toplevel X window the app has had. A window's number stays here
/// after it is destroyed, because its late errors arrive after GDK forgets
/// it.
static OURS: Mutex<Option<HashSet<c_ulong>>> = Mutex::new(None);

/// Whether an error is one only a window destroyed by another program gives.
fn ignored(error_code: c_uchar, resource: c_ulong, ours: &HashSet<c_ulong>) -> bool {
    matches!(error_code, BAD_WINDOW | BAD_DRAWABLE) && ours.contains(&resource)
}

unsafe extern "C" fn handle(display: *mut c_void, event: *mut XErrorEvent) -> c_int {
    // SAFETY: Xlib passes a valid event for the duration of the call.
    let (code, resource) = unsafe { ((*event).error_code, (*event).resource) };
    let mine = OURS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .is_some_and(|ours| ignored(code, resource, ours));
    if mine {
        return 0;
    }
    match GDK_HANDLER.get().copied().flatten() {
        // SAFETY: the handler Xlib had before ours, called as Xlib would.
        Some(gdk_handler) => unsafe { gdk_handler(display, event) },
        None => 0,
    }
}

fn remember(window: &gtk::Window) {
    let Some(surface) = window.surface() else {
        return;
    };
    if !surface.type_().name().starts_with("GdkX11") {
        return;
    }
    // SAFETY: the surface is an X11 surface of the running display.
    let xid = unsafe { gdk_x11_surface_get_xid(surface.to_glib_none().0) };
    OURS.lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(xid);
}

fn watch(window: &gtk::Window) {
    if window.is_realized() {
        remember(window);
    }
    window.connect_realize(remember);
}

/// Put the handler in front of GDK's and learn every toplevel window the app
/// opens from now on. Call once, after the display is open.
pub fn install(display: &gdk::Display) {
    if !display.type_().name().starts_with("GdkX11") || GDK_HANDLER.get().is_some() {
        return;
    }
    // SAFETY: called on the GTK thread after GDK installed its handler; the
    // one it returns is kept and called for every error not ignored.
    let gdk_handler = unsafe { XSetErrorHandler(Some(handle)) };
    let _ = GDK_HANDLER.set(gdk_handler);
    let toplevels = gtk::Window::toplevels();
    for item in toplevels.iter::<glib::Object>().flatten() {
        if let Ok(window) = item.downcast::<gtk::Window>() {
            watch(&window);
        }
    }
    toplevels.connect_items_changed(|list, position, _, added| {
        for index in position..position + added {
            if let Some(window) = list.item(index).and_downcast::<gtk::Window>() {
                watch(&window);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{BAD_DRAWABLE, BAD_WINDOW, ignored};
    use std::collections::HashSet;

    #[test]
    fn only_lost_windows_of_our_own_are_ignored() {
        let ours = HashSet::from([0x600005]);
        assert!(ignored(BAD_WINDOW, 0x600005, &ours));
        assert!(ignored(BAD_DRAWABLE, 0x600005, &ours));
        assert!(!ignored(BAD_WINDOW, 0x600006, &ours));
        for other in [8, 11, 2] {
            assert!(!ignored(other, 0x600005, &ours));
        }
    }
}

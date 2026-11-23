//! Keep an X error about one of the app's own windows, destroyed by another
//! program, from ending the app.
//!
//! When another client destroys one of our toplevel X windows, GTK can still
//! paint it or set a property on it before GDK reads the destroy notice. The
//! server answers that request with `BadWindow` or `BadDrawable`, and GDK
//! ends the process with status 1. Such an error names a window that is
//! already gone, so nothing is lost by ignoring it. Every other error goes
//! to the handler that was asked for, as before.
//!
//! GDK installs its own handler again each time it opens an error trap, and
//! that handler ends the process for any error outside the trap. So the
//! executable provides `XSetErrorHandler` itself. Rust exports an
//! executable's `#[no_mangle]` functions, so libraries' calls reach this one
//! before libX11's: whatever handler a caller sets becomes the one this
//! module's filter passes errors on to, and the filter stays in front of it.
//! [`interposed`] says whether that holds.
//!
//! Two limits remain. Inside a GDK error trap, a request about one of these
//! destroyed windows now counts as succeeded, because the filter takes the
//! error before GDK can match it to the trap; the window is gone and GDK
//! handles its destroy notice soon after. And a library that looks up libX11's own
//! function by name and calls it directly would replace the filter until the
//! next call here; none that the app loads does.
//!
//! [`install`] starts learning the app's windows. It does nothing on a
//! display that is not X11. Should the symbol not be reached, it puts the filter
//! in front of GDK's handler directly, which covers every error outside a
//! GDK error trap.

use gtk::{gdk, glib, glib::translate::ToGlibPtr, prelude::*};
use gtk4 as gtk;
use std::collections::HashSet;
use std::ffi::{c_int, c_uchar, c_ulong, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

const BAD_WINDOW: c_uchar = 3;
const BAD_DRAWABLE: c_uchar = 9;

#[repr(C)]
pub struct XErrorEvent {
    kind: c_int,
    display: *mut c_void,
    resource: c_ulong,
    serial: c_ulong,
    error_code: c_uchar,
    request_code: c_uchar,
    minor_code: c_uchar,
}

pub type Handler = unsafe extern "C" fn(*mut c_void, *mut XErrorEvent) -> c_int;
type SetHandler = unsafe extern "C" fn(Option<Handler>) -> Option<Handler>;

unsafe extern "C" {
    fn gdk_x11_surface_get_xid(surface: *mut gdk::ffi::GdkSurface) -> c_ulong;
}

/// The handler a caller last asked for, as an address; 0 for Xlib's own.
static ASKED: AtomicUsize = AtomicUsize::new(0);
/// Xlib's own handler, which `XSetErrorHandler(NULL)` means. Learned once,
/// by the first call, which installs the filter; callers that arrive while
/// it runs wait for it.
static XLIB_DEFAULT: OnceLock<usize> = OnceLock::new();
/// Every toplevel X window the app has had. A window's number stays here
/// after it is destroyed, because its late errors arrive after GDK forgets
/// it.
static OURS: Mutex<Option<HashSet<c_ulong>>> = Mutex::new(None);

fn libx11_set_handler() -> Option<SetHandler> {
    static REAL: OnceLock<Option<SetHandler>> = OnceLock::new();
    *REAL.get_or_init(|| {
        // SAFETY: dlsym with RTLD_NEXT and a NUL-terminated name; a non-null
        // result is libX11's XSetErrorHandler, whose signature this is.
        unsafe {
            let found = libc::dlsym(libc::RTLD_NEXT, c"XSetErrorHandler".as_ptr());
            (!found.is_null()).then(|| std::mem::transmute::<*mut c_void, SetHandler>(found))
        }
    })
}

fn as_handler(address: usize) -> Option<Handler> {
    // SAFETY: every non-zero address stored here was a `Handler`.
    (address != 0).then(|| unsafe { std::mem::transmute::<usize, Handler>(address) })
}

/// Xlib's `XSetErrorHandler`, as every library in the process calls it: the
/// filter is installed, and `handler` is what it passes errors on to.
///
/// # Safety
///
/// As Xlib's: `handler`, if any, must be a valid error handler.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XSetErrorHandler(handler: Option<Handler>) -> Option<Handler> {
    let set = libx11_set_handler()?;
    let mut first = false;
    let default = *XLIB_DEFAULT.get_or_init(|| {
        first = true;
        // SAFETY: libX11's own function, called as Xlib documents.
        unsafe { set(Some(filter as Handler)) }.map_or(0, |h| h as usize)
    });
    if !first {
        // SAFETY: as above. Puts the filter back should anything have
        // replaced it through libX11 directly; that handler is then the one
        // this call replaces.
        let replaced = unsafe { set(Some(filter as Handler)) };
        if let Some(replaced) = replaced
            && replaced as usize != filter as Handler as usize
        {
            ASKED.store(replaced as usize, Ordering::SeqCst);
        }
    }
    let previous = ASKED.swap(handler.map_or(0, |h| h as usize), Ordering::SeqCst);
    as_handler(previous).or_else(|| as_handler(default))
}

/// Whether an error is one only a window destroyed by another program gives.
fn ignored(error_code: c_uchar, resource: c_ulong, ours: &HashSet<c_ulong>) -> bool {
    matches!(error_code, BAD_WINDOW | BAD_DRAWABLE) && ours.contains(&resource)
}

unsafe extern "C" fn filter(display: *mut c_void, event: *mut XErrorEvent) -> c_int {
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
    let next = as_handler(ASKED.load(Ordering::SeqCst))
        .or_else(|| XLIB_DEFAULT.get().copied().and_then(as_handler));
    match next {
        // SAFETY: the handler a caller set, called as Xlib would.
        Some(next) => unsafe { next(display, event) },
        None => 0,
    }
}

/// Whether libraries' calls to `XSetErrorHandler` reach this module's.
pub fn interposed() -> bool {
    // SAFETY: dlsym with RTLD_DEFAULT and a NUL-terminated name.
    let found = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"XSetErrorHandler".as_ptr()) };
    found as usize == XSetErrorHandler as SetHandler as usize
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

/// Learn every toplevel window the app opens from now on. Call once, after
/// the display is open.
pub fn install(display: &gdk::Display) {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if !display.type_().name().starts_with("GdkX11") || INSTALLED.set(()).is_err() {
        return;
    }
    if !interposed()
        && let Some(set) = libx11_set_handler()
    {
        // SAFETY: libX11's own function; what it replaces is GDK's handler,
        // which the filter then passes errors on to.
        let gdk_handler = unsafe { set(Some(filter as Handler)) };
        ASKED.store(gdk_handler.map_or(0, |h| h as usize), Ordering::SeqCst);
    }
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
    use super::{BAD_DRAWABLE, BAD_WINDOW, ignored, interposed};
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

    #[test]
    fn libraries_reach_this_set_handler() {
        assert!(interposed());
    }
}

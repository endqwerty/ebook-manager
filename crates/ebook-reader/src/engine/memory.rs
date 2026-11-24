//! The helper's memory bound on macOS, where the system offers no usable
//! address-space limit. Every allocation the helper makes goes through
//! [`Capped`], which refuses one that would pass the limit. A block being
//! resized counts at both sizes until the move is done. Rust then aborts,
//! and the helper's fatal-signal handler turns that into an ordinary exit.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static USED: AtomicUsize = AtomicUsize::new(0);
static LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The system allocator with a ceiling on the bytes in use.
pub struct Capped;

/// Set the ceiling. Returns false when [`Capped`] is not this program's
/// allocator, in which case nothing is bounded.
pub fn limit(bytes: u64) -> bool {
    LIMIT.store(
        usize::try_from(bytes).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
    drop(std::hint::black_box(Box::new(0u8)));
    INSTALLED.load(Ordering::Relaxed)
}

fn take(bytes: usize) -> bool {
    INSTALLED.store(true, Ordering::Relaxed);
    let mut used = USED.load(Ordering::Relaxed);
    loop {
        let Some(next) = used
            .checked_add(bytes)
            .filter(|&next| next <= LIMIT.load(Ordering::Relaxed))
        else {
            return false;
        };
        match USED.compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(now) => used = now,
        }
    }
}

fn give(bytes: usize) {
    USED.fetch_sub(bytes, Ordering::Relaxed);
}

#[allow(unsafe_code)]
// SAFETY: every call is passed to `System` unchanged, so its contract is
// the caller's. This type only counts sizes around those calls.
unsafe impl GlobalAlloc for Capped {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return std::ptr::null_mut();
        }
        let block = unsafe { System.alloc(layout) };
        if block.is_null() {
            give(layout.size());
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return std::ptr::null_mut();
        }
        let block = unsafe { System.alloc_zeroed(layout) };
        if block.is_null() {
            give(layout.size());
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        unsafe { System.dealloc(block, layout) };
        give(layout.size());
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if !take(size) {
            return std::ptr::null_mut();
        }
        let moved = unsafe { System.realloc(block, layout, size) };
        give(if moved.is_null() { size } else { layout.size() });
        moved
    }
}

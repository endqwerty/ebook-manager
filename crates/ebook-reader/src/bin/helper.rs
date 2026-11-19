//! Sandboxed helper. Started by the host library inside bubblewrap; refuses to
//! read a book when it finds itself outside a sandbox.
#![deny(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};

/// Exit code for a helper that ran out of its memory allowance.
const OUT_OF_MEMORY: i32 = 4;

/// The system allocator, except that running out of memory ends the helper
/// with an ordinary exit instead of Rust's default abort. A book that needs
/// more memory than its limit is an expected outcome, and an abort would be
/// reported by the desktop as a crash.
struct ExitOnFailure;

#[allow(unsafe_code)]
// SAFETY: every call is forwarded unchanged to `System`; the only addition is
// what happens after `System` reports failure, and that path never returns.
unsafe impl GlobalAlloc for ExitOnFailure {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        or_exit(System.alloc(layout))
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        or_exit(System.alloc_zeroed(layout))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        or_exit(System.realloc(ptr, layout, new_size))
    }
}

/// Pass a successful allocation through; on failure, write a fixed note to
/// stderr and leave without running destructors or allocating again.
fn or_exit(ptr: *mut u8) -> *mut u8 {
    if ptr.is_null() {
        const NOTE: &[u8] = b"helper: out of memory\n";
        #[allow(unsafe_code)]
        // SAFETY: write(2) and _exit(2) are async-signal-safe and allocate
        // nothing; NOTE is a static buffer of the stated length.
        unsafe {
            libc::write(libc::STDERR_FILENO, NOTE.as_ptr().cast(), NOTE.len());
            libc::_exit(OUT_OF_MEMORY);
        }
    }
    ptr
}

#[global_allocator]
static ALLOCATOR: ExitOnFailure = ExitOnFailure;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(ebook_reader::engine::run::main(&args));
}

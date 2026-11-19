//! The one place this crate steps outside safe Rust: preparing the child
//! process between `fork` and `exec`, before bubblewrap starts.

use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Make the child keep nothing of the application's but its three standard
/// streams, and die with the application.
///
/// A desktop program holds open files, sockets and process handles. Any of
/// them not marked close-on-exec would otherwise be inherited through
/// bubblewrap into the helper and bypass the sandbox's file, network and
/// process isolation. The kernel's own parent-death signal is set here as
/// well, because bubblewrap sets its own only after it has started.
///
/// The signal follows the thread that started the child, so that thread must
/// stay alive until the child is collected. Every call in this crate blocks
/// on its child, which keeps that true.
#[allow(unsafe_code)]
pub fn confine(cmd: &mut Command) {
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: the closure runs in the forked child before exec. It makes
    // three direct system calls, allocates nothing, takes no locks and
    // touches no memory shared with the parent, so it is async-signal-safe.
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                return Err(io::Error::last_os_error());
            }
            // The application may have gone away before the line above ran.
            if libc::getppid() != parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            // Mark every descriptor above stderr close-on-exec rather than
            // closing it now: the standard library reports a failed exec
            // through one of them.
            let first: libc::c_uint = 3;
            let last: libc::c_uint = libc::c_uint::MAX;
            let flags: libc::c_uint = libc::CLOSE_RANGE_CLOEXEC;
            if libc::syscall(libc::SYS_close_range, first, last, flags) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

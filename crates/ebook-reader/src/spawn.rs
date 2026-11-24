//! Preparing the child process between `fork` and `exec`, before the sandbox
//! launcher starts.

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
#[cfg(target_os = "linux")]
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

/// macOS has no parent-death signal and no `close_range`. The helper ends
/// when its input closes or its CPU limit runs out, and each descriptor is
/// marked one by one, up to the highest this process has open.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub fn confine(cmd: &mut Command) {
    let parent = std::process::id() as libc::pid_t;
    let end = descriptor_limit();
    // SAFETY: the closure runs in the forked child before exec. It makes
    // direct system calls only, allocates nothing, takes no locks and
    // touches no memory shared with the parent, so it is async-signal-safe.
    unsafe {
        cmd.pre_exec(move || {
            if libc::getppid() != parent {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            libc::setsid();
            // The limit is read again here, in the child's own copy, so one
            // raised by another thread since the listing is covered too.
            let mut now = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            let under_limit = if libc::getrlimit(libc::RLIMIT_NOFILE, &mut now) == 0 {
                libc::c_int::try_from(now.rlim_cur.min(MOST)).unwrap_or(libc::c_int::MAX)
            } else {
                libc::c_int::MAX
            };
            for fd in 3..end.max(under_limit) {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

/// The most descriptor numbers ever marked, which is more than a process
/// can hold.
#[cfg(target_os = "macos")]
const MOST: u64 = 1 << 20;

/// One past the highest descriptor the child could inherit. Everything
/// under the descriptor limit is covered, whenever it was opened. A limit
/// lowered after a file was opened leaves that file open above it, so the
/// open descriptors are listed as well, with room for ones another thread
/// opens between the listing and the fork: a new descriptor takes the lowest
/// free number, so it lands just past the highest. When the listing fails,
/// every number a process can hold is covered.
#[cfg(target_os = "macos")]
fn descriptor_limit() -> libc::c_int {
    const ROOM: u64 = 256;
    let open = std::fs::read_dir("/dev/fd").ok().and_then(|entries| {
        entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u64>().ok())
            .max()
    });
    let limit = rustix::process::getrlimit(rustix::process::Resource::Nofile).current;
    let end = match (open, limit) {
        (Some(highest), Some(limit)) => limit.max(highest.saturating_add(ROOM)),
        (Some(highest), None) => highest.saturating_add(ROOM),
        (None, _) => MOST,
    };
    libc::c_int::try_from(end.min(MOST)).unwrap_or(libc::c_int::MAX)
}

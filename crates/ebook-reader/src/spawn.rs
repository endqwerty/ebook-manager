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
/// marked one by one, up to the most this process may hold.
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
            for fd in 3..end {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags >= 0 && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
}

/// One past the highest descriptor this process can hold: its own limit, or
/// the system's per-process maximum when it has none.
#[cfg(target_os = "macos")]
fn descriptor_limit() -> libc::c_int {
    const FALLBACK: u64 = 1 << 20;
    let soft = rustix::process::getrlimit(rustix::process::Resource::Nofile).current;
    let system = || {
        let mut value: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>();
        #[allow(unsafe_code)]
        // SAFETY: the name is a NUL-terminated string, and `value` and
        // `size` describe one writable c_int.
        let found = unsafe {
            libc::sysctlbyname(
                c"kern.maxfilesperproc".as_ptr(),
                std::ptr::addr_of_mut!(value).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        u64::try_from(value).ok().filter(|_| found)
    };
    let limit = soft.or_else(system).unwrap_or(FALLBACK).min(FALLBACK);
    libc::c_int::try_from(limit).unwrap_or(libc::c_int::MAX)
}

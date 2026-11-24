//! The helper checks that it really is confined before it touches a book.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::Path;
use std::process::Stdio;

#[cfg(target_os = "linux")]
const HIDDEN: [&str; 9] = [
    "/proc", "/home", "/etc", "/tmp", "/run", "/var", "/root", "/sys", "/dev",
];
#[cfg(target_os = "linux")]
const WRITE_PROBES: [&str; 1] = ["/write-probe"];

#[cfg(target_os = "linux")]
fn this_program() -> Result<std::path::PathBuf, String> {
    Ok(crate::wire::HELPER_IN_SANDBOX.into())
}

/// The system volume's root is read-only for every process on macOS, so the
/// write probes are places an unconfined process can write.
#[cfg(target_os = "macos")]
const HIDDEN: [&str; 12] = [
    "/Users",
    "/etc",
    "/tmp",
    "/var",
    "/private",
    "/Library",
    "/Applications",
    "/Volumes",
    "/dev",
    "/bin",
    "/opt",
    "/System/Library/CoreServices",
];
#[cfg(target_os = "macos")]
const WRITE_PROBES: [&str; 2] = [
    "/private/tmp/ebook-reader-helper-write-probe",
    "/private/var/tmp/ebook-reader-helper-write-probe",
];

/// The start probe is only a proof if the program it names exists, so this
/// is the helper's own file, which the sandbox lets it read.
#[cfg(target_os = "macos")]
fn this_program() -> Result<std::path::PathBuf, String> {
    std::env::current_exe()
        .ok()
        .filter(|p| p.is_file())
        .ok_or_else(|| "the helper cannot find its own program".to_owned())
}

/// Returns why the process does not look sandboxed, or `Ok` when it does.
pub fn verify() -> Result<(), String> {
    for dir in HIDDEN {
        if Path::new(dir).exists() {
            return Err(format!("{dir} is visible"));
        }
    }
    for probe in WRITE_PROBES {
        if std::fs::write(probe, b"x").is_ok() {
            let _ = std::fs::remove_file(probe);
            return Err("the filesystem is writable".into());
        }
    }
    // 192.0.2.1 is reserved for documentation. With no network the kernel has
    // no route and refuses at once; no packet is sent either way. The socket
    // is connected without binding it first, because a sandbox can refuse
    // the one and allow the other.
    use rustix::net::{connect, socket, AddressFamily, SocketType};
    if let Ok(socket) = socket(AddressFamily::INET, SocketType::DGRAM, None) {
        let documentation = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 9);
        if connect(&socket, &documentation).is_ok() {
            return Err("the network is reachable".into());
        }
    }
    // The process limit is what makes the memory limit cover the whole
    // sandbox. Prove it holds here rather than trust that it was set. The
    // streams are pipes because the sandbox has no /dev/null to open, and a
    // start that failed for want of one would prove nothing.
    let program = this_program()?;
    let child = std::process::Command::new(program)
        .arg("none")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    if let Ok(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the helper can start other processes".into());
    }
    Ok(())
}

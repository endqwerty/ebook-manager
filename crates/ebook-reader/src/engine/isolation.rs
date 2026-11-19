//! The helper checks that it really is confined before it touches a book.

use std::net::UdpSocket;
use std::path::Path;

/// Returns why the process does not look sandboxed, or `Ok` when it does.
pub fn verify() -> Result<(), String> {
    for dir in [
        "/proc", "/home", "/etc", "/tmp", "/run", "/var", "/root", "/sys", "/dev",
    ] {
        if Path::new(dir).exists() {
            return Err(format!("{dir} is visible"));
        }
    }
    if std::fs::write("/write-probe", b"x").is_ok() {
        let _ = std::fs::remove_file("/write-probe");
        return Err("the filesystem is writable".into());
    }
    // 192.0.2.1 is reserved for documentation. With no network the kernel has
    // no route and refuses at once; no packet is sent either way.
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("192.0.2.1:9").is_ok() {
            return Err("the network is reachable".into());
        }
    }
    Ok(())
}

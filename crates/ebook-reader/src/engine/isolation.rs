//! The helper checks that it really is confined before it touches a book.

use std::net::UdpSocket;
use std::path::Path;
use std::process::Stdio;

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
    // The process limit is what makes the memory limit cover the whole
    // sandbox. Prove it holds here rather than trust that it was set.
    let child = std::process::Command::new(crate::wire::HELPER_IN_SANDBOX)
        .arg("none")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the helper can start other processes".into());
    }
    Ok(())
}

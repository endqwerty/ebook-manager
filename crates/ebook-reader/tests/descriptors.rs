//! S3 on macOS, in a process of its own because it lowers the descriptor
//! limit: a file left open above a limit that was lowered afterwards still
//! does not reach the sandbox.
#![cfg(target_os = "macos")]

mod common;

use common::*;
use ebook_reader::{Reader, ReaderConfig, ReaderError};
use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
use std::os::fd::AsRawFd;

#[test]
fn a_descriptor_above_a_lowered_limit_does_not_reach_the_sandbox() {
    const LEAKY: i32 = 200;
    let dir = Scratch::new();
    let secret = std::fs::File::open(dir.file("secret.txt", b"private")).unwrap();
    // SAFETY: dup2 onto a number this single-test process does not use.
    assert_eq!(unsafe { libc::dup2(secret.as_raw_fd(), LEAKY) }, LEAKY);
    let script = format!(
        "#!/bin/sh\nif [ -e /dev/fd/{LEAKY} ]; then echo '{LAUNCHER}: LEAKED' >&2; else echo '{LAUNCHER}: CLEAN' >&2; fi\nexit 1\n"
    );
    let fake = dir.file("fd-launcher", script.as_bytes());
    std::process::Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let inherited = getrlimit(Resource::Nofile);
    setrlimit(
        Resource::Nofile,
        Rlimit {
            current: Some(128),
            maximum: inherited.maximum,
        },
    )
    .unwrap();
    let cfg = ReaderConfig {
        sandbox_path: Some(fake),
        ..config()
    };
    let report = match Reader::new(cfg) {
        Err(ReaderError::SandboxUnavailable { reason }) => reason,
        other => panic!("expected the stand-in to fail, got {other:?}"),
    };
    assert_eq!(report, format!("{LAUNCHER}: CLEAN"));
}

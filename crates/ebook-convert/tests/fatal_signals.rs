use std::process::Command;
const HELPER: &str = env!("CARGO_BIN_EXE_ebook-convert-helper");

/// E6: no way the helper can fail is a crash. Every signal that would dump
/// core, and a stack overflow, ends it with exit status 4 instead.
#[test]
fn fatal_signals_end_the_helper_without_a_crash() {
    let fatal = [
        libc::SIGABRT,
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGILL,
        libc::SIGFPE,
        libc::SIGTRAP,
        libc::SIGSYS,
        libc::SIGQUIT,
        libc::SIGXCPU,
        libc::SIGXFSZ,
    ];
    let cases = fatal
        .iter()
        .map(|s| s.to_string())
        .chain(["stack".to_owned()]);
    for case in cases {
        // Also with every signal blocked, as a parent could leave them.
        for blocked in [false, true] {
            let mut cmd = Command::new(HELPER);
            cmd.args(["fatal-probe", &case]);
            if blocked {
                // SAFETY: only async-signal-safe calls between fork and exec.
                unsafe {
                    std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                        let mut all: libc::sigset_t = std::mem::zeroed();
                        libc::sigfillset(&mut all);
                        libc::sigprocmask(libc::SIG_BLOCK, &all, std::ptr::null_mut());
                        Ok(())
                    });
                }
            }
            let out = cmd.output().unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(4),
                "{case} blocked={blocked}: {:?} {stderr}",
                out.status
            );
            assert!(stderr.contains("helper: stopped"), "{case}: {stderr}");
        }
    }
}

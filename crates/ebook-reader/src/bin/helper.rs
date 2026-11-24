//! Sandboxed helper. Started by the host library inside its sandbox; refuses to
//! read a book when it finds itself outside a sandbox.
#![deny(unsafe_code)]

#[cfg(target_os = "macos")]
#[global_allocator]
static MEMORY: ebook_reader::engine::memory::Capped = ebook_reader::engine::memory::Capped;

/// Exit code for a helper stopped by a fatal signal.
const STOPPED: i32 = 4;

/// Every signal whose default action dumps core. Running out of memory or
/// stack aborts, and a limit or a bug can raise the rest. The desktop reports
/// each of these as a crash, so the helper turns them into an ordinary exit
/// that the host reports as a failed call.
const FATAL: [libc::c_int; 10] = [
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

const ALT_STACK_BYTES: usize = 64 * 1024;

/// The handler's stack. Static, so installing the handlers allocates nothing
/// and cannot itself fail for lack of memory.
static mut ALT_STACK: [u8; ALT_STACK_BYTES] = [0; ALT_STACK_BYTES];

extern "C" fn stop(_: libc::c_int) {
    const NOTE: &[u8] = b"helper: stopped after a fatal error\n";
    #[allow(unsafe_code)]
    // SAFETY: write(2) and _exit(2) are async-signal-safe and allocate
    // nothing; NOTE is a static buffer of the stated length.
    unsafe {
        libc::write(libc::STDERR_FILENO, NOTE.as_ptr().cast(), NOTE.len());
        libc::_exit(STOPPED);
    }
}

/// Install `stop` for every fatal signal, on its own stack so that a stack
/// overflow can still be handled, and unblock those signals in case the
/// parent blocked them. A blocked fault would otherwise take the default
/// action. This is the first thing `main` does and allocates nothing.
fn exit_on_fatal_signals() {
    #[allow(unsafe_code)]
    // SAFETY: ALT_STACK is used only here, once, before any other code runs,
    // and lives for the whole process. The handler only calls
    // async-signal-safe functions. Every struct is fully initialised before
    // the call that reads it.
    unsafe {
        let alt = libc::stack_t {
            ss_sp: std::ptr::addr_of_mut!(ALT_STACK).cast(),
            ss_flags: 0,
            ss_size: ALT_STACK_BYTES,
        };
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_ONSTACK;
        libc::sigfillset(&mut action.sa_mask);
        let mut fatal: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut fatal);
        for signal in FATAL {
            libc::sigaddset(&mut fatal, signal);
        }
        let installed = libc::sigaltstack(&alt, std::ptr::null_mut()) == 0
            && FATAL
                .iter()
                .all(|&s| libc::sigaction(s, &action, std::ptr::null_mut()) == 0)
            && libc::sigprocmask(libc::SIG_UNBLOCK, &fatal, std::ptr::null_mut()) == 0;
        if !installed {
            libc::_exit(STOPPED);
        }
    }
}

/// Test hook: raise one fatal signal, overflow the stack or use up a CPU
/// limit, so a test can check that the helper exits instead of crashing.
fn fatal_probe(what: Option<&str>) -> i32 {
    #[allow(unconditional_recursion)]
    fn deeper(n: u64) -> u64 {
        let pad = std::hint::black_box([n; 64]);
        deeper(pad[0] + 1) + pad[63]
    }
    match what {
        Some("stack") => {
            deeper(0);
        }
        Some("cpu") => {
            use rustix::process::{setrlimit, Resource, Rlimit};
            let second = Rlimit {
                current: Some(1),
                maximum: Some(1),
            };
            if setrlimit(Resource::Cpu, second).is_err() {
                return 2;
            }
            let mut spin = 0u64;
            loop {
                spin = std::hint::black_box(spin.wrapping_add(1));
            }
        }
        Some(number) => {
            let Ok(signal) = number.parse::<libc::c_int>() else {
                return 2;
            };
            if !FATAL.contains(&signal) {
                return 2;
            }
            #[allow(unsafe_code)]
            // SAFETY: raise(3) has no memory-safety preconditions.
            unsafe {
                libc::raise(signal);
            }
        }
        None => return 2,
    }
    0
}

/// Test hook: say which of these paths this process can see, read or write,
/// so a test can check what the sandbox it was started in allows.
fn visible_probe(paths: &[String]) -> i32 {
    use std::io::Read;
    let mut found = Vec::new();
    for path in paths {
        let read = std::fs::File::open(path)
            .and_then(|mut file| file.read(&mut [0u8; 1]))
            .is_ok();
        let seen = std::fs::symlink_metadata(path).is_ok();
        if std::fs::OpenOptions::new().append(true).open(path).is_ok() {
            found.push(format!("writes {path}"));
        } else if read {
            found.push(format!("reads {path}"));
        } else if seen {
            found.push(format!("sees {path}"));
        }
    }
    eprint!("{}", found.join(", "));
    5
}

fn main() {
    exit_on_fatal_signals();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fatal-probe") {
        std::process::exit(fatal_probe(args.get(1).map(String::as_str)));
    }
    if args.first().map(String::as_str) == Some("visible-probe") {
        std::process::exit(visible_probe(&args[1..]));
    }
    std::process::exit(ebook_reader::engine::run::main(&args));
}

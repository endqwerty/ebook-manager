//! Sandboxed helper. Started by the host library inside bubblewrap; refuses to
//! read a book when it finds itself outside a sandbox.
#![deny(unsafe_code)]

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
/// overflow can still be handled. Runs before any limit is set.
fn exit_on_fatal_signals() {
    let stack: &'static mut [u8] = Vec::leak(vec![0u8; ALT_STACK_BYTES]);
    #[allow(unsafe_code)]
    // SAFETY: the alternate stack is leaked, so it outlives the process's use
    // of it. The handler only calls async-signal-safe functions. Both structs
    // are fully initialised before the calls that read them.
    unsafe {
        let alt = libc::stack_t {
            ss_sp: stack.as_mut_ptr().cast(),
            ss_flags: 0,
            ss_size: stack.len(),
        };
        if libc::sigaltstack(&alt, std::ptr::null_mut()) != 0 {
            libc::_exit(STOPPED);
        }
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_ONSTACK;
        libc::sigfillset(&mut action.sa_mask);
        for signal in FATAL {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                libc::_exit(STOPPED);
            }
        }
    }
}

/// Test hook: raise one fatal signal, or overflow the stack, so a test can
/// check that the helper exits instead of crashing.
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

fn main() {
    exit_on_fatal_signals();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fatal-probe") {
        std::process::exit(fatal_probe(args.get(1).map(String::as_str)));
    }
    std::process::exit(ebook_reader::engine::run::main(&args));
}

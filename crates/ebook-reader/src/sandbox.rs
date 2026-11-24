//! The command that starts the helper confined: bubblewrap on Linux,
//! `sandbox-exec` on macOS. The helper checks the result for itself
//! (`engine/isolation.rs`), so a launcher that confines nothing is refused.

use crate::host::Inner;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
pub use linux::{command, MISSING, NAME, PLACES};
#[cfg(target_os = "macos")]
pub use macos::{command, MISSING, NAME, PLACES};

#[cfg(target_os = "linux")]
mod linux {
    use super::{Command, Inner, Path};
    use crate::wire::{BOOK_IN_SANDBOX, HELPER_IN_SANDBOX};

    pub const NAME: &str = "bwrap";
    pub const PLACES: [&str; 2] = ["/usr/bin/bwrap", "/bin/bwrap"];
    pub const MISSING: &str = "bubblewrap (bwrap) is not installed";

    /// Read-only system library directories the helper's loader may need.
    const LIBRARY_DIRS: [&str; 4] = ["/usr/lib", "/usr/lib64", "/lib", "/lib64"];

    pub fn command(inner: &Inner, book: Option<&Path>, cpu_seconds: &str) -> Command {
        let mut cmd = Command::new(&inner.sandbox);
        cmd.args([
            "--unshare-all",
            "--unshare-user",
            "--disable-userns",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
        ]);
        cmd.args(["--cap-drop", "ALL"]);
        for dir in LIBRARY_DIRS {
            cmd.args(["--ro-bind-try", dir, dir]);
        }
        cmd.arg("--ro-bind")
            .arg(&inner.helper)
            .arg(HELPER_IN_SANDBOX);
        if let Some(book) = book {
            cmd.arg("--ro-bind").arg(book).arg(BOOK_IN_SANDBOX);
        }
        cmd.args(["--remount-ro", "/", "--chdir", "/", "--", HELPER_IN_SANDBOX]);
        cmd.arg("session").arg(&inner.limits_json).arg(cpu_seconds);
        cmd
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Command, Inner, Path};
    use std::ffi::OsString;

    pub const NAME: &str = "sandbox-exec";
    pub const PLACES: [&str; 1] = ["/usr/bin/sandbox-exec"];
    pub const MISSING: &str = "sandbox-exec is not installed";

    /// Everything is refused except running and reading the helper and
    /// reading what the system's loader needs to start it, which includes
    /// the names in the root directory. Paths arrive as parameters, so none
    /// is written into this text.
    const PROFILE: &str = r#"(version 1)
(deny default (with no-log))
(allow process-exec (literal (param "HELPER")))
(allow file-read* (literal (param "HELPER")))
(allow file-read*
  (subpath "/usr/lib")
  (subpath "/System/Library/dyld")
  (subpath "/System/Cryptexes/OS/System/Library/dyld")
  (subpath "/System/Volumes/Preboot/Cryptexes/OS/System/Library/dyld")
  (subpath "/private/preboot/Cryptexes/OS/System/Library/dyld"))
(allow file-read* (literal "/"))
(allow sysctl-read)
"#;

    const BOOK_RULE: &str = r#"(allow file-read* (literal (param "BOOK")))
"#;

    fn parameter(name: &str, path: &Path) -> OsString {
        let mut value = OsString::from(name);
        value.push("=");
        value.push(path);
        value
    }

    /// `inner.helper` and `book` are resolved paths: the profile compares
    /// against the file a path leads to, not the path as written.
    pub fn command(inner: &Inner, book: Option<&Path>, cpu_seconds: &str) -> Command {
        let mut cmd = Command::new(&inner.sandbox);
        let profile = match book {
            Some(_) => [PROFILE, BOOK_RULE].concat(),
            None => PROFILE.to_owned(),
        };
        cmd.arg("-p").arg(profile);
        cmd.arg("-D").arg(parameter("HELPER", &inner.helper));
        if let Some(book) = book {
            cmd.arg("-D").arg(parameter("BOOK", book));
        }
        cmd.arg(&inner.helper);
        cmd.arg("session").arg(&inner.limits_json).arg(cpu_seconds);
        if let Some(book) = book {
            cmd.arg(book);
        }
        cmd
    }
}

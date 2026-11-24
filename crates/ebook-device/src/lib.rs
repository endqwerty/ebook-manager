//! Copy a managed EPUB to a user-selected folder on a mounted volume.

use rustix::fs::{linkat, openat, renameat_with, unlinkat, AtFlags, Mode, OFlags, RenameFlags};
use rustix::io::Errno;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq)]
pub enum ExportOutcome {
    Copied(PathBuf),
    AlreadyPresent(PathBuf),
}

fn device_filename(title: &str) -> String {
    let mut stem = String::new();
    for c in title.trim().chars() {
        if stem.len() + c.len_utf8() > 180 {
            break;
        }
        let invalid = c.is_control() || r#"<>:"/\|?*"#.contains(c);
        stem.push(if invalid { '_' } else { c });
    }
    let stem = stem.trim_matches(['.', ' ']);
    let stem = if stem.is_empty() { "Book" } else { stem };
    let prefix = stem
        .split('.')
        .next()
        .unwrap_or(stem)
        .trim_end()
        .to_ascii_uppercase();
    let reserved = matches!(prefix.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            prefix
                .strip_prefix(p)
                .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        });
    format!("{}{stem}.epub", if reserved { "_" } else { "" })
}

fn regular_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected a regular book file",
        ));
    }
    Ok(file)
}

fn digest(file: &mut File) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    io::copy(file, &mut hash)?;
    Ok(hash.finalize().into())
}

fn compare_existing(
    mut existing: File,
    expected: [u8; 32],
    display: PathBuf,
) -> io::Result<ExportOutcome> {
    if digest(&mut existing)? == expected {
        return Ok(ExportOutcome::AlreadyPresent(display));
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "{} already contains a different file. Choose another destination folder.",
            display.display()
        ),
    ))
}

/// Open a regular file by name inside the open directory. Every operation
/// on the device goes through that directory's descriptor, so the transfer
/// stays on it if its path or mount point changes.
fn open_in(dir: &File, name: &str) -> io::Result<File> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let file = File::from(openat(dir, name, flags, Mode::empty())?);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected a regular book file",
        ));
    }
    Ok(file)
}

/// A partly written copy under a private name. Dropping it removes that
/// name, which is already gone once the copy has been published by renaming.
struct Staged<'a> {
    dir: &'a File,
    name: String,
    file: File,
}

impl<'a> Staged<'a> {
    fn create(dir: &'a File) -> io::Result<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let flags =
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        loop {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let name = format!(
                ".ebook-{}-{}-{nanos}.part",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            match openat(dir, &name, flags, Mode::from_raw_mode(0o600)) {
                Ok(fd) => {
                    return Ok(Staged {
                        dir,
                        name,
                        file: File::from(fd),
                    })
                }
                Err(Errno::EXIST) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Give the copy its real name unless something already has it. A
    /// filesystem that can't rename without replacing gets a link instead.
    fn publish(&self, name: &str) -> io::Result<()> {
        match renameat_with(self.dir, &self.name, self.dir, name, RenameFlags::NOREPLACE) {
            Err(Errno::INVAL | Errno::NOSYS | Errno::NOTSUP) => {
                linkat(self.dir, &self.name, self.dir, name, AtFlags::empty())?
            }
            other => other?,
        }
        Ok(())
    }
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        let _ = unlinkat(self.dir, &self.name, AtFlags::empty());
    }
}

pub fn export_epub(source: &Path, directory: &Path, title: &str) -> io::Result<ExportOutcome> {
    let mut source = regular_file(source)?;
    let dir = File::open(directory)?;
    if !dir.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "Choose a device folder",
        ));
    }
    let name = device_filename(title);
    let expected = digest(&mut source)?;
    let try_name =
        |source: &mut File, name: &str| copy_to(source, &dir, name, directory.join(name), expected);
    match try_name(&mut source, &name) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let suffix: String = expected[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let stem = name
                .strip_suffix(".epub")
                .expect("device filename has an EPUB extension");
            try_name(&mut source, &format!("{stem}-{suffix}.epub"))
        }
        result => result,
    }
}

fn copy_to(
    source: &mut File,
    dir: &File,
    name: &str,
    display: PathBuf,
    expected: [u8; 32],
) -> io::Result<ExportOutcome> {
    match open_in(dir, name) {
        Ok(existing) => return compare_existing(existing, expected, display),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut staged = Staged::create(dir)?;
    source.rewind()?;
    io::copy(source, &mut staged.file)?;
    staged.file.sync_all()?;
    staged.file.rewind()?;
    if digest(&mut staged.file)? != expected {
        return Err(io::Error::other(
            "The copy on the device did not match the original",
        ));
    }
    match staged.publish(name) {
        Ok(()) => {
            dir.sync_all()?;
            Ok(ExportOutcome::Copied(display))
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            compare_existing(open_in(dir, name)?, expected, display)
        }
        Err(e) => Err(e),
    }
}

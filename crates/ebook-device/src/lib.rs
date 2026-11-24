//! Copy a managed EPUB to a user-selected folder on a mounted volume.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

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

/// A path that names the open directory. On Linux it goes through the
/// descriptor, so every operation stays on that directory if its mount point
/// changes.
#[cfg(target_os = "linux")]
fn held(dir: &File) -> io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    Ok(PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())))
}

/// macOS has no path through a descriptor, only the directory's path as it
/// is now. [`still_held`] checks afterwards that it led to the same place.
#[cfg(target_os = "macos")]
fn held(dir: &File) -> io::Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let path = rustix::fs::getpath(dir)?;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(
        path.into_bytes(),
    )))
}

fn still_held(dir: &File, root: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let (held, named) = (dir.metadata()?, std::fs::metadata(root)?);
    if (held.dev(), held.ino()) == (named.dev(), named.ino()) {
        Ok(())
    } else {
        Err(io::Error::other(
            "The device folder changed during the copy. Check the device and send the book again.",
        ))
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
    let root = held(&dir)?;
    let name = device_filename(title);
    let expected = digest(&mut source)?;
    let try_name = |source: &mut File, name: &str| {
        copy_to(
            source,
            &dir,
            &root,
            &root.join(name),
            directory.join(name),
            expected,
        )
    };
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
    root: &Path,
    target: &Path,
    display: PathBuf,
    expected: [u8; 32],
) -> io::Result<ExportOutcome> {
    match regular_file(target) {
        Ok(existing) => return compare_existing(existing, expected, display),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut staged = tempfile::Builder::new()
        .prefix(".ebook-")
        .suffix(".part")
        .tempfile_in(root)?;
    source.rewind()?;
    io::copy(source, staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged.as_file_mut().rewind()?;
    if digest(staged.as_file_mut())? != expected {
        return Err(io::Error::other(
            "The copy on the device did not match the original",
        ));
    }
    match staged.persist_noclobber(target) {
        Ok(_) => {
            still_held(dir, root)?;
            dir.sync_all()?;
            Ok(ExportOutcome::Copied(display))
        }
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
            compare_existing(regular_file(target)?, expected, display)
        }
        Err(e) => Err(e.error),
    }
}

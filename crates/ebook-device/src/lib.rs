//! Copy a managed EPUB to a user-selected folder on a mounted Linux volume.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::fd::AsRawFd;
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

fn copy_and_hash(input: &mut File, output: &mut impl Write) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let count = match input.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

fn compare_existing(
    mut existing: File,
    expected: [u8; 32],
    display: PathBuf,
) -> io::Result<ExportOutcome> {
    if copy_and_hash(&mut existing, &mut io::sink())? == expected {
        return Ok(ExportOutcome::AlreadyPresent(display));
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "{} already contains a different file. Choose another folder or change the book title.",
            display.display()
        ),
    ))
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
    // Keep all filesystem operations on the selected directory if its mount point changes.
    let root = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    let name = device_filename(title);
    let target = root.join(&name);
    let display = directory.join(name);
    match regular_file(&target) {
        Ok(existing) => {
            return compare_existing(
                existing,
                copy_and_hash(&mut source, &mut io::sink())?,
                display,
            )
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut staged = tempfile::Builder::new()
        .prefix(".ebook-")
        .suffix(".part")
        .tempfile_in(&root)?;
    let expected = copy_and_hash(&mut source, staged.as_file_mut())?;
    staged.as_file().sync_all()?;
    staged.as_file_mut().rewind()?;
    if copy_and_hash(staged.as_file_mut(), &mut io::sink())? != expected {
        return Err(io::Error::other(
            "The copy on the device did not match the original",
        ));
    }
    match staged.persist_noclobber(&target) {
        Ok(_) => {
            dir.sync_all()?;
            Ok(ExportOutcome::Copied(display))
        }
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
            compare_existing(regular_file(&target)?, expected, display)
        }
        Err(e) => Err(e.error),
    }
}

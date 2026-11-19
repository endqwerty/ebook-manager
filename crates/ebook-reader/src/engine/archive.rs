//! Bounded, read-only access to the ZIP container. Nothing is written to disk,
//! so entry names are only ever lookup keys.

use crate::model::Limits;
use crate::wire::{WireError, WireErrorKind};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::{CompressionMethod, ZipArchive};

/// Below this much output the ratio is not judged: short, repetitive
/// documents are ordinary, and the size limits already bound them.
const RATIO_FLOOR: u64 = 4 * 1024 * 1024;

pub struct Archive {
    zip: ZipArchive<File>,
    folded: Option<HashMap<String, Option<usize>>>,
    encrypted_entries: bool,
    unpacked: u64,
    max_unpacked: u64,
    max_ratio: u64,
}

impl Archive {
    pub fn open(file: File, limits: &Limits) -> Result<Archive, WireError> {
        let len = file
            .metadata()
            .map_err(|e| WireError::invalid(format!("cannot read file size: {e}")))?
            .len();
        if len > limits.max_file_bytes {
            return Err(WireError::limit("file size"));
        }
        // Read the entry count from the end record first, so an archive that
        // lists millions of entries is refused before its directory is parsed.
        if declared_entries(&file, len).is_some_and(|n| n > u64::from(limits.max_archive_entries)) {
            return Err(WireError::limit("number of files in the book"));
        }
        let zip = ZipArchive::new(file)
            .map_err(|_| WireError::new(WireErrorKind::NotEpub, "not a ZIP container"))?;
        if zip.len() > limits.max_archive_entries as usize {
            return Err(WireError::limit("number of files in the book"));
        }
        let mut zip = zip;
        let mut encrypted_entries = false;
        for index in 0..zip.len() {
            if zip.by_index_raw(index).is_ok_and(|entry| entry.encrypted()) {
                encrypted_entries = true;
                break;
            }
        }
        Ok(Archive {
            zip,
            folded: None,
            encrypted_entries,
            unpacked: 0,
            max_unpacked: limits.max_unpacked_bytes,
            max_ratio: u64::from(limits.max_compression_ratio.max(1)),
        })
    }

    fn index_of(&mut self, name: &str) -> Option<usize> {
        if let Some(i) = self.zip.index_for_name(name) {
            return Some(i);
        }
        // Real books sometimes differ from their manifest in letter case.
        // Accept that only when exactly one entry matches.
        if self.folded.is_none() {
            let mut map: HashMap<String, Option<usize>> = HashMap::new();
            let names: Vec<String> = self.zip.file_names().map(str::to_owned).collect();
            for n in names {
                let idx = self.zip.index_for_name(&n);
                map.entry(n.to_lowercase())
                    .and_modify(|v| *v = None)
                    .or_insert(idx);
            }
            self.folded = Some(map);
        }
        self.folded
            .as_ref()
            .and_then(|m| m.get(&name.to_lowercase()).copied().flatten())
    }

    /// Some entry is ZIP-encrypted, wherever it is in the archive.
    pub fn has_encrypted_entries(&self) -> bool {
        self.encrypted_entries
    }

    pub fn contains(&mut self, name: &str) -> bool {
        self.index_of(name).is_some()
    }

    /// Unpack one entry, refusing more than `cap` bytes of output.
    pub fn read(&mut self, name: &str, cap: u64) -> Result<Vec<u8>, WireError> {
        let index = self
            .index_of(name)
            .ok_or_else(|| WireError::invalid(format!("missing file in book: {}", short(name))))?;
        let packed;
        {
            let raw = self
                .zip
                .by_index_raw(index)
                .map_err(|e| WireError::invalid(format!("unreadable entry: {e}")))?;
            if raw.encrypted() {
                return Err(WireError::new(
                    WireErrorKind::Protected,
                    "encrypted ZIP entry",
                ));
            }
            match raw.compression() {
                CompressionMethod::Stored | CompressionMethod::Deflated => {}
                _ => {
                    return Err(WireError::new(
                        WireErrorKind::Compression,
                        "unsupported compression method",
                    ))
                }
            }
            if raw.size() > cap {
                return Err(WireError::limit(format!("size of {}", short(name))));
            }
            packed = raw.compressed_size();
        }
        // Never unpack more than this run's remaining allowance.
        let left = self.max_unpacked.saturating_sub(self.unpacked);
        let over_budget = cap > left;
        let cap = cap.min(left);
        let entry = self
            .zip
            .by_index(index)
            .map_err(|e| WireError::invalid(format!("unreadable entry: {e}")))?;
        let mut out = Vec::new();
        // The declared size is the book's claim. Stop on actual output.
        let read = entry.take(cap.saturating_add(1)).read_to_end(&mut out);
        // Unpacked bytes count against the run whether or not the read ended well.
        let size = out.len() as u64;
        self.unpacked = self.unpacked.saturating_add(size.min(cap));
        read.map_err(|e| WireError::invalid(format!("damaged entry {}: {e}", short(name))))?;
        if size > cap {
            let what = if over_budget {
                "total unpacked size".to_owned()
            } else {
                format!("size of {}", short(name))
            };
            return Err(WireError::limit(what));
        }
        // Megabytes of output from almost no input is the shape of a
        // decompression bomb, whatever sizes the archive declares.
        if size > RATIO_FLOOR && size > packed.max(1).saturating_mul(self.max_ratio) {
            return Err(WireError::limit(format!(
                "compression ratio of {}",
                short(name)
            )));
        }
        Ok(out)
    }
}

/// The entry count the archive's end record declares, read without parsing
/// the directory. `None` when no end record is found; the ZIP reader then
/// gives the verdict.
fn declared_entries(mut file: &File, len: u64) -> Option<u64> {
    use std::io::{Seek, SeekFrom};
    const END: &[u8] = b"PK\x05\x06";
    // The end record is 22 bytes plus a comment of at most 65,535, and a
    // ZIP64 archive puts a 20-byte locator just before it.
    let tail_len = len.min(20 + 22 + 65_535);
    let tail_at = len - tail_len;
    file.seek(SeekFrom::Start(tail_at)).ok()?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail).ok()?;
    let at = (0..tail.len().checked_sub(21)?)
        .rev()
        .find(|&i| &tail[i..i + 4] == END)?;
    let short_count = u64::from(u16::from_le_bytes([tail[at + 10], tail[at + 11]]));
    let mut count = short_count;
    if short_count == 0xFFFF && at >= 20 && &tail[at - 20..at - 16] == b"PK\x06\x07" {
        // ZIP64: the locator before the end record points at the real count.
        let mut offset = [0u8; 8];
        offset.copy_from_slice(&tail[at - 12..at - 4]);
        let mut record = [0u8; 56];
        file.seek(SeekFrom::Start(u64::from_le_bytes(offset)))
            .ok()?;
        file.read_exact(&mut record).ok()?;
        if &record[..4] == b"PK\x06\x06" {
            let mut total = [0u8; 8];
            total.copy_from_slice(&record[32..40]);
            count = u64::from_le_bytes(total);
        }
    }
    file.seek(SeekFrom::Start(0)).ok()?;
    Some(count)
}

fn short(name: &str) -> String {
    let mut s = super::clean_inline(name);
    super::truncate_chars(&mut s, 120);
    s
}

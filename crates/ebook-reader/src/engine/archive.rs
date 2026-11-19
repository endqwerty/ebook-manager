//! Bounded, read-only access to the ZIP container. Nothing is written to disk,
//! so entry names are only ever lookup keys.

use crate::model::Limits;
use crate::wire::{WireError, WireErrorKind};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::{CompressionMethod, ZipArchive};

const RATIO_FLOOR: u64 = 1024 * 1024;

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
        entry
            .take(cap.saturating_add(1))
            .read_to_end(&mut out)
            .map_err(|e| WireError::invalid(format!("damaged entry {}: {e}", short(name))))?;
        let size = out.len() as u64;
        if size > cap {
            let what = if over_budget {
                "total unpacked size".to_owned()
            } else {
                format!("size of {}", short(name))
            };
            return Err(WireError::limit(what));
        }
        self.unpacked += size;
        // A megabyte or more of output from almost no input is the shape of a
        // decompression bomb, whatever sizes the archive declares.
        if size > RATIO_FLOOR && size / packed.max(1) > self.max_ratio {
            return Err(WireError::limit(format!(
                "compression ratio of {}",
                short(name)
            )));
        }
        Ok(out)
    }
}

fn short(name: &str) -> String {
    let mut s = super::clean_inline(name);
    super::truncate_chars(&mut s, 120);
    s
}

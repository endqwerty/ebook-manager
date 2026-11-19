//! Name the book's pictures by their place in the manifest and read one.
//!
//! The bytes are checked by signature and passed on undecoded. Decoding is
//! for whoever draws them.

use super::archive::Archive;
use super::package::Package;
use super::paths;
use crate::model::{Limits, ResourceKind};
use crate::wire::{base64_encode, WireError, WireResource};
use std::collections::HashMap;

fn kind_for(media_type: &str) -> Option<ResourceKind> {
    match media_type {
        "image/png" => Some(ResourceKind::Png),
        "image/jpeg" | "image/jpg" => Some(ResourceKind::Jpeg),
        "image/gif" => Some(ResourceKind::Gif),
        "image/webp" => Some(ResourceKind::Webp),
        _ => None,
    }
}

/// Container path to manifest position for every picture this build serves.
/// When two items name one file, the first wins.
pub fn pictures(package: &Package) -> HashMap<&str, u32> {
    let mut map = HashMap::new();
    for (i, item) in package.manifest.iter().enumerate() {
        if kind_for(&item.media_type).is_some() {
            map.entry(item.path.as_str()).or_insert(i as u32);
        }
    }
    map
}

/// The picture a reference written in the file `base` names.
pub fn resolve(pictures: &HashMap<&str, u32>, base: &str, reference: &str) -> Option<u32> {
    let local = paths::resolve(base, reference)?;
    pictures.get(local.path.as_str()).copied()
}

pub fn read(
    archive: &mut Archive,
    package: &Package,
    limits: &Limits,
    id: u32,
) -> Result<WireResource, WireError> {
    let item = package
        .manifest
        .get(id as usize)
        .ok_or_else(|| WireError::invalid("no such resource in the book"))?;
    let declared = kind_for(&item.media_type)
        .ok_or_else(|| WireError::invalid("the resource is not a picture this build shows"))?;
    let bytes = archive.read(&item.path, limits.max_resource_bytes)?;
    if ResourceKind::of(&bytes) != Some(declared) {
        return Err(WireError::invalid(
            "a picture's content is not the type the book declares",
        ));
    }
    Ok(WireResource {
        kind: declared,
        base64: base64_encode(&bytes),
    })
}

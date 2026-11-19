//! The thumbnail cache: one PNG per cover under the app data folder, named
//! by the digest the book record stores. The folder is a cache and any file
//! in it can be made again from the managed EPUB.
use anyhow::Result;
use ebook_library::{CoverFacts, SurveyFacts};
use ebook_reader::{Survey, Thumbnail};
use gtk::{gdk, glib};
use gtk4 as gtk;
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// The survey this build asks the helper for.
pub const SURVEY_VERSION: u32 = 1;

pub fn folder(app_data: &Path) -> PathBuf {
    app_data.join("cache/thumbs")
}

pub fn file(folder: &Path, digest: &str) -> PathBuf {
    folder.join(format!("{digest}.png"))
}

/// SHA-256 of the width and height as big-endian 32-bit numbers, then the pixels.
pub fn digest(thumbnail: &Thumbnail) -> String {
    let mut hash = Sha256::new();
    hash.update(thumbnail.width().to_be_bytes());
    hash.update(thumbnail.height().to_be_bytes());
    hash.update(thumbnail.pixels());
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What the book record keeps of a survey.
pub fn facts(survey: &Survey) -> SurveyFacts {
    SurveyFacts {
        version: SURVEY_VERSION,
        words: survey.words,
        cover: survey.cover.as_ref().map(|c| CoverFacts {
            digest: digest(&c.thumbnail),
            average: c.average.to_u32(),
            spine: c.spine.to_u32(),
        }),
    }
}

/// Writes the thumbnail as `<digest>.png` through a temporary file and a
/// rename, so a reader of the folder never sees half a file.
pub fn write(folder: &Path, digest: &str, thumbnail: &Thumbnail) -> Result<()> {
    std::fs::create_dir_all(folder)?;
    let texture = gdk::MemoryTexture::new(
        thumbnail.width() as i32,
        thumbnail.height() as i32,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from(thumbnail.pixels()),
        thumbnail.width() as usize * 3,
    );
    let mut staged = tempfile::NamedTempFile::new_in(folder)?;
    staged.write_all(&gdk::prelude::TextureExt::save_to_png_bytes(&texture))?;
    staged.persist(file(folder, digest))?;
    Ok(())
}

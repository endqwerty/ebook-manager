//! Verified PNG cover thumbnail storage under `<library>/cache/thumbs`.

use anyhow::Result;
use ebook_library::{CoverFacts, SurveyFacts};
use ebook_reader::{Survey, Thumbnail};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub const SURVEY_VERSION: u32 = 1;
const LARGEST_FILE: u64 = 512 * 1024;

#[derive(Clone, Copy)]
pub struct Pixels<'a> {
    pub width: u32,
    pub height: u32,
    pub rgb: &'a [u8],
}

impl<'a> From<&'a Thumbnail> for Pixels<'a> {
    fn from(thumbnail: &'a Thumbnail) -> Self {
        Self {
            width: thumbnail.width(),
            height: thumbnail.height(),
            rgb: thumbnail.pixels(),
        }
    }
}

pub fn folder(app_data: &Path) -> PathBuf {
    app_data.join("cache/thumbs")
}

pub fn file(folder: &Path, digest: &str) -> PathBuf {
    folder.join(format!("{digest}.png"))
}

pub fn digest(cover: Pixels<'_>) -> String {
    let mut hash = Sha256::new();
    hash.update(cover.width.to_be_bytes());
    hash.update(cover.height.to_be_bytes());
    hash.update(cover.rgb);
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn facts(survey: &Survey) -> SurveyFacts {
    SurveyFacts {
        version: SURVEY_VERSION,
        words: survey.words,
        cover: survey.cover.as_ref().map(|c| CoverFacts {
            digest: digest((&c.thumbnail).into()),
            average: c.average.to_u32(),
            spine: c.spine.to_u32(),
        }),
    }
}

pub fn encode_png(cover: Pixels<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, cover.width, cover.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(cover.rgb)?;
    writer.finish()?;
    Ok(out)
}

pub fn write(folder: &Path, digest: &str, cover: Pixels<'_>) -> Result<Vec<u8>> {
    std::fs::create_dir_all(folder)?;
    let bytes = encode_png(cover)?;
    let mut staged = NamedTempFile::new_in(folder)?;
    staged.write_all(&bytes)?;
    staged.flush()?;
    staged.persist(file(folder, digest))?;
    Ok(bytes)
}

fn plain_png(png: &[u8]) -> Option<(u32, u32)> {
    let number = |bytes: &[u8]| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut rest = png.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
    let mut size = None;
    let mut data = false;
    loop {
        let length = number(rest.get(..4)?) as usize;
        let name = rest.get(4..8)?;
        let body = rest.get(8..8usize.checked_add(length)?)?;
        rest = rest.get(12 + length..)?;
        match (name, size, data) {
            (b"IHDR", None, false) if length == 13 => {
                let (width, height) = (number(&body[..4]), number(&body[4..8]));
                let plain = body[8..] == [8, 2, 0, 0, 0];
                let fits = (1..=Thumbnail::MAX_WIDTH).contains(&width)
                    && (1..=Thumbnail::MAX_HEIGHT).contains(&height);
                if !(plain && fits) {
                    return None;
                }
                size = Some((width, height));
            }
            (b"IDAT", Some(_), _) => data = true,
            (b"IEND", Some(size), true) if length == 0 && rest.is_empty() => return Some(size),
            _ => return None,
        }
    }
}

pub fn load_verified_png(folder: &Path, wanted: &str) -> Option<Vec<u8>> {
    let found = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(file(folder, wanted))
        .ok()?;
    if !found.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    found.take(LARGEST_FILE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > LARGEST_FILE {
        return None;
    }
    let (width, height) = plain_png(&bytes)?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().ok()?;
    let across = width as usize * 3;
    let mut rgb = vec![0; across * height as usize];
    let frame = reader.next_frame(&mut rgb).ok()?;
    let as_read = (frame.width, frame.height, frame.color_type, frame.bit_depth);
    if as_read != (width, height, png::ColorType::Rgb, png::BitDepth::Eight) {
        return None;
    }
    let cover = Pixels {
        width,
        height,
        rgb: &rgb,
    };
    if digest(cover) != wanted {
        return None;
    }
    encode_png(cover).ok()
}

//! Find, decode and shrink the cover picture. Runs only inside the helper:
//! the image is untrusted bytes from the book, and image decoders are a
//! common place for memory faults.

use super::archive::Archive;
use super::package::Package;
use crate::model::{Limits, Rgb, Thumbnail};
use crate::wire::WireCover;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage};
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};

const MAX_W: u64 = Thumbnail::MAX_WIDTH as u64;
const MAX_H: u64 = Thumbnail::MAX_HEIGHT as u64;
/// A smaller picture would have to be blown up more than this to fill a
/// thumbnail, and would only show as blur.
const MAX_ENLARGEMENT: u64 = 3;

/// The declared cover as a thumbnail with its colours, or `None` when there
/// is no usable one. Every way a cover can fail ends here as `None`, so the
/// rest of a survey goes on.
pub fn cover(archive: &mut Archive, package: &Package, limits: &Limits) -> Option<WireCover> {
    let item = package.cover.as_ref()?;
    let bytes = archive.read(&item.path, limits.max_cover_bytes).ok()?;
    // A decoder bug that panics costs the cover, not the word count. Running
    // out of memory still stops the helper; the pixel limit keeps that rare.
    let image =
        catch_unwind(AssertUnwindSafe(|| decode(&bytes, limits.max_cover_pixels))).ok()??;
    drop(bytes);
    let (width, height) = fit(image.width(), image.height())?;
    let pixels = shrink(&image, width, height);
    drop(image);
    let rgb = pixels.as_chunks::<3>().0;
    let average = average(rgb);
    let spine = spine(rgb, width, package.metadata.right_to_left);
    Some(WireCover {
        width,
        height,
        pixels,
        average,
        spine,
    })
}

/// Decode JPEG, PNG, GIF (its first frame) or WebP, recognised by content
/// rather than by the file's declared type. The size is checked from the
/// header before any pixel is decoded.
fn decode(bytes: &[u8], max_pixels: u64) -> Option<RgbaImage> {
    let format = image::guess_format(bytes).ok()?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP
    ) {
        return None;
    }
    if format == ImageFormat::Jpeg && !jpeg_is_whole(bytes) {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut bounds = image::Limits::default();
    // Room for every allowed pixel at four bytes; a deeper format of the
    // largest size is refused by the decoder.
    bounds.max_alloc = Some(max_pixels.saturating_mul(4));
    reader.limits(bounds);
    let decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > max_pixels {
        return None;
    }
    Some(DynamicImage::from_decoder(decoder).ok()?.into_rgba8())
}

/// The JPEG decoder repairs a file that stops early and returns a picture
/// with the missing part filled in, which is not the book's cover. A whole
/// file has its end-of-image marker after its last start-of-scan marker.
/// Neither pair of bytes can occur inside the picture data, where a 0xFF is
/// always followed by a zero.
fn jpeg_is_whole(bytes: &[u8]) -> bool {
    let last = |marker: u8| bytes.windows(2).rposition(|w| w == [0xFF, marker]);
    matches!((last(0xDA), last(0xD9)), (Some(scan), Some(end)) if end > scan)
}

/// The thumbnail's size: the picture scaled to fit within the maximum with
/// its aspect kept, or `None` if that means enlarging it too far.
fn fit(w: u32, h: u32) -> Option<(u32, u32)> {
    let (w, h) = (u64::from(w), u64::from(h));
    let (tw, th) = if w * MAX_H >= h * MAX_W {
        (MAX_W, ((h * MAX_W + w / 2) / w).max(1))
    } else {
        (((w * MAX_H + h / 2) / h).max(1), MAX_H)
    };
    if tw > MAX_ENLARGEMENT * w || th > MAX_ENLARGEMENT * h {
        return None;
    }
    Some((u32::try_from(tw).ok()?, u32::try_from(th).ok()?))
}

/// The source positions that output position `j` covers and the share of
/// each, so every output pixel is the area average of what lies under it.
/// Computed as it is walked: a list per output position would hold an entry
/// for every source column, which for a very wide picture is more memory
/// than the picture itself.
fn span(source: u32, target: u32, j: u32) -> impl Iterator<Item = (usize, f32)> + Clone {
    let scale = f64::from(source) / f64::from(target);
    let start = f64::from(j) * scale;
    let end = (f64::from(j) + 1.0) * scale;
    let first = start.floor() as u32;
    let last = (end.ceil() as u32).min(source);
    (first..last).filter_map(move |i| {
        let cover = end.min(f64::from(i) + 1.0) - start.max(f64::from(i));
        (cover > 0.0).then_some((i as usize, (cover / scale) as f32))
    })
}

/// Area-average the picture down (or up) to `width` by `height`, drawing any
/// transparency over white. Works one output row at a time, so memory stays
/// at a row of the output whatever the picture's shape.
fn shrink(image: &RgbaImage, width: u32, height: u32) -> Vec<u8> {
    let raw = image.as_raw();
    let stride = image.width() as usize * 4;
    let over_white = |at: usize| -> [f32; 3] {
        let alpha = f32::from(raw[at + 3]) / 255.0;
        let paper = 255.0 * (1.0 - alpha);
        [
            f32::from(raw[at]) * alpha + paper,
            f32::from(raw[at + 1]) * alpha + paper,
            f32::from(raw[at + 2]) * alpha + paper,
        ]
    };
    let mut out = Vec::with_capacity(width as usize * height as usize * 3);
    let mut sum = vec![[0f32; 3]; width as usize];
    for j in 0..height {
        sum.iter_mut().for_each(|s| *s = [0.0; 3]);
        for (y, wy) in span(image.height(), height, j) {
            for (i, acc) in sum.iter_mut().enumerate() {
                for (x, wx) in span(image.width(), width, i as u32) {
                    let p = over_white(y * stride + x * 4);
                    let w = wx * wy;
                    acc[0] += p[0] * w;
                    acc[1] += p[1] * w;
                    acc[2] += p[2] * w;
                }
            }
        }
        for acc in &sum {
            out.extend(acc.iter().map(|c| c.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

fn average(pixels: &[[u8; 3]]) -> Rgb {
    let mut sum = [0u64; 3];
    for p in pixels {
        for (s, c) in sum.iter_mut().zip(p) {
            *s += u64::from(*c);
        }
    }
    let n = (pixels.len() as u64).max(1);
    let mean = |s: u64| ((s + n / 2) / n) as u8;
    Rgb {
        r: mean(sum[0]),
        g: mean(sum[1]),
        b: mean(sum[2]),
    }
}

/// The most common colour, with each channel rounded to 5 bits, on the tenth
/// of the thumbnail at the bound side: the left, or the right for a book
/// whose pages turn right to left. A thumbnail under ten pixels wide uses all
/// of it. The result is the mean of the pixels in that most common bucket.
fn spine(pixels: &[[u8; 3]], width: u32, right_to_left: bool) -> Rgb {
    let width = width as usize;
    let columns = match (width / 10, right_to_left) {
        (0, _) => 0..width,
        (n, false) => 0..n,
        (n, true) => width - n..width,
    };
    let bucket = |p: &[u8; 3]| -> usize {
        let q = |c: u8| (usize::from(c) + 4).min(255) >> 3;
        q(p[0]) << 10 | q(p[1]) << 5 | q(p[2])
    };
    let strip = || {
        pixels
            .chunks_exact(width)
            .flat_map(|row| &row[columns.clone()])
    };
    let mut counts = vec![0u32; 1 << 15];
    for p in strip() {
        counts[bucket(p)] += 1;
    }
    let winner =
        (0..counts.len()).fold(0, |best, i| if counts[i] > counts[best] { i } else { best });
    let members: Vec<[u8; 3]> = strip().filter(|p| bucket(p) == winner).copied().collect();
    average(&members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_within_the_box_keeping_the_aspect() {
        assert_eq!(fit(600, 900), Some((256, 384)));
        assert_eq!(fit(1000, 500), Some((256, 128)));
        assert_eq!(fit(100, 3000), Some((13, 384)));
        assert_eq!(fit(1, 1_000_000), Some((1, 384)));
        // Three times exactly is allowed; a little more is not.
        assert_eq!(fit(86, 129), Some((256, 384)));
        assert_eq!(fit(128, 128), Some((256, 256)));
        assert_eq!(fit(80, 120), None);
        assert_eq!(fit(85, 85), None);
    }

    #[test]
    fn area_averaging_keeps_a_flat_colour_and_mixes_neighbours() {
        let mut img = RgbaImage::new(4, 2);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 2 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([200, 100, 50, 255])
            };
        }
        assert_eq!(shrink(&img, 2, 1), vec![0, 0, 0, 200, 100, 50]);
        assert_eq!(shrink(&img, 1, 1), vec![100, 50, 25]);
    }
}

//! The survey request: a word count and the cover, two independent facts.

use super::archive::Archive;
use super::package::Package;
use super::{cover, encoding, is_text_document, text};
use crate::model::{BlockKind, Limits};
use crate::wire::WireSurvey;

pub fn survey(archive: &mut Archive, package: &Package, limits: &Limits) -> WireSurvey {
    // An encrypted book's chapters cannot be read, nor its cover when that
    // is encrypted too. That is an answer, not a failure: asking again would
    // give the same.
    if package.metadata.protection.is_some() {
        let open = package
            .cover
            .as_ref()
            .zip(package.encrypted.as_ref())
            .is_some_and(|(c, listed)| !listed.contains(&c.path.to_lowercase()));
        return WireSurvey {
            words: None,
            cover: open
                .then(|| cover::cover(archive, package, limits))
                .flatten(),
        };
    }
    // The cover first: its size is bounded, and a long book's text then
    // takes what remains of the run's unpacking allowance.
    let cover = cover::cover(archive, package, limits);
    WireSurvey {
        words: words(archive, package, limits),
        cover,
    }
}

/// Words in the linear reading order, or `None` when any part of it could
/// not be counted.
fn words(archive: &mut Archive, package: &Package, limits: &Limits) -> Option<u32> {
    if package.unresolved_linear || package.spine.is_empty() {
        return None;
    }
    let mut count = WordCount::default();
    for item in package.spine.iter().filter(|i| i.linear) {
        if item.media_type.starts_with("image/") {
            // A page that is a picture has no words, but it must be there.
            if !archive.contains(&item.path) {
                return None;
            }
            continue;
        }
        if !is_text_document(&item.media_type, &item.path) {
            return None;
        }
        let bytes = archive.read(&item.path, limits.max_chapter_bytes).ok()?;
        let source = encoding::decode(&bytes).ok()?;
        drop(bytes);
        let extracted = text::extract(&source, limits.max_blocks_per_chapter as usize, &|_| None);
        if extracted.truncated {
            return None;
        }
        for block in extracted
            .blocks
            .iter()
            .filter(|b| b.kind != BlockKind::Image)
        {
            count.block(&block.text());
        }
    }
    Some(u32::try_from(count.total()).unwrap_or(u32::MAX))
}

/// A word is a run of characters holding a letter or digit, ended by
/// whitespace or the end of a block. Punctuation inside a run, as in
/// `don't` or `well-known`, does not split it. Han, kana and hangul are
/// written without spaces between words, so each such character counts as
/// half a word; the halves are added up over the whole book and rounded up.
#[derive(Default)]
pub struct WordCount {
    words: u64,
    halves: u64,
}

impl WordCount {
    pub fn block(&mut self, text: &str) {
        let mut in_word = false;
        for c in text.chars() {
            if is_cjk(c) {
                self.halves += 1;
                in_word = false;
            } else if c.is_whitespace() {
                in_word = false;
            } else if c.is_alphanumeric() && !in_word {
                self.words += 1;
                in_word = true;
            }
        }
    }

    pub fn total(&self) -> u64 {
        self.words + self.halves.div_ceil(2)
    }
}

/// Han ideographs, hiragana, katakana and hangul.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x3400..=0x4DBF          // Han extension A
        | 0x4E00..=0x9FFF        // Han unified ideographs
        | 0xF900..=0xFAFF        // Han compatibility ideographs
        | 0x20000..=0x2FA1F      // Han extensions B to F and I, compatibility supplement
        | 0x30000..=0x323AF      // Han extensions G and H
        | 0x3040..=0x309F        // Hiragana
        | 0x30A1..=0x30FA        // Katakana letters
        | 0x30FC..=0x30FF        // Katakana length mark and iteration marks
        | 0x31F0..=0x31FF        // Katakana phonetic extensions
        | 0xFF66..=0xFF9F        // Half-width katakana
        | 0x1100..=0x11FF        // Hangul jamo
        | 0x3130..=0x318F        // Hangul compatibility jamo
        | 0xA960..=0xA97F        // Hangul jamo extended A
        | 0xAC00..=0xD7AF        // Hangul syllables
        | 0xD7B0..=0xD7FF        // Hangul jamo extended B
        | 0xFFA0..=0xFFDC // Half-width hangul
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(blocks: &[&str]) -> u64 {
        let mut c = WordCount::default();
        blocks.iter().for_each(|b| c.block(b));
        c.total()
    }

    #[test]
    fn counts_words_by_letters_and_digits() {
        assert_eq!(
            count(&["The keeper\u{2019}s boat \u{2014} late, as always."]),
            6
        );
        assert_eq!(count(&["well-known 1999 -- ...", "next"]), 3);
        assert_eq!(count(&["end", "start"]), 2);
    }

    #[test]
    fn han_kana_and_hangul_count_as_half_words_rounded_up_at_the_end() {
        // 4 Han and 3 hiragana, then 3 hangul: 10 halves, 5 words.
        assert_eq!(
            count(&[
                "\u{6771}\u{4EAC}\u{3067}\u{3059}\u{306D}\u{5927}\u{962A}",
                "\u{D55C}\u{AD6D}\u{C5B4}"
            ]),
            5
        );
        // 3 halves round up to 2, once, not per block; plus "Tokyo".
        assert_eq!(count(&["\u{6771}", "\u{4EAC}Tokyo\u{3002}", "\u{3067}"]), 3);
    }
}

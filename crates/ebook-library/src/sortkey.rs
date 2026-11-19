use crate::*;
use caseless::Caseless;
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences,
    options::{CollatorOptions, Strength},
    preferences::CollationNumericOrdering,
};
use icu_locale::Locale;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub(crate) struct Keys {
    pub profile: String,
    collator: CollatorBorrowed<'static>,
    letters: CollatorBorrowed<'static>,
}
impl Keys {
    pub fn new() -> Result<Self> {
        let name = ["LC_ALL", "LC_COLLATE", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        Self::for_locale(&name)
    }
    /// `name` as the environment writes it, such as `da_DK.UTF-8`.
    pub fn for_locale(name: &str) -> Result<Self> {
        let locale = name
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-")
            .parse::<Locale>()
            .unwrap_or(Locale::UNKNOWN);
        let mut prefs = CollatorPreferences::from(&locale);
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
        let mut base = CollatorOptions::default();
        base.strength = Some(Strength::Primary);
        Ok(Self {
            profile: format!("1-icu2.4-{locale}"),
            collator: Collator::try_new(prefs, Default::default())?,
            letters: Collator::try_new(prefs, base)?,
        })
    }
    fn letter_key(&self, s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        self.letters.write_sort_key_to(s, &mut out).unwrap();
        out
    }
    /// The group a name is filed in on a shelf sorted by author: a key that
    /// is equal for names under the same letter, and what to call it.
    ///
    /// The letter is the name's first character, compared as the locale
    /// compares letters, so `A`, `a` and in English `Å` are one group and in
    /// Swedish `Å` is its own. Where the locale files the first two
    /// characters differently from the first alone, as Danish does `Aa`,
    /// Czech `Ch` and Welsh `Ng`, the pair is the letter. A name that begins
    /// with anything but a letter is in the group `#`. Marks before the first
    /// character are skipped, and the name is read in composed form, so a
    /// letter written as a base and a mark is the same letter. Only the
    /// name's first [`LETTER_HEAD`] characters are read.
    pub fn letter_group(&self, name: &str) -> (Vec<u8>, String) {
        let mut chars = letter_head(name)
            .nfc()
            .skip_while(|c| is_combining_mark(*c) || *c == '\u{FEFF}');
        let Some(first) = chars.next().filter(|c| c.is_alphabetic()) else {
            return (Vec::new(), "#".into());
        };
        let one = first.to_string();
        let key = self.letter_key(&one);
        let letter = match chars.next().filter(|c| c.is_alphabetic()) {
            Some(second) => {
                let two: String = [first, second].iter().collect();
                // Two letters in a row give the first one's key and then the
                // second's. Anything else means the locale reads the pair as
                // a unit.
                if self.letter_key(&two).starts_with(&key) {
                    one
                } else {
                    two
                }
            }
            None => one,
        };
        let plain = fold(&letter);
        let shown = if self.letters.compare(&letter, &plain).is_eq() {
            &plain
        } else {
            &letter
        };
        let mut shown = shown.chars();
        let head = shown.next().into_iter().flat_map(char::to_uppercase);
        let caption = head.chain(shown.flat_map(char::to_lowercase)).collect();
        (self.letter_key(&letter), caption)
    }
    pub fn key(&self, s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        self.collator.write_sort_key_to(s, &mut out).unwrap();
        out
    }
}
/// How much of a name decides its letter: enough for marks before the first
/// letter and for two letters each written as a base and marks.
const LETTER_HEAD: usize = 8;
/// The start of a name that [`Keys::letter_group`] reads. Names with the same
/// head are in the same group.
pub(crate) fn letter_head(name: &str) -> &str {
    let end = name
        .char_indices()
        .nth(LETTER_HEAD)
        .map_or(name.len(), |(i, _)| i);
    &name[..end]
}
pub(crate) fn fold(s: &str) -> String {
    s.nfkd()
        .default_case_fold()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}
pub(crate) fn filing(b: &Book) -> &str {
    if let Some(s) = &b.sort_title {
        return s;
    }
    let articles: &[&str] = match b
        .imported
        .language
        .as_deref()
        .unwrap_or("")
        .split(['-', '_'])
        .next()
        .unwrap()
        .to_ascii_lowercase()
        .as_str()
    {
        "en" => &["a", "an", "the"],
        "fr" => &["le", "la", "les", "un", "une", "l'", "l’"],
        "de" => &["der", "die", "das", "ein", "eine"],
        "es" => &["el", "la", "los", "las", "un", "una"],
        "it" => &["il", "lo", "la", "i", "gli", "le", "un", "una", "l'", "l’"],
        "nl" => &["de", "het", "een"],
        "pt" => &["o", "a", "os", "as", "um", "uma"],
        _ => &[],
    };
    for article in articles {
        if let Some(prefix) = b.title.get(..article.len())
            && prefix.eq_ignore_ascii_case(article)
        {
            let tail = &b.title[article.len()..];
            if tail.starts_with(char::is_whitespace) || article.ends_with(['\'', '’']) {
                return tail.trim_start();
            }
        }
    }
    &b.title
}
pub(crate) fn position(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for part in s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        let mut tail = part;
        while let Some(c) = tail.chars().next() {
            let numeric = c.is_ascii_digit();
            let end = tail
                .find(|v: char| v.is_ascii_digit() != numeric)
                .unwrap_or(tail.len());
            let run = &tail[..end];
            tail = &tail[end..];
            if numeric {
                let digits = run.trim_start_matches('0');
                out.push(1);
                out.extend_from_slice(&(digits.len() as u64).to_be_bytes());
                out.extend_from_slice(digits.as_bytes());
            } else {
                out.push(2);
                out.extend(fold(run).bytes());
                out.push(0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The captions of a list of names in the order given, one per group.
    fn groups(locale: &str, names: &[&str]) -> Vec<String> {
        let keys = Keys::for_locale(locale).unwrap();
        let mut out: Vec<(Vec<u8>, String)> = Vec::new();
        for name in names {
            let group = keys.letter_group(name);
            if out.last().is_none_or(|last| last.0 != group.0) {
                out.push(group);
            }
        }
        out.into_iter().map(|(_, caption)| caption).collect()
    }

    #[test]
    fn letters_follow_the_locale() {
        let nordic = ["Adams", "apa", "Zorn", "Åberg", "ålund", "Ärlig", "Öberg"];
        assert_eq!(groups("sv_SE.UTF-8", &nordic), ["A", "Z", "Å", "Ä", "Ö"]);
        let plain = ["Adams", "Åberg", "ålund", "apa", "Ärlig", "Öberg", "Zorn"];
        assert_eq!(groups("en_US.UTF-8", &plain), ["A", "O", "Z"]);
        assert_eq!(groups("C", &plain), ["A", "O", "Z"]);
    }

    #[test]
    fn a_pair_the_locale_files_as_one_letter_is_one_group() {
        // Danish files Aa with Å, after Z.
        let danish = ["Adams", "Zorn", "Åberg", "Aagaard", "Ålund"];
        assert_eq!(groups("da_DK.UTF-8", &danish), ["A", "Z", "Å"]);
        // Czech files Ch after H.
        let czech = ["Cibulka", "Čapek", "Hrabal", "Chapek", "Ivan"];
        assert_eq!(groups("cs_CZ.UTF-8", &czech), ["C", "Č", "H", "Ch", "I"]);
        // Elsewhere the same names are under their first character.
        assert_eq!(
            groups("en_US.UTF-8", &["Aagaard", "Adams", "Chapek", "Cibulka"]),
            ["A", "C"]
        );
    }

    #[test]
    fn names_without_a_leading_letter_share_one_group() {
        let odd = [
            " Plato",
            "!!!",
            "(Anonymous)",
            "[Anonymous]",
            "2 Pac",
            "10cc",
            "Adams",
        ];
        assert_eq!(groups("en_US.UTF-8", &odd), ["#", "A"]);
        // A mark before the first letter is skipped, as the order skips it.
        assert_eq!(groups("C", &["Ibsen", "\u{301}ice", "Ives"]), ["I"]);
    }

    #[test]
    fn a_letter_written_as_base_and_mark_is_the_same_letter() {
        let names = ["Adams", "Zorn", "A\u{30A}berg", "Åberg"];
        assert_eq!(groups("sv_SE.UTF-8", &names), ["A", "Z", "Å"]);
        assert_eq!(
            groups("cs_CZ.UTF-8", &["Cibulka", "C\u{30C}apek", "Čech"]),
            ["C", "Č"]
        );
    }

    #[test]
    fn a_pair_filed_before_its_first_letter_is_still_one_letter() {
        // Welsh files Ng between G and H.
        let welsh = ["Gwyn", "Nguyen", "Huw", "Nia"];
        assert_eq!(groups("cy_GB.UTF-8", &welsh), ["G", "Ng", "H", "N"]);
        // Thai writes some vowels before the consonant they follow in order.
        let thai = ["กมล", "เกษม", "ขวัญ", "เขียว"];
        let captions = groups("th_TH.UTF-8", &thai);
        let mut unique = captions.clone();
        unique.dedup();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), captions.len(), "{captions:?}");
    }
}

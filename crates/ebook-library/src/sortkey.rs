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
    /// The letter a name is filed under, as the start of the name: one
    /// character, or two where the locale files the pair as a letter of its
    /// own, as Danish does `Aa` and Czech `Ch`. Such a pair sorts after every
    /// other name that begins with its first character.
    fn letter<'a>(&self, name: &'a str) -> &'a str {
        let name = name.trim_start();
        let mut ends = name.char_indices().map(|(i, c)| i + c.len_utf8());
        let Some(one) = ends.next() else {
            return "";
        };
        match ends.next() {
            Some(two)
                if name[one..two].chars().all(char::is_alphabetic)
                    && self
                        .letters
                        .compare(&name[..two], &format!("{}\u{FFFF}", &name[..one]))
                        .is_gt() =>
            {
                &name[..two]
            }
            _ => &name[..one],
        }
    }
    /// Whether two names are filed under the same letter in this locale: `A`,
    /// `a` and, in English, `Å`. Swedish files `Å` as a letter of its own.
    /// Names that begin with a digit are one group.
    pub fn same_letter(&self, a: &str, b: &str) -> bool {
        let digit = |s: &str| s.trim_start().starts_with(char::is_numeric);
        match (digit(a), digit(b)) {
            (true, true) => true,
            (false, false) => self.letters.compare(self.letter(a), self.letter(b)).is_eq(),
            _ => false,
        }
    }
    /// What to call the group a name is filed in: its letter in capitals,
    /// without marks where the locale files the marked letter with the plain
    /// one, and `#` for a name that begins with a digit or with no letter.
    pub fn caption(&self, name: &str) -> String {
        let letter = self.letter(name);
        if !letter.starts_with(char::is_alphabetic) {
            return "#".into();
        }
        let plain = fold(letter);
        let shown = if self.letters.compare(letter, &plain).is_eq() {
            &plain
        } else {
            letter
        };
        let mut chars = shown.chars();
        let first = chars.next().into_iter().flat_map(char::to_uppercase);
        first.chain(chars.flat_map(char::to_lowercase)).collect()
    }
    pub fn key(&self, s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        self.collator.write_sort_key_to(s, &mut out).unwrap();
        out
    }
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
        let mut out: Vec<(String, &str)> = Vec::new();
        for name in names {
            if out
                .last()
                .is_none_or(|(_, first)| !keys.same_letter(first, name))
            {
                out.push((keys.caption(name), name));
            }
        }
        out.into_iter().map(|(caption, _)| caption).collect()
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
        let odd = ["2 Pac", "3 Doors", "10cc", "100 Monkeys", " Plato"];
        assert_eq!(groups("en_US.UTF-8", &odd), ["#", "P"]);
        assert_eq!(Keys::for_locale("C").unwrap().caption("\u{301}i"), "#");
    }
}

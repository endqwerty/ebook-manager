use crate::*;
use caseless::Caseless;
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences, preferences::CollationNumericOrdering,
};
use icu_locale::Locale;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub(crate) struct Keys {
    pub profile: String,
    collator: CollatorBorrowed<'static>,
}
impl Keys {
    pub fn new() -> Result<Self> {
        let name = ["LC_ALL", "LC_COLLATE", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        let locale = name
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-")
            .parse::<Locale>()
            .unwrap_or(Locale::UNKNOWN);
        let mut prefs = CollatorPreferences::from(&locale);
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
        Ok(Self {
            profile: format!("1-icu2.4-{locale}"),
            collator: Collator::try_new(prefs, Default::default())?,
        })
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
        .split('-')
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

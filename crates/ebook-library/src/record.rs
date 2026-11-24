use crate::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ParsedMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_sorts: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<(String, Option<String>)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub right_to_left: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survey: Option<SurveyFacts>,
    /// The scheme that locks the file, absent when nothing does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<Protection>,
}

/// A protection scheme a book's file declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protection {
    Lcp,
    Adobe,
    Apple,
    Password,
    Unknown,
}

impl Protection {
    /// The stable name kept in records and the catalogue.
    pub fn name(self) -> &'static str {
        match self {
            Protection::Lcp => "lcp",
            Protection::Adobe => "adobe",
            Protection::Apple => "apple",
            Protection::Password => "password",
            Protection::Unknown => "unknown",
        }
    }
    pub fn from_name(name: &str) -> Option<Protection> {
        [
            Protection::Lcp,
            Protection::Adobe,
            Protection::Apple,
            Protection::Password,
            Protection::Unknown,
        ]
        .into_iter()
        .find(|p| p.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurveyFacts {
    pub version: u32,
    pub words: Option<u32>,
    pub cover: Option<CoverFacts>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverFacts {
    pub digest: String,
    pub average: u32,
    pub spine: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub edition_id: String,
    pub file_id: String,
    pub version_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub imported: ParsedMetadata,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_sorts: Option<Vec<Option<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Vec<(String, Option<String>)>>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub version_id: String,
    pub chapter: u32,
    pub offset: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f32>,
}
pub(super) fn fields(title: &str, authors: &[String]) -> Result<()> {
    ensure!(
        !title.trim().is_empty()
            && title.len() <= 8192
            && authors.len() <= 128
            && authors.iter().all(|a| a.len() <= 4096),
        "invalid title or author metadata"
    );
    Ok(())
}
impl ParsedMetadata {
    pub(super) fn validate(&self) -> Result<()> {
        fields(&self.title, &self.authors)?;
        ensure!(
            self.identifier
                .as_ref()
                .is_none_or(|s| !s.trim().is_empty() && s.len() <= 8192),
            "invalid identifier"
        );
        if let Some(cover) = self.survey.as_ref().and_then(|s| s.cover.as_ref()) {
            ensure!(
                cover.digest.len() == 64
                    && cover
                        .digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && cover.average <= 0xffffff
                    && cover.spine <= 0xffffff,
                "invalid thumbnail facts"
            );
        }
        Ok(())
    }
}
impl Book {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            [&self.id, &self.edition_id, &self.file_id, &self.version_id]
                .into_iter()
                .all(|s| uuid_ok(s)),
            "invalid book identity"
        );
        ensure!(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid digest"
        );
        fields(&self.title, &self.authors)?;
        self.imported.validate()
    }
}

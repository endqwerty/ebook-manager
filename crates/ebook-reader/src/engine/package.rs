//! The container file, the package document and the protection check.

use super::archive::Archive;
use super::xml::{parse_tree, Element};
use super::{clean_inline, encoding, paths, truncate_chars};
use crate::model::{Author, Identifier, Limits, Metadata};
use crate::wire::{WireError, WireErrorKind};
use std::collections::HashMap;

const MAX_TEXT: usize = 2000;
const MAX_LIST: usize = 64;
const MAX_MANIFEST: usize = 50_000;

/// Algorithms that only scramble embedded fonts. They do not protect the text.
const FONT_OBFUSCATION: [&str; 2] =
    ["http://www.idpf.org/2008/embedding", "http://ns.adobe.com/pdf/enc#RC"];

pub struct ManifestItem {
    pub path: String,
    pub media_type: String,
    pub properties: String,
}

pub struct SpineItem {
    pub path: String,
    pub media_type: String,
    pub linear: bool,
}

pub struct Package {
    pub metadata: Metadata,
    pub spine: Vec<SpineItem>,
    /// Container path of the EPUB 3 navigation document, if declared.
    pub nav: Option<String>,
    /// Container path of the NCX, if declared.
    pub ncx: Option<String>,
}

impl Package {
    /// Container path to reading-order index; the first use of a file wins.
    pub fn chapter_index(&self) -> HashMap<&str, u32> {
        let mut map = HashMap::new();
        for (i, item) in self.spine.iter().enumerate() {
            map.entry(item.path.as_str()).or_insert(i as u32);
        }
        map
    }
}

fn read_tree(archive: &mut Archive, path: &str, cap: u64) -> Result<Element, WireError> {
    let bytes = archive.read(path, cap)?;
    Ok(parse_tree(&encoding::decode(&bytes)?))
}

pub fn load(archive: &mut Archive, limits: &Limits) -> Result<Package, WireError> {
    if !archive.contains("META-INF/container.xml") {
        return Err(WireError::new(WireErrorKind::NotEpub, "no META-INF/container.xml"));
    }
    let container = read_tree(archive, "META-INF/container.xml", limits.max_package_bytes)?;
    let mut rootfiles = Vec::new();
    collect(&container, "rootfile", &mut rootfiles);
    let opf_path = rootfiles
        .iter()
        .filter(|r| {
            r.attr("media-type").map_or(true, |m| m.trim() == "application/oebps-package+xml")
        })
        .chain(rootfiles.iter())
        .filter_map(|r| r.attr("full-path"))
        .filter_map(|p| paths::resolve("", p))
        .map(|l| l.path)
        .next()
        .ok_or_else(|| WireError::invalid("the container names no package document"))?;

    let protected = is_protected(archive, limits)?;

    let doc = read_tree(archive, &opf_path, limits.max_package_bytes)?;
    let package = doc
        .find("package")
        .ok_or_else(|| WireError::invalid("the package document has no package element"))?;

    let mut metadata = read_metadata(package);
    metadata.protected = protected;

    let mut manifest: HashMap<&str, ManifestItem> = HashMap::new();
    let mut nav = None;
    let mut ncx_by_type = None;
    if let Some(m) = package.child("manifest") {
        for item in m.elements().filter(|e| e.name == "item").take(MAX_MANIFEST) {
            let (Some(id), Some(href)) = (item.attr("id"), item.attr("href")) else { continue };
            let Some(local) = paths::resolve(&opf_path, href) else { continue };
            let media_type = item.attr("media-type").unwrap_or("").trim().to_ascii_lowercase();
            let properties = item.attr("properties").unwrap_or("").to_owned();
            if nav.is_none() && properties.split_whitespace().any(|p| p == "nav") {
                nav = Some(local.path.clone());
            }
            if ncx_by_type.is_none() && media_type == "application/x-dtbncx+xml" {
                ncx_by_type = Some(local.path.clone());
            }
            manifest.entry(id).or_insert(ManifestItem { path: local.path, media_type, properties });
        }
    }

    let mut spine = Vec::new();
    let mut ncx = None;
    if let Some(s) = package.child("spine") {
        ncx = s.attr("toc").and_then(|id| manifest.get(id)).map(|i| i.path.clone());
        let refs: Vec<&Element> = s.elements().filter(|e| e.name == "itemref").collect();
        if refs.len() > limits.max_spine_items as usize {
            return Err(WireError::limit("number of chapters"));
        }
        for itemref in refs {
            let Some(item) = itemref.attr("idref").and_then(|id| manifest.get(id)) else { continue };
            spine.push(SpineItem {
                path: item.path.clone(),
                media_type: item.media_type.clone(),
                linear: itemref.attr("linear").map_or(true, |l| l.trim() != "no"),
            });
        }
    }
    Ok(Package { metadata, spine, nav, ncx: ncx.or(ncx_by_type) })
}

fn collect<'a>(root: &'a Element, name: &str, out: &mut Vec<&'a Element>) {
    for e in root.elements() {
        if e.name == name {
            out.push(e);
        }
        collect(e, name, out);
    }
}

/// OCF lists encrypted resources in META-INF/encryption.xml. Anything encrypted
/// with an algorithm other than font obfuscation means the text may be locked.
fn is_protected(archive: &mut Archive, limits: &Limits) -> Result<bool, WireError> {
    if !archive.contains("META-INF/encryption.xml") {
        return Ok(false);
    }
    let tree = read_tree(archive, "META-INF/encryption.xml", limits.max_package_bytes)?;
    let mut entries = Vec::new();
    collect(&tree, "encrypteddata", &mut entries);
    Ok(entries.iter().any(|e| {
        let algorithm = e.find("encryptionmethod").and_then(|m| m.attr_local("algorithm"));
        !algorithm.is_some_and(|a| FONT_OBFUSCATION.contains(&a.trim()))
    }))
}

fn field(e: &Element) -> String {
    let mut s = e.text();
    truncate_chars(&mut s, MAX_TEXT);
    s
}

fn read_metadata(package: &Element) -> Metadata {
    let mut out = Metadata {
        epub_version: package.attr("version").map(clean_inline).filter(|v| !v.is_empty()),
        ..Metadata::default()
    };
    let unique_id = package.attr("unique-identifier").map(str::trim);
    let Some(meta) = package.child("metadata") else { return out };

    // OEB 1.x nests the Dublin Core elements one level down.
    let mut fields: Vec<&Element> = Vec::new();
    for e in meta.elements() {
        if e.name == "dc-metadata" || e.name == "x-metadata" {
            fields.extend(e.elements());
        } else {
            fields.push(e);
        }
    }

    // EPUB 3 refinements: <meta refines="#id" property="role">aut</meta>.
    let mut refines: HashMap<&str, Vec<(&str, String)>> = HashMap::new();
    for e in fields.iter().filter(|e| e.name == "meta") {
        if let (Some(target), Some(property)) = (e.attr("refines"), e.attr("property")) {
            let target = target.trim().trim_start_matches('#');
            refines.entry(target).or_default().push((property.trim(), field(e)));
        }
    }
    let refined = |e: &Element, property: &str| -> Option<String> {
        let id = e.attr("id")?.trim();
        refines.get(id)?.iter().find(|(p, _)| *p == property).map(|(_, v)| v.clone())
    };

    let titles: Vec<&&Element> =
        fields.iter().filter(|e| e.name == "title" && !field(e).is_empty()).collect();
    out.title = titles
        .iter()
        .find(|e| refined(e, "title-type").as_deref() == Some("main"))
        .or(titles.first())
        .map(|e| field(e));

    let mut all = Vec::new();
    let mut authors = Vec::new();
    for e in fields.iter().filter(|e| e.name == "creator") {
        let name = field(e);
        if name.is_empty() {
            continue;
        }
        let role = e.attr_local("role").map(str::to_owned).or_else(|| refined(e, "role"));
        let sort_name = e
            .attr_local("file-as")
            .map(clean_inline)
            .or_else(|| refined(e, "file-as"))
            .filter(|s| !s.is_empty())
            .map(|mut s| {
                truncate_chars(&mut s, MAX_TEXT);
                s
            });
        let author = Author { name, sort_name };
        if role.as_deref().map_or(true, |r| r.trim().eq_ignore_ascii_case("aut")) {
            authors.push(author.clone());
        }
        all.push(author);
    }
    // A book that credits only editors or translators still has people to show.
    out.authors = if authors.is_empty() { all } else { authors };
    out.authors.truncate(MAX_LIST);

    for e in fields.iter().filter(|e| e.name == "identifier") {
        let raw = field(e);
        if raw.is_empty() || out.identifiers.len() >= MAX_LIST {
            continue;
        }
        let (urn_scheme, value) = split_urn(&raw);
        let scheme = e
            .attr_local("scheme")
            .map(|s| clean_inline(s).to_lowercase())
            .filter(|s| !s.is_empty())
            .or(urn_scheme);
        if value.is_empty() {
            continue;
        }
        out.identifiers.push(Identifier {
            scheme,
            value,
            is_unique_id: unique_id.is_some() && e.attr("id").map(str::trim) == unique_id,
        });
    }

    out.languages = fields
        .iter()
        .filter(|e| e.name == "language")
        .map(|e| field(e))
        .filter(|l| !l.is_empty())
        .take(MAX_LIST)
        .collect();
    out
}

/// `urn:isbn:978...` and `isbn:978...` carry their scheme in the value.
fn split_urn(raw: &str) -> (Option<String>, String) {
    let lower = raw.to_ascii_lowercase();
    let rest = lower.strip_prefix("urn:").unwrap_or(&lower);
    let had_urn = rest.len() != lower.len();
    if let Some((scheme, _)) = rest.split_once(':') {
        let known = ["isbn", "uuid", "doi", "issn", "asin"].contains(&scheme);
        let plain = !scheme.is_empty() && scheme.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if known || (had_urn && plain) {
            let offset = raw.len() - rest.len() + scheme.len() + 1;
            return (Some(scheme.to_owned()), raw[offset..].trim().to_owned());
        }
    }
    (None, raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_identifier_schemes_out_of_urns() {
        assert_eq!(split_urn("urn:isbn:9780000000001"), (Some("isbn".into()), "9780000000001".into()));
        assert_eq!(split_urn("URN:UUID:ABC-1"), (Some("uuid".into()), "ABC-1".into()));
        assert_eq!(split_urn("isbn:12345"), (Some("isbn".into()), "12345".into()));
        assert_eq!(split_urn("http://example.com/b"), (None, "http://example.com/b".into()));
        assert_eq!(split_urn("plain-id"), (None, "plain-id".into()));
    }

    #[test]
    fn reads_epub3_refinements_and_epub2_attributes() {
        let opf = r##"<package version="3.0" unique-identifier="pub-id" xmlns:dc="x" xmlns:opf="y">
          <metadata>
            <dc:title id="t2">Subtitle</dc:title>
            <dc:title id="t1">  Main
               Title </dc:title>
            <meta refines="#t1" property="title-type">main</meta>
            <dc:creator id="c1">Ann Author</dc:creator>
            <meta refines="#c1" property="role">aut</meta>
            <meta refines="#c1" property="file-as">Author, Ann</meta>
            <dc:creator opf:role="ill" opf:file-as="Drawer, Ian">Ian Drawer</dc:creator>
            <dc:creator>Bo Writer</dc:creator>
            <dc:identifier id="pub-id">urn:uuid:1234</dc:identifier>
            <dc:identifier opf:scheme="ISBN">9780000000001</dc:identifier>
            <dc:language>en-GB</dc:language>
          </metadata></package>"##;
        let tree = parse_tree(opf);
        let m = read_metadata(tree.child("package").unwrap());
        assert_eq!(m.title.as_deref(), Some("Main Title"));
        assert_eq!(m.authors.len(), 2);
        assert_eq!(m.authors[0].name, "Ann Author");
        assert_eq!(m.authors[0].sort_name.as_deref(), Some("Author, Ann"));
        assert_eq!(m.authors[1].name, "Bo Writer");
        assert_eq!(m.identifiers[0], Identifier { scheme: Some("uuid".into()), value: "1234".into(), is_unique_id: true });
        assert_eq!(m.identifiers[1], Identifier { scheme: Some("isbn".into()), value: "9780000000001".into(), is_unique_id: false });
        assert_eq!(m.languages, vec!["en-GB"]);
        assert_eq!(m.epub_version.as_deref(), Some("3.0"));
    }
}

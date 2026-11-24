//! The container file, the package document and the protection check.

use super::archive::Archive;
use super::xml::{parse_tree_checked, Element};
use super::{clean_inline, encoding, paths, truncate_chars};
use crate::model::{Author, Identifier, Limits, Metadata, Protection, SeriesMembership};
use crate::wire::{WireError, WireErrorKind};
use std::collections::{HashMap, HashSet};

const MAX_TEXT: usize = 2000;
const MAX_LIST: usize = 64;
const MAX_MANIFEST: usize = 50_000;

/// Algorithms that only scramble embedded fonts. They do not protect the text.
const FONT_OBFUSCATION: [&str; 2] = [
    "http://www.idpf.org/2008/embedding",
    "http://ns.adobe.com/pdf/enc#RC",
];

#[derive(Clone)]
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
    /// Every manifest item with an id and a path inside the book, in the
    /// order the package document lists them. A resource is named by its
    /// position here.
    pub manifest: Vec<ManifestItem>,
    /// Some linear reading-order entry names no manifest item, so it is
    /// missing from `spine`.
    pub unresolved_linear: bool,
    /// The manifest item the package declares as its cover image.
    pub cover: Option<ManifestItem>,
    /// Container path of the EPUB 3 navigation document, if declared.
    pub nav: Option<String>,
    /// Container path of the NCX, if declared.
    pub ncx: Option<String>,
    /// Container paths `META-INF/encryption.xml` lists, whatever their
    /// algorithm.
    pub encrypted: HashSet<String>,
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
    match parse_tree_checked(&encoding::decode(&bytes)?) {
        (tree, true) => Ok(tree),
        (_, false) => Err(WireError::limit("number of elements in a package document")),
    }
}

pub fn load(archive: &mut Archive, limits: &Limits) -> Result<Package, WireError> {
    if !archive.contains("META-INF/container.xml") {
        return Err(WireError::new(
            WireErrorKind::NotEpub,
            "no META-INF/container.xml",
        ));
    }
    let container = match read_tree(archive, "META-INF/container.xml", limits.max_package_bytes) {
        Err(e) if e.kind == WireErrorKind::Protected => return Ok(Package::sealed()),
        other => other?,
    };
    let mut rootfiles = Vec::new();
    collect(&container, "rootfile", &mut rootfiles);
    let opf_path = rootfiles
        .iter()
        .filter(|r| {
            r.attr("media-type")
                .is_none_or(|m| m.trim() == "application/oebps-package+xml")
        })
        .chain(rootfiles.iter())
        .filter_map(|r| r.attr("full-path"))
        .filter_map(|p| paths::resolve("", p))
        .map(|l| l.path)
        .next()
        .ok_or_else(|| WireError::invalid("the container names no package document"))?;

    let (protection, encrypted) = protection(archive, limits)?;

    let doc = match read_tree(archive, &opf_path, limits.max_package_bytes) {
        Err(e) if e.kind == WireErrorKind::Protected => return Ok(Package::sealed()),
        other => other?,
    };
    let package = doc
        .find("package")
        .ok_or_else(|| WireError::invalid("the package document has no package element"))?;

    let mut metadata = read_metadata(package);
    metadata.protection = protection;

    let mut items: Vec<ManifestItem> = Vec::new();
    // Item id to its position in `items`; the first use of an id wins.
    let mut manifest: HashMap<&str, usize> = HashMap::new();
    let mut cover_by_property = None;
    let mut nav = None;
    let mut ncx_by_type = None;
    if let Some(m) = package.child("manifest") {
        for item in m.elements().filter(|e| e.name == "item").take(MAX_MANIFEST) {
            let (Some(id), Some(href)) = (item.attr("id"), item.attr("href")) else {
                continue;
            };
            let Some(local) = paths::resolve(&opf_path, href) else {
                continue;
            };
            let media_type = item
                .attr("media-type")
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let properties = item.attr("properties").unwrap_or("").to_owned();
            if cover_by_property.is_none()
                && properties.split_whitespace().any(|p| p == "cover-image")
            {
                cover_by_property = Some(id);
            }
            if nav.is_none() && properties.split_whitespace().any(|p| p == "nav") {
                nav = Some(local.path.clone());
            }
            if ncx_by_type.is_none() && media_type == "application/x-dtbncx+xml" {
                ncx_by_type = Some(local.path.clone());
            }
            manifest.entry(id).or_insert(items.len());
            items.push(ManifestItem {
                path: local.path,
                media_type,
                properties,
            });
        }
    }

    let mut spine = Vec::new();
    let mut unresolved_linear = false;
    let mut ncx = None;
    if let Some(s) = package.child("spine") {
        ncx = s
            .attr("toc")
            .and_then(|id| manifest.get(id))
            .map(|i| items[*i].path.clone());
        let refs: Vec<&Element> = s.elements().filter(|e| e.name == "itemref").collect();
        if refs.len() > limits.max_spine_items as usize {
            return Err(WireError::limit("number of chapters"));
        }
        for itemref in refs {
            let linear = itemref.attr("linear").is_none_or(|l| l.trim() != "no");
            let Some(item) = itemref
                .attr("idref")
                .and_then(|id| manifest.get(id))
                .map(|i| &items[*i])
            else {
                unresolved_linear |= linear;
                continue;
            };
            spine.push(SpineItem {
                path: item.path.clone(),
                media_type: item.media_type.clone(),
                linear,
            });
        }
    }
    let cover_id = cover_by_property.or_else(|| epub2_cover(package));
    let cover = cover_id
        .and_then(|id| manifest.get(id))
        .map(|i| items[*i].clone())
        .or_else(|| {
            // Some books name the cover's file rather than its manifest id.
            let content = epub2_cover(package)?;
            let local = paths::resolve(&opf_path, content)?;
            items.iter().find(|i| i.path == local.path).cloned()
        });
    Ok(Package {
        metadata,
        spine,
        manifest: items,
        unresolved_linear,
        cover,
        nav,
        ncx: ncx.or(ncx_by_type),
        encrypted,
    })
}

/// The EPUB 2 convention `<meta name="cover" content="...">`.
fn epub2_cover(package: &Element) -> Option<&str> {
    package
        .child("metadata")?
        .elements()
        .find(|e| e.name == "meta" && e.attr("name").map(str::trim) == Some("cover"))?
        .attr("content")
        .map(str::trim)
        .filter(|c| !c.is_empty())
}

fn collect<'a>(root: &'a Element, name: &str, out: &mut Vec<&'a Element>) {
    for e in root.elements() {
        if e.name == name {
            out.push(e);
        }
        collect(e, name, out);
    }
}

/// OCF lists encrypted resources in `META-INF/encryption.xml` and reserves
/// `META-INF/rights.xml` for a vendor's rights information. Anything
/// encrypted with an algorithm other than font obfuscation means the text may
/// be locked. The scheme is the first that the container declares, in the
/// order Readium LCP, Adobe, Apple, a password, then unrecognised.
fn protection(
    archive: &mut Archive,
    limits: &Limits,
) -> Result<(Option<Protection>, HashSet<String>), WireError> {
    // A marker that cannot be read still says the book is locked.
    let marker = |archive: &mut Archive, path| {
        archive
            .contains(path)
            .then(|| read_tree(archive, path, limits.max_package_bytes).ok())
    };
    let mut encrypted = HashSet::new();
    let mut locked = archive.has_encrypted_entries();
    match marker(archive, "META-INF/encryption.xml") {
        None => {}
        Some(None) => locked = true,
        Some(Some(tree)) => {
            let mut entries = Vec::new();
            collect(&tree, "encrypteddata", &mut entries);
            for e in entries {
                let algorithm = e
                    .find("encryptionmethod")
                    .and_then(|m| m.attr_local("algorithm"));
                if !algorithm.is_some_and(|a| FONT_OBFUSCATION.contains(&a.trim())) {
                    locked = true;
                }
                if let Some(local) = e
                    .find("cipherreference")
                    .and_then(|r| r.attr_local("uri"))
                    .and_then(|uri| paths::resolve("", uri))
                {
                    encrypted.insert(local.path);
                }
            }
        }
    }
    let rights = marker(archive, "META-INF/rights.xml");
    locked |= rights.is_some()
        || archive.contains("META-INF/license.lcpl")
        || archive.contains("META-INF/sinf.xml");
    if !locked {
        return Ok((None, encrypted));
    }
    let adobe = rights.flatten().is_some_and(|tree| {
        tree.elements().next().is_some_and(|root| {
            root.attrs.iter().any(|(name, value)| {
                (name == "xmlns" || name.starts_with("xmlns:")) && value.contains("ns.adobe.com")
            })
        })
    });
    let scheme = if archive.contains("META-INF/license.lcpl") {
        Protection::ReadiumLcp
    } else if adobe {
        Protection::Adobe
    } else if archive.contains("META-INF/sinf.xml") {
        Protection::Apple
    } else if archive.has_encrypted_entries() {
        Protection::Password
    } else {
        Protection::Unrecognised
    };
    Ok((Some(scheme), encrypted))
}

impl Package {
    /// A book whose container file or package is itself encrypted, so
    /// nothing in it can be read.
    fn sealed() -> Package {
        Package {
            metadata: Metadata {
                protection: Some(Protection::Password),
                ..Metadata::default()
            },
            spine: Vec::new(),
            manifest: Vec::new(),
            unresolved_linear: false,
            cover: None,
            nav: None,
            ncx: None,
            encrypted: HashSet::new(),
        }
    }
}

fn field(e: &Element) -> String {
    let mut s = e.text();
    truncate_chars(&mut s, MAX_TEXT);
    s
}

fn read_metadata(package: &Element) -> Metadata {
    let mut out = Metadata {
        epub_version: package
            .attr("version")
            .map(|v| {
                let mut v = clean_inline(v);
                truncate_chars(&mut v, MAX_TEXT);
                v
            })
            .filter(|v| !v.is_empty()),
        ..Metadata::default()
    };
    let unique_id = package.attr("unique-identifier").map(str::trim);
    let Some(meta) = package.child("metadata") else {
        return out;
    };

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
            refines
                .entry(target)
                .or_default()
                .push((property.trim(), field(e)));
        }
    }
    let refined = |e: &Element, property: &str| -> Option<String> {
        let id = e.attr("id")?.trim();
        refines
            .get(id)?
            .iter()
            .find(|(p, _)| *p == property)
            .map(|(_, v)| v.clone())
    };

    let titles: Vec<&&Element> = fields
        .iter()
        .filter(|e| e.name == "title" && !field(e).is_empty())
        .collect();
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
        let role = e
            .attr_local("role")
            .map(str::to_owned)
            .or_else(|| refined(e, "role"));
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
        if role
            .as_deref()
            .is_none_or(|r| r.trim().eq_ignore_ascii_case("aut"))
        {
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
            .map(|s| {
                let mut s = clean_inline(s).to_lowercase();
                truncate_chars(&mut s, MAX_TEXT);
                s
            })
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

    out.published = published(&fields);
    out.series = series(&fields, &refined);
    out.right_to_left = package
        .child("spine")
        .and_then(|s| s.attr("page-progression-direction"))
        .is_some_and(|d| d.trim().eq_ignore_ascii_case("rtl"));
    out
}

/// EPUB 3 has one `dc:date`, the publication date. EPUB 2 may have several,
/// told apart by `opf:event`; prefer the publication one and never take a
/// date marked as another event.
fn published(fields: &[&Element]) -> Option<String> {
    let dates: Vec<(Option<String>, String)> = fields
        .iter()
        .filter(|e| e.name == "date")
        .map(|e| {
            let event = e.attr_local("event").map(|v| v.trim().to_ascii_lowercase());
            (event, field(e))
        })
        .filter(|(_, d)| !d.is_empty())
        .collect();
    let pick = |want: Option<&str>| {
        dates
            .iter()
            .find(|(event, _)| event.as_deref() == want)
            .map(|(_, d)| d.clone())
    };
    pick(Some("publication")).or_else(|| pick(None))
}

/// EPUB 3 collections refined as series, in package order, then the EPUB 2
/// `calibre:series` and `calibre:series_index` names when they add a series
/// not already listed.
fn series(
    fields: &[&Element],
    refined: &dyn Fn(&Element, &str) -> Option<String>,
) -> Vec<SeriesMembership> {
    let position = |text: Option<String>| text.filter(|p| !p.is_empty());
    let mut out: Vec<SeriesMembership> = fields
        .iter()
        .filter(|e| {
            e.name == "meta"
                && e.attr("refines").is_none()
                && e.attr("property").map(str::trim) == Some("belongs-to-collection")
                && refined(e, "collection-type").is_some_and(|t| t.eq_ignore_ascii_case("series"))
        })
        .filter_map(|e| {
            let name = field(e);
            (!name.is_empty()).then(|| SeriesMembership {
                position: position(refined(e, "group-position")),
                name,
            })
        })
        .take(MAX_LIST)
        .collect();

    let named = |name: &str| {
        fields
            .iter()
            .find(|e| e.name == "meta" && e.attr("name").map(str::trim) == Some(name))
            .and_then(|e| e.attr("content"))
            .map(|v| {
                let mut v = clean_inline(v);
                truncate_chars(&mut v, MAX_TEXT);
                v
            })
    };
    if let Some(name) = named("calibre:series").filter(|n| !n.is_empty()) {
        if out.len() < MAX_LIST && !out.iter().any(|s| s.name == name) {
            let index = position(named("calibre:series_index")).map(whole_number);
            out.push(SeriesMembership {
                name,
                position: index,
            });
        }
    }
    out
}

/// The EPUB 2 series index is written as a decimal number, so book 2 is
/// often `2.0`. Drop a fraction that is all zeros; keep any other text.
fn whole_number(text: String) -> String {
    match text.split_once('.') {
        Some((whole, fraction))
            if !whole.is_empty()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && !fraction.is_empty()
                && fraction.bytes().all(|b| b == b'0') =>
        {
            whole.to_owned()
        }
        _ => text,
    }
}

/// `urn:isbn:978...` and `isbn:978...` carry their scheme in the value.
fn split_urn(raw: &str) -> (Option<String>, String) {
    let lower = raw.to_ascii_lowercase();
    let rest = lower.strip_prefix("urn:").unwrap_or(&lower);
    let had_urn = rest.len() != lower.len();
    if let Some((scheme, _)) = rest.split_once(':') {
        let known = ["isbn", "uuid", "doi", "issn", "asin"].contains(&scheme);
        let plain = !scheme.is_empty()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
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
    use crate::engine::xml::parse_tree;

    #[test]
    fn caps_every_metadata_text_value() {
        let long = "v".repeat(MAX_TEXT + 50);
        let opf = format!(
            r#"<package version="{long}"><metadata><identifier scheme="{long}">x</identifier></metadata></package>"#
        );
        let tree = parse_tree(&opf);
        let m = read_metadata(tree.child("package").unwrap());
        assert_eq!(m.epub_version.unwrap().chars().count(), MAX_TEXT);
        assert_eq!(
            m.identifiers[0].scheme.as_ref().unwrap().chars().count(),
            MAX_TEXT
        );
    }

    #[test]
    fn splits_identifier_schemes_out_of_urns() {
        assert_eq!(
            split_urn("urn:isbn:9780000000001"),
            (Some("isbn".into()), "9780000000001".into())
        );
        assert_eq!(
            split_urn("URN:UUID:ABC-1"),
            (Some("uuid".into()), "ABC-1".into())
        );
        assert_eq!(
            split_urn("isbn:12345"),
            (Some("isbn".into()), "12345".into())
        );
        assert_eq!(
            split_urn("http://example.com/b"),
            (None, "http://example.com/b".into())
        );
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
        assert_eq!(
            m.identifiers[0],
            Identifier {
                scheme: Some("uuid".into()),
                value: "1234".into(),
                is_unique_id: true
            }
        );
        assert_eq!(
            m.identifiers[1],
            Identifier {
                scheme: Some("isbn".into()),
                value: "9780000000001".into(),
                is_unique_id: false
            }
        );
        assert_eq!(m.languages, vec!["en-GB"]);
        assert_eq!(m.epub_version.as_deref(), Some("3.0"));
    }
}

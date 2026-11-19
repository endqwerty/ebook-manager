//! Table of contents from the EPUB 3 navigation document or the EPUB 2 NCX.

use super::archive::Archive;
use super::package::Package;
use super::xml::{parse_tree, Element};
use super::{encoding, paths, truncate_chars};
use crate::model::{Limits, Target, TocEntry};
use std::collections::HashMap;

const MAX_DEPTH: u32 = 32;

struct Builder<'a> {
    base: &'a str,
    chapters: &'a HashMap<&'a str, u32>,
    max: usize,
    out: Vec<TocEntry>,
}

impl Builder<'_> {
    fn push(&mut self, title: String, depth: u32, href: Option<&str>) {
        if self.out.len() >= self.max {
            return;
        }
        let mut title = title;
        truncate_chars(&mut title, 500);
        let target = href.and_then(|h| paths::resolve(self.base, h)).and_then(|local| {
            let chapter = *self.chapters.get(local.path.as_str())?;
            let anchor = local.fragment.map(|mut f| {
                truncate_chars(&mut f, 256);
                f
            });
            Some(Target { chapter, anchor })
        });
        if title.is_empty() && target.is_none() {
            return;
        }
        self.out.push(TocEntry { title, depth, target });
    }
}

/// A broken or missing contents file gives an empty list, never a failed book:
/// the reading order alone is enough to read.
pub fn load(archive: &mut Archive, package: &Package, limits: &Limits) -> Vec<TocEntry> {
    let chapters = package.chapter_index();
    let max = limits.max_toc_entries as usize;
    let mut read = |path: &str| -> Option<Element> {
        let bytes = archive.read(path, limits.max_package_bytes).ok()?;
        Some(parse_tree(&encoding::decode(&bytes).ok()?))
    };
    if let Some(path) = &package.nav {
        if let Some(tree) = read(path) {
            let mut b = Builder { base: path, chapters: &chapters, max, out: Vec::new() };
            if let Some(list) = find_toc_nav(&tree).and_then(|nav| nav.find("ol")) {
                nav_list(list, 0, &mut b);
            }
            if !b.out.is_empty() {
                return b.out;
            }
        }
    }
    if let Some(path) = &package.ncx {
        if let Some(tree) = read(path) {
            let mut b = Builder { base: path, chapters: &chapters, max, out: Vec::new() };
            if let Some(map) = tree.find("navmap") {
                ncx_points(map, 0, &mut b);
            }
            return b.out;
        }
    }
    Vec::new()
}

fn find_toc_nav(root: &Element) -> Option<&Element> {
    let mut navs = Vec::new();
    collect_navs(root, &mut navs);
    navs.iter()
        .find(|n| n.attr_local("type").is_some_and(|t| t.split_whitespace().any(|w| w == "toc")))
        .or(navs.first())
        .copied()
}

fn collect_navs<'a>(root: &'a Element, out: &mut Vec<&'a Element>) {
    for e in root.elements() {
        if e.name == "nav" {
            out.push(e);
        } else {
            collect_navs(e, out);
        }
    }
}

fn nav_list(list: &Element, depth: u32, b: &mut Builder<'_>) {
    for li in list.elements().filter(|e| e.name == "li") {
        let label = li.elements().find(|e| e.name == "a" || e.name == "span");
        match label {
            Some(l) => b.push(l.text(), depth, l.attr("href")),
            None => {
                // Text directly in the item, before any nested list.
                let mut text = String::new();
                for node in &li.children {
                    if let super::xml::Node::Text(t) = node {
                        text.push_str(t);
                    }
                }
                b.push(super::clean_inline(&text), depth, None);
            }
        }
        if depth < MAX_DEPTH {
            for nested in li.elements().filter(|e| e.name == "ol") {
                nav_list(nested, depth + 1, b);
            }
        }
    }
}

fn ncx_points(parent: &Element, depth: u32, b: &mut Builder<'_>) {
    for point in parent.elements().filter(|e| e.name == "navpoint") {
        let title = point
            .child("navlabel")
            .and_then(|l| l.child("text"))
            .map(Element::text)
            .unwrap_or_default();
        let href = point.child("content").and_then(|c| c.attr("src"));
        b.push(title, depth, href);
        if depth < MAX_DEPTH {
            ncx_points(point, depth + 1, b);
        }
    }
}

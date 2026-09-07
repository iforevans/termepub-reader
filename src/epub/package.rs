//! EPUB package document parsing.
//!
//! Handles container.xml location, OPF metadata/manifest/spine, and both NCX
//! (EPUB 2) and nav document (EPUB 3) table-of-contents extraction.

use crate::error::Error;
use crate::TocEntry;

use super::archive::{normalize_epub_path, Archive};

/// A manifest item from the OPF.
#[derive(Debug, Clone)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
    pub properties: Vec<String>,
}

/// Parsed package document data.
#[derive(Debug)]
pub struct Package {
    pub title: String,
    pub author: String,
    pub manifest: Vec<ManifestItem>,
    pub spine: Vec<String>,
}

/// Returns the local part of a qualified XML name (strips namespace prefix).
fn local_name(qname: &[u8]) -> Vec<u8> {
    qname
        .splitn(2, |&b| b == b':')
        .last()
        .unwrap_or(qname)
        .to_vec()
}

/// Locates the OPF path via `META-INF/container.xml`.
pub fn find_opf_path(archive: &mut Archive) -> Result<String, Error> {
    let xml = archive
        .read_text("META-INF/container.xml")
        .map_err(|_| Error::InvalidEpub("missing or invalid META-INF/container.xml".into()))?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e))
            | Ok(quick_xml::events::Event::Empty(ref e)) => {
                let name = local_name(e.name().as_ref());
                if name == b"rootfile" {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|err| {
                            Error::InvalidEpub(format!("malformed container.xml: {err}"))
                        })?;
                        let key = local_name(attr.key.as_ref());
                        if key == b"full-path" {
                            let path = std::str::from_utf8(attr.value.as_ref()).map_err(|_| {
                                Error::InvalidEpub("non-UTF-8 path in container.xml".into())
                            })?;
                            return Ok(normalize_epub_path(path.trim()));
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => {
                return Err(Error::InvalidEpub(
                    "no rootfile found in container.xml".into(),
                ));
            }
            Err(e) => {
                return Err(Error::InvalidEpub(format!(
                    "failed to parse container.xml: {e}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }
}

/// Parses the OPF package document and returns metadata, manifest, and spine.
pub fn parse_package(archive: &mut Archive, opf_path: &str) -> Result<Package, Error> {
    let xml = archive
        .read_text(opf_path)
        .map_err(|_| Error::InvalidEpub(format!("cannot read package document: {opf_path}")))?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    let mut title = String::new();
    let mut author = String::new();
    let mut manifest = Vec::new();
    let mut spine = Vec::new();

    let mut section: Option<&'static str> = None;
    let mut current_meta_tag: Option<Vec<u8>> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());

                match name.as_slice() {
                    b"metadata" => section = Some("metadata"),
                    b"manifest" => section = Some("manifest"),
                    b"spine" => section = Some("spine"),
                    _ => match section {
                        Some("metadata") => {
                            current_meta_tag = Some(name);
                        }
                        Some("manifest") => {
                            if name == b"item" {
                                if let Some(item) = parse_manifest_item(e) {
                                    manifest.push(item);
                                }
                            }
                        }
                        Some("spine") if name == b"itemref" => {
                            for attr in e.attributes() {
                                let attr = attr.map_err(|err| {
                                    Error::InvalidEpub(format!("malformed OPF: {err}"))
                                })?;
                                let key = local_name(attr.key.as_ref());
                                if key == b"idref" {
                                    let idref =
                                        std::str::from_utf8(attr.value.as_ref()).map_err(|_| {
                                            Error::InvalidEpub("non-UTF-8 idref in OPF".into())
                                        })?;
                                    spine.push(idref.trim().to_string());
                                }
                            }
                        }
                        _ => {}
                    },
                }
            }
            Ok(quick_xml::events::Event::Empty(ref e)) => {
                let name = local_name(e.name().as_ref());
                // Empty/self-closing elements: handle like Start but no child
                // tracking.  A self-closing metadata tag (e.g. <dc:creator/>)
                // still sets the current tag so any following text is
                // attributed to it; a self-closing item/itemref is parsed here.
                match section {
                    Some("metadata") => {
                        current_meta_tag = Some(name);
                    }
                    Some("manifest") if name == b"item" => {
                        if let Some(item) = parse_manifest_item(e) {
                            manifest.push(item);
                        }
                    }
                    Some("spine") if name == b"itemref" => {
                        for attr in e.attributes() {
                            let attr = attr.map_err(|err| {
                                Error::InvalidEpub(format!("malformed OPF: {err}"))
                            })?;
                            let key = local_name(attr.key.as_ref());
                            if key == b"idref" {
                                let idref =
                                    std::str::from_utf8(attr.value.as_ref()).map_err(|_| {
                                        Error::InvalidEpub("non-UTF-8 idref in OPF".into())
                                    })?;
                                spine.push(idref.trim().to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                let name = local_name(e.name().as_ref());
                match section {
                    Some("metadata") if name == b"metadata" => section = None,
                    Some("manifest") if name == b"manifest" => section = None,
                    Some("spine") if name == b"spine" => section = None,
                    Some("metadata") => {
                        if let Some(ref tag) = current_meta_tag {
                            if tag == &name {
                                current_meta_tag = None;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Text(ref e)) => {
                if section == Some("metadata") {
                    let text = e.unescape().map_err(|err| {
                        Error::InvalidEpub(format!("failed to unescape OPF text: {err}"))
                    })?;
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if let Some(ref tag) = current_meta_tag {
                            if tag == b"title" && title.is_empty() {
                                title = trimmed.to_string();
                            } else if tag == b"creator" && author.is_empty() {
                                author = trimmed.to_string();
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                return Err(Error::InvalidEpub(format!("failed to parse OPF: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    if spine.is_empty() {
        return Err(Error::InvalidEpub(
            "EPUB has no spine items (empty spine)".into(),
        ));
    }

    // Resolve manifest hrefs relative to the OPF document's directory.
    let opf_dir = opf_path.rfind('/').map(|i| &opf_path[..i]);
    for item in &mut manifest {
        if !item.href.starts_with('/') {
            if let Some(dir) = opf_dir {
                item.href = format!("{}/{}", dir, item.href);
            }
        }
    }

    Ok(Package {
        title,
        author,
        manifest,
        spine,
    })
}

/// Parses a manifest `<item>` element.
fn parse_manifest_item(e: &quick_xml::events::BytesStart) -> Option<ManifestItem> {
    let mut id = String::new();
    let mut href = String::new();
    let mut media_type = String::new();
    let mut properties = Vec::new();

    for attr in e.attributes() {
        let attr = attr.ok()?;
        let key = local_name(attr.key.as_ref());
        let value = std::str::from_utf8(attr.value.as_ref()).ok()?;

        match key.as_slice() {
            b"id" => id = value.trim().to_string(),
            b"href" => href = value.trim().to_string(),
            b"media-type" => media_type = value.trim().to_string(),
            b"properties" => {
                properties = value
                    .split_whitespace()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }

    if id.is_empty() || href.is_empty() {
        return None;
    }

    Some(ManifestItem {
        id,
        href,
        media_type,
        properties,
    })
}

/// Finds the TOC document href from the manifest.
pub fn find_toc_document(manifest: &[ManifestItem]) -> Option<String> {
    manifest
        .iter()
        .find(|item| item.properties.iter().any(|p| p == "nav"))
        .map(|item| item.href.clone())
        .or_else(|| {
            manifest
                .iter()
                .find(|item| item.media_type == "application/x-dtbncx+xml")
                .map(|item| item.href.clone())
        })
}

/// Returns the 0-based occurrence index of the first `<nav>` element marked
/// `epub:type="toc"` (or `type="toc"`), or `None` if no nav carries that type.
///
/// A nav document may contain several `<nav>` elements (landmarks, guide,
/// etc.); only the one typed as the TOC should be treated as the table of
/// contents.
fn find_toc_nav_index(html: &str) -> Option<usize> {
    let mut reader = quick_xml::Reader::from_str(html);
    let mut buf = Vec::new();
    let mut nav_counter = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e))
            | Ok(quick_xml::events::Event::Empty(ref e)) => {
                let name = local_name(e.name().as_ref());
                if name == b"nav" {
                    let mut is_toc = false;
                    for attr in e.attributes() {
                        let attr = match attr {
                            Ok(a) => a,
                            Err(_) => continue,
                        };
                        let key = local_name(attr.key.as_ref());
                        if key == b"type" {
                            if let Ok(v) = std::str::from_utf8(attr.value.as_ref()) {
                                if v.trim() == "toc" {
                                    is_toc = true;
                                }
                            }
                        }
                    }
                    if is_toc {
                        return Some(nav_counter);
                    }
                    nav_counter += 1;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    None
}

/// Parses an EPUB 3 nav document and extracts TOC entries.
///
/// The TOC is the `<nav>` element marked `epub:type="toc"` (or `type="toc"`).
/// If no nav carries that type, the first `<nav>` is used as a fallback, since
/// some books omit the type attribute.
pub fn parse_nav_toc(
    archive: &mut Archive,
    nav_href: &str,
    _spine: &[String],
) -> Result<Vec<TocEntry>, Error> {
    let html = archive
        .read_text(nav_href)
        .map_err(|_| Error::InvalidEpub(format!("cannot read nav document: {nav_href}")))?;

    // Which nav occurrence (0-based among all <nav> elements) is the TOC.
    let target_nav = find_toc_nav_index(&html).unwrap_or(0);

    let mut entries = Vec::new();
    let mut reader = quick_xml::Reader::from_str(&html);
    let mut buf = Vec::new();

    // Total <nav> elements opened so far, and the occurrence indices of the
    // navs currently open (a stack, so nested/sibling navs track correctly).
    let mut nav_counter = 0usize;
    let mut nav_stack: Vec<usize> = Vec::new();
    let mut in_toc_nav = false;
    let mut in_link = false;
    let mut link_href = String::new();
    let mut link_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());

                if name == b"nav" {
                    let idx = nav_counter;
                    nav_counter += 1;
                    nav_stack.push(idx);
                    if idx == target_nav {
                        in_toc_nav = true;
                    }
                }

                if in_toc_nav && name == b"a" {
                    in_link = true;
                    link_href.clear();
                    link_text.clear();
                    for attr in e.attributes() {
                        let attr = match attr {
                            Ok(a) => a,
                            Err(_) => continue,
                        };
                        let key = local_name(attr.key.as_ref());
                        if key == b"href" {
                            if let Ok(v) = std::str::from_utf8(attr.value.as_ref()) {
                                link_href = v.trim().to_string();
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                let name = local_name(e.name().as_ref());

                if in_toc_nav && in_link && name == b"a" {
                    entries.push(TocEntry {
                        title: if link_text.is_empty() {
                            link_href.clone()
                        } else {
                            link_text.clone()
                        },
                        href: link_href.clone(),
                        spine_index: None,
                    });
                    in_link = false;
                    link_href.clear();
                    link_text.clear();
                }

                if name == b"nav" {
                    if let Some(&top) = nav_stack.last() {
                        if top == target_nav {
                            in_toc_nav = false;
                        }
                        nav_stack.pop();
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(ref e)) => {
                if in_toc_nav && in_link {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if !text.is_empty() {
                        link_text = text;
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                return Err(Error::InvalidEpub(format!(
                    "failed to parse nav document: {e}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(entries)
}

/// Parses an EPUB 2 NCX document and extracts TOC entries.
pub fn parse_ncx_toc(archive: &mut Archive, ncx_href: &str) -> Result<Vec<TocEntry>, Error> {
    let xml = archive
        .read_text(ncx_href)
        .map_err(|_| Error::InvalidEpub(format!("cannot read NCX document: {ncx_href}")))?;

    let mut entries = Vec::new();
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    let mut in_nav_point = false;
    let mut in_nav_label = false;
    let mut point_title = String::new();
    let mut point_href = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e)) => {
                let name = local_name(e.name().as_ref());

                if name == b"navPoint" {
                    in_nav_point = true;
                    point_title.clear();
                    point_href.clear();
                } else if in_nav_point && name == b"navLabel" {
                    in_nav_label = true;
                } else if in_nav_point && name == b"content" {
                    for attr in e.attributes() {
                        let attr = match attr {
                            Ok(a) => a,
                            Err(_) => continue,
                        };
                        let key = local_name(attr.key.as_ref());
                        if key == b"src" {
                            if let Ok(v) = std::str::from_utf8(attr.value.as_ref()) {
                                point_href = v.trim().to_string();
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Empty(ref e)) => {
                let name = local_name(e.name().as_ref());
                if in_nav_point && name == b"content" {
                    for attr in e.attributes() {
                        let attr = match attr {
                            Ok(a) => a,
                            Err(_) => continue,
                        };
                        let key = local_name(attr.key.as_ref());
                        if key == b"src" {
                            if let Ok(v) = std::str::from_utf8(attr.value.as_ref()) {
                                point_href = v.trim().to_string();
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::End(ref e)) => {
                let name = local_name(e.name().as_ref());

                if in_nav_point && name == b"navPoint" {
                    entries.push(TocEntry {
                        title: if point_title.is_empty() {
                            point_href.clone()
                        } else {
                            point_title.clone()
                        },
                        href: point_href.clone(),
                        spine_index: None,
                    });
                    in_nav_point = false;
                }

                if in_nav_label && name == b"navLabel" {
                    in_nav_label = false;
                }
            }
            Ok(quick_xml::events::Event::Text(ref e)) => {
                if in_nav_label {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if !text.is_empty() {
                        point_title = text;
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                return Err(Error::InvalidEpub(format!(
                    "failed to parse NCX document: {e}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(entries)
}

/// Resolves TOC entry spine_index values by matching hrefs against the manifest
/// and spine order.
pub fn resolve_toc_spine_indices(
    toc: &mut Vec<TocEntry>,
    manifest: &[ManifestItem],
    spine: &[String],
    toc_base_dir: Option<&str>,
) {
    let href_to_spine: std::collections::HashMap<String, usize> = spine
        .iter()
        .enumerate()
        .filter_map(|(idx, idref)| {
            manifest
                .iter()
                .find(|item| item.id == *idref)
                .map(|item| (normalize_epub_path(&item.href), idx))
        })
        .collect();

    for entry in toc {
        let raw = entry.href.split('#').next().unwrap_or(&entry.href);
        let resolved = if let Some(dir) = toc_base_dir {
            normalize_epub_path(&format!("{}/{}", dir, raw))
        } else {
            normalize_epub_path(raw)
        };
        // Leave unresolved entries as `None` so the UI can skip them
        // instead of silently jumping to chapter 0.
        entry.spine_index = href_to_spine.get(&resolved).copied();
    }
}

/// Generates generic TOC entries when no NCX or nav document is available.
pub fn generate_fallback_toc(spine: &[String], manifest: &[ManifestItem]) -> Vec<TocEntry> {
    spine
        .iter()
        .enumerate()
        .filter_map(|(idx, idref)| {
            manifest
                .iter()
                .find(|item| item.id == *idref)
                .map(|item| TocEntry {
                    title: format!("Chapter {}", idx + 1),
                    href: item.href.clone(),
                    spine_index: Some(idx),
                })
        })
        .collect()
}

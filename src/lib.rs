//! termepub — Terminal EPUB reader.
//!
//! Public API for integration tests and future phases.

pub mod cli;
pub mod dictionary;
pub mod epub;
pub mod error;
pub mod layout;
pub mod state;
pub mod ui;

use std::path::Path;

use crate::epub::archive::Archive;
use crate::epub::extract;
use crate::epub::package;

/// Extract styled segments from HTML content.
///
/// Parses HTML using a tag frame stack, applies inline and semantic styles
/// when `use_css` is true, and returns merged `StyledSegment` results with
/// paragraph break preservation.
pub fn extract_html(html: &str, use_css: bool) -> Vec<StyledSegment> {
    extract::extract(html, use_css)
}

/// Paginate styled segments into rendered pages.
///
/// Returns pages, lines, and segments.  Uses grapheme-safe wrapping with
/// greedy word-aware line breaks, optional justification, and heading
/// deduplication.
pub fn paginate(
    segments: &[StyledSegment],
    width: usize,
    height: usize,
    show_header: bool,
    justify: bool,
) -> Vec<Vec<Vec<StyledSegment>>> {
    layout::paginate::paginate(segments, width, height, show_header, justify)
}

/// Search rendered pages for a phrase.
///
/// Returns the index of the first page containing the match, including
/// matches that span line and page boundaries.
pub fn search_pages(pages: &[Vec<Vec<StyledSegment>>], query: &str) -> Option<usize> {
    layout::paginate::search_pages(pages, query)
}

/// A loaded EPUB book with metadata, TOC, and chapter content.
#[derive(Debug)]
pub struct EpubBook {
    /// The bounded archive for reading members.
    #[allow(dead_code)]
    archive: Archive,
    /// Book title from OPF metadata.
    title: String,
    /// Book author from OPF metadata.
    author: String,
    /// Table of contents entries.
    toc: Vec<TocEntry>,
    /// Per-chapter styled segments.
    chapters: Vec<Vec<StyledSegment>>,
}

impl EpubBook {
    /// Opens and validates an EPUB file.
    ///
    /// Verifies:
    /// - ZIP member count within limit.
    /// - No encrypted spine text resources (font-only encryption is allowed).
    /// - Package document (OPF) is present and parseable.
    /// - Spine contains at least one item.
    /// - TOC (nav or NCX) is loaded.
    pub fn open(path: &Path, use_css: bool) -> Result<Self, crate::error::Error> {
        let mut archive = Archive::open(path)?;

        // Check for encrypted spine resources.
        let encrypted_uris = archive.parse_encryption()?;
        if !encrypted_uris.is_empty() {
            let text_extensions = ["xhtml", "xml", "html", "opf", "ncx", "css", "svg", "svgz"];
            for uri in &encrypted_uris {
                let normalized = crate::epub::archive::normalize_epub_path(uri);
                let lower = normalized.to_lowercase();
                let is_font = lower.starts_with("font")
                    || lower.ends_with(".ttf")
                    || lower.ends_with(".otf")
                    || lower.ends_with(".woff")
                    || lower.ends_with(".woff2");
                if !is_font
                    && text_extensions
                        .iter()
                        .any(|ext| lower.ends_with(&format!(".{ext}")))
                {
                    return Err(crate::error::Error::UnsupportedContent(format!(
                        "encrypted text resource: {normalized}"
                    )));
                }
            }
        }

        // Parse package document.
        let opf_path = package::find_opf_path(&mut archive)?;
        let pkg = package::parse_package(&mut archive, &opf_path)?;

        // Load TOC.
        let toc = if let Some(toc_href) = package::find_toc_document(&pkg.manifest) {
            // TOC hrefs are resolved relative to the TOC document's own
            // directory, not the OPF's.  `toc_href` was already resolved
            // against the OPF directory by parse_package, so its parent
            // directory is the correct base for resolving entry hrefs.
            let toc_base = toc_href.rfind('/').map(|i| &toc_href[..i]);
            let nav_item = pkg.manifest.iter().find(|item| item.href == toc_href);

            if nav_item
                .map(|i| i.properties.iter().any(|p| p == "nav"))
                .unwrap_or(false)
            {
                // EPUB 3 nav document
                let mut entries = package::parse_nav_toc(&mut archive, &toc_href, &pkg.spine)?;
                package::resolve_toc_spine_indices(
                    &mut entries,
                    &pkg.manifest,
                    &pkg.spine,
                    toc_base,
                );
                entries
            } else {
                // EPUB 2 NCX
                let mut entries = package::parse_ncx_toc(&mut archive, &toc_href)?;
                package::resolve_toc_spine_indices(
                    &mut entries,
                    &pkg.manifest,
                    &pkg.spine,
                    toc_base,
                );
                entries
            }
        } else {
            // No TOC document found — generate fallback
            package::generate_fallback_toc(&pkg.spine, &pkg.manifest)
        };

        // Load spine chapters.
        let mut chapters = Vec::with_capacity(pkg.spine.len());

        for idref in &pkg.spine {
            let manifest_item = pkg.manifest.iter().find(|item| &item.id == idref);
            match manifest_item {
                Some(item) => {
                    let html = match archive.read_text(&item.href) {
                        Ok(content) => content,
                        Err(_) => {
                            // Missing chapter member
                            chapters.push(vec![StyledSegment {
                                text: String::from("[Missing chapter content]"),
                                style: crate::TextStyle::default(),
                                is_heading: false,
                            }]);
                            continue;
                        }
                    };

                    let segments = extract::extract(&html, use_css);

                    if segments.is_empty() || segments.iter().all(|s| s.text.trim().is_empty()) {
                        chapters.push(vec![StyledSegment {
                            text: String::from("[This chapter contains no readable text.]"),
                            style: crate::TextStyle::default(),
                            is_heading: false,
                        }]);
                    } else {
                        chapters.push(segments);
                    }
                }
                None => {
                    // Unknown itemref — ignore as the Python app does
                    // But still produce a placeholder for the chapter slot
                    chapters.push(vec![StyledSegment {
                        text: String::from("[Missing chapter content]"),
                        style: crate::TextStyle::default(),
                        is_heading: false,
                    }]);
                }
            }
        }

        // Reject books with no readable spine chapters.
        if chapters.is_empty() {
            return Err(crate::error::Error::InvalidEpub(
                "EPUB has no readable spine chapters".into(),
            ));
        }

        // Resolve TOC spine indices using manifest
        // (Already done above, but ensure consistency)

        Ok(Self {
            archive,
            title: pkg.title,
            author: pkg.author,
            toc,
            chapters,
        })
    }

    /// Returns the book title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the book author.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// Returns the table of contents.
    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    /// Returns the styled segments for each chapter in the spine.
    pub fn chapters(&self) -> &[Vec<StyledSegment>] {
        &self.chapters
    }

    /// Returns the number of chapters in the spine.
    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }
}

/// Re-export StateStore and BookState from the state module.
pub use crate::state::BookState;
pub use crate::state::StateStore;

/// Looks up a word in the embedded dictionary.
///
/// Tries exact lowercase match, then punctuation-stripped match.
/// Falls back to deterministic fuzzy suggestions.
pub fn lookup_word(word: &str) -> String {
    dictionary::lookup_word(word)
}

/// Starts loading the dictionary in the background (idempotent).
///
/// Safe to call at startup; lookups before the load finishes report
/// "still loading" instead of blocking.
pub fn preload_dictionary() {
    dictionary::preload_dictionary();
}

// --- Domain models ---

/// A table-of-contents entry.
#[derive(Debug, Clone)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    /// Resolved spine position, if the href could be matched to a spine
    /// item.  `None` means the entry could not be resolved and the UI
    /// should skip it rather than jumping to an arbitrary chapter.
    pub spine_index: Option<usize>,
}

/// A segment of text with associated style information.
#[derive(Debug, Clone)]
pub struct StyledSegment {
    pub text: String,
    pub style: TextStyle,
    pub is_heading: bool,
}

impl StyledSegment {
    /// Returns the terminal cell width of this segment's text.
    pub fn text_width(&self) -> usize {
        unicode_width::UnicodeWidthStr::width(self.text.as_str())
    }
}

/// Render style for a text segment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyle {
    pub bold: bool,
    pub underline: bool,
    pub foreground: Option<[u8; 3]>,
    pub italic: bool,
    pub strike: bool,
}

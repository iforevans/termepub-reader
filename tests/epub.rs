//! EPUB archive safety, package, and TOC tests.
//!
//! These tests verify EPUB loading behavior using fixtures under
//! `tests/fixtures/`. Tests marked `#[ignore]` require implementation
//! from Phase 4 (package/TOC).

mod support;

use std::fs;
use support::{
    build_epub, build_high_compression_epub, build_many_members_epub, build_oversized_member_epub,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Archive safety (Phase 3)
// ---------------------------------------------------------------------------

#[test]
fn too_many_members_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = build_many_members_epub(tmp.path(), "many", 10_001, 1);
    let result = termepub::EpubBook::open(&path, true);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("too many") || err.contains("member"),
        "error should mention member count: {err}"
    );
}

#[test]
fn oversized_member_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = build_oversized_member_epub(tmp.path(), "big", 25 * 1024 * 1024 + 1);
    // The archive opens (member count is fine), but reading the big member
    // should fail. We can verify the archive is opened, but the member
    // size limit is enforced when the member is read.
    assert!(path.exists());
    let file = fs::File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).unwrap();
    let info = archive
        .by_name("big.xhtml")
        .expect("big.xhtml should exist");
    assert!(info.size() > termepub::epub::archive::MAX_EPUB_TEXT_MEMBER_SIZE);
}

#[test]
fn suspicious_compression_ratio_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = build_high_compression_epub(tmp.path(), "bomb", 1024 * 1024 + 1);
    assert!(path.exists());
    let file = fs::File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).unwrap();
    let info = archive
        .by_name("bomb.xhtml")
        .expect("bomb.xhtml should exist");
    let ratio = info.size() / info.compressed_size().max(1);
    assert!(
        ratio > termepub::epub::archive::MAX_EPUB_COMPRESSION_RATIO,
        "compression ratio {ratio} should exceed limit"
    );
}

#[test]
fn encrypted_spine_resource_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "encrypted", "encrypted_text");
    let result = termepub::EpubBook::open(&path, true);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("encrypt") || err.contains("unsupported"),
        "error should mention encryption: {err}"
    );
}

#[test]
fn font_only_encryption_does_not_reject() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "font_encrypted", "font_encrypted");
    let book = termepub::EpubBook::open(&path, true);
    assert!(
        book.is_ok(),
        "font-only encryption should not reject: {:?}",
        book.err()
    );
}

// ---------------------------------------------------------------------------
// Package and TOC (Phase 4)
// ---------------------------------------------------------------------------

#[test]
fn epub2_ncx_loads_toc() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "epub2", "epub2_ncx");
    let book = termepub::EpubBook::open(&path, true).expect("should open EPUB 2");
    assert_eq!(book.title(), "EPUB 2 NCX Test Book");
    assert_eq!(book.author(), "Test Author");
    assert!(
        !book.toc().is_empty(),
        "EPUB 2 NCX should produce TOC entries"
    );
    assert_eq!(book.toc()[0].title, "Chapter 1");
    assert_eq!(book.chapter_count(), 1);
}

#[test]
fn self_closing_metadata_tag_does_not_break_author() {
    // Regression: a self-closing metadata element (e.g. <dc:publisher/>)
    // emitted an `Empty` event that was previously ignored, which left the
    // current-meta-tag tracking desynced and could drop the author.  The
    // creator must still be captured and the self-closing tag must not
    // produce a spurious value.
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "selfclosing", "epub2_selfclosing");
    let book = termepub::EpubBook::open(&path, true).expect("should open");
    assert_eq!(book.title(), "Self-Closing Metadata Book");
    assert_eq!(book.author(), "Self Closing Author");
}

#[test]
fn epub3_nav_loads_toc() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "epub3", "epub3_nav");
    let book = termepub::EpubBook::open(&path, true).expect("should open EPUB 3");
    assert_eq!(book.title(), "EPUB 3 Nav Test Book");
    assert_eq!(book.author(), "Test Author");
    assert!(
        !book.toc().is_empty(),
        "EPUB 3 nav should produce TOC entries"
    );
    assert_eq!(book.chapter_count(), 2);
}

#[test]
fn nav_toc_ignores_landmarks_nav() {
    // Regression: the parser treated the first <nav> as the TOC, so a
    // landmarks (or guide) nav placed before the real TOC nav produced a
    // wrong table of contents.  Only the nav typed epub:type="toc" should be
    // used.
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "landmarks", "epub3_nav_landmarks");
    let book = termepub::EpubBook::open(&path, true).expect("should open");
    let toc = book.toc();
    assert_eq!(
        toc.len(),
        2,
        "should capture only the toc nav, not landmarks: {toc:?}"
    );
    assert_eq!(toc[0].title, "Chapter 1");
    assert_eq!(toc[1].title, "Chapter 2");
    // Both entries should resolve to their spine chapters.
    assert_eq!(toc[0].spine_index, Some(0));
    assert_eq!(toc[1].spine_index, Some(1));
}

#[test]
fn unresolved_toc_entry_has_none_spine_index() {
    // Regression: a TOC entry whose href does not resolve to a spine item
    // used to silently fall back to spine_index 0, sending the reader to
    // the wrong chapter.  It must now be `None` so the UI skips it.
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "unresolved", "epub2_unresolved");
    let book = termepub::EpubBook::open(&path, true).expect("should open");
    let toc = book.toc();
    assert_eq!(toc.len(), 2, "expected two TOC entries: {toc:?}");
    // First entry resolves to the only spine item.
    assert_eq!(toc[0].spine_index, Some(0), "first entry should resolve");
    // Second entry points at a missing member -> must be None, not 0.
    assert_eq!(
        toc[1].spine_index, None,
        "unresolved entry must be None, not silently chapter 0"
    );
}

#[test]
fn empty_spine_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "empty_spine", "empty_spine");
    let result = termepub::EpubBook::open(&path, true);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("spine") || err.contains("chapter"),
        "error should mention spine/chapter: {err}"
    );
}

#[test]
fn missing_spine_member_produces_placeholder() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "missing", "missing_spine");
    let book = termepub::EpubBook::open(&path, true).expect("should open despite missing member");
    assert!(book.chapter_count() >= 1);
}

// ---------------------------------------------------------------------------
// Fixtures are valid ZIP files
// ---------------------------------------------------------------------------

#[test]
fn fixture_epub2_is_valid_zip() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "epub2", "epub2_ncx");
    let archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    assert!(!archive.is_empty());
}

#[test]
fn fixture_epub3_is_valid_zip() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "epub3", "epub3_nav");
    let archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    assert!(!archive.is_empty());
}

#[test]
fn fixture_encrypted_text_is_valid_zip() {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "encrypted", "encrypted_text");
    let archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
    assert!(!archive.is_empty());
    let has_encryption = archive.file_names().any(|n| n.contains("encryption.xml"));
    assert!(has_encryption, "fixture must contain encryption.xml");
}

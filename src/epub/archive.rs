use crate::error::Error;
use std::collections::HashSet;
use std::io::{self, Read};
use std::path::Path;

/// Maximum number of ZIP members allowed in an EPUB.
pub const MAX_EPUB_MEMBERS: usize = 10_000;

/// Maximum decompressed size for a single text member (25 MiB).
pub const MAX_EPUB_TEXT_MEMBER_SIZE: u64 = 25 * 1024 * 1024;

/// Maximum aggregate decompressed text size (100 MiB).
pub const MAX_EPUB_TOTAL_TEXT_SIZE: u64 = 100 * 1024 * 1024;

/// Maximum compression ratio for members larger than 1 MiB.
pub const MAX_EPUB_COMPRESSION_RATIO: u64 = 1_000;

/// Threshold for checking compression ratio (1 MiB).
const COMPRESSION_CHECK_THRESHOLD: u64 = 1024 * 1024;

/// A bounded reader for EPUB ZIP archives.
///
/// Enforces member count, per-member size, aggregate size, and compression
/// ratio limits.  Tracks which members have been read to avoid double-
/// counting aggregate size.
pub struct Archive {
    inner: zip::ZipArchive<io::BufReader<std::fs::File>>,
    /// Members whose decompressed size has already been counted toward the
    /// aggregate total.  Repeated reads of the same member do not add again.
    counted: HashSet<String>,
    /// Running total of unique decompressed bytes read.
    text_bytes_read: u64,
}

impl std::fmt::Debug for Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Archive")
            .field("counted", &self.counted)
            .field("text_bytes_read", &self.text_bytes_read)
            .finish_non_exhaustive()
    }
}

impl Archive {
    /// Opens an EPUB file at `path` with safety checks.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = std::fs::File::open(path).map_err(|e| Error::io_path(path, e))?;
        let reader = io::BufReader::new(file);
        let inner = zip::ZipArchive::new(reader)
            .map_err(|e| Error::InvalidEpub(format!("failed to parse ZIP archive: {e}")))?;

        let member_count = inner.len();
        if member_count > MAX_EPUB_MEMBERS {
            return Err(Error::InvalidEpub(format!(
                "too many files in archive: {member_count} (max {MAX_EPUB_MEMBERS})"
            )));
        }

        Ok(Self {
            inner,
            counted: HashSet::new(),
            text_bytes_read: 0,
        })
    }

    /// Reads a text member from the archive, enforcing safety limits.
    pub fn read_text(&mut self, name: &str) -> Result<String, Error> {
        let path = normalize_epub_path(name);

        let info = self
            .inner
            .by_name(&path)
            .map_err(|_| Error::InvalidEpub(format!("member not found: {path}")))?;

        if info.is_dir() {
            return Err(Error::InvalidEpub(format!(
                "expected file, got directory: {path}"
            )));
        }

        let decompressed_size = info.size();
        if decompressed_size > MAX_EPUB_TEXT_MEMBER_SIZE {
            return Err(Error::InvalidEpub(format!(
                "member \"{path}\" is too large: {} bytes (max {})",
                decompressed_size, MAX_EPUB_TEXT_MEMBER_SIZE
            )));
        }

        // Compression ratio check for large members.
        let compressed_size = info.compressed_size();
        if decompressed_size > COMPRESSION_CHECK_THRESHOLD && compressed_size > 0 {
            let ratio = decompressed_size / compressed_size;
            if ratio > MAX_EPUB_COMPRESSION_RATIO {
                return Err(Error::InvalidEpub(format!(
                    "suspicious compression ratio for \"{path}\": {ratio}:1 (max {MAX_EPUB_COMPRESSION_RATIO}:1)"
                )));
            }
        }

        // Read the content, bounding the read with `take` so a lying central
        // directory (under-reporting a member's decompressed size) cannot
        // trigger an unbounded allocation before the size check below fires.
        let mut content = Vec::with_capacity(decompressed_size as usize);
        let mut reader = info.take(MAX_EPUB_TEXT_MEMBER_SIZE + 1);
        reader
            .read_to_end(&mut content)
            .map_err(|e| Error::InvalidEpub(format!("failed to read \"{path}\": {e}")))?;

        if (content.len() as u64) > MAX_EPUB_TEXT_MEMBER_SIZE {
            return Err(Error::InvalidEpub(format!(
                "member \"{path}\" exceeds the per-member size limit after read"
            )));
        }

        // Aggregate tracking: count each unique member once.
        if self.counted.insert(path.clone()) {
            self.text_bytes_read += content.len() as u64;
            if self.text_bytes_read > MAX_EPUB_TOTAL_TEXT_SIZE {
                return Err(Error::InvalidEpub(format!(
                    "too much decompressed text: {} bytes (max {})",
                    self.text_bytes_read, MAX_EPUB_TOTAL_TEXT_SIZE
                )));
            }
        }

        Ok(String::from_utf8_lossy(&content).into_owned())
    }

    /// Returns whether a member exists in the archive.
    pub fn contains(&mut self, name: &str) -> bool {
        let path = normalize_epub_path(name);
        self.inner.by_name(&path).is_ok()
    }

    /// Parses `META-INF/encryption.xml` if present, and returns the set of
    /// encrypted URIs.
    pub fn parse_encryption(&mut self) -> Result<Vec<String>, Error> {
        if !self.contains("META-INF/encryption.xml") {
            return Ok(Vec::new());
        }

        let xml = self.read_text("META-INF/encryption.xml")?;
        parse_encryption_uris(&xml)
    }
}

/// Returns the local part of a qualified XML name (strips namespace prefix).
fn local_name(qname: &[u8]) -> &[u8] {
    qname.splitn(2, |&b| b == b':').last().unwrap_or(qname)
}

/// Parses `encryption.xml` and extracts the URI references.
fn parse_encryption_uris(xml: &str) -> Result<Vec<String>, Error> {
    let mut uris = Vec::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(ref e))
            | Ok(quick_xml::events::Event::Empty(ref e)) => {
                if local_name(e.name().as_ref()) == b"CipherReference" {
                    for attr in e.attributes() {
                        let attr = attr.map_err(|err| {
                            Error::InvalidEpub(format!("malformed encryption.xml: {err}"))
                        })?;
                        if local_name(attr.key.as_ref()) == b"URI" {
                            let value = std::str::from_utf8(attr.value.as_ref()).map_err(|_| {
                                Error::InvalidEpub("non-UTF-8 URI in encryption.xml".into())
                            })?;
                            uris.push(value.trim().to_string());
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(Error::InvalidEpub(format!("malformed encryption.xml: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    Ok(uris)
}

/// Normalizes an EPUB internal path: strips fragments, resolves `.` and `..`
/// segments safely, and ensures forward slashes.
pub fn normalize_epub_path(path: &str) -> String {
    let path = path.split('#').next().unwrap_or(path);
    // Some EPUBs (notably those authored on Windows) use backslash separators;
    // normalize them to forward slashes before resolving segments.
    let path = path.replace('\\', "/");

    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        match segment {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_removes_fragment() {
        assert_eq!(normalize_epub_path("chapter.xhtml#sec1"), "chapter.xhtml");
    }

    #[test]
    fn normalize_resolves_dot() {
        assert_eq!(normalize_epub_path("./chapter.xhtml"), "chapter.xhtml");
    }

    #[test]
    fn normalize_resolves_dotdot() {
        assert_eq!(
            normalize_epub_path("OEBPS/chapters/../images/pic.png"),
            "OEBPS/images/pic.png"
        );
    }

    #[test]
    fn normalize_converts_backslashes() {
        // EPUBs authored on Windows may use backslash separators.
        assert_eq!(
            normalize_epub_path("OEBPS\\chapters\\chapter1.xhtml"),
            "OEBPS/chapters/chapter1.xhtml"
        );
        assert_eq!(
            normalize_epub_path("content.opf\\..\\nav.xhtml"),
            "nav.xhtml"
        );
    }

    #[test]
    fn normalize_handles_trailing_slash() {
        assert_eq!(normalize_epub_path("OEBPS/"), "OEBPS");
    }

    #[test]
    fn normalize_is_idempotent() {
        assert_eq!(normalize_epub_path("chapter.xhtml"), "chapter.xhtml");
    }

    #[test]
    fn parse_encryption_finds_cipher_reference() {
        let xml = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <EncryptedContent>
    <EncryptionInfo>
      <enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#keyword"/>
      <EncryptedCipherData>
        <enc:CipherReference URI="chapter1.xhtml"/>
      </EncryptedCipherData>
    </EncryptionInfo>
  </EncryptedContent>
</encryption>"#;
        let uris = parse_encryption_uris(xml).unwrap();
        assert!(
            uris.contains(&"chapter1.xhtml".to_string()),
            "uris: {uris:?}"
        );
    }
}

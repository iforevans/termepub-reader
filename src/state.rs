//! State persistence: `state.json` compatibility and atomic writes.
//!
//! Reads the Python-format `state.json` defensively, preserving unknown
//! fields and entries.  Uses a sibling temporary file and rename for
//! atomic state writes.

use sha1::Digest;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::Error;

/// Default state file name.
const STATE_FILE: &str = "state.json";

/// Default theme.
const DEFAULT_THEME: &str = "dark";

/// Represents a single book's saved reading position.
#[derive(Debug, Clone, Default)]
pub struct BookState {
    /// Zero-based chapter index, clamped to zero on load if invalid.
    pub chapter_index: usize,
    /// Zero-based page index, clamped to zero on load if invalid.
    pub page_index: usize,
}

impl BookState {
    /// Extract chapter and page from a JSON value, clamping invalid values.
    fn from_value(val: &serde_json::Value) -> Self {
        let map = match val {
            serde_json::Value::Object(map) => map,
            _ => return Self::default(),
        };

        let chapter_index = map
            .get("chapter_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        let page_index = map
            .get("page_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        Self {
            chapter_index,
            page_index,
        }
    }
}

/// Computes the termepub config directory for a given home directory.
fn config_dir_for(home: &Path) -> PathBuf {
    home.join(".config").join("termepub")
}

/// Persistent state store for reading positions, bookmarks, and global
/// settings.
pub struct StateStore {
    /// Path to the state file.
    path: PathBuf,
    /// Raw JSON data, preserving unknown fields.
    data: serde_json::Value,
}

impl StateStore {
    /// Opens a state file at the given path.
    ///
    /// If the file does not exist or is invalid, returns a store with
    /// default values.
    pub fn open(path: PathBuf) -> Result<Self, Error> {
        let data = if path.exists() {
            Self::load_file(&path)?
        } else {
            serde_json::json!({})
        };

        let mut store = Self { path, data };
        store.migrate_bookmarks();
        Ok(store)
    }

    /// Copies legacy bookmark positions (stored in the shared
    /// `chapter_index`/`page_index` fields) into the dedicated
    /// `bookmark_chapter_index`/`bookmark_page_index` fields so that saving a
    /// new reading position no longer overwrites the bookmark.  Runs once on
    /// load for files written before the split; the copied values are then
    /// persisted on the next save.
    fn migrate_bookmarks(&mut self) {
        let Some(map) = self.data.as_object_mut() else {
            return;
        };
        for (key, val) in map.iter_mut() {
            if key == "_global" {
                continue;
            }
            let Some(entry) = val.as_object_mut() else {
                continue;
            };
            let has_bookmark = entry
                .get("bookmark")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if has_bookmark && !entry.contains_key("bookmark_chapter_index") {
                let chapter = entry
                    .get("chapter_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let page = entry
                    .get("page_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                entry.insert(
                    "bookmark_chapter_index".to_string(),
                    serde_json::json!(chapter),
                );
                entry.insert("bookmark_page_index".to_string(), serde_json::json!(page));
            }
        }
    }

    /// Opens the default state file at `~/.config/termepub/state.json`.
    ///
    /// Creates the directory if it does not exist.
    pub fn open_default() -> Result<Self, Error> {
        let config_dir = Self::config_dir()?;
        fs::create_dir_all(&config_dir).map_err(|e| Error::io_path(&config_dir, e))?;
        let path = config_dir.join(STATE_FILE);
        Self::open(path)
    }

    /// Returns the termepub config directory (`~/.config/termepub`, or
    /// `$XDG_CONFIG_HOME/termepub` when XDG_CONFIG_HOME is set — matching
    /// `dictionary::dirs_config_path`).
    ///
    /// The directory itself may not exist yet; callers create it as needed.
    fn config_dir() -> Result<PathBuf, Error> {
        if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(config).join("termepub"));
        }
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        Ok(config_dir_for(&home))
    }

    /// Loads and validates a state file.
    ///
    /// - Root must be an object.
    /// - Retains only object-valued entries.
    fn load_file(path: &Path) -> Result<serde_json::Value, Error> {
        let content = fs::read_to_string(path).map_err(|e| Error::io_path(path, e))?;
        let val: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| Error::Message(format!("invalid JSON: {e}")))?;

        match val {
            serde_json::Value::Object(map) => {
                // Retain only object-valued entries.
                let filtered: serde_json::Map<String, serde_json::Value> =
                    map.into_iter().filter(|(_, v)| v.is_object()).collect();
                Ok(serde_json::Value::Object(filtered))
            }
            _ => {
                // Non-object root -> safe default.
                Ok(serde_json::json!({}))
            }
        }
    }

    /// Returns the path to the state file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the current theme (default: "dark").
    pub fn get_theme(&self) -> String {
        self.get_global_str("theme")
            .unwrap_or_else(|| DEFAULT_THEME.into())
    }

    /// Returns whether the header should be shown (default: true).
    pub fn get_show_header(&self) -> bool {
        self.get_global_bool("show_header", true)
    }

    /// Returns whether text should be justified (default: false).
    pub fn get_justify_text(&self) -> bool {
        self.get_global_bool("justify_text", false)
    }

    /// Returns the last opened book path, if any.
    pub fn get_last_book_path(&self) -> Option<String> {
        self.get_global_str("last_book_path")
    }

    /// Reads an optional string from the `_global` object.
    fn get_global_str(&self, key: &str) -> Option<String> {
        let globals = self.data.get("_global")?;
        let map = globals.as_object()?;
        map.get(key).and_then(|v| v.as_str().map(String::from))
    }

    /// Reads a boolean from the `_global` object.
    fn get_global_bool(&self, key: &str, default: bool) -> bool {
        let Some(globals) = self.data.get("_global") else {
            return default;
        };
        let Some(map) = globals.as_object() else {
            return default;
        };
        map.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
    }

    /// Returns the reading position for a book key.
    pub fn get_state_for_book(&self, key: &str) -> BookState {
        match self.data.get(key) {
            Some(val) => BookState::from_value(val),
            None => BookState::default(),
        }
    }

    /// Updates the reading position for a book.
    ///
    /// Preserves unknown fields in the existing book entry.
    pub fn set_state_for_book(&mut self, key: &str, chapter: usize, page: usize) {
        let entry = self
            .data
            .as_object_mut()
            .expect("state root is always an object")
            .entry(String::from(key))
            .or_insert_with(|| serde_json::json!({}));

        if let Some(map) = entry.as_object_mut() {
            map.insert(String::from("chapter_index"), serde_json::json!(chapter));
            map.insert(String::from("page_index"), serde_json::json!(page));
        }
    }

    /// Sets a bookmark for a book.
    ///
    /// The bookmark is stored in dedicated `bookmark_chapter_index` /
    /// `bookmark_page_index` fields so that saving a reading position (which
    /// writes `chapter_index` / `page_index`) never clobbers it.
    pub fn set_bookmark(&mut self, key: &str, chapter: usize, page: usize) -> Result<(), Error> {
        let entry = self
            .data
            .as_object_mut()
            .expect("state root is always an object")
            .entry(String::from(key))
            .or_insert_with(|| serde_json::json!({}));

        if let Some(map) = entry.as_object_mut() {
            map.insert(
                String::from("bookmark_chapter_index"),
                serde_json::json!(chapter),
            );
            map.insert(String::from("bookmark_page_index"), serde_json::json!(page));
            map.insert(String::from("bookmark"), serde_json::json!(true));
        }

        Ok(())
    }

    /// Returns the bookmark position for a book, if set.
    ///
    /// Reads the dedicated bookmark fields, falling back to the shared
    /// `chapter_index` / `page_index` fields for files written before the
    /// split that were not migrated.
    pub fn get_bookmark(&self, key: &str) -> Option<BookState> {
        let val = self.data.get(key)?;
        let map = val.as_object()?;
        if !map
            .get("bookmark")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return None;
        }
        let chapter_index = map
            .get("bookmark_chapter_index")
            .or_else(|| map.get("chapter_index"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);
        let page_index = map
            .get("bookmark_page_index")
            .or_else(|| map.get("page_index"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);
        Some(BookState {
            chapter_index,
            page_index,
        })
    }

    /// Sets a global string value in the `_global` object.
    pub fn set_global_str(&mut self, key: &str, value: &str) {
        let global = self
            .data
            .as_object_mut()
            .expect("state root is always an object")
            .entry(String::from("_global"))
            .or_insert_with(|| serde_json::json!({}));
        if let Some(map) = global.as_object_mut() {
            map.insert(String::from(key), serde_json::json!(value));
        }
    }

    /// Sets a global boolean value in the `_global` object.
    pub fn set_global_bool(&mut self, key: &str, value: bool) {
        let global = self
            .data
            .as_object_mut()
            .expect("state root is always an object")
            .entry(String::from("_global"))
            .or_insert_with(|| serde_json::json!({}));
        if let Some(map) = global.as_object_mut() {
            map.insert(String::from(key), serde_json::json!(value));
        }
    }

    /// Saves state atomically using a sibling temp file and rename.
    ///
    /// The temp file name is suffixed with the PID so concurrently running
    /// instances do not clobber each other's in-progress write before the
    /// rename.
    pub fn save(&self) -> Result<(), Error> {
        let content = serde_json::to_string_pretty(&self.data)
            .map_err(|e| Error::Message(format!("serialization failed: {e}")))?;

        let tmp_path = self.path.with_file_name(format!(
            "{}.{}.tmp",
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("state"),
            std::process::id()
        ));
        let mut file = fs::File::create(&tmp_path).map_err(|e| Error::io_path(&tmp_path, e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| Error::io_path(&tmp_path, e))?;
        file.flush().map_err(|e| Error::io_path(&tmp_path, e))?;
        drop(file);

        fs::rename(&tmp_path, &self.path).map_err(|e| Error::io_path(&self.path, e))?;

        Ok(())
    }

    /// Computes the SHA-1 hex digest of a path string (used as book key).
    pub fn book_key(path: &str) -> String {
        let mut hasher = sha1::Sha1::new();
        hasher.update(path.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_key_matches_sha1() {
        let key = StateStore::book_key("/tmp/test.epub");
        assert_eq!(key.len(), 40);
        let mut hasher = sha1::Sha1::new();
        hasher.update("/tmp/test.epub".as_bytes());
        let expected = format!("{:x}", hasher.finalize());
        assert_eq!(key, expected);
    }

    #[test]
    fn empty_state_returns_defaults() {
        let tmp = std::env::temp_dir().join("termepub_test_empty.json");
        let store = StateStore::open(tmp).unwrap();
        assert_eq!(store.get_theme(), "dark");
        assert!(store.get_show_header());
    }

    #[test]
    fn config_dir_is_termepub_subdir_of_home_config() {
        // Regression: config_dir must point at ~/.config/termepub, NOT
        // ~/.config — writing state.json to the bare .config dir breaks
        // v1.x state compatibility and litters the user's home.
        assert_eq!(
            config_dir_for(Path::new("/home/testuser")),
            PathBuf::from("/home/testuser/.config/termepub")
        );
        assert_eq!(
            config_dir_for(Path::new("/home/testuser")),
            PathBuf::from("/home/testuser")
                .join(".config")
                .join("termepub")
        );
    }
}

//! External dictionary with lazy loading and fuzzy suggestions.
//!
//! Loads `ecdict_index.json` from disk on first lookup via `OnceLock`.
//! Search order:
//! 1. `~/.config/termepub/ecdict_index.json`
//! 2. Next to the binary (resolved from `argv[0]`)
//!
//! Performs exact lowercase lookup, then retries with punctuation stripped.
//! Falls back to deterministic fuzzy suggestions limited to 5,000 candidates.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Maximum fuzzy-matching candidates to examine.
const MAX_CANDIDATES: usize = 5_000;

/// Lazy-loaded dictionary data. `None` until the background load settles;
/// `Some(None)` means the load finished but no dictionary file was found.
static DICTIONARY: OnceLock<Option<BTreeMap<String, Definition>>> = OnceLock::new();

/// Guards the one-time spawn of the background loader.
static DICT_LOAD_STARTED: OnceLock<()> = OnceLock::new();

/// A dictionary entry.
#[derive(Debug, Clone)]
struct Definition {
    headword: String,
    definition: String,
}

/// Kicks off background dictionary loading (idempotent, non-blocking).
///
/// The first call spawns a worker thread that parses the JSON and stores
/// the result. `lookup_word` reports "still loading" until it finishes, so
/// the UI thread never blocks on the ~21 MB parse.
pub fn preload_dictionary() {
    DICT_LOAD_STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            let _ = DICTIONARY.set(load_dictionary());
        });
    });
}

/// Resolves the path to the dictionary file.
fn find_dictionary_path() -> Option<PathBuf> {
    // 1. Config directory.
    if let Some(config_dir) = dirs_config_path() {
        let p = config_dir.join("ecdict_index.json");
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Next to the binary.
    if let Ok(argv0) = env::current_exe() {
        if let Some(parent) = argv0.parent() {
            let p = parent.join("ecdict_index.json");
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

fn dirs_config_path() -> Option<PathBuf> {
    // Shared with the state store so the dictionary and state file always
    // resolve to the same directory (including Windows %APPDATA%).
    crate::state::termepub_config_dir()
}

fn load_dictionary() -> Option<BTreeMap<String, Definition>> {
    let path = find_dictionary_path()?;
    let data = std::fs::read(&path).ok()?;
    let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&data).ok()?;

    let mut map = BTreeMap::new();
    for (word, val) in parsed {
        // Tolerate malformed entries: a single bad row must not abort the
        // whole load (the file has 160K+ entries; one non-object value
        // would otherwise make the entire dictionary unavailable).
        let Some(obj) = val.as_object() else {
            continue;
        };
        let headword = obj
            .get("headword")
            .and_then(|v| v.as_str())
            .unwrap_or(&word)
            .to_string();
        let definition = obj
            .get("def")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        map.insert(
            word,
            Definition {
                headword,
                definition,
            },
        );
    }
    Some(map)
}

/// Looks up a word in the dictionary.
///
/// First tries exact lowercase match, then retries with punctuation
/// stripped.  If no exact match is found, returns deterministic
/// suggestions limited to `MAX_CANDIDATES` candidates.  If the background
/// load has not finished yet, returns a friendly "still loading" message
/// instead of blocking the UI thread.
pub fn lookup_word(word: &str) -> String {
    preload_dictionary();
    let word_lower = word.to_lowercase();
    let stripped = word_lower
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>();

    match DICTIONARY.get() {
        Some(Some(dict)) => {
            // Exact match on lowercased word.
            if let Some(def) = dict.get(&word_lower) {
                return format!(
                    "{}\n\nfound: {}\n{}",
                    def.headword, def.headword, def.definition
                );
            }

            // Retry with punctuation stripped.
            if stripped != word_lower {
                if let Some(def) = dict.get(&stripped) {
                    return format!(
                        "{}\n\nfound: {}\n{}",
                        def.headword, def.headword, def.definition
                    );
                }
            }

            // No exact match — provide suggestions.
            suggest_word(&word_lower, dict)
        }
        Some(None) => format!("Dictionary not available: {word_lower}"),
        None => String::from("Dictionary is still loading — try again in a moment."),
    }
}

/// Generates deterministic suggestions for a misspelled word.
///
/// Sorts candidate words, examines up to `MAX_CANDIDATES` length-compatible
/// entries, and returns the best matches.
fn suggest_word(word: &str, dict: &BTreeMap<String, Definition>) -> String {
    let target_len = word.len();

    // Filter to length-compatible candidates (within ±2 of target length).
    let min_len = target_len.saturating_sub(2);
    let max_len = target_len + 2;

    // Typos usually preserve the first character, so collect length-compatible
    // candidates that share it separately from the rest.  Scoring both pools
    // (prefix matches first) keeps suggestions relevant instead of defaulting
    // to the alphabetically-earliest length-matching words.
    let first_char = word.chars().next();
    let mut prefix_matches: Vec<&str> = Vec::new();
    let mut length_matches: Vec<&str> = Vec::new();
    for k in dict.keys() {
        if k.len() < min_len || k.len() > max_len {
            continue;
        }
        if first_char.is_some_and(|c| k.starts_with(c)) {
            if prefix_matches.len() < MAX_CANDIDATES {
                prefix_matches.push(k.as_str());
            }
        } else if length_matches.len() < MAX_CANDIDATES {
            length_matches.push(k.as_str());
        }
    }

    // Score candidates by similarity, prefix matches first.
    let mut scored: Vec<(usize, &str)> = Vec::new();
    for pool in [&prefix_matches, &length_matches] {
        for cand in pool {
            let score = similarity_score(word, cand);
            if score > 0 {
                scored.push((score, cand));
            }
        }
    }

    // Sort by score descending, then alphabetically for determinism.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));

    let top: Vec<String> = scored
        .iter()
        .take(5)
        .map(|(_, cand)| {
            let cand = *cand;
            dict.get(cand)
                .map(|d| d.headword.clone())
                .unwrap_or_else(|| cand.to_string())
        })
        .collect();

    if top.is_empty() {
        format!("Not found: {word}\n\nNo suggestions available.")
    } else {
        let suggestions = top.join(", ");
        format!("Not found: {word}\n\nDid you mean: {suggestions}?")
    }
}

/// Computes a simple similarity score between two strings.
///
/// Uses character overlap and edit-distance heuristics.
fn similarity_score(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    if a_bytes.len().abs_diff(b_bytes.len()) > 3 {
        return 0;
    }

    // Count common characters (simple overlap).
    let common = a_bytes.iter().filter(|c| b_bytes.contains(c)).count();

    if common < a_bytes.len() / 2 {
        return 0;
    }

    // Prefer exact prefix matches.
    let prefix = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();

    common + prefix * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_deterministic() {
        // Allow the lazy background load to settle before asserting.
        for _ in 0..200 {
            if DICTIONARY.get().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let r1 = lookup_word("zzzzzzzzz");
        let r2 = lookup_word("zzzzzzzzz");
        assert_eq!(r1, r2, "suggestions must be deterministic");
    }
}

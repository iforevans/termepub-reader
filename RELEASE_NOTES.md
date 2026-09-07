# Release Notes

## termepub v2.4.1 — 2026-09-07

Bug-fix release from a second code review. Six correctness bugs fixed; no new features.

### What Changed

- **Inline styling no longer lost mid-line** — a wrapped line was flattened into a single segment carrying the *first* word's style, so a bold or colored span sharing a line with other text rendered unstyled. Each contiguous run of identical style is now kept as its own segment (justified lines included).
- **Bookmarks no longer clobbered by the reading position** — bookmarks and the saved position shared the same `chapter_index`/`page_index` fields, so continuing to read after setting a bookmark moved it. Bookmarks are now stored in dedicated fields, and pre-existing (Python v1) state files are migrated on load.
- **Footer shows the correct chapter title** — the footer indexed the TOC by chapter position, but TOC entries are not 1:1 with spine chapters. It now looks up the entry whose resolved spine position matches the current chapter.
- **Search now covers the whole book** — search only scanned the current chapter. It now paginates and searches every chapter and jumps to the chapter/page of the first match, showing a "No results found." popup on a miss.
- **EPUB 3 nav: only the `epub:type="toc"` nav is used** — the parser previously grabbed the first `<nav>`, so a landmarks or guide nav placed before the TOC nav produced a wrong table of contents. Books that omit the type attribute still fall back to the first nav.
- **TOC links resolve against the TOC document's directory** — nav/NCX entry hrefs were resolved relative to the OPF's directory; a TOC document in a subdirectory now resolves its links correctly.

### Technical Details

- `wrap_paragraph` builds lines via a new `build_line_segments` helper (with `justify_gaps`/`plain_gaps`) that groups consecutive words by style.
- `StateStore` gained `migrate_bookmarks()` and dedicated `bookmark_chapter_index`/`bookmark_page_index` fields; `get_bookmark` falls back to the shared fields for unmigrated entries.
- `App::search_book` paginates all spine chapters at the current terminal size and maps the matched page back to `(chapter, page)`.
- `package::find_toc_nav_index` selects the TOC nav; `parse_nav_toc` tracks nav occurrences with a counter and stack.
- Test coverage: 100 passing tests locally (8 new regression tests across `layout`, `state`, `epub`, and a new `search` integration suite), plus 6 PTY integration tests that run with `--ignored`. New `epub3_nav_landmarks` fixture covers the nav-type fix.

---

## termepub v2.4.0 — 2026-09-06

Correctness and robustness release from a full code review. No new features; a series of bug fixes, edge-case hardening, and dead-code removal.

### What Changed

- **File picker no longer traps you** — with no book open (the picker is the home screen), `Esc` and `q` now raise the quit confirmation instead of doing nothing. Previously there was no way out but killing the process.
- **Justification can no longer panic** — a justified line that wraps to a single word has zero gaps to distribute across; the old code divided by zero (panic in release, integer-wrap in debug). Single-word lines are now joined plainly.
- **TOC jumps to the right chapter** — a table-of-contents entry whose link could not be resolved to a spine item used to silently jump to chapter 0. It is now marked unresolved and skipped, so the TOC no longer sends you to the wrong place.
- **Search finds phrases across any number of pages** — the old search only checked within-page and two-page windows, so a phrase spanning three page boundaries was never found. Search is now a single pass over the whole book with correct page mapping.
- **Book colors adapt to the theme** — a near-black color in a book's CSS is now lightened on dark themes (and a near-white color darkened on light themes) so text stays readable, matching the behavior the README always claimed.
- **Malformed CSS colors are rejected** — a 3-digit hex shorthand with a non-hex digit (e.g. `#a1g`) no longer produces a bogus color; it is ignored.
- **Self-closing metadata tags in the OPF** — a `<dc:creator/>`-style self-closing element no longer desyncs the package parser or drops the author.
- **`q` cancels the quit prompt** — pressing `q` on the "Quit? (y/n)" popup now cancels it (it was a no-op before).
- **Atomic state writes are concurrency-safe** — the temp file is now PID-suffixed so two running instances don't clobber each other's in-progress write.
- **Dead code removed** — the unused `ui/modal` module and several unused helpers (`Archive::read_binary`/`len`/`is_empty`, `width::{graphemes, grapheme_at, grapheme_byte_offset, split_into_words}`, `Theme::iter`) are gone.

### Technical Details

- `TocEntry::spine_index` is now `Option<usize>`; unresolved TOC entries are `None` and skipped by the UI.
- `EpubBook::chapters()` returns `&[Vec<StyledSegment>]` (slice, not `&Vec`).
- Body rendering no longer double-wraps text that pagination already sized to the terminal width.
- Test coverage: 96 passing tests locally (plus 6 PTY integration tests that run with `--ignored`), including regression tests for the picker escape, justification single-word lines, unresolved TOC entries, multi-page search, color adaptation, and PID-suffixed temp files.

---

## termepub v2.3.1 — 2026-08-15

Small polish release from the v2.3.0 code review.

### What Changed

- **Italic and strikethrough now render** — both were parsed from HTML (`<i>`, `<em>`, `<s>`, `<strike>`, `<del>`, `font-style: italic`, `text-decoration: line-through`) but silently dropped by the renderer. They now map to ratatui's `Modifier::ITALIC` and `Modifier::CROSSED_OUT` (SGR 3 and SGR 9 via crossterm).
- **Dictionary no longer tracked in git** — the 21 MB `ecdict_index.json` was committed by accident (and gitignored at the same time). It is untracked now; the loader finds it in `~/.config/termepub/` first, with the binary's directory as fallback, so nothing changes for users who have it installed.

### Technical Details

- Test coverage: 85 passing tests locally (4 new unit tests for the italic/strikethrough style mapping).

---

## termepub v2.3.0 — 2026-08-05

Bug-fix and UX release.

### What Changed

- **State file location fixed** — `state.json` is now read/written at `~/.config/termepub/state.json` (it was incorrectly written to `~/.config/state.json`). Reading positions, bookmarks, and theme from v1.x are picked up again automatically.
- **Stable book keys** — state keys are derived from lexically absolute paths (no symlink resolution), so opening a book via a relative path (`termepub book.epub`) and via its absolute path share one saved position.
- **File picker filtering implemented** — press `/` or `s` in the picker to filter entries by typing; Enter keeps the filter, Esc clears it. (Previously advertised in the help text but non-functional.)
- **Terminal always restored** — a Drop guard restores raw mode / alternate screen on errors and panics, not just clean exits.
- **Key auto-repeat** — holding navigation keys (arrows, Page Up/Down, `f`, `l`) now flips pages repeatedly; typing repeats in search, dictionary, and picker-filter input. Toggle keys and quit never auto-repeat.
- **TOC scrolling** — the selection stays visible in long tables of contents.
- **HTML extractor robustness** — a stray closing tag no longer wipes the style stack for the rest of the chapter.
- **Dictionary robustness** — the ~21 MB dictionary loads on a background thread (no UI freeze on first lookup, with a "still loading" message in the brief window) and malformed entries are skipped instead of aborting the entire load.
- **Esc fixed** in TOC, search, and picker modes (previously only Ctrl-C worked despite the help text); Ctrl-C while picker-filtering clears the filter instead of typing a literal `c`.
- **Repository hygiene** — bundled `.epub` test books are no longer tracked; `*.epub` is gitignored.

### Technical Details

- Test coverage: 80 passing tests locally (unit, CLI, EPUB parsing, HTML extraction, pagination, state persistence, dictionary lookup, responsive layout, PTY integration). Dictionary-lookup tests skip gracefully when the optional ECDICT dictionary is not installed; PTY tests build their EPUB fixtures from the tracked `tests/fixtures/` tree.

### Known Limitations

- Block-level inline CSS (`<p style="...">`, `<section style="...">`) colors are parsed but do not currently style text — only inline tags (`<span>`, `<b>`, etc.) and headings carry styles. (Pre-existing, noted during v2.3.0 review.)

---

## termepub v2.0.0 — 2026-07-30

Complete rewrite in Rust. Faster startup, zero runtime dependencies, self-contained binary with embedded dictionary.

### What Changed

- **Rewritten in Rust** — the entire codebase is now a compiled binary with no Python runtime requirement
- **Executable renamed** — `termepub.py` → `termepub`
- **Self-contained** — the ~24 MB binary includes the ECDICT dictionary; no installation step on first run
- **Faster startup and rendering** — no interpreter overhead, lazy dictionary loading
- **Grapheme-safe layout** — CJK characters, emoji, and combining marks are handled correctly without splitting across lines
- **Four themes** — added Solarized Dark and Solarized Light alongside the existing Dark and Light
- **Same keyboard controls** — all key bindings from v1.x are preserved
- **Same state file format** — `~/.config/termepub/state.json` is read and written in the Python-compatible format; bookmarks, reading positions, and theme preferences are preserved
- **EPUB safety limits** — bounded archive reads, encryption detection, compression ratio checks

### Technical Details

- **Dependencies:** clap, crossterm, ratatui, tokio, zip, quick-xml, unicode-width, unicode-segmentation, serde_json, sha1, thiserror
- **Test coverage:** 73 passing tests (unit, CLI, EPUB parsing, HTML extraction, pagination, state persistence, dictionary lookup, responsive layout) plus 6 PTY integration tests (ignored by default)
- **Minimum terminal size:** 10 columns × 5 rows; below this a "terminal too small" message is shown
- **Offline-only:** no network access, no cloud synchronization, no external data files

### Migration Notes

- If you were using `termepub.py`, update your shell alias or symlink to `termepub`
- Your existing `~/.config/termepub/state.json` will work without modification
- The dictionary no longer needs to be downloaded or installed — it is built into the binary
- The `--no-css`, `--bookmark`, and `--version` flags work exactly as before

### Known Limitations

- DRM-protected EPUBs with encrypted text resources are rejected (same as v1.x)
- Italic and line-through are parsed but may not render visibly in all terminal emulators
- PTY resize tests are provided but must be run manually with `--ignored --test-threads=1`

---

## termepub v1.0.4 — 2026-07-29

Post-1.0 code hardening: UTF-8 rendering, 256-color support, deterministic dictionary suggestions, and miscellaneous correctness fixes.

### Changes

- `ascii_sanitize` now preserves all printable Unicode characters (accented Latin, CJK, etc.) instead of stripping everything above ASCII
- Color rendering uses the 256-color cube on capable terminals, falling back to the 16-color ANSI palette on error
- Dictionary "did you mean" suggestions are now deterministic
- Duplicate-segment detection requires blank-line separation and text ≤ 40 chars, preventing loss of legitimate repeated body text
- Color pair wraparound at 256 pairs now clears the cache before reuse
- `_get_plain_pages` removed — dead code after the v1.0.1 styled-pages optimization
- `show_help` collapsed from three sequential popups into a single scrollable popup
- Test count increased from 22 to 30

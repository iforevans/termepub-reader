# termepub-reader

A terminal-based (NCurses) ePUB reader with a clean, keyboard-driven interface. Built for offline reading in terminal environments.

**Version:** 0.4.8 (2026-03-30)

## Features

- **File Picker:** Browse and open EPUB files with advanced navigation
- **Navigation:** Page/chapter forward/back (arrow keys, j/k, n/p)
- **Table of Contents:** Interactive TOC with visual selection indicator (=>) - *v0.4.4*
- **Search:** Full-text search with chapter highlighting
- **Bookmarks:** Save and restore reading position
- **Themes:** Dark/light mode toggle
- **Progress Tracking:** Overall book pagination with percentage
- **CSS Styling:** Inline CSS support (bold, underline, italic) - *v0.4.2*

## Controls

| Key | Action |
|-----|--------|
| `←/→` or `j/k` | Page navigation |
| `↑/↓` or `n/p` | Chapter navigation |
| `t` | Table of contents |
| `/` | Search |
| `b` | Bookmark |
| `o` | Open book (file picker) |
| `s` | In picker - start live search/filter |
| `j` | In picker - jump to letter |
| `m` | Toggle theme |
| `h` | Toggle header |
| `g` | Toggle heading style (bold/reverse) |
| `q` | Quit |

## Usage

```bash
termepub_reader.py [book.epub] [--bookmark] [--no-css] [--version]
```

**Options:**
- `--bookmark`: Open book at saved bookmark position
- `--no-css`: Disable inline CSS styling (faster on slow devices)
- `--version`: Show version number and exit

## Installation

1. Clone this repository
2. Make the script executable:
   ```bash
   chmod +x termepub_reader.py
   ```
3. Run it:
   ```bash
   ./termepub_reader.py
   ```

## CSS Styling Support (v0.4.2)

The reader now supports inline CSS styling from EPUB files:

**Currently rendered:**
- **Bold text:** `<b>`, `<strong>`, `font-weight: bold`
- **Underline:** `<u>`, `text-decoration: underline`
- **Italic:** `<i>`, `<em>`, `font-style: italic` (terminal-dependent)
- **Line-through:** `<s>`, `<strike>`, `<del>` (terminal-dependent)

**Future work:**
- Color support (`color: #rrggbb`, `color: rgb(r,g,b)`)
- More text decorations and font properties

## Requirements

- Python 3.9+
- No external dependencies (uses only stdlib: `zipfile`, `xml.etree`, `html.parser`, `curses`)

## State

User state (bookmarks, reading position) is stored in `~/.config/termepub/state.json`.

## License

MIT

## Author

Ifor Evans - [@iforevans](https://github.com/iforevans)

---

Pair programmed with my OpenClaw Agent Sparky ⚡. Using local Qwen 27B running on an eGPU (RTX 3090/24GB)

---

## Recent Changes

### v0.4.7 (2026-03-29) - Code Quality & Safety

**Cleanup:**
- Removed dead code: `_get_pages_with_attrs()` (was defined but never called)
- Renamed `_get_pages()` → `_get_plain_pages()` for clarity
- Extracted footer format string to `FOOTER_FORMAT` constant
- Added clarifying comments to status message clears

**Documentation:**
- Added comments to all 13 bare `pass` statements explaining error handling
- Improved docstring for `_get_current_styles()`

**Safety:**
- Added runtime validation for style stack underflow (catches HTML parsing bugs)

**Net change:** -4 lines of dead code, +15 lines of documentation/safety

### v0.4.6 (2026-03-29) - Code Cleanup

**Cleanup:**
- Converted remaining `%` formatting to f-strings (18 instances)
- Improved consistency with modern Python 3.9+ style

### v0.4.8 (2026-03-30) - Style Boundary Fix

**Bug Fixes:**
- Fixed CSS style mapping for wrapped text (styles now preserved across line breaks)
- Deduplicates consecutive identical segments (e.g., duplicate chapter headings)
- Limits consecutive blank lines to avoid excessive whitespace

**Technical:**
- Rewrote text wrapping to use segment-aware algorithm
- Each line can now have multiple style fragments (e.g., "Title: Pride and Prejudice" with "Title" bold)
- Style boundaries are preserved even when text wraps

### v0.4.7 (2026-03-29) - Code Quality

**Cleanup:**
- Removed dead code (`_get_pages_with_attrs` method)
- Renamed `_get_pages` to `_get_plain_pages` for clarity
- Added safety checks for style stack underflow

### v0.4.5 (2026-03-28) - TOC Improvements

**Features:**
- Added `=>` visual indicator for selected TOC entries
- Removed redundant chapter numbers (cleaner display)
- Added navigation hint in TOC footer

### v0.4.4 (2026-03-28) - Code Quality

**Cleanup:**
- Removed 44 lines of dead code (duplicate methods, unused CSS extraction)
- Added cache invalidation for theme/heading style toggles
- Added `--version` flag
- Added Sparky co-author credit

### v0.4.3 (2026-03-27) - CSS Rendering

**Features:**
- Full inline CSS styling support (bold, underline, italic, line-through)
- StyledSegment dataclass with style stack for CSS inheritance
- Handles semantic tags: `<b>`, `<strong>`, `<i>`, `<em>`, `<u>`, `<s>`

### v0.4.2 (2026-03-25) - Unicode & Popups

**Improvements:**
- Comprehensive Unicode sanitization (34 character replacements)
- Styled popup system with bordered styling
- Better terminal compatibility for complex Unicode content

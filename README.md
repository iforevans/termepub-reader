# termepub-reader

A terminal-based (NCurses) ePUB reader with a clean, keyboard-driven interface. Built for offline reading in terminal environments.

**Version:** 0.4.2

## Features

- **File Picker:** Browse and open EPUB files with advanced navigation
- **Navigation:** Page/chapter forward/back (arrow keys, j/k, n/p)
- **Table of Contents:** Jump to chapters via TOC
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
termepub_reader.py [book.epub] [--bookmark] [--no-css]
```

**Options:**
- `--bookmark`: Open book at saved bookmark position
- `--no-css`: Disable inline CSS styling (faster on slow devices)

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

---

*Created by Ifor Evans (Sparky)*

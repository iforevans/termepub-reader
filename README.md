# termepub

A terminal EPUB reader with a clean, keyboard-driven interface. Written in Rust for speed, reliability, and zero runtime dependencies.

**Version:** 2.5.0 (2026-09-07)

## Features

- **File Picker:** Browse and open EPUB files with filtering (press `/` or `s`, then type; Enter keeps the filter, Esc clears it)
- **Navigation:** Page/chapter forward/back (arrow keys, Page Up/Down; holding a key auto-repeats)
- **Table of Contents:** Interactive, scrollable TOC for EPUB 2 (NCX) and EPUB 3 (nav document)
- **Search:** Full-text search with phrase matching across lines and page boundaries
- **Bookmarks:** Save and restore reading position
- **Themes:** Four themes — Dark, Light, Solarized Dark, Solarized Light
- **Progress Tracking:** Overall book pagination with percentage
- **CSS Styling:** Inline CSS support (bold, underline, italic, strikethrough, colors)
- **Justified Text:** Toggle justified text alignment
- **Dictionary Lookup:** 160K+ word dictionary (ECDICT), loaded in the background so the first lookup never freezes the UI
- **Responsive Layout:** Terminal resize support with automatic re-pagination
- **Safe EPUB Handling:** Bounded archive reads with encryption and size limits
- **Non-interactive Mode:** Run without a TTY to dump the book's plain text to stdout (`termepub book.epub | less`)
- **Color-coded Dialogs:** Informational dialogs (help, dictionary, confirmations) render on a blue background; error dialogs on red — both with white text, independent of the active theme

## Controls

| Key | Action |
|-----|--------|
| `←` | Page back (prev chapter at start) |
| `→` | Page forward (next chapter at end) |
| `↑` | Previous chapter |
| `↓` | Next chapter |
| Page Down | Next page |
| Page Up | Previous page |
| `f` | Go to first page |
| `l` | Go to last page |
| `i` | Table of contents |
| `/` | Search |
| `o` | Open book (file picker) |
| `m` | Set bookmark |
| `b` | Go to bookmark |
| `t` | Cycle theme |
| `h` | Toggle header |
| `j` | Toggle justification |
| `d` | Dictionary prompt |
| `?` | Help |
| `q` / `Ctrl-c` | Quit |

## Usage

```bash
termepub [book.epub] [--bookmark] [--no-css] [--version]
```

**Options:**
- `--bookmark`: Open book at saved bookmark position
- `--no-css`: Disable inline CSS styling (faster on slow devices)
- `--version`: Show version number and exit

**Non-interactive (piped) mode:** When run without a TTY, termepub dumps the book's plain text to stdout instead of starting the interactive reader: `termepub book.epub | less`.

## Installation

### From source (Rust required)

```bash
cargo install --path .
```

### Build manually

```bash
cargo build --release
cp target/release/termepub ~/.local/bin/
```

### Prebuilt binary

Download the latest release binary and place it on your `PATH`.

## CSS Styling Support

The reader supports inline CSS styling from EPUB files:

**Currently rendered:**
- **Bold text:** `<b>`, `<strong>`, `font-weight: bold`
- **Underline:** `<u>`, `text-decoration: underline`
- **Italic:** `<i>`, `<em>`, `font-style: italic` (via ANSI escapes; visible in most modern terminals)
- **Strikethrough:** `<s>`, `<strike>`, `<del>`, `text-decoration: line-through`
- **Colors:**
  - Hex: `color: #rrggbb` or `color: #rgb`
  - RGB: `color: rgb(r,g,b)`
  - Named colors (red, blue, green, etc.)
  - Colors adapt to current theme (dark/light mode)

## Requirements

- Linux, macOS, or Windows (with a compatible terminal emulator)
- Terminal with minimum 10 columns × 5 rows; recommended 80×24
- 256-color terminal support for optimal color rendering
- No runtime dependencies (no Python, no external libraries)
- ECDICT dictionary file (`ecdict_index.json`) for dictionary lookup

## Dictionary

The reader uses the ECDICT dictionary (~21 MB, 160,000+ modern English definitions). The dictionary file (`ecdict_index.json`) is loaded from disk on first use via lazy loading. It is searched for in the following locations:

1. `~/.config/termepub/ecdict_index.json`
2. Next to the `termepub` binary

Place the dictionary file in one of these locations to enable dictionary lookup. The dictionary can be obtained from the [ECDICT project](https://github.com/xu-song/ecdict).

## State

User state (reading position, bookmarks, theme preference, last opened book) is stored in `~/.config/termepub/state.json`. The state format is compatible with the Python v1.x version of termepub.

## EPUB Safety

The reader enforces safety limits when opening EPUB files:
- Maximum 10,000 archive members
- Maximum 25 MiB per text member
- Maximum 100 MiB total decompressed text
- Compression ratio check (rejects suspiciously high ratios)
- Encrypted spine text resources are rejected (DRM-protected books are not supported)
- Font-only encryption is tolerated

## Offline-Only

termepub operates entirely offline. It does not access the network, download content, or synchronize with cloud services.

## License

MIT

## Author

Ifor Evans - [@iforevans](https://github.com/iforevans)

## Migration from v1.x (Python)

termepub v2.1.0 is a complete rewrite in Rust. The executable name changed from `termepub.py` to `termepub`. Key differences:

- **Faster startup and rendering** — compiled binary with no interpreter overhead
- **Self-contained** — no Python runtime or external dependencies required
- **Dictionary lookup** — ECDICT dictionary loaded from disk on first use (place `ecdict_index.json` in `~/.config/termepub/` or next to the binary)
- **State compatibility** — `~/.config/termepub/state.json` is read and written in the same format; reading position, bookmarks, and theme preferences are preserved
- **Same keyboard controls** — all v1.x key bindings are preserved
- **Four themes** — added Solarized Dark and Solarized Light
- **Grapheme-safe layout** — CJK, emoji, and combining marks are handled correctly without splitting

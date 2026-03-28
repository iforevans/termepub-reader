#!/usr/bin/env python3
"""
termepub_reader.py - Terminal-based EPUB reader with inline CSS styling support.

Features:
- Read EPUB files in the terminal using curses
- Inline CSS styling (bold, underline, italic, line-through)
- Chapter navigation, bookmarks, file picker with live search
- State persistence across sessions

Version: 0.4.2
"""
import curses
import hashlib
import html
import json
import os
import re
import sys
import textwrap
import unicodedata
import zipfile
from dataclasses import dataclass
from html.parser import HTMLParser
from typing import Dict, List, Optional, Tuple
import xml.etree.ElementTree as ET

__version__ = "0.4.4"

CONFIG_DIR = os.path.join(os.path.expanduser("~"), ".config", "termepub")
STATE_FILE = os.path.join(CONFIG_DIR, "state.json")


@dataclass
class TocEntry:
    title: str
    href: str
    spine_index: int


@dataclass
class BookState:
    chapter_index: int = 0
    page_index: int = 0


@dataclass
class StyledSegment:
    """A text segment with associated CSS styles.
    
    Each segment represents a contiguous run of text with the same styling.
    Segments are created during HTML parsing and merged when adjacent segments
    have identical styles for efficiency.
    """
    text: str
    styles: dict  # {'font_weight': 'bold', 'color': '#ff0000', ...}


def strip_ns(tag: str) -> str:
    return tag.split("}", 1)[-1] if "}" in tag else tag


def norm_href(base_path: str, href: str) -> str:
    clean = href.split("#", 1)[0]
    if not base_path:
        return os.path.normpath(clean)
    return os.path.normpath(os.path.join(os.path.dirname(base_path), clean))


def ascii_sanitize(text: str) -> str:
    """Convert special characters to ASCII equivalents for terminal compatibility."""
    replacements = {
        "\u2018": "'",  # Left single quote
        "\u2019": "'",  # Right single quote
        "\u201c": '"',  # Left double quote
        "\u201d": '"',  # Right double quote
        "\u2013": "-",  # En dash
        "\u2014": "--", # Em dash
        "\u2026": "...",# Ellipsis
        "\u00a0": " ",  # Non-breaking space
        "\u00ad": "",   # Soft hyphen
        "\u2190": "<-", # Left arrow
        "\u2191": "^",  # Up arrow
        "\u2192": "->", # Right arrow
        "\u2193": "v",  # Down arrow
        "\u2010": "-",  # Hyphen
        "\u2011": "-",  # Non-breaking hyphen
        "\u2012": "-",  # Figure dash
        "\u2015": "---",# Horizontal bar
        "\u2039": "<",  # Single left-pointing angle quotation mark
        "\u203a": ">",  # Single right-pointing angle quotation mark
        "\u2500": "-",  # Box drawings light horizontal
        "\u2502": "|",  # Box drawings light vertical
        "\u2514": "+",  # Box drawings light up and horizontal
        "\u251c": "+",  # Box drawings light vertical and horizontal
        "\u2524": "+",  # Box drawings light up and vertical
        "\u2534": "+",  # Box drawings light down and horizontal
        "\u253c": "+",  # Box drawings light vertical and horizontal
        "\u2550": "=",  # Box drawings double horizontal
        "\u2551": "||", # Box drawings double vertical
        "\u2588": "##", # Full block
        "\u2591": "::", # Light shade
        "\u2592": "::", # Medium shade
        "\u2593": "##", # Dark shade
    }
    for src, dst in replacements.items():
        text = text.replace(src, dst)
    
    # Normalize and filter any remaining non-ASCII
    text = unicodedata.normalize("NFKC", text)
    
    # Replace any remaining non-ASCII with space or remove
    result = []
    for char in text:
        if ord(char) < 128:
            result.append(char)
        elif char.isspace():
            result.append(' ')
        # Otherwise skip the character
    return ''.join(result)


def parse_inline_style(style_attr: str) -> dict:
    """Parse inline style attribute into a dict of CSS properties.
    
    Example: 'font-weight: bold; color: #ff0000; text-align: center'
    Returns: {'font_weight': 'bold', 'color': '#ff0000', 'text-align': 'center'}
    
    Args:
        style_attr: CSS style string from HTML style="" attribute.
    
    Returns:
        Dictionary with CSS properties as keys (with - replaced by _).
    
    Note:
        Does not validate CSS syntax - malformed values are passed through.
        Empty or invalid style strings return empty dict.
    """
    if not style_attr:
        return {}
    styles = {}
    try:
        for prop in style_attr.split(';'):
            if ':' in prop:
                key, value = prop.split(':', 1)
                key = key.strip().replace('-', '_')
                value = value.strip()
                if key and value:
                    styles[key] = value
    except Exception:
        # If parsing fails for any reason, return empty dict
        # This prevents malformed CSS from crashing the reader
        pass
    return styles


def hex_to_16_color(hex_color: str) -> Optional[int]:
    """Convert a hex color to the nearest 16-color ANSI palette.
    
    Maps hex colors (e.g., '#ff0000', 'rgb(255,0,0)') to curses.COLOR_* constants.
    This is a helper for future color rendering - currently not used in rendering
    as dynamic color pair management is not yet implemented.
    
    Args:
        hex_color: Color in hex format (#rrggbb, #rgb, rrggbb) or rgb(r,g,b).
    
    Returns:
        ANSI color index (0-15) for the closest matching color, or None if
        the color cannot be parsed.
    
    Note:
        Uses Euclidean distance in RGB space to find the closest color.
        The 16-color palette includes standard colors plus bright variants.
    """
    hex_color = hex_color.strip()
    
    # Handle rgb() format
    rgb_match = re.search(r'rgb\s*\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)', hex_color)
    if rgb_match:
        r, g, b = int(rgb_match.group(1)), int(rgb_match.group(2)), int(rgb_match.group(3))
    else:
        # Handle hex format (#rrggbb, #rgb, or rrggbb)
        hex_color = hex_color.lstrip('#')
        if len(hex_color) == 3:
            hex_color = ''.join(c*2 for c in hex_color)
        if len(hex_color) == 6:
            try:
                r, g, b = int(hex_color[0:2], 16), int(hex_color[2:4], 16), int(hex_color[4:6], 16)
            except ValueError:
                return None
        else:
            return None
    
    # 16-color ANSI palette
    ansi_colors = [
        (0, 0, 0),       # 0: black
        (170, 0, 0),     # 1: red
        (0, 170, 0),     # 2: green
        (170, 170, 0),   # 3: yellow
        (0, 0, 170),     # 4: blue
        (170, 0, 170),   # 5: magenta
        (0, 170, 170),   # 6: cyan
        (170, 170, 170), # 7: white
        (85, 85, 85),    # 8: bright black (gray)
        (255, 85, 85),   # 9: bright red
        (85, 255, 85),   # 10: bright green
        (255, 255, 85),  # 11: bright yellow
        (85, 85, 255),   # 12: bright blue
        (255, 85, 255),  # 13: bright magenta
        (85, 255, 255),  # 14: bright cyan
        (255, 255, 255), # 15: bright white
    ]
    
    # Find closest color by Euclidean distance
    min_dist = float('inf')
    closest_idx = 0
    
    for idx, (ar, ag, ab) in enumerate(ansi_colors):
        dist = (r - ar) ** 2 + (g - ag) ** 2 + (b - ab) ** 2
        if dist < min_dist:
            min_dist = dist
            closest_idx = idx
    
    return closest_idx


class EpubTextExtractor(HTMLParser):
    """HTML parser that extracts text with inline CSS styling information."""
    BLOCK_TAGS = {
        "p", "div", "section", "article", "aside", "blockquote", "pre",
        "ul", "ol", "li", "dl", "dt", "dd", "table", "tr", "td", "th",
        "h1", "h2", "h3", "h4", "h5", "h6", "br", "hr"
    }

    def __init__(self, use_css: bool = True):
        super().__init__(convert_charrefs=True)
        self.use_css = use_css
        self.segments: List[StyledSegment] = []
        self.pre_depth = 0
        self.list_depth = 0
        self.skip_depth = 0
        # Style stack for CSS inheritance: each entry is a dict of styles
        # added by a particular tag. The current style is the merge of all
        # styles in the stack. Stack discipline: push on starttag with styles,
        # pop on corresponding endtag.
        self.style_stack: List[dict] = [{}]

    def _get_current_styles(self) -> dict:
        """Get current inherited styles by merging the style stack."""
        merged = {}
        for style_dict in self.style_stack:
            merged.update(style_dict)
        return merged

    def handle_starttag(self, tag, attrs):
        attrs_dict = dict(attrs)
        if tag in {"script", "style"}:
            self.skip_depth += 1
            return
        if self.skip_depth:
            return

        # Extract inline styles from this tag
        tag_styles = {}
        if self.use_css and 'style' in attrs_dict:
            tag_styles = parse_inline_style(attrs_dict['style'])

        # Add semantic styles from tag type (e.g., <b> means bold)
        if self.use_css:
            if tag in {"b", "strong"}:
                tag_styles['font_weight'] = 'bold'
            elif tag in {"i", "em"}:
                tag_styles['font_style'] = 'italic'
            elif tag == "u":
                tag_styles['text_decoration'] = 'underline'
            elif tag in {"s", "strike", "del"}:
                tag_styles['text_decoration'] = 'line-through'

        # Push styles onto stack if there are any
        if tag_styles:
            self.style_stack.append(tag_styles)

        if tag == "pre":
            self.pre_depth += 1
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag in {"ul", "ol"}:
            self.list_depth += 1
            self.segments.append(StyledSegment("\n", self._get_current_styles().copy()))
        elif tag == "li":
            indent = "  " * max(0, self.list_depth - 1)
            self.segments.append(StyledSegment(indent + "- ", self._get_current_styles().copy()))
        elif tag in {"h1", "h2", "h3", "h4", "h5", "h6"}:
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag == "blockquote":
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag == "img":
            alt = (attrs_dict.get("alt") or "").strip()
            if alt:
                self.segments.append(StyledSegment("[Image: %s]" % alt, self._get_current_styles().copy()))
        elif tag == "br":
            self.segments.append(StyledSegment("\n", self._get_current_styles().copy()))
        elif tag in {"p", "div", "section", "article"}:
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag in self.BLOCK_TAGS:
            self.segments.append(StyledSegment("\n", self._get_current_styles().copy()))

    def handle_endtag(self, tag):
        if tag in {"script", "style"} and self.skip_depth > 0:
            self.skip_depth -= 1
            return
        if self.skip_depth:
            return

        # Pop styles from stack if this tag could have added styles
        if (tag in {"b", "strong", "i", "em", "u", "s", "strike", "del", "span",
                    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6"} and
            len(self.style_stack) > 1):
            self.style_stack.pop()

        if tag == "pre" and self.pre_depth > 0:
            self.pre_depth -= 1
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag in {"ul", "ol"} and self.list_depth > 0:
            self.list_depth -= 1
            self.segments.append(StyledSegment("\n", self._get_current_styles().copy()))
        elif tag in {"h1", "h2", "h3", "h4", "h5", "h6"}:
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag == "blockquote":
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag in {"p", "div", "section", "article"}:
            self.segments.append(StyledSegment("\n\n", self._get_current_styles().copy()))
        elif tag in self.BLOCK_TAGS:
            self.segments.append(StyledSegment("\n", self._get_current_styles().copy()))

    def handle_data(self, data):
        if not data or self.skip_depth:
            return

        # Clean the text based on context
        if self.pre_depth:
            clean_text = ascii_sanitize(data)
        else:
            clean_text = re.sub(r"\s+", " ", data)
            clean_text = ascii_sanitize(clean_text)

        if not clean_text.strip():
            return

        # Create segment with current inherited styles
        segment = StyledSegment(clean_text, self._get_current_styles().copy())
        self.segments.append(segment)

    def _merge_adjacent_segments(self, segments: List[StyledSegment]) -> List[StyledSegment]:
        """Merge adjacent segments with identical styles for efficiency.
        
        Does NOT merge across paragraph breaks (whitespace-only segments).
        This preserves paragraph structure while reducing the number of
        segments we need to process during rendering.
        
        Args:
            segments: List of styled segments to merge.
        
        Returns:
            List of segments with identical adjacent segments combined.
        """
        if not segments:
            return segments

        merged = [segments[0]]
        for seg in segments[1:]:
            last = merged[-1]
            # Don't merge across paragraph breaks (whitespace-only segments)
            if last.text.strip() == '' or seg.text.strip() == '':
                merged.append(seg)
            elif last.styles == seg.styles:
                # Merge: concatenate text, keep same styles
                merged[-1] = StyledSegment(last.text + seg.text, last.styles)
            else:
                merged.append(seg)
        return merged

    def get_segments(self) -> List[StyledSegment]:
        """Return styled text segments, merged where possible.
        
        Preserves paragraph breaks (segments that are only whitespace).
        """
        # Filter out truly empty segments (zero length), but keep whitespace-only ones
        # as they serve as paragraph breaks
        non_empty = [s for s in self.segments if s.text]
        return self._merge_adjacent_segments(non_empty)

    def get_text(self) -> str:
        """Return plain text (for backward compatibility with search, etc.)."""
        segments = self.get_segments()
        return ''.join(seg.text for seg in segments)


class EpubBook:
    def __init__(self, path: str, use_css: bool = True):
        self.path = os.path.abspath(path)
        self.use_css = use_css
        self.zf = zipfile.ZipFile(path)
        self.title = os.path.basename(path)
        self.author = "Unknown"
        self.rootfile = self._find_rootfile()
        self.manifest: Dict[str, Dict[str, str]] = {}
        self.spine: List[str] = []
        self.spine_hrefs: List[str] = []
        self.chapter_titles: List[str] = []
        self.chapters: List[str] = []
        self.chapter_segments: List[List[StyledSegment]] = []  # Styled segments per chapter
        self.toc: List[TocEntry] = []
        self._parse_package()
        self._load_toc()       # Load TOC FIRST
        self._load_chapters()  # Then load chapters (can use TOC titles)
        self._fill_missing_toc_entries()

    def _read_xml(self, member: str) -> ET.Element:
        data = self.zf.read(member)
        return ET.fromstring(data)

    def _find_rootfile(self) -> str:
        root = ET.fromstring(self.zf.read("META-INF/container.xml"))
        for elem in root.iter():
            if strip_ns(elem.tag) == "rootfile":
                full_path = elem.attrib.get("full-path")
                if full_path:
                    return full_path
        raise ValueError("Could not locate OPF rootfile")

    def _parse_package(self):
        root = self._read_xml(self.rootfile)
        metadata = manifest = spine = None
        for child in root:
            name = strip_ns(child.tag)
            if name == "metadata":
                metadata = child
            elif name == "manifest":
                manifest = child
            elif name == "spine":
                spine = child

        if metadata is not None:
            for elem in metadata.iter():
                name = strip_ns(elem.tag)
                if name == "title" and elem.text:
                    self.title = ascii_sanitize(elem.text.strip())
                elif name == "creator" and elem.text:
                    self.author = ascii_sanitize(elem.text.strip())

        if manifest is None or spine is None:
            raise ValueError("Malformed EPUB package")

        for item in manifest:
            if strip_ns(item.tag) != "item":
                continue
            item_id = item.attrib.get("id")
            if not item_id:
                continue
            self.manifest[item_id] = {
                "href": norm_href(self.rootfile, item.attrib.get("href", "")),
                "media_type": item.attrib.get("media-type", ""),
                "properties": item.attrib.get("properties", ""),
            }

        for itemref in spine:
            if strip_ns(itemref.tag) != "itemref":
                continue
            idref = itemref.attrib.get("idref")
            if idref and idref in self.manifest:
                self.spine.append(idref)
                self.spine_hrefs.append(self.manifest[idref]["href"])

    def _load_chapters(self):
        for idx, idref in enumerate(self.spine):
            href = self.manifest[idref]["href"]
            try:
                raw = self.zf.read(href).decode("utf-8", errors="replace")
            except KeyError:
                self.chapters.append("[Missing chapter content]\n")
                self.chapter_segments.append([])
                self.chapter_titles.append("Chapter %d" % (idx + 1))
                continue
            
            # Use the new styled segment extractor
            extractor = EpubTextExtractor(use_css=self.use_css)
            extractor.feed(raw)
            
            # Get styled segments
            segments = extractor.get_segments()
            
            # Convert to plain text for backward compatibility
            text = ''.join(seg.text for seg in segments)
            
            # Get title from TOC if available, otherwise use generic chapter number
            toc_title = None
            for toc_entry in self.toc:
                if toc_entry.spine_index == idx:
                    toc_title = toc_entry.title
                    break
            
            # If no TOC entry for this spine index, use the href to find a matching TOC entry
            if not toc_title:
                for toc_entry in self.toc:
                    # Compare hrefs (normalize both)
                    toc_href = norm_href(self.rootfile, toc_entry.href)
                    if os.path.normpath(toc_href) == os.path.normpath(href):
                        toc_title = toc_entry.title
                        break
            
            if toc_title:
                title = toc_title
            else:
                title = "Chapter %d" % (idx + 1)
            
            # Remove duplicate chapter heading from body (if it appears at the start)
            # This handles cases where the TOC title is repeated in the chapter content
            if toc_title:
                title_stripped = toc_title.strip()
                text_stripped = text.strip()
                # If text starts with the title (possibly with extra newlines), remove it
                pattern = r'^' + re.escape(title_stripped) + r'\s*\n*\s*' + re.escape(title_stripped) + r'\s*\n+'
                text = re.sub(pattern, title_stripped + '\n\n', text, flags=re.I)
            
            # Normalize paragraph breaks - reduce multiple blank lines to just one
            text = re.sub(r'\n{3,}', '\n\n', text)
            
            if not text:
                text = "[This chapter contains no readable text.]"
            
            self.chapters.append(text + "\n")
            self.chapter_segments.append(segments)
            self.chapter_titles.append(title)

    def _guess_title(self, raw_html: str, idx: int) -> str:
        m = re.search(r"<title[^>]*>(.*?)</title>", raw_html, flags=re.I | re.S)
        if m:
            title = re.sub(r"\s+", " ", html.unescape(m.group(1))).strip()
            title = ascii_sanitize(title)
            if title:
                return title
        for tag in ["h1", "h2", "h3"]:
            m = re.search(r"<%s[^>]*>(.*?)</%s>" % (tag, tag), raw_html, flags=re.I | re.S)
            if m:
                title = re.sub(r"<[^>]+>", "", m.group(1))
                title = re.sub(r"\s+", " ", html.unescape(title)).strip()
                title = ascii_sanitize(title)
                if title:
                    return title
        return "Chapter %d" % (idx + 1)

    def _load_toc(self):
        nav_item = None
        ncx_item = None
        for item in self.manifest.values():
            props = item.get("properties", "")
            mt = item.get("media_type", "")
            if "nav" in props.split():
                nav_item = item
                break
            if mt == "application/x-dtbncx+xml":
                ncx_item = item
        if nav_item:
            self.toc = self._parse_nav_toc(nav_item["href"])
        elif ncx_item:
            self.toc = self._parse_ncx_toc(ncx_item["href"])
        else:
            self.toc = []

    def _parse_nav_toc(self, href: str) -> List[TocEntry]:
        try:
            raw = self.zf.read(href).decode("utf-8", errors="replace")
        except KeyError:
            return []
        links = re.findall(r'<a[^>]+href=["\']([^"\']+)["\'][^>]*>(.*?)</a>', raw, flags=re.I | re.S)
        entries = []
        seen = set()
        for target, label in links:
            title = re.sub(r"<[^>]+>", "", label)
            title = re.sub(r"\s+", " ", html.unescape(title)).strip()
            title = ascii_sanitize(title)
            if not title:
                continue
            chapter_href = norm_href(href, target)
            spine_index = self._spine_index_for_href(chapter_href)
            if spine_index is None:
                continue
            key = (title, spine_index)
            if key in seen:
                continue
            seen.add(key)
            entries.append(TocEntry(title, chapter_href, spine_index))
        return entries

    def _parse_ncx_toc(self, href: str) -> List[TocEntry]:
        try:
            root = self._read_xml(href)
        except Exception:
            return []
        entries = []
        seen = set()
        for elem in root.iter():
            if strip_ns(elem.tag) != "navPoint":
                continue
            label = ""
            src = ""
            for sub in elem.iter():
                name = strip_ns(sub.tag)
                if name == "text" and sub.text and not label:
                    label = ascii_sanitize(sub.text.strip())
                elif name == "content" and not src:
                    src = sub.attrib.get("src", "")
            if not label or not src:
                continue
            chapter_href = norm_href(href, src)
            spine_index = self._spine_index_for_href(chapter_href)
            if spine_index is None:
                continue
            key = (label, spine_index)
            if key in seen:
                continue
            seen.add(key)
            entries.append(TocEntry(label, chapter_href, spine_index))
        return entries

    def _spine_index_for_href(self, href: str) -> Optional[int]:
        clean = href.split("#", 1)[0]
        for idx, spine_href in enumerate(self.spine_hrefs):
            if os.path.normpath(spine_href) == os.path.normpath(clean):
                return idx
        return None

    def _fill_missing_toc_entries(self):
        if not self.toc:
            self.toc = [
                TocEntry(title=self.chapter_titles[i], href=self.spine_hrefs[i], spine_index=i)
                for i in range(len(self.spine_hrefs))
            ]


class StateStore:
    def __init__(self):
        os.makedirs(CONFIG_DIR, exist_ok=True)
        self.data = self._load()

    def _load(self) -> Dict[str, dict]:
        if not os.path.exists(STATE_FILE):
            return {}
        try:
            with open(STATE_FILE, "r", encoding="utf-8") as fh:
                return json.load(fh)
        except Exception:
            return {}

    def save(self):
        tmp = STATE_FILE + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(self.data, fh, indent=2, sort_keys=True)
        os.replace(tmp, STATE_FILE)

    @staticmethod
    def book_key(path: str) -> str:
        return hashlib.sha1(os.path.abspath(path).encode("utf-8")).hexdigest()

    def get_state(self, path: str) -> BookState:
        entry = self.data.get(self.book_key(path), {})
        return BookState(
            chapter_index=int(entry.get("chapter_index", 0)),
            page_index=int(entry.get("page_index", 0)),
        )

    def set_state(self, path: str, state: BookState):
        abs_path = os.path.abspath(path)
        key = self.book_key(abs_path)
        entry = self.data.setdefault(key, {})
        entry["path"] = abs_path
        entry["chapter_index"] = int(state.chapter_index)
        entry["page_index"] = int(state.page_index)
        global_entry = self.data.setdefault("_global", {})
        global_entry["last_book_path"] = abs_path

    def set_bookmark(self, path: str, chapter_index: int, page_index: int):
        abs_path = os.path.abspath(path)
        key = self.book_key(abs_path)
        entry = self.data.setdefault(key, {})
        entry["path"] = abs_path
        entry["bookmark"] = {
            "chapter_index": int(chapter_index),
            "page_index": int(page_index),
        }
        global_entry = self.data.setdefault("_global", {})
        global_entry["last_book_path"] = abs_path

    def get_bookmark(self, path: str) -> Optional[BookState]:
        entry = self.data.get(self.book_key(path), {})
        bm = entry.get("bookmark")
        if not bm:
            return None
        return BookState(
            chapter_index=int(bm.get("chapter_index", 0)),
            page_index=int(bm.get("page_index", 0)),
        )

    def get_theme(self) -> str:
        theme = self.data.get("_global", {}).get("theme", "dark")
        return theme if theme in {"dark", "light"} else "dark"

    def set_theme(self, theme: str):
        if theme not in {"dark", "light"}:
            return
        global_entry = self.data.setdefault("_global", {})
        global_entry["theme"] = theme

    def get_show_header(self) -> bool:
        return bool(self.data.get("_global", {}).get("show_header", True))

    def set_show_header(self, show_header: bool):
        global_entry = self.data.setdefault("_global", {})
        global_entry["show_header"] = bool(show_header)

    def get_last_book_path(self) -> Optional[str]:
        path = self.data.get("_global", {}).get("last_book_path")
        if isinstance(path, str) and path:
            return path
        return None


class FilePicker:
    def __init__(self, stdscr, start_path: Optional[str] = None):
        self.stdscr = stdscr
        self.current_dir = os.path.abspath(start_path or os.getcwd())
        if os.path.isfile(self.current_dir):
            self.current_dir = os.path.dirname(self.current_dir)
        self.selected = 0
        self.entries: List[Tuple[str, str, bool]] = []
        self.status = ""
        self.filter_text = ""
        self.filtered_entries: List[Tuple[str, str, bool]] = []
        self.in_search_mode = False
        self.refresh_entries()

    def _normalize_title(self, title: str) -> str:
        """Normalize title for jumping: remove leading articles, strip extension."""
        # Remove .epub extension
        base = os.path.splitext(title)[0]
        # Remove leading articles (case-insensitive)
        articles = ["the ", "a ", "an "]
        for article in articles:
            if base.lower().startswith(article):
                base = base[len(article):].strip()
                break
        return base.strip()

    def jump_to_letter(self, letter: str):
        """Jump to the first entry starting with the given letter (case-insensitive)."""
        letter = letter.lower()
        normalized_entries = [(self._normalize_title(label), label, full, is_dir)
                              for label, full, is_dir in self.entries]
        
        # Find first entry starting with the letter
        for idx, (normalized, label, full, is_dir) in enumerate(normalized_entries):
            if normalized and normalized[0].lower() == letter:
                self.selected = idx
                self.status = "Jumped to: %s" % label
                break
        else:
            self.status = "No entries starting with '%s'" % letter.upper()

    def refresh_entries(self):
        entries: List[Tuple[str, str, bool]] = []
        parent = os.path.dirname(self.current_dir)
        if parent and parent != self.current_dir:
            entries.append(("..", parent, True))
        try:
            names = os.listdir(self.current_dir)
        except OSError as exc:
            self.status = "Cannot open directory: %s" % exc
            names = []
        dirs = []
        files = []
        for name in names:
            full = os.path.join(self.current_dir, name)
            if os.path.isdir(full):
                dirs.append((name + "/", full, True))
            elif name.lower().endswith(".epub"):
                files.append((name, full, False))
        dirs.sort(key=lambda x: x[0].lower())
        files.sort(key=lambda x: x[0].lower())
        entries.extend(dirs)
        entries.extend(files)
        self.entries = entries
        if self.selected >= len(self.entries):
            self.selected = max(0, len(self.entries) - 1)
        self.apply_filter()

    def apply_filter(self):
        """Apply the current filter text to the entries list."""
        if not self.filter_text:
            self.filtered_entries = self.entries
            return
        
        filter_lower = self.filter_text.lower()
        self.filtered_entries = [
            entry for entry in self.entries
            if filter_lower in entry[0].lower()
        ]
        
        # Adjust selection if needed
        if self.filtered_entries and self.selected >= len(self.filtered_entries):
            self.selected = max(0, len(self.filtered_entries) - 1)

    def _normalize_title(self, title: str) -> str:
        """Normalize title for jumping: remove leading articles, strip extension."""
        # Remove .epub extension
        base = os.path.splitext(title)[0]
        # Remove leading articles (case-insensitive)
        articles = ["the ", "a ", "an "]
        for article in articles:
            if base.lower().startswith(article):
                base = base[len(article):].strip()
                break
        return base.strip()

    def jump_to_letter(self, letter: str):
        """Jump to the first entry starting with the given letter (case-insensitive)."""
        letter = letter.lower()
        normalized_entries = [(self._normalize_title(label), label, full, is_dir)
                              for label, full, is_dir in self.entries]
        
        # Find first entry starting with the letter
        for idx, (normalized, label, full, is_dir) in enumerate(normalized_entries):
            if normalized and normalized[0].lower() == letter:
                self.selected = idx
                self.status = "Jumped to: %s" % label
                break
        else:
            self.status = "No entries starting with '%s'" % letter.upper()

    def refresh_entries(self):
        entries: List[Tuple[str, str, bool]] = []
        parent = os.path.dirname(self.current_dir)
        if parent and parent != self.current_dir:
            entries.append(("..", parent, True))
        try:
            names = os.listdir(self.current_dir)
        except OSError as exc:
            self.status = "Cannot open directory: %s" % exc
            names = []
        dirs = []
        files = []
        for name in names:
            full = os.path.join(self.current_dir, name)
            if os.path.isdir(full):
                dirs.append((name + "/", full, True))
            elif name.lower().endswith(".epub"):
                files.append((name, full, False))
        dirs.sort(key=lambda x: x[0].lower())
        files.sort(key=lambda x: x[0].lower())
        entries.extend(dirs)
        entries.extend(files)
        self.entries = entries
        if self.selected >= len(self.entries):
            self.selected = max(0, len(self.entries) - 1)
        self.apply_filter()

    def apply_filter(self):
        """Apply the current filter text to the entries list."""
        if not self.filter_text:
            self.filtered_entries = self.entries
            return
        
        filter_lower = self.filter_text.lower()
        self.filtered_entries = [
            entry for entry in self.entries
            if filter_lower in entry[0].lower()
        ]
        
        # Adjust selection if needed
        if self.filtered_entries and self.selected >= len(self.filtered_entries):
            self.selected = max(0, len(self.filtered_entries) - 1)

    def run(self) -> Optional[str]:
        waiting_for_letter = False
        waiting_letter_buffer = ""
        search_input = ""
        
        while True:
            self.draw()
            
            # Handle waiting for letter mode
            if waiting_for_letter:
                ch = self.stdscr.getch()
                if ch == 27:  # Esc - cancel
                    waiting_for_letter = False
                    self.status = "Jump cancelled"
                elif ch == 13 or ch == 10:  # Enter - use buffered letter
                    if waiting_letter_buffer:
                        self.jump_to_letter(waiting_letter_buffer)
                    waiting_for_letter = False
                    waiting_letter_buffer = ""
                elif ch == 127 or ch == curses.KEY_BACKSPACE or ch == 8:  # Backspace - clear buffer
                    waiting_letter_buffer = ""
                    self.status = "Jump: "
                elif 32 <= ch <= 126:  # Printable character
                    waiting_letter_buffer = chr(ch)
                    self.status = "Jump to: %s" % waiting_letter_buffer.upper()
                continue
            
            # Handle search mode
            if self.in_search_mode:
                ch = self.stdscr.getch()
                if ch == 27:  # Esc - cancel search
                    self.in_search_mode = False
                    search_input = ""
                    self.filter_text = ""
                    self.apply_filter()
                    self.status = "Search cancelled"
                elif ch == 10 or ch == 13:  # Enter - exit search mode
                    self.in_search_mode = False
                    search_input = ""
                    self.status = "Filter: %s" % self.filter_text if self.filter_text else ""
                elif ch == 127 or ch == curses.KEY_BACKSPACE or ch == 8:  # Backspace
                    search_input = search_input[:-1]
                    self.filter_text = search_input
                    self.apply_filter()
                elif 32 <= ch <= 126:  # Printable character
                    search_input += chr(ch)
                    self.filter_text = search_input
                    self.selected = 0  # Reset selection when filtering
                    self.apply_filter()
                continue
            
            ch = self.stdscr.getch()
            if ch in (27, ord("q"), ord("Q")):
                return None
            
            # Jump-to-letter mode: 'j' followed by letter
            if ch == ord("j") and not waiting_for_letter:
                waiting_for_letter = True
                waiting_letter_buffer = ""
                self.status = "Jump to: "
                continue
            
            # Search mode: 's' to start typing filter
            if ch == ord("s") and not self.in_search_mode:
                self.in_search_mode = True
                search_input = ""
                self.filter_text = ""
                self.selected = 0
                self.status = "Search: "
                continue
            
            if ch in (curses.KEY_UP, ord("k")):
                if self.selected > 0:
                    self.selected -= 1
            elif ch in (curses.KEY_DOWN, ord("j")):
                if self.selected + 1 < len(self.filtered_entries):
                    self.selected += 1
            elif ch in (curses.KEY_NPAGE, 338):
                self.selected = min(len(self.filtered_entries) - 1, self.selected + max(1, self.body_height()))
            elif ch in (curses.KEY_PPAGE, 339):
                self.selected = max(0, self.selected - max(1, self.body_height()))
            elif ch == curses.KEY_LEFT:
                parent = os.path.dirname(self.current_dir)
                if parent and parent != self.current_dir:
                    self.current_dir = parent
                    self.selected = 0
                    self.refresh_entries()
            elif ch in (10, 13, curses.KEY_ENTER, curses.KEY_RIGHT):
                if not self.filtered_entries:
                    continue
                _label, full, is_dir = self.filtered_entries[self.selected]
                if is_dir:
                    self.current_dir = full
                    self.selected = 0
                    self.refresh_entries()
                else:
                    return full

    def body_height(self) -> int:
        h, _ = self.stdscr.getmaxyx()
        return max(1, h - 3)

    def draw(self):
        self.stdscr.erase()
        h, w = self.stdscr.getmaxyx()
        
        # Top line: title + search/filter bar
        search_bar = ""
        if self.in_search_mode:
            search_bar = "Search: " + self.filter_text + "_"
        elif self.filter_text:
            search_bar = "Filter: " + self.filter_text
        
        jump_hint = "j:jump" if not self.status and not search_bar else ""
        title = "Open EPUB - Enter/Right: open, Left: up, s:search, j:jump, q/Esc: cancel %s" % jump_hint
        
        try:
            self.stdscr.addnstr(0, 0, title.ljust(max(0, w - 1)), w - 1, curses.A_REVERSE)
        except curses.error:
            pass
        
        # Second line: search/filter bar or current directory
        try:
            if search_bar:
                self.stdscr.addnstr(1, 0, search_bar[:w-1], w - 1)
            else:
                self.stdscr.addnstr(1, 0, self.current_dir, w - 1)
        except curses.error:
            pass

        body_top = 2
        body_h = max(1, h - 4 if search_bar else h - 3)
        start = max(0, self.selected - body_h // 2)
        end = min(len(self.filtered_entries), start + body_h)
        start = max(0, end - body_h)

        row = body_top
        for idx in range(start, end):
            actual_idx = start + idx - start
            label, _full, _is_dir = self.filtered_entries[actual_idx]
            attr = curses.A_REVERSE if idx == self.selected else curses.A_NORMAL
            try:
                self.stdscr.addnstr(row, 0, label, w - 1, attr)
            except curses.error:
                pass
            row += 1

        footer = self.status or "Directories and .epub files"
        try:
            self.stdscr.addnstr(h - 1, 0, footer.ljust(max(0, w - 1)), w - 1, curses.A_REVERSE)
        except curses.error:
            pass
        self.stdscr.refresh()
        self.status = ""


class ReaderUI:
    def __init__(self, stdscr, book: EpubBook, store: StateStore):
        self.stdscr = stdscr
        self.store = store
        self.book = book
        self.chapter_index = 0
        self.page_index = 0
        self.status_message = ""
        self.total_pages: int = 0
        self.pages_cache: Dict[Tuple[int, int, int, str], List[List[str]]] = {}
        self.pages_attrs_cache: Dict[int, List[Tuple[List[str], List[int]]]] = {}  # Cache pages with attrs by chapter
        self.running = True
        self.theme = self.store.get_theme()
        self.show_header = self.store.get_show_header()
        self.has_colors = False
        self.header_attr = curses.A_REVERSE
        self.footer_attr = curses.A_REVERSE
        self.selected_attr = curses.A_REVERSE
        self.heading_attr = curses.A_REVERSE  # Can be changed to A_BOLD, A_ITALIC, etc.
        self.load_book(book, use_saved_position=True)

    def load_book(self, book: EpubBook, use_saved_position: bool = True):
        self.book = book
        if use_saved_position:
            state = self.store.get_state(book.path)
            self.chapter_index = max(0, min(state.chapter_index, len(book.chapters) - 1))
            self.page_index = max(0, state.page_index)
        else:
            self.chapter_index = 0
            self.page_index = 0
        self.pages_cache.clear()
        self.pages_attrs_cache.clear()  # Clear heading cache too
        self._ensure_page_in_range()
        self.total_pages = self._compute_total_pages()  # Compute once
        self.store.set_state(self.book.path, BookState(self.chapter_index, self.page_index))
        self.store.save()
        self.show_info_popup("Loaded", "Loaded: %s" % self.book.title)

    def _compute_total_pages(self) -> int:
        """Compute total pages in the book (cached, doesn't change with screen resize)."""
        h, w = self.stdscr.getmaxyx()
        reserved = 2 if self.show_header else 1
        body_h = max(3, h - reserved)
        
        total = 0
        for chapter_text in self.book.chapters:
            lines = self._wrap_text(chapter_text, max(20, w - 1))
            chapter_pages = max(1, (len(lines) + body_h - 1) // body_h)
            total += chapter_pages
        
        return total

    def _get_pages_count(self, chapter_index: int) -> int:
        """Get number of pages in a chapter."""
        h, w = self.stdscr.getmaxyx()
        reserved = 2 if self.show_header else 1
        body_h = max(3, h - reserved)
        body_w = max(20, w - 1)
        
        lines = self._wrap_text(self.book.chapters[chapter_index], body_w)
        return max(1, (len(lines) + body_h - 1) // body_h)

    def setup_colors(self):
        self.has_colors = False
        self.header_attr = curses.A_REVERSE
        self.footer_attr = curses.A_REVERSE
        self.selected_attr = curses.A_REVERSE
        self.heading_attr = curses.A_REVERSE  # Can be changed to A_BOLD, A_ITALIC, etc.
        if not curses.has_colors():
            return
        try:
            curses.start_color()
            curses.use_default_colors()
            curses.init_pair(1, curses.COLOR_WHITE, curses.COLOR_BLACK)
            curses.init_pair(2, curses.COLOR_BLACK, curses.COLOR_WHITE)
            curses.init_pair(3, curses.COLOR_YELLOW, -1)  # Titles (yellow on default)
            # Popup colors
            curses.init_pair(6, curses.COLOR_WHITE, curses.COLOR_BLUE)  # Info popup (white on blue)
            curses.init_pair(7, curses.COLOR_WHITE, curses.COLOR_RED)   # Error popup (white on red)
            self.has_colors = True
        except curses.error:
            self.has_colors = False
        self.apply_theme()

    def apply_theme(self):
        if not self.has_colors:
            self.header_attr = curses.A_REVERSE
            self.footer_attr = curses.A_REVERSE
            self.selected_attr = curses.A_REVERSE
            self.heading_attr = curses.A_REVERSE
            return
        attr = curses.color_pair(2) if self.theme == "light" else curses.color_pair(1)
        self.header_attr = attr
        self.footer_attr = attr
        self.selected_attr = attr
        # Use bold for headings instead of reverse (more readable)
        self.heading_attr = curses.A_BOLD

    def styles_to_curses_attr(self, css_styles: dict) -> int:
        """Convert CSS styles dict to curses attribute flags.
        
        Handles the following CSS properties:
        - font_weight: 'bold' or numeric values (700, 800, 900) → curses.A_BOLD
        - text_decoration: 'underline' → curses.A_UNDERLINE
        - text_decoration: 'line-through' → not rendered (terminal limitation)
        - color: parsed but not yet applied to rendering (future work)
        
        Args:
            css_styles: Dictionary of CSS property-value pairs.
                       Keys are underscores (e.g., 'font_weight' not 'font-weight').
        
        Returns:
            curses attribute flags (e.g., curses.A_BOLD | curses.A_UNDERLINE)
        """
        attr = curses.A_NORMAL

        # Font weight: bold
        if css_styles.get('font_weight') in ('bold', '700', '800', '900'):
            attr |= curses.A_BOLD

        # Text decoration: underline (line-through not well supported in terminals)
        if css_styles.get('text_decoration') == 'underline':
            attr |= curses.A_UNDERLINE

        # Font style: italic (not well supported, skip for now)
        # Some terminals support curses.A_ITALIC but it's non-standard

        # Color mapping (simplified: just use bright attributes for now)
        # Full implementation would need dynamic color pair allocation
        if 'color' in css_styles:
            color_idx = hex_to_16_color(css_styles['color'])
            if color_idx is not None and self.has_colors:
                # For now, skip color rendering - would need pair management
                pass

        return attr

    def toggle_theme(self):
        self.theme = "light" if self.theme == "dark" else "dark"
        self.store.set_theme(self.theme)
        self.store.save()
        self.pages_cache.clear()
        self.pages_attrs_cache.clear()
        self.apply_theme()
        self.show_info_popup("Theme", "Theme: %s" % self.theme)

    def toggle_header(self):
        self.show_header = not self.show_header
        self.store.set_show_header(self.show_header)
        self.pages_cache.clear()
        self.total_pages = self._compute_total_pages()  # Recompute when header changes
        self.store.save()
        self.show_info_popup("Header", "Header: %s" % ("on" if self.show_header else "off"))

    def toggle_heading_style(self):
        """Toggle between reverse and bold for headings."""
        if self.heading_attr == curses.A_REVERSE:
            self.heading_attr = curses.A_BOLD
            style = "bold"
        else:
            self.heading_attr = curses.A_REVERSE
            style = "reverse"
        self.pages_cache.clear()
        self.pages_attrs_cache.clear()
        self.show_info_popup("Heading", "Heading style: %s" % style)

    def get_overall_progress(self) -> Tuple[int, int]:
        """Return (current_page, total_pages) for the entire book."""
        # Count pages in all chapters before current one
        h, w = self.stdscr.getmaxyx()
        reserved = 2 if self.show_header else 1
        body_h = max(3, h - reserved)
        
        pages_before = 0
        for i in range(self.chapter_index):
            chapter_pages = self._get_pages_count(i)
            pages_before += chapter_pages
        
        # Pages in current chapter
        current_page = pages_before + self.page_index + 1
        
        return (current_page, self.total_pages)

    def show_info_popup(self, title: str, message: str, is_error: bool = False):
        """Show an info/error popup with styled border (white on blue/red), blocking until key press."""
        # Ensure colors are set up and screen is ready
        if not self.has_colors:
            self.setup_colors()
        
        # Set background color
        if self.has_colors:
            self.stdscr.bkgd(" ", curses.color_pair(2) if self.theme == "light" else curses.color_pair(1))
        
        self.stdscr.erase()
        h, w = self.stdscr.getmaxyx()
        
        # Calculate message dimensions (wrap to fit screen width minus padding)
        max_msg_width = min(w - 6, 60)
        lines = []
        for word in message.split():
            if not lines:
                lines.append(word)
            elif len(lines[-1]) + len(word) + 1 <= max_msg_width:
                lines[-1] += " " + word
            else:
                lines.append(word)
        
        popup_width = min(max(len(max(lines, default="")), len(title)) + 4, w - 2)
        popup_height = len(lines) + 4  # title + message + borders
        start_y = (h - popup_height) // 2
        start_x = (w - popup_width) // 2
        
        # Determine popup attribute based on type
        if is_error:
            popup_attr = curses.color_pair(7)  # White on red
        else:
            popup_attr = curses.color_pair(6)  # White on blue
        
        try:
            # Draw popup border (top)
            self.stdscr.attron(popup_attr)
            self.stdscr.addnstr(start_y, start_x, "+" + "-" * (popup_width - 2) + "+", popup_width)
            self.stdscr.attroff(popup_attr)
            
            # Draw sides (empty)
            for y in range(1, popup_height - 1):
                self.stdscr.attron(popup_attr)
                self.stdscr.addnstr(start_y + y, start_x, "|" + " " * (popup_width - 2) + "|", popup_width)
                self.stdscr.attroff(popup_attr)
            
            # Draw popup border (bottom)
            self.stdscr.attron(popup_attr)
            self.stdscr.addnstr(start_y + popup_height - 1, start_x, "+" + "-" * (popup_width - 2) + "+", popup_width)
            self.stdscr.attroff(popup_attr)
            
            # Draw title (centered, yellow bold like termgpt)
            title_line = " " + title + " "
            title_x = start_x + (popup_width - len(title_line)) // 2
            self.stdscr.attron(curses.color_pair(3) | curses.A_BOLD)
            self.stdscr.addnstr(start_y + 1, title_x, title_line[:popup_width - 2], popup_width - 2)
            self.stdscr.attroff(curses.color_pair(3) | curses.A_BOLD)
            
            # Draw message with popup background
            msg_start_y = start_y + 2
            for i, line in enumerate(lines):
                msg_width = popup_width - 4
                # Fill the line with background color first
                self.stdscr.attron(popup_attr)
                self.stdscr.addnstr(msg_start_y + i, start_x + 2, " " * msg_width, msg_width)
                # Then draw text on top
                self.stdscr.addnstr(msg_start_y + i, start_x + 2, line[:msg_width], msg_width)
                self.stdscr.attroff(popup_attr)
            
            # Draw "Press any key" at bottom
            prompt = "Press any key...".center(popup_width - 2)
            self.stdscr.attron(popup_attr)
            self.stdscr.addnstr(start_y + popup_height - 2, start_x + 1, prompt, popup_width - 2)
            self.stdscr.attroff(popup_attr)
        except curses.error:
            pass
        
        self.stdscr.refresh()
        # Wait for any key
        self.stdscr.getch()
        self.status_message = ""
    
    def run(self):
        curses.curs_set(0)
        self.stdscr.keypad(True)
        self.setup_colors()
        # Draw initial screen before waiting for input
        self._ensure_page_in_range()
        self.draw()
        while self.running:
            self._ensure_page_in_range()
            self.draw()
            ch = self.stdscr.getch()
            self.handle_key(ch)
        self._save_position()

    def _save_position(self):
        self.store.set_state(self.book.path, BookState(self.chapter_index, self.page_index))
        self.store.save()

    def _ensure_page_in_range(self):
        pages = self._get_pages(self.chapter_index)
        if not pages:
            self.page_index = 0
            return
        if self.page_index < 0:
            self.page_index = 0
        elif self.page_index >= len(pages):
            self.page_index = len(pages) - 1

    def _get_pages_count(self, chapter_index: int) -> int:
        """Get number of pages in a chapter."""
        h, w = self.stdscr.getmaxyx()
        reserved = 2 if self.show_header else 1
        body_h = max(3, h - reserved)
        body_w = max(20, w - 1)
        
        lines = self._wrap_text(self.book.chapters[chapter_index], body_w)
        return max(1, (len(lines) + body_h - 1) // body_h)

    def _wrap_text(self, text: str, width: int) -> List[str]:
        out: List[str] = []
        paragraphs = text.split("\n\n")
        for para in paragraphs:
            para = para.strip("\n")
            if not para.strip():
                out.append("")
                continue
            lines = para.splitlines()
            if len(lines) > 1 and any(line.startswith(("    ", "\t")) for line in lines):
                for line in lines:
                    wrapped = textwrap.wrap(
                        line,
                        width=max(10, width),
                        replace_whitespace=False,
                        drop_whitespace=False,
                        break_long_words=False,
                        break_on_hyphens=False,
                    ) or [line]
                    out.extend(wrapped)
            else:
                joined = " ".join(part.strip() for part in lines if part.strip())
                wrapped = textwrap.wrap(
                    joined,
                    width=max(10, width),
                    replace_whitespace=True,
                    drop_whitespace=True,
                    break_long_words=False,
                    break_on_hyphens=False,
                ) or [""]
                out.extend(wrapped)
            out.append("")
        if out and out[-1] == "":
            out.pop()
        return out

    def _get_styled_pages(self, chapter_index: int) -> List[List[Tuple[str, int]]]:
        """Get pages as list of (line_text, curses_attr) tuples.
        
        Wraps text properly and applies CSS styles based on character positions.
        """
        h, w = self.stdscr.getmaxyx()
        cache_key = (chapter_index, h, w)
        
        if cache_key in self.pages_attrs_cache:
            return self.pages_attrs_cache[cache_key]
        
        body_h = max(3, h - (2 if self.show_header else 1))
        body_w = max(20, w - 1)
        
        # Get the plain text chapter
        chapter_text = self.book.chapters[chapter_index]
        segments = self.book.chapter_segments[chapter_index]
        
        # Wrap using the proven method
        lines = self._wrap_text(chapter_text, body_w)
        
        # Build style map from segments: character position -> curses attribute
        if segments:
            style_ranges: List[Tuple[int, int, int]] = []
            char_pos = 0
            for seg in segments:
                attr = self.styles_to_curses_attr(seg.styles)
                style_ranges.append((char_pos, char_pos + len(seg.text), attr))
                char_pos += len(seg.text)
            
            # Apply styles to each line based on character position
            styled_lines = []
            char_offset = 0
            for line in lines:
                line_len = len(line)
                # Find the first style that applies to this line
                line_attr = curses.A_NORMAL
                for start, end, attr in style_ranges:
                    if start <= char_offset < end:
                        line_attr = attr
                        break
                styled_lines.append((line, line_attr))
                char_offset += line_len
        else:
            # Fallback to plain text if no segments
            styled_lines = [(line, curses.A_NORMAL) for line in lines]
        
        # Split into pages
        pages = []
        for i in range(0, len(styled_lines), body_h):
            page = styled_lines[i:i + body_h]
            pages.append(page)
        
        self.pages_attrs_cache[cache_key] = pages
        return pages

    def _get_pages_with_attrs(self, chapter_index: int) -> List[Tuple[List[str], List[int]]]:
        """Legacy method - now just wraps _get_styled_pages for compatibility."""
        styled_pages = self._get_styled_pages(chapter_index)
        return [([line for line, _ in page], [attr for _, attr in page]) for page in styled_pages]

    def _get_pages(self, chapter_index: int) -> List[List[str]]:
        """Legacy method for backward compatibility."""
        styled_pages = self._get_styled_pages(chapter_index)
        return [[line for line, _ in page] for page in styled_pages]

    def draw(self):
        self.stdscr.erase()
        if self.has_colors:
            self.stdscr.bkgd(" ", curses.color_pair(2) if self.theme == "light" else curses.color_pair(1))
        h, w = self.stdscr.getmaxyx()
        
        # Get styled pages directly
        styled_pages = self._get_styled_pages(self.chapter_index)
        page = styled_pages[self.page_index] if styled_pages else [("", curses.A_NORMAL)]
        
        body_start = 1 if self.show_header else 0
        if self.show_header:
            # Just show the book title in the header
            header = self.book.title
            try:
                self.stdscr.addnstr(0, 0, header, w - 1, self.header_attr)
            except curses.error:
                pass
        
        # Render each line with its computed attributes
        for idx, (line, attr) in enumerate(page, start=body_start):
            if idx >= h - 1:
                break
            try:
                self.stdscr.addnstr(idx, 0, line, w - 1, attr)
            except curses.error:
                pass
        
        # Get overall book progress
        current_page, total_pages = self.get_overall_progress()
        progress_pct = int((current_page / total_pages) * 100) if total_pages > 0 else 0
        
        # Compact footer for smaller screens
        footer = "C %d/%d P %d/%d %d%% | L/R page | U/D chap | t TOC | / find | Bmark | Open | Mode | Head | Quit |" % (
            self.chapter_index + 1,
            len(self.book.chapters),
            current_page,
            total_pages,
            progress_pct,
        )
        
        try:
            self.stdscr.addnstr(h - 1, 0, footer.ljust(max(0, w - 1)), w - 1, self.footer_attr)
        except curses.error:
            pass
        self.stdscr.refresh()
        self.status_message = ""

    def prompt(self, prompt_text: str) -> str:
        h, w = self.stdscr.getmaxyx()
        curses.curs_set(1)
        curses.echo()
        try:
            self.stdscr.move(h - 1, 0)
            self.stdscr.clrtoeol()
            self.stdscr.addnstr(h - 1, 0, prompt_text, w - 1, self.footer_attr)
            self.stdscr.refresh()
            raw = self.stdscr.getstr(h - 1, min(len(prompt_text), max(0, w - 1)), max(1, w - len(prompt_text) - 1))
            return raw.decode("utf-8", errors="replace")
        finally:
            curses.noecho()
            curses.curs_set(0)

    def handle_key(self, ch: int):
        if ch in (ord("q"), ord("Q")):
            self.running = False
        elif ch == curses.KEY_RIGHT:
            self.next_page()
        elif ch == curses.KEY_LEFT:
            self.prev_page()
        elif ch == curses.KEY_DOWN:
            self.next_chapter()
        elif ch == curses.KEY_UP:
            self.prev_chapter()
        elif ch in (curses.KEY_NPAGE, 338):
            self.next_page()
        elif ch in (curses.KEY_PPAGE, 339):
            self.prev_page()
        elif ch in (ord("n"), ord("N")):
            self.next_chapter()
        elif ch in (ord("p"), ord("P")):
            self.prev_chapter()
        elif ch in (ord("t"), ord("T")):
            self.open_toc()
        elif ch == ord("/"):
            self.search_prompt()
        elif ch in (ord("b"), ord("B")):
            self.set_bookmark()
        elif ch in (ord("o"), ord("O")):
            self.open_file_picker()
        elif ch in (ord("m"), ord("M")):
            self.toggle_theme()
        elif ch in (ord("h"), ord("H")):
            self.toggle_header()
        elif ch in (ord("g"), ord("G")):
            self.toggle_heading_style()
        elif ch == curses.KEY_RESIZE:
            self.pages_cache.clear()
            self._ensure_page_in_range()

    def next_page(self):
        pages = self._get_pages(self.chapter_index)
        if self.page_index + 1 < len(pages):
            self.page_index += 1
        elif self.chapter_index + 1 < len(self.book.chapters):
            self.chapter_index += 1
            self.page_index = 0
        else:
            self.show_info_popup("Info", "End of book")

    def prev_page(self):
        if self.page_index > 0:
            self.page_index -= 1
        elif self.chapter_index > 0:
            self.chapter_index -= 1
            prev_pages = self._get_pages(self.chapter_index)
            self.page_index = max(0, len(prev_pages) - 1)
        else:
            self.show_info_popup("Info", "Start of book")

    def next_chapter(self):
        if self.chapter_index + 1 < len(self.book.chapters):
            self.chapter_index += 1
            self.page_index = 0
        else:
            self.show_info_popup("Info", "Last chapter")

    def prev_chapter(self):
        if self.chapter_index > 0:
            self.chapter_index -= 1
            self.page_index = 0
        else:
            self.show_info_popup("Info", "First chapter")

    def open_toc(self):
        entries = self.book.toc
        idx = 0
        for i, entry in enumerate(entries):
            if entry.spine_index == self.chapter_index:
                idx = i
                break
        while True:
            self._draw_toc(entries, idx)
            ch = self.stdscr.getch()
            if ch in (ord("q"), 27):
                return
            if ch in (curses.KEY_DOWN, ord("j")):
                idx = min(len(entries) - 1, idx + 1)
            elif ch in (curses.KEY_UP, ord("k")):
                idx = max(0, idx - 1)
            elif ch in (10, 13, curses.KEY_ENTER, curses.KEY_RIGHT):
                self.chapter_index = entries[idx].spine_index
                self.page_index = 0
                return
            elif ch in (curses.KEY_NPAGE, 338):
                idx = min(len(entries) - 1, idx + 10)
            elif ch in (curses.KEY_PPAGE, 339):
                idx = max(0, idx - 10)

    def _draw_toc(self, entries: List[TocEntry], selected: int):
        self.stdscr.erase()
        if self.has_colors:
            self.stdscr.bkgd(" ", curses.color_pair(2) if self.theme == "light" else curses.color_pair(1))
        h, w = self.stdscr.getmaxyx()
        title = "Table of Contents - Enter/Right: open, q/Esc: back"
        try:
            self.stdscr.addnstr(0, 0, title.ljust(max(0, w - 1)), w - 1, self.header_attr)
        except curses.error:
            pass
        body_h = max(1, h - 2)
        start = max(0, selected - body_h // 2)
        end = min(len(entries), start + body_h)
        start = max(0, end - body_h)
        row = 1
        for i in range(start, end):
            entry = entries[i]
            # Format: "=> CHAPTER ONE" or "   main" (no numbers, cleaner)
            if i == selected:
                # Use selected_attr if available, otherwise fallback to reverse video
                attr = self.selected_attr if self.selected_attr != curses.A_NORMAL else curses.A_REVERSE
                text = "=> " + entry.title
            else:
                attr = curses.A_NORMAL
                text = "   " + entry.title  # Pad with 3 spaces to align with "=> "
            try:
                self.stdscr.addnstr(row, 0, text, w - 1, attr)
            except curses.error:
                pass
            row += 1
        
        # Show navigation hint at bottom
        hint = "↑/↓ or j/k: navigate | Enter/Right: jump | q/Esc: back"
        try:
            self.stdscr.addnstr(h - 1, 0, hint.ljust(max(0, w - 1)), w - 1, self.footer_attr)
        except curses.error:
            pass
        
        self.stdscr.refresh()

    def search_prompt(self):
        query = self.prompt("Search: ").strip()
        if not query:
            self.show_info_popup("Search", "Search cancelled")
            return
        if not self.search(query):
            self.show_info_popup("Search", "Not found: %s" % query)

    def search(self, query: str) -> bool:
        q = ascii_sanitize(query).lower()
        start_ch = self.chapter_index
        for offset in range(len(self.book.chapters)):
            ch_idx = (start_ch + offset) % len(self.book.chapters)
            text = ascii_sanitize(self.book.chapters[ch_idx]).lower()
            pos = text.find(q)
            if pos == -1:
                continue
            self.chapter_index = ch_idx
            self.page_index = self._page_for_char_offset(ch_idx, pos)
            self.show_info_popup("Search", "Found in chapter %d" % (ch_idx + 1))
            return True
        return False

    def _page_for_char_offset(self, chapter_index: int, char_offset: int) -> int:
        h, w = self.stdscr.getmaxyx()
        reserved = 2 if self.show_header else 1
        body_h = max(3, h - reserved)
        lines = self._wrap_text(self.book.chapters[chapter_index], max(20, w - 1))
        count = 0
        line_index = 0
        for i, line in enumerate(lines):
            count += len(line) + 1
            if count >= char_offset:
                line_index = i
                break
        return line_index // body_h

    def set_bookmark(self):
        self.store.set_bookmark(self.book.path, self.chapter_index, self.page_index)
        self.store.set_state(self.book.path, BookState(self.chapter_index, self.page_index))
        self.store.save()
        self.show_info_popup("Bookmark", "Bookmark saved")

    def open_file_picker(self):
        start_dir = os.path.dirname(self.book.path) if self.book and self.book.path else os.getcwd()
        picker = FilePicker(self.stdscr, start_dir)
        selected = picker.run()
        if not selected:
            self.show_info_popup("Load", "Load cancelled")
            return
        self._save_position()
        try:
            new_book = EpubBook(selected)
        except Exception as exc:
            self.show_info_popup("Error", "Failed to open: %s" % exc, is_error=True)
            return
        self.load_book(new_book, use_saved_position=True)


def usage() -> str:
    return (
        "Usage: termepub_reader.py [book.epub] [--bookmark] [--no-css]\n\n"
        "Controls:\n"
        "  Left / Right  previous/next page\n"
        "  Up / Down     previous/next chapter\n"
        "  t             table of contents\n"
        "  /             search\n"
        "  b             save bookmark\n"
        "  o             open book (file picker)\n"
        "  s             in picker: start live search/filter (type to filter)\n"
        "  j             in picker: jump to book starting with letter (then type a-z)\n"
        "  m             toggle dark/light mode\n"
        "  h             toggle top title bar\n"
        "  g             toggle heading style (bold/reverse)\n"
        "  q             quit\n"
        "\n"
        "Options:\n"
        "  --bookmark    open book at saved bookmark position\n"
        "  --no-css      disable inline CSS styling (faster on slow devices)\n"
        "  --version     show version number and exit\n"
        "\n"
        "CSS Support:\n"
        "  Inline CSS styling is now rendered (bold, underline, colors).\n"
        "  Uses --no-css to disable on slow devices.\n"
    )


def main(argv: List[str]) -> int:
    if len(argv) >= 2 and argv[1] == "--version":
        print(f"termepub-reader {__version__}")
        return 0
    if len(argv) >= 2 and argv[1] in {"-h", "--help"}:
        sys.stdout.write(usage())
        return 0

    epub_path = None
    open_bookmark = False
    use_css = True
    if len(argv) >= 2:
        epub_path = argv[1]
        open_bookmark = "--bookmark" in argv[2:]
        use_css = "--no-css" not in argv[2:]

    store = StateStore()

    def runner(stdscr):
        if epub_path:
            if not os.path.exists(epub_path):
                raise FileNotFoundError("File not found: %s" % epub_path)
            initial_book = EpubBook(epub_path, use_css=use_css)
        else:
            last_book_path = store.get_last_book_path()
            if last_book_path and os.path.exists(last_book_path):
                initial_book = EpubBook(last_book_path, use_css=use_css)
            else:
                picker = FilePicker(stdscr, os.getcwd())
                selected = picker.run()
                if not selected:
                    return
                initial_book = EpubBook(selected, use_css=use_css)

        if open_bookmark:
            bm = store.get_bookmark(initial_book.path)
            if bm:
                store.set_state(initial_book.path, bm)
                store.save()

        ui = ReaderUI(stdscr, initial_book, store)
        ui.run()

    try:
        curses.wrapper(runner)
        return 0
    except FileNotFoundError as exc:
        sys.stderr.write(f"{exc}\n")
        return 1
    except zipfile.BadZipFile:
        sys.stderr.write(f"Not a valid EPUB/ZIP file: {epub_path or 'selected file'}\n")
        return 1
    except Exception as exc:
        sys.stderr.write(f"Failed to open EPUB: {exc}\n")
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))

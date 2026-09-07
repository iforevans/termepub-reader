//! Pagination: styled wrapping, pages, dedup, and search.

use crate::layout::width;
use crate::{StyledSegment, TextStyle};

/// Minimum supported terminal dimensions.
pub const MIN_TERMINAL_ROWS: usize = 5;
pub const MIN_TERMINAL_COLS: usize = 10;

/// Paginates styled segments into rendered pages.
///
/// Returns `Vec<Vec<Vec<StyledSegment>>>` — pages, lines, segments.
pub fn paginate(
    segments: &[StyledSegment],
    width: usize,
    height: usize,
    show_header: bool,
    justify: bool,
) -> Vec<Vec<Vec<StyledSegment>>> {
    if width < MIN_TERMINAL_COLS || height < MIN_TERMINAL_ROWS {
        return vec![vec![vec![StyledSegment {
            text: String::from("[Terminal too small]"),
            style: TextStyle::default(),
            is_heading: false,
        }]]];
    }

    let deduped = deduplicate_headings(segments);
    let paragraphs = split_into_paragraphs(&deduped);

    let mut all_lines: Vec<Vec<StyledSegment>> = Vec::new();
    for (i, para) in paragraphs.iter().enumerate() {
        let wrapped = wrap_paragraph(para, width, justify);
        if i > 0 && !wrapped.is_empty() {
            all_lines.push(vec![StyledSegment {
                text: String::new(),
                style: TextStyle::default(),
                is_heading: false,
            }]);
        }
        all_lines.extend(wrapped);
    }

    // Strip leading blank lines caused by BOM or whitespace before content.
    while all_lines
        .first()
        .is_some_and(|line| line.first().is_none_or(|seg| seg.text.trim().is_empty()))
    {
        all_lines.remove(0);
    }

    let reserved_rows = if show_header { 4 } else { 2 };
    let body_height = height.saturating_sub(reserved_rows);
    if body_height == 0 {
        return vec![vec![vec![StyledSegment {
            text: String::from("[Terminal too small]"),
            style: TextStyle::default(),
            is_heading: false,
        }]]];
    }

    let mut pages: Vec<Vec<Vec<StyledSegment>>> = Vec::new();
    for chunk in all_lines.chunks(body_height) {
        pages.push(chunk.to_vec());
    }

    if pages.is_empty() {
        pages.push(Vec::new());
    }

    pages
}

/// Deduplicates short heading-like segments separated by blank lines.
fn deduplicate_headings(segments: &[StyledSegment]) -> Vec<StyledSegment> {
    let mut result: Vec<StyledSegment> = Vec::new();
    let mut i = 0;

    while i < segments.len() {
        let seg = &segments[i];

        if seg.text == "\n\n" && i + 1 < segments.len() {
            let next = &segments[i + 1];
            if let Some(prev) = result.last() {
                let prev_trimmed = prev.text.trim();
                let next_trimmed = next.text.trim();
                if !prev_trimmed.is_empty()
                    && prev_trimmed.len() <= 40
                    && prev_trimmed == next_trimmed
                    && !next_trimmed.is_empty()
                {
                    i += 2;
                    continue;
                }
            }
        }

        result.push(seg.clone());
        i += 1;
    }

    result
}

/// Splits segments into paragraphs delimited by "\n\n" break segments.
fn split_into_paragraphs(segments: &[StyledSegment]) -> Vec<Vec<StyledSegment>> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for seg in segments {
        if seg.text == "\n\n" {
            if !current.is_empty() {
                paragraphs.push(current);
                current = Vec::new();
            }
        } else {
            current.push(seg.clone());
        }
    }

    if !current.is_empty() {
        paragraphs.push(current);
    }

    if paragraphs.is_empty() {
        paragraphs.push(Vec::new());
    }

    paragraphs
}

/// Wraps a paragraph of segments into lines of at most `max_width` cells.
fn wrap_paragraph(
    segments: &[StyledSegment],
    max_width: usize,
    justify: bool,
) -> Vec<Vec<StyledSegment>> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Concatenate all text to check for embedded newlines.
    let full_text: String = segments.iter().map(|s| s.text.as_str()).collect();

    if full_text.contains('\n') {
        return wrap_preformatted(segments, &full_text, max_width);
    }

    // Flatten into words, keeping track of which original segment each word
    // came from (for style inheritance).  Words exceeding max_width are
    // pre-split into chunks at grapheme boundaries.
    let mut word_spans: Vec<(String, TextStyle, bool)> = Vec::new();
    for seg in segments {
        let words: Vec<&str> = seg.text.split_whitespace().collect();
        for w in words {
            if w.is_empty() {
                continue;
            }
            let w = w.to_string();
            let w_width = width::text_width(&w);
            if w_width <= max_width {
                word_spans.push((w, seg.style.clone(), seg.is_heading));
            } else {
                // Split long word into chunks of max_width.
                if let Some(mut chunks) = width::split_long_word(&w, max_width) {
                    for chunk in chunks.drain(..) {
                        word_spans.push((chunk, seg.style.clone(), seg.is_heading));
                    }
                }
            }
        }
    }

    if word_spans.is_empty() {
        return Vec::new();
    }

    // Greedy word-aware wrapping.
    let mut raw_lines: Vec<Vec<(String, TextStyle, bool)>> = Vec::new();
    let mut current_line: Vec<(String, TextStyle, bool)> = Vec::new();
    let mut current_width = 0;

    for (word, style, is_heading) in word_spans {
        let word_width = width::text_width(&word);

        if current_width == 0 {
            current_line.push((word, style, is_heading));
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            current_line.push((word, style, is_heading));
            current_width += 1 + word_width;
        } else {
            raw_lines.push(current_line);
            current_line = vec![(word, style, is_heading)];
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        raw_lines.push(current_line);
    }

    if raw_lines.is_empty() {
        return Vec::new();
    }

    // Convert to styled segments.  Each line is split into segments per
    // contiguous run of identical style so that inline styling (bold, color,
    // ...) within a single wrapped line is preserved instead of being
    // flattened to the first word's style.
    let is_final_line = raw_lines.len();
    raw_lines
        .into_iter()
        .enumerate()
        .map(|(line_idx, line_words)| {
            let gaps = if justify && line_idx + 1 < is_final_line {
                justify_gaps(&line_words, max_width).unwrap_or_else(|| plain_gaps(line_words.len()))
            } else {
                plain_gaps(line_words.len())
            };
            build_line_segments(&line_words, &gaps)
        })
        .collect()
}

/// Returns the number of spaces to place after each word of a justified
/// non-final line, or `None` when justification would add no extra space
/// (single-word line, or nothing left to distribute) — the caller then falls
/// back to plain single-space joining.
fn justify_gaps(line_words: &[(String, TextStyle, bool)], max_width: usize) -> Option<Vec<usize>> {
    if line_words.len() <= 1 {
        return None;
    }
    let content_width: usize = line_words
        .iter()
        .map(|(w, _, _)| width::text_width(w))
        .sum();
    let gaps = line_words.len() - 1;
    let total_space = max_width.saturating_sub(content_width);

    // Nothing to distribute: fall back to plain joining.  This also guards
    // against integer division by zero when there are no gaps.
    if total_space <= gaps {
        return None;
    }

    let base_extra = (total_space - gaps) / gaps;
    let extra_remainder = (total_space - gaps) % gaps;
    let mut gap_spaces = vec![0usize; line_words.len()];
    for (i, slot) in gap_spaces.iter_mut().enumerate().take(gaps) {
        let extra = if i < extra_remainder {
            base_extra + 1
        } else {
            base_extra
        };
        *slot = 1 + extra;
    }
    Some(gap_spaces)
}

/// Single-space gaps between words, with no trailing space after the last word.
fn plain_gaps(n: usize) -> Vec<usize> {
    let mut gaps = vec![1usize; n];
    if n > 0 {
        gaps[n - 1] = 0;
    }
    gaps
}

/// Builds the rendered segments for one wrapped line, grouping consecutive
/// words that share the same style into a single segment so inline styling is
/// preserved.  `gap_spaces[i]` is the number of spaces following word `i` (the
/// gap before word `i + 1`); a trailing space belongs to the style of the word
/// it follows.
fn build_line_segments(
    line_words: &[(String, TextStyle, bool)],
    gap_spaces: &[usize],
) -> Vec<StyledSegment> {
    let mut segments: Vec<StyledSegment> = Vec::new();
    for (i, (word, style, is_heading)) in line_words.iter().enumerate() {
        let mut text = word.clone();
        if i < gap_spaces.len() && gap_spaces[i] > 0 {
            text.push_str(&" ".repeat(gap_spaces[i]));
        }
        if let Some(last) = segments.last_mut() {
            if last.style == *style && last.is_heading == *is_heading {
                last.text.push_str(&text);
                continue;
            }
        }
        segments.push(StyledSegment {
            text,
            style: style.clone(),
            is_heading: *is_heading,
        });
    }
    segments
}

/// Wraps preformatted text (contains \n), preserving line breaks.
fn wrap_preformatted(
    _segments: &[StyledSegment],
    full_text: &str,
    max_width: usize,
) -> Vec<Vec<StyledSegment>> {
    let mut lines = Vec::new();

    for line_str in full_text.split('\n') {
        if line_str.is_empty() {
            lines.push(vec![StyledSegment {
                text: String::new(),
                style: TextStyle::default(),
                is_heading: false,
            }]);
            continue;
        }

        if width::text_width(line_str) <= max_width {
            lines.push(vec![StyledSegment {
                text: line_str.to_string(),
                style: TextStyle::default(),
                is_heading: false,
            }]);
        } else {
            let mut remaining = line_str.to_string();
            while !remaining.is_empty() {
                if let Some((chunk, rest)) = width::split_at_width(&remaining, max_width) {
                    lines.push(vec![StyledSegment {
                        text: chunk,
                        style: TextStyle::default(),
                        is_heading: false,
                    }]);
                    remaining = rest;
                } else {
                    lines.push(vec![StyledSegment {
                        text: remaining,
                        style: TextStyle::default(),
                        is_heading: false,
                    }]);
                    break;
                }
            }
        }
    }

    lines
}

/// Searches rendered pages for a phrase, including matches spanning lines
/// and page boundaries (any number of boundaries).
///
/// Concatenates all page text with a single-space separator, finds the first
/// match, and maps its position back to the page where the match begins.
/// The single-space separator preserves the original line-joining behavior
/// and keeps the result consistent for within-page, two-page, and
/// multi-page-spanning matches alike.
pub fn search_pages(pages: &[Vec<Vec<StyledSegment>>], query: &str) -> Option<usize> {
    if query.is_empty() || pages.is_empty() {
        return None;
    }

    let query_lower = query.to_lowercase();

    // Build concatenated text per page and the page-start byte offsets in
    // the combined string.
    let mut page_texts: Vec<String> = Vec::new();
    for page in pages {
        let mut page_text = String::new();
        for line in page {
            for seg in line {
                page_text.push_str(&seg.text);
            }
            page_text.push(' ');
        }
        page_texts.push(page_text.to_lowercase());
    }

    // Byte offset in the combined string where each page's text begins.
    let mut page_starts: Vec<usize> = Vec::with_capacity(page_texts.len());
    let mut combined = String::new();
    for (i, pt) in page_texts.iter().enumerate() {
        page_starts.push(combined.len());
        combined.push_str(pt);
        if i + 1 < page_texts.len() {
            combined.push(' ');
        }
    }

    let match_pos = combined.find(&query_lower)?;

    // The match begins on the last page whose start offset is <= match_pos.
    page_starts.iter().rposition(|&start| start <= match_pos)
}

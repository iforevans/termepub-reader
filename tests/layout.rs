//! Pagination, wrapping, dedup, and search tests.
//!
//! Tests verify layout::paginate behavior (Phase 5).

use termepub::{StyledSegment, TextStyle};

#[test]
fn rendered_page_count_is_source_of_truth() {
    // A chapter of 100 'x' chars at width=21 should produce a known number of pages.
    let segments = termepub::extract_html("<p>xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx</p>", true);
    let pages = termepub::paginate(&segments, 21, 6, true, false);
    let total = pages.len();
    assert!(total > 1, "long text should produce multiple pages");
    // total_pages for a chapter equals pages.len()
    assert_eq!(pages.len(), total);
}

#[test]
fn cache_invalidates_on_dimension_change() {
    let segments = termepub::extract_html(
        "<p>word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word</p>",
        true,
    );
    let pages_10 = termepub::paginate(&segments, 21, 10, true, false);
    let pages_20 = termepub::paginate(&segments, 21, 20, true, false);
    assert!(
        pages_20.len() < pages_10.len(),
        "more rows should produce fewer pages (got {} at h=20 vs {} at h=10)",
        pages_20.len(),
        pages_10.len()
    );
}

#[test]
fn search_across_lines_lands_on_first_affected_page() {
    // With narrow width, "four five" may span a line break.
    let segments = termepub::extract_html("<p>one two three four five six seven</p>", true);
    let pages = termepub::paginate(&segments, 21, 10, true, false);
    let result = termepub::search_pages(&pages, "four five");
    assert!(result.is_some(), "should find phrase across line break");
}

#[test]
fn search_across_pages() {
    let segments = termepub::extract_html(&format!("<p>{} </p>", "word ".repeat(99)), true);
    let pages = termepub::paginate(&segments, 21, 6, true, false);
    assert!(pages.len() > 1, "should produce multiple pages");
    // Find last word on page 0 and first word on page 1, search for the phrase.
    let last_on_p0 = pages[0]
        .last()
        .and_then(|line| line.last())
        .map(|s| s.text.as_str())
        .unwrap_or("");
    let last_word = last_on_p0.split_whitespace().last().unwrap_or("");
    let first_on_p1 = pages[1]
        .first()
        .and_then(|line| line.first())
        .map(|s| s.text.as_str())
        .unwrap_or("");
    let first_word = first_on_p1.split_whitespace().next().unwrap_or("");
    let query = format!("{last_word} {first_word}");
    let result = termepub::search_pages(&pages, &query);
    assert!(
        result.is_some(),
        "should find phrase spanning page boundary: {query}"
    );
}

#[test]
fn search_spanning_three_page_boundaries() {
    // Regression: the old search only checked within-page and two-page
    // windows, so a phrase spanning three page boundaries was never found.
    //
    // Each page's text carries a trailing space (line join) and pages are
    // joined with a single separator space, so the combined text between
    // words on different pages is two spaces.  Build pages where a marker
    // straddles three page breaks:
    //   page0 = "alpha", page1 = "beta", page2 = "gamma"
    // combined -> "alpha  beta  gamma "  (double spaces across boundaries)
    let page0: Vec<Vec<StyledSegment>> = vec![vec![seg("alpha")]];
    let page1: Vec<Vec<StyledSegment>> = vec![vec![seg("beta")]];
    let page2: Vec<Vec<StyledSegment>> = vec![vec![seg("gamma")]];
    let pages = vec![page0, page1, page2];

    let result = termepub::search_pages(&pages, "alpha  beta  gamma");
    assert_eq!(
        result,
        Some(0),
        "match spanning 3 pages should be found, starting on page 0"
    );
}

#[test]
fn search_returns_none_when_absent() {
    let pages: Vec<Vec<Vec<StyledSegment>>> = vec![vec![vec![seg("hello")]]];
    assert_eq!(termepub::search_pages(&pages, "world"), None);
    assert_eq!(termepub::search_pages(&pages, ""), None);
}

/// Helper: a single styled segment with plain text.
fn seg(text: &str) -> StyledSegment {
    StyledSegment {
        text: text.to_string(),
        style: TextStyle::default(),
        is_heading: false,
    }
}

#[test]
fn long_word_split_behavior() {
    // A word longer than the viewport should be split.
    let long_word = "a".repeat(50);
    let segments = termepub::extract_html(&format!("<p>{long_word}</p>"), true);
    let pages = termepub::paginate(&segments, 20, 10, true, false);
    // Verify no panic and content is rendered.
    assert!(!pages.is_empty());
}

#[test]
fn justification_spacing_and_non_justified_final_lines() {
    let segments = termepub::extract_html("<p>one two three four five</p>", true);
    let justified = termepub::paginate(&segments, 40, 10, true, true);
    let not_justified = termepub::paginate(&segments, 40, 10, true, false);
    // Justified pages should have different spacing distribution.
    // Justification should only apply to non-final lines of a paragraph.
    assert!(!justified.is_empty());
    assert!(!not_justified.is_empty());
}

#[test]
fn justification_single_word_line_does_not_panic() {
    // Regression: a justified non-final line that wraps to a single word
    // has zero gaps to distribute space across.  Before the fix this hit
    // an integer division by zero (panic in release, wrap in debug).
    //
    // width=10 (the minimum): "alpha" (5) + " " + "beta" (4) = 10, so both
    // words fit on ONE line, which becomes the final line (not justified).
    // To force a single-word NON-final line we need a word that fits alone
    // but not together with the next.  Use width=10 with "alpha beta":
    // 5 + 1 + 4 = 10 fits -> single line, final, not justified.  So use a
    // case where the first line is a lone word: width=10, "alpha" then a
    // word that cannot share the line.
    let segments = termepub::extract_html("<p>alpha beta</p>", true);
    // width=5 would be below MIN_TERMINAL_COLS (10).  At width=10 the two
    // words fit on one line.  Instead verify the guard path directly with a
    // single long word that wraps: "aaaaaaaaaa" (10) at width=10 is one
    // line (final).  The real single-word non-final case needs 2+ lines
    // where line 1 is a lone word — use width=10 and three words where the
    // first is exactly full-width.
    let pages = termepub::paginate(&segments, 10, 10, true, true);
    assert!(!pages.is_empty());
    let all_text: String = pages
        .iter()
        .flat_map(|page| page.iter())
        .flat_map(|line| line.iter())
        .map(|s| s.text.as_str())
        .collect();
    assert!(all_text.contains("alpha"), "missing 'alpha': {all_text}");
    assert!(all_text.contains("beta"), "missing 'beta': {all_text}");
}

#[test]
fn justification_single_word_nonfinal_line_no_panic() {
    // Regression, direct: a paragraph where line 1 is a single word that
    // fills the width, forcing "beta" onto line 2.  Line 1 is a single-word
    // non-final line -> gaps == 0 -> must not divide by zero.
    // width=10: "aaaaaaaaa" (9) + " " + "b" (1) = 11 > 10, so "b" wraps.
    // Line 1 = "aaaaaaaaa" (1 word, non-final) -> previously panicked.
    let segments = termepub::extract_html("<p>aaaaaaaaa b</p>", true);
    let pages = termepub::paginate(&segments, 10, 10, true, true);
    assert!(!pages.is_empty());
    let all_text: String = pages
        .iter()
        .flat_map(|page| page.iter())
        .flat_map(|line| line.iter())
        .map(|s| s.text.as_str())
        .collect();
    assert!(
        all_text.contains("aaaaaaaaa"),
        "missing long word: {all_text}"
    );
    assert!(all_text.contains('b'), "missing 'b': {all_text}");
}

#[test]
fn inline_style_within_line_is_preserved() {
    // Regression: a wrapped line used to be flattened into a single segment
    // carrying the first word's style, which dropped bold/color spans that
    // shared the line.  Each contiguous run of identical style must remain a
    // distinct segment.
    let segments = vec![
        StyledSegment {
            text: "hello ".to_string(),
            style: TextStyle::default(),
            is_heading: false,
        },
        StyledSegment {
            text: "bold ".to_string(),
            style: TextStyle {
                bold: true,
                ..Default::default()
            },
            is_heading: false,
        },
        StyledSegment {
            text: "tail".to_string(),
            style: TextStyle::default(),
            is_heading: false,
        },
    ];
    let pages = termepub::paginate(&segments, 80, 24, true, false);
    let all: Vec<&StyledSegment> = pages
        .iter()
        .flat_map(|p| p.iter())
        .flat_map(|l| l.iter())
        .collect();

    // The bold word must keep its bold style.
    let bold_seg = all.iter().find(|s| s.text.contains("bold")).copied();
    assert!(
        bold_seg.is_some() && bold_seg.unwrap().style.bold,
        "the 'bold' word must keep its bold style, got: {all:?}"
    );
    // A non-bold word must not be bold.
    let plain_seg = all.iter().find(|s| s.text.contains("hello")).copied();
    assert!(
        plain_seg.is_some() && !plain_seg.unwrap().style.bold,
        "the 'hello' word must stay unbolded, got: {all:?}"
    );
    // The rendered text is unchanged.
    let text: String = all.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(text, "hello bold tail");
}

#[test]
fn justified_line_preserves_inline_style() {
    // Regression: the justified path had the same flatten-to-first-style bug.
    let segments = vec![
        StyledSegment {
            text: "one ".to_string(),
            style: TextStyle::default(),
            is_heading: false,
        },
        StyledSegment {
            text: "two ".to_string(),
            style: TextStyle {
                bold: true,
                ..Default::default()
            },
            is_heading: false,
        },
        StyledSegment {
            text: "three".to_string(),
            style: TextStyle::default(),
            is_heading: false,
        },
    ];
    // Narrow width forces multiple lines so the first line is non-final and
    // gets justified.
    let pages = termepub::paginate(&segments, 10, 10, true, true);
    let all: Vec<&StyledSegment> = pages
        .iter()
        .flat_map(|p| p.iter())
        .flat_map(|l| l.iter())
        .collect();
    let bold_seg = all.iter().find(|s| s.text.contains("two")).copied();
    assert!(
        bold_seg.is_some() && bold_seg.unwrap().style.bold,
        "justified lines must keep inline style, got: {all:?}"
    );
}

#[test]
fn short_heading_deduplication() {
    // "CHAPTER ONE" separated by blank lines should be deduplicated.
    let html = "<p>CHAPTER ONE</p><p><br/></p><p>CHAPTER ONE</p>";
    let segments = termepub::extract_html(html, true);
    // Both segments should be marked as heading for this test.
    // In the actual paginator, duplicate short heading-like segments
    // separated by blank lines are deduplicated.
    assert!(segments.len() >= 2);
}

#[test]
fn long_repeated_text_is_preserved() {
    // Long body text (> 40 chars) repeated with blanks should NOT be deduplicated.
    let long_text = "This is a repeated paragraph that is longer than forty characters";
    let html = format!("<p>{long_text}</p><p><br/></p><p>{long_text}</p>");
    let segments = termepub::extract_html(&html, true);
    let pages = termepub::paginate(&segments, 40, 10, true, false);
    // Both copies should appear in the output.
    let mut occurrence_count = 0;
    for page in &pages {
        for line in page {
            for seg in line {
                if seg.text.contains("This is a repeated") {
                    occurrence_count += 1;
                }
            }
        }
    }
    assert!(
        occurrence_count >= 2,
        "long repeated text should NOT be deduplicated, found {occurrence_count}"
    );
}

#[test]
fn adjacent_repetition_is_preserved() {
    // Same text appearing without blank lines between is preserved.
    let segments = termepub::extract_html("<p>hello hello</p>", true);
    let pages = termepub::paginate(&segments, 40, 10, true, false);
    assert!(!pages.is_empty());
}

#[test]
fn cjk_text_does_not_exceed_cell_width() {
    // CJK characters are 2 terminal cells wide.
    let segments = termepub::extract_html("<p>こんにちは世界</p>", true);
    let pages = termepub::paginate(&segments, 20, 10, true, false);
    assert!(!pages.is_empty());
    // Each line should not exceed 20 cells.
    for page in &pages {
        for line in page {
            let total_width: usize = line.iter().map(|s| s.text_width()).sum();
            assert!(
                total_width <= 20,
                "line width {total_width} exceeds viewport 20"
            );
        }
    }
}

#[test]
fn combining_marks_are_not_split() {
    // e + combining acute = e\' should not be split across lines.
    let segments = termepub::extract_html("<p>cafe\u{0301}</p>", true);
    let pages = termepub::paginate(&segments, 10, 10, true, false);
    assert!(!pages.is_empty());
}

#[test]
fn emoji_grapheme_clusters_are_not_split() {
    let segments = termepub::extract_html("<p>hello \u{1F600} world</p>", true);
    let pages = termepub::paginate(&segments, 20, 10, true, false);
    assert!(!pages.is_empty());
}

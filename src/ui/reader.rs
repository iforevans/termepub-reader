use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::layout::paginate as layout_paginate;

use super::app::{App, Mode};
use super::picker::draw_picker;
use super::theme::{style_for_segment, Theme, DIALOG_FG, ERROR_BG, INFO_BG};

pub fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        Mode::Reader => draw_reader(frame, app),
        Mode::Toc => draw_toc(frame, app),
        Mode::Search => draw_search(frame, app),
        Mode::Picker => draw_picker(frame, app),
        Mode::Popup => draw_popup(frame, app),
        Mode::Help => draw_help(frame),
        Mode::Dictionary => draw_dictionary(frame, app),
    }
}

fn draw_reader(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Check if terminal is too small
    if area.height < layout_paginate::MIN_TERMINAL_ROWS as u16
        || area.width < layout_paginate::MIN_TERMINAL_COLS as u16
    {
        let msg = Paragraph::new(Span::raw("Terminal too small")).alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    let show_header = app.show_header;
    let theme = app.theme;

    let (chunks, body_idx) = if show_header {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1), // header
                    Constraint::Length(1), // separator
                    Constraint::Min(1),    // body
                    Constraint::Length(1), // separator
                    Constraint::Length(1), // footer
                ]
                .as_ref(),
            )
            .split(area);
        (chunks, 2)
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Min(1),    // body
                    Constraint::Length(1), // separator
                    Constraint::Length(1), // footer
                ]
                .as_ref(),
            )
            .split(area);
        (chunks, 0)
    };

    if show_header {
        draw_header(frame, app, chunks[0]);
        draw_separator(frame, &theme, chunks[1]);
    }

    draw_body(frame, app, chunks[body_idx]);

    let sep_idx = body_idx + 1;
    let footer_idx = body_idx + 2;
    draw_separator(frame, &theme, chunks[sep_idx]);
    draw_footer(frame, app, chunks[footer_idx]);
}

fn draw_separator(frame: &mut Frame, theme: &Theme, area: Rect) {
    let sep = "─".repeat(area.width as usize);
    let line = Paragraph::new(Span::raw(sep)).style(
        Style::default()
            .fg(theme.foreground())
            .bg(theme.background()),
    );
    frame.render_widget(line, area);
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    // Clear background
    frame.render_widget(Clear, area);

    // Set background color based on theme
    let bg_color = app.theme.background();
    let bg_block = Block::default().style(Style::default().bg(bg_color));
    frame.render_widget(bg_block, area);

    if app.pages.is_empty() || app.page_index >= app.pages.len() {
        return;
    }

    let current_page = &app.pages[app.page_index];

    let mut lines: Vec<Line> = Vec::new();
    for line_segs in current_page {
        let spans: Vec<Span> = line_segs
            .iter()
            .map(|seg| Span::styled(seg.text.clone(), style_for_segment(seg, &app.theme)))
            .collect();

        // If no spans (empty line), add a single empty span
        let line = if spans.is_empty() {
            Line::from(Span::raw(""))
        } else {
            Line::from(spans)
        };
        lines.push(line);
    }

    let text = Text::from(lines);
    // The lines were already wrapped to the body width during pagination, so
    // no second wrap is applied here.  Wrapping again would let the paragraph
    // re-break lines at slightly different points (the body rect can differ
    // from the paginated width by a cell or two), splitting words
    // unexpectedly.
    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;

    let version = env!("CARGO_PKG_VERSION");
    // TOC entries are not 1:1 with spine chapters (an entry may point at any
    // spine item and several may share one), so look up the entry whose
    // resolved spine position matches the current chapter rather than indexing
    // the TOC directly.
    let chapter_title = if let Some(ref book) = app.book {
        let toc = book.toc();
        toc.iter()
            .find(|e| e.spine_index == Some(app.chapter_index))
            .map(|e| e.title.clone())
            .unwrap_or_else(|| format!("Chapter {}", app.chapter_index + 1))
    } else {
        String::from("No book")
    };

    let page_info = format!(
        "{}/{} ({:.0}%)",
        app.page_index + 1,
        app.total_pages,
        if app.total_pages > 0 {
            (app.page_index as f64 / app.total_pages as f64) * 100.0
        } else {
            0.0
        }
    );

    let footer_text = format!(
        " termepub v{} | {} | {} | h:header ??:help q:quit ",
        version, chapter_title, page_info
    );

    let paragraph = Paragraph::new(Span::raw(footer_text))
        .style(
            Style::default()
                .fg(theme.foreground())
                .bg(theme.background())
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let title = app
        .book
        .as_ref()
        .map(|b| b.title().to_string())
        .unwrap_or_else(|| String::from("termepub"));

    let paragraph = Paragraph::new(Span::raw(title))
        .style(
            Style::default()
                .fg(theme.foreground())
                .bg(theme.background())
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

fn draw_toc(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = app.theme;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
        .split(area);

    // Title
    let title = Paragraph::new(Span::raw(" Table of Contents "))
        .style(
            Style::default()
                .fg(theme.foreground())
                .bg(theme.background())
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // TOC entries
    if let Some(ref book) = app.book {
        let toc = book.toc();
        let view_height = chunks[1].height as usize;
        let scroll = toc_scroll(app.toc_index, view_height);
        let mut lines: Vec<Line> = Vec::new();

        for (i, entry) in toc.iter().enumerate().skip(scroll).take(view_height) {
            let prefix = if i == app.toc_index { "> " } else { "  " };
            let line = Line::from(vec![Span::styled(
                format!("{}{}", prefix, entry.title),
                Style::default()
                    .fg(if i == app.toc_index {
                        Color::Yellow
                    } else {
                        theme.foreground()
                    })
                    .bg(theme.background())
                    .add_modifier(if i == app.toc_index {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )]);
            lines.push(line);
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, chunks[1]);
    }
}

fn draw_search(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = app.theme;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
        .split(area);

    // Draw underlying reader
    draw_reader(frame, app);

    // Search prompt
    let prompt = format!("/{} ", app.search_query);
    let paragraph = Paragraph::new(Span::raw(prompt))
        .style(
            Style::default()
                .fg(Color::Yellow)
                .bg(theme.background())
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, chunks[1]);
}

fn draw_help(frame: &mut Frame) {
    let area = frame.area();

    let help_text = [
        "termepub - Terminal EPUB Reader",
        "",
        "NORMAL MODE",
        "  Left        Page back",
        "  Right       Page forward",
        "  Up          Previous chapter",
        "  Down        Next chapter",
        "  PgDn        Page forward",
        "  PgUp        Page back",
        "  f           First page",
        "  l           Last page",
        "  i           Table of Contents",
        "  /           Search",
        "  o           File picker",
        "  t           Cycle theme",
        "  h           Toggle header",
        "  j           Toggle justification",
        "  m           Set bookmark",
        "  b           Go to bookmark",
        "  d           Dictionary",
        "  ?           This help screen",
        "  q/Ctrl-c    Quit",
        "",
        "SEARCH MODE",
        "  type      Enter query",
        "  Enter     Search",
        "  Esc/Ctrl-c Cancel",
        "",
        "TOC MODE",
        "  j/Down    Navigate down",
        "  k/Up      Navigate up",
        "  Enter     Go to chapter",
        "  Esc/Ctrl-c Close",
        "",
        "PICKER MODE",
        "  j/Down    Navigate down",
        "  k/Up      Navigate up",
        "  Enter     Open file/book",
        "  / or s    Filter entries (type to filter, Enter done, Esc clear)",
        "  Esc/Ctrl-c Close",
        "",
        "Press Esc or q to close this help.",
    ];

    let lines: Vec<Line> = help_text
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(DIALOG_FG).bg(INFO_BG),
            ))
        })
        .collect();

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .style(
                    Style::default()
                        .fg(DIALOG_FG)
                        .bg(INFO_BG)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: false });

    let popup_area = centered_rect(area, 80, 24);
    frame.render_widget(Clear, popup_area);
    let bg = Block::default().style(Style::default().bg(INFO_BG));
    frame.render_widget(bg, popup_area);
    frame.render_widget(paragraph, popup_area);
}

fn draw_dictionary(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Draw underlying reader first
    draw_reader(frame, app);

    // Size to content
    let result_content = app
        .dictionary_result
        .as_deref()
        .unwrap_or("Type a word and press Enter.");
    let inner_w = ((area.width as f64 * 0.7).min(area.width as f64) as usize).saturating_sub(2);
    // Estimate wrapped lines: each line of content may wrap
    let mut wrapped_lines = 0usize;
    for line in result_content.lines() {
        wrapped_lines += line.len().div_ceil(inner_w.max(1)).max(1);
    }
    // popup = border(2) + content + prompt(1)
    let needed_h = (wrapped_lines + 2 + 1) as u16; // +2 border, +1 prompt
    let min_popup_h = 6u16;
    let max_popup_h = (area.height as f64 * 0.7) as u16;
    let popup_h = needed_h.max(min_popup_h).min(max_popup_h).min(area.height);

    let popup_w = (area.width as f64 * 0.7).min(area.width as f64) as u16;
    if popup_h < 5 || popup_w < 20 {
        return;
    }

    let popup_area = centered_rect(area, popup_w, popup_h);

    // Clear behind popup and draw background block
    frame.render_widget(Clear, popup_area);
    let bg = Block::default().style(Style::default().bg(INFO_BG));
    frame.render_widget(bg, popup_area);

    // Draw outer border on the full popup area
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::raw(" Dictionary "))
        .style(
            Style::default()
                .fg(DIALOG_FG)
                .bg(INFO_BG)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(outer_block, popup_area);

    // Inner area (inside the border)
    let inner_area = Rect::new(
        popup_area.x + 1,
        popup_area.y + 1,
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );

    // Split inner into result area + prompt bar
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(inner_area);

    let result_lines_vec: Vec<Line> = result_content
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                format!("{} ", line),
                Style::default().fg(DIALOG_FG).bg(INFO_BG),
            ))
        })
        .collect();

    let result_text = Text::from(result_lines_vec);
    let result_para = Paragraph::new(result_text)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(INFO_BG));
    frame.render_widget(result_para, inner[0]);

    // Prompt bar
    let prompt = format!("dict>{}", app.dictionary_word);
    let prompt_para = Paragraph::new(Span::styled(
        prompt,
        Style::default()
            .fg(DIALOG_FG)
            .bg(INFO_BG)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(prompt_para, inner[1]);
}

fn draw_popup(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if let Some(ref msg) = app.popup_message {
        // Errors render on a red background; confirmations and status on blue.
        // Both use white text, independent of the active theme.
        let bg = if app.popup_is_error {
            ERROR_BG
        } else {
            INFO_BG
        };

        // At least 30 wide, or wide enough for the message; centered_rect caps
        // it to the terminal so long error messages don't get truncated.
        let needed_w = (msg.chars().count() as u16).saturating_add(4).max(30);
        let popup_area = centered_rect(area, needed_w, 5);

        let lines: Vec<Line> = vec![Line::from(Span::styled(
            msg.clone(),
            Style::default()
                .fg(DIALOG_FG)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ))];

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(
                Block::default().borders(Borders::ALL).style(
                    Style::default()
                        .fg(DIALOG_FG)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(Clear, popup_area);
        let bg_block = Block::default().style(Style::default().bg(bg));
        frame.render_widget(bg_block, popup_area);
        frame.render_widget(paragraph, popup_area);
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Computes the scroll offset that keeps `index` visible in a view of
/// `view_height` rows, pinning the selection to the bottom once it
/// overflows the view.
fn toc_scroll(index: usize, view_height: usize) -> usize {
    if view_height > 0 && index >= view_height {
        index - view_height + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::super::theme::{style_for_segment, Theme};
    use super::toc_scroll;
    use crate::StyledSegment;
    use crate::TextStyle;
    use ratatui::style::Modifier;

    fn styled(style: TextStyle) -> StyledSegment {
        StyledSegment {
            text: "hello".into(),
            style,
            is_heading: false,
        }
    }

    #[test]
    fn toc_scroll_keeps_selection_visible() {
        assert_eq!(toc_scroll(0, 20), 0);
        assert_eq!(toc_scroll(10, 20), 0);
        assert_eq!(toc_scroll(20, 20), 1);
        assert_eq!(toc_scroll(50, 20), 31);
        assert_eq!(toc_scroll(100, 5), 96);
        // Degenerate tiny view must not panic.
        assert_eq!(toc_scroll(7, 0), 0);
    }

    #[test]
    fn italic_style_maps_to_italic_modifier() {
        let s = TextStyle {
            italic: true,
            ..Default::default()
        };
        let style = style_for_segment(&styled(s), &Theme::Dark);
        assert!(
            style.add_modifier.contains(Modifier::ITALIC),
            "italic must set ITALIC"
        );
    }

    #[test]
    fn non_italic_segment_has_no_italic_or_strike() {
        let style = style_for_segment(&styled(TextStyle::default()), &Theme::Dark);
        assert!(!style.add_modifier.contains(Modifier::ITALIC));
        assert!(!style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn strike_style_maps_to_crossed_out_modifier() {
        let s = TextStyle {
            strike: true,
            ..Default::default()
        };
        let style = style_for_segment(&styled(s), &Theme::Dark);
        assert!(
            style.add_modifier.contains(Modifier::CROSSED_OUT),
            "strike must set CROSSED_OUT"
        );
    }
}

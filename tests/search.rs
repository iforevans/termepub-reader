//! Whole-book search tests.
//!
//! These drive the `App` key handler to verify that search scans the entire
//! book (not just the current chapter) and navigates to the matching chapter.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use support::build_epub;
use tempfile::TempDir;
use termepub::ui::app::{App, Mode};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn open_fixture_app() -> App {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "book", "epub3_nav");
    let mut app = App::new(true, (80, 24));
    app.open_book(path).expect("should open the fixture book");
    // The book content is fully loaded into memory by open_book, so the
    // temporary EPUB can be cleaned up now.
    drop(tmp);
    app
}

#[test]
fn search_finds_phrase_in_later_chapter() {
    // Regression: search only scanned the current chapter, so a phrase that
    // appears only in a later chapter was never found.  Whole-book search
    // must locate it and navigate to the right chapter.
    let mut app = open_fixture_app();
    assert_eq!(app.chapter_index, 0, "should start on the first chapter");

    // "chapter two" only appears in the second chapter.
    app.mode = Mode::Search;
    for c in "chapter two".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.chapter_index, 1,
        "search should jump to the second chapter"
    );
    assert_eq!(app.mode, Mode::Reader, "should return to the reader");
}

#[test]
fn search_from_second_chapter_finds_first_chapter_phrase() {
    // The reverse direction: start on chapter 2 and find a phrase in chapter 1.
    let mut app = open_fixture_app();
    // Down arrow advances to the next chapter.
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.chapter_index, 1);

    app.mode = Mode::Search;
    for c in "chapter one".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.chapter_index, 0,
        "search should jump back to the first chapter"
    );
}

#[test]
fn search_no_result_stays_in_reader_with_popup() {
    let mut app = open_fixture_app();
    app.mode = Mode::Search;
    for c in "this phrase is not in the book".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.mode, Mode::Popup, "no result should show a popup");
    assert_eq!(app.popup_message.as_deref(), Some("No results found."));
}

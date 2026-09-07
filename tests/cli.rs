use assert_cmd::Command;
use std::path::PathBuf;
use tempfile::TempDir;

mod support;
use support::build_epub;

/// Builds a real fixture EPUB and keeps its temp dir alive for the test.
fn fixture_epub() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = build_epub(tmp.path(), "book", "epub3_nav");
    (tmp, path)
}

#[test]
fn version_prints_and_exits_zero() {
    Command::cargo_bin("termepub")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn help_includes_documented_options() {
    let assert = Command::cargo_bin("termepub")
        .unwrap()
        .arg("--help")
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("--bookmark"),
        "help should mention --bookmark"
    );
    assert!(stdout.contains("--no-css"), "help should mention --no-css");
}

#[test]
fn short_help_works() {
    Command::cargo_bin("termepub")
        .unwrap()
        .arg("-h")
        .assert()
        .success();
}

#[test]
fn flags_before_epub_path() {
    let (_tmp, path) = fixture_epub();
    Command::cargo_bin("termepub")
        .unwrap()
        .args(["--bookmark", "--no-css", path.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn flags_after_epub_path() {
    let (_tmp, path) = fixture_epub();
    Command::cargo_bin("termepub")
        .unwrap()
        .args([path.to_str().unwrap(), "--bookmark", "--no-css"])
        .assert()
        .success();
}

#[test]
fn flags_mixed_with_epub_path() {
    let (_tmp, path) = fixture_epub();
    Command::cargo_bin("termepub")
        .unwrap()
        .args(["--bookmark", path.to_str().unwrap(), "--no-css"])
        .assert()
        .success();
}

#[test]
fn unknown_flag_fails() {
    Command::cargo_bin("termepub")
        .unwrap()
        .arg("--unknown-flag")
        .assert()
        .failure();
}

#[test]
fn non_tty_dumps_book_text() {
    // Without a TTY the reader dumps the book's text to stdout (pipeable).
    let (_tmp, path) = fixture_epub();
    let assert = Command::cargo_bin("termepub")
        .unwrap()
        .arg(path.to_str().unwrap())
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("chapter one"),
        "dump should contain chapter text: {stdout}"
    );
}

#[test]
fn non_tty_without_path_fails() {
    // No TTY and no path: nothing to dump, so it must fail cleanly.
    Command::cargo_bin("termepub").unwrap().assert().failure();
}

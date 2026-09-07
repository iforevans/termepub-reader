use clap::Parser;
use termepub::cli::Cli;
use termepub::error::Error;
use termepub::ui;
use termepub::EpubBook;

fn main() {
    match run() {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    // Check if we have a TTY — if not, fall back to non-interactive mode
    let has_tty = atty::is(atty::Stream::Stdout);

    if !has_tty {
        return run_non_interactive(&cli);
    }

    let (cols, rows) = crossterm::terminal::size().map_err(|e| Error::io_path("terminal", e))?;
    let mut app = ui::app::App::new(cli.use_css(), (cols, rows));

    // Start the dictionary load in the background so the first lookup
    // doesn't block the UI on the ~21 MB parse.
    termepub::preload_dictionary();

    // Try state store
    if let Ok(store) = termepub::StateStore::open_default() {
        app.state_store = Some(store);
        app.load_global_settings();
    }

    // Startup: explicit path -> prior book -> picker.  The default mode is
    // Reader, so a successfully opened book needs no explicit mode set.
    if let Some(ref path) = cli.epub_path {
        app.open_book(path.clone())?;
    } else if let Some(last_path) = app.get_last_book_path() {
        if let Ok(absolute) = std::path::absolute(&last_path) {
            if let Err(e) = app.open_book(absolute) {
                // Surface the failure instead of silently dropping to the
                // picker with no explanation.
                eprintln!("termepub: could not reopen last book: {e}");
            }
        }
    }

    if cli.bookmark {
        app.go_to_bookmark();
    }

    if app.book.is_none() {
        app.mode = ui::app::Mode::Picker;
        app.refresh_picker();
    }

    ui::terminal::run_app(app)
}

/// Non-interactive mode (no TTY): dump the book's text to stdout so the
/// reader can be piped, e.g. `termepub book.epub | less`.
fn run_non_interactive(cli: &Cli) -> Result<(), Error> {
    let Some(path) = &cli.epub_path else {
        return Err(Error::Message(
            "no EPUB path provided (and no TTY for the interactive reader)".into(),
        ));
    };

    let book = EpubBook::open(path, cli.use_css())?;
    for (i, chapter) in book.chapters().iter().enumerate() {
        if i > 0 {
            println!();
        }
        for seg in chapter {
            print!("{}", seg.text);
        }
    }
    println!();
    Ok(())
}

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::error::Error;
use crate::state::StateStore;
use crate::EpubBook;
use crate::StyledSegment;

use super::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Reader,
    Toc,
    Search,
    Picker,
    Popup,
    Help,
    Dictionary,
}

#[derive(Debug, Clone)]
pub(super) struct PickerEntry {
    pub(super) name: String,
    pub(super) is_dir: bool,
    pub(super) is_epub: bool,
}

pub struct App {
    pub book: Option<EpubBook>,
    pub book_path: Option<PathBuf>,
    pub chapter_index: usize,
    pub page_index: usize,
    pub total_pages: usize,
    pub pages: Vec<Vec<Vec<StyledSegment>>>,
    pub mode: Mode,
    pub show_header: bool,
    pub justify: bool,
    pub theme: Theme,
    pub use_css: bool,
    pub terminal_size: (u16, u16),
    pub search_query: String,
    pub search_result_page: Option<usize>,
    pub popup_message: Option<String>,
    /// Whether the open popup is an error (red) vs. informational (blue).
    pub popup_is_error: bool,
    pub toc_index: usize,
    pub picker_dir: PathBuf,
    pub(super) picker_entries: Vec<PickerEntry>,
    pub picker_filter: String,
    pub picker_filtering: bool,
    pub picker_selected: usize,
    pub dictionary_word: String,
    pub dictionary_result: Option<String>,
    pub state_store: Option<StateStore>,
    pub should_quit: bool,
}

impl App {
    pub fn new(use_css: bool, terminal_size: (u16, u16)) -> Self {
        Self {
            book: None,
            book_path: None,
            chapter_index: 0,
            page_index: 0,
            total_pages: 0,
            pages: Vec::new(),
            mode: Mode::Reader,
            show_header: true,
            justify: false,
            theme: Theme::Dark,
            use_css,
            terminal_size,
            search_query: String::new(),
            search_result_page: None,
            popup_message: None,
            popup_is_error: false,
            toc_index: 0,
            picker_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            picker_entries: Vec::new(),
            picker_filter: String::new(),
            picker_filtering: false,
            picker_selected: 0,
            dictionary_word: String::new(),
            dictionary_result: None,
            state_store: None,
            should_quit: false,
        }
    }

    pub fn get_last_book_path(&self) -> Option<PathBuf> {
        self.state_store
            .as_ref()
            .and_then(|s| s.get_last_book_path())
            .map(PathBuf::from)
    }

    /// Loads persisted global settings from the state store.
    pub fn load_global_settings(&mut self) {
        if let Some(ref store) = self.state_store {
            // Load theme
            if let Some(theme) = Theme::from_name(&store.get_theme()) {
                self.theme = theme;
            }
            // Load show_header
            self.show_header = store.get_show_header();
            // Load justify
            self.justify = store.get_justify_text();
        }
    }

    /// Shows a modal popup.  Errors render with a red background; all other
    /// popups (confirmations, status) render with a blue one.  Both use white
    /// text, independent of the active theme.
    pub fn show_popup(&mut self, message: impl Into<String>, is_error: bool) {
        self.popup_message = Some(message.into());
        self.popup_is_error = is_error;
        self.mode = Mode::Popup;
    }

    /// Whether an auto-repeat (held-key) event should be acted on.
    /// Navigation and typing repeat; toggles and destructive keys do not,
    /// so holding a key never flickers a setting or risks an accidental quit.
    pub fn is_repeat_safe(&self, key: KeyEvent) -> bool {
        match self.mode {
            Mode::Reader => matches!(
                key.code,
                KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Char('f')
                    | KeyCode::Char('l')
            ),
            Mode::Search | Mode::Dictionary => true,
            Mode::Toc => true,
            Mode::Picker => {
                if self.picker_filtering {
                    // Typing into the filter repeats like any text input.
                    true
                } else {
                    // Navigation repeats; '/' and 's' must not, or holding
                    // them would re-enter filter mode and type the key.
                    matches!(
                        key.code,
                        KeyCode::Up | KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('k')
                    )
                }
            }
            Mode::Popup | Mode::Help => false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Quit is available in every mode except input modes where 'q' is
        // text, and except the popup itself, where 'q' cancels the open
        // confirmation (handled by handle_popup_key).
        let typing_filter = self.mode == Mode::Picker && self.picker_filtering;
        if key.code == KeyCode::Char('q')
            && !matches!(self.mode, Mode::Dictionary | Mode::Search | Mode::Popup)
            && !typing_filter
        {
            self.show_popup("Quit? (y/n)", false);
            return true;
        }

        match self.mode {
            Mode::Reader => self.handle_reader_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Toc => self.handle_toc_key(key),
            Mode::Picker => self.handle_picker_key(key),
            Mode::Popup => self.handle_popup_key(key),
            Mode::Help => self.handle_help_key(key),
            Mode::Dictionary => self.handle_dictionary_key(key),
        }
    }

    fn handle_reader_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.show_popup("Quit? (y/n)", false);
                true
            }
            KeyCode::Left => {
                if self.page_index == 0 {
                    self.prev_chapter();
                } else {
                    self.prev_page();
                }
                true
            }
            KeyCode::Right => {
                if self.page_index + 1 >= self.total_pages {
                    self.next_chapter();
                } else {
                    self.next_page();
                }
                true
            }
            KeyCode::Up => {
                self.prev_chapter();
                true
            }
            KeyCode::Down => {
                self.next_chapter();
                true
            }
            KeyCode::PageDown => {
                self.next_page();
                true
            }
            KeyCode::PageUp => {
                self.prev_page();
                true
            }
            KeyCode::Char('f') => {
                self.first_page();
                true
            }
            KeyCode::Char('l') => {
                self.last_page();
                true
            }
            KeyCode::Char('i') => {
                self.mode = Mode::Toc;
                true
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                true
            }
            KeyCode::Char('o') => {
                self.mode = Mode::Picker;
                self.refresh_picker();
                true
            }
            KeyCode::Char('t') => {
                self.cycle_theme();
                true
            }
            KeyCode::Char('h') => {
                self.toggle_header();
                true
            }
            KeyCode::Char('j') => {
                self.toggle_justify();
                true
            }
            KeyCode::Char('m') => {
                self.set_bookmark();
                true
            }
            KeyCode::Char('b') => {
                self.go_to_bookmark();
                true
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                true
            }
            KeyCode::Char('d') => {
                self.mode = Mode::Dictionary;
                true
            }
            _ => false,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                // Search the whole book, not just the current chapter, and
                // jump to the chapter/page of the first match.
                if !self.search_query.is_empty() && self.book.is_some() {
                    match self.search_book(&self.search_query) {
                        Some((chapter, page)) => {
                            self.search_result_page = Some(page);
                            self.navigate_chapter(chapter);
                            self.go_to_page(page);
                        }
                        None => {
                            self.search_result_page = None;
                            self.search_query.clear();
                            self.show_popup("No results found.", false);
                            return true;
                        }
                    }
                }
                self.mode = Mode::Reader;
                self.search_query.clear();
                true
            }
            KeyCode::Esc => {
                self.mode = Mode::Reader;
                self.search_query.clear();
                true
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.mode = Mode::Reader;
                self.search_query.clear();
                true
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                true
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                true
            }
            _ => false,
        }
    }

    fn handle_toc_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref book) = self.book {
                    let len = book.toc().len();
                    if len > 0 && self.toc_index < len - 1 {
                        self.toc_index += 1;
                    }
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.toc_index > 0 {
                    self.toc_index -= 1;
                }
                true
            }
            KeyCode::Enter => {
                if let Some(ref book) = self.book {
                    let toc = book.toc();
                    if self.toc_index < toc.len() {
                        let entry = &toc[self.toc_index];
                        // Skip entries whose href could not be resolved to a
                        // spine item; jumping to an arbitrary chapter would be
                        // misleading.
                        if let Some(spine_index) = entry.spine_index {
                            self.navigate_chapter(spine_index);
                        }
                    }
                }
                self.mode = Mode::Reader;
                true
            }
            KeyCode::Esc => {
                self.mode = Mode::Reader;
                true
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.mode = Mode::Reader;
                true
            }
            _ => false,
        }
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> bool {
        // Filter mode: printable keys build the filter; j/k become literal
        // text instead of navigation.
        if self.picker_filtering {
            match key.code {
                KeyCode::Esc => {
                    self.clear_picker_filter();
                    return true;
                }
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                    self.clear_picker_filter();
                    return true;
                }
                KeyCode::Char(c) => {
                    self.picker_filter.push(c);
                    self.picker_selected = 0;
                    return true;
                }
                KeyCode::Backspace => {
                    self.picker_filter.pop();
                    self.picker_selected = 0;
                    return true;
                }
                KeyCode::Enter => {
                    // Keep the filter applied; the next Enter opens the
                    // selection.
                    self.picker_filtering = false;
                    return true;
                }
                _ => return true,
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let indices = self.filtered_picker_entries();
                if !indices.is_empty() {
                    let current = self.picker_selected;
                    let next = (current + 1).min(indices.len() - 1);
                    self.picker_selected = next;
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.picker_selected > 0 {
                    self.picker_selected -= 1;
                }
                true
            }
            KeyCode::Enter => {
                let indices = self.filtered_picker_entries();
                if !indices.is_empty() && self.picker_selected < indices.len() {
                    let idx = indices[self.picker_selected];
                    let entry = &self.picker_entries[idx];
                    if entry.name == ".." {
                        self.picker_dir.pop();
                        self.clear_picker_filter();
                        self.refresh_picker();
                    } else if entry.is_dir {
                        self.picker_dir.push(&entry.name);
                        self.clear_picker_filter();
                        self.refresh_picker();
                    } else if entry.is_epub {
                        let path = self.picker_dir.join(&entry.name);
                        if let Ok(absolute) = std::path::absolute(&path) {
                            match self.open_book(absolute) {
                                Ok(()) => self.mode = Mode::Reader,
                                Err(e) => self.show_popup(format!("Cannot open: {e}"), true),
                            }
                        }
                    }
                }
                true
            }
            KeyCode::Esc => {
                self.clear_picker_filter();
                self.close_overlay();
                true
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.clear_picker_filter();
                self.close_overlay();
                true
            }
            KeyCode::Char('s') | KeyCode::Char('/') => {
                self.picker_selected = 0;
                self.picker_filtering = true;
                true
            }
            _ => false,
        }
    }

    /// Clears the picker filter and leaves filter mode.
    fn clear_picker_filter(&mut self) {
        self.picker_filtering = false;
        self.picker_filter.clear();
    }

    /// Where to return when closing an overlay (picker, popup, …).
    ///
    /// If a book is open, go back to the reader.  If no book is open the
    /// picker is the app's home screen, so closing it means quitting — we
    /// raise the quit confirmation instead of trapping the user in the
    /// picker with no way out.
    fn close_overlay(&mut self) {
        if self.book.is_some() {
            self.mode = Mode::Reader;
        } else {
            self.show_popup("Quit? (y/n)", false);
        }
    }

    fn handle_popup_key(&mut self, key: KeyEvent) -> bool {
        // Any non-affirmative key (n, q, Esc, Enter, space) dismisses the
        // popup.  Only 'y' confirms a quit-confirmation popup.
        match key.code {
            KeyCode::Char('y') => {
                if self
                    .popup_message
                    .as_deref()
                    .is_some_and(|msg| msg == "Quit? (y/n)")
                {
                    self.should_quit = true;
                    true
                } else {
                    false
                }
            }
            KeyCode::Char('n')
            | KeyCode::Char('q')
            | KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Char(' ') => {
                self.popup_message = None;
                self.close_overlay();
                true
            }
            _ => false,
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Reader;
                true
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.mode = Mode::Reader;
                true
            }
            _ => false,
        }
    }

    fn handle_dictionary_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                if !self.dictionary_word.is_empty() {
                    self.dictionary_result = Some(crate::lookup_word(&self.dictionary_word));
                }
                true
            }
            KeyCode::Esc => {
                self.mode = Mode::Reader;
                self.dictionary_word.clear();
                self.dictionary_result = None;
                true
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.mode = Mode::Reader;
                self.dictionary_word.clear();
                self.dictionary_result = None;
                true
            }
            KeyCode::Char(c) => {
                self.dictionary_word.push(c);
                true
            }
            KeyCode::Backspace => {
                self.dictionary_word.pop();
                true
            }
            _ => false,
        }
    }

    fn go_to_page(&mut self, page: usize) {
        if self.pages.is_empty() {
            return;
        }
        self.page_index = page.min(self.pages.len() - 1);
    }

    /// Searches the entire book (all spine chapters) for `query`, paginating
    /// each chapter at the current terminal size.  Returns the
    /// `(chapter_index, page_index)` of the first match, or `None` if the
    /// phrase is not found anywhere in the book.
    fn search_book(&self, query: &str) -> Option<(usize, usize)> {
        let book = self.book.as_ref()?;
        let (cols, rows) = self.terminal_size;

        let mut flat_pages: Vec<Vec<Vec<StyledSegment>>> = Vec::new();
        let mut flat_map: Vec<(usize, usize)> = Vec::new();
        for (chapter, segments) in book.chapters().iter().enumerate() {
            let pages = crate::paginate(
                segments,
                cols as usize,
                rows as usize,
                self.show_header,
                self.justify,
            );
            for (page, rendered) in pages.into_iter().enumerate() {
                flat_map.push((chapter, page));
                flat_pages.push(rendered);
            }
        }

        let idx = crate::search_pages(&flat_pages, query)?;
        Some(flat_map[idx])
    }

    fn next_page(&mut self) {
        if self.page_index + 1 < self.pages.len() {
            self.page_index += 1;
        }
    }

    fn prev_page(&mut self) {
        if self.page_index > 0 {
            self.page_index -= 1;
        }
    }

    fn first_page(&mut self) {
        self.page_index = 0;
    }

    fn next_chapter(&mut self) {
        if let Some(ref book) = self.book {
            let max_ch = book.chapter_count().saturating_sub(1);
            if self.chapter_index < max_ch {
                self.navigate_chapter(self.chapter_index + 1);
            }
        }
    }

    fn prev_chapter(&mut self) {
        if self.chapter_index > 0 {
            self.navigate_chapter(self.chapter_index - 1);
        }
    }

    fn last_page(&mut self) {
        if !self.pages.is_empty() {
            self.page_index = self.pages.len() - 1;
        }
    }

    fn navigate_chapter(&mut self, chapter_idx: usize) {
        self.chapter_index = chapter_idx;
        self.page_index = 0;
        self.paginate();
    }

    fn paginate(&mut self) {
        if let Some(ref book) = self.book {
            let chapters = book.chapters();
            if self.chapter_index < chapters.len() {
                let segments = &chapters[self.chapter_index];
                let (cols, rows) = self.terminal_size;
                self.pages = crate::paginate(
                    segments,
                    cols as usize,
                    rows as usize,
                    self.show_header,
                    self.justify,
                );
                self.total_pages = self.pages.len();
                if self.page_index >= self.total_pages {
                    self.page_index = self.total_pages.saturating_sub(1);
                }
            }
        }
    }

    pub fn open_book(&mut self, path: PathBuf) -> Result<(), Error> {
        // Lexically absolute path for stable state keys (no symlink
        // resolution), so a book opened via a relative path and via its
        // absolute path share one state entry and one saved position.
        let path = match std::path::absolute(&path) {
            Ok(abs) => abs,
            Err(_) => path,
        };

        let book = EpubBook::open(&path, self.use_css)?;

        let book_hash = StateStore::book_key(&path.to_string_lossy());
        let (saved_chapter, saved_page) = if let Some(ref store) = self.state_store {
            let state = store.get_state_for_book(&book_hash);
            (state.chapter_index, state.page_index)
        } else {
            (0, 0)
        };

        self.book = Some(book);
        self.book_path = Some(path);
        self.chapter_index = 0;
        self.page_index = 0;

        // Restore saved position after paginating
        self.paginate();

        if saved_chapter > 0 || saved_page > 0 {
            // Clamp saved position to available chapters/pages
            if let Some(ref book) = self.book {
                let max_chapter = book.chapter_count().saturating_sub(1);
                let clamped_chapter = saved_chapter.min(max_chapter);
                if clamped_chapter != self.chapter_index {
                    self.navigate_chapter(clamped_chapter);
                }
                self.page_index = saved_page.min(self.pages.len().saturating_sub(1));
            }
        }

        // Save last book path
        if let Some(ref mut store) = self.state_store {
            if let Some(ref p) = self.book_path {
                store.set_global_str("last_book_path", &p.to_string_lossy());
            }
        }

        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.terminal_size = (cols, rows);
        self.paginate();
    }

    fn set_bookmark(&mut self) {
        if let Some(ref mut store) = self.state_store {
            if let Some(ref path) = self.book_path {
                let key = StateStore::book_key(&path.to_string_lossy());
                let _ = store.set_bookmark(&key, self.chapter_index, self.page_index);
            }
        }
    }

    pub fn go_to_bookmark(&mut self) {
        if let Some(ref store) = self.state_store {
            if let Some(ref path) = self.book_path {
                let key = StateStore::book_key(&path.to_string_lossy());
                if let Some(bm) = store.get_bookmark(&key) {
                    if bm.chapter_index != self.chapter_index {
                        self.navigate_chapter(bm.chapter_index);
                    }
                    self.page_index = bm.page_index.min(self.pages.len().saturating_sub(1));
                }
            }
        }
    }

    fn cycle_theme(&mut self) {
        self.theme = self.theme.next_theme();
    }

    fn toggle_header(&mut self) {
        self.show_header = !self.show_header;
        self.paginate();
    }

    fn toggle_justify(&mut self) {
        self.justify = !self.justify;
        self.paginate();
    }

    pub fn save_state(&mut self) {
        if let Some(ref mut store) = self.state_store {
            if let Some(ref path) = self.book_path {
                let key = StateStore::book_key(&path.to_string_lossy());
                store.set_state_for_book(&key, self.chapter_index, self.page_index);

                // Save theme and settings
                store.set_global_str("theme", self.theme.name());
                store.set_global_bool("show_header", self.show_header);
                store.set_global_bool("justify_text", self.justify);

                let _ = store.save();
            }
        }
    }

    pub fn refresh_picker(&mut self) {
        self.picker_entries.clear();
        self.picker_selected = 0;

        // Add parent directory entry
        if self.picker_dir.ancestors().count() > 1 {
            self.picker_entries.push(PickerEntry {
                name: "..".to_string(),
                is_dir: true,
                is_epub: false,
            });
        }

        if let Ok(entries) = std::fs::read_dir(&self.picker_dir) {
            let mut dirs: Vec<PathBuf> = Vec::new();
            let mut epubs: Vec<PathBuf> = Vec::new();

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let is_dir = path.is_dir();
                let is_epub = path
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("epub"))
                    .unwrap_or(false);

                if is_dir {
                    dirs.push(path);
                } else if is_epub {
                    epubs.push(path);
                }
            }

            dirs.sort_by(|a, b| {
                a.file_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
            });
            epubs.sort_by(|a, b| {
                a.file_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
            });

            for d in dirs {
                if let Some(name) = d.file_name().and_then(|n| n.to_str()) {
                    self.picker_entries.push(PickerEntry {
                        name: name.to_string(),
                        is_dir: true,
                        is_epub: false,
                    });
                }
            }

            for e in epubs {
                if let Some(name) = e.file_name().and_then(|n| n.to_str()) {
                    self.picker_entries.push(PickerEntry {
                        name: name.to_string(),
                        is_dir: false,
                        is_epub: true,
                    });
                }
            }
        }
    }

    pub fn filtered_picker_entries(&self) -> Vec<usize> {
        if self.picker_filter.is_empty() {
            return (0..self.picker_entries.len()).collect();
        }
        let filter_lower = self.picker_filter.to_lowercase();
        self.picker_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.name.to_lowercase().contains(&filter_lower))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn app() -> App {
        App::new(true, (80, 24))
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn with_picker(mut app: App) -> App {
        app.mode = Mode::Picker;
        app.picker_entries = vec![
            PickerEntry {
                name: "steel-beach.epub".into(),
                is_dir: false,
                is_epub: true,
            },
            PickerEntry {
                name: "neuromancer.epub".into(),
                is_dir: false,
                is_epub: true,
            },
            PickerEntry {
                name: "notes".into(),
                is_dir: true,
                is_epub: false,
            },
        ];
        app
    }

    #[test]
    fn picker_slash_enters_filter_mode_and_typing_filters() {
        let mut app = with_picker(app());
        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.picker_filtering, "'/' should enter filter mode");

        for c in ['s', 't', 'e'] {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.picker_filter, "ste");
        let indices = app.filtered_picker_entries();
        assert_eq!(indices, vec![0], "only steel-beach.epub matches 'ste'");

        // Backspace edits the filter.
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.picker_filter, "st");

        // Enter keeps the filter applied but leaves filter mode.
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.picker_filtering);
        assert_eq!(app.picker_filter, "st");
    }

    #[test]
    fn picker_filter_esc_clears_and_q_types_instead_of_quitting() {
        let mut app = with_picker(app());
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(
            app.mode,
            Mode::Picker,
            "'q' while filtering should type, not quit"
        );
        assert_eq!(app.picker_filter, "q");

        app.handle_key(key(KeyCode::Esc));
        assert!(!app.picker_filtering);
        assert!(app.picker_filter.is_empty(), "Esc should clear the filter");
    }

    #[test]
    fn repeat_safe_allows_navigation_and_typing_but_not_toggles() {
        let mut app = app();
        app.mode = Mode::Reader;
        assert!(app.is_repeat_safe(key(KeyCode::Right)));
        assert!(app.is_repeat_safe(key(KeyCode::PageDown)));
        assert!(
            !app.is_repeat_safe(key(KeyCode::Char('t'))),
            "theme toggle must not repeat"
        );
        assert!(
            !app.is_repeat_safe(key(KeyCode::Char('q'))),
            "quit must not repeat"
        );

        app.mode = Mode::Search;
        assert!(
            app.is_repeat_safe(key(KeyCode::Char('a'))),
            "typing repeats"
        );

        app.mode = Mode::Picker;
        app.picker_filtering = false;
        assert!(
            app.is_repeat_safe(key(KeyCode::Down)),
            "picker navigation repeats"
        );
        assert!(
            !app.is_repeat_safe(key(KeyCode::Char('/'))),
            "filter trigger must not repeat outside filter mode"
        );
        app.picker_filtering = true;
        assert!(
            app.is_repeat_safe(key(KeyCode::Char('a'))),
            "filter typing repeats"
        );

        app.mode = Mode::Popup;
        assert!(
            !app.is_repeat_safe(key(KeyCode::Char('y'))),
            "destructive keys must not repeat"
        );
    }

    #[test]
    fn popup_q_no_book_reopens_quit_confirmation() {
        // Regression: with no book open, cancelling the quit popup used to
        // bounce back to the picker, trapping the user with no way to exit.
        // It must re-raise the quit confirmation instead.
        let mut app = app();
        app.mode = Mode::Popup;
        app.popup_message = Some("Quit? (y/n)".into());

        app.handle_key(key(KeyCode::Char('q')));

        assert!(!app.should_quit, "'q' must not confirm quit");
        assert_eq!(
            app.popup_message.as_deref(),
            Some("Quit? (y/n)"),
            "no book -> re-raise quit confirmation, don't trap in picker"
        );
    }

    #[test]
    fn picker_esc_with_no_book_offers_quit() {
        // Regression: ESC in the picker with no book open used to do nothing
        // (mode stayed Picker, no way out but killing the process).  It must
        // now raise the quit confirmation.
        let mut app = app();
        app.mode = Mode::Picker;
        app.picker_entries = vec![PickerEntry {
            name: "book.epub".into(),
            is_dir: false,
            is_epub: true,
        }];

        app.handle_key(key(KeyCode::Esc));

        assert_eq!(
            app.mode,
            Mode::Popup,
            "no book -> ESC should raise quit confirmation"
        );
        assert_eq!(app.popup_message.as_deref(), Some("Quit? (y/n)"));
    }

    #[test]
    fn popup_y_confirms_quit() {
        let mut app = app();
        app.mode = Mode::Popup;
        app.popup_message = Some("Quit? (y/n)".into());

        app.handle_key(key(KeyCode::Char('y')));

        assert!(app.should_quit, "'y' should confirm quit");
    }
}

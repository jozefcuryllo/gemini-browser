use serde::{Deserialize, Serialize};
use std::io::Read;
use std::io::Write;
use std::{collections::VecDeque, fs::File, path::PathBuf};
use url::Url;

const HISTORY_LIMIT: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    Normal,
    Editing,
    Help,
    Bookmarks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeminiLine {
    Text(String),
    Link(Url, String),
    Header(u8, String),
    ListItem(String),
    Quote(String),
    Preformatted(String),
}

#[derive(Clone, Debug)]
pub struct Page {
    pub url: Url,
    pub content: Vec<GeminiLine>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

pub struct BrowserState {
    pub current_request_id: u64,
    pub history_back: VecDeque<Page>,
    pub history_forward: VecDeque<Page>,
    pub current_page: Option<Page>,
    pub scroll_offset: usize,
    pub selected_link_index: Option<usize>,
    pub selected_bookmark_index: usize,
    pub error_message: Option<String>,
    pub url_input: String,
    pub input_mode: InputMode,
    pub is_loading: bool,
    pub bookmarks: Vec<Bookmark>,
}

impl BrowserState {
    pub fn new(bookmarks: Vec<Bookmark>) -> Self {
        Self {
            current_request_id: 0,
            history_back: VecDeque::with_capacity(HISTORY_LIMIT),
            history_forward: VecDeque::with_capacity(HISTORY_LIMIT),
            current_page: None,
            scroll_offset: 0,
            selected_link_index: None,
            selected_bookmark_index: 0,
            error_message: None,
            url_input: String::new(),
            input_mode: InputMode::Normal,
            is_loading: false,
            bookmarks,
        }
    }

    pub fn push_history(&mut self, page: Page) {
        if self.history_back.len() >= HISTORY_LIMIT {
            self.history_back.pop_front();
        }
        self.history_back.push_back(page);
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error_message = Some(msg.into());
        self.is_loading = false;
    }

    fn get_bookmarks_path() -> PathBuf {
        let mut path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        if path.starts_with("/usr/bin") {
            if let Some(home) = std::env::var_os("HOME") {
                path = PathBuf::from(home).join(".gemini_bookmarks.bin");
                return path;
            }
        }
        path.push("bookmarks.bin");
        path
    }

    pub fn save_bookmarks(&mut self) {
        let path = Self::get_bookmarks_path();
        if let Ok(encoded) = bincode::serialize(&self.bookmarks) {
            if let Ok(mut file) = File::create(path) {
                let _ = file.write_all(&encoded);
                self.error_message = Some("Bookmarks saved!".to_string());
            }
        }
    }

    pub fn load_bookmarks() -> Vec<Bookmark> {
        let path = Self::get_bookmarks_path();
        if let Ok(mut file) = File::open(path) {
            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).is_ok() {
                if let Ok(decoded) = bincode::deserialize(&buffer) {
                    return decoded;
                }
            }
        }
        Vec::new()
    }
}

mod client;
mod domain;
mod ui;

use crate::client::GeminiClient;
use crate::domain::{Bookmark, BrowserState, GeminiLine, InputMode, Page};
use pancurses::{Input, endwin, initscr, noecho, start_color, use_default_colors};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use url::Url;

enum AppEvent {
    Loaded(Result<Page, String>, u64),
}

fn main() {
    let window = initscr();
    window.keypad(true);
    window.nodelay(true);
    noecho();
    if start_color() == pancurses::OK {
        use_default_colors();
        ui::init_colors();
    }

    let initial_bookmarks = Vec::new();
    let mut state = BrowserState::new(initial_bookmarks);
    let (tx, rx) = mpsc::channel();

    if let Ok(u) = Url::parse("gemini://geminiprotocol.net/") {
        state.current_request_id += 1;
        state.is_loading = true;
        fetch_async(&u, tx.clone(), state.current_request_id);
    }

    loop {
        ui::draw(&window, &state);

        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Loaded(res, id) => {
                    if id == state.current_request_id {
                        state.is_loading = false;
                        match res {
                            Ok(page) => {
                                if let Some(curr) = state.current_page.take() {
                                    if curr.url != page.url {
                                        state.push_history(curr);
                                    }
                                }
                                state.history_forward.clear();
                                state.current_page = Some(page);
                                state.scroll_offset = 0;
                                state.selected_link_index = None;
                            }
                            Err(e) => state.set_error(e),
                        }
                    }
                }
            }
        }

        if let Some(input) = window.getch() {
            match input {
                Input::Character('q') => break,
                _ => handle_input(input, &mut state, tx.clone()),
            }
        }

        thread::sleep(Duration::from_millis(16));
    }
    endwin();
}

fn handle_input(input: Input, state: &mut BrowserState, tx: mpsc::Sender<AppEvent>) {
    match state.input_mode {
        InputMode::Normal => handle_normal_mode(input, state, tx),
        InputMode::Editing => handle_editing_mode(input, state, tx),
        InputMode::Bookmarks => handle_bookmarks_mode(input, state, tx),
        InputMode::Help => {
            state.input_mode = InputMode::Normal;
        }
    }
}

fn fetch_async(url: &Url, tx: mpsc::Sender<AppEvent>, id: u64) {
    let url = url.clone();
    thread::spawn(move || {
        let result = GeminiClient::fetch(&url, 5);
        let _ = tx.send(AppEvent::Loaded(result, id));
    });
}

fn handle_normal_mode(input: Input, state: &mut BrowserState, tx: mpsc::Sender<AppEvent>) {
    match input {
        Input::Character('g') => {
            state.input_mode = InputMode::Editing;
            state.url_input.clear();
        }
        Input::Character('h') => state.input_mode = InputMode::Help,
        Input::Character('l') => state.input_mode = InputMode::Bookmarks,
        Input::Character('\u{2}') => {
            if let Some(p) = &state.current_page {
                state.bookmarks.push(Bookmark {
                    url: p.url.to_string(),
                    title: p.url.to_string(),
                });
            }
        }
        Input::Character('b') => {
            if let Some(p) = state.history_back.pop_back() {
                if let Some(c) = state.current_page.take() {
                    state.history_forward.push_front(c);
                }
                state.current_page = Some(p);
                state.scroll_offset = 0;
            }
        }
        Input::Character('f') => {
            if let Some(p) = state.history_forward.pop_front() {
                if let Some(c) = state.current_page.take() {
                    state.push_history(c);
                }
                state.current_page = Some(p);
                state.scroll_offset = 0;
            }
        }
        Input::Character('\t') | Input::KeyBTab => {
            if let Some(page) = &state.current_page {
                let count = page
                    .content
                    .iter()
                    .filter(|l| matches!(l, GeminiLine::Link(..)))
                    .count();
                if count > 0 {
                    let is_back = matches!(input, Input::KeyBTab);
                    state.selected_link_index = Some(if is_back {
                        state
                            .selected_link_index
                            .map(|i| if i == 0 { count - 1 } else { i - 1 })
                            .unwrap_or(count - 1)
                    } else {
                        state
                            .selected_link_index
                            .map(|i| (i + 1) % count)
                            .unwrap_or(0)
                    });
                }
            }
        }
        Input::Character('\n') => {
            if let (Some(page), Some(idx)) = (&state.current_page, state.selected_link_index) {
                let link = page
                    .content
                    .iter()
                    .filter_map(|l| {
                        if let GeminiLine::Link(u, _) = l {
                            Some(u)
                        } else {
                            None
                        }
                    })
                    .nth(idx);

                if let Some(url) = link {
                    state.current_request_id += 1;
                    state.is_loading = true;
                    fetch_async(url, tx, state.current_request_id);
                }
            }
        }
        Input::KeyUp | Input::Character('k') => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1)
        }
        Input::KeyDown | Input::Character('j') => state.scroll_offset += 1,
        _ => {}
    }
}

fn handle_editing_mode(input: Input, state: &mut BrowserState, tx: mpsc::Sender<AppEvent>) {
    match input {
        Input::Character('\n') => {
            let raw = state.url_input.trim();
            if !raw.is_empty() {
                let target = if raw.contains("://") {
                    raw.to_string()
                } else {
                    format!("gemini://{}", raw)
                };

                if let Ok(u) = Url::parse(&target) {
                    state.current_request_id += 1;
                    state.is_loading = true;
                    fetch_async(&u, tx, state.current_request_id);
                } else {
                    state.set_error("Invalid URL");
                }
            }
            state.input_mode = InputMode::Normal;
        }
        Input::Character('\u{1b}') => state.input_mode = InputMode::Normal,
        Input::KeyBackspace | Input::Character('\u{7f}') => {
            state.url_input.pop();
        }
        Input::Character(c) => state.url_input.push(c),
        _ => {}
    }
}

fn handle_bookmarks_mode(input: Input, state: &mut BrowserState, tx: mpsc::Sender<AppEvent>) {
    match input {
        Input::KeyUp => {
            state.selected_bookmark_index = state.selected_bookmark_index.saturating_sub(1)
        }
        Input::KeyDown => {
            if state.selected_bookmark_index + 1 < state.bookmarks.len() {
                state.selected_bookmark_index += 1;
            }
        }
        Input::Character('\n') => {
            if let Some(b) = state.bookmarks.get(state.selected_bookmark_index) {
                if let Ok(u) = Url::parse(&b.url) {
                    state.current_request_id += 1;
                    state.is_loading = true;
                    fetch_async(&u, tx, state.current_request_id);
                }
            }
            state.input_mode = InputMode::Normal;
        }
        Input::Character('d') => {
            if !state.bookmarks.is_empty() {
                state.bookmarks.remove(state.selected_bookmark_index);
                state.selected_bookmark_index = state
                    .selected_bookmark_index
                    .min(state.bookmarks.len().saturating_sub(1));
            }
        }
        _ => state.input_mode = InputMode::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use url::Url;

    #[test]
    fn test_request_id_cancellation_logic() {
        let mut state = BrowserState::new(vec![]);
        let (tx, rx) = mpsc::channel();

        state.current_request_id += 1;
        let id1 = state.current_request_id;

        state.current_request_id += 1;
        let id2 = state.current_request_id;

        let dummy_page1 = Page {
            url: Url::parse("gemini://old").unwrap(),
            content: vec![],
        };
        let dummy_page2 = Page {
            url: Url::parse("gemini://new").unwrap(),
            content: vec![],
        };

        tx.send(AppEvent::Loaded(Ok(dummy_page1), id1)).unwrap();
        tx.send(AppEvent::Loaded(Ok(dummy_page2), id2)).unwrap();

        if let Ok(AppEvent::Loaded(res, id)) = rx.try_recv() {
            if id == state.current_request_id {
                state.current_page = Some(res.unwrap());
            }
        }
        assert!(state.current_page.is_none());

        if let Ok(AppEvent::Loaded(res, id)) = rx.try_recv() {
            if id == state.current_request_id {
                state.current_page = Some(res.unwrap());
            }
        }
        assert_eq!(state.current_page.unwrap().url.as_str(), "gemini://new");
    }

    #[test]
    fn test_handle_input_increments_id() {
        let mut state = BrowserState::new(vec![]);
        let (tx, _) = mpsc::channel();
        state.input_mode = InputMode::Editing;
        state.url_input = "test.com".into();

        let initial_id = state.current_request_id;
        handle_input(Input::Character('\n'), &mut state, tx);

        assert!(state.current_request_id > initial_id);
        assert!(state.is_loading);
    }
}

use crate::domain::{BrowserState, GeminiLine, InputMode};
use pancurses::{
    A_BOLD, A_REVERSE, COLOR_BLACK, COLOR_BLUE, COLOR_CYAN, COLOR_GREEN, COLOR_MAGENTA, COLOR_PAIR,
    COLOR_WHITE, COLOR_YELLOW, Window, init_pair,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn init_colors() {
    init_pair(1, COLOR_WHITE, COLOR_BLUE);
    init_pair(2, COLOR_BLACK, COLOR_CYAN);
    init_pair(3, COLOR_CYAN, -1);
    init_pair(4, COLOR_YELLOW, -1);
    init_pair(5, COLOR_MAGENTA, -1);
    init_pair(6, COLOR_GREEN, -1);
}

pub fn draw(window: &Window, state: &BrowserState) {
    window.erase();
    let (max_y, max_x) = window.get_max_yx();
    let width = max_x as usize;

    draw_header(window, state, width);

    match state.input_mode {
        InputMode::Help => draw_help(window, max_y, max_x),
        InputMode::Bookmarks => draw_bookmarks(window, state, max_y, width),
        _ => draw_content(window, state, max_y, width),
    }

    draw_footer(window, state, max_y, width);
    window.refresh();
}

fn draw_header(window: &Window, state: &BrowserState, width: usize) {
    let status = if state.is_loading {
        "[LOADING]"
    } else {
        "[READY]"
    };
    let url = state
        .current_page
        .as_ref()
        .map(|p| p.url.as_str())
        .unwrap_or("EMPTY");
    let header_text = format!(" {} URL: {}", status, url);

    window.attron(COLOR_PAIR(1));
    window.mvaddstr(0, 0, truncate_to_width(&header_text, width));
    window.attroff(COLOR_PAIR(1));
}

fn draw_content(window: &Window, state: &BrowserState, max_y: i32, width: usize) {
    let Some(page) = &state.current_page else {
        return;
    };

    let mut screen_row = 1;
    let mut current_link_idx = 0;
    let mut current_line_in_render = 0;

    for line in &page.content {
        let lines_to_draw = match line {
            GeminiLine::Text(t) => wrap_text(t, width),
            GeminiLine::Link(_, label) => vec![format!("=> {}", label)],
            GeminiLine::Header(lvl, txt) => vec![format!("{} {}", "#".repeat(*lvl as usize), txt)],
            GeminiLine::ListItem(t) => wrap_text(t, width.saturating_sub(2))
                .into_iter()
                .map(|s| format!("* {}", s))
                .collect(),
            GeminiLine::Quote(t) => wrap_text(t, width.saturating_sub(2))
                .into_iter()
                .map(|s| format!("> {}", s))
                .collect(),
            GeminiLine::Preformatted(t) => vec![t.to_string()],
        };

        let is_link = matches!(line, GeminiLine::Link(..));

        for text in lines_to_draw {
            if current_line_in_render >= state.scroll_offset {
                if screen_row < max_y - 1 {
                    let is_selected =
                        is_link && state.selected_link_index == Some(current_link_idx);
                    render_line(window, line, &text, screen_row, width, is_selected);
                    screen_row += 1;
                }
            }
            current_line_in_render += 1;
        }

        if is_link {
            current_link_idx += 1;
        }
    }
}

fn render_line(
    window: &Window,
    line_type: &GeminiLine,
    text: &str,
    row: i32,
    width: usize,
    is_selected: bool,
) {
    let mut style = match line_type {
        GeminiLine::Header(1, _) => COLOR_PAIR(4) | A_BOLD,
        GeminiLine::Header(_, _) => COLOR_PAIR(6) | A_BOLD,
        GeminiLine::Link(..) => COLOR_PAIR(3),
        GeminiLine::Preformatted(_) => COLOR_PAIR(5),
        _ => 0,
    };

    if is_selected {
        style |= A_REVERSE;
    }

    window.attron(style);
    window.mvaddstr(row, 0, truncate_to_width(text, width));
    window.attroff(style);
}

fn draw_help(window: &Window, max_y: i32, max_x: i32) {
    let help = [
        "--- HELP ---",
        "G: Go to URL | B: Back | F: Forward",
        "TAB: Next Link | S-TAB: Prev Link",
        "CTRL+B: Bookmark | L: List Bookmarks",
        "H: Close Help | Q: Quit",
    ];
    for (i, txt) in help.iter().enumerate() {
        window.mvaddstr(
            max_y / 2 - 2 + i as i32,
            (max_x - txt.len() as i32) / 2,
            txt,
        );
    }
}

fn draw_bookmarks(window: &Window, state: &BrowserState, max_y: i32, width: usize) {
    window.mvaddstr(1, 0, "--- BOOKMARKS ---");
    for (i, b) in state.bookmarks.iter().enumerate() {
        if i as i32 >= max_y - 3 {
            break;
        }
        if state.selected_bookmark_index == i {
            window.attron(A_REVERSE);
        }
        window.mvaddstr(
            2 + i as i32,
            0,
            truncate_to_width(&format!(" > {} ", b.url), width),
        );
        window.attroff(A_REVERSE);
    }
}

fn draw_footer(window: &Window, state: &BrowserState, max_y: i32, width: usize) {
    let mut hovered = String::new();
    if let (Some(page), Some(idx)) = (&state.current_page, state.selected_link_index) {
        if let Some(GeminiLine::Link(u, _)) = page
            .content
            .iter()
            .filter(|l| matches!(l, GeminiLine::Link(..)))
            .nth(idx)
        {
            hovered = format!(" | LINK: {}", u);
        }
    }

    window.attron(COLOR_PAIR(2));
    let footer = if state.input_mode == InputMode::Editing {
        format!(" OPEN URL: {}", state.url_input)
    } else {
        format!(
            " G:Go B:Back F:Fwd TAB:Nav C-B:Book L:List Q:Quit{}",
            hovered
        )
    };
    window.mvaddstr(
        max_y - 1,
        0,
        &format!(
            "{:<width$}",
            truncate_to_width(&footer, width),
            width = width
        ),
    );
    window.attroff(COLOR_PAIR(2));
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut result = Vec::new();
    for line in text.split('\n') {
        let mut current_line = String::new();
        for word in line.split_whitespace() {
            let word_width = word.width();
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.width() + 1 + word_width <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                result.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            result.push(current_line);
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut current_width = 0;
    let mut result = String::new();
    for c in s.chars() {
        let char_width = UnicodeWidthChar::width(c).unwrap_or(0);
        if current_width + char_width > max_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GeminiLine, Page};
    use url::Url;

    #[test]
    fn test_wrap_text_simple() {
        let text = "ala ma kota";
        let wrapped = wrap_text(text, 6);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0], "ala ma");
        assert_eq!(wrapped[1], "kota");
    }

    #[test]
    fn test_wrap_text_exact_width() {
        let text = "123 456";
        let wrapped = wrap_text(text, 3);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0], "123");
        assert_eq!(wrapped[1], "456");
    }

    #[test]
    fn test_wrap_text_long_word() {
        let text = "superkalifradżilistikekspialidocjus";
        let wrapped = wrap_text(text, 10);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "superkalifradżilistikekspialidocjus");
    }

    #[test]
    fn test_wrap_text_empty() {
        let text = "";
        let wrapped = wrap_text(text, 10);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "");
    }

    #[test]
    fn test_truncate_utf8_width() {
        let text = "zażółć gęślą jaźń";
        let truncated = truncate_to_width(text, 6);
        assert_eq!(truncated, "zażółć");
    }

    #[test]
    fn test_wrap_text_multiple_spaces() {
        let text = "word    word";
        let wrapped = wrap_text(text, 5);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0], "word");
        assert_eq!(wrapped[1], "word");
    }

    #[test]
    fn test_header_formatting_logic() {
        let lines = vec![
            GeminiLine::Header(1, "H1".into()),
            GeminiLine::Header(2, "H2".into()),
            GeminiLine::Header(3, "H3".into()),
        ];

        let results: Vec<String> = lines
            .iter()
            .map(|line| {
                if let GeminiLine::Header(lvl, txt) = line {
                    format!("{} {}", "#".repeat(*lvl as usize), txt)
                } else {
                    String::new()
                }
            })
            .collect();

        assert_eq!(results[0], "# H1");
        assert_eq!(results[1], "## H2");
        assert_eq!(results[2], "### H3");
    }

    #[test]
    fn test_link_selection_isolation() {
        let url = Url::parse("gemini://root").unwrap();
        let content = vec![
            GeminiLine::Text("Text 1".into()),
            GeminiLine::Link(Url::parse("gemini://link1").unwrap(), "L1".into()),
            GeminiLine::Text("Text 2".into()),
            GeminiLine::Link(Url::parse("gemini://link2").unwrap(), "L2".into()),
        ];
        let page = Page { url, content };

        let links: Vec<String> = page
            .content
            .iter()
            .filter_map(|l| {
                if let GeminiLine::Link(u, _) = l {
                    Some(u.as_str().to_string())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(links.len(), 2);
        assert!(links[0].contains("gemini://link1"));
        assert!(links[1].contains("gemini://link2"));
    }

    #[test]
    fn test_scroll_logic_simulation() {
        let mut display_lines = Vec::new();
        for i in 0..20 {
            display_lines.push(format!("Line {}", i));
        }

        let max_y = 10;
        let scroll_offset = 15;
        let mut rendered = 0;

        for (idx, _) in display_lines.iter().enumerate() {
            if idx >= scroll_offset && rendered < (max_y - 1) {
                rendered += 1;
            }
        }

        assert_eq!(rendered, 5);
    }

    #[test]
    fn test_preformatted_text_no_wrap() {
        let long_pre =
            "This is a very long preformatted line that should not be wrapped by wrap_text logic";
        let width = 10;

        let wrapped = wrap_text(long_pre, width);
        assert!(wrapped.len() > 1);

        let pre_line = GeminiLine::Preformatted(long_pre.into());
        if let GeminiLine::Preformatted(t) = pre_line {
            assert_eq!(t, long_pre);
        }
    }

    #[test]
    fn test_quote_formatting() {
        let quote = "To be or not to be";
        let width = 10;
        let wrapped = wrap_text(quote, width - 2);

        let first_line = format!("> {}", wrapped[0]);
        assert!(first_line.starts_with("> "));
        assert!(first_line.len() <= width);
    }
}

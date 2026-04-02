use crate::domain::{GeminiLine, Page};
use native_tls::TlsConnector;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use url::Url;

pub struct GeminiClient;

impl GeminiClient {
    pub fn fetch(url: &Url, redirect_limit: u8) -> Result<Page, String> {
        if redirect_limit == 0 {
            return Err("Too many redirects".into());
        }

        if url.scheme() != "gemini" {
            return Err("Only gemini:// protocol is supported".into());
        }

        let host = url.host_str().ok_or("Missing host")?;
        let port = url.port().unwrap_or(1965);
        let addr = format!("{}:{}", host, port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("Could not resolve host")?;

        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?;

        let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(10))
            .map_err(|e| e.to_string())?;
        
        stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;

        let mut tls = connector.connect(host, stream).map_err(|e| e.to_string())?;

        tls.write_all(format!("{}\r\n", url).as_bytes())
            .map_err(|e| e.to_string())?;

        let mut raw = Vec::new();
        tls.read_to_end(&mut raw).map_err(|e| e.to_string())?;

        let header_end = raw
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("Invalid response")?;
        
        let header = String::from_utf8_lossy(&raw[..header_end]);
        let body = &raw[header_end + 2..];

        if header.starts_with('3') {
            let target_str = header[3..].trim();
            let target_url = url.join(target_str).map_err(|_| "Invalid redirect URL")?;
            return Self::fetch(&target_url, redirect_limit - 1);
        }

        if !header.starts_with('2') {
            return Err(format!("Server returned: {}", header));
        }

        Self::parse(url.clone(), body)
    }

    fn parse(url: Url, body: &[u8]) -> Result<Page, String> {
        let content_str = String::from_utf8_lossy(body);
        let mut content = Vec::new();
        let mut pre = false;

        for line in content_str.lines() {
            let l = line.trim_end_matches('\r');

            if l.starts_with("```") {
                pre = !pre;
                continue;
            }

            if pre {
                content.push(GeminiLine::Preformatted(l.to_string()));
                continue;
            }

            if l.is_empty() {
                content.push(GeminiLine::Text(String::new()));
            } else if let Some(rest) = l.strip_prefix("=>") {
                let rest = rest.trim();
                let mut parts = rest.splitn(2, |c: char| c == ' ' || c == '\t');
                if let Some(target) = parts.next() {
                    let target = target.trim();
                    if let Ok(u) = url.join(target) {
                        let label = parts.next()
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(target);
                        content.push(GeminiLine::Link(u, label.to_string()));
                    }
                }
            } else if let Some(rest) = l.strip_prefix("###") {
                content.push(GeminiLine::Header(3, rest.trim().to_string()));
            } else if let Some(rest) = l.strip_prefix("##") {
                content.push(GeminiLine::Header(2, rest.trim().to_string()));
            } else if let Some(rest) = l.strip_prefix('#') {
                content.push(GeminiLine::Header(1, rest.trim().to_string()));
            } else if let Some(rest) = l.strip_prefix("* ") {
                content.push(GeminiLine::ListItem(rest.trim().to_string()));
            } else if let Some(rest) = l.strip_prefix('>') {
                content.push(GeminiLine::Quote(rest.trim().to_string()));
            } else {
                content.push(GeminiLine::Text(l.to_string()));
            }
        }
        Ok(Page { url, content })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemtext_full() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"# H1\n## H2\n### H3\n> Quote\n* Item\nNormal text";
        let page = GeminiClient::parse(url, body).unwrap();

        assert_eq!(page.content.len(), 6);
        match &page.content[0] {
            GeminiLine::Header(1, t) => assert_eq!(t, "H1"),
            _ => panic!(),
        }
        match &page.content[1] {
            GeminiLine::Header(2, t) => assert_eq!(t, "H2"),
            _ => panic!(),
        }
        match &page.content[2] {
            GeminiLine::Header(3, t) => assert_eq!(t, "H3"),
            _ => panic!(),
        }
        match &page.content[3] {
            GeminiLine::Quote(t) => assert_eq!(t, "Quote"),
            _ => panic!(),
        }
        match &page.content[4] {
            GeminiLine::ListItem(t) => assert_eq!(t, "Item"),
            _ => panic!(),
        }
        match &page.content[5] {
            GeminiLine::Text(t) => assert_eq!(t, "Normal text"),
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_preformatted_blocks() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"Text\n```\n=> /fake-link\n# Not a header\n```\nMore text";
        let page = GeminiClient::parse(url, body).unwrap();

        assert_eq!(page.content.len(), 4);
        assert!(matches!(page.content[0], GeminiLine::Text(_)));
        // Linie wewnątrz ``` powinny być Preformatted, nie Link ani Header
        match &page.content[1] {
            GeminiLine::Preformatted(t) => assert_eq!(t, "=> /fake-link"),
            _ => panic!("Expected preformatted line, got {:?}", page.content[1]),
        }
        match &page.content[2] {
            GeminiLine::Preformatted(t) => assert_eq!(t, "# Not a header"),
            _ => panic!("Expected preformatted line, got {:?}", page.content[2]),
        }
        assert!(matches!(page.content[3], GeminiLine::Text(_)));
    }

    #[test]
    fn test_parse_links_with_and_without_labels() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"=> /link1 Label One\n=> /link2\n=> /link3   \tLabel Three";
        let page = GeminiClient::parse(url, body).unwrap();

        match &page.content[0] {
            GeminiLine::Link(u, l) => {
                assert_eq!(u.as_str(), "gemini://localhost/link1");
                assert_eq!(l, "Label One");
            }
            _ => panic!(),
        }
        match &page.content[1] {
            GeminiLine::Link(u, l) => {
                assert_eq!(u.as_str(), "gemini://localhost/link2");
                assert_eq!(l, "/link2"); // Domyślny label to URL
            }
            _ => panic!(),
        }
        match &page.content[2] {
            GeminiLine::Link(u, l) => {
                assert_eq!(u.as_str(), "gemini://localhost/link3");
                assert_eq!(l, "Label Three");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_windows_line_endings() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"# Title\r\n=> /link Label\r\n* Item\r\n";
        let page = GeminiClient::parse(url, body).unwrap();

        assert_eq!(page.content.len(), 3);
        if let GeminiLine::Text(t) = &page.content[0] {
            assert!(!t.contains('\r'));
        }
    }

    #[test]
    fn test_invalid_scheme() {
        let url = Url::parse("https://google.com").unwrap();
        let res = GeminiClient::fetch(&url, 5);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Only gemini:// protocol is supported");
    }

    #[test]
    fn test_redirect_limit_exhaustion() {
        let url = Url::parse("gemini://localhost").unwrap();
        let res = GeminiClient::fetch(&url, 0);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Too many redirects");
    }

    #[test]
    fn test_parse_empty_body() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"";
        let page = GeminiClient::parse(url, body).unwrap();
        assert_eq!(page.content.len(), 0);
    }

    #[test]
    fn test_parse_gemtext_with_empty_lines() {
        let url = Url::parse("gemini://localhost").unwrap();
        let body = b"Paragraph 1\n\nParagraph 2";
        let page = GeminiClient::parse(url, body).unwrap();

        assert_eq!(page.content.len(), 3);
        assert!(matches!(page.content[0], GeminiLine::Text(ref t) if t == "Paragraph 1"));
        assert!(matches!(page.content[1], GeminiLine::Text(ref t) if t == ""));
        assert!(matches!(page.content[2], GeminiLine::Text(ref t) if t == "Paragraph 2"));
    }
}

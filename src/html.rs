//! Tiny HTML -> readable text converter for syllabi and announcements.
//! Not a parser; just enough to turn Canvas rich text into something legible.

pub fn to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut chars = html.chars().peekable();
    let mut skip_until: Option<&'static str> = None;

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let lower = tag.to_ascii_lowercase();
            let name: String =
                lower.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let closing = lower.starts_with('/');

            if let Some(end) = skip_until {
                if closing && name == end {
                    skip_until = None;
                }
                continue;
            }
            match name.as_str() {
                "script" | "style" if !closing => skip_until = Some(if name == "script" { "script" } else { "style" }),
                "br" | "tr" => out.push('\n'),
                "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "table" | "blockquote" => {
                    out.push_str("\n\n")
                }
                "li" if !closing => out.push_str("\n• "),
                "td" | "th" if !closing => out.push_str("  "),
                _ => {}
            }
        } else if skip_until.is_some() {
            continue;
        } else if c == '&' {
            let mut ent = String::new();
            while let Some(&n) = chars.peek() {
                if n == ';' || ent.len() > 8 {
                    break;
                }
                ent.push(n);
                chars.next();
            }
            if chars.peek() == Some(&';') {
                chars.next();
                out.push_str(&decode_entity(&ent));
            } else {
                out.push('&');
                out.push_str(&ent);
            }
        } else if c.is_whitespace() {
            if !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    tidy(&out)
}

/// `<a href>` targets with their link text, in document order.
pub fn links(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(i) = lower[pos..].find("<a ") {
        let start = pos + i;
        let Some(tag_end) = lower[start..].find('>').map(|e| start + e) else { break };
        let tag = &html[start..tag_end];
        let close = lower[tag_end..].find("</a>").map(|e| tag_end + e).unwrap_or(tag_end + 1);
        pos = close.max(tag_end + 1);
        let href = tag.split("href=\"").nth(1).and_then(|h| h.split('"').next());
        if let Some(href) = href.filter(|h| !h.starts_with('#') && !h.starts_with("javascript:")) {
            let inner = &html[(tag_end + 1).min(close)..close];
            let mut text = to_text(inner);
            // Image links (common for "Syllabus" buttons) carry their meaning in alt text.
            if text.is_empty() {
                text = inner.split("alt=\"").nth(1).and_then(|a| a.split('"').next()).unwrap_or_default().to_string();
            }
            let text = if text.is_empty() { href.to_string() } else { text };
            out.push((text, href.replace("&amp;", "&")));
        }
    }
    out
}

/// Tidy text scraped from a rendered web page: drop icon-font ligatures
/// (`expand_more`, `menu`) and lone UI words, collapse blank runs.
pub fn clean_page_text(s: &str) -> String {
    const UI_WORDS: &[&str] = &["menu", "print", "close", "search", "download", "share", "skip to main content"];
    let kept: Vec<&str> = s
        .lines()
        .map(str::trim_end)
        .filter(|l| {
            let t = l.trim();
            let icon = !t.is_empty()
                && t.len() < 30
                && t.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
                && t.contains('_');
            !icon && !UI_WORDS.contains(&t.to_ascii_lowercase().as_str())
        })
        .collect();
    tidy(&kept.join("\n"))
}

fn decode_entity(e: &str) -> String {
    match e {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => " ".into(),
        "ndash" => "–".into(),
        "mdash" => "—".into(),
        "rsquo" | "lsquo" => "'".into(),
        "rdquo" | "ldquo" => "\"".into(),
        "hellip" => "…".into(),
        _ => {
            let n = e
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| e.strip_prefix('#').and_then(|d| d.parse().ok()));
            n.and_then(char::from_u32).map(String::from).unwrap_or_default()
        }
    }
}

/// Trim each line and collapse runs of blank lines.
fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines().map(str::trim) {
        if line.is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_links() {
        let l = super::links(
            r##"<p><a class="x" href="https://c.edu/courses/1/external_tools/2042">Simple Syllabus</a> <a href="#top">t</a></p>"##,
        );
        assert_eq!(l, vec![("Simple Syllabus".to_string(), "https://c.edu/courses/1/external_tools/2042".to_string())]);
    }

    #[test]
    fn image_links_use_alt_text() {
        let l = super::links(
            r#"<a class="x" href="/courses/1/external_tools/2042"> <img src="a.png" alt="Syllabus Student Resource Button"></a>"#,
        );
        assert_eq!(l[0].0, "Syllabus Student Resource Button");
    }

    #[test]
    fn cleans_page_chrome() {
        let t = super::clean_page_text("menu\nCOMP 181\nPrint\nexpand_more\n\n\n\nCourse Information\nCredit Hours: 1");
        assert_eq!(t, "COMP 181\n\nCourse Information\nCredit Hours: 1");
    }

    #[test]
    fn strips_basic_markup() {
        let t = super::to_text(
            "<p>Hello&nbsp;<b>world</b></p><ul><li>one</li><li>two &amp; three</li></ul><script>x()</script>",
        );
        assert_eq!(t, "Hello world\n\n• one\n• two & three");
    }
}

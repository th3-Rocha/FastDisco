//! Discord's message markdown: blocks (paragraphs, code blocks, quotes,
//! headings, lists, subtext) made of styled spans (bold, italic, underline,
//! strikethrough, inline code, spoilers, links, mentions, custom emoji and
//! timestamps).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    /// The spoiler this text is in, numbered within the message.
    pub spoiler: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String, Style),
    /// A link: the text shown and where it goes.
    Link {
        text: String,
        url: String,
        style: Style,
    },
    User(u64, Style),
    Role(u64, Style),
    Channel(u64, Style),
    /// `<:name:id>` or `<a:name:id>`.
    Emoji {
        name: String,
        id: u64,
        animated: bool,
        style: Style,
    },
    /// `<t:unix:format>`.
    Time {
        unix: i64,
        format: char,
        style: Style,
    },
    Everyone(String, Style),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Subtext(Vec<Inline>),
    Item {
        depth: u8,
        marker: String,
        content: Vec<Inline>,
    },
    Quote(Vec<Block>),
    Code {
        language: String,
        text: String,
    },
}

pub fn parse(text: &str) -> Vec<Block> {
    let mut spoilers = 0;
    blocks(text, &mut spoilers)
}

fn blocks(text: &str, spoilers: &mut usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |paragraph: &mut Vec<&str>, out: &mut Vec<Block>, spoilers: &mut usize| {
        if !paragraph.is_empty() {
            out.push(Block::Paragraph(inline(&paragraph.join("\n"), spoilers)));
            paragraph.clear();
        }
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        // ``` code blocks, possibly with a language, possibly on one line.
        if let Some(rest) = trimmed.strip_prefix("```") {
            if let Some(end) = rest.find("```") {
                flush(&mut paragraph, &mut out, spoilers);
                out.push(Block::Code {
                    language: String::new(),
                    text: rest[..end].to_owned(),
                });
                let after = rest[end + 3..].trim();
                if !after.is_empty() {
                    paragraph.push(after);
                }
                i += 1;
                continue;
            }
            let close = (i + 1..lines.len()).find(|&j| lines[j].contains("```"));
            if let Some(close) = close {
                flush(&mut paragraph, &mut out, spoilers);
                let language = rest.trim().to_owned();
                let mut body: Vec<&str> = lines[i + 1..close].to_vec();
                let last = lines[close];
                let before = &last[..last.find("```").unwrap_or(0)];
                if !before.is_empty() {
                    body.push(before);
                }
                let (language, body) = if language.contains(' ') {
                    (String::new(), format!("{language}\n{}", body.join("\n")))
                } else {
                    (language, body.join("\n"))
                };
                out.push(Block::Code {
                    language,
                    text: body,
                });
                i = close + 1;
                continue;
            }
        }
        // >>> quotes everything after it; > quotes one line.
        if let Some(rest) = line.strip_prefix(">>> ") {
            flush(&mut paragraph, &mut out, spoilers);
            let mut body = vec![rest];
            body.extend_from_slice(&lines[i + 1..]);
            out.push(Block::Quote(blocks(&body.join("\n"), spoilers)));
            break;
        }
        if line.starts_with("> ") || line == ">" {
            flush(&mut paragraph, &mut out, spoilers);
            let mut body = Vec::new();
            while i < lines.len() && (lines[i].starts_with("> ") || lines[i] == ">") {
                body.push(lines[i].strip_prefix("> ").unwrap_or(""));
                i += 1;
            }
            out.push(Block::Quote(blocks(&body.join("\n"), spoilers)));
            continue;
        }
        if let Some((level, rest)) = heading(line) {
            flush(&mut paragraph, &mut out, spoilers);
            out.push(Block::Heading(level, inline(rest, spoilers)));
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("-# ") {
            flush(&mut paragraph, &mut out, spoilers);
            out.push(Block::Subtext(inline(rest, spoilers)));
            i += 1;
            continue;
        }
        if let Some((depth, marker, rest)) = list_item(line) {
            flush(&mut paragraph, &mut out, spoilers);
            out.push(Block::Item {
                depth,
                marker,
                content: inline(rest, spoilers),
            });
            i += 1;
            continue;
        }
        paragraph.push(line);
        i += 1;
    }
    flush(&mut paragraph, &mut out, spoilers);
    out
}

fn heading(line: &str) -> Option<(u8, &str)> {
    for (prefix, level) in [("### ", 3), ("## ", 2), ("# ", 1)] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((level, rest));
        }
    }
    None
}

fn list_item(line: &str) -> Option<(u8, String, &str)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    let depth = (indent / 2).min(3) as u8;
    for bullet in ["- ", "* "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            return Some((depth, "•".into(), rest));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits < 10 && trimmed[digits..].starts_with(". ") {
        return Some((
            depth,
            trimmed[..digits + 1].to_owned(),
            &trimmed[digits + 2..],
        ));
    }
    None
}

/// Inline markup, scanned left to right with a stack of open styles.
fn inline(text: &str, spoilers: &mut usize) -> Vec<Inline> {
    let mut out = Vec::new();
    parse_inline(text, Style::default(), spoilers, &mut out);
    merge(out)
}

fn parse_inline(text: &str, style: Style, spoilers: &mut usize, out: &mut Vec<Inline>) {
    let mut plain = String::new();
    let mut i = 0;
    let bytes = text.as_bytes();
    let push_plain = |plain: &mut String, out: &mut Vec<Inline>| {
        if !plain.is_empty() {
            out.push(Inline::Text(std::mem::take(plain), style));
        }
    };
    while i < text.len() {
        let rest = &text[i..];
        // Escapes.
        if rest.starts_with('\\')
            && let Some(c) = rest[1..].chars().next()
            && "*_~`|\\<>[]#-".contains(c)
        {
            plain.push(c);
            i += 1 + c.len_utf8();
            continue;
        }
        if rest.starts_with('`') {
            let ticks = if rest.starts_with("``") { 2 } else { 1 };
            let fence = &rest[..ticks];
            if let Some(end) = rest[ticks..].find(fence) {
                push_plain(&mut plain, out);
                let code = rest[ticks..ticks + end].trim_matches(' ');
                out.push(Inline::Text(
                    code.to_owned(),
                    Style {
                        code: true,
                        ..style
                    },
                ));
                i += ticks * 2 + end;
                continue;
            }
        }
        // Paired delimiters, longest first.
        let mut matched = false;
        for (delimiter, apply) in DELIMITERS {
            if let Some(after) = rest.strip_prefix(delimiter)
                && let Some(end) = closing(after, delimiter)
            {
                push_plain(&mut plain, out);
                let mut inner = style;
                if *delimiter == "||" {
                    inner.spoiler = Some(*spoilers);
                    *spoilers += 1;
                } else {
                    apply(&mut inner);
                }
                parse_inline(&after[..end], inner, spoilers, out);
                i += delimiter.len() * 2 + end;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        if rest.starts_with('<')
            && let Some(end) = rest.find('>')
            && let Some(token) = angle(&rest[1..end], style)
        {
            push_plain(&mut plain, out);
            out.push(token);
            i += end + 1;
            continue;
        }
        if rest.starts_with('[')
            && let Some(close) = rest.find("](")
            && let Some(paren) = rest[close + 2..].find(')')
        {
            let url = &rest[close + 2..close + 2 + paren];
            if url.starts_with("http://") || url.starts_with("https://") {
                push_plain(&mut plain, out);
                out.push(Inline::Link {
                    text: rest[1..close].to_owned(),
                    url: url.to_owned(),
                    style,
                });
                i += close + 3 + paren;
                continue;
            }
        }
        if (rest.starts_with("https://") || rest.starts_with("http://"))
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
        {
            let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let mut url = &rest[..end];
            // Trailing punctuation belongs to the sentence, not the link.
            while url.ends_with(['.', ',', ')', '!', '?', ':', ';', '"', '\'']) && url.len() > 8 {
                url = &url[..url.len() - 1];
            }
            push_plain(&mut plain, out);
            out.push(Inline::Link {
                text: url.to_owned(),
                url: url.to_owned(),
                style,
            });
            i += url.len();
            continue;
        }
        for word in ["@everyone", "@here"] {
            if rest.starts_with(word) {
                push_plain(&mut plain, out);
                out.push(Inline::Everyone(word.to_owned(), style));
                i += word.len();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        plain.push(c);
        i += c.len_utf8();
    }
    push_plain(&mut plain, out);
}

type Apply = fn(&mut Style);

const DELIMITERS: &[(&str, Apply)] = &[
    ("***", |s| {
        s.bold = true;
        s.italic = true;
    }),
    ("**", |s| s.bold = true),
    ("__", |s| s.underline = true),
    ("~~", |s| s.strike = true),
    ("||", |_| {}),
    ("*", |s| s.italic = true),
    ("_", |s| s.italic = true),
];

/// Where a delimiter closes: not right after the opening, not across a
/// space-padded single `*`.
fn closing(after: &str, delimiter: &str) -> Option<usize> {
    if after.starts_with(' ') && delimiter.len() == 1 {
        return None;
    }
    let mut from = 0;
    while let Some(found) = after[from..].find(delimiter) {
        let at = from + found;
        if at > 0
            && !after[..at].ends_with('\\')
            && !(delimiter.len() == 1 && after[..at].ends_with(' '))
        {
            // `_` only counts at word edges.
            if delimiter == "_"
                && after[at + 1..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric)
            {
                from = at + 1;
                continue;
            }
            return Some(at);
        }
        from = at + delimiter.len();
    }
    None
}

fn angle(inner: &str, style: Style) -> Option<Inline> {
    if let Some(id) = inner.strip_prefix("@&") {
        return Some(Inline::Role(id.parse().ok()?, style));
    }
    if let Some(id) = inner.strip_prefix("@!").or_else(|| inner.strip_prefix('@')) {
        return Some(Inline::User(id.parse().ok()?, style));
    }
    if let Some(id) = inner.strip_prefix('#') {
        return Some(Inline::Channel(id.parse().ok()?, style));
    }
    if let Some(rest) = inner.strip_prefix("t:") {
        let mut parts = rest.split(':');
        let unix = parts.next()?.parse().ok()?;
        let format = parts.next().and_then(|f| f.chars().next()).unwrap_or('f');
        return Some(Inline::Time {
            unix,
            format,
            style,
        });
    }
    // <https://...> is a link without a preview.
    if inner.starts_with("http://") || inner.starts_with("https://") {
        return Some(Inline::Link {
            text: inner.to_owned(),
            url: inner.to_owned(),
            style,
        });
    }
    let (animated, rest) = match inner.strip_prefix("a:") {
        Some(rest) => (true, rest),
        None => (false, inner.strip_prefix(':')?),
    };
    let (name, id) = rest.split_once(':')?;
    Some(Inline::Emoji {
        name: name.to_owned(),
        id: id.parse().ok()?,
        animated,
        style,
    })
}

/// Joins neighbouring text of the same style.
fn merge(list: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(list.len());
    for item in list {
        if let (Some(Inline::Text(last, last_style)), Inline::Text(text, style)) =
            (out.last_mut(), &item)
            && last_style == style
        {
            last.push_str(text);
            continue;
        }
        out.push(item);
    }
    out
}

/// Whether a message is only emoji (and at most 30), which Discord shows large.
pub fn only_emoji(blocks: &[Block]) -> bool {
    let [Block::Paragraph(spans)] = blocks else {
        return false;
    };
    let mut count = 0;
    for span in spans {
        match span {
            Inline::Emoji { .. } => count += 1,
            Inline::Text(text, style) if !style.code && style.spoiler.is_none() => {
                for c in text.chars().filter(|c| !c.is_whitespace()) {
                    // Variation selectors and joiners belong to the emoji before.
                    if matches!(c, '\u{fe0f}' | '\u{200d}' | '\u{1f3fb}'..='\u{1f3ff}' | '\u{e0020}'..='\u{e007f}')
                    {
                        continue;
                    }
                    if is_emoji_char(c) {
                        count += 1;
                    } else {
                        return false;
                    }
                }
            }
            _ => return false,
        }
    }
    count > 0 && count <= 30
}

fn is_emoji_char(c: char) -> bool {
    matches!(c,
        '\u{1f000}'..='\u{1faff}' | '\u{2600}'..='\u{27bf}' | '\u{2300}'..='\u{23ff}' | '\u{2b00}'..='\u{2bff}'
        | '\u{3030}' | '\u{303d}' | '\u{3297}' | '\u{3299}' | '\u{a9}' | '\u{ae}'
        | '\u{2122}' | '\u{2190}'..='\u{21ff}' | '\u{2934}' | '\u{2935}' | '\u{20e3}' | '#' | '*' | '0'..='9')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str, style: Style) -> Inline {
        Inline::Text(s.into(), style)
    }

    #[test]
    fn bold_italic_and_code() {
        let bold = Style {
            bold: true,
            ..Default::default()
        };
        let code = Style {
            code: true,
            ..Default::default()
        };
        assert_eq!(
            parse("a **b** `c`"),
            vec![Block::Paragraph(vec![
                text("a ", Style::default()),
                text("b", bold),
                text(" ", Style::default()),
                text("c", code)
            ])]
        );
    }

    #[test]
    fn snake_case_is_not_italic() {
        assert_eq!(
            parse("foo_bar_baz"),
            vec![Block::Paragraph(vec![text(
                "foo_bar_baz",
                Style::default()
            )])]
        );
    }

    #[test]
    fn code_block_with_language() {
        assert_eq!(
            parse("```rust\nfn main() {}\n```"),
            vec![Block::Code {
                language: "rust".into(),
                text: "fn main() {}".into()
            }]
        );
    }

    #[test]
    fn links_mentions_and_emoji() {
        let blocks = parse("oi <@123> veja https://x.com/a. <:pog:456>");
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        assert!(spans.contains(&Inline::User(123, Style::default())));
        assert!(
            spans
                .iter()
                .any(|s| matches!(s, Inline::Link { url, .. } if url == "https://x.com/a"))
        );
        assert!(
            spans
                .iter()
                .any(|s| matches!(s, Inline::Emoji { id: 456, .. }))
        );
    }

    #[test]
    fn spoilers_are_numbered() {
        let blocks = parse("||a|| e ||b||");
        let Block::Paragraph(spans) = &blocks[0] else {
            panic!()
        };
        let numbers: Vec<_> = spans
            .iter()
            .filter_map(|s| match s {
                Inline::Text(_, style) => style.spoiler,
                _ => None,
            })
            .collect();
        assert_eq!(numbers, vec![0, 1]);
    }

    #[test]
    fn quotes_and_lists() {
        let blocks = parse("> citado\n- item\ntexto");
        assert!(matches!(blocks[0], Block::Quote(_)));
        assert!(matches!(blocks[1], Block::Item { .. }));
        assert!(matches!(blocks[2], Block::Paragraph(_)));
    }

    #[test]
    fn emoji_only() {
        assert!(only_emoji(&parse("😀 <:a:1>")));
        assert!(!only_emoji(&parse("oi 😀")));
    }
}

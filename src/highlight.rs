//! Colours for code, as Discord's highlight.js gives code blocks and text
//! file previews: keywords, strings, comments and numbers, by a small
//! scanner that knows each common language's keywords and comment marks.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

/// The language of a file name's extension or a code block's tag.
pub fn language(name: &str) -> Option<&'static Language> {
    let tag = name.rsplit('.').next()?.to_ascii_lowercase();
    LANGUAGES.iter().find(|l| l.tags.contains(&tag.as_str()))
}

/// Whether a file is text a preview can show: by its type, else its name.
pub fn is_text(name: &str, content_type: Option<&str>) -> bool {
    let kind = content_type.unwrap_or_default();
    kind.starts_with("text/")
        || ["json", "xml", "javascript", "x-sh", "sql", "yaml", "toml"]
            .iter()
            .any(|k| kind.contains(k))
        || language(name).is_some()
        || name.rsplit('.').next().is_some_and(|ext| {
            ["txt", "log", "md", "csv", "ini", "cfg", "conf", "env"]
                .contains(&ext.to_ascii_lowercase().as_str())
        })
}

pub struct Language {
    tags: &'static [&'static str],
    keywords: &'static [&'static str],
    /// SQL's keywords are any case.
    any_case: bool,
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
}

const C_LIKE: &[&str] = &[
    "if", "else", "for", "while", "do", "return", "break", "continue", "switch", "case",
    "default", "new", "class", "struct", "enum", "public", "private", "protected", "static",
    "const", "void", "int", "long", "float", "double", "char", "bool", "true", "false", "null",
    "this", "try", "catch", "finally", "throw", "import", "package", "namespace", "using",
    "interface", "extends", "implements", "virtual", "override", "async", "await", "var",
    "let", "function", "typeof", "instanceof", "export", "from", "of", "in", "string",
    "readonly", "sealed", "abstract", "final", "nil", "func", "go", "defer", "type", "map",
    "chan", "select", "undefined", "yield", "get", "set", "unsigned", "auto", "include",
];

static LANGUAGES: &[Language] = &[
    Language {
        tags: &["sql", "tsql", "mysql", "pgsql", "postgresql"],
        keywords: &[
            "select", "from", "where", "and", "or", "not", "insert", "into", "values", "update",
            "set", "delete", "create", "alter", "drop", "table", "procedure", "function", "view",
            "index", "as", "begin", "end", "go", "use", "declare", "exec", "execute", "join",
            "inner", "left", "right", "outer", "full", "on", "group", "by", "order", "having",
            "top", "distinct", "union", "all", "null", "is", "in", "like", "between", "case",
            "when", "then", "else", "if", "exists", "return", "returns", "primary", "key",
            "foreign", "references", "default", "nocount", "int", "varchar", "nvarchar", "bit",
            "datetime", "date", "decimal", "with", "nolock", "asc", "desc", "limit", "offset",
            "trigger", "transaction", "commit", "rollback", "grant", "count", "sum", "max", "min",
            "getdate", "cast", "convert", "isnull", "coalesce", "output", "schema", "database",
        ],
        any_case: true,
        line_comments: &["--"],
        block_comment: Some(("/*", "*/")),
    },
    Language {
        tags: &["rs", "rust"],
        keywords: &[
            "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
            "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
            "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
            "unsafe", "use", "where", "while", "async", "await", "dyn", "Some", "None", "Ok",
            "Err",
        ],
        any_case: false,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
    },
    Language {
        tags: &["py", "pyw", "python"],
        keywords: &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "False", "finally", "for", "from", "global", "if",
            "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise",
            "return", "True", "try", "while", "with", "yield", "self", "print",
        ],
        any_case: false,
        line_comments: &["#"],
        block_comment: None,
    },
    Language {
        tags: &[
            "js", "jsx", "ts", "tsx", "mjs", "cjs", "java", "c", "h", "cpp", "hpp", "cc", "cs",
            "go", "kt", "swift", "dart", "php", "scala", "javascript", "typescript", "csharp",
            "cpp", "kotlin", "golang",
        ],
        keywords: C_LIKE,
        any_case: false,
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
    },
    Language {
        tags: &["sh", "bash", "zsh", "fish", "ps1", "shell", "powershell"],
        keywords: &[
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
            "function", "return", "in", "export", "local", "echo", "exit", "set", "unset",
        ],
        any_case: false,
        line_comments: &["#"],
        block_comment: None,
    },
    Language {
        tags: &["json", "yaml", "yml", "toml", "ini"],
        keywords: &["true", "false", "null"],
        any_case: false,
        line_comments: &["#"],
        block_comment: None,
    },
    Language {
        tags: &["html", "htm", "xml", "svg", "css", "scss", "lua", "rb"],
        keywords: &[
            "function", "end", "local", "then", "if", "else", "elseif", "return", "def", "class",
            "module", "do", "nil", "true", "false", "important",
        ],
        any_case: false,
        line_comments: &["--"],
        block_comment: Some(("<!--", "-->")),
    },
];

/// The theme's colours for each kind of token.
struct Colors {
    keyword: Color32,
    string: Color32,
    comment: Color32,
    number: Color32,
}

fn colors(dark: bool) -> Colors {
    if dark {
        Colors {
            keyword: Color32::from_rgb(0xf3, 0x78, 0x80),
            string: Color32::from_rgb(0x7e, 0xc6, 0x99),
            comment: Color32::from_rgb(0x8a, 0x8e, 0x95),
            number: Color32::from_rgb(0xf0, 0xb2, 0x32),
        }
    } else {
        Colors {
            keyword: Color32::from_rgb(0xc5, 0x22, 0x2b),
            string: Color32::from_rgb(0x1a, 0x7f, 0x37),
            comment: Color32::from_rgb(0x6a, 0x73, 0x7d),
            number: Color32::from_rgb(0xa3, 0x5b, 0x00),
        }
    }
}

/// `code` laid out in `font`, coloured for `language` (plain without one).
pub fn job(
    code: &str,
    language: Option<&Language>,
    font: FontId,
    base: Color32,
    dark: bool,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = TextFormat {
        font_id: font,
        color: base,
        ..Default::default()
    };
    let Some(language) = language else {
        job.append(code, 0.0, plain);
        return job;
    };
    let colors = colors(dark);
    let tinted = |color: Color32| TextFormat {
        color,
        ..plain.clone()
    };
    let bytes = code.as_bytes();
    let mut at = 0;
    let mut plain_from = 0;
    let flush = |job: &mut LayoutJob, from: usize, to: usize, format: TextFormat| {
        if to > from {
            job.append(&code[from..to], 0.0, format);
        }
    };
    while at < bytes.len() {
        let rest = &code[at..];
        let token: Option<(usize, Color32)> = if language
            .line_comments
            .iter()
            .any(|m| rest.starts_with(m))
        {
            Some((rest.find('\n').unwrap_or(rest.len()), colors.comment))
        } else if let Some((open, close)) = language.block_comment.filter(|(o, _)| rest.starts_with(o)) {
            let end = rest[open.len()..]
                .find(close)
                .map_or(rest.len(), |i| open.len() + i + close.len());
            Some((end, colors.comment))
        } else if let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'' | '`')) {
            let mut end = rest.len();
            let mut escaped = false;
            for (i, c) in rest.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    end = i + 1;
                    break;
                } else if c == '\n' && quote != '`' {
                    end = i;
                    break;
                }
            }
            Some((end, colors.string))
        } else if rest.starts_with(|c: char| c.is_ascii_digit())
            && !code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_')
        {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
                .unwrap_or(rest.len());
            Some((end, colors.number))
        } else if rest.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && !code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_')
        {
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let word = &rest[..end];
            let keyword = if language.any_case {
                language.keywords.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                language.keywords.contains(&word)
            };
            if keyword {
                Some((end, colors.keyword))
            } else {
                at += end;
                continue;
            }
        } else {
            None
        };
        match token {
            Some((length, color)) => {
                flush(&mut job, plain_from, at, plain.clone());
                flush(&mut job, at, at + length, tinted(color));
                at += length;
                plain_from = at;
            }
            None => at += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    flush(&mut job, plain_from, bytes.len(), plain);
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colored(code: &str, file: &str) -> Vec<(String, Color32)> {
        let job = job(code, language(file), FontId::monospace(14.0), Color32::WHITE, true);
        job.sections
            .iter()
            .map(|s| (job.text[s.byte_range.start.0..s.byte_range.end.0].to_owned(), s.format.color))
            .collect()
    }

    #[test]
    fn sql_keywords_strings_and_comments() {
        let parts = colored("CREATE PROCEDURE [dbo].x AS -- note\nSELECT 'a' FROM t", "a.sql");
        let keyword = colors(true).keyword;
        let of = |text: &str| parts.iter().find(|(t, _)| t == text).map(|(_, c)| *c);
        assert_eq!(of("CREATE"), Some(keyword));
        assert_eq!(of("PROCEDURE"), Some(keyword));
        assert_eq!(of("-- note"), Some(colors(true).comment));
        assert_eq!(of("'a'"), Some(colors(true).string));
        let text: String = parts.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "CREATE PROCEDURE [dbo].x AS -- note\nSELECT 'a' FROM t");
    }

    #[test]
    fn words_inside_names_stay_plain() {
        let parts = colored("selected = 1", "a.sql");
        assert_eq!(parts[0].1, Color32::WHITE);
    }

    #[test]
    fn text_files() {
        assert!(is_text("x.sql", None));
        assert!(is_text("notes", Some("text/plain; charset=utf-8")));
        assert!(!is_text("photo.png", Some("image/png")));
    }
}

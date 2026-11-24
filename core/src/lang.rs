//! Language detection by file extension and a small tokenizer for code colouring.
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tok {
    Plain,
    Keyword,
    Str,
    Num,
    Comment,
    Func,
    Type,
}

pub struct Lang {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
    pub line_comment: &'static str,
    pub block_comment: bool,
}

pub const PLAIN: Lang = Lang { name: "Plain Text", keywords: &[], line_comment: "", block_comment: false };

pub fn lang_for(path: &str) -> Lang {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Lang {
            name: "Rust",
            line_comment: "//",
            block_comment: true,
            keywords: &[
                "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
                "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
                "unsafe", "use", "where", "while", "async", "await", "dyn",
            ],
        },
        "py" => Lang {
            name: "Python",
            line_comment: "#",
            block_comment: false,
            keywords: &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally",
                "for", "from", "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
                "try", "while", "with", "yield", "self",
            ],
        },
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "json" => Lang {
            name: if ext.starts_with('t') {
                "TypeScript"
            } else if ext == "json" {
                "JSON"
            } else {
                "JavaScript"
            },
            line_comment: "//",
            block_comment: true,
            keywords: &[
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "default",
                "delete",
                "do",
                "else",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "null",
                "return",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "undefined",
                "var",
                "void",
                "while",
                "yield",
                "async",
                "await",
                "of",
                "from",
                "interface",
                "type",
            ],
        },
        "c" | "h" | "cpp" | "hpp" | "cc" | "java" | "cs" | "go" | "swift" | "kt" => Lang {
            name: match ext.as_str() {
                "java" => "Java",
                "cs" => "C#",
                "go" => "Go",
                "swift" => "Swift",
                "kt" => "Kotlin",
                "c" | "h" => "C",
                _ => "C++",
            },
            line_comment: "//",
            block_comment: true,
            keywords: &[
                "auto",
                "break",
                "case",
                "char",
                "class",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extern",
                "false",
                "float",
                "for",
                "func",
                "fun",
                "goto",
                "if",
                "import",
                "int",
                "long",
                "namespace",
                "new",
                "nil",
                "null",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "template",
                "this",
                "true",
                "typedef",
                "union",
                "unsigned",
                "using",
                "var",
                "void",
                "volatile",
                "while",
                "let",
                "val",
                "interface",
                "go",
                "defer",
                "range",
                "map",
                "chan",
                "bool",
                "string",
            ],
        },
        "sh" | "bash" | "zsh" | "toml" | "yaml" | "yml" | "rb" => Lang {
            name: match ext.as_str() {
                "toml" => "TOML",
                "yaml" | "yml" => "YAML",
                "rb" => "Ruby",
                _ => "Shell Script",
            },
            line_comment: "#",
            block_comment: false,
            keywords: &[
                "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac", "function", "return", "export", "local",
                "def", "end", "class", "module", "true", "false", "nil",
            ],
        },
        "md" | "markdown" => Lang { name: "Markdown", ..PLAIN },
        _ => PLAIN,
    }
}

/// Splits text into coloured byte ranges. ponytail: hand-rolled lexer, good for
/// common C-like/script languages; swap in a grammar engine if exact colouring matters.
pub fn tokenize(text: &str, lang: &Lang) -> Vec<(usize, usize, Tok)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let lc = lang.line_comment.as_bytes();
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind;
        if !lc.is_empty() && b[i..].starts_with(lc) {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            kind = Tok::Comment;
        } else if lang.block_comment && b[i..].starts_with(b"/*") {
            i += 2;
            while i < b.len() && !b[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(b.len());
            kind = Tok::Comment;
        } else if lang.name != "Plain Text" && lang.name != "Markdown" && (c == b'"' || c == b'\'' || c == b'`') {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            kind = Tok::Str;
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') {
                i += 1;
            }
            kind = if lang.name == "Plain Text" { Tok::Plain } else { Tok::Num };
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            kind = if lang.keywords.contains(&word) {
                Tok::Keyword
            } else if lang.keywords.is_empty() {
                Tok::Plain
            } else if b.get(i) == Some(&b'(') || b.get(i) == Some(&b'!') {
                Tok::Func
            } else if c.is_ascii_uppercase() {
                Tok::Type
            } else {
                Tok::Plain
            };
        } else {
            // advance one whole UTF-8 char
            i += text[i..].chars().next().map_or(1, |ch| ch.len_utf8());
            kind = Tok::Plain;
        }
        if i > b.len() {
            i = b.len();
        }
        // keep string/comment ends on char boundaries
        while !text.is_char_boundary(i) {
            i += 1;
        }
        match out.last_mut() {
            Some((_, end, k)) if *k == kind && *end == start => *end = i,
            _ => out.push((start, i, kind)),
        }
    }
    out
}

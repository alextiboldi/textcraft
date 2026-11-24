//! TextCraft's core, with no UI dependency: documents and their files, language detection and
//! tokenizing, search helpers and fuzzy matching.
mod doc;
mod lang;
mod search;

pub use doc::{read_text, split_eol, Doc, Eol};
pub use lang::{lang_for, tokenize, Lang, Tok, PLAIN};
pub use search::{byte_of_line, char_idx, find_all, fuzzy, index_folder};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helpers() {
        assert_eq!(find_all("Foo foo FOO", "foo", false).len(), 3);
        assert_eq!(find_all("Foo foo FOO", "foo", true), vec![(4, 7)]);
        assert_eq!(byte_of_line("a\nbc\nd", 3), 5);
        assert_eq!(char_idx("é a", 3), 2);
        assert!(fuzzy("src/main.rs", "mrs").is_some());
        assert!(fuzzy("src/main.rs", "zz").is_none());
        let toks = tokenize("fn x() { \"s\" } // c", &lang_for("a.rs"));
        assert_eq!(toks[0].2, Tok::Keyword);
        assert!(toks.iter().any(|t| t.2 == Tok::Str));
        assert_eq!(toks.last().unwrap().2, Tok::Comment);
    }
}

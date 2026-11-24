//! textcraft-core's integration tests: one binary per crate (AGENTS.md rule 30).
use craft_testkit::hostile::{hammer, HammerConfig};
use textcraft_core::{is_stage_name, lang_for, split_eol, tokenize};

/// Small sources in each language `lang_for` knows, written for this test. Together they reach
/// every branch of the tokenizer: line and block comments, the three quote kinds with escapes,
/// numbers, keywords, calls, type names and characters outside ASCII.
const SEEDS: &[(&str, &str)] = &[
    ("a.rs", "/* block\n * comment */ fn main() { let s = \"a\\\"b\\n\"; let c = 'x'; println!(\"{s}\"); } // tail é\nstruct Point<'a> { x: f64 } const N: u32 = 1_000;\n"),
    ("a.py", "# comment\ndef f(x='é', y=\"\\\\\"):\n    return 0x1F + 3.5e-2  # tail\nclass C: pass\n"),
    ("a.ts", "// c\nconst t = `tmpl ${x}`; /* unclosed\ninterface A { b: number }\nlet n = 12.5; f(n);\n"),
    ("a.c", "#include <stdio.h>\nint main(void) { char *s = \"日本\\0\"; return sizeof(int); } /* end */\n"),
    ("a.sh", "#!/bin/sh\nif [ \"$1\" = 'x' ]; then echo \"ok\"; fi # done\n"),
    ("a.md", "# Title\nSome *text* with `code` and 'quotes' — and 42 numbers.\n"),
    ("a.txt", "plain words 123 \"quoted\" /* not a comment */ ünïcödé\n"),
];

/// Damaged sources, as `read_text` would hand them over (invalid UTF-8 replaced), never make the
/// tokenizer panic or hang, and its tokens always tile the text on character boundaries, since the
/// editor slices the text by them.
#[test]
fn the_tokenizer_survives_damaged_sources() {
    for &(name, seed) in SEEDS {
        hammer(&HammerConfig::new(name, 2_000), seed.as_bytes(), move |bytes| {
            let text = String::from_utf8_lossy(bytes);
            let mut end = 0;
            for (s, e, _) in tokenize(&text, &lang_for(name)) {
                assert!(s == end && s < e, "{name}: token {s}..{e} after {end}");
                let _ = &text[s..e];
                end = e;
            }
            assert_eq!(end, text.len(), "{name}: the tokens cover the whole text");
        });
    }
}

/// Line-end detection, on damaged files as `read_text` decodes them, never panics or hangs, leaves
/// no CR in the editor's text, and gives back the decoded text when the file used one style.
#[test]
fn line_end_detection_survives_damaged_files() {
    for (name, seed) in [("lf", "a\nb\n"), ("crlf", "a\r\nb\r\n"), ("cr", "a\rb\r"), ("mixed", "a\r\nb\nc\r\r\nd é\n")] {
        hammer(&HammerConfig::new(name, 2_000), seed.as_bytes(), move |bytes| {
            let read = String::from_utf8_lossy(bytes);
            let (text, eol, mixed) = split_eol(&read);
            assert!(!text.contains('\r'), "{name}: a CR is left in the text");
            if !mixed {
                assert_eq!(text.replace('\n', eol.as_str()), read, "{name}: one style round-trips");
            }
        });
    }
}

/// The stage-name check, on damaged folder entries, never panics or hangs, and never accepts a name
/// outside the stage pattern's hidden `.craft_tmp` shape, since what it accepts may be deleted.
#[test]
fn the_stage_name_check_survives_damaged_names() {
    for seed in [".notes.txt.4242-0.craft_tmp", ".a.b.c.1-2.craft_tmp", ".é.9-9.craft_tmp"] {
        hammer(&HammerConfig::new(seed, 2_000), seed.as_bytes(), |bytes| {
            let name = String::from_utf8_lossy(bytes);
            if is_stage_name(&name) {
                assert!(name.starts_with('.') && name.ends_with(".craft_tmp") && name.len() >= ".x.1-1.craft_tmp".len(), "{name:?}");
            }
        });
    }
}

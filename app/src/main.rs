// TextCraft: a small dark code editor (tabs, folder sidebar, code colouring,
// line numbers, minimap, find & replace, go to line, command palette).
use craft_dialogs::{Answer, Dialogs, FileDialog, Message, MessageButtons, MessageLevel, MessageResult};
use craft_keymap::{egui::Dispatcher, Active, Bindable, Binding, CommandRef, Keymap, Scope, Scopes};
use craft_settings::{Location, SaveStatus, SettingsStore};
use serde::{Deserialize, Serialize};
use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use craft_ui::capture::CaptureSlot;
use eframe::egui::{
    self, text::LayoutJob, Color32, FontFamily, FontId, Key, Pos2, Rect, RichText, Sense, Stroke,
    TextFormat, Vec2,
};
use egui::text::{CCursor, CCursorRange};
use lucide_icons::Icon;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---------- palette (original, not taken from any product) ----------
const BG: Color32 = Color32::from_rgb(0x1e, 0x21, 0x27);
const BG_SIDE: Color32 = Color32::from_rgb(0x17, 0x19, 0x1e);
const BG_TABBAR: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
const BG_LINE: Color32 = Color32::from_rgb(0x26, 0x2a, 0x32);
const FG: Color32 = Color32::from_rgb(0xd4, 0xd7, 0xdd);
const FG_DIM: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
const ACCENT: Color32 = Color32::from_rgb(0xf2, 0xa6, 0x3b);
const C_KEYWORD: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x9f);
const C_STRING: Color32 = Color32::from_rgb(0xb5, 0xd3, 0x6b);
const C_NUMBER: Color32 = Color32::from_rgb(0xc3, 0x9a, 0xf2);
const C_COMMENT: Color32 = Color32::from_rgb(0x6f, 0x77, 0x86);
const C_FUNC: Color32 = Color32::from_rgb(0x6c, 0xc4, 0xe8);
const C_TYPE: Color32 = Color32::from_rgb(0xf0, 0xc6, 0x74);
const C_MATCH: Color32 = Color32::from_rgba_premultiplied(0x60, 0x50, 0x10, 0xa0);

fn icon(i: Icon) -> String {
    i.unicode().to_string()
}

/// Frameless icon buttons (tab close, new tab, side bar toggle).
const PLAIN_ICON: IconButtonStyle = {
    let normal = ButtonVisuals { fill: Color32::TRANSPARENT, stroke: Stroke::NONE, icon: FG_DIM };
    IconButtonStyle {
        normal,
        hovered: ButtonVisuals { fill: BG_LINE, icon: FG, ..normal },
        pressed: None,
        selected: ButtonVisuals { icon: FG, ..normal },
        corner_radius: egui::CornerRadius::same(4),
        focus_stroke: Stroke { width: 1.0, color: FG },
    }
};

/// Icon buttons framed like egui's own buttons, for rows of framed buttons (find bar).
fn framed_icon(ui: &egui::Ui) -> IconButtonStyle {
    let (w, focus) = (&ui.visuals().widgets, ui.visuals().selection.stroke);
    let v = |s: &egui::style::WidgetVisuals| ButtonVisuals { fill: s.weak_bg_fill, stroke: s.bg_stroke, icon: s.fg_stroke.color };
    IconButtonStyle { normal: v(&w.inactive), hovered: v(&w.hovered), pressed: Some(v(&w.active)), selected: v(&w.active), corner_radius: w.inactive.corner_radius, focus_stroke: focus }
}

/// An icon-only button: `glyph` (a Lucide font glyph) drawn centred, `label` for screen readers.
fn icon_button(ui: &mut egui::Ui, style: &IconButtonStyle, label: &str, glyph: Icon, size: f32) -> egui::Response {
    let side = ui.spacing().interact_size.y;
    IconButton::new(label, Vec2::splat(side), style).show(ui, |ui, rect, tint| {
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon(glyph), FontId::proportional(size), tint);
    })
}

// ---------- translations (craft-i18n; English source text is the key) ----------
include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs")); // CRAFT_I18N, built by build.rs

static I18N: std::sync::LazyLock<craft_i18n::I18n> = std::sync::LazyLock::new(|| {
    // ponytail: offers every embedded left-to-right locale; TextCraft has no right-to-left layout yet.
    let offered: Vec<&str> = CRAFT_I18N.iter().map(|l| l.0).filter(|c| craft_i18n::direction(c) != craft_i18n::Direction::Rtl).collect();
    let (i18n, problems) = craft_i18n::I18n::new(CRAFT_I18N, &offered);
    problems.iter().for_each(|p| eprintln!("translation catalog: {p}"));
    i18n
});

thread_local! { static CURRENT: std::cell::Cell<craft_i18n::Lang> = const { std::cell::Cell::new(craft_i18n::Lang::EN) }; }

/// English `$s` in the interface language.
macro_rules! tl { ($s:expr) => { I18N.t(CURRENT.get(), $s) }; }
/// A command's label by its id.
macro_rules! tid { ($id:expr, $label:expr) => { I18N.tid(CURRENT.get(), $id, $label) }; }
/// A count's wording, `{n}` filled: `trn!(n, "{n} match", "{n} matches")`.
macro_rules! trn { ($n:expr, $one:expr, $other:expr) => { I18N.trn(CURRENT.get(), $n as u64, $one, $other) }; }
/// `{name}` placeholders of a translated message filled in.
macro_rules! tf { ($s:expr, $($k:ident = $v:expr),+ $(,)?) => { craft_i18n::fmt(tl!($s), &[$((stringify!($k), &$v.to_string())),+]) }; }

/// A number as the region writes it, isolated left to right.
fn num(n: usize) -> String {
    craft_i18n::ltr(&I18N.format().integer(n as u64))
}

// ---------- syntax colouring ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Tok {
    Plain,
    Keyword,
    Str,
    Num,
    Comment,
    Func,
    Type,
}

fn tok_color(t: Tok) -> Color32 {
    match t {
        Tok::Plain => FG,
        Tok::Keyword => C_KEYWORD,
        Tok::Str => C_STRING,
        Tok::Num => C_NUMBER,
        Tok::Comment => C_COMMENT,
        Tok::Func => C_FUNC,
        Tok::Type => C_TYPE,
    }
}

struct Lang {
    name: &'static str,
    keywords: &'static [&'static str],
    line_comment: &'static str,
    block_comment: bool,
}

const PLAIN: Lang = Lang { name: "Plain Text", keywords: &[], line_comment: "", block_comment: false };

fn lang_for(path: &str) -> Lang {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "rs" => Lang { name: "Rust", line_comment: "//", block_comment: true, keywords: &[
            "as","break","const","continue","crate","else","enum","extern","false","fn","for","if","impl","in","let","loop","match","mod","move","mut","pub","ref","return","self","Self","static","struct","super","trait","true","type","unsafe","use","where","while","async","await","dyn"] },
        "py" => Lang { name: "Python", line_comment: "#", block_comment: false, keywords: &[
            "and","as","assert","async","await","break","class","continue","def","del","elif","else","except","False","finally","for","from","global","if","import","in","is","lambda","None","nonlocal","not","or","pass","raise","return","True","try","while","with","yield","self"] },
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "json" => Lang { name: if ext.starts_with('t') { "TypeScript" } else if ext == "json" { "JSON" } else { "JavaScript" }, line_comment: "//", block_comment: true, keywords: &[
            "break","case","catch","class","const","continue","default","delete","do","else","export","extends","false","finally","for","function","if","import","in","instanceof","let","new","null","return","super","switch","this","throw","true","try","typeof","undefined","var","void","while","yield","async","await","of","from","interface","type"] },
        "c" | "h" | "cpp" | "hpp" | "cc" | "java" | "cs" | "go" | "swift" | "kt" => Lang { name: match ext.as_str() { "java" => "Java", "cs" => "C#", "go" => "Go", "swift" => "Swift", "kt" => "Kotlin", "c" | "h" => "C", _ => "C++" }, line_comment: "//", block_comment: true, keywords: &[
            "auto","break","case","char","class","const","continue","default","do","double","else","enum","extern","false","float","for","func","fun","goto","if","import","int","long","namespace","new","nil","null","package","private","protected","public","return","short","signed","sizeof","static","struct","switch","template","this","true","typedef","union","unsigned","using","var","void","volatile","while","let","val","interface","go","defer","range","map","chan","bool","string"] },
        "sh" | "bash" | "zsh" | "toml" | "yaml" | "yml" | "rb" => Lang { name: match ext.as_str() { "toml" => "TOML", "yaml" | "yml" => "YAML", "rb" => "Ruby", _ => "Shell Script" }, line_comment: "#", block_comment: false, keywords: &[
            "if","then","else","elif","fi","for","in","do","done","while","case","esac","function","return","export","local","def","end","class","module","true","false","nil"] },
        "md" | "markdown" => Lang { name: "Markdown", ..PLAIN },
        _ => PLAIN,
    }
}

/// Splits text into coloured byte ranges. ponytail: hand-rolled lexer, good for
/// common C-like/script languages; swap in a grammar engine if exact colouring matters.
fn tokenize(text: &str, lang: &Lang) -> Vec<(usize, usize, Tok)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let lc = lang.line_comment.as_bytes();
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind;
        if !lc.is_empty() && b[i..].starts_with(lc) {
            while i < b.len() && b[i] != b'\n' { i += 1; }
            kind = Tok::Comment;
        } else if lang.block_comment && b[i..].starts_with(b"/*") {
            i += 2;
            while i < b.len() && !b[i..].starts_with(b"*/") { i += 1; }
            i = (i + 2).min(b.len());
            kind = Tok::Comment;
        } else if lang.name != "Plain Text" && lang.name != "Markdown" && (c == b'"' || c == b'\'' || c == b'`') {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != b'\n' {
                if b[i] == b'\\' { i += 1; }
                i += 1;
            }
            i = (i + 1).min(b.len());
            kind = Tok::Str;
        } else if c.is_ascii_digit() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_') { i += 1; }
            kind = if lang.name == "Plain Text" { Tok::Plain } else { Tok::Num };
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') { i += 1; }
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
        if i > b.len() { i = b.len(); }
        // keep string/comment ends on char boundaries
        while !text.is_char_boundary(i) { i += 1; }
        match out.last_mut() {
            Some((_, end, k)) if *k == kind && *end == start => *end = i,
            _ => out.push((start, i, kind)),
        }
    }
    out
}

/// Byte ranges of every match of `needle` (ASCII case folding keeps byte offsets stable).
fn find_all(text: &str, needle: &str, case: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return vec![];
    }
    let (h, n) = if case { (text.to_string(), needle.to_string()) } else { (text.to_ascii_lowercase(), needle.to_ascii_lowercase()) };
    h.match_indices(&n).map(|(s, m)| (s, s + m.len())).collect()
}

fn char_idx(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn byte_of_line(text: &str, line: usize) -> usize {
    if line <= 1 { return 0; }
    text.match_indices('\n').nth(line - 2).map_or(text.len(), |(i, _)| i + 1)
}

/// Loose "letters in order" match; lower score is better.
fn fuzzy(hay: &str, needle: &str) -> Option<usize> {
    let hay_l = hay.to_lowercase();
    let mut pos = 0;
    let mut gaps = 0;
    for ch in needle.to_lowercase().chars() {
        let found = hay_l[pos..].find(ch)?;
        gaps += found;
        pos += found + ch.len_utf8();
    }
    Some(gaps * 4 + hay.len())
}

// ---------- documents ----------
struct Doc {
    /// Stays the same while tabs open and close, unlike the tab index.
    id: u64,
    path: Option<PathBuf>,
    text: String,
    saved: String,
    untitled_no: usize,
}

impl Doc {
    fn new(path: Option<PathBuf>, text: String, untitled_no: usize) -> Doc {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Doc { id, path, saved: text.clone(), text, untitled_no }
    }
    fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => if self.untitled_no == 0 { tl!("Untitled").into() } else { craft_i18n::fmt_plain(tl!("Untitled {n}"), &[("n", &(self.untitled_no + 1).to_string())]) }, // also the Save As file name
        }
    }
    fn dirty(&self) -> bool {
        self.text != self.saved
    }
    fn lang(&self) -> Lang {
        match &self.path {
            Some(p) => lang_for(&p.to_string_lossy()),
            None if self.untitled_no == 0 && self.text.starts_with("//") => lang_for("x.rs"),
            None => PLAIN,
        }
    }
}

/// What to do when a dialog answers. Documents are named by `Doc::id`, since tabs can open and
/// close while a dialog is up.
enum Then {
    Open,
    OpenFolder,
    SaveAs { doc: u64, then_close: bool },
    ConfirmClose(u64),
    ConfirmQuit,
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Palette,
}

struct Editor {
    docs: Vec<Doc>,
    active: usize,
    untitled_counter: usize,
    folder: Option<PathBuf>,
    show_sidebar: bool,
    show_minimap: bool,
    font_size: f32,
    word_wrap: bool,
    // find / replace
    find_open: bool,
    replace_open: bool,
    find_text: String,
    replace_text: String,
    find_case: bool,
    find_focus: bool,
    // palette
    overlay: Overlay,
    palette_query: String,
    palette_sel: usize,
    file_index: Vec<PathBuf>,
    // editor state
    cursor_line: usize,
    cursor_col: usize,
    row_height: f32,
    scroll_to: Option<f32>,
    pending_select: Option<(usize, usize)>, // char range
    status: String,
    // screenshot mode
    shot: Option<PathBuf>,
    frame: u32,
    capture: Option<CaptureSlot>,
    quit_confirmed: bool,
    asking_quit: bool,
    logo_tex: Option<egui::TextureHandle>,
    dialogs: Dialogs<Then>,
    keymap: Keymap,
    dispatcher: Dispatcher,
    settings: SettingsStore<Prefs>,
    /// The macOS menu bar; `None` elsewhere, or when AppKit refused it (the in-window menus show then).
    native_menu: Option<craft_menu::NativeMenu>,
}

const SAMPLE: &str = r#"// Welcome to TextCraft.
// Open a file or folder from the File menu, or press Cmd+Shift+P for commands.

use std::collections::HashMap;

/// Counts how often each word appears.
fn word_counts(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

struct Note {
    title: String,
    words: usize,
}

fn main() {
    let note = Note { title: "Ideas".into(), words: 42 };
    let counts = word_counts("the quick brown fox jumps over the lazy dog");
    println!("{} has {} words", note.title, note.words);
    if counts["the"] == 2 {
        println!("found 'the' twice");
    }
}
"#;

impl Editor {
    fn new(ctx: &egui::Context, shot: Option<PathBuf>, files: Vec<PathBuf>) -> Self {
        let wake_ctx = ctx.clone();
        // Screenshots use defaults and never touch the user's config.toml, so they stay reproducible.
        let (settings, report) = SettingsStore::<Prefs>::open(if shot.is_some() { Location::Memory } else { Location::Auto });
        let view = settings.get().view.clone();
        let keymap = keymap(craft_keymap::egui::platform(ctx)).with_overrides(settings.get().shortcuts.clone());
        let mut problems = vec![];
        if let Some(path) = &report.broken_moved_to {
            problems.push(tf!("config.toml was damaged and moved to {path}; using default settings", path = craft_i18n::ltr(&path.display().to_string())));
        }
        for e in &report.section_errors {
            problems.push(tf!("settings [{section}] reset to defaults: {reason}", section = e.section, reason = e.message));
        }
        if let Some(why) = &report.read_only {
            problems.push(tf!("settings will not be saved: {reason}", reason = why));
        }
        if !keymap.problems().is_empty() {
            problems.push(trn!(keymap.problems().len(), "{n} shortcut entry in config.toml could not be used", "{n} shortcut entries in config.toml could not be used"));
        }
        let mut ed = Editor {
            docs: vec![],
            active: 0,
            untitled_counter: 0,
            folder: None,
            show_sidebar: view.show_sidebar,
            show_minimap: view.show_minimap,
            // a hand-edited size outside the menu's range falls back into it
            font_size: if view.font_size.is_nan() { ViewPrefs::default().font_size } else { view.font_size.clamp(8.0, 40.0) },
            word_wrap: view.word_wrap,
            find_open: false,
            replace_open: false,
            find_text: String::new(),
            replace_text: String::new(),
            find_case: false,
            find_focus: false,
            overlay: Overlay::None,
            palette_query: String::new(),
            palette_sel: 0,
            file_index: vec![],
            cursor_line: 1,
            cursor_col: 1,
            row_height: 17.0,
            scroll_to: None,
            pending_select: None,
            status: problems.join("; "),
            shot,
            frame: 0,
            capture: None,
            quit_confirmed: false,
            asking_quit: false,
            logo_tex: None,
            dialogs: Dialogs::new(Arc::new(move || wake_ctx.request_repaint())),
            keymap,
            dispatcher: Dispatcher::new(),
            settings,
            native_menu: None,
        };
        // `new` runs in eframe's creator closure, so this replaces winit's default menu (whose Quit
        // skips the unsaved-changes check).
        #[cfg(target_os = "macos")]
        {
            ed.native_menu = craft_menu::mac::install(ctx, &mac_menus(&ed.keymap).bar);
        }
        for f in files {
            if f.is_dir() {
                ed.folder = Some(f);
            } else {
                ed.open_path(f);
            }
        }
        if ed.folder.is_none() {
            // Launched from a terminal inside a project: show that folder.
            if let Ok(cwd) = std::env::current_dir() {
                if cwd.parent().is_some() && cwd != dirs_home() {
                    ed.folder = Some(cwd);
                }
            }
        }
        if ed.docs.is_empty() {
            ed.docs.push(Doc::new(None, SAMPLE.into(), 0));
            ed.untitled_counter = 1;
        }
        ed
    }

    fn new_file(&mut self) {
        self.docs.push(Doc::new(None, String::new(), self.untitled_counter));
        self.untitled_counter += 1;
        self.active = self.docs.len() - 1;
    }

    fn open_path(&mut self, p: PathBuf) {
        if let Some(i) = self.docs.iter().position(|d| d.path.as_ref() == Some(&p)) {
            self.active = i;
            return;
        }
        match std::fs::read(&p) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                // Replace a pristine empty untitled tab instead of piling up tabs.
                if self.docs.len() == 1 && self.docs[0].path.is_none() && !self.docs[0].dirty() && self.docs[0].untitled_no != 0 && self.docs[0].text.is_empty() {
                    self.docs.clear();
                }
                self.docs.push(Doc::new(Some(p), text, 0));
                self.active = self.docs.len() - 1;
            }
            Err(e) => self.status = craft_i18n::fmt(I18N.tc(CURRENT.get(), "@msg", "Couldn't open {name}: {e}"), &[("name", &craft_i18n::ltr(&p.display().to_string())), ("e", &e.to_string())]),
        }
    }

    /// Saves doc `idx`. Untitled docs and Save As ask for a path first, so the write happens
    /// when the dialog answers (`handle_dialogs`).
    fn save(&mut self, idx: usize, save_as: bool, then_close: bool) {
        match self.docs[idx].path.clone().filter(|_| !save_as) {
            Some(p) => self.write(idx, p, then_close),
            None => self.dialogs.ask(FileDialog::save().file_name(self.docs[idx].title()), Then::SaveAs { doc: self.docs[idx].id, then_close }),
        }
    }

    fn write(&mut self, idx: usize, path: PathBuf, then_close: bool) {
        match std::fs::write(&path, &self.docs[idx].text) {
            Ok(()) => {
                let d = &mut self.docs[idx];
                d.saved = d.text.clone();
                d.path = Some(path);
                self.status = tl!("Saved").into();
                if then_close {
                    self.remove_tab(idx);
                }
            }
            Err(e) => self.status = tf!("Could not save: {error}", error = e),
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if self.docs[idx].dirty() {
            let message = Message {
                level: MessageLevel::Warning,
                title: tl!("Unsaved changes").into(),
                description: tf!("Save changes to {name} before closing?", name = self.docs[idx].title()),
                buttons: MessageButtons::YesNoCancel,
            };
            self.dialogs.ask_message(message, Then::ConfirmClose(self.docs[idx].id));
        } else {
            self.remove_tab(idx);
        }
    }

    fn remove_tab(&mut self, idx: usize) {
        self.docs.remove(idx);
        if self.docs.is_empty() {
            self.new_file();
        }
        if self.active >= self.docs.len() || self.active > idx {
            self.active = self.active.saturating_sub(1).min(self.docs.len() - 1);
        }
    }

    /// Acts on the dialogs that answered since the last frame.
    fn handle_dialogs(&mut self, ctx: &egui::Context) {
        for (then, answer) in self.dialogs.take_answers() {
            if matches!(then, Then::ConfirmQuit) {
                self.asking_quit = false;
            }
            let tab = |id: u64| self.docs.iter().position(|d| d.id == id);
            match (then, answer) {
                (_, Answer::Unavailable(why)) => self.status = tf!("Could not show the dialog: {reason}", reason = why),
                (Then::Open, Answer::Paths(paths)) => paths.into_iter().for_each(|p| self.open_path(p)),
                (Then::OpenFolder, Answer::Paths(paths)) => {
                    if let Some(d) = paths.into_iter().next() {
                        self.folder = Some(d);
                        self.show_sidebar = true;
                        self.file_index.clear();
                    }
                }
                (Then::SaveAs { doc, then_close }, Answer::Paths(paths)) => match (tab(doc), paths.into_iter().next()) {
                    (Some(idx), Some(p)) => self.write(idx, p, then_close),
                    (None, Some(_)) => self.status = tl!("Not saved: the file was closed before you chose where to save it").into(),
                    _ => {}
                },
                // A document closed meanwhile needs nothing more.
                (Then::ConfirmClose(doc), Answer::Message(MessageResult::Yes)) => {
                    if let Some(idx) = tab(doc) {
                        self.save(idx, false, true);
                    }
                }
                (Then::ConfirmClose(doc), Answer::Message(MessageResult::No)) => {
                    if let Some(idx) = tab(doc) {
                        self.remove_tab(idx);
                    }
                }
                (Then::ConfirmQuit, Answer::Message(MessageResult::Yes)) => {
                    self.quit_confirmed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
    }

    /// The shortcut bound to a command, as this platform writes it (⇧⌘P, Ctrl+Shift+P).
    fn shortcut_text(&self, id: &str) -> String {
        let platform = self.keymap.platform();
        self.keymap.bindings(&CommandRef::new(id)).first().map(|b| b.trigger.display(platform)).unwrap_or_default()
    }

    fn goto_line(&mut self, line: usize) {
        let text = &self.docs[self.active].text;
        let b = byte_of_line(text, line);
        let c = char_idx(text, b);
        self.pending_select = Some((c, c));
        self.scroll_to = Some(((line.max(1) - 1) as f32 - 5.0).max(0.0) * self.row_height);
    }

    fn find_step(&mut self, forward: bool) {
        let text = &self.docs[self.active].text;
        let hits = find_all(text, &self.find_text, self.find_case);
        if hits.is_empty() {
            self.status = tl!("No matches.").into();
            return;
        }
        // current cursor byte offset
        let cur = byte_of_line(text, self.cursor_line) + text[byte_of_line(text, self.cursor_line)..].chars().take(self.cursor_col - 1).map(|c| c.len_utf8()).sum::<usize>();
        let pick = if forward {
            hits.iter().find(|h| h.0 >= cur).or(hits.first())
        } else {
            hits.iter().rev().find(|h| h.1 < cur).or(hits.last())
        }
        .copied()
        .unwrap();
        // select it; cursor goes to the end so the next search moves on
        let (s, e) = (char_idx(text, pick.0), char_idx(text, pick.1));
        let line = text[..pick.0].matches('\n').count() + 1;
        self.pending_select = Some((s, e));
        self.scroll_to = Some(((line as f32) - 6.0).max(0.0) * self.row_height);
        self.status = trn!(hits.len(), "{n} match", "{n} matches");
    }

    fn replace_one(&mut self) {
        // Replace the match at the current selection start, then move to the next.
        let (needle, rep, case) = (self.find_text.clone(), self.replace_text.clone(), self.find_case);
        let line = self.cursor_line;
        let col = self.cursor_col;
        let text = &mut self.docs[self.active].text;
        let cur = byte_of_line(text, line) + text[byte_of_line(text, line)..].chars().take(col - 1).map(|c| c.len_utf8()).sum::<usize>();
        // the cursor sits at the end of the last found match
        if let Some(&(s, e)) = find_all(text, &needle, case).iter().find(|h| h.1 == cur || h.0 == cur) {
            text.replace_range(s..e, &rep);
            let c = char_idx(text, s + rep.len());
            self.pending_select = Some((c, c));
        }
        self.find_step(true);
    }

    fn replace_all(&mut self) {
        let hits = find_all(&self.docs[self.active].text, &self.find_text, self.find_case);
        let rep = self.replace_text.clone();
        let text = &mut self.docs[self.active].text;
        for &(s, e) in hits.iter().rev() {
            text.replace_range(s..e, &rep);
        }
        self.status = trn!(hits.len(), "Replaced {n} occurrence", "Replaced {n} occurrences");
    }

    fn index_folder(&mut self) {
        self.file_index.clear();
        let Some(root) = self.folder.clone() else { return };
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else {
                    self.file_index.push(p);
                }
                // ponytail: capped walk; add a background indexer for huge trees.
                if self.file_index.len() > 20_000 {
                    return;
                }
            }
        }
    }

    fn open_palette(&mut self, prefix: &str) {
        self.overlay = Overlay::Palette;
        self.palette_query = prefix.into();
        self.palette_sel = 0;
        if prefix.is_empty() {
            self.index_folder();
        }
    }

    fn run_command(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::NewFile => self.new_file(),
            Cmd::Open => self.dialogs.ask(FileDialog::open_many(), Then::Open),
            Cmd::OpenFolder => self.dialogs.ask(FileDialog::folder(), Then::OpenFolder),
            Cmd::Save => self.save(self.active, false, false),
            Cmd::SaveAs => self.save(self.active, true, false),
            Cmd::SaveAll => {
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() { self.save(i, false, false); }
                }
            }
            Cmd::CloseTab => self.close_tab(self.active),
            Cmd::NextTab => self.active = (self.active + 1) % self.docs.len(),
            Cmd::PrevTab => self.active = (self.active + self.docs.len() - 1) % self.docs.len(),
            Cmd::Find => { self.find_open = true; self.replace_open = false; self.find_focus = true; }
            Cmd::Replace => { self.find_open = true; self.replace_open = true; self.find_focus = true; }
            Cmd::FindNext => self.find_step(true),
            Cmd::FindPrev => self.find_step(false),
            Cmd::GotoLine => self.open_palette(":"),
            Cmd::GotoFile => self.open_palette(""),
            Cmd::Palette => self.open_palette(">"),
            Cmd::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            Cmd::ToggleMinimap => self.show_minimap = !self.show_minimap,
            Cmd::WordWrap => self.word_wrap = !self.word_wrap,
            Cmd::Bigger => self.font_size = (self.font_size + 1.0).min(40.0),
            Cmd::Smaller => self.font_size = (self.font_size - 1.0).max(8.0),
            Cmd::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

#[derive(Clone, Copy)]
enum Cmd {
    NewFile, Open, OpenFolder, Save, SaveAs, SaveAll, CloseTab, NextTab, PrevTab,
    Find, Replace, FindNext, FindPrev, GotoLine, GotoFile, Palette,
    ToggleSidebar, ToggleMinimap, WordWrap, Bigger, Smaller, Quit,
}

/// (command, id, label, default shortcuts in craft-keymap's text form). `Cmd` is ⌘ on macOS and
/// Ctrl elsewhere; `Ctrl` is the Mac Control key. The menus refer to rows by index. `NOT_MAC`
/// replaces some rows' shortcuts on Windows and Linux.
const CMDS: &[(Cmd, &str, &str, &[&str])] = &[
    (Cmd::NewFile, "file.new", "File: New File", &["Cmd+N"]),
    (Cmd::Open, "file.open", "File: Open…", &["Cmd+O"]),
    (Cmd::OpenFolder, "file.open_folder", "File: Open Folder…", &["Cmd+Shift+O"]),
    (Cmd::Save, "file.save", "File: Save", &["Cmd+S"]),
    (Cmd::SaveAs, "file.save_as", "File: Save As…", &["Cmd+Shift+S"]),
    (Cmd::SaveAll, "file.save_all", "File: Save All", &["Cmd+Alt+S"]),
    (Cmd::CloseTab, "file.close_tab", "File: Close Tab", &["Cmd+W"]),
    (Cmd::NextTab, "view.next_tab", "View: Next Tab", &["Ctrl+Tab"]),
    (Cmd::PrevTab, "view.prev_tab", "View: Previous Tab", &["Ctrl+Shift+Tab"]),
    (Cmd::Find, "find.find", "Find: Find…", &["Cmd+F"]),
    (Cmd::Replace, "find.replace", "Find: Replace…", &["Cmd+Alt+F"]),
    (Cmd::FindNext, "find.next", "Find: Find Next", &["Cmd+G"]),
    (Cmd::FindPrev, "find.prev", "Find: Find Previous", &["Cmd+Shift+G"]),
    (Cmd::GotoLine, "goto.line", "Go to Line…", &["Ctrl+G"]),
    (Cmd::GotoFile, "goto.file", "Go to File…", &["Cmd+P"]),
    (Cmd::Palette, "palette", "Command Palette", &["Cmd+Shift+P"]),
    (Cmd::ToggleSidebar, "view.sidebar", "View: Toggle Side Bar", &["Cmd+K"]),
    (Cmd::ToggleMinimap, "view.minimap", "View: Toggle Minimap", &[]),
    (Cmd::WordWrap, "view.word_wrap", "View: Toggle Word Wrap", &["Cmd+Alt+W"]),
    // craft-keymap lets = and + stand for each other, so Cmd+= also covers Cmd++.
    (Cmd::Bigger, "view.font_bigger", "View: Increase Font Size", &["Cmd+="]),
    (Cmd::Smaller, "view.font_smaller", "View: Decrease Font Size", &["Cmd+-"]),
    (Cmd::Quit, "app.quit", "Quit", &["Cmd+Q"]),
];

/// Default shortcuts on Windows and Linux where they differ from `CMDS`, by command id. There
/// Ctrl+G is Go to Line, so Find Next and Previous take F3 and Shift+F3, and Replace adds Ctrl+H
/// (on macOS ⌘H hides the app).
const NOT_MAC: &[(&str, &[&str])] = &[
    ("find.replace", &["Cmd+Alt+F", "Cmd+H"]),
    ("find.next", &["F3"]),
    ("find.prev", &["Shift+F3"]),
];

/// The menus, as rows of `CMDS`.
const MENUS: [(&str, &[usize]); 4] = [
    ("File", &[0, 1, 2, 3, 4, 5, 6, 21]),
    ("Edit", &[9, 10, 11, 12]),
    ("Goto", &[15, 14, 13, 7, 8]),
    ("View", &[16, 17, 18, 19, 20]),
];

/// A command's label in a menu: without the palette's "File: " prefix.
fn menu_label(label: &str) -> &str {
    label.split_once(": ").map_or(label, |(_, l)| l)
}

/// The macOS menu bar: `MENUS` with the keymap's shortcuts as key equivalents, laid out by
/// craft-menu (app menu with Hide and Quit, Window, Help). Quit is the app's own command, so ⌘Q
/// asks about unsaved changes like the close button.
fn mac_menus(keymap: &Keymap) -> craft_menu::Layout {
    let rows: Vec<craft_menu::Row> = MENUS
        .iter()
        .flat_map(|&(menu, items)| items.iter().filter_map(|&i| CMDS.get(i)).map(move |&(_, id, label, _)| (menu, id, label)))
        .map(|(menu, id, label)| craft_menu::Row {
            path: vec![tl!(menu).into()],
            command: CommandRef::new(id),
            label: tl!(menu_label(label)).into(),
            shortcut: None,
            enabled: true,
            kind: craft_menu::ItemKind::Plain,
        })
        .collect();
    let mut bar = craft_menu::from_items(&MENUS.map(|m| tl!(m.0)), &rows);
    bar.tag_menu(tl!("File"), craft_menu::MenuRole::File);
    bar.tag_menu(tl!("Edit"), craft_menu::MenuRole::Edit);
    bar.tag_menu(tl!("View"), craft_menu::MenuRole::View);
    bar.tag_item(&CommandRef::new("app.quit"), craft_menu::ItemRole::Quit);
    craft_menu::apply_keymap(&mut bar, keymap);
    // craft-menu's own items (Hide {app}, Quit {app}, Window, Help) through the same catalog.
    let label = |english: &'static str| tl!(english).to_string();
    craft_menu::mac_layout(&bar, &craft_menu::LayoutOptions { label: &label, ..craft_menu::LayoutOptions::new("TextCraft") })
}

fn keymap(platform: craft_keymap::Platform) -> Keymap {
    let catalog = CMDS
        .iter()
        .map(|&(_, id, label, keys)| {
            let keys = NOT_MAC.iter().find(|o| o.0 == id && !platform.is_mac()).map_or(keys, |o| o.1);
            keys.iter().filter_map(|k| Binding::parse(k).ok()).fold(Bindable::new(id, tid!(id, label), Scope::app()), Bindable::with_default)
        })
        .collect();
    Keymap::new(catalog, Scopes::new(), platform)
}

/// What TextCraft keeps in `config.toml` between runs.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct Prefs {
    view: ViewPrefs,
    /// Shortcut changes by command id, e.g. `[[shortcuts]] command = "file.save" keys = ["F2"]`.
    #[serde(with = "craft_keymap::keymap::overrides_list")]
    shortcuts: craft_keymap::Overrides,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
struct ViewPrefs {
    show_sidebar: bool,
    show_minimap: bool,
    font_size: f32,
    word_wrap: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        ViewPrefs { show_sidebar: true, show_minimap: true, font_size: 14.0, word_wrap: false }
    }
}

impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "textcraft";
    const SCHEMA_VERSION: u32 = 1;
}

// ---------- UI ----------
impl eframe::App for Editor {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // confirm quitting with unsaved work
        if ctx.input(|i| i.viewport().close_requested()) && self.shot.is_none() && !self.quit_confirmed && self.docs.iter().any(|d| d.dirty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.asking_quit {
                self.asking_quit = true;
                let message = Message {
                    level: MessageLevel::Warning,
                    title: tl!("Unsaved changes").into(),
                    description: tl!("Some files have unsaved changes. Quit anyway?").into(),
                    buttons: MessageButtons::YesNo,
                };
                self.dialogs.ask_message(message, Then::ConfirmQuit);
            }
        }
        // last, so it sees the CancelClose above
        craft_quit::on_close_request(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(menu) = &mut self.native_menu {
            menu.raw_input(raw_input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dialogs(&ctx);

        // screenshot mode: keep drawing, grab frame ~120, then quit
        if let Some(path) = self.shot.clone() {
            self.frame += 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
            if self.frame == 120 {
                // A per-image bound well above a full-screen 5K capture.
                let (slot, callback) = CaptureSlot::new(&ctx, ctx.viewport_id(), 512 << 20);
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(callback));
                self.capture = Some(slot);
            }
            let result = self.capture.as_ref().and_then(CaptureSlot::take);
            let done = result.is_some();
            match result {
                Some(Ok(img)) => {
                    let [w, h] = img.size;
                    let raw: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(&path, &raw, w as u32, h as u32, image::ColorType::Rgba8) {
                        eprintln!("screenshot failed: {e}");
                    }
                }
                Some(Err(e)) => eprintln!("screenshot failed: {e}"),
                None => {}
            }
            if done || self.frame > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // native menu picks. Its key equivalents reach the dispatcher below as the key presses
        // they were, or (while typing) come back here as picks, never both, so each runs once.
        let mut picked = vec![];
        if let Some(menu) = &mut self.native_menu {
            let modal = self.dialogs.is_open();
            menu.set_gate(craft_menu::Gate { modal, recording_shortcut: false, text_editing: ctx.text_edit_focused() });
            picked = menu.take_clicks(&ctx);
            let keymap = &self.keymap;
            // ponytail: the menus never change while running (no enabled state, fixed keymap), so
            // the state hash is a constant; hash what they show once they can change.
            menu.sync_with(0, || mac_menus(keymap));
        }

        // keyboard shortcuts
        let active = Active { typing: ctx.memory(|m| m.focused().is_some()), ..Active::NONE };
        let hits = ctx.input_mut(|i| self.dispatcher.dispatch(i, &self.keymap, &active));
        for command in picked.into_iter().chain(hits.into_iter().map(|h| h.command)) {
            if let Some(&(cmd, ..)) = CMDS.iter().find(|c| CommandRef::new(c.1) == command) {
                self.run_command(&ctx, cmd);
            }
        }
        if self.overlay != Overlay::None && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.overlay = Overlay::None;
        } else if self.find_open && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.find_open = false;
        }

        let title = format!("{}{} — TextCraft", self.docs[self.active].title(), if self.docs[self.active].dirty() { " •" } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        if self.native_menu.is_none() {
            self.menu_bar(ui, &ctx);
        }
        self.status_bar(ui);
        if self.find_open {
            self.find_bar(ui);
        }
        if self.show_sidebar {
            self.sidebar(ui);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ui, |ui| {
            self.tab_bar(ui);
            self.editor_area(ui);
        });
        if self.overlay == Overlay::Palette {
            self.palette(&ctx);
        }
        self.dialogs.show_pending(Some(frame));

        // Save view changes half a second after the last one, however they were made.
        let view = ViewPrefs { show_sidebar: self.show_sidebar, show_minimap: self.show_minimap, font_size: self.font_size, word_wrap: self.word_wrap };
        self.settings.update(|p| p.view = view);
        let tick = self.settings.tick(std::time::Instant::now());
        if let SaveStatus::Failed(e) = &tick.status {
            self.status = e.to_string();
        }
        for path in self.settings.take_moved_aside() {
            self.status = tf!("config.toml could not be read and was moved to {path}", path = craft_i18n::ltr(&path.display().to_string()));
        }
        if let Some(deadline) = tick.next_deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(std::time::Instant::now()));
        }
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.settings.flush() {
            eprintln!("TextCraft: {e}");
        }
    }
}

impl Editor {
    fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("menu").frame(egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin::symmetric(8, 3))).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let tex = self.logo_tex.get_or_insert_with(|| {
                    let px = 64;
                    ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([px, px], &logo_rgba(px)), Default::default())
                });
                ui.add(egui::Image::new((tex.id(), Vec2::splat(18.0))));
                ui.label(RichText::new("TextCraft").strong().color(FG));
                ui.add_space(8.0);
                let mut chosen = None;
                for (name, items) in MENUS {
                    ui.menu_button(tl!(name), |ui| {
                        for &i in items {
                            let (cmd, id, label, _) = CMDS[i];
                            // The short label is its own key, so shared rows ("Open…", "Save") translate it.
                            let short = tl!(menu_label(label));
                            if ui.add(egui::Button::new(short).shortcut_text(self.shortcut_text(id))).clicked() {
                                chosen = Some(cmd);
                            }
                        }
                    });
                }
                if let Some(c) = chosen {
                    self.run_command(ctx, c);
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let lang = match self.docs[self.active].lang().name {
            // Generic names; the others are proper names and stay as written.
            "Plain Text" => tl!("Plain Text"),
            "Shell Script" => tl!("Shell Script"),
            name => name,
        };
        egui::Panel::bottom("status").frame(egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let small = |s: String| RichText::new(s).size(12.0).color(FG_DIM);
                let toggle = IconButton::new(tl!("Side bar"), Vec2::splat(18.0), &PLAIN_ICON).selected(self.show_sidebar).show(ui, |ui, rect, tint| {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon(Icon::PanelLeft), FontId::proportional(12.0), tint);
                });
                if toggle.on_hover_text(tl!("Toggle side bar")).clicked() {
                    self.show_sidebar = !self.show_sidebar;
                }
                ui.label(small(tf!("Line {line}, Column {column}", line = num(self.cursor_line), column = num(self.cursor_col))));
                if !self.status.is_empty() {
                    ui.label(small(format!("·  {}", self.status)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(small(lang.to_string()));
                    ui.label(small(tf!("Spaces: {n}", n = num(4))));
                    ui.label(small("UTF-8".into()));
                });
            });
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("find").frame(egui::Frame::NONE.fill(BG_SIDE).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| {
            let n = find_all(&self.docs[self.active].text, &self.find_text, self.find_case).len();
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.find_case, RichText::new(icon(Icon::CaseSensitive))).on_hover_text(tl!("Case Sensitive"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.find_text).hint_text(tl!("Find")).desired_width(320.0));
                if self.find_focus {
                    r.request_focus();
                    self.find_focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let back = ui.input(|i| i.modifiers.shift);
                    self.find_step(!back);
                    r.request_focus();
                }
                if ui.button(format!("{} {}", icon(Icon::ArrowDown), tl!("Find"))).clicked() { self.find_step(true); }
                if ui.button(format!("{} {}", icon(Icon::ArrowUp), tl!("Previous"))).clicked() { self.find_step(false); }
                ui.label(RichText::new(if self.find_text.is_empty() { String::new() } else { trn!(n, "{n} match", "{n} matches") }).color(FG_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, &framed_icon(ui), tl!("Close find bar"), Icon::X, 14.0).on_hover_text(tl!("Close")).clicked() { self.find_open = false; }
                    ui.toggle_value(&mut self.replace_open, icon(Icon::Replace)).on_hover_text(tl!("Show replace"));
                });
            });
            if self.replace_open {
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    ui.add(egui::TextEdit::singleline(&mut self.replace_text).hint_text(tl!("Replace")).desired_width(320.0));
                    if ui.button(format!("{} {}", icon(Icon::Replace), tl!("Replace"))).clicked() { self.replace_one(); }
                    if ui.button(format!("{} {}", icon(Icon::ReplaceAll), tl!("Replace All"))).clicked() { self.replace_all(); }
                });
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut switch_to = None;
        let mut close = None;
        egui::Panel::left("side").default_size(220.0).frame(egui::Frame::NONE.fill(BG_SIDE).inner_margin(egui::Margin::symmetric(8, 8))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new(tl!("OPEN FILES")).size(11.0).color(FG_DIM).strong());
                for (i, d) in self.docs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mark = if d.dirty() { icon(Icon::Circle) } else { icon(Icon::X) };
                        if ui.add(egui::Button::new(RichText::new(mark).size(11.0).color(FG_DIM)).frame(false)).clicked() {
                            close = Some(i);
                        }
                        let col = if i == self.active { FG } else { FG_DIM };
                        if ui.add(egui::Label::new(RichText::new(d.title()).color(col)).sense(Sense::click())).clicked() {
                            switch_to = Some(i);
                        }
                    });
                }
                if let Some(folder) = &self.folder {
                    ui.add_space(10.0);
                    ui.label(RichText::new(tl!("FOLDERS")).size(11.0).color(FG_DIM).strong());
                    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.display().to_string());
                    let active_path = self.docs[self.active].path.clone();
                    dir_tree(ui, folder, &name, true, &active_path, &mut open);
                }
            });
        });
        if let Some(i) = switch_to { self.active = i; }
        if let Some(i) = close { self.close_tab(i); }
        if let Some(p) = open { self.open_path(p); }
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch_to = None;
        let mut close = None;
        egui::Frame::NONE.fill(BG_TABBAR).inner_margin(egui::Margin { left: 4, right: 4, top: 4, bottom: 0 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, d) in self.docs.iter().enumerate() {
                        let active = i == self.active;
                        let fill = if active { BG } else { BG_SIDE };
                        let frame = egui::Frame::NONE.fill(fill).corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(12, 6));
                        let r = frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon(Icon::FileText)).size(12.0).color(if active { ACCENT } else { FG_DIM }));
                                ui.label(RichText::new(d.title()).color(if active { FG } else { FG_DIM }));
                                let (mark, label) = if d.dirty() { (Icon::Circle, tl!("Close tab, unsaved changes")) } else { (Icon::X, tl!("Close tab")) };
                                if icon_button(ui, &PLAIN_ICON, label, mark, 11.0).on_hover_text(tl!("Close")).clicked() {
                                    close = Some(i);
                                }
                            });
                        });
                        let title = d.title();
                        let tab = craft_ui::tabs::Tab::new(ui.scope_id().with(("tab", i)), &title, active).show_at(ui, r.response.rect, |ui, resp| {
                            if active {
                                ui.painter().hline(resp.rect.x_range(), resp.rect.top(), Stroke::new(2.0, ACCENT));
                            }
                        });
                        if tab.clicked() {
                            switch_to = Some(i);
                        }
                    }
                    if icon_button(ui, &PLAIN_ICON, tl!("New file"), Icon::Plus, 14.0).on_hover_text(tl!("New file")).clicked() {
                        self.new_file();
                    }
                });
            });
        });
        if let Some(i) = switch_to { self.active = i; }
        if let Some(i) = close { self.close_tab(i); }
    }

    fn editor_area(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        let mini_w = if self.show_minimap { 110.0 } else { 0.0 };
        let edit_rect = Rect::from_min_max(full.min, Pos2::new(full.max.x - mini_w, full.max.y));
        let mini_rect = Rect::from_min_max(Pos2::new(full.max.x - mini_w, full.min.y), full.max);

        let font = FontId::monospace(self.font_size);
        let lang = self.docs[self.active].lang();
        let (find_text, find_case, find_open) = (self.find_text.clone(), self.find_case, self.find_open);
        let wrap = self.word_wrap;
        let edit_id = egui::Id::unique(("editor", self.active));

        // apply a pending cursor move (go to line, find)
        if let Some((s, e)) = self.pending_select.take() {
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), edit_id) {
                st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(s), CCursor::new(e))));
                st.store(ui.ctx(), edit_id);
            }
            ui.ctx().memory_mut(|m| m.request_focus(edit_id));
        }

        let doc = &mut self.docs[self.active];
        let line_count = doc.text.matches('\n').count() + 1;
        let cur_line = self.cursor_line;
        let font_g = font.clone();

        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let text = buf.as_str();
            let mut job = LayoutJob::default();
            let matches = if find_open { find_all(text, &find_text, find_case) } else { vec![] };
            for (s, e, t) in tokenize(text, &lang) {
                // split a token where search matches start/end so they get a background
                let mut cuts = vec![s, e];
                for &(ms, me) in &matches {
                    if ms > s && ms < e { cuts.push(ms); }
                    if me > s && me < e { cuts.push(me); }
                }
                cuts.sort_unstable();
                for w in cuts.windows(2) {
                    let hit = matches.iter().any(|&(ms, me)| w[0] >= ms && w[1] <= me);
                    let fmt = TextFormat { font_id: font_g.clone(), color: tok_color(t), background: if hit { C_MATCH } else { Color32::TRANSPARENT }, ..Default::default() };
                    job.append(&text[w[0]..w[1]], 0.0, fmt);
                }
            }
            job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
            ui.painter().layout_job(job)
        };

        let mut out_cursor = None;
        let mut scroll = egui::ScrollArea::both().id_salt(("scroll", self.active)).auto_shrink([false, false]);
        if let Some(y) = self.scroll_to.take() {
            scroll = scroll.vertical_scroll_offset(y);
        }
        let gutter_w = (line_count.to_string().len().max(3) as f32) * self.font_size * 0.62 + 20.0;
        let mut row_h = self.row_height;

        let sa = ui.scope_builder(egui::UiBuilder::new().max_rect(edit_rect), |ui| {
            scroll.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let (gutter_rect, _) = ui.allocate_exact_size(Vec2::new(gutter_w, 0.0), Sense::hover());
                    // reserve a slot behind the text for the current-line highlight
                    let hl_slot = ui.painter().add(egui::Shape::Noop);
                    let out = egui::TextEdit::multiline(&mut doc.text)
                        .id(edit_id)
                        .font(font.clone())
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .lock_focus(true)
                        .desired_width(if wrap { ui.available_width() } else { f32::INFINITY })
                        .desired_rows(40)
                        .layouter(&mut layouter)
                        .show(ui);
                    out_cursor = out.cursor_range;
                    // line numbers + current line highlight, drawn row by row from the galley
                    let painter = ui.painter();
                    let mut line_no = 1;
                    let mut new_line = true;
                    for row in &out.galley.rows {
                        let y = out.galley_pos.y + row.min_y();
                        row_h = row.height();
                        if new_line {
                            let col = if line_no == cur_line { FG } else { FG_DIM };
                            if line_no == cur_line {
                                let r = Rect::from_min_size(Pos2::new(gutter_rect.min.x, y), Vec2::new(ui.clip_rect().max.x - gutter_rect.min.x, row.height()));
                                painter.set(hl_slot, egui::Shape::rect_filled(r, 0.0, BG_LINE));
                            }
                            painter.text(Pos2::new(gutter_rect.max.x - 12.0, y), egui::Align2::RIGHT_TOP, line_no.to_string(), font.clone(), col);
                        }
                        new_line = row.ends_with_newline;
                        if new_line { line_no += 1; }
                    }
                });
            })
        });
        self.row_height = row_h;
        let sa = sa.inner;

        if let Some(r) = out_cursor {
            let idx = r.primary.index.0;
            let text = &self.docs[self.active].text;
            let before: String = text.chars().take(idx).collect();
            self.cursor_line = before.matches('\n').count() + 1;
            self.cursor_col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        }

        if self.show_minimap {
            self.minimap(ui, mini_rect, &sa);
        }
    }

    fn minimap(&mut self, ui: &mut egui::Ui, rect: Rect, sa: &egui::scroll_area::ScrollAreaOutput<()>) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);
        painter.vline(rect.left(), rect.y_range(), Stroke::new(1.0, BG_LINE));
        let doc = &self.docs[self.active];
        let text = &doc.text;
        let lines = text.matches('\n').count() + 1;
        let lh = 2.0_f32; // pixels per line in the minimap
        let cw = 1.1_f32;
        let total_h = lines as f32 * lh;
        // scroll the minimap itself when the file is taller than the panel
        let content_h = sa.content_size.y.max(1.0);
        let frac = sa.state.clamped_offset().y / (content_h - sa.inner_rect.height()).max(1.0);
        let mini_off = (total_h - rect.height()).max(0.0) * frac.clamp(0.0, 1.0);
        let x0 = rect.left() + 8.0;
        let mut line = 0usize;
        let mut col = 0usize;
        // ponytail: paints every token each frame; cache per text version if big files lag.
        for (s, e, t) in tokenize(text, &doc.lang()) {
            for seg in text[s..e].split_inclusive('\n') {
                let word = seg.trim_end_matches('\n');
                let y = rect.top() + line as f32 * lh - mini_off;
                let lead = word.len() - word.trim_start().len();
                let visible = word.trim();
                if !visible.is_empty() && y > rect.top() - lh && y < rect.bottom() {
                    let x = x0 + (col + lead) as f32 * cw;
                    let w = (visible.chars().count() as f32 * cw).min(rect.right() - x - 4.0);
                    if w > 0.0 {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, lh * 0.75)), 0.0, tok_color(t).gamma_multiply(0.7));
                    }
                }
                col += word.chars().count();
                if seg.ends_with('\n') { line += 1; col = 0; }
            }
        }
        // visible-area box
        let vis_top = sa.state.clamped_offset().y / content_h * total_h;
        let vis_h = (sa.inner_rect.height() / content_h * total_h).max(8.0);
        let vr = Rect::from_min_size(Pos2::new(rect.left() + 1.0, rect.top() + vis_top - mini_off), Vec2::new(rect.width() - 1.0, vis_h));
        painter.rect_filled(vr, 0.0, Color32::from_white_alpha(14));

        let resp = ui.interact(rect, ui.scope_id().with("minimap"), Sense::click_and_drag());
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.clicked() || resp.dragged() {
                let line_at = ((p.y - rect.top() + mini_off) / lh).max(0.0);
                let y = line_at / total_h * content_h - sa.inner_rect.height() / 2.0;
                self.scroll_to = Some(y.max(0.0));
            }
        }
    }

    fn palette(&mut self, ctx: &egui::Context) {
        // items: (label, detail, action)
        enum Act { Cmd(Cmd), File(PathBuf), Line(usize) }
        let q = self.palette_query.clone();
        let mut items: Vec<(String, String, Act)> = vec![];
        if let Some(rest) = q.strip_prefix('>') {
            let mut scored: Vec<_> = CMDS.iter().filter_map(|&(c, id, l, _)| { let l = tid!(id, l); fuzzy(l, rest.trim()).map(|s| (s, c, id, l)) }).collect();
            scored.sort_by_key(|x| x.0);
            items = scored.into_iter().map(|(_, c, id, l)| (l.to_string(), self.shortcut_text(id), Act::Cmd(c))).collect();
        } else if let Some(rest) = q.strip_prefix(':') {
            let n: usize = rest.trim().parse().unwrap_or(0);
            let total = self.docs[self.active].text.matches('\n').count() + 1;
            if n > 0 {
                items.push((tf!("Go to line {n}", n = num(n.min(total))), String::new(), Act::Line(n.min(total))));
            } else {
                items.push((tf!("Type a line number between {first} and {last}", first = num(1), last = num(total)), String::new(), Act::Line(0)));
            }
        } else {
            let root = self.folder.clone().unwrap_or_default();
            let mut scored: Vec<_> = self.file_index.iter().filter_map(|p| {
                let rel = p.strip_prefix(&root).unwrap_or(p).to_string_lossy().into_owned();
                fuzzy(&rel, q.trim()).map(|s| (s, rel, p.clone()))
            }).collect();
            scored.sort_by_key(|x| x.0);
            items = scored.into_iter().take(200).map(|(_, rel, p)| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (name, rel, Act::File(p))
            }).collect();
            if self.folder.is_none() {
                items.push((tl!("Open a folder to search its files").into(), String::new(), Act::Cmd(Cmd::OpenFolder)));
            }
        }
        self.palette_sel = self.palette_sel.min(items.len().saturating_sub(1));
        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) { self.palette_sel = (self.palette_sel + 1).min(items.len().saturating_sub(1)); }
        if ctx.input(|i| i.key_pressed(Key::ArrowUp)) { self.palette_sel = self.palette_sel.saturating_sub(1); }

        let mut run: Option<usize> = None;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("palette")).order(egui::Order::Foreground).fixed_pos(Pos2::new(screen.center().x - 260.0, screen.top() + 60.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(BG_SIDE).inner_margin(8).show(ui, |ui| {
                ui.set_width(520.0);
                let r = ui.add(egui::TextEdit::singleline(&mut self.palette_query).hint_text(tl!("Search files, > for commands, : for line")).desired_width(f32::INFINITY));
                r.request_focus();
                if r.changed() { self.palette_sel = 0; }
                if ui.input(|i| i.key_pressed(Key::Enter)) && !items.is_empty() { run = Some(self.palette_sel); }
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    for (i, (label, detail, _)) in items.iter().enumerate() {
                        let sel = i == self.palette_sel;
                        let fr = egui::Frame::NONE.fill(if sel { BG_LINE } else { Color32::TRANSPARENT }).inner_margin(egui::Margin::symmetric(8, 4)).corner_radius(4);
                        let resp = fr.show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).color(if sel { ACCENT } else { FG }));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(detail).size(11.0).color(FG_DIM));
                                });
                            });
                        }).response;
                        if sel { resp.scroll_to_me(None); }
                        if ui.interact(resp.rect, ui.scope_id().with(("pi", i)), Sense::click()).clicked() { run = Some(i); }
                    }
                });
            });
        });
        if let Some(i) = run {
            let act = items.swap_remove(i).2;
            self.overlay = Overlay::None;
            match act {
                Act::Cmd(c) => self.run_command(ctx, c),
                Act::File(p) => self.open_path(p),
                Act::Line(n) if n > 0 => self.goto_line(n),
                Act::Line(_) => {}
            }
        }
    }
}

fn dir_tree(ui: &mut egui::Ui, dir: &Path, name: &str, open_default: bool, active: &Option<PathBuf>, open: &mut Option<PathBuf>) {
    egui::CollapsingHeader::new(RichText::new(format!("{} {}", icon(Icon::Folder), name)).color(FG))
        .id_salt(dir)
        .default_open(open_default)
        .show(ui, |ui| {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            let mut entries: Vec<_> = rd.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).collect();
            entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_ascii_lowercase()));
            // ponytail: re-reads the folder every frame while open; cache if it gets slow on network drives.
            for e in entries {
                let p = e.path();
                let n = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    dir_tree(ui, &p, &n, false, active, open);
                } else {
                    let is_active = active.as_ref() == Some(&p);
                    let txt = RichText::new(format!("{} {}", icon(Icon::File), n)).color(if is_active { ACCENT } else { FG_DIM });
                    if ui.add(egui::Label::new(txt).sense(Sense::click())).clicked() {
                        *open = Some(p);
                    }
                }
            }
        });
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("lucide".into(), Arc::new(egui::FontData::from_static(lucide_icons::LUCIDE_FONT_BYTES)));
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("lucide".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.visuals.panel_fill = BG_SIDE;
        s.visuals.extreme_bg_color = BG_TABBAR;
        s.visuals.selection.bg_fill = Color32::from_rgb(0x3a, 0x4a, 0x66);
        s.visuals.selection.stroke = Stroke::new(1.0, FG);
        s.visuals.text_cursor.stroke = Stroke::new(2.0, ACCENT);
        s.visuals.override_text_color = None;
        s.spacing.item_spacing = Vec2::new(6.0, 4.0);
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BG_LINE);
    });
}

/// The TextCraft logo, drawn from shapes: an amber text cursor beside three
/// coloured lines of code on a dark rounded square. Returns RGBA pixels.
fn logo_rgba(size: usize) -> Vec<u8> {
    // signed distance to a rounded box (all in 0..1 units)
    fn rbox(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (hx, hy) = ((x1 - x0) / 2.0 - r, (y1 - y0) / 2.0 - r);
        let (dx, dy) = ((x - cx).abs() - hx, (y - cy).abs() - hy);
        (dx.max(0.0).hypot(dy.max(0.0))) + dx.max(dy).min(0.0) - r
    }
    let mut out = vec![0u8; size * size * 4];
    let px = 1.0 / size as f32;
    for j in 0..size {
        for i in 0..size {
            let (x, y) = ((i as f32 + 0.5) * px, (j as f32 + 0.5) * px);
            let cover = |d: f32| (0.5 - d / px).clamp(0.0, 1.0);
            // background: vertical gradient, rounded square
            let t = y;
            let bg = [0x2c as f32 * (1.0 - t) + 0x15 as f32 * t, 0x31 as f32 * (1.0 - t) + 0x17 as f32 * t, 0x3b as f32 * (1.0 - t) + 0x1c as f32 * t];
            let a_bg = cover(rbox(x, y, 0.06, 0.06, 0.94, 0.94, 0.2));
            let mut c = [bg[0], bg[1], bg[2]];
            let mut paint = |d: f32, col: Color32| {
                let a = cover(d);
                for (ck, k) in c.iter_mut().zip(col.to_array()) { *ck = *ck * (1.0 - a) + k as f32 * a; }
            };
            // amber I-beam cursor
            let stem = rbox(x, y, 0.27, 0.24, 0.35, 0.76, 0.02);
            let top = rbox(x, y, 0.19, 0.22, 0.43, 0.29, 0.03);
            let bot = rbox(x, y, 0.19, 0.71, 0.43, 0.78, 0.03);
            paint(stem.min(top).min(bot), ACCENT);
            // three lines of code
            paint(rbox(x, y, 0.50, 0.30, 0.80, 0.38, 0.04), C_KEYWORD);
            paint(rbox(x, y, 0.56, 0.46, 0.76, 0.54, 0.04), C_FUNC);
            paint(rbox(x, y, 0.50, 0.62, 0.72, 0.70, 0.04), C_STRING);
            let o = (j * size + i) * 4;
            out[o..o + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, (a_bg * 255.0) as u8]);
        }
    }
    out
}

/// What the command line asks for.
#[derive(Default)]
struct Args {
    shot: Option<PathBuf>,
    export_logo: Option<PathBuf>,
    files: Vec<PathBuf>,
}

/// Reads the arguments after the program name. Other options are ignored rather than opened as
/// files: macOS passes `-psn_…` and user defaults as `-Name value` pairs (`-AppleLanguages "(fr)"`).
/// After `--` every argument is a file, so a name starting with `-` can still be opened.
fn parse_args(args: impl IntoIterator<Item = String>) -> Args {
    let mut args = args.into_iter();
    let mut out = Args::default();
    let mut options = true;
    while let Some(a) = args.next() {
        if !options || !a.starts_with('-') {
            out.files.push(a.into());
        } else if a == "--" {
            options = false;
        } else if a == "--export-logo" {
            out.export_logo = Some(args.next().unwrap_or_else(|| "logo.png".into()).into());
        } else if a == "--screenshot" {
            out.shot = args.next().map(PathBuf::from);
        } else if !a.starts_with("--") && !a.starts_with("-psn_") {
            // ponytail: every other single-dash option is taken as a macOS `-Name value` pair.
            args.next();
        }
    }
    out
}

fn main() -> eframe::Result {
    // The system's language; the UI runs on this thread. English when no catalog fits.
    CURRENT.set(I18N.system_lang());
    let Args { shot, export_logo, files } = parse_args(std::env::args().skip(1));
    if let Some(path) = export_logo {
        // writes the logo as a 1024px PNG, e.g. for a website or app bundle
        image::save_buffer(&path, &logo_rgba(1024), 1024, 1024, image::ColorType::Rgba8).expect("could not write logo");
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default().with_title("TextCraft").with_inner_size([1200.0, 760.0]).with_min_inner_size([500.0, 300.0])
        .with_icon(Arc::new(egui::IconData { rgba: logo_rgba(256), width: 256, height: 256 }));
    let mut options = eframe::NativeOptions::default();
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
        options.run_and_return = true;
    }
    options.viewport = viewport;
    eframe::run_native("TextCraft", options, Box::new(move |cc| {
        setup_style(&cc.egui_ctx);
        Ok(Box::new(Editor::new(&cc.egui_ctx, shot, files)))
    }))
}

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

    #[test]
    fn launch_flags_are_not_opened_as_files() {
        let parse = |args: &[&str]| parse_args(args.iter().map(|a| a.to_string()));
        let args = parse(&["-AppleLanguages", "(fr)", "-psn_0_12345", "notes.txt", "--", "-dash.txt"]);
        assert_eq!(args.files, [PathBuf::from("notes.txt"), PathBuf::from("-dash.txt")]);
        assert!(args.shot.is_none() && args.export_logo.is_none());
        assert!(parse(&["--screenshot", "shot.png", "-NSDocumentRevisionsDebugMode", "YES"]).shot.is_some());
    }

    #[test]
    fn dialog_answers_reach_their_document_after_tabs_close() {
        let dir = std::env::temp_dir().join(format!("textcraft-dialog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("saved.txt");
        // A screenshot path keeps the settings in memory, away from the user's config.toml.
        let mut ed = Editor::new(&egui::Context::default(), Some(dir.join("unused.png")), vec![]);
        let answer = target.clone();
        ed.dialogs.set_override(Some(Box::new(move |req| match req {
            craft_dialogs::Request::File(_) => Answer::Paths(vec![answer.clone()]),
            craft_dialogs::Request::Message(_) => Answer::Message(MessageResult::No),
        })));
        // Tabs: the clean sample, then two edited files.
        for text in ["kept", "closed"] {
            ed.new_file();
            ed.docs[ed.active].text = text.into();
        }
        ed.save(1, true, false); // Save As for "kept"
        ed.close_tab(2); // "closed" asks whether to save; the answer is No
        ed.close_tab(0); // the clean sample closes at once, moving the others left
        ed.dialogs.show_pending(None::<&eframe::Frame>);
        ed.handle_dialogs(&egui::Context::default());

        let written = std::fs::read_to_string(&target);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(written.ok().as_deref(), Some("kept"), "Save As wrote the document it was asked for");
        let texts: Vec<&str> = ed.docs.iter().map(|d| d.text.as_str()).collect();
        assert_eq!(texts, ["kept"], "the answered close prompt closed its own document");
        assert!(!ed.docs[0].dirty());
    }

    #[test]
    fn every_default_shortcut_parses() {
        // keymap() drops texts it can't parse, so a typo would silently unbind a command.
        for (id, keys) in CMDS.iter().map(|c| (c.1, c.3)).chain(NOT_MAC.iter().copied()) {
            for k in keys {
                assert!(Binding::parse(k).is_ok(), "{id}: {k}");
            }
        }
    }

    #[test]
    fn default_shortcuts_are_free_on_every_platform() {
        use craft_keymap::{reserved, Chord, Platform};
        for platform in [Platform::Mac, Platform::Windows, Platform::Linux] {
            let mut taken = reserved::system(platform);
            if platform.is_mac() {
                // The app menu's Hide, Hide Others and Minimize, which reserved::system leaves out.
                taken.extend(["Cmd+H", "Cmd+Alt+H", "Cmd+M"].iter().filter_map(|t| Chord::parse(t).ok()));
            }
            let km = keymap(platform).with_reserved(taken);
            assert!(km.conflicts().is_empty(), "{platform:?}: {:?}", km.conflicts());
            assert!(km.problems().is_empty(), "{platform:?}: {:?}", km.problems());
            for b in km.catalog() {
                for binding in km.bindings(&b.command) {
                    assert!(!km.is_reserved(&binding.trigger.chord), "{platform:?}: {} on {binding}", b.command);
                }
            }
        }
    }

    #[test]
    fn hand_edited_config_rebinds_and_keeps_view_prefs() {
        // What a user writes by hand: one view key and one rebound command. A temp folder, never
        // the real settings folder.
        let dir = std::env::temp_dir().join(format!("textcraft-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "schema_version = 1\n[view]\nword_wrap = true\n\n[[shortcuts]]\ncommand = \"file.save\"\nkeys = [\"F2\"]\n").unwrap();
        let (store, report) = SettingsStore::<Prefs>::open_at(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!report.has_problems(), "{report:?}");

        let prefs = store.get();
        assert_ne!(prefs.view.word_wrap, ViewPrefs::default().word_wrap);
        assert!(prefs.view.show_sidebar == ViewPrefs::default().show_sidebar, "missing keys keep their defaults");
        let save = CommandRef::new("file.save");
        let platform = craft_keymap::Platform::current();
        let rebound = keymap(platform).with_overrides(prefs.shortcuts.clone());
        assert!(rebound.problems().is_empty());
        assert_ne!(rebound.bindings(&save), keymap(platform).bindings(&save));
    }
}

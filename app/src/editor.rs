//! The editor window: open documents, dialogs, commands, and the frame loop.
mod palette;
mod panels;
mod text_view;

use crate::commands::{keymap, mac_menus, Cmd, CMDS};
use crate::i18n::{CURRENT, I18N};
use crate::prefs::{Prefs, ViewPrefs};
use crate::theme::BG;
use craft_dialogs::{Answer, Dialogs, FileDialog, Message, MessageButtons, MessageLevel, MessageResult};
use craft_keymap::{egui::Dispatcher, Active, CommandRef, Keymap};
use craft_settings::{Location, SaveStatus, SettingsStore};
use craft_ui::capture::CaptureSlot;
use eframe::egui::{self, Key};
use std::path::PathBuf;
use std::sync::Arc;
use textcraft_core::{byte_of_line, char_idx, find_all, Doc};

/// A document's name in tabs and the window title.
fn title(d: &Doc) -> String {
    match &d.path {
        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        None => {
            if d.untitled_no == 0 {
                tl!("Untitled").into()
            } else {
                craft_i18n::fmt_plain(tl!("Untitled {n}"), &[("n", &(d.untitled_no + 1).to_string())])
            }
        } // also the Save As file name
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

/// The editor text field of document `doc` (its stable id, not its tab index).
fn editor_id(doc: u64) -> egui::Id {
    egui::Id::unique(("editor", doc))
}

#[derive(PartialEq)]
enum Overlay {
    None,
    Palette,
}

pub struct Editor {
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
    /// The document whose editor last took the keyboard; a different one shown takes it.
    editor_doc: Option<u64>,
    status: String,
    /// The window title last sent; sending one asks for another frame, so it goes only on a change.
    window_title: String,
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
    pub fn new(ctx: &egui::Context, shot: Option<PathBuf>, files: Vec<PathBuf>) -> Self {
        let wake_ctx = ctx.clone();
        // Screenshots and tests use defaults and never touch the user's config.toml, so they stay reproducible.
        let (settings, report) = SettingsStore::<Prefs>::open(if shot.is_some() || cfg!(test) { Location::Memory } else { Location::Auto });
        let view = settings.get().view.clone();
        let keymap = keymap(craft_keymap::egui::platform(ctx)).with_overrides(settings.get().shortcuts.clone());
        let mut problems = vec![];
        if let Some(path) = &report.broken_moved_to {
            problems.push(tf!(
                "config.toml was damaged and moved to {path}; using default settings",
                path = craft_i18n::ltr(&path.display().to_string())
            ));
        }
        for e in &report.section_errors {
            problems.push(tf!("settings [{section}] reset to defaults: {reason}", section = e.section, reason = e.message));
        }
        if let Some(why) = &report.read_only {
            problems.push(tf!("settings will not be saved: {reason}", reason = why));
        }
        if !keymap.problems().is_empty() {
            problems.push(trn!(
                keymap.problems().len(),
                "{n} shortcut entry in config.toml could not be used",
                "{n} shortcut entries in config.toml could not be used"
            ));
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
            editor_doc: None,
            status: problems.join("; "),
            window_title: String::new(),
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
        match textcraft_core::read_text(&p) {
            Ok(text) => {
                // Replace a pristine empty untitled tab instead of piling up tabs.
                if self.docs.len() == 1
                    && self.docs[0].path.is_none()
                    && !self.docs[0].dirty()
                    && self.docs[0].untitled_no != 0
                    && self.docs[0].text.is_empty()
                {
                    self.docs.clear();
                }
                self.docs.push(Doc::new(Some(p), text, 0));
                self.active = self.docs.len() - 1;
            }
            Err(e) => {
                self.status = craft_i18n::fmt(
                    I18N.tc(CURRENT.get(), "@msg", "Couldn't open {name}: {e}"),
                    &[("name", &craft_i18n::ltr(&p.display().to_string())), ("e", &e.to_string())],
                )
            }
        }
    }

    /// Saves doc `idx`. Untitled docs and Save As ask for a path first, so the write happens
    /// when the dialog answers (`handle_dialogs`).
    fn save(&mut self, idx: usize, save_as: bool, then_close: bool) {
        match self.docs[idx].path.clone().filter(|_| !save_as) {
            Some(p) => self.write(idx, p, then_close),
            None => self.dialogs.ask(FileDialog::save().file_name(title(&self.docs[idx])), Then::SaveAs { doc: self.docs[idx].id, then_close }),
        }
    }

    fn write(&mut self, idx: usize, path: PathBuf, then_close: bool) {
        match self.docs[idx].save_to(path) {
            Ok(()) => {
                self.status = tl!("Saved").into();
                if then_close {
                    self.remove_tab(idx);
                }
            }
            // craft-atomic's error names the file, says why, and says whether the original was kept.
            Err(e) => self.status = tf!("Could not save: {error}", error = e),
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if self.docs[idx].dirty() {
            let message = Message {
                level: MessageLevel::Warning,
                title: tl!("Unsaved changes").into(),
                description: tf!("Save changes to {name} before closing?", name = title(&self.docs[idx])),
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
        let cur = byte_of_line(text, self.cursor_line)
            + text[byte_of_line(text, self.cursor_line)..].chars().take(self.cursor_col - 1).map(|c| c.len_utf8()).sum::<usize>();
        let pick = if forward { hits.iter().find(|h| h.0 >= cur).or(hits.first()) } else { hits.iter().rev().find(|h| h.1 < cur).or(hits.last()) }
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

    fn open_palette(&mut self, prefix: &str) {
        self.overlay = Overlay::Palette;
        self.palette_query = prefix.into();
        self.palette_sel = 0;
        if prefix.is_empty() {
            self.file_index = self.folder.as_deref().map(textcraft_core::index_folder).unwrap_or_default();
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
                    if self.docs[i].dirty() {
                        self.save(i, false, false);
                    }
                }
            }
            Cmd::CloseTab => self.close_tab(self.active),
            Cmd::NextTab => self.active = (self.active + 1) % self.docs.len(),
            Cmd::PrevTab => self.active = (self.active + self.docs.len() - 1) % self.docs.len(),
            Cmd::Find => {
                self.find_open = true;
                self.replace_open = false;
                self.find_focus = true;
            }
            Cmd::Replace => {
                self.find_open = true;
                self.replace_open = true;
                self.find_focus = true;
            }
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
            self.editor_doc = None; // the shown document's editor takes the keyboard back
        } else if self.find_open && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.find_open = false;
        }

        let title = format!("{}{} — TextCraft", title(&self.docs[self.active]), if self.docs[self.active].dirty() { " •" } else { "" });
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

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
        let view =
            ViewPrefs { show_sidebar: self.show_sidebar, show_minimap: self.show_minimap, font_size: self.font_size, word_wrap: self.word_wrap };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shown_document_gets_the_keyboard() {
        let ctx = egui::Context::default();
        // A screenshot path keeps the settings in memory, away from the user's config.toml.
        let mut ed = Editor::new(&ctx, Some(std::env::temp_dir().join("textcraft-focus-unused.png")), vec![]);
        let frame = |ed: &mut Editor| {
            for _ in 0..2 {
                // No renderer here: the suite's egui wants texture updates handled, so drop them.
                ctx.run_ui(Default::default(), |ui| ed.editor_area(ui)).textures_delta.clear();
            }
            ctx.memory(|m| m.focused())
        };
        assert_eq!(frame(&mut ed), Some(editor_id(ed.docs[ed.active].id)), "the editor has the keyboard at start");
        ed.new_file(); // ⌘N
        assert_eq!(frame(&mut ed), Some(editor_id(ed.docs[ed.active].id)), "the new tab's editor has the keyboard");
        ed.active = 0; // switching back, as a tab click or Ctrl+Tab does
        assert_eq!(frame(&mut ed), Some(editor_id(ed.docs[0].id)));
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
}

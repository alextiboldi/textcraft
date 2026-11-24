//! Commands, their default shortcuts and the menus built from them.
use craft_keymap::{Bindable, Binding, CommandRef, Keymap, Scope, Scopes};

#[derive(Clone, Copy)]
pub enum Cmd {
    NewFile,
    Open,
    OpenFolder,
    Save,
    SaveAs,
    SaveAll,
    CloseTab,
    NextTab,
    PrevTab,
    Find,
    Replace,
    FindNext,
    FindPrev,
    GotoLine,
    GotoFile,
    Palette,
    ToggleSidebar,
    ToggleMinimap,
    WordWrap,
    Bigger,
    Smaller,
    Quit,
}

/// (command, id, label, default shortcuts in craft-keymap's text form). `Cmd` is ⌘ on macOS and
/// Ctrl elsewhere; `Ctrl` is the Mac Control key. The menus refer to rows by index. `NOT_MAC`
/// replaces some rows' shortcuts on Windows and Linux.
pub const CMDS: &[(Cmd, &str, &str, &[&str])] = &[
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
pub const NOT_MAC: &[(&str, &[&str])] = &[("find.replace", &["Cmd+Alt+F", "Cmd+H"]), ("find.next", &["F3"]), ("find.prev", &["Shift+F3"])];

/// The menus, as rows of `CMDS`.
pub const MENUS: [(&str, &[usize]); 4] =
    [("File", &[0, 1, 2, 3, 4, 5, 6, 21]), ("Edit", &[9, 10, 11, 12]), ("Goto", &[15, 14, 13, 7, 8]), ("View", &[16, 17, 18, 19, 20])];

/// A command's label in a menu: without the palette's "File: " prefix.
pub fn menu_label(label: &str) -> &str {
    label.split_once(": ").map_or(label, |(_, l)| l)
}

/// The macOS menu bar: `MENUS` with the keymap's shortcuts as key equivalents, laid out by
/// craft-menu (app menu with Hide and Quit, Window, Help). Quit is the app's own command, so ⌘Q
/// asks about unsaved changes like the close button.
pub fn mac_menus(keymap: &Keymap) -> craft_menu::Layout {
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

pub fn keymap(platform: craft_keymap::Platform) -> Keymap {
    let catalog = CMDS
        .iter()
        .map(|&(_, id, label, keys)| {
            let keys = NOT_MAC.iter().find(|o| o.0 == id && !platform.is_mac()).map_or(keys, |o| o.1);
            keys.iter().filter_map(|k| Binding::parse(k).ok()).fold(Bindable::new(id, tid!(id, label), Scope::app()), Bindable::with_default)
        })
        .collect();
    // Chords the system takes before TextCraft sees them (⌘Tab, Alt+F4…): no default or
    // config.toml entry may use them; an entry that tries is reported through `problems`.
    Keymap::new(catalog, Scopes::new(), platform).with_reserved(craft_keymap::reserved::system(platform))
}

#[cfg(test)]
mod tests {
    use super::*;

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
                // TextCraft's own choice: its defaults leave the app menu's Hide, Hide Others and
                // Minimize alone. A user may still bind them (craft-menu then moves Hide and Minimize
                // to ⌃⌘H and ⌃⌘M), so they are not reserved in the app's keymap.
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
}

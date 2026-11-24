//! Settings kept in config.toml.
use serde::{Deserialize, Serialize};

/// What TextCraft keeps in `config.toml` between runs.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub view: ViewPrefs,
    /// Shortcut changes by command id, e.g. `[[shortcuts]] command = "file.save" keys = ["F2"]`.
    #[serde(with = "craft_keymap::keymap::overrides_list")]
    pub shortcuts: craft_keymap::Overrides,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct ViewPrefs {
    pub show_sidebar: bool,
    pub show_minimap: bool,
    pub font_size: f32,
    pub word_wrap: bool,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::keymap;
    use craft_keymap::CommandRef;
    use craft_settings::SettingsStore;

    #[test]
    fn config_cannot_bind_a_chord_the_system_takes() {
        use craft_keymap::{Chord, Platform};
        let dir = std::env::temp_dir().join(format!("textcraft-reserved-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "schema_version = 1\n\n[[shortcuts]]\ncommand = \"file.save\"\nkeys = [\"Cmd+Tab\"]\n").unwrap();
        let (store, _) = SettingsStore::<Prefs>::open_at(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        // ⌘Tab switches apps on macOS before TextCraft sees it.
        let km = keymap(Platform::Mac).with_overrides(store.get().shortcuts.clone());
        let cmd_tab = Chord::parse("Cmd+Tab").unwrap();
        assert!(km.bindings(&CommandRef::new("file.save")).iter().all(|b| b.trigger.chord != cmd_tab));
        assert!(!km.problems().is_empty(), "the unusable entry is reported");
    }

    #[test]
    fn hand_edited_config_rebinds_and_keeps_view_prefs() {
        // What a user writes by hand: one view key and one rebound command. A temp folder, never
        // the real settings folder.
        let dir = std::env::temp_dir().join(format!("textcraft-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "schema_version = 1\n[view]\nword_wrap = true\n\n[[shortcuts]]\ncommand = \"file.save\"\nkeys = [\"F2\"]\n",
        )
        .unwrap();
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

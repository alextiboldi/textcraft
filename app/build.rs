//! Embeds the craft-i18n catalogs (craft-i18n's `docs/integration.md`): `CRAFT_I18N_DIR` names a
//! craft-i18n checkout, and every locale with a `textcraft.tsv` is embedded with its shared
//! `common.tsv`. Unset, the table is empty and TextCraft is English-only.
use std::path::Path;

const APP: &str = "textcraft";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=CRAFT_I18N_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_I18N_REQUIRED");
    let required = std::env::var("CRAFT_I18N_REQUIRED").is_ok_and(|v| v == "1");
    let mut rows = String::new();
    match std::env::var_os("CRAFT_I18N_DIR").filter(|d| !d.is_empty()) {
        None if required => println!("cargo::error=CRAFT_I18N_REQUIRED=1 but CRAFT_I18N_DIR is unset"),
        None => {}
        Some(dir) => {
            let locales = Path::new(&dir).join("locales");
            match std::fs::read_dir(&locales) {
                Err(e) => {
                    let level = if required { "error" } else { "warning" };
                    println!("cargo::{level}=CRAFT_I18N_DIR is not a craft-i18n checkout ({}: {e}); building English-only", locales.display());
                }
                Ok(entries) => {
                    println!("cargo::rerun-if-changed={}", locales.display());
                    let mut found: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.join(format!("{APP}.tsv")).is_file()).collect();
                    found.sort();
                    for locale in found {
                        let code = locale.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let app = locale.join(format!("{APP}.tsv"));
                        let common = locale.join("common.tsv");
                        println!("cargo::rerun-if-changed={}", app.display());
                        println!("cargo::rerun-if-changed={}", common.display());
                        let shared = if common.is_file() { format!("include_str!({:?})", common) } else { "\"\"".into() };
                        rows.push_str(&format!("    ({code:?}, {shared}, include_str!({app:?})),\n"));
                    }
                }
            }
        }
    }
    let out = Path::new(&std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_i18n.rs");
    let src = format!("/// (locale, shared catalog, TextCraft's catalog), one entry per locale; empty without CRAFT_I18N_DIR.\npub static CRAFT_I18N: &craft_i18n::Embedded = &[\n{rows}];\n");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=could not write {}: {e}", out.display());
    }
}

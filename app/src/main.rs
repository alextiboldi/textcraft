// TextCraft: a small dark code editor (tabs, folder sidebar, code colouring,
// line numbers, minimap, find & replace, go to line, command palette).
// The text logic is in textcraft-core (`core/`); this crate is the egui app.
#[macro_use]
mod i18n;
mod commands;
mod editor;
mod prefs;
mod theme;

use editor::Editor;
use eframe::egui;
use i18n::{CURRENT, I18N};
use std::path::PathBuf;
use std::sync::Arc;
use theme::{logo_rgba, setup_style};

/// What the command line asks for.
#[derive(Default)]
struct Args {
    shot: Option<PathBuf>,
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
    let Args { shot, files } = parse_args(std::env::args().skip(1));

    let mut viewport = egui::ViewportBuilder::default().with_title("TextCraft").with_inner_size([1200.0, 760.0]).with_min_inner_size([500.0, 300.0]);
    if let Some(rgba) = logo_rgba(512) {
        viewport = viewport.with_icon(Arc::new(egui::IconData { rgba, width: 512, height: 512 }));
    }
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
    eframe::run_native(
        "TextCraft",
        options,
        Box::new(move |cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(Editor::new(&cc.egui_ctx, shot, files)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_flags_are_not_opened_as_files() {
        let parse = |args: &[&str]| parse_args(args.iter().map(|a| a.to_string()));
        let args = parse(&["-AppleLanguages", "(fr)", "-psn_0_12345", "notes.txt", "--", "-dash.txt"]);
        assert_eq!(args.files, [PathBuf::from("notes.txt"), PathBuf::from("-dash.txt")]);
        assert!(args.shot.is_none());
        assert!(parse(&["--screenshot", "shot.png", "-NSDocumentRevisionsDebugMode", "YES"]).shot.is_some());
    }
}

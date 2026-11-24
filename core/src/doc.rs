//! Open documents, and reading and writing their files.
use crate::lang::{lang_for, Lang, PLAIN};
use std::path::{Path, PathBuf};

pub struct Doc {
    /// Stays the same while tabs open and close, unlike the tab index.
    pub id: u64,
    pub path: Option<PathBuf>,
    pub text: String,
    pub saved: String,
    pub untitled_no: usize,
}

impl Doc {
    pub fn new(path: Option<PathBuf>, text: String, untitled_no: usize) -> Doc {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Doc { id, path, saved: text.clone(), text, untitled_no }
    }
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
    pub fn lang(&self) -> Lang {
        match &self.path {
            Some(p) => lang_for(&p.to_string_lossy()),
            None if self.untitled_no == 0 && self.text.starts_with("//") => lang_for("x.rs"),
            None => PLAIN,
        }
    }
    /// Writes the text to `path`, which becomes the document's file, and marks it saved.
    pub fn save_to(&mut self, path: PathBuf) -> std::io::Result<()> {
        std::fs::write(&path, &self.text)?;
        self.saved = self.text.clone();
        self.path = Some(path);
        Ok(())
    }
}

/// A file's text, with invalid UTF-8 replaced and CRLF line ends made LF.
pub fn read_text(path: &Path) -> std::io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).replace("\r\n", "\n"))
}

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
    /// craft-atomic stages the text beside the file and renames it over, so a failed save leaves
    /// the file's old bytes and the document unsaved. Symbolic links and permission bits are kept.
    pub fn save_to(&mut self, path: PathBuf) -> std::io::Result<()> {
        self.save_with(&craft_atomic::RealFs, path)
    }
    fn save_with(&mut self, fs: &dyn craft_atomic::AtomicFs, path: PathBuf) -> std::io::Result<()> {
        craft_atomic::atomic_write_custom(fs, craft_atomic::RenameRetry::platform(), &path, self.text.as_bytes())?;
        self.saved = self.text.clone();
        self.path = Some(path);
        Ok(())
    }
}

/// A file's text, with invalid UTF-8 replaced and CRLF line ends made LF.
pub fn read_text(path: &Path) -> std::io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).replace("\r\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_atomic::{AtomicFs, NewFilePermissions, RealFs};
    use std::fs::File;
    use std::io;

    /// The real file system, except that flushing the staged text fails as on a full disk.
    struct FullDisk;
    impl AtomicFs for FullDisk {
        fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
            RealFs.create_new(path, permissions)
        }
        fn sync(&self, _: &File) -> io::Result<()> {
            Err(io::ErrorKind::StorageFull.into())
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            RealFs.rename(from, to)
        }
        fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
            RealFs.hard_link(from, to)
        }
        fn remove(&self, path: &Path) -> io::Result<()> {
            RealFs.remove(path)
        }
        fn sync_dir(&self, dir: &Path) -> io::Result<()> {
            RealFs.sync_dir(dir)
        }
        fn set_permissions(&self, file: &File, perms: std::fs::Permissions) -> io::Result<()> {
            RealFs.set_permissions(file, perms)
        }
        #[cfg(unix)]
        fn set_group(&self, file: &File, gid: u32) -> io::Result<()> {
            RealFs.set_group(file, gid)
        }
    }

    #[test]
    fn a_failed_save_leaves_the_file_and_the_document_as_they_were() {
        let dir = std::env::temp_dir().join(format!("textcraft-failed-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.txt");
        std::fs::write(&path, "old text\n").unwrap();
        let mut doc = Doc::new(Some(path.clone()), read_text(&path).unwrap(), 0);
        doc.text.push_str("new text\n");

        assert!(doc.save_with(&FullDisk, path.clone()).is_err());
        assert!(doc.save_with(&FullDisk, dir.join("elsewhere.txt")).is_err());

        assert_eq!(std::fs::read(&path).unwrap(), b"old text\n");
        assert!(doc.dirty());
        assert_eq!(doc.path.as_ref(), Some(&path));
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["notes.txt"], "the staged text is cleaned up and no new file appears");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

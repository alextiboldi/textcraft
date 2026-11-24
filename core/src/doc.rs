//! Open documents, and reading and writing their files.
use crate::encoding::{decode, Decoded, Encoding, ReadOnly};
use crate::lang::{lang_for, Lang, PLAIN};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// A line-end style. The editor holds text with LF line ends and writes the file's style back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Eol {
    #[default]
    Lf,
    Crlf,
    Cr,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
            Eol::Cr => "\r",
        }
    }
    /// The style's usual short name, as editors show it.
    pub fn name(self) -> &'static str {
        match self {
            Eol::Lf => "LF",
            Eol::Crlf => "CRLF",
            Eol::Cr => "CR",
        }
    }
}

pub struct Doc {
    /// Stays the same while tabs open and close, unlike the tab index.
    pub id: u64,
    pub path: Option<PathBuf>,
    pub text: String,
    pub saved: String,
    pub untitled_no: usize,
    /// The line ends `save_to` writes; `text` itself always uses LF.
    pub eol: Eol,
    /// The encoding `save_to` writes.
    pub encoding: Encoding,
    /// Set when the text can't be saved over its file; the editor doesn't let it be edited.
    pub read_only: Option<ReadOnly>,
}

impl Doc {
    pub fn new(path: Option<PathBuf>, text: String, untitled_no: usize) -> Doc {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Doc { id, path, saved: text.clone(), text, untitled_no, eol: Eol::Lf, encoding: Encoding::UTF8, read_only: None }
    }
    /// The document for a file read with `read_text`.
    pub fn opened(path: PathBuf, file: Decoded) -> Doc {
        Doc { eol: file.eol, encoding: file.encoding, read_only: file.read_only, ..Doc::new(Some(path), file.text, 0) }
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
    /// A read-only document, or text its encoding can't write (see `Encoding::unencodable`), is
    /// refused before anything is written.
    pub fn save_to(&mut self, path: PathBuf) -> std::io::Result<()> {
        self.save_with(&craft_atomic::RealFs, path)
    }
    fn save_with(&mut self, fs: &dyn craft_atomic::AtomicFs, path: PathBuf) -> std::io::Result<()> {
        if self.read_only.is_some() {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the document is read-only"));
        }
        let text = match self.eol {
            Eol::Lf => Cow::Borrowed(&self.text),
            eol => Cow::Owned(self.text.replace('\n', eol.as_str())),
        };
        let bytes = self.encoding.encode(&text).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{} can't represent some of the text", self.encoding.name()))
        })?;
        craft_atomic::atomic_write_custom(fs, craft_atomic::RenameRetry::platform(), &path, &bytes)?;
        remove_stale_stages(&path);
        self.saved = self.text.clone();
        self.path = Some(path);
        Ok(())
    }
}

/// A file's text in its own encoding, with line ends made LF; see `decode` and `split_eol`.
pub fn read_text(path: &Path) -> std::io::Result<Decoded> {
    Ok(decode(&std::fs::read(path)?))
}

/// Removes, as best it can, the save stages a crash left in `file`'s folder.
///
/// craft-atomic saves by writing `.<name>.<pid>-<n>.craft_tmp` beside the file (the link's target,
/// for a symbolic link) and renaming it over; a crash, kill or power cut in between leaves it behind.
/// A live save renames its stage within seconds, so any stage untouched for an hour is left over,
/// whichever file or Craft app it was for. Errors are ignored: the save or open itself succeeded.
// ponytail: copies craft-atomic's stage name format; it moves into craft-atomic in #224. Lists the
// folder on each save and open, fine for ordinary folders; skip it above some entry count if a
// folder of 100k files ever makes saving feel slow.
pub fn remove_stale_stages(file: &Path) {
    const STALE: std::time::Duration = std::time::Duration::from_secs(60 * 60);
    let Some(dir) = std::fs::canonicalize(file).ok().and_then(|p| p.parent().map(Path::to_path_buf)) else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(is_stage_name) {
            continue;
        }
        // `DirEntry::metadata` does not follow links, so a link named like a stage is kept.
        let age = entry.metadata().ok().filter(|m| m.is_file()).and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok());
        if age.is_some_and(|age| age > STALE) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Whether `name` is a craft-atomic save stage: `.`, a file name, `.`, digits, `-`, digits, `.craft_tmp`.
pub fn is_stage_name(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let Some(rest) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".craft_tmp")) else { return false };
    let Some((file, tail)) = rest.rsplit_once('.') else { return false };
    !file.is_empty() && tail.split_once('-').is_some_and(|(pid, n)| digits(pid) && digits(n))
}

/// `text` with every CRLF, CR and LF made LF, its most common line end (LF on a tie or when it
/// has none), and whether it mixed styles.
pub fn split_eol(text: &str) -> (String, Eol, bool) {
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let mut bytes = text.bytes().peekable();
    while let Some(b) = bytes.next() {
        match b {
            b'\r' if bytes.next_if_eq(&b'\n').is_some() => crlf += 1,
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
    }
    // `max_by_key` keeps the last of equal counts, so ties go to LF.
    let eol = [(cr, Eol::Cr), (crlf, Eol::Crlf), (lf, Eol::Lf)].into_iter().max_by_key(|&(n, _)| n).map_or(Eol::Lf, |(_, e)| e);
    let mixed = [lf, crlf, cr].iter().filter(|&&n| n > 0).count() > 1;
    let text = if cr + crlf == 0 { text.to_owned() } else { text.replace("\r\n", "\n").replace('\r', "\n") };
    (text, eol, mixed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_atomic::{AtomicFs, NewFilePermissions, RealFs};
    use std::fs::File;
    use std::io;

    fn reopen_and_save(name: &str, bytes: &[u8]) -> (Vec<u8>, Eol, bool) {
        let dir = std::env::temp_dir().join(format!("textcraft-eol-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = read_text(&path).unwrap();
        let (eol, mixed) = (file.eol, file.mixed);
        Doc::opened(path.clone(), file).save_to(path.clone()).unwrap();
        let out = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        (out, eol, mixed)
    }

    #[test]
    fn unedited_files_save_the_bytes_that_were_read() {
        for (name, bytes) in [("lf", &b"a\nb\n\nc"[..]), ("crlf", b"a\r\nb\r\n\r\nc"), ("cr", b"a\rb\r\rc")] {
            let (out, _, mixed) = reopen_and_save(name, bytes);
            assert_eq!(out, bytes, "{name}");
            assert!(!mixed, "{name}");
        }
    }

    #[test]
    fn mixed_files_save_with_their_most_common_style() {
        let (out, eol, mixed) = reopen_and_save("mixed", b"a\r\nb\r\nc\nd\re");
        assert!(mixed);
        assert_eq!(eol, Eol::Crlf);
        assert_eq!(out, b"a\r\nb\r\nc\r\nd\r\ne");
    }

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
        let mut doc = Doc::opened(path.clone(), read_text(&path).unwrap());
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

    #[test]
    fn only_stale_save_stages_are_removed() {
        let dir = std::env::temp_dir().join(format!("textcraft-stages-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.txt");
        std::fs::write(&file, "text").unwrap();
        let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
        for (name, old) in [
            (".notes.txt.4242-0.craft_tmp", true),     // left by a crash: removed
            (".other.md.17-3.craft_tmp", true),        // another file's, also left over: removed
            (".notes.txt.4242-1.craft_tmp", false),    // a save in progress: kept
            (".notes.txt.42x-0.craft_tmp", true),      // not a stage name: kept
            (".notes.txt.craft_tmp", true),            // not a stage name: kept
            ("notes.txt.4242-0.craft_tmp", true),      // not hidden, so not a stage: kept
            (".notes.txt.4242-0.craft_tmp.bak", true), // not a stage name: kept
        ] {
            let f = File::create(dir.join(name)).unwrap();
            if old {
                f.set_modified(two_hours_ago).unwrap();
            }
        }

        remove_stale_stages(&file);

        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        let kept = [
            ".notes.txt.4242-0.craft_tmp.bak",
            ".notes.txt.4242-1.craft_tmp",
            ".notes.txt.42x-0.craft_tmp",
            ".notes.txt.craft_tmp",
            "notes.txt",
            "notes.txt.4242-0.craft_tmp",
        ];
        assert_eq!(left, kept);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

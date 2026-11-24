//! Search helpers: text matches, line and character offsets, fuzzy matching, folder indexing.
use std::path::{Path, PathBuf};

/// Byte ranges of every match of `needle` (ASCII case folding keeps byte offsets stable).
pub fn find_all(text: &str, needle: &str, case: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return vec![];
    }
    let (h, n) = if case { (text.to_string(), needle.to_string()) } else { (text.to_ascii_lowercase(), needle.to_ascii_lowercase()) };
    h.match_indices(&n).map(|(s, m)| (s, s + m.len())).collect()
}

pub fn char_idx(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

pub fn byte_of_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    text.match_indices('\n').nth(line - 2).map_or(text.len(), |(i, _)| i + 1)
}

/// Loose "letters in order" match; lower score is better.
pub fn fuzzy(hay: &str, needle: &str) -> Option<usize> {
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

/// Every file under `root`, for Go to File: hidden entries, `target` and `node_modules` left out.
pub fn index_folder(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![];
    let mut stack = vec![root.to_path_buf()];
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
                files.push(p);
            }
            // ponytail: capped walk; add a background indexer for huge trees.
            if files.len() > 20_000 {
                return files;
            }
        }
    }
    files
}

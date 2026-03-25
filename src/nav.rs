//! nav.rs — Directory-based image navigation.
//!
//! Maintains a human-sorted list of sibling image files in the same directory
//! as the currently open file.  Navigation wraps around at both ends.

use std::path::{Path, PathBuf};
use crate::loader::is_supported;

#[derive(Default)]
pub struct DirNavigator {
    /// All image files in the current directory, human-sorted.
    files: Vec<PathBuf>,
    /// Index of the currently displayed file.
    index: usize,
}

impl DirNavigator {
    /// Call whenever a new path is opened.
    /// Rebuilds the sibling list only when the directory actually changes.
    pub fn set_current(&mut self, path: PathBuf) {
        let new_dir = path.parent().map(Path::to_path_buf);

        let same_dir = self.files.first()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .as_deref() == new_dir.as_deref();

        if !same_dir {
            self.rebuild(new_dir.as_deref());
        }

        // Locate `path` in the list, or fall back to a single-entry list
        self.index = self.files.iter().position(|p| *p == path)
            .unwrap_or_else(|| {
                self.files = vec![path];
                0
            });
    }

    /// Advance forward (wraps). Returns `None` if the list is empty.
    pub fn next(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() { return None; }
        self.index = (self.index + 1) % self.files.len();
        Some(self.files[self.index].clone())
    }

    /// Go back (wraps). Returns `None` if the list is empty.
    pub fn prev(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() { return None; }
        self.index = self.index.checked_sub(1).unwrap_or(self.files.len() - 1);
        Some(self.files[self.index].clone())
    }

    /// Peek at the next path without mutating state.
    pub fn peek_next(&self) -> Option<PathBuf> {
        (self.files.len() > 1).then(|| {
            self.files[(self.index + 1) % self.files.len()].clone()
        })
    }

    /// Peek at the previous path without mutating state.
    pub fn peek_prev(&self) -> Option<PathBuf> {
        (self.files.len() > 1).then(|| {
            let idx = self.index.checked_sub(1).unwrap_or(self.files.len() - 1);
            self.files[idx].clone()
        })
    }

    /// Remove a path (e.g. after deletion) and adjust the index so the next
    /// logical sibling becomes current.
    pub fn remove(&mut self, path: &Path) {
        if let Some(pos) = self.files.iter().position(|p| p.as_path() == path) {
            self.files.remove(pos);
            self.index = pos.min(self.files.len().saturating_sub(1));
        }
    }

    /// The currently active path (may differ from the last `set_current` call
    /// if `remove` was called).
    pub fn current(&self) -> Option<&PathBuf> {
        self.files.get(self.index)
    }

    pub fn current_index(&self) -> usize { self.index }
    pub fn total(&self)         -> usize { self.files.len() }

    // ── Private ───────────────────────────────────────────────────────────────

    fn rebuild(&mut self, dir: Option<&Path>) {
        self.index = 0;
        self.files = dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|entries| {
                let mut paths: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_file() && is_supported(p))
                    .collect();
                paths.sort_by(|a, b| {
                    let a = a.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    let b = b.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                    human_sort(&a, &b)
                });
                paths
            })
            .unwrap_or_default();
    }
}

// ── Human / natural sort ──────────────────────────────────────────────────────
//
// Compares runs of digits numerically so that "img2" < "img10".

fn human_sort(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _)    => return Ordering::Less,
            (_, None)    => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let ord = consume_number(&mut ai).cmp(&consume_number(&mut bi));
                if ord != Ordering::Equal { return ord; }
            }
            (Some(ac), Some(bc)) => {
                let ord = ac.cmp(&bc);
                if ord != Ordering::Equal { return ord; }
                ai.next(); bi.next();
            }
        }
    }
}

fn consume_number(iter: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n = 0u64;
    while let Some(&c) = iter.peek() {
        if c.is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add(c as u64 - b'0' as u64);
            iter.next();
        } else {
            break;
        }
    }
    n
}

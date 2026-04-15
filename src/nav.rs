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
    /// The directory that `files` was built from. Stored explicitly so that
    /// the same-directory check in `set_current` does not have to infer it
    /// from `files.first()`, which would give a wrong answer after the
    /// single-entry fallback path resets the list.
    current_dir: Option<PathBuf>,
}

impl DirNavigator {
    /// Call whenever a new path is opened.
    /// Rebuilds the sibling list only when the directory actually changes.
    pub fn set_current(&mut self, path: PathBuf) {
        let new_dir = path.parent().map(Path::to_path_buf);

        if self.current_dir.as_deref() != new_dir.as_deref() {
            self.current_dir = new_dir;
            self.rebuild();
        }

        // Locate `path` in the sorted list. The fallback (replacing the list
        // with a single entry) should not occur in normal use — it would only
        // trigger if the file disappeared from the directory between the
        // rebuild and this position scan. Navigation still works; the sibling
        // list is restored the next time the directory changes.
        self.index = self.files.iter().position(|p| *p == path)
            .unwrap_or_else(|| {
                self.files = vec![path];
                0
            });
    }

    /// Advance forward (wraps). Returns `None` if fewer than 2 images exist.
    pub fn next(&mut self) -> Option<PathBuf> {
        self.step(Direction::Forward)
    }

    /// Go back (wraps). Returns `None` if fewer than 2 images exist.
    pub fn prev(&mut self) -> Option<PathBuf> {
        self.step(Direction::Backward)
    }

    /// Peek at the next path without mutating state.
    pub fn peek_next(&self) -> Option<PathBuf> {
        self.peek(Direction::Forward)
    }

    /// Peek at the previous path without mutating state.
    pub fn peek_prev(&self) -> Option<PathBuf> {
        self.peek(Direction::Backward)
    }

    /// Remove a path (e.g. after deletion) and adjust the index so the next
    /// logical sibling becomes current.
    pub fn remove(&mut self, path: &Path) {
        if let Some(pos) = self.files.iter().position(|p| p.as_path() == path) {
            self.files.remove(pos);
            self.index = pos.min(self.files.len().saturating_sub(1));
        }
    }

    /// Update a path in the sibling list after a rename.
    /// Preserves the current index so navigation is unaffected.
    pub fn rename(&mut self, old_path: &Path, new_path: PathBuf) {
        if let Some(entry) = self.files.iter_mut().find(|p| p.as_path() == old_path) {
            *entry = new_path;
        }
    }

    /// The currently active path (may differ from the last `set_current` call
    /// if `remove` was called).
    pub fn current(&self) -> Option<&PathBuf> {
        self.files.get(self.index)
    }

    /// Number of image files in the current sibling list.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether `path` is present in the current sibling list.
    pub fn contains(&self, path: &Path) -> bool {
        self.files.iter().any(|f| f.as_path() == path)
    }

    /// Rebuild the sibling list in place after an OS-reported directory change.
    ///
    /// Re-locates the currently displayed file in the new sorted list so the
    /// index stays accurate regardless of how many files were added or removed.
    /// If the current file is no longer present the index is clamped — the
    /// caller is responsible for detecting this and navigating away.
    pub fn rebuild_current(&mut self, current_path: Option<&Path>) {
        self.rebuild();
        self.index = current_path
            .and_then(|p| self.files.iter().position(|f| f.as_path() == p))
            .unwrap_or_else(|| self.index.min(self.files.len().saturating_sub(1)));
    }

    // ── Private ───────────────────────────────────────────────────────────────

    /// Advance the index in `direction` and return the new current path.
    /// Returns `None` if fewer than 2 images exist (nothing to navigate to).
    fn step(&mut self, direction: Direction) -> Option<PathBuf> {
        if self.files.len() < 2 { return None; }
        self.index = self.wrapped_index(direction);
        Some(self.files[self.index].clone())
    }

    /// Peek at the neighbour in `direction` without mutating state.
    fn peek(&self, direction: Direction) -> Option<PathBuf> {
        (self.files.len() > 1).then(|| {
            self.files[self.wrapped_index(direction)].clone()
        })
    }

    /// Compute the neighbour index with wraparound.
    fn wrapped_index(&self, direction: Direction) -> usize {
        match direction {
            Direction::Forward  => (self.index + 1) % self.files.len(),
            Direction::Backward => self.index
                .checked_sub(1)
                .unwrap_or(self.files.len() - 1),
        }
    }

    /// Populate `files` from `current_dir`. Called only when the directory
    /// changes. `self.index` is not touched here — `set_current` sets it
    /// immediately after via `position()`.
    fn rebuild(&mut self) {
        let Some(dir) = self.current_dir.as_deref() else {
            self.files.clear();
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            self.files.clear();
            return;
        };

        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| {
                let e = e.ok()?;
                // Use the DirEntry file type — already available from
                // the readdir() result on every major OS, no extra
                // stat() syscall needed. is_file() would cost one
                // syscall per file, i.e. 5000 disk hits on a cold
                // cache for a 5K-file folder.
                if !e.file_type().ok()?.is_file() { return None; }
                let path = e.path();
                is_supported(&path).then_some(path)
            })
            .collect();

        // The sort allocates two Strings per comparison via to_string_lossy().
        // This is acceptable: rebuild() runs only on directory change, not
        // per-navigation, and avoids caching complexity.
        paths.sort_by(|a, b| {
            let a = a.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            let b = b.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            human_sort(&a, &b)
        });
        self.files = paths;
    }
}

/// Direction for single-step navigation.
#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
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

/// Consume and return a run of ASCII digits as a `u64`, saturating on overflow.
fn consume_number(iter: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n = 0u64;
    while let Some(&c) = iter.peek() {
        let Some(digit) = c.to_digit(10) else { break };
        n = n.saturating_mul(10).saturating_add(digit as u64);
        iter.next();
    }
    n
}

//! nav.rs — Directory-based image navigation.
//!
//! Maintains a human-sorted list of sibling image files in the same directory
//! as the currently open file.  Navigation wraps around at both ends.

use crate::loader::is_supported;
use std::path::{Path, PathBuf};

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
        self.index = self
            .files
            .iter()
            .position(|p| *p == path)
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
            // Stay on the same file when another one goes; when the current
            // file goes, its successor (or the new last file) takes its place.
            if pos < self.index {
                self.index -= 1;
            } else if pos == self.index {
                self.index = pos.min(self.files.len().saturating_sub(1));
            }
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
    #[allow(clippy::len_without_is_empty)]
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
        if self.files.len() < 2 {
            return None;
        }
        self.index = self.wrapped_index(direction);
        Some(self.files[self.index].clone())
    }

    /// Peek at the neighbour in `direction` without mutating state.
    fn peek(&self, direction: Direction) -> Option<PathBuf> {
        (self.files.len() > 1).then(|| self.files[self.wrapped_index(direction)].clone())
    }

    /// Compute the neighbour index with wraparound.
    fn wrapped_index(&self, direction: Direction) -> usize {
        match direction {
            Direction::Forward => (self.index + 1) % self.files.len(),
            Direction::Backward => self.index.checked_sub(1).unwrap_or(self.files.len() - 1),
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
                if !e.file_type().ok()?.is_file() {
                    return None;
                }
                let path = e.path();
                is_supported(&path).then_some(path)
            })
            .collect();

        sort_like_explorer(&mut paths);
        self.files = paths;
    }
}

// ── Explorer order ────────────────────────────────────────────────────────────

/// Sort paths by file name in the order Explorer shows them.
///
/// Each sort key is built once per file rather than once per comparison.
fn sort_like_explorer(paths: &mut Vec<PathBuf>) {
    let mut keyed: Vec<(SortKey, PathBuf)> = paths.drain(..).map(|p| (sort_key(&p), p)).collect();
    keyed.sort_by(|(a, _), (b, _)| compare_keys(a, b));
    paths.extend(keyed.into_iter().map(|(_, p)| p));
}

/// Null-terminated UTF-16 file name, encoded losslessly from the `OsStr`.
#[cfg(windows)]
type SortKey = Vec<u16>;

#[cfg(windows)]
fn sort_key(path: &Path) -> SortKey {
    use std::os::windows::ffi::OsStrExt;
    path.file_name()
        .unwrap_or_default()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// `StrCmpLogicalW` is the comparison Explorer uses for its default name
/// sort. Reimplementing it in safe Rust could only approximate it, which is
/// the bug this replaced.
#[cfg(windows)]
fn compare_keys(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    crate::win32::compare_logical(a, b)
}

#[cfg(not(windows))]
type SortKey = String;

#[cfg(not(windows))]
fn sort_key(path: &Path) -> SortKey {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

#[cfg(not(windows))]
fn compare_keys(a: &str, b: &str) -> std::cmp::Ordering {
    human_sort(a, b)
}

/// Direction for single-step navigation.
#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

// ── Human / natural sort (non-Windows fallback) ───────────────────────────────
//
// Compares runs of digits numerically so that "img2" < "img10".

#[cfg(not(windows))]
fn human_sort(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let ord = consume_number(&mut ai).cmp(&consume_number(&mut bi));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(ac), Some(bc)) => {
                let ord = ac.cmp(&bc);
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Consume and return a run of ASCII digits as a `u64`, saturating on overflow.
#[cfg(not(windows))]
fn consume_number(iter: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut n = 0u64;
    while let Some(&c) = iter.peek() {
        let Some(digit) = c.to_digit(10) else { break };
        n = n.saturating_mul(10).saturating_add(digit as u64);
        iter.next();
    }
    n
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A scratch directory holding empty files with the given names.
    /// Content is irrelevant: the navigator only looks at names and types.
    fn dir_with(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        dir
    }

    fn names(nav: &DirNavigator) -> Vec<String> {
        nav.files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn open(dir: &tempfile::TempDir, name: &str) -> DirNavigator {
        let mut nav = DirNavigator::default();
        nav.set_current(dir.path().join(name));
        nav
    }

    fn current_name(nav: &DirNavigator) -> String {
        nav.current()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    // ── Listing ───────────────────────────────────────────────────────────────

    #[test]
    fn lists_only_supported_files_case_insensitively() {
        let dir = dir_with(&["a.png", "B.JPG", "notes.txt", "c.Webp", "no_extension"]);
        fs::create_dir(dir.path().join("folder.png")).unwrap();
        let nav = open(&dir, "a.png");
        assert_eq!(names(&nav), ["a.png", "B.JPG", "c.Webp"]);
    }

    #[test]
    fn numbers_sort_by_value() {
        let dir = dir_with(&["img10.png", "img2.png", "img1.png"]);
        assert_eq!(
            names(&open(&dir, "img1.png")),
            ["img1.png", "img2.png", "img10.png"]
        );
    }

    #[test]
    fn letter_case_does_not_affect_order() {
        let dir = dir_with(&["b.png", "A.png", "C.png"]);
        assert_eq!(names(&open(&dir, "b.png")), ["A.png", "b.png", "C.png"]);
    }

    /// The disagreement that motivated `StrCmpLogicalW`: by code point `é`
    /// sorts after `z`; Explorer sorts it with `e`.
    #[cfg(windows)]
    #[test]
    fn accented_letters_sort_with_their_base_letter() {
        let dir = dir_with(&["zebra.png", "\u{e9}lan.png", "eagle.png"]);
        assert_eq!(
            names(&open(&dir, "eagle.png")),
            ["eagle.png", "\u{e9}lan.png", "zebra.png"]
        );
    }

    /// Explorer's word sort gives a hyphen almost no weight, so `a-c` sorts
    /// as `ac` (after `ab`); by code point `-` sorts before every letter.
    #[cfg(windows)]
    #[test]
    fn hyphens_are_ignored_like_explorer() {
        let dir = dir_with(&["a-c.png", "ab.png"]);
        assert_eq!(names(&open(&dir, "ab.png")), ["ab.png", "a-c.png"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn human_sort_compares_digit_runs_numerically() {
        use std::cmp::Ordering::*;
        assert_eq!(human_sort("img2", "img10"), Less);
        assert_eq!(human_sort("img10", "img10"), Equal);
        assert_eq!(human_sort("img", "img1"), Less);
        assert_eq!(human_sort("99999999999999999999999", "1"), Greater);
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    #[test]
    fn next_and_prev_wrap_around() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let mut nav = open(&dir, "3.png");
        assert_eq!(nav.next().unwrap(), dir.path().join("1.png"));
        assert_eq!(nav.prev().unwrap(), dir.path().join("3.png"));
        assert_eq!(nav.prev().unwrap(), dir.path().join("2.png"));
    }

    #[test]
    fn peek_does_not_move() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let nav = open(&dir, "1.png");
        assert_eq!(nav.peek_next().unwrap(), dir.path().join("2.png"));
        assert_eq!(nav.peek_prev().unwrap(), dir.path().join("3.png"));
        assert_eq!(current_name(&nav), "1.png");
    }

    #[test]
    fn a_single_image_has_nowhere_to_go() {
        let dir = dir_with(&["only.png", "readme.txt"]);
        let mut nav = open(&dir, "only.png");
        assert_eq!(nav.next(), None);
        assert_eq!(nav.prev(), None);
        assert_eq!(nav.peek_next(), None);
    }

    #[test]
    fn removing_the_current_file_moves_to_the_next_sibling() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let mut nav = open(&dir, "2.png");
        nav.remove(&dir.path().join("2.png"));
        assert_eq!(current_name(&nav), "3.png");
        assert!(!nav.contains(&dir.path().join("2.png")));
    }

    #[test]
    fn removing_the_last_file_clamps_to_the_new_last() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let mut nav = open(&dir, "3.png");
        nav.remove(&dir.path().join("3.png"));
        assert_eq!(current_name(&nav), "2.png");
    }

    /// A delete confirmed after the user has navigated on removes a file
    /// that is no longer current; the view must stay where it is.
    #[test]
    fn removing_another_file_keeps_the_current_one() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let mut nav = open(&dir, "3.png");
        nav.remove(&dir.path().join("1.png"));
        assert_eq!(current_name(&nav), "3.png");
        nav.remove(&dir.path().join("4.png"));
        assert_eq!(current_name(&nav), "3.png");
    }

    #[test]
    fn rename_keeps_position() {
        let dir = dir_with(&["1.png", "2.png", "3.png"]);
        let mut nav = open(&dir, "2.png");
        nav.rename(&dir.path().join("2.png"), dir.path().join("2.jpg"));
        assert_eq!(current_name(&nav), "2.jpg");
        assert_eq!(nav.next().unwrap(), dir.path().join("3.png"));
    }

    #[test]
    fn opening_a_file_in_another_directory_rebuilds_the_list() {
        let first = dir_with(&["a.png", "b.png"]);
        let second = dir_with(&["x.png"]);
        let mut nav = open(&first, "a.png");
        nav.set_current(second.path().join("x.png"));
        assert_eq!(names(&nav), ["x.png"]);
    }

    #[test]
    fn rebuild_picks_up_external_changes_and_keeps_the_current_file() {
        let dir = dir_with(&["1.png", "3.png"]);
        let mut nav = open(&dir, "3.png");
        fs::write(dir.path().join("2.png"), b"").unwrap();
        fs::remove_file(dir.path().join("1.png")).unwrap();
        nav.rebuild_current(Some(&dir.path().join("3.png")));
        assert_eq!(names(&nav), ["2.png", "3.png"]);
        assert_eq!(current_name(&nav), "3.png");
    }

    #[test]
    fn rebuild_after_the_current_file_vanished_leaves_it_absent() {
        let dir = dir_with(&["1.png", "2.png"]);
        let mut nav = open(&dir, "2.png");
        fs::remove_file(dir.path().join("2.png")).unwrap();
        let gone = dir.path().join("2.png");
        nav.rebuild_current(Some(&gone));
        assert!(!nav.contains(&gone));
        assert_eq!(current_name(&nav), "1.png");
    }
}

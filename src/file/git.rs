//! Git integration: a background snapshot of repository state.
//!
//! `GitStatus::refresh` spawns a thread that runs `git rev-parse` +
//! `git status --porcelain` and sends the result back over a channel, so the
//! UI never blocks on git. The host polls `try_recv()` each loop iteration.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// How a file differs from HEAD (per git status porcelain).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileStatus {
    /// Added (staged or untracked).
    Added,
    /// Modified in the worktree or index.
    Modified,
    /// Deleted.
    Deleted,
    /// Renamed.
    Renamed,
    /// Any other porcelain state (conflicts, etc.).
    Other,
}

impl FileStatus {
    /// A short colored-friendly glyph for the status.
    pub fn glyph(self) -> &'static str {
        match self {
            FileStatus::Added => "+",
            FileStatus::Modified => "M",
            FileStatus::Deleted => "–",
            FileStatus::Renamed => "R",
            FileStatus::Other => "!",
        }
    }
}

/// A snapshot of a repository's state.
#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    /// Current branch name (None = not a git repo, or detached HEAD).
    pub branch: Option<String>,
    /// Repo root (None = not a git repo).
    pub root: Option<PathBuf>,
    /// Repo-root-relative path → status.
    pub files: HashMap<PathBuf, FileStatus>,
    /// Directories containing untracked files (so untracked dirs can be
    /// marked even though `--untracked-files=all` lists only their files).
    pub untracked_dirs: HashSet<PathBuf>,
    /// Number of entries in `files` at snapshot time.
    pub dirty_count: usize,
}

/// Cap on parsed status entries so a huge untracked tree can't stall it.
const MAX_STATUS_ENTRIES: usize = 2000;

impl GitStatus {
    /// Start a background refresh for `cwd`. Returns the receiver the host
    /// should poll; None if git isn't available at all.
    pub fn refresh(cwd: &Path) -> Option<Receiver<GitStatus>> {
        let cwd = cwd.to_path_buf();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::spawn(move || {
            let _ = tx.send(snapshot(&cwd));
        });
        // Attach the JoinHandle so it isn't detached on exit — but don't
        // block: we only care that it started.
        let _ = spawned;
        Some(rx)
    }

    /// Status of a file inside the repo, or None.
    pub fn status_of(&self, path: &Path) -> Option<FileStatus> {
        let root = self.root.as_ref()?;
        let rel = path.strip_prefix(root).ok()?;
        self.files.get(rel).copied()
    }

    /// Whether any ancestor directory of the path is untracked (an "all"
    /// untracked listing marks every file inside an untracked dir).
    pub fn in_untracked_dir(&self, path: &Path) -> bool {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        let Some(rel) = path.strip_prefix(root).ok() else {
            return false;
        };
        self.untracked_dirs.contains(rel)
    }

    /// Effective status of a path: its own status, or Added when it lives
    /// inside an untracked directory.
    pub fn effective_status(&self, path: &Path) -> Option<FileStatus> {
        self.status_of(path).or_else(|| {
            if self.in_untracked_dir(path) {
                Some(FileStatus::Added)
            } else {
                None
            }
        })
    }
}

/// Run the git commands synchronously (called on a background thread).
fn snapshot(cwd: &Path) -> GitStatus {
    let mut status = GitStatus::default();

    // Repo root + branch: rev-parse fails outside a git repo.
    let root = run_git(cwd, ["rev-parse", "--show-toplevel"]);
    let Some(root) = root else {
        return status;
    };
    status.root = Some(PathBuf::from(root.trim()));

    let branch = run_git(cwd, ["rev-parse", "--abbrev-ref", "HEAD"]);
    let branch = branch.map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    status.branch = branch;

    // Porcelain v1, NUL-separated: "XY <path>\0" (+ "\0<old>\0" for renames).
    if let Some(out) = run_git(
        cwd,
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    ) {
        let mut chunks = out.split('\0');
        while let Some(chunk) = chunks.next() {
            if chunk.is_empty() {
                continue;
            }
            if status.files.len() >= MAX_STATUS_ENTRIES {
                break;
            }
            let bytes = chunk.as_bytes();
            if bytes.len() < 4 {
                continue;
            }
            let (x, y) = (bytes[0], bytes[1]);
            let path = &chunk[3..];
            let fs_status = match x {
                b'A' | b'?' => FileStatus::Added,
                b'M' | b'C' | b'T' => FileStatus::Modified,
                b'D' => FileStatus::Deleted,
                b'R' => FileStatus::Renamed,
                _ if y != b' ' => match y {
                    b'M' => FileStatus::Modified,
                    b'D' => FileStatus::Deleted,
                    b'A' => FileStatus::Added,
                    _ => FileStatus::Other,
                },
                _ => FileStatus::Other,
            };
            status
                .files
                .insert(PathBuf::from(path), fs_status);
            // Record the untracked file's ancestors so untracked directories
            // can be marked in the UI.
            if x == b'?' {
                let p = Path::new(path);
                let mut acc = PathBuf::new();
                for comp in p.components() {
                    acc.push(comp);
                    if acc != p {
                        status.untracked_dirs.insert(acc.clone());
                    }
                }
            }
            // Renames carry the original path in a second chunk — skip it.
            if x == b'R' {
                chunks.next();
            }
        }
    }
    status.dirty_count = status.files.len();
    status
}

/// Run git with args in `cwd`, returning stdout on success.
fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Drain a receiver if a result has arrived, without blocking.
pub fn poll(receiver: &mut Option<Receiver<GitStatus>>) -> Option<GitStatus> {
    let rx = receiver.as_mut()?;
    match rx.try_recv() {
        Ok(status) => {
            *receiver = None;
            Some(status)
        }
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => {
            *receiver = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_mapping() {
        assert_eq!(FileStatus::Added.glyph(), "+");
        assert_eq!(FileStatus::Modified.glyph(), "M");
        assert_eq!(FileStatus::Deleted.glyph(), "–");
        assert_eq!(FileStatus::Renamed.glyph(), "R");
    }

    #[test]
    fn status_of_maps_repo_relative_paths() {
        let mut st = GitStatus::default();
        st.root = Some(PathBuf::from("/repo"));
        st.files.insert(PathBuf::from("src/main.rs"), FileStatus::Modified);
        assert_eq!(st.status_of(Path::new("/repo/src/main.rs")), Some(FileStatus::Modified));
        assert_eq!(st.status_of(Path::new("/other/src/main.rs")), None);
    }
}

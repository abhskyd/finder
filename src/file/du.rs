//! Disk usage: background computation of recursive directory sizes.
//!
//! `DirSizes::refresh` spawns a thread that walks the tree with `walkdir`
//! and sends a map of directory → total size back over a channel, so the UI
//! never blocks on I/O. The host polls `try_recv()` each loop iteration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use walkdir::WalkDir;

/// Cap on visited entries so a huge tree can't stall the computation.
const MAX_ENTRIES: usize = 200_000;

/// A snapshot of recursive directory sizes under a root.
#[derive(Clone, Debug, Default)]
pub struct DirSizes {
    /// The tree these sizes were computed for.
    pub root: PathBuf,
    /// Directory → total size of everything inside it (recursive).
    pub dirs: HashMap<PathBuf, u64>,
}

impl DirSizes {
    /// Start a background refresh for `root`. Returns the receiver the host
    /// should poll.
    pub fn refresh(root: &Path) -> Receiver<DirSizes> {
        let root = root.to_path_buf();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(compute(&root));
        });
        rx
    }

    /// Recursive size of a directory, or its own on-file size for files.
    pub fn size_of(&self, path: &Path) -> Option<u64> {
        self.dirs.get(path).copied()
    }
}

/// Compute recursive sizes for every directory under `root`.
fn compute(root: &Path) -> DirSizes {
    // Depth-first bottom-up: each parent accumulates its children's totals.
    let mut dirs: HashMap<PathBuf, u64> = HashMap::new();
    let mut files: Vec<(PathBuf, u64)> = Vec::new();

    for (visited, entry) in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if visited >= MAX_ENTRIES {
            break;
        }
        let path = entry.path().to_path_buf();
        if entry.file_type().is_dir() {
            dirs.insert(path, 0);
        } else if let Ok(meta) = entry.metadata() {
            files.push((path, meta.len()));
        }
    }

    for (path, size) in files {
        // Attribute the file to itself and every ancestor up to root.
        dirs.entry(path.clone()).and_modify(|s| *s += size);
        let mut acc = path.clone();
        while let Some(parent) = acc.parent() {
            if !dirs.contains_key(parent) {
                break;
            }
            *dirs.get_mut(parent).unwrap() += size;
            acc = parent.to_path_buf();
        }
    }

    DirSizes {
        root: root.to_path_buf(),
        dirs,
    }
}

/// Drain a receiver if a result has arrived, without blocking.
pub fn poll(receiver: &mut Option<Receiver<DirSizes>>) -> Option<DirSizes> {
    let rx = receiver.as_mut()?;
    match rx.try_recv() {
        Ok(sizes) => {
            *receiver = None;
            Some(sizes)
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
    use std::fs;

    #[test]
    fn sizes_are_recursive() {
        let tmp = std::env::temp_dir().join("finder_tui_test_du");
        let _ = fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        std::fs::write(tmp.join("a.txt"), [0u8; 100]).unwrap();
        std::fs::write(tmp.join("sub").join("b.txt"), [0u8; 50]).unwrap();

        let sizes = compute(&tmp);
        let total = sizes.dirs.get(&tmp).copied().unwrap();
        let sub = sizes.dirs.get(&tmp.join("sub")).copied().unwrap();
        assert!(total >= 150, "root should include both files, got {}", total);
        assert!(sub >= 50, "sub should include its file, got {}", sub);
        assert!(total > sub, "root must be larger than its subdir");

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}

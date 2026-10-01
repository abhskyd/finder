//! Filesystem explorer: navigation, search, sorting, copy/move.

use walkdir::WalkDir;
use std::path::{PathBuf, Path};
use std::fs;
use path_absolutize::Absolutize;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_exec: bool,
    pub size: u64,
    /// Modification time (seconds since the Unix epoch).
    pub mtime: u64,
    /// Unix permission mode bits (0 on non-unix platforms).
    pub mode: u32,
}

/// How entries are ordered (directories always come first).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    Name,
    Size,
    Ext,
    /// Modification time, newest first.
    Mtime,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::Name => SortMode::Size,
            SortMode::Size => SortMode::Ext,
            SortMode::Ext => SortMode::Mtime,
            SortMode::Mtime => SortMode::Name,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Name => "name",
            SortMode::Size => "size",
            SortMode::Ext => "ext",
            SortMode::Mtime => "date",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "name" | "n" => Some(SortMode::Name),
            "size" | "s" => Some(SortMode::Size),
            "ext" | "e" | "type" | "t" => Some(SortMode::Ext),
            "date" | "mtime" | "time" | "d" => Some(SortMode::Mtime),
            _ => None,
        }
    }
}

pub struct Explorer {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub clipboard: Vec<PathBuf>,
    pub cut_mode: bool,
    pub show_hidden: bool,
    pub search_mode: bool,
    pub search_pattern: String,
    pub all_entries: Vec<Entry>,
    pub sort_mode: SortMode,
    /// Items moved to the trash this session (newest last), for `:restore`.
    pub trash_history: Vec<TrashedItem>,
}

/// An item that was moved to the trash, kept so it can be restored.
#[derive(Clone, Debug)]
pub struct TrashedItem {
    /// Where the item lived before it was trashed.
    pub original: PathBuf,
    /// Where it sits inside the trash now.
    pub trash_path: PathBuf,
    pub name: String,
}

impl Explorer {
    pub fn new(start: impl AsRef<Path>) -> Self {
        // Absolutize so `cwd.parent()` works. A relative "." has parent
        // Some(""), which previously broke `h` navigation from the start dir.
        let cwd = start
            .as_ref()
            .absolutize()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| start.as_ref().to_path_buf());
        let mut entries = Self::read_dir(&cwd, false);
        Self::sort_entries(&mut entries, SortMode::Name);
        let entries = entries;
        Self {
            cwd,
            entries,
            selected: 0,
            clipboard: vec![],
            cut_mode: false,
            show_hidden: false,
            search_mode: false,
            search_pattern: String::new(),
            all_entries: vec![],
            sort_mode: SortMode::Name,
            trash_history: vec![],
        }
    }

    fn read_dir(dir: &Path, show_hidden: bool) -> Vec<Entry> {
        WalkDir::new(dir)
            .max_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            // Skip depth 0: WalkDir yields the directory itself as the first
            // entry, which previously showed up in the file list.
            .filter(|e| e.depth() > 0)
            .filter(|e| {
                if show_hidden {
                    true
                } else {
                    !e.file_name().to_str().is_some_and(|n| n.starts_with('.'))
                }
            })
            .filter_map(|e| {
                // Don't panic if metadata vanishes between listing and stat.
                let meta = e.metadata().ok()?;
                let is_symlink = e.file_type().is_symlink();
                #[cfg(unix)]
                let is_exec = !meta.is_dir()
                    && meta.permissions().mode() & 0o111 != 0;
                #[cfg(not(unix))]
                let is_exec = false;
                #[cfg(unix)]
                let mode = meta.permissions().mode();
                #[cfg(not(unix))]
                let mode: u32 = 0;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                Some(Entry {
                    path: e.path().to_path_buf(),
                    is_dir: meta.is_dir(),
                    is_symlink,
                    is_exec,
                    size: meta.len(),
                    mtime,
                    mode,
                })
            })
            .collect()
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
        self.refresh();
    }

    // navigation -------------------------------------------------------------
    pub fn move_up(&mut self) { 
        if self.selected > 0 { 
            self.selected -= 1; 
        } 
    }
    
    pub fn move_down(&mut self) { 
        if self.selected + 1 < self.entries.len() { 
            self.selected += 1; 
        } 
    }
    
    pub fn go_to_top(&mut self) {
        self.selected = 0;
    }
    
    pub fn go_to_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }
    
    pub fn go_up(&mut self) {
        // Remember which directory we're leaving so the cursor can land on it.
        let leaving = self.cwd.file_name().map(|n| n.to_os_string());
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.refresh();
            if let Some(name) = leaving {
                if let Some(idx) = self
                    .entries
                    .iter()
                    .position(|e| e.path.file_name() == Some(name.as_os_str()))
                {
                    self.selected = idx;
                }
            }
        }
    }
    
    pub fn enter_selected(&mut self) {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                self.cwd = entry.path.clone();
                self.refresh();
            }
        }
    }

    pub fn change_dir(&mut self, path: &str) -> std::io::Result<()> {
        let expanded = Self::expand_tilde(path);
        let new_path = if expanded.is_absolute() {
            expanded
        } else {
            self.cwd.join(&expanded)
        };
        let new_path = Self::normalize(&new_path);

        if new_path.is_dir() {
            self.cwd = new_path;
            self.refresh();
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("not a directory: {}", path),
            ))
        }
    }

    /// Expand a leading `~` to the user's home directory.
    fn expand_tilde(path: &str) -> PathBuf {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        if path == "~" {
            return home.unwrap_or_else(|| PathBuf::from(path));
        }
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = home {
                return home.join(rest);
            }
        }
        PathBuf::from(path)
    }

    /// Lexically resolve `.` and `..` components.
    fn normalize(path: &Path) -> PathBuf {
        use std::path::Component;
        let mut out = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() && path.is_absolute() {
                        out.push("/");
                    }
                }
                c => out.push(c.as_os_str()),
            }
        }
        if out.as_os_str().is_empty() {
            out.push(".");
        }
        out
    }

    /// Re-read the current directory (used by the `R` refresh key).
    pub fn refresh(&mut self) {
        // Remember the currently selected entry so the cursor can follow it.
        let remembered = self
            .entries
            .get(self.selected)
            .and_then(|e| e.path.file_name().map(|n| n.to_os_string()));
        if self.search_mode {
            self.all_entries = Self::read_dir(&self.cwd, self.show_hidden);
            Self::sort_entries(&mut self.all_entries, self.sort_mode);
            self.apply_search_filter();
        } else {
            self.entries = Self::read_dir(&self.cwd, self.show_hidden);
            Self::sort_entries(&mut self.entries, self.sort_mode);
            // Keep the cursor on the same entry if it still exists.
            self.selected = remembered
                .and_then(|name| {
                    self.entries
                        .iter()
                        .position(|e| e.path.file_name() == Some(name.as_os_str()))
                })
                .unwrap_or(0);
        }
    }

    // copy / move -------------------------------------------------------------
    /// Put `paths` on the clipboard (yank or cut).
    pub fn copy_paths(&mut self, paths: Vec<PathBuf>, cut: bool) {
        self.clipboard = paths;
        self.cut_mode = cut;
    }

    pub fn copy_selected(&mut self) {
        self.copy_paths(self.selected_paths(), false);
    }

    pub fn cut_selected(&mut self) {
        self.copy_paths(self.selected_paths(), true);
    }

    /// Paths of the currently selected entry.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.entries
            .get(self.selected)
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    /// Paste clipboard contents into the current directory.
    /// Returns the number of items successfully pasted.
    pub fn paste(&mut self) -> usize {
        let mut pasted = 0;
        for src in &self.clipboard {
            let Some(file_name) = src.file_name() else { continue };
            let dest = self.cwd.join(file_name);
            let result = if self.cut_mode {
                fs::rename(src, &dest)
            } else if src.is_dir() {
                copy_dir_recursive(src, &dest)
            } else {
                fs::copy(src, &dest).map(|_| ())
            };
            if result.is_ok() {
                pasted += 1;
            }
        }
        self.clipboard.clear();
        self.refresh();
        pasted
    }

    /// Delete the given paths permanently. Returns (deleted, failed) counts.
    pub fn delete_paths(&mut self, paths: &[PathBuf]) -> (usize, usize) {
        let mut ok = 0;
        let mut failed = 0;
        for p in paths {
            let result = if p.is_dir() {
                fs::remove_dir_all(p)
            } else {
                fs::remove_file(p)
            };
            if result.is_ok() {
                ok += 1;
            } else {
                failed += 1;
            }
        }
        self.refresh();
        (ok, failed)
    }

    // trash ------------------------------------------------------------------
    /// OS trash location (None = unsupported on this platform).
    pub fn trash_dir() -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".Trash"))
        } else if cfg!(target_os = "linux") {
            // XDG trash spec: $XDG_DATA_HOME/Trash, default ~/.local/share/Trash
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| home_dir_fallback().map(|h| h.join(".local").join("share")));
            data_home.map(|d| d.join("Trash"))
        } else {
            None
        }
    }

    /// Move the given paths to the OS trash. Returns (moved, failed).
    pub fn trash_paths(&mut self, paths: &[PathBuf]) -> (usize, usize) {
        let Some(trash_dir) = Self::trash_dir() else {
            return (0, paths.len());
        };
        if fs::create_dir_all(&trash_dir).is_err() {
            return (0, paths.len());
        }
        let mut moved = 0;
        let mut failed = 0;
        for p in paths {
            let Some(name) = p.file_name() else {
                failed += 1;
                continue;
            };
            let dest = Self::unique_dest(&trash_dir, name);
            match fs::rename(p, &dest) {
                Ok(()) => {
                    moved += 1;
                    self.trash_history.push(TrashedItem {
                        original: p.clone(),
                        trash_path: dest,
                        name: name.to_string_lossy().to_string(),
                    });
                }
                Err(_) => failed += 1,
            }
        }
        self.refresh();
        (moved, failed)
    }

    /// A non-colliding destination inside `dir` for `name` (`name_2.ext`, …).
    fn unique_dest(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
        let base = dir.join(name);
        if !base.exists() {
            return base;
        }
        let stem = Path::new(name)
            .file_stem()
            .unwrap_or(name)
            .to_string_lossy()
            .to_string();
        let ext = Path::new(name)
            .extension()
            .map(|x| format!(".{}", x.to_string_lossy()))
            .unwrap_or_default();
        for i in 2..10000 {
            let candidate = dir.join(format!("{}_{}{}", stem, i, ext));
            if !candidate.exists() {
                return candidate;
            }
        }
        base
    }

    /// Restore the most recently trashed item. Returns a status message.
    pub fn restore_last(&mut self) -> String {
        let Some(item) = self.trash_history.pop() else {
            return "Nothing to restore".into();
        };
        let original_ok = item
            .original
            .parent()
            .map(|p| p.exists())
            .unwrap_or(false);
        // If the original directory is gone, restore into the current one.
        let dest = if original_ok {
            item.original.clone()
        } else {
            self.cwd.join(&item.name)
        };
        match fs::rename(&item.trash_path, &dest) {
            Ok(()) => {
                self.refresh();
                format!("Restored {}", item.name)
            }
            Err(_) => {
                self.trash_history.push(item);
                "Restore failed".into()
            }
        }
    }

    /// Rename the selected entry to `new_name`.
    pub fn rename_selected_to(&mut self, new_name: &str) -> std::io::Result<()> {
        let entry = self.entries.get(self.selected).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "nothing selected")
        })?;
        let old_path = entry.path.clone();
        let new_path = self.cwd.join(new_name);
        fs::rename(&old_path, &new_path)?;
        self.refresh();
        Ok(())
    }

    pub fn mkdir(&mut self, name: &str) -> std::io::Result<()> {
        let new_path = self.cwd.join(name);
        fs::create_dir_all(&new_path)?;
        self.refresh();
        Ok(())
    }

    pub fn touch(&mut self, name: &str) -> std::io::Result<()> {
        let new_path = self.cwd.join(name);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&new_path)?;
        self.refresh();
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> std::io::Result<()> {
        let path = self.cwd.join(name);
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
        self.refresh();
        Ok(())
    }

    // archive extract / compress ------------------------------------------------

    /// Extract the selected archive into `dest` (default: a sibling directory
    /// named after the archive). Returns a status message.
    #[cfg(feature = "archive")]
    pub fn extract_selected(&mut self, dest: Option<&str>) -> Result<String, String> {
        let entry = self
            .entries
            .get(self.selected)
            .ok_or_else(|| "nothing selected".to_string())?;
        let kind = archive_kind(&entry.path)
            .ok_or_else(|| "not an archive (zip/tar/tar.gz)".to_string())?;
        let name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let dest_dir = match dest {
            Some(d) if !d.is_empty() => self.cwd.join(d),
            _ => {
                // Default: a directory named after the archive.
                let stem = entry
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "extracted".into());
                self.cwd.join(stem)
            }
        };
        std::fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir failed: {}", e))?;

        let file = fs::File::open(&entry.path).map_err(|e| format!("open failed: {}", e))?;
        let result = match kind {
            "zip" => {
                let mut z = zip::ZipArchive::new(std::io::BufReader::new(file))
                    .map_err(|e| format!("bad zip: {}", e))?;
                z.extract(&dest_dir).map_err(|e| format!("extract failed: {}", e))
            }
            "tar" | "tar.gz" => {
                let reader: Box<dyn std::io::Read> = if kind == "tar.gz" {
                    Box::new(flate2::read::GzDecoder::new(file))
                } else {
                    Box::new(file)
                };
                tar::Archive::new(reader)
                    .unpack(&dest_dir)
                    .map_err(|e| format!("extract failed: {}", e))
            }
            _ => Err("unsupported archive".into()),
        };
        match result {
            Ok(()) => {
                self.refresh();
                Ok(format!(
                    "Extracted {} → {}",
                    name,
                    crate::config::Config::shorten_path(&dest_dir)
                ))
            }
            Err(e) => Err(e),
        }
    }

    /// Compress `paths` into `name` (a .zip or .tar.gz) in the current
    /// directory. Returns a status message.
    #[cfg(feature = "archive")]
    pub fn compress_paths(&mut self, paths: &[PathBuf], name: &str) -> Result<String, String> {
        if paths.is_empty() {
            return Err("nothing selected".into());
        }
        let lower = name.to_lowercase();
        if !lower.ends_with(".zip") && !lower.ends_with(".tar.gz") && !lower.ends_with(".tgz") {
            return Err("archive name must end with .zip or .tar.gz".into());
        }
        let dest = self.cwd.join(name);
        if dest.exists() {
            return Err(format!("{} already exists", name));
        }
        let file = fs::File::create(&dest).map_err(|e| format!("create failed: {}", e))?;
        if lower.ends_with(".zip") {
            let mut z = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            for p in paths {
                add_to_zip(&mut z, p, p, &options)?;
            }
            z.finish().map_err(|e| format!("zip failed: {}", e))?;
        } else {
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut b = tar::Builder::new(gz);
            for p in paths {
                let fname = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if p.is_dir() {
                    b.append_dir_all(&fname, p)
                        .map_err(|e| format!("tar failed: {}", e))?;
                } else {
                    b.append_path_with_name(p, &fname)
                        .map_err(|e| format!("tar failed: {}", e))?;
                }
            }
            b.into_inner()
                .map_err(|e| format!("tar failed: {}", e))?
                .finish()
                .map_err(|e| format!("gzip failed: {}", e))?;
        }
        self.refresh();
        Ok(format!("Created {} ({} item(s))", name, paths.len()))
    }

    // search -----------------------------------------------------------------
    /// Begin a live search: snapshot the unfiltered listing and show it all.
    pub fn start_search(&mut self) {
        self.search_mode = true;
        self.search_pattern.clear();
        self.all_entries = Self::read_dir(&self.cwd, self.show_hidden);
        Self::sort_entries(&mut self.all_entries, self.sort_mode);
        self.entries = self.all_entries.clone();
        self.selected = 0;
    }

    /// Update the filter for the current search pattern (as-you-type).
    pub fn search_live(&mut self, pattern: &str) {
        self.search_pattern = pattern.to_string();
        self.apply_search_filter();
    }

    /// Run a one-shot search from command mode.
    pub fn search(&mut self, pattern: String) {
        self.search_mode = true;
        self.search_pattern = pattern;
        self.all_entries = Self::read_dir(&self.cwd, self.show_hidden);
        Self::sort_entries(&mut self.all_entries, self.sort_mode);
        self.apply_search_filter();
    }

    fn apply_search_filter(&mut self) {
        if self.search_pattern.is_empty() {
            self.entries = self.all_entries.clone();
        } else {
            let pattern = self.search_pattern.to_lowercase();
            self.entries = self.all_entries.iter()
                .filter(|e| {
                    e.path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_lowercase().contains(&pattern))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
        }
        self.selected = 0;
    }

    pub fn clear_search(&mut self) {
        self.search_mode = false;
        self.search_pattern.clear();
        self.all_entries.clear();
        self.entries = Self::read_dir(&self.cwd, self.show_hidden);
        Self::sort_entries(&mut self.entries, self.sort_mode);
        self.selected = 0;
    }

    // sorting ----------------------------------------------------------------
    /// Change the sort mode, keeping the cursor on the same entry.
    pub fn set_sort_mode(&mut self, mode: SortMode) {
        self.sort_mode = mode;
        self.refresh();
    }

    fn sort_entries(entries: &mut [Entry], mode: SortMode) {
        entries.sort_by(|a, b| {
            // Directories always come first.
            b.is_dir.cmp(&a.is_dir).then_with(|| match mode {
                SortMode::Name => name_key(a).cmp(&name_key(b)),
                SortMode::Size => a.size.cmp(&b.size),
                SortMode::Ext => ext_key(a)
                    .cmp(&ext_key(b))
                    .then_with(|| name_key(a).cmp(&name_key(b))),
                SortMode::Mtime => b.mtime
                    .cmp(&a.mtime)
                    .then_with(|| name_key(a).cmp(&name_key(b))),
            })
        });
    }

    // navigation helpers ------------------------------------------------------
    /// Jump to the next entry, wrapping around.
    pub fn next_match(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    /// Jump to the previous entry, wrapping around.
    pub fn prev_match(&mut self) {
        if !self.entries.is_empty() {
            self.selected = if self.selected == 0 {
                self.entries.len() - 1
            } else {
                self.selected - 1
            };
        }
    }
}

/// Archive container kind by extension (None = not a zip/tar/tar.gz).
pub fn archive_kind(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())?
        .to_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some("tar.gz");
    }
    let ext = path.extension().and_then(|s| s.to_str())?.to_lowercase();
    match ext.as_str() {
        "zip" => Some("zip"),
        "tar" => Some("tar"),
        "gz" => Some("tar.gz"),
        _ => None,
    }
}

/// Add a file or directory tree to a zip archive (runs recursively; entries
/// are named relative to `root`'s parent).
#[cfg(feature = "archive")]
fn add_to_zip<W: std::io::Write + std::io::Seek>(
    z: &mut zip::ZipWriter<W>,
    root: &Path,
    p: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let rel = p
        .strip_prefix(root.parent().unwrap_or(root))
        .map(|r| r.display().to_string())
        .unwrap_or_else(|_| p.display().to_string());
    if p.is_dir() {
        z.add_directory(format!("{}/", rel), *options)
            .map_err(|e| e.to_string())?;
        for e in fs::read_dir(p).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            add_to_zip(z, root, &e.path(), options)?;
        }
    } else {
        z.start_file(&rel, *options)
            .map_err(|e| e.to_string())?;
        let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, z).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Case-insensitive file name used as the sort key.
fn name_key(e: &Entry) -> String {
    e.path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Lowercased extension used as the sort key (empty if none).
fn ext_key(e: &Entry) -> String {
    e.path
        .extension()
        .map(|x| x.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// What kind of thing an entry is — drives eza-style icon colors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconClass {
    Dir,
    Symlink,
    Exec,
    Source,
    Doc,
    Archive,
    Config,
    Media,
    Audio,
    Book,
    Text,
}

/// eza-style Nerd Font icon for an entry, plus its color class.
///
/// Glyphs match eza v0.23.5's icon set (requires a Nerd Font, e.g.
/// JetBrainsMono Nerd Font). Exact file names win over extensions, like
/// eza's `by_file_name` map.
pub fn icon(e: &Entry) -> (&'static str, IconClass) {
    if e.is_dir {
        return ("\u{f115}", IconClass::Dir); //  folder-open
    }
    if e.is_symlink {
        return ("\u{f086f}", IconClass::Symlink); // 󰁯 link
    }

    // Exact file names first (eza's by_file_name map).
    let name = e
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    if let Some(hit) = match name.as_str() {
        "license" | "licence" | "readme" | "readme.txt" => {
            Some(("\u{f02d}", IconClass::Book)) //  book
        }
        "dockerfile" | "docker-compose.yml" | "docker-compose.yaml" => {
            Some(("\u{f308}", IconClass::Config)) //  docker
        }
        ".git" | ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => {
            Some(("\u{f1d3}", IconClass::Config)) //  git
        }
        "cargo.lock" | "cargo.toml" => Some(("\u{e68b}", IconClass::Source)), //  rust (like eza)
        "go.mod" | "go.sum" => Some(("\u{e65e}", IconClass::Source)), //  go
        "makefile" | "makefile.am" | "makefile.in" => {
            Some(("\u{f489}", IconClass::Archive)) //  terminal
        }
        _ => None,
    } {
        return hit;
    }

    // Then by extension.
    let ext = e
        .path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_lowercase();
    let by_ext = match ext.as_str() {
        "rs" => ("\u{e68b}", IconClass::Source), //  rust
        "js" | "mjs" | "cjs" => ("\u{e74e}", IconClass::Source), //  js
        "ts" | "tsx" => ("\u{e628}", IconClass::Source), //  ts
        "c" => ("\u{e61e}", IconClass::Source), //  c
        "cpp" | "cc" | "hpp" | "h" => ("\u{e61d}", IconClass::Source), //  cpp
        "py" => ("\u{e606}", IconClass::Source), //  python
        "go" => ("\u{e65e}", IconClass::Source), //  go
        "mod" | "sum" => ("\u{e65e}", IconClass::Source), //  go modules
        "java" => ("\u{f0793}", IconClass::Source), // 󰎓 java
        "rb" => ("\u{e739}", IconClass::Source), //  ruby
        "php" => ("\u{e608}", IconClass::Source), //  php
        "html" | "htm" => ("\u{e736}", IconClass::Source), //  html5
        "css" | "scss" => ("\u{e749}", IconClass::Source), //  css3
        "md" | "markdown" => ("\u{f00ba}", IconClass::Source), // 󰂺 markdown
        "json" => ("\u{e60b}", IconClass::Config), //  json
        "toml" => ("\u{e6b2}", IconClass::Config), //  toml
        "yaml" | "yml" | "ini" | "cfg" | "conf" => ("\u{e615}", IconClass::Config), //  config
        "txt" | "log" => ("\u{f15c}", IconClass::Text), //  file-text
        "pdf" => ("\u{f1c1}", IconClass::Doc), //  pdf
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "ico" | "svg" => {
            ("\u{f1c5}", IconClass::Media) //  image
        }
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" => {
            ("\u{f410}", IconClass::Archive) //  archive
        }
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => ("\u{f001}", IconClass::Audio), //  music
        "mp4" | "mkv" | "avi" | "mov" | "webm" => ("\u{f03d}", IconClass::Media), //  film
        "sh" | "bash" | "zsh" | "fish" => ("\u{f489}", IconClass::Archive), //  terminal (eza: red)
        "lock" => ("\u{f023}", IconClass::Config), //  lock
        "exe" | "dll" | "so" | "dylib" | "bin" => ("\u{f2db}", IconClass::Exec), //  microchip
        _ => ("", IconClass::Text),
    };
    if !by_ext.0.is_empty() {
        return by_ext;
    }

    if e.is_exec {
        return ("\u{f489}", IconClass::Exec); //  terminal
    }
    ("\u{f15c}", IconClass::Text) //  file-text, eza's fallback
}

/// Human-readable file size, shared by the list and the preview.
pub fn format_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut s = size as f64;
    let mut u = 0;
    while s >= 1024.0 && u < UNITS.len() - 1 {
        s /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} B", size)
    } else {
        format!("{:.1} {}", s, UNITS[u])
    }
}

/// Short modification date for the list's date column: `MM-DD HH:MM` (UTC).
pub fn format_date_short(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, mi) = (rem / 3600, (rem % 3600) / 60);

    // Howard Hinnant's days-from-civil inverse (same math as the preview's
    // full formatter, minus the year).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let _ = y;

    format!("{:02}-{:02} {:02}:{:02}", m, d, h, mi)
}

fn home_dir_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dot_components() {
        assert_eq!(Explorer::normalize(Path::new("/a/b/..")), PathBuf::from("/a"));
        assert_eq!(Explorer::normalize(Path::new("/a/b/../..")), PathBuf::from("/"));
        assert_eq!(Explorer::normalize(Path::new("/a/./c")), PathBuf::from("/a/c"));
        assert_eq!(Explorer::normalize(Path::new("/..")), PathBuf::from("/"));
    }

    #[test]
    fn expand_tilde_expands_home() {
        std::env::set_var("HOME", "/home/testuser");
        assert_eq!(Explorer::expand_tilde("~"), PathBuf::from("/home/testuser"));
        assert_eq!(
            Explorer::expand_tilde("~/docs"),
            PathBuf::from("/home/testuser/docs")
        );
        assert_eq!(Explorer::expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    }

    #[test]
    fn read_dir_skips_the_directory_itself() {
        let tmp = std::env::temp_dir().join("my_tui_fm_test_self");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("a.txt"), "x").unwrap();

        let entries = Explorer::read_dir(&tmp, true);
        assert!(!entries.iter().any(|e| e.path == tmp), "dir itself must not be listed");
        assert!(entries.iter().any(|e| e.path.file_name() == Some("a.txt".as_ref())));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn read_dir_hides_dotfiles_unless_requested() {
        let tmp = std::env::temp_dir().join("my_tui_fm_test_hidden");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(".hidden"), "x").unwrap();
        fs::write(tmp.join("visible"), "x").unwrap();

        let shown = Explorer::read_dir(&tmp, false);
        assert!(shown.iter().all(|e| e.path.file_name() != Some(".hidden".as_ref())));

        let all = Explorer::read_dir(&tmp, true);
        assert!(all.iter().any(|e| e.path.file_name() == Some(".hidden".as_ref())));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn sort_entries_dirs_first_then_mode() {
        let mk = |p: &str, dir: bool, size: u64| Entry {
            path: PathBuf::from(p),
            is_dir: dir,
            is_symlink: false,
            is_exec: false,
            size,
            mtime: 0,
            mode: 0,
        };
        let mut entries = vec![
            mk("z_file", false, 10),
            mk("a_dir", true, 0),
            mk("b_file", false, 2),
            mk("A_FILE", false, 100),
        ];

        Explorer::sort_entries(&mut entries, SortMode::Name);
        let names: Vec<&str> = entries.iter().map(|e| e.path.to_str().unwrap()).collect();
        assert_eq!(names, vec!["a_dir", "A_FILE", "b_file", "z_file"],
            "dirs first, then case-insensitive by name");

        Explorer::sort_entries(&mut entries, SortMode::Size);
        let sizes: Vec<u64> = entries.iter().map(|e| e.size).collect();
        assert_eq!(sizes.first(), Some(&0), "dir first");
        assert_eq!(&sizes[1..], &[2, 10, 100]);
    }

    #[test]
    fn sort_mode_cycles() {
        assert_eq!(SortMode::Name.next(), SortMode::Size);
        assert_eq!(SortMode::Size.next(), SortMode::Ext);
        assert_eq!(SortMode::Ext.next(), SortMode::Mtime);
        assert_eq!(SortMode::Mtime.next(), SortMode::Name);
    }

    #[test]
    fn sort_entries_mtime_newest_first() {
        let mk = |p: &str, dir: bool, mtime: u64| Entry {
            path: PathBuf::from(p),
            is_dir: dir,
            is_symlink: false,
            is_exec: false,
            size: 0,
            mtime,
            mode: 0,
        };
        let mut entries = vec![
            mk("old", false, 100),
            mk("new", false, 300),
            mk("mid", false, 200),
        ];
        Explorer::sort_entries(&mut entries, SortMode::Mtime);
        let mtimes: Vec<u64> = entries.iter().map(|e| e.mtime).collect();
        assert_eq!(mtimes, vec![300, 200, 100], "newest first");
    }
}
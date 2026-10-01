//! Sidebar: places, disks and bookmarks — quick navigation in one pane.
//!
//! Places are common user directories (that exist), disks come from `df`
//! (queried on a background thread), bookmarks from the config.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::config::Config;

/// One row in the sidebar.
#[derive(Clone, Debug)]
pub enum SidebarItem {
    /// A section header (not selectable).
    Header(&'static str),
    Place { label: String, path: PathBuf },
    Disk { mount: PathBuf, free: u64 },
    Bookmark { key: char, path: PathBuf },
}

impl SidebarItem {
    /// Whether the item can be selected/jumped to.
    pub fn selectable(&self) -> bool {
        !matches!(self, SidebarItem::Header(_))
    }

    /// The jump target, if any.
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            SidebarItem::Place { path, .. }
            | SidebarItem::Disk { mount: path, .. }
            | SidebarItem::Bookmark { path, .. } => Some(path),
            SidebarItem::Header(_) => None,
        }
    }

    /// Nerd Font icon for the item.
    pub fn icon(&self) -> &'static str {
        match self {
            SidebarItem::Header(_) => "",
            SidebarItem::Place { label, .. } => match label.as_str() {
                "home" => "\u{f015}",       // home
                "desktop" => "\u{f108}",    // desktop
                "downloads" => "\u{f019}",  // download
                "documents" => "\u{f016}",  // file-text-o
                "pictures" => "\u{f03e}",   // picture-o
                "music" => "\u{f001}",      // music
                _ => "\u{f03d}",            // film (videos)
            },
            SidebarItem::Disk { .. } => "\u{f0a0}",   // hdd-o
            SidebarItem::Bookmark { .. } => "\u{f006}", // star-o
        }
    }
}

/// One mounted disk from `df`.
#[derive(Clone, Debug)]
pub struct Disk {
    pub mount: PathBuf,
    /// Free bytes.
    pub free: u64,
}

/// Sidebar state: items + selection + scroll.
pub struct Sidebar {
    pub selected: usize,
    pub scroll: usize,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll: 0,
        }
    }

    /// Build the item list (cheap — the host calls it every draw).
    pub fn items(&self, disks: &[Disk], bookmarks: &BTreeMap<String, String>) -> Vec<SidebarItem> {
        build_items(disks, bookmarks)
    }

    /// Move the selection down, skipping section headers.
    pub fn move_down(&mut self, items: &[SidebarItem]) {
        let mut i = self.selected;
        while i + 1 < items.len() {
            i += 1;
            if items[i].selectable() {
                self.selected = i;
                return;
            }
        }
    }

    /// Move the selection up, skipping section headers.
    pub fn move_up(&mut self, items: &[SidebarItem]) {
        let mut i = self.selected;
        while i > 0 {
            i -= 1;
            if items[i].selectable() {
                self.selected = i;
                return;
            }
        }
    }

    /// Jump to the first selectable item.
    pub fn go_to_top(&mut self, items: &[SidebarItem]) {
        self.selected = items
            .iter()
            .position(|i| i.selectable())
            .unwrap_or(0);
    }

    /// Keep the scroll offset so the selection stays visible.
    pub fn clamp_scroll(&mut self, visible: usize) {
        if visible == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
    }
}

/// Build the sidebar items: places (that exist), disks, bookmarks.
fn build_items(disks: &[Disk], bookmarks: &BTreeMap<String, String>) -> Vec<SidebarItem> {
    let mut items = vec![SidebarItem::Header("Places")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        items.push(SidebarItem::Place {
            label: "home".into(),
            path: home.clone(),
        });
        for (label, sub) in [
            ("desktop", "Desktop"),
            ("documents", "Documents"),
            ("downloads", "Downloads"),
            ("pictures", "Pictures"),
            ("music", "Music"),
            // macOS uses Movies, Linux uses Videos.
            ("videos", "Movies"),
            ("videos", "Videos"),
        ] {
            let p = home.join(sub);
            // Skip duplicates (only one of Movies/Videos exists per OS).
            if p.is_dir() && !items.iter().any(|i| i.path() == Some(&p)) {
                items.push(SidebarItem::Place {
                    label: label.to_string(),
                    path: p,
                });
            }
        }
    }
    if !disks.is_empty() {
        items.push(SidebarItem::Header("Disks"));
        for d in disks {
            items.push(SidebarItem::Disk {
                mount: d.mount.clone(),
                free: d.free,
            });
        }
    }
    if !bookmarks.is_empty() {
        items.push(SidebarItem::Header("Bookmarks"));
        for (k, path) in bookmarks {
            if let Some(c) = k.chars().next() {
                items.push(SidebarItem::Bookmark {
                    key: c,
                    path: Config::expand_path(path),
                });
            }
        }
    }
    items
}

/// Query mounted disks with `df` (runs on a background thread).
pub fn refresh_disks() -> Receiver<Vec<Disk>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(query_disks());
    });
    rx
}

/// Drain a receiver if a result has arrived, without blocking.
pub fn poll(receiver: &mut Option<Receiver<Vec<Disk>>>) -> Option<Vec<Disk>> {
    let rx = receiver.as_mut()?;
    match rx.try_recv() {
        Ok(disks) => {
            *receiver = None;
            Some(disks)
        }
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            *receiver = None;
            None
        }
    }
}

/// Parse `df -l -k -P` output into a list of mounted disks.
fn query_disks() -> Vec<Disk> {
    let Some(out) = std::process::Command::new("df")
        .args(["-l", "-k", "-P"])
        .output()
        .ok()
        .filter(|o| o.status.success())
    else {
        return Vec::new();
    };
    let Some(text) = String::from_utf8(out.stdout).ok() else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut disks = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        // POSIX df format: fs blocks used avail capacity mount (mount can
        // contain spaces, so rejoin the tail).
        let mount: PathBuf = fields[5..].join(" ").into();
        if skip_mount(&mount) || !seen.insert(mount.clone()) {
            continue;
        }
        let free_kb: u64 = fields[3].parse().unwrap_or(0);
        disks.push(Disk {
            mount,
            free: free_kb * 1024,
        });
    }
    disks
}

/// Hide virtual / noisy mounts (macOS system volumes, /dev, …).
fn skip_mount(m: &Path) -> bool {
    let s = m.to_string_lossy();
    if s == "/dev" || s == "/private/var/vm" || s == "/System/Volumes/VM" {
        return true;
    }
    if s.starts_with("/System/Volumes/") && s != "/System/Volumes/Data" {
        return true;
    }
    false
}

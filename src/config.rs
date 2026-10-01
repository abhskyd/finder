//! Configuration file support: `~/.config/finder/config.toml`.
//!
//! Holds general settings, the theme, and directory bookmarks. The config is
//! loaded at startup and saved when bookmarks/theme change and on exit.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub theme: ThemeSection,
    /// Bookmark key → path (kept sorted for stable files).
    #[serde(default)]
    pub bookmarks: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct General {
    #[serde(default = "default_start_dir")]
    pub start_dir: String,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Places/disks/bookmarks sidebar pane visible.
    #[serde(default = "default_sidebar")]
    pub sidebar: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            start_dir: default_start_dir(),
            show_hidden: false,
            sort: default_sort(),
            sidebar: default_sidebar(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ThemeSection {
    #[serde(default = "default_theme_name")]
    pub name: String,
}

impl Default for ThemeSection {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
        }
    }
}

fn default_start_dir() -> String {
    ".".into()
}

fn default_sort() -> String {
    "name".into()
}

fn default_sidebar() -> bool {
    true
}

fn default_theme_name() -> String {
    "mocha".into()
}

impl Config {
    /// Path of the config file: `~/.config/finder/config.toml`.
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home_dir().map(|h| h.join(".config")))?;
        Some(base.join("finder").join("config.toml"))
    }

    /// Load the config, falling back to defaults for missing fields/files.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(data) => toml::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the config to disk.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, self.render())
    }

    /// Expand a leading `~` to the home directory.
    pub fn expand_path(p: &str) -> PathBuf {
        if p == "~" {
            if let Some(home) = home_dir() {
                return home;
            }
        } else if let Some(rest) = p.strip_prefix("~/") {
            if let Some(home) = home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(p)
    }

    /// Collapse $HOME to `~` for storage/display (portable configs).
    pub fn shorten_path(p: &Path) -> String {
        let mut homes: Vec<PathBuf> = home_dir().into_iter().collect();
        // Also compare against the canonicalized HOME so symlinked paths
        // (e.g. /tmp -> /private/tmp on macOS) still shorten.
        if let Some(h) = home_dir() {
            if let Ok(c) = std::fs::canonicalize(&h) {
                homes.push(c);
            }
        }
        for home in homes {
            if let Ok(rest) = p.strip_prefix(&home) {
                // strip_prefix removes whole components, so re-add the slash.
                if rest.as_os_str().is_empty() {
                    return "~".to_string();
                }
                return format!("~/{}", rest.display());
            }
        }
        p.display().to_string()
    }

    /// The config file with a helpful header, as written to disk.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# finder configuration\n\
             # theme: mocha | macchiato | tokyo night | rose pine | nord\n\
             # bookmarks: `<key> = \"<path>\"` — jump with ' + key\n\n",
        );
        out.push_str(&toml::to_string_pretty(self).unwrap_or_default());
        out
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

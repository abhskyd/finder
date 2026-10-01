//! Plugin manager: dynamic loading and dispatch.

use libloading::Library;
use std::path::PathBuf;
use crate::plugin::api::Plugin;
use crate::ui::app::App;

/// Context passed to plugins during initialization.
pub struct PluginContext<'a> {
    /// Mutable reference to the main application so plugins can interact.
    /// (Read by plugins, not by the host.)
    #[allow(dead_code)]
    pub app: &'a mut App,
}



/// A loaded plugin with its library kept alive.
pub(crate) struct LoadedPlugin {
    _lib: Library,
    instance: Box<dyn Plugin>,
}

#[derive(Default)]
pub struct PluginManager {
    pub plugins: Vec<LoadedPlugin>,
    path: PathBuf,
    /// Whether plugin UI overlay is visible.
    pub show_ui: bool,
}

impl PluginManager {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { plugins: vec![], path: path.into(), show_ui: false }
    }

    /// Load all dynamic libraries from the plugin directory.
    pub fn load_all(&mut self, app: &mut crate::ui::app::App) {
        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let is_lib = match path.extension().and_then(|s| s.to_str()) {
                    #[cfg(target_os = "macos")] Some("dylib") => true,
                    #[cfg(target_os = "linux")] Some("so") => true,
                    #[cfg(target_os = "windows")] Some("dll") => true,
                    _ => false,
                };
                if !is_lib { continue; }

                unsafe {
                    if let Ok(lib) = Library::new(&path) {
                        if let Ok(ctor) = lib.get::<unsafe extern "C" fn() -> *mut dyn Plugin>(b"plugin_entry") {
                            let raw = ctor();
                            if !raw.is_null() {
                                let mut plugin = Box::from_raw(raw);
                                let mut ctx = PluginContext { app };
                                plugin.init(&mut ctx);
                                self.plugins.push(LoadedPlugin { _lib: lib, instance: plugin });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draw all plugins that implement `draw`.
    pub fn draw(&self, f: &mut ratatui::Frame<'_>) {
        if !self.show_ui { return; }
        for p in &self.plugins {
            p.instance.draw(f);
        }
    }

    /// Forward a key event to all plugins.
    pub fn on_key(&mut self, key: crossterm::event::KeyEvent) {
        for p in &mut self.plugins {
            p.instance.on_key(key);
        }
    }

    /// Toggle the plugin UI overlay.
    pub fn toggle_ui(&mut self) {
        self.show_ui = !self.show_ui;
    }
}
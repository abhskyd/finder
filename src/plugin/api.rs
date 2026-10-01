//! Plugin API definition.

use crossterm::event::KeyEvent;
use ratatui::Frame;

/// Information about a plugin.
///
/// Part of the plugin API surface; unused by the host itself.
#[allow(dead_code)]
#[derive(Debug)]
pub struct PluginInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
}

/// Trait that all plugins must implement.
pub trait Plugin: Send {
    /// Called once after the plugin is loaded.
    fn init(&mut self, ctx: &mut crate::plugin::manager::PluginContext);
    /// Optional: handle a custom key press.
    fn on_key(&mut self, _key: KeyEvent) {}
    /// Optional: render a small overlay UI.
    fn draw(&self, _f: &mut Frame<'_>) {}
}

/// Symbol the host looks up. Plugins must export this function.
///
/// The host will call it and expect a pointer to a trait object.
///
/// `*mut dyn Plugin` is a fat pointer; it is not C-ABI portable, but host and
/// plugins are expected to be built with the same compiler, where this
/// representation is stable.
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn plugin_entry() -> *mut dyn Plugin {
    // Default stub – real plugins will provide their own implementation.
    unimplemented!("plugin_entry must be implemented by the plugin")
}
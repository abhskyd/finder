//! UI application state and main event loop.

use ratatui::Terminal;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use crate::config::Config;
use crate::file::explorer::Explorer;
use crate::file::fuzzy::FuzzyFinder;
use crate::file::git::GitStatus;
use crate::file::du::DirSizes;
use crate::preview::text::Preview;
use crate::theme::styles::Theme;
use crate::plugin::manager::PluginManager;
use crate::ui::sidebar::{self, Disk, Sidebar, SidebarItem};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Application input mode (nvim-style).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Visual,
    Search,
    Command,
}

pub struct App {
    pub explorer: Explorer,
    pub preview: Preview,
    pub theme: Theme,
    pub plugins: PluginManager,
    pub mode: Mode,
    pub command_buffer: String,
    pub status_message: String,
    pub show_hidden: bool,
    pub should_quit: bool,
    /// Set until the user confirms a destructive delete with a second 'd'.
    pub pending_delete: bool,
    /// Typed digit prefix, e.g. "12" before 'j' moves 12 rows.
    pub count_buffer: String,
    /// Selection index where visual mode started.
    pub visual_anchor: usize,
    /// Last known terminal height (for page/half-page scrolling).
    pub term_height: usize,
    /// Force a full redraw on the next frame (after spawning an editor).
    pub force_redraw: bool,
    /// First visible entry index (scroll position of the file list).
    pub scroll_offset: usize,
    /// Inner area of the file list viewport, recorded during draw so mouse
    /// clicks can be mapped to entries.
    pub list_area: Option<ratatui::layout::Rect>,
    /// Last mouse click (time, row, column) for double-click detection.
    pub last_click: Option<(Instant, u16, u16)>,
    /// Loaded configuration (mutated by theme/bookmark changes, saved on exit).
    pub config: Config,
    /// Bookmarks popup is open.
    pub bookmarks_open: bool,
    /// Selected row inside the bookmarks popup.
    pub bookmark_selected: usize,
    /// Waiting for a key to bookmark the current directory (`m` + key).
    pub pending_mark: bool,
    /// Waiting for a key to jump to a bookmark (`'` + key).
    pub pending_jump: bool,
    /// Something changed since the last draw — skip drawing otherwise.
    pub dirty: bool,
    /// When the status message was set (auto-expires back to "Ready").
    pub status_set_at: Option<Instant>,
    /// Last time a frame was drawn (periodic redraws for status expiry).
    pub last_draw: Instant,
    /// Item area of the bookmarks popup (for mouse mapping).
    pub bookmark_area: Option<ratatui::layout::Rect>,
    /// Fuzzy finder popup (None = closed).
    pub fuzzy: Option<FuzzyFinder>,
    /// Results area of the fuzzy popup (for mouse mapping).
    pub fuzzy_area: Option<ratatui::layout::Rect>,
    /// Full rect of the fuzzy popup (click-outside detection).
    pub fuzzy_rect: Option<ratatui::layout::Rect>,
    /// Help popup is open.
    pub help_open: bool,
    /// Scroll position of the help popup.
    pub help_scroll: u16,
    /// Inner area of the help popup (for wheel scrolling).
    pub help_area: Option<ratatui::layout::Rect>,
    /// Backwards directory history (newest last).
    pub dir_history: Vec<PathBuf>,
    /// Forward directory history (filled by going back).
    pub dir_future: Vec<PathBuf>,
    /// Scroll position of the preview pane.
    pub preview_scroll: u16,
    /// Inner area of the preview pane (for wheel mapping).
    pub preview_area: Option<ratatui::layout::Rect>,
    /// First visible row in the bookmarks popup (it scrolls now).
    pub bookmark_scroll: usize,
    /// Command history (newest last).
    pub command_history: Vec<String>,
    /// Position inside the command history while browsing it.
    pub command_history_pos: Option<usize>,
    /// Last git status snapshot (from the background thread).
    pub git_status: Option<GitStatus>,
    /// Pending git status refresh (polled each loop iteration).
    pub git_rx: Option<Receiver<GitStatus>>,
    /// Recursive directory sizes (disk-usage mode).
    pub dir_sizes: Option<DirSizes>,
    /// Pending dir-size computation (polled each loop iteration).
    pub du_rx: Option<Receiver<DirSizes>>,
    /// Disk-usage mode: show recursive directory sizes.
    pub du_enabled: bool,
    /// Sidebar pane is visible.
    pub sidebar_visible: bool,
    /// Keyboard focus is on the sidebar.
    pub sidebar_focus: bool,
    /// Sidebar state (selection + scroll).
    pub sidebar: Sidebar,
    /// Sidebar item area (for mouse mapping).
    pub sidebar_area: Option<ratatui::layout::Rect>,
    /// Mounted disks (from the background `df` query).
    pub disks: Vec<Disk>,
    /// Pending disk query (polled each loop iteration).
    pub disks_rx: Option<std::sync::mpsc::Receiver<Vec<Disk>>>,
}

impl App {
    pub fn new(cfg: &Config) -> Self {
        let start = Config::expand_path(&cfg.general.start_dir);
        let mut explorer = Explorer::new(&start);
        if let Some(sort) = crate::file::explorer::SortMode::from_label(&cfg.general.sort) {
            explorer.set_sort_mode(sort);
        }
        explorer.set_show_hidden(cfg.general.show_hidden);
        let mut theme = Theme::default();
        theme.set_by_name(&cfg.theme.name);
        let preview = Preview::new();
        let plugins = PluginManager::new("./plugins");
        Self {
            explorer,
            preview,
            theme,
            plugins,
            mode: Mode::Normal,
            command_buffer: String::new(),
            status_message: String::new(),
            show_hidden: cfg.general.show_hidden,
            should_quit: false,
            pending_delete: false,
            count_buffer: String::new(),
            visual_anchor: 0,
            term_height: 24,
            force_redraw: false,
            scroll_offset: 0,
            list_area: None,
            last_click: None,
            config: cfg.clone(),
            bookmarks_open: false,
            bookmark_selected: 0,
            pending_mark: false,
            pending_jump: false,
            dirty: true,
            status_set_at: None,
            last_draw: Instant::now(),
            bookmark_area: None,
            fuzzy: None,
            fuzzy_area: None,
            fuzzy_rect: None,
            help_open: false,
            help_scroll: 0,
            help_area: None,
            dir_history: vec![],
            dir_future: vec![],
            preview_scroll: 0,
            preview_area: None,
            bookmark_scroll: 0,
            command_history: vec![],
            command_history_pos: None,
            git_status: None,
            git_rx: None,
            dir_sizes: None,
            du_rx: None,
            du_enabled: false,
            sidebar_visible: cfg.general.sidebar,
            sidebar_focus: false,
            sidebar: Sidebar::new(),
            sidebar_area: None,
            disks: Vec::new(),
            disks_rx: None,
        }
    }

    /// Persist theme, bookmark, sort and hidden-file changes to the config.
    pub fn save_config(&self) {
        let mut cfg = self.config.clone();
        cfg.theme.name = self.theme.name().to_string();
        cfg.general.sort = self.explorer.sort_mode.label().to_string();
        cfg.general.show_hidden = self.show_hidden;
        let _ = cfg.save();
    }

    pub async fn run<B: ratatui::backend::Backend>(
        &mut self,
        mut term: Terminal<B>,
    ) -> std::io::Result<()> {
        // Initial preview load + background git status + disk query
        self.update_preview();
        self.start_git_refresh();
        self.disks_rx = Some(sidebar::refresh_disks());

        loop {
            // Apply a finished background preview load (if still current).
            if self.preview.poll(
                self.explorer
                    .entries
                    .get(self.explorer.selected)
                    .map(|e| e.path.as_path()),
            ) {
                self.dirty = true;
            }

            // Pick up background git / disk-usage results without blocking.
            if let Some(status) = crate::file::git::poll(&mut self.git_rx) {
                self.git_status = Some(status);
                self.dirty = true;
            }
            if let Some(sizes) = crate::file::du::poll(&mut self.du_rx) {
                self.dir_sizes = Some(sizes);
                self.dirty = true;
            }
            if let Some(disks) = sidebar::poll(&mut self.disks_rx) {
                self.disks = disks;
                self.dirty = true;
            }

            if self.force_redraw {
                term.clear()?;
                self.force_redraw = false;
                self.dirty = true;
            }

            // Status messages fade back to "Ready" after a few seconds.
            let now = Instant::now();
            if self
                .status_set_at
                .is_some_and(|t| now.duration_since(t) > Duration::from_secs(4))
            {
                self.status_message.clear();
                self.status_set_at = None;
                self.dirty = true;
            }

            // Redraw only when something changed (plus a slow tick so expired
            // statuses appear); ratatui diffs make unchanged frames cheap, but
            // skipping them entirely keeps large directories silky.
            if self.dirty || now.duration_since(self.last_draw) > Duration::from_millis(250) {
                term.draw(|f| crate::ui::layout::draw(f, self))?;
                self.last_draw = now;
                self.dirty = false;
            }
            self.term_height = term.size()?.height as usize;

            // ---- input handling -------------------------------------------------
            if event::poll(std::time::Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key).await,
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Resize(_, _) => self.dirty = true,
                    _ => {}
                }
            }

            // Quit by breaking the loop so main() can restore the terminal.
            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn update_preview(&mut self) {
        // The preview follows the cursor; reset its scroll position.
        self.preview_scroll = 0;
        self.clamp_scroll();
        if let Some(entry) = self.explorer.entries.get(self.explorer.selected) {
            if entry.is_dir {
                self.preview.load_dir(&entry.path);
            } else {
                self.preview.load(entry);
            }
        } else {
            self.preview.clear();
        }
    }

    fn set_status(&mut self, msg: &str) {
        self.status_message = msg.to_string();
        self.status_set_at = Some(Instant::now());
    }

    /// Keep the scroll offset so the selected entry stays in view.
    pub fn clamp_scroll(&mut self) {
        let Some(area) = self.list_area else { return };
        let visible = area.height as usize;
        if visible == 0 {
            return;
        }
        if self.explorer.selected < self.scroll_offset {
            self.scroll_offset = self.explorer.selected;
        } else if self.explorer.selected >= self.scroll_offset + visible {
            self.scroll_offset = self.explorer.selected + 1 - visible;
        }
    }

    /// Keep the bookmarks popup's scroll offset in sync with the selection.
    pub fn clamp_bookmark_scroll(&mut self) {
        let Some(area) = self.bookmark_area else { return };
        let visible = area.height as usize;
        if visible == 0 {
            return;
        }
        if self.bookmark_selected < self.bookmark_scroll {
            self.bookmark_scroll = self.bookmark_selected;
        } else if self.bookmark_selected >= self.bookmark_scroll + visible {
            self.bookmark_scroll = self.bookmark_selected + 1 - visible;
        }
    }

    // ── directory history ────────────────────────────────────────────────────────

    /// Record a jump in the back-history, clearing the forward stack.
    fn push_history(&mut self, from: PathBuf) {
        self.dir_history.push(from);
        self.dir_future.clear();
    }

    /// Navigate to a directory via `change_dir`, recording history and
    /// refreshing git / disk-usage info.
    fn navigate_to(&mut self, path: &str) -> std::io::Result<()> {
        let old = self.explorer.cwd.clone();
        self.explorer.change_dir(path)?;
        if self.explorer.cwd != old {
            self.push_history(old);
            self.refresh_info();
        }
        Ok(())
    }

    /// Go back one directory in history (Ctrl+o).
    fn go_back(&mut self) {
        let Some(prev) = self.dir_history.pop() else {
            self.set_status("No earlier directory");
            return;
        };
        let cur = self.explorer.cwd.clone();
        match self.explorer.change_dir(&prev.to_string_lossy()) {
            Ok(()) => {
                self.dir_future.push(cur);
                self.refresh_info();
                self.update_preview();
                self.set_status(&format!("← {}", Config::shorten_path(&prev)));
            }
            Err(e) => {
                self.dir_history.push(prev);
                self.set_status(&format!("Jump failed: {}", e));
            }
        }
    }

    /// Go forward one directory in history (Tab).
    fn go_forward(&mut self) {
        let Some(next) = self.dir_future.pop() else {
            self.set_status("No forward directory");
            return;
        };
        let cur = self.explorer.cwd.clone();
        match self.explorer.change_dir(&next.to_string_lossy()) {
            Ok(()) => {
                self.dir_history.push(cur);
                self.refresh_info();
                self.update_preview();
                self.set_status(&format!("→ {}", Config::shorten_path(&next)));
            }
            Err(e) => {
                self.dir_future.push(next);
                self.set_status(&format!("Jump failed: {}", e));
            }
        }
    }

    /// Navigate to the parent directory, recording history.
    fn go_up_dir(&mut self) {
        let old = self.explorer.cwd.clone();
        self.explorer.go_up();
        if self.explorer.cwd != old {
            self.push_history(old);
            self.refresh_info();
        }
        self.update_preview();
    }

    /// Enter the selected directory, recording history.
    fn enter_dir(&mut self) {
        let old = self.explorer.cwd.clone();
        self.explorer.enter_selected();
        if self.explorer.cwd != old {
            self.push_history(old);
            self.refresh_info();
        }
        self.update_preview();
    }

    /// Called after the cwd changes or files mutate: refresh git status and
    /// dir sizes in the background.
    fn refresh_info(&mut self) {
        self.start_git_refresh();
        if self.du_enabled {
            self.start_du_refresh();
        }
    }

    /// Kick off a background git status refresh (no-op without git).
    fn start_git_refresh(&mut self) {
        self.git_rx = GitStatus::refresh(&self.explorer.cwd);
    }

    /// Kick off a background dir-size computation for the current tree.
    fn start_du_refresh(&mut self) {
        self.du_rx = Some(DirSizes::refresh(&self.explorer.cwd));
        self.dir_sizes = None;
        self.dirty = true;
    }

    // ── fuzzy finder ─────────────────────────────────────────────────────────────

    /// Open the fuzzy finder over all files under the current directory.
    fn open_fuzzy(&mut self) {
        self.fuzzy = Some(FuzzyFinder::open(&self.explorer.cwd));
        self.help_open = false;
        self.bookmarks_open = false;
    }

    /// Jump to the fuzzy finder's selected file.
    fn fuzzy_jump(&mut self) {
        let Some(path) = self.fuzzy.as_ref().and_then(|f| f.selected_path().cloned()) else {
            return;
        };
        let parent = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.explorer.cwd.clone());
        let old = self.explorer.cwd.clone();
        if parent != old {
            if let Err(e) = self.explorer.change_dir(&parent.to_string_lossy()) {
                self.set_status(&format!("Jump failed: {}", e));
                return;
            }
            self.push_history(old);
        }
        // Select the file by name in the new listing.
        let name = path.file_name().map(|n| n.to_os_string());
        if let Some(idx) = self
            .explorer
            .entries
            .iter()
            .position(|e| e.path.file_name() == name.as_deref())
        {
            self.explorer.selected = idx;
        }
        self.fuzzy = None;
        self.refresh_info();
        self.update_preview();
        self.set_status(&format!("→ {}", Config::shorten_path(&path)));
    }

    /// Key handling while the fuzzy finder is open.
    fn handle_fuzzy_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            Char(c) if !ctrl => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.push_char(c);
                }
            }
            Backspace => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.pop_char();
                }
            }
            Down => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_down();
                }
            }
            Up => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_up();
                }
            }
            Char('j') if ctrl => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_down();
                }
            }
            Char('k') if ctrl => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_up();
                }
            }
            Char('n') if ctrl => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_down();
                }
            }
            Char('p') if ctrl => {
                if let Some(fz) = &mut self.fuzzy {
                    fz.move_up();
                }
            }
            Enter => self.fuzzy_jump(),
            Esc => self.fuzzy = None,
            _ => {}
        }
    }

    // ── help popup ───────────────────────────────────────────────────────────────

    /// Toggle the help popup (opening one popup closes the others).
    fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        self.help_scroll = 0;
        if self.help_open {
            self.fuzzy = None;
            self.bookmarks_open = false;
        }
    }

    /// Key handling while the help popup is open.
    fn handle_help_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            Char('j') | Down => self.help_scroll = self.help_scroll.saturating_add(1),
            Char('k') | Up => self.help_scroll = self.help_scroll.saturating_sub(1),
            Char('d') if ctrl => self.help_scroll = self.help_scroll.saturating_add(10),
            Char('u') if ctrl => self.help_scroll = self.help_scroll.saturating_sub(10),
            Char('q') | Char('?') | Esc => {
                self.help_open = false;
                self.help_scroll = 0;
            }
            _ => {}
        }
    }

    // ── sidebar ─────────────────────────────────────────────────────────────────

    /// Key handling while the sidebar has keyboard focus.
    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        let items = self
            .sidebar
            .items(&self.disks, &self.config.bookmarks);
        match key.code {
            Char('j') | Down => {
                self.sidebar.move_down(&items);
                self.sidebar.clamp_scroll(16);
            }
            Char('k') | Up => {
                self.sidebar.move_up(&items);
                self.sidebar.clamp_scroll(16);
            }
            Char('g') => self.sidebar.go_to_top(&items),
            Enter | Char('l') => self.sidebar_jump(),
            Char('d') | Char('x') => {
                // Remove a bookmark entry from the config.
                if let Some(SidebarItem::Bookmark { key: k, .. }) = items.get(self.sidebar.selected)
                {
                    let k = k.to_string();
                    self.config.bookmarks.remove(&k);
                    self.save_config();
                    let n = self.config.bookmarks.len();
                    if n == 0 {
                        self.sidebar_focus = false;
                        self.sidebar_visible = false;
                    }
                    self.set_status(&format!("Removed bookmark '{}'", k));
                }
            }
            Char('q') => self.should_quit = true,
            Esc | Tab => {
                self.sidebar_focus = false;
            }
            _ => {}
        }
    }

    /// Jump to the currently selected sidebar item.
    fn sidebar_jump(&mut self) {
        let items = self
            .sidebar
            .items(&self.disks, &self.config.bookmarks);
        let Some(item) = items.get(self.sidebar.selected) else {
            return;
        };
        let Some(path) = item.path().cloned() else {
            return;
        };
        if path == self.explorer.cwd {
            self.sidebar_focus = false;
            return;
        }
        match self.navigate_to(&path.to_string_lossy()) {
            Ok(()) => {
                self.sidebar_focus = false;
                self.update_preview();
                self.set_status(&format!("→ {}", Config::shorten_path(&path)));
            }
            Err(e) => self.set_status(&format!("Jump failed: {}", e)),
        }
    }

    // ── mouse handling ---------------------------------------------------------

    /// Mouse: wheel scrolls, click selects, double-click opens. Popups take
    /// precedence; the wheel scrolls the pane the pointer is over.
    fn handle_mouse(&mut self, m: MouseEvent) {
        use crossterm::event::MouseEventKind::*;
        self.dirty = true;

        // Fuzzy finder popup: click a row to select (double-click jumps),
        // click outside to close, wheel moves the selection.
        if self.fuzzy.is_some() {
            match m.kind {
                Down(crossterm::event::MouseButton::Left) => {
                    match self.fuzzy_index_at(m.row) {
                        Some(idx) => {
                            let now = Instant::now();
                            let double_click = self
                                .last_click
                                .is_some_and(|(t, row, col)| {
                                    t.elapsed() < Duration::from_millis(400)
                                        && row == m.row
                                        && col == m.column
                                });
                            self.last_click = Some((now, m.row, m.column));
                            if let Some(fz) = &mut self.fuzzy {
                                fz.selected = idx;
                            }
                            if double_click {
                                self.fuzzy_jump();
                                self.last_click = None;
                            }
                        }
                        // Clicked the query line or outside the popup.
                        None => {
                            let outside = self.fuzzy_rect.is_none_or(|r| {
                                m.row < r.y
                                    || m.row >= r.y + r.height
                                    || m.column < r.x
                                    || m.column >= r.x + r.width
                            });
                            if outside {
                                self.fuzzy = None;
                            }
                        }
                    }
                }
                ScrollUp => {
                    if let Some(fz) = &mut self.fuzzy {
                        fz.move_up();
                    }
                }
                ScrollDown => {
                    if let Some(fz) = &mut self.fuzzy {
                        fz.move_down();
                    }
                }
                _ => {}
            }
            return;
        }

        // Help popup: click closes, wheel scrolls.
        if self.help_open {
            match m.kind {
                Down(crossterm::event::MouseButton::Left) => {
                    self.help_open = false;
                }
                ScrollUp => self.help_scroll = self.help_scroll.saturating_sub(1),
                ScrollDown => self.help_scroll = self.help_scroll.saturating_add(1),
                _ => {}
            }
            return;
        }

        // Bookmarks popup: click a row to jump, click outside to close,
        // wheel moves the selection.
        if self.bookmarks_open {
            match m.kind {
                Down(crossterm::event::MouseButton::Left) => {
                    if let Some(idx) = self.bookmark_index_at(m.row) {
                        self.bookmark_selected = idx;
                        self.jump_bookmark_at(idx);
                    } else {
                        self.bookmarks_open = false;
                    }
                }
                ScrollUp => {
                    if self.bookmark_selected > 0 {
                        self.bookmark_selected -= 1;
                        self.clamp_bookmark_scroll();
                    }
                }
                ScrollDown
                    if self.bookmark_selected + 1 < self.config.bookmarks.len() => {
                        self.bookmark_selected += 1;
                        self.clamp_bookmark_scroll();
                    }
                _ => {}
            }
            return;
        }

        // Sidebar: click a row to jump, wheel moves the selection.
        if self.sidebar_visible {
            if let Some(area) = self.sidebar_area {
                if m.column >= area.x
                    && m.column < area.x + area.width
                    && m.row >= area.y
                    && m.row < area.y + area.height
                {
                    match m.kind {
                        Down(crossterm::event::MouseButton::Left) => {
                            if let Some(idx) = self.sidebar_index_at(m.row) {
                                self.sidebar.selected = idx;
                                self.sidebar_jump();
                            }
                        }
                        ScrollUp => {
                            let items = self.sidebar.items(&self.disks, &self.config.bookmarks);
                            self.sidebar.move_up(&items);
                        }
                        ScrollDown => {
                            let items = self.sidebar.items(&self.disks, &self.config.bookmarks);
                            self.sidebar.move_down(&items);
                        }
                        _ => {}
                    }
                    return;
                }
            }
        }

        // Wheel over the preview pane scrolls the preview (not the list).
        if let Some(pa) = self.preview_area {
            if m.column >= pa.x
                && m.column < pa.x + pa.width
                && m.row >= pa.y
                && m.row < pa.y + pa.height
            {
                match m.kind {
                    ScrollUp => self.preview_scroll = self.preview_scroll.saturating_sub(3),
                    ScrollDown => self.preview_scroll = self.preview_scroll.saturating_add(3),
                    _ => {}
                }
                return;
            }
        }

        match m.kind {
            ScrollUp => {
                self.explorer.move_up();
                self.update_preview();
            }
            ScrollDown => {
                self.explorer.move_down();
                self.update_preview();
            }
            Down(crossterm::event::MouseButton::Left) => {
                let Some(idx) = self.list_index_at(m.row, m.column) else {
                    return;
                };
                let now = Instant::now();
                let double_click = self
                    .last_click
                    .is_some_and(|(t, row, col)| {
                        t.elapsed() < Duration::from_millis(400)
                            && row == m.row
                            && col == m.column
                    });
                self.last_click = Some((now, m.row, m.column));

                self.explorer.selected = idx;
                self.update_preview();

                if double_click {
                    // Open like a GUI file manager: dirs enter, files open.
                    let is_dir = self
                        .explorer
                        .entries
                        .get(idx)
                        .is_some_and(|e| e.is_dir);
                    if is_dir {
                        self.enter_dir();
                    } else {
                        self.open_selected();
                    }
                    self.last_click = None;
                }
            }
            _ => {}
        }
    }

    /// Map a mouse row to a bookmarks-popup entry index (scroll-aware).
    fn bookmark_index_at(&self, row: u16) -> Option<usize> {
        let area = self.bookmark_area?;
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let idx = self.bookmark_scroll + (row - area.y) as usize;
        if idx < self.config.bookmarks.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Map a mouse row to a fuzzy-popup result index.
    fn fuzzy_index_at(&self, row: u16) -> Option<usize> {
        let area = self.fuzzy_area?;
        let fz = self.fuzzy.as_ref()?;
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let idx = fz.scroll + (row - area.y) as usize;
        if idx < fz.matches.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Map a mouse row to a sidebar item index (scroll-aware).
    fn sidebar_index_at(&self, row: u16) -> Option<usize> {
        let area = self.sidebar_area?;
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let idx = self.sidebar.scroll + (row - area.y) as usize;
        let items = self.sidebar.items(&self.disks, &self.config.bookmarks);
        if idx < items.len() && items[idx].selectable() {
            Some(idx)
        } else {
            None
        }
    }

    /// Map a mouse position to a file-list entry index.
    fn list_index_at(&self, row: u16, col: u16) -> Option<usize> {
        let area = self.list_area?;
        if col < area.x || col >= area.x + area.width {
            return None;
        }
        if row < area.y || row >= area.y + area.height {
            return None;
        }
        let idx = self.scroll_offset + (row - area.y) as usize;
        if idx < self.explorer.entries.len() {
            Some(idx)
        } else {
            None
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        self.dirty = true;
        // The popups take precedence over every mode.
        if self.fuzzy.is_some() {
            self.handle_fuzzy_key(key);
            return;
        }
        if self.help_open {
            self.handle_help_key(key);
            return;
        }
        if self.bookmarks_open {
            self.handle_bookmarks_key(key);
            return;
        }
        if self.sidebar_focus {
            self.handle_sidebar_key(key);
            return;
        }
        match self.mode {
            Mode::Command => self.handle_command_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Visual => self.handle_visual_key(key),
            Mode::Normal => self.handle_normal_key(key).await,
        }
    }

    // ── mode handlers ----------------------------------------------------------

    async fn handle_normal_key(&mut self, key: KeyEvent) {
        use KeyCode::*;

        // Complete or cancel a pending mark (`m`) / jump (`'`) action.
        if self.pending_mark {
            self.pending_mark = false;
            if let Char(c) = &key.code {
                self.set_bookmark(*c);
            }
            return;
        }
        if self.pending_jump {
            self.pending_jump = false;
            if let Char(c) = &key.code {
                self.jump_bookmark(*c);
            }
            return;
        }

        // Cancel a pending delete/trash on any key other than 'd'/'D'.
        if !matches!(key.code, Char('d') | Char('D')) {
            self.pending_delete = false;
        }

        // Collect a digit prefix as a count ("5j" moves 5 rows). A lone '0'
        // doesn't start a count.
        if let Char(c) = &key.code {
            if c.is_ascii_digit() && !(c == &'0' && self.count_buffer.is_empty()) {
                self.count_buffer.push(*c);
                return;
            }
        }
        let count: usize = self.count_buffer.parse().unwrap_or(1);
        self.count_buffer.clear();

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        // Chrome rows (borders + header + footer) don't scroll.
        let page = self.term_height.saturating_sub(8).max(1);
        let half = (page / 2).max(1);

        match key.code {
            Char('q') => self.should_quit = true,
            // Ctrl+j/k scroll the preview pane (must precede the j/k arms).
            Char('j') if ctrl => {
                self.preview_scroll = self.preview_scroll.saturating_add(half as u16);
            }
            Char('k') if ctrl => {
                self.preview_scroll = self.preview_scroll.saturating_sub(half as u16);
            }
            // Sidebar focus toggle (Tab switches panes, superfile-style).
            Tab => {
                if self.sidebar_visible {
                    self.sidebar_focus = !self.sidebar_focus;
                } else {
                    self.sidebar_visible = true;
                    self.sidebar_focus = true;
                }
                self.set_status(if self.sidebar_focus {
                    "Sidebar focused — j/k move, Enter jump, Esc back"
                } else {
                    "Sidebar unfocused"
                });
            }
            // Directory history: Ctrl+o / Alt+← back, Alt+→ forward.
            Char('o') if ctrl => self.go_back(),
            Left if alt => self.go_back(),
            Right if alt => self.go_forward(),
            Char('j') | Down => {
                for _ in 0..count {
                    self.explorer.move_down();
                }
                self.update_preview();
            }
            Char('k') | Up => {
                for _ in 0..count {
                    self.explorer.move_up();
                }
                self.update_preview();
            }
            Char('h') | Left => {
                self.go_up_dir();
            }
            Char('l') | Right => {
                self.enter_dir();
            }
            Char('g') => {
                self.explorer.go_to_top();
                self.update_preview();
            }
            Char('G') => {
                self.explorer.go_to_bottom();
                self.update_preview();
            }
            Home => {
                self.explorer.go_to_top();
                self.update_preview();
            }
            End => {
                self.explorer.go_to_bottom();
                self.update_preview();
            }
            PageDown => {
                for _ in 0..page {
                    self.explorer.move_down();
                }
                self.update_preview();
            }
            PageUp => {
                for _ in 0..page {
                    self.explorer.move_up();
                }
                self.update_preview();
            }
            Char('d') if ctrl => {
                for _ in 0..half {
                    self.explorer.move_down();
                }
                self.update_preview();
            }
            Char('u') if ctrl => {
                for _ in 0..half {
                    self.explorer.move_up();
                }
                self.update_preview();
            }
            Char('f') if ctrl => {
                for _ in 0..page {
                    self.explorer.move_down();
                }
                self.update_preview();
            }
            Char('b') if ctrl => {
                for _ in 0..page {
                    self.explorer.move_up();
                }
                self.update_preview();
            }
            Char('X') => {
                #[cfg(feature = "archive")]
                match self.explorer.extract_selected(None) {
                    Ok(msg) => {
                        self.refresh_info();
                        self.update_preview();
                        self.set_status(&msg);
                    }
                    Err(e) => self.set_status(&e),
                }
                #[cfg(not(feature = "archive"))]
                self.set_status("Extract needs the 'archive' feature");
            }
            Char('!') => {
                self.mode = Mode::Command;
                self.command_buffer = "! ".to_string();
            }
            Char('f') => self.open_fuzzy(),
            Char('?') => self.toggle_help(),
            Char('R') => {
                self.explorer.refresh();
                self.refresh_info();
                self.set_status("Refreshed");
            }
            Char('S') => {
                if self.du_enabled {
                    self.du_enabled = false;
                    self.dir_sizes = None;
                    self.du_rx = None;
                    self.set_status("Disk usage: off");
                } else {
                    self.du_enabled = true;
                    self.start_du_refresh();
                    self.set_status("Disk usage: computing…");
                }
            }
            Char('y') | Char('c') => {
                let n = self.explorer.selected_paths().len();
                self.explorer.copy_selected();
                self.set_status(&format!("Yanked {} item(s)", n));
            }
            Char('x') => {
                let n = self.explorer.selected_paths().len();
                self.explorer.cut_selected();
                self.set_status(&format!("Cut {} item(s)", n));
            }
            Char('p') => {
                let n = self.explorer.paste();
                if n > 0 {
                    self.set_status(&format!("Pasted {} item(s)", n));
                } else {
                    self.set_status("Clipboard is empty");
                }
                self.refresh_info();
                self.update_preview();
            }
            Char('d') => {
                if self.pending_delete {
                    let paths = self.explorer.selected_paths();
                    if Explorer::trash_dir().is_none() {
                        self.set_status("Trash not supported on this platform — use D or :rm");
                        self.pending_delete = false;
                    } else {
                        let (moved, failed) = self.explorer.trash_paths(&paths);
                        self.set_status(&format!(
                            "Moved {} item(s) to trash{}  (:restore to undo)",
                            moved,
                            if failed > 0 { format!(", {} failed", failed) } else { String::new() }
                        ));
                        self.pending_delete = false;
                        self.refresh_info();
                        self.update_preview();
                    }
                } else {
                    self.pending_delete = true;
                    self.set_status("Press 'd' again to move to trash (D = delete forever)");
                }
            }
            Char('D') => {
                if self.pending_delete {
                    let paths = self.explorer.selected_paths();
                    let (ok, failed) = self.explorer.delete_paths(&paths);
                    self.set_status(&format!("Deleted {} item(s){}", ok,
                        if failed > 0 { format!(", {} failed", failed) } else { String::new() }));
                    self.pending_delete = false;
                    self.update_preview();
                } else {
                    self.pending_delete = true;
                    self.set_status("Press 'D' again to PERMANENTLY delete (d = trash)");
                }
            }
            Char('r') => {
                if let Some(entry) = self.explorer.entries.get(self.explorer.selected) {
                    // Prefill the current name — rename is usually a small edit.
                    let name = entry
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.mode = Mode::Command;
                    self.command_buffer = format!("rename {}", name);
                }
            }
            Char('n') => {
                // nvim-style: n jumps to the next match while a search is
                // active; otherwise it creates a new file.
                if self.explorer.search_mode {
                    self.explorer.next_match();
                    self.update_preview();
                } else {
                    self.mode = Mode::Command;
                    self.command_buffer = "touch ".to_string();
                }
            }
            Char('N') => {
                if self.explorer.search_mode {
                    self.explorer.prev_match();
                    self.update_preview();
                } else {
                    self.mode = Mode::Command;
                    self.command_buffer = "mkdir ".to_string();
                }
            }
            Char('v') => {
                self.mode = Mode::Visual;
                self.visual_anchor = self.explorer.selected;
            }
            Char('m') => {
                self.pending_mark = true;
                self.set_status("Press any key to bookmark the current directory");
            }
            Char('\'') => {
                self.pending_jump = true;
                self.set_status("Jump to bookmark: press its key");
            }
            Char('w') => {
                self.bookmarks_open = true;
                self.bookmark_selected = 0;
            }
            Char('/') => {
                self.mode = Mode::Search;
                self.command_buffer.clear();
                self.explorer.start_search();
            }
            Char(':') => {
                self.mode = Mode::Command;
                self.command_buffer.clear();
            }
            Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.explorer.set_show_hidden(self.show_hidden);
                self.update_preview();
                self.set_status(&format!("Hidden files: {}", if self.show_hidden { "shown" } else { "hidden" }));
            }
            Char('t') => {
                self.theme.cycle();
                self.config.theme.name = self.theme.name().to_string();
                self.set_status(&format!("Theme: {}", self.theme.name()));
            }
            Char('s') => {
                self.explorer.set_sort_mode(self.explorer.sort_mode.next());
                self.set_status(&format!("Sorted by {}", self.explorer.sort_mode.label()));
            }
            Char('e') => self.edit_selected().await,
            Char('o') => self.open_selected(),
            Char('`') => self.plugins.toggle_ui(),
            Enter => {
                self.enter_dir();
            }
            Backspace => {
                self.go_up_dir();
            }
            Esc
                // Esc clears an active search; previously there was no way to
                // leave search mode once it was entered.
                if self.explorer.search_mode => {
                    self.explorer.clear_search();
                    self.update_preview();
                    self.set_status("Search cleared");
                }
            _ => {}
        }

        // Forward the key to plugins (other modes return before this).
        self.plugins.on_key(key);
    }

    fn handle_visual_key(&mut self, key: KeyEvent) {
        use KeyCode::*;

        if !matches!(key.code, Char('d')) {
            self.pending_delete = false;
        }

        match key.code {
            Char('j') | Down => {
                self.explorer.move_down();
            }
            Char('k') | Up => {
                self.explorer.move_up();
            }
            Char('g') => {
                self.explorer.go_to_top();
                self.update_preview();
            }
            Char('G') => {
                self.explorer.go_to_bottom();
                self.update_preview();
            }
            Char('y') => {
                let n = self.visual_paths().len();
                self.explorer.copy_paths(self.visual_paths(), false);
                self.mode = Mode::Normal;
                self.set_status(&format!("Yanked {} item(s)", n));
            }
            Char('x') => {
                let n = self.visual_paths().len();
                self.explorer.copy_paths(self.visual_paths(), true);
                self.mode = Mode::Normal;
                self.set_status(&format!("Cut {} item(s)", n));
            }
            Char('d') => {
                if self.pending_delete {
                    let paths = self.visual_paths();
                    if Explorer::trash_dir().is_none() {
                        self.set_status("Trash not supported on this platform");
                        self.pending_delete = false;
                        self.mode = Mode::Normal;
                    } else {
                        let (moved, failed) = self.explorer.trash_paths(&paths);
                        self.set_status(&format!(
                            "Moved {} item(s) to trash{}  (:restore to undo)",
                            moved,
                            if failed > 0 { format!(", {} failed", failed) } else { String::new() }
                        ));
                        self.pending_delete = false;
                        self.mode = Mode::Normal;
                        self.refresh_info();
                        self.update_preview();
                    }
                } else {
                    self.pending_delete = true;
                    self.set_status("Press 'd' again to move to trash");
                }
            }
            Char('v') | Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        match key.code {
            Char(c) => {
                self.command_buffer.push(c);
                self.explorer.search_live(&self.command_buffer);
            }
            Backspace => {
                self.command_buffer.pop();
                self.explorer.search_live(&self.command_buffer);
            }
            Enter => {
                self.mode = Mode::Normal;
                let pattern = self.command_buffer.clone();
                if pattern.is_empty() {
                    self.explorer.clear_search();
                } else {
                    self.set_status(&format!("Search: {}  (n/N next/prev, Esc clear)", pattern));
                }
            }
            Esc => {
                self.explorer.clear_search();
                self.mode = Mode::Normal;
                self.command_buffer.clear();
                self.set_status("Search cancelled");
                self.update_preview();
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        match key.code {
            Char(c) => {
                self.command_buffer.push(c);
                self.command_history_pos = None;
            }
            Backspace => {
                self.command_buffer.pop();
                self.command_history_pos = None;
            }
            Up => self.command_history_prev(),
            Down => self.command_history_next(),
            Tab => self.complete_command(),
            Enter => {
                let cmd = self.command_buffer.clone();
                self.mode = Mode::Normal;
                self.command_buffer.clear();
                self.push_command_history(&cmd);
                self.execute_command(&cmd);
            }
            Esc => {
                self.mode = Mode::Normal;
                self.command_buffer.clear();
                self.command_history_pos = None;
            }
            _ => {}
        }
    }

    /// Add a command to the history (deduped against the last entry).
    fn push_command_history(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        if self.command_history.last().is_none_or(|last| last != cmd) {
            self.command_history.push(cmd.to_string());
        }
        self.command_history_pos = None;
    }

    /// Browse to the previous (older) command in the history.
    fn command_history_prev(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let pos = match self.command_history_pos {
            None => self.command_history.len() - 1,
            Some(0) => return,
            Some(p) => p - 1,
        };
        self.command_history_pos = Some(pos);
        self.command_buffer = self.command_history[pos].clone();
    }

    /// Browse to the next (newer) command in the history.
    fn command_history_next(&mut self) {
        let Some(pos) = self.command_history_pos else { return };
        if pos + 1 >= self.command_history.len() {
            self.command_history_pos = None;
            self.command_buffer.clear();
        } else {
            self.command_history_pos = Some(pos + 1);
            self.command_buffer = self.command_history[pos + 1].clone();
        }
    }

    /// Tab completion in command mode: complete the command word, or file
    /// names for its argument. `rename` prefills the selected entry's name.
    fn complete_command(&mut self) {
        let buf = self.command_buffer.clone();
        let ends_with_space = buf.ends_with(' ');
        let parts: Vec<&str> = buf.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        if parts.len() == 1 && !ends_with_space {
            // Complete the command word.
            let word = parts[0];
            let cands: Vec<&str> = COMMANDS
                .iter()
                .filter(|c| c.starts_with(word) && **c != word)
                .cloned()
                .collect();
            if cands.is_empty() {
                return;
            }
            if cands.len() == 1 {
                self.command_buffer = format!("{} ", cands[0]);
            } else {
                let lcp = longest_common_prefix(&cands);
                if lcp.len() > word.len() {
                    self.command_buffer = lcp;
                }
                self.set_status(&format!("Commands: {}", cands.join(" ")));
            }
            return;
        }

        // Complete the argument, splitting on the last '/' so nested paths
        // complete too (e.g. `cd src/ui<Tab>`).
        let cmd = parts[0].to_string();
        let arg = if ends_with_space {
            ""
        } else {
            parts[parts.len() - 1]
        };
        let (dir_part, word) = match arg.rfind('/') {
            Some(i) => (Some(&arg[..=i]), &arg[i + 1..]),
            None => (None, arg),
        };
        let base = match dir_part {
            Some(d) => {
                let p = self.explorer.cwd.join(d);
                if p.is_dir() {
                    p
                } else {
                    return;
                }
            }
            None => self.explorer.cwd.clone(),
        };

        if cmd == "rename" && word.is_empty() {
            // Prefill the selected entry's name — the common rename edit.
            if let Some(entry) = self.explorer.entries.get(self.explorer.selected) {
                if let Some(name) = entry.path.file_name() {
                    self.command_buffer = format!("rename {}", name.to_string_lossy());
                }
            }
            return;
        }

        let mut names: Vec<String> = std::fs::read_dir(&base)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().to_str().map(|n| n.to_string()))
                    .filter(|n| n.starts_with(word) && n != word)
                    .collect()
            })
            .unwrap_or_default();
        if names.is_empty() {
            return;
        }
        names.sort();
        let head = if dir_part.is_some() || parts.len() > 2 {
            format!("{} {}", cmd, &arg[..arg.len() - word.len()])
        } else {
            format!("{} ", cmd)
        };
        if names.len() == 1 {
            let mut full = names.into_iter().next().unwrap();
            // Directory completions keep a trailing slash so the next Tab
            // completes inside them.
            if base.join(&full).is_dir() {
                full.push('/');
            }
            self.command_buffer = format!("{}{}", head, full);
        } else {
            let lcp = longest_common_prefix(&names);
            if lcp.len() > word.len() {
                self.command_buffer = format!("{}{}", head, lcp);
            }
            let shown: Vec<String> = names.iter().take(8).cloned().collect();
            let more = if names.len() > 8 {
                format!(" … +{}", names.len() - 8)
            } else {
                String::new()
            };
            self.set_status(&format!("Matches: {}{}", shown.join(" "), more));
        }
    }

    // ── bookmarks ----------------------------------------------------------------

    /// Bookmark the current directory under `key` and persist the config.
    fn set_bookmark(&mut self, key: char) {
        // Store with `~` for HOME so the config file stays portable.
        let path = Config::shorten_path(&self.explorer.cwd);
        self.config.bookmarks.insert(key.to_string(), path.clone());
        self.save_config();
        self.set_status(&format!("Bookmarked {} as '{}'", path, key));
    }

    /// Jump to the bookmark registered under `key`.
    fn jump_bookmark(&mut self, key: char) {
        let Some(path) = self.config.bookmarks.get(&key.to_string()).cloned() else {
            self.set_status(&format!("No bookmark '{}' (press m to create one)", key));
            return;
        };
        let expanded = Config::expand_path(&path);
        match self
            .navigate_to(&expanded.to_string_lossy())
        {
            Ok(()) => {
                self.update_preview();
                self.set_status(&format!("Jumped to '{}'", path));
            }
            Err(e) => self.set_status(&format!("Jump to '{}' failed: {}", path, e)),
        }
    }

    /// Key handling while the bookmarks popup is open.
    fn handle_bookmarks_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        let count = self.config.bookmarks.len();
        match key.code {
            Char('j') | Down => {
                if self.bookmark_selected + 1 < count {
                    self.bookmark_selected += 1;
                    self.clamp_bookmark_scroll();
                }
            }
            Char('k') | Up => {
                if self.bookmark_selected > 0 {
                    self.bookmark_selected -= 1;
                    self.clamp_bookmark_scroll();
                }
            }
            Enter | Char('l') => {
                self.jump_bookmark_at(self.bookmark_selected);
            }
            Char('d') | Char('x') => {
                let keys: Vec<String> = self.config.bookmarks.keys().cloned().collect();
                if let Some(k) = keys.get(self.bookmark_selected) {
                    let k = k.clone();
                    self.config.bookmarks.remove(&k);
                    self.save_config();
                    let new_count = self.config.bookmarks.len();
                    if new_count == 0 {
                        self.bookmarks_open = false;
                    } else if self.bookmark_selected >= new_count {
                        self.bookmark_selected = new_count - 1;
                    }
                    self.set_status(&format!("Removed bookmark '{}'", k));
                }
            }
            Char('q') | Char('w') | Esc => {
                self.bookmarks_open = false;
            }
            _ => {}
        }
    }

    /// Jump to the bookmark at popup row `idx` and close the popup.
    fn jump_bookmark_at(&mut self, idx: usize) {
        let keys: Vec<String> = self.config.bookmarks.keys().cloned().collect();
        if let Some(k) = keys.get(idx) {
            let k = k.clone();
            self.bookmarks_open = false;
            if let Some(c) = k.chars().next() {
                self.jump_bookmark(c);
            }
        }
    }

    /// Paths in the visual-mode selection range.
    fn visual_paths(&self) -> Vec<PathBuf> {
        let sel = self.explorer.selected;
        let (a, b) = if self.visual_anchor <= sel {
            (self.visual_anchor, sel)
        } else {
            (sel, self.visual_anchor)
        };
        self.explorer.entries[a..=b.min(self.explorer.entries.len().saturating_sub(1))]
            .iter()
            .map(|e| e.path.clone())
            .collect()
    }

    /// Open the selected file in $EDITOR, suspending the TUI.
    async fn edit_selected(&mut self) {
        let Some(entry) = self.explorer.entries.get(self.explorer.selected) else {
            return;
        };
        if entry.is_dir {
            self.explorer.enter_selected();
            self.update_preview();
            return;
        }
        let path = entry.path.clone();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Suspend the TUI
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );

        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());
        let status = match tokio::process::Command::new(&editor).arg(&path).status().await {
            Ok(st) if st.success() => format!("Edited {} with {}", name, editor),
            Ok(st) => format!("{} exited with {}", editor, st),
            Err(e) => format!("Failed to launch {}: {} (set $EDITOR)", editor, e),
        };

        // Resume the TUI
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::EnterAlternateScreen
        );
        self.force_redraw = true;
        self.set_status(&status);
        self.update_preview();
    }

    /// Run a shell command in the current directory, suspending the TUI so
    /// its output is fully visible. (Blocking is fine: the TUI is suspended
    /// and nothing else needs the thread.)
    fn run_shell(&mut self, cmd: &str) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        println!("$ {}", cmd);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let status = match std::process::Command::new(&shell)
            .arg("-c")
            .arg(cmd)
            .current_dir(&self.explorer.cwd)
            .status()
        {
            Ok(st) if st.success() => "done".to_string(),
            Ok(st) => format!("exited with {}", st),
            Err(e) => format!("failed: {}", e),
        };

        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::EnterAlternateScreen
        );
        self.force_redraw = true;
        self.refresh_info();
        self.update_preview();
        self.set_status(&format!("{} — press Enter", status));
    }

    /// Open the selected file with the system opener.
    fn open_selected(&mut self) {
        let Some(entry) = self.explorer.entries.get(self.explorer.selected) else {
            return;
        };
        if entry.is_dir {
            self.explorer.enter_selected();
            self.update_preview();
            return;
        }
        let name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&entry.path).spawn()
        } else if cfg!(target_os = "linux") {
            std::process::Command::new("xdg-open").arg(&entry.path).spawn()
        } else {
            std::process::Command::new("cmd")
                .args(["/C", "start"])
                .arg(&entry.path)
                .spawn()
        };
        match result {
            Ok(_) => self.set_status(&format!("Opened {}", name)),
            Err(e) => self.set_status(&format!("Failed to open {}: {}", name, e)),
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        // `! <cmd>` runs a shell command in the current directory (TUI
        // suspended like the editor, so the output is fully visible).
        if let Some(rest) = cmd.strip_prefix('!') {
            let rest = rest.trim();
            if !rest.is_empty() {
                self.run_shell(rest);
            }
            return;
        }
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "search" | "s" => {
                if parts.len() > 1 {
                    let pattern = parts[1..].join(" ");
                    self.explorer.search(pattern.clone());
                    self.set_status(&format!("Search: {}  (n/N next/prev, Esc clear)", pattern));
                } else {
                    self.set_status("Usage: search <pattern>");
                }
            }
            "cd" => {
                if parts.len() > 1 {
                    match self.navigate_to(parts[1]) {
                        Ok(()) => {
                            self.update_preview();
                            self.set_status(&format!("Changed to: {}", self.explorer.cwd.display()));
                        }
                        Err(e) => self.set_status(&format!("cd failed: {}", e)),
                    }
                } else {
                    self.set_status("Usage: cd <path>");
                }
            }
            "mkdir" => {
                if parts.len() > 1 {
                    match self.explorer.mkdir(parts[1]) {
                        Ok(()) => {
                            self.refresh_info();
                            self.set_status(&format!("Created directory: {}", parts[1]));
                        }
                        Err(e) => self.set_status(&format!("mkdir failed: {}", e)),
                    }
                } else {
                    self.set_status("Usage: mkdir <name>");
                }
            }
            "touch" => {
                if parts.len() > 1 {
                    match self.explorer.touch(parts[1]) {
                        Ok(()) => {
                            self.refresh_info();
                            self.set_status(&format!("Created file: {}", parts[1]));
                        }
                        Err(e) => self.set_status(&format!("touch failed: {}", e)),
                    }
                } else {
                    self.set_status("Usage: touch <name>");
                }
            }
            "rm" => {
                if parts.len() > 1 {
                    match self.explorer.remove(parts[1]) {
                        Ok(()) => {
                            self.refresh_info();
                            self.update_preview();
                            self.set_status(&format!("Removed: {}", parts[1]));
                        }
                        Err(e) => self.set_status(&format!("rm failed: {}", e)),
                    }
                } else {
                    self.set_status("Usage: rm <name>");
                }
            }
            "trash" => {
                if Explorer::trash_dir().is_none() {
                    self.set_status("Trash not supported on this platform");
                    return;
                }
                let paths: Vec<PathBuf> = if parts.len() > 1 {
                    vec![self.explorer.cwd.join(parts[1])]
                } else {
                    self.explorer.selected_paths()
                };
                if paths.is_empty() {
                    self.set_status("Nothing selected to trash");
                    return;
                }
                let (moved, failed) = self.explorer.trash_paths(&paths);
                self.set_status(&format!(
                    "Moved {} item(s) to trash{}  (:restore to undo)",
                    moved,
                    if failed > 0 { format!(", {} failed", failed) } else { String::new() }
                ));
                self.refresh_info();
                self.update_preview();
            }
            "restore" => {
                let msg = self.explorer.restore_last();
                self.refresh_info();
                self.update_preview();
                self.set_status(&msg);
            }
            "rename" => {
                if parts.len() > 1 {
                    let new_name = parts[1..].join(" ");
                    match self.explorer.rename_selected_to(&new_name) {
                        Ok(()) => {
                            self.refresh_info();
                            self.update_preview();
                            self.set_status(&format!("Renamed to: {}", new_name));
                        }
                        Err(e) => self.set_status(&format!("rename failed: {}", e)),
                    }
                } else {
                    self.set_status("Usage: rename <new name>");
                }
            }
            "copy" | "yank" => {
                let n = self.explorer.selected_paths().len();
                self.explorer.copy_selected();
                self.set_status(&format!("Yanked {} item(s)", n));
            }
            "cut" => {
                let n = self.explorer.selected_paths().len();
                self.explorer.cut_selected();
                self.set_status(&format!("Cut {} item(s)", n));
            }
            "paste" => {
                let n = self.explorer.paste();
                if n > 0 {
                    self.set_status(&format!("Pasted {} item(s)", n));
                } else {
                    self.set_status("Clipboard is empty");
                }
                self.refresh_info();
                self.update_preview();
            }
            "open" => self.open_selected(),
            "theme" => {
                if parts.len() > 1 {
                    let name = parts[1..].join(" ");
                    if self.theme.set_by_name(&name) {
                        self.config.theme.name = self.theme.name().to_string();
                        self.set_status(&format!("Theme: {}", self.theme.name()));
                    } else {
                        self.set_status(&format!("Unknown theme: {} (mocha, macchiato, tokyo night, rose pine, nord)", name));
                    }
                } else {
                    self.theme.cycle();
                    self.config.theme.name = self.theme.name().to_string();
                    self.set_status(&format!("Theme: {}", self.theme.name()));
                }
            }
            "hidden" => {
                self.show_hidden = !self.show_hidden;
                self.explorer.set_show_hidden(self.show_hidden);
                self.update_preview();
                self.set_status(&format!("Hidden files: {}", if self.show_hidden { "shown" } else { "hidden" }));
            }
            "sort" => {
                if parts.len() > 1 {
                    match crate::file::explorer::SortMode::from_label(parts[1]) {
                        Some(m) => {
                            self.explorer.set_sort_mode(m);
                            self.set_status(&format!("Sorted by {}", self.explorer.sort_mode.label()));
                        }
                        None => self.set_status("Usage: sort [name|size|ext]"),
                    }
                } else {
                    self.explorer.set_sort_mode(self.explorer.sort_mode.next());
                    self.set_status(&format!("Sorted by {}", self.explorer.sort_mode.label()));
                }
            }
            "sidebar" => {
                self.sidebar_visible = !self.sidebar_visible;
                if !self.sidebar_visible {
                    self.sidebar_focus = false;
                }
                self.set_status(&format!(
                    "Sidebar: {}",
                    if self.sidebar_visible { "shown" } else { "hidden" }
                ));
            }
            "extract" => {
                #[cfg(feature = "archive")]
                {
                    let dest = if parts.len() > 1 { Some(parts[1..].join(" ")) } else { None };
                    match self.explorer.extract_selected(dest.as_deref()) {
                        Ok(msg) => {
                            self.refresh_info();
                            self.update_preview();
                            self.set_status(&msg);
                        }
                        Err(e) => self.set_status(&e),
                    }
                }
                #[cfg(not(feature = "archive"))]
                self.set_status("Extract needs the 'archive' feature");
            }
            "zip" | "targz" | "tar" => {
                #[cfg(feature = "archive")]
                {
                let mut name = match parts.len() {
                    1 => None,
                    _ => Some(parts[1].to_string()),
                };
                // Compress the visual selection when one is active, else the
                // selected entry.
                let paths = if self.visual_anchor != self.explorer.selected {
                    self.visual_paths()
                } else {
                    self.explorer.selected_paths()
                };
                if paths.is_empty() {
                    self.set_status("Nothing selected to compress");
                    return;
                }
                let default_name = |ext: &str| {
                    let p = paths[0]
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "archive".to_string());
                    format!("{}.{}", p, ext)
                };
                let name = match name.take() {
                    Some(n) => {
                        // Add the extension if missing.
                        let lower = n.to_lowercase();
                        if lower.ends_with(".zip") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                            n
                        } else if parts[0] == "zip" {
                            format!("{}.zip", n)
                        } else {
                            format!("{}.tar.gz", n)
                        }
                    }
                    None => default_name(if parts[0] == "zip" { "zip" } else { "tar.gz" }),
                };
                match self.explorer.compress_paths(&paths, &name) {
                    Ok(msg) => {
                        self.refresh_info();
                        self.update_preview();
                        self.set_status(&msg);
                    }
                    Err(e) => self.set_status(&e),
                }
                }
                #[cfg(not(feature = "archive"))]
                self.set_status("Compress needs the 'archive' feature");
            }
            "q" | "quit" | "exit" => {
                self.should_quit = true;
            }
            "help" | "h" => {
                self.toggle_help();
            }
            _ => {
                self.set_status(&format!("Unknown command: {}. Type 'help' for commands.", parts[0]));
            }
        }
    }
}

/// Commands offered by Tab completion in command mode.
const COMMANDS: &[&str] = &[
    "cd", "mkdir", "touch", "rm", "trash", "restore", "rename", "search",
    "sort", "theme", "hidden", "copy", "cut", "paste", "open", "help", "quit",
];

/// Longest common prefix of a list of strings.
fn longest_common_prefix<S: AsRef<str>>(items: &[S]) -> String {
    let mut iter = items.iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut prefix = first.as_ref().to_string();
    for item in iter {
        while !item.as_ref().starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcp_basics() {
        assert_eq!(longest_common_prefix(&["cd", "copy", "cut"]), "c");
        assert_eq!(longest_common_prefix(&["theme", "trash"]), "t");
        assert_eq!(longest_common_prefix(&["cd"]), "cd");
        assert_eq!(longest_common_prefix(&["abc", "xyz"]), "");
    }
}

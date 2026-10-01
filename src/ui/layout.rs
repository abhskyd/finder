//! UI layout and drawing functions.

use ratatui::{
    prelude::*,
    widgets::{
        block::Title, Block, Borders, BorderType, Clear, List, ListItem, ListState, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use unicode_width::UnicodeWidthStr;

use crate::file::explorer::{self, Entry, IconClass};
use crate::file::fuzzy;
use crate::file::git::FileStatus;
use crate::ui::app::{App, Mode};
use crate::ui::sidebar::SidebarItem;
use std::path::{Path, PathBuf};

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.size();

    // Keep the scroll offset in sync with the selection and viewport size.
    app.clamp_scroll();

    // ── No outer frame: panes float on the terminal's own background ──
    // (transparent + blurred terminals keep showing the wallpaper; less
    // border chrome than a box-in-box layout).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(area);
    let (header, main, footer) = (rows[0], rows[1], rows[2]);

    // ── Header: app title + mode badge + breadcrumbs (left), stats (right) ──────
    let mode_badge = match app.mode {
        Mode::Visual => {
            let sel = app
                .explorer
                .selected
                .min(app.explorer.entries.len().saturating_sub(1));
            let (a, b) = (app.visual_anchor.min(sel), app.visual_anchor.max(sel));
            format!(" {} · {} ", mode_label(app), b - a + 1)
        }
        _ => format!(" {} ", mode_label(app)),
    };
    let header_left = Paragraph::new(Line::from(vec![
        Span::styled(
            "finder",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            mode_badge,
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            breadcrumbs(&app.explorer.cwd),
            Style::default().fg(app.theme.dir_fg),
        ),
    ]));
    f.render_widget(header_left, header);

    let header_right = Paragraph::new(Line::from(Span::styled(
        header_stats(app),
        Style::default().fg(app.theme.size_fg),
    )))
    .alignment(Alignment::Right);
    f.render_widget(header_right, header);

    // ── Main: sidebar (optional) + file list (left) + preview (right) ───────────
    // The sidebar auto-hides on narrow terminals.
    let sidebar_on = app.sidebar_visible && main.width >= 90;
    let (sidebar_area, rest) = if sidebar_on {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(30)])
            .spacing(1)
            .split(main);
        (cols[0], cols[1])
    } else {
        // Placeholder area — unused when the sidebar is hidden.
        (Rect::default(), main)
    };
    if sidebar_on {
        draw_sidebar(f, app, sidebar_area);
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .spacing(1)
        .split(rest);

    // Left pane – file list (only the visible slice is rendered, so mouse
    // clicks map 1:1 onto entries via `scroll_offset`). The path already
    // lives in the header breadcrumbs, so the pane title stays empty.
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border_color));
    let list_inner = list_block.inner(cols[0]);
    let list_width = list_inner.width as usize;
    let visible = list_inner.height as usize;
    let start = app.scroll_offset.min(app.explorer.entries.len());
    let end = (start + visible).min(app.explorer.entries.len());

    let items = build_items(app, &app.explorer.entries[start..end], start, list_width);
    let list = List::new(items)
        .block(list_block)
        .highlight_style(app.theme.highlight_style())
        .highlight_symbol("▌ ");
    let mut state = ListState::default();
    state.select(Some(app.explorer.selected.saturating_sub(start)));
    f.render_stateful_widget(list, cols[0], &mut state);

    // Remember where the list viewport is so mouse clicks can be mapped.
    app.list_area = Some(list_inner);

    // Scrollbar on the list's right border when it overflows.
    if app.explorer.entries.len() > visible {
        let mut sb_state = ScrollbarState::new(app.explorer.entries.len())
            .position(app.explorer.selected);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(app.theme.accent));
        f.render_stateful_widget(
            scrollbar,
            cols[0].inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }

    // Empty-directory hint
    if app.explorer.entries.is_empty() {
        let hint = Paragraph::new("directory is empty")
            .style(Style::default().fg(app.theme.size_fg))
            .alignment(Alignment::Center);
        f.render_widget(hint, list_inner);
    }

    // Right pane – preview
    let preview_title = match app.explorer.entries.get(app.explorer.selected) {
        Some(e) => {
            let name = e
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let (icon, class) = explorer::icon(e);
            let lang = app.preview.language();
            let mut spans = vec![
                Span::styled(
                    " Preview ",
                    Style::default()
                        .fg(app.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(icon, Style::default().fg(class_color(app, class))),
                Span::styled(format!(" {}", name), Style::default().fg(app.theme.size_fg)),
            ];
            if !lang.is_empty() {
                spans.push(Span::styled(
                    format!(" · {} ", lang),
                    Style::default()
                        .fg(app.theme.config_fg)
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            Title::from(Line::from(spans))
        }
        None => Title::from(Line::from(Span::styled(
            " Preview ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))),
    };
    let preview_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.border_color))
        .title(preview_title);
    let preview_inner = preview_block.inner(cols[1]);
    // Remember where the preview viewport is so the wheel can scroll it.
    app.preview_area = Some(preview_inner);

    // Clamp the preview scroll to the content height.
    let total_lines = app.preview.line_count();
    let visible_lines = preview_inner.height as usize;
    app.preview_scroll = (app.preview_scroll as usize)
        .min(total_lines.saturating_sub(visible_lines)) as u16;

    let preview = app
        .preview
        .widget()
        .block(preview_block)
        .scroll((app.preview_scroll, 0));
    f.render_widget(preview, cols[1]);

    // Scrollbar on the preview's right border when it overflows.
    if total_lines > visible_lines && visible_lines > 0 {
        let mut sb_state =
            ScrollbarState::new(total_lines).position(app.preview_scroll as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(app.theme.accent));
        f.render_stateful_widget(
            scrollbar,
            cols[1].inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }

    // ── Footer: status message (top), key hints (bottom) ────────────
    let status_area = Rect {
        x: footer.x,
        y: footer.y,
        width: footer.width,
        height: 1,
    };
    let hints_area = Rect {
        x: footer.x,
        y: footer.y + 1,
        width: footer.width,
        height: 1,
    };

    let status_text = if app.status_message.is_empty() {
        "Ready".to_string()
    } else {
        app.status_message.clone()
    };
    f.render_widget(
        Paragraph::new(status_text).style(Style::default().fg(app.theme.accent)),
        status_area,
    );
    f.render_widget(
        Paragraph::new(hints_for(app)).style(Style::default().fg(app.theme.size_fg)),
        hints_area,
    );

    // ── Command / search input line (replaces the hints row) ────────
    if matches!(app.mode, Mode::Command | Mode::Search) {
        let prompt = if app.mode == Mode::Command { ":" } else { "/" };
        let input = Paragraph::new(Line::from(vec![
            Span::styled(
                prompt,
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.command_buffer.clone(),
                Style::default().fg(app.theme.file_fg),
            ),
        ]));
        f.render_widget(Clear, hints_area);
        f.render_widget(input, hints_area);
        // Show the real terminal cursor right after the typed text.
        let buf_w = UnicodeWidthStr::width(app.command_buffer.as_str()) as u16;
        f.set_cursor(hints_area.x + 1 + buf_w, hints_area.y);
    }

    // Plugin overlay UI (toggled with `)
    app.plugins.draw(f);

    // ── Fuzzy finder popup (toggled with f) ────────────────────────
    if app.fuzzy.is_some() {
        draw_fuzzy(f, app, area);
    }

    // ── Help popup (toggled with ?) ────────────────────────────────
    if app.help_open {
        draw_help(f, app, area);
    }

    // ── Bookmarks popup (toggled with w) ──────────────────────────
    if app.bookmarks_open {
        draw_bookmarks(f, app, area);
    }
}

/// Centered bookmarks popup: list, hint, vim-style navigation.
fn draw_bookmarks(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = centered_rect(52, 50, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Title::from(Line::from(Span::styled(
            " Bookmarks ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))))
        .title(Title::from(
            Line::from(Span::styled(
                format!(" {} ", app.config.bookmarks.len()),
                Style::default()
                    .add_modifier(Modifier::REVERSED)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
        ));
    let inner = block.inner(popup);
    let bookmark_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    app.bookmark_area = Some(bookmark_area);

    // Keep the popup's scroll offset in sync with the selection.
    app.clamp_bookmark_scroll();

    let first = app.bookmark_scroll.min(app.config.bookmarks.len());
    let items: Vec<ListItem> = app
        .config
        .bookmarks
        .iter()
        .skip(first)
        .take(bookmark_area.height as usize)
        .map(|(k, path)| {
            let p = short_path(Path::new(path));
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", k),
                    Style::default()
                        .fg(app.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", p), Style::default().fg(app.theme.file_fg)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(app.theme.highlight_style());
    let mut st = ListState::default();
    st.select(if app.config.bookmarks.is_empty() {
        None
    } else {
        Some(app.bookmark_selected.saturating_sub(first))
    });
    f.render_stateful_widget(list, popup, &mut st);

    if app.config.bookmarks.is_empty() {
        let hint = Paragraph::new("no bookmarks yet — press m in a directory")
            .style(Style::default().fg(app.theme.size_fg))
            .alignment(Alignment::Center);
        f.render_widget(hint, inner);
    }

    let hint = Paragraph::new("Enter jump · d remove · Esc close")
        .style(Style::default().fg(app.theme.size_fg));
    let hint_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    f.render_widget(hint, hint_area);
}

/// The places/disks/bookmarks sidebar pane.
fn draw_sidebar(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let focused = app.sidebar_focus;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            app.theme.accent
        } else {
            app.theme.border_color
        }));
    let inner = block.inner(area);
    let items_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height,
    };
    app.sidebar_area = Some(items_area);

    let items = app.sidebar.items(&app.disks, &app.config.bookmarks);
    app.sidebar
        .clamp_scroll(items_area.height.saturating_sub(1) as usize);

    let first = app.sidebar.scroll.min(items.len());
    let visible: Vec<ListItem> = items[first..]
        .iter()
        .take(items_area.height as usize)
        .map(|item| {
            let (label, secondary) = match item {
                SidebarItem::Header(t) => (t.to_string(), String::new()),
                SidebarItem::Place { label, .. } => (label.clone(), String::new()),
                SidebarItem::Disk { mount, free } => {
                    let m = if mount.to_string_lossy() == "/" {
                        "/".to_string()
                    } else {
                        mount
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| mount.display().to_string())
                    };
                    (
                        m,
                        format!("{} free", explorer::format_size(*free)),
                    )
                }
                SidebarItem::Bookmark { key, path } => {
                    (key.to_string(), short_path(path))
                }
            };
            let icon = item.icon();
            let is_header = matches!(item, SidebarItem::Header(_));
            let style = if is_header {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.file_fg)
            };
            let mut spans = Vec::new();
            if !icon.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(if is_header {
                        app.theme.accent
                    } else {
                        class_color(app, crate::file::explorer::IconClass::Dir)
                    }),
                ));
            }
            spans.push(Span::styled(label.to_string(), style));
            if !secondary.is_empty() {
                spans.push(Span::styled(
                    format!(" {}", secondary),
                    Style::default().fg(app.theme.size_fg),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(visible)
        .highlight_style(app.theme.highlight_style());
    let mut st = ListState::default();
    st.select(if items.is_empty() {
        None
    } else {
        Some(app.sidebar.selected.saturating_sub(first))
    });
    f.render_stateful_widget(list, items_area, &mut st);

    if focused {
        let hint = Paragraph::new("Enter jump · Esc back")
            .style(Style::default().fg(app.theme.size_fg));
        let hint_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        f.render_widget(Clear, hint_area);
        f.render_widget(hint, hint_area);
    }
}

/// Centered, scrollable fzf-style popup: results with fuzzy-highlighted
/// matches and the query input at the bottom.
fn draw_fuzzy(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = centered_rect(64, 64, area);
    app.fuzzy_rect = Some(popup);
    f.render_widget(Clear, popup);

    let count = app.fuzzy.as_ref().map_or(0, |fz| fz.matches.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Title::from(Line::from(Span::styled(
            " Find file ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))))
        .title(Title::from(
            Line::from(Span::styled(
                format!(" {} ", count),
                Style::default()
                    .add_modifier(Modifier::REVERSED)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
        ));
    // Render the popup's borders + title (the list below draws without a
    // block, so the block must be rendered explicitly).
    f.render_widget(&block, popup);
    let inner = block.inner(popup);
    let results_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    let input_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };
    app.fuzzy_area = Some(results_area);

    let Some(fz) = app.fuzzy.as_mut() else { return };
    fz.clamp_scroll(results_area.height as usize);

    let query = fz.query.clone();
    let sel_rel = fz.selected.saturating_sub(fz.scroll);
    let visible_paths: Vec<PathBuf> = fz.matches[fz.scroll.min(fz.matches.len())..]
        .iter()
        .take(results_area.height as usize)
        .map(|c| c.path.clone())
        .collect();

    let items: Vec<ListItem> = visible_paths
        .iter()
        .map(|p| fuzzy_item(app, p, &query))
        .collect();
    let empty = items.is_empty();

    let list = List::new(items).highlight_style(app.theme.highlight_style());
    let mut st = ListState::default();
    st.select(Some(sel_rel));
    f.render_stateful_widget(list, results_area, &mut st);

    if empty {
        let hint = Paragraph::new(if query.is_empty() {
            "no files under this directory"
        } else {
            "no matches"
        })
        .style(Style::default().fg(app.theme.size_fg))
        .alignment(Alignment::Center);
        f.render_widget(Clear, results_area);
        f.render_widget(hint, results_area);
    }

    // Query input line at the bottom (fzf-style), with the live cursor.
    let prompt = Paragraph::new(Line::from(vec![
        Span::styled(
            "/ ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(query, Style::default().fg(app.theme.file_fg)),
    ]));
    f.render_widget(prompt, input_area);
    let buf_w = UnicodeWidthStr::width(
        app.fuzzy.as_ref().map_or("", |fz| fz.query.as_str()),
    ) as u16;
    f.set_cursor(input_area.x + 2 + buf_w, input_area.y);
}

/// One fuzzy result: icon + relative path, with the matched characters
/// highlighted. Positions are matched against the file name.
fn fuzzy_item(app: &App, path: &Path, query: &str) -> ListItem<'static> {
    let dummy = Entry {
        path: path.to_path_buf(),
        is_dir: false,
        is_symlink: false,
        is_exec: false,
        size: 0,
        mtime: 0,
        mode: 0,
    };
    let (icon, class) = explorer::icon(&dummy);

    // Relative to the cwd when possible, else home-shortened.
    let rel = path
        .strip_prefix(&app.explorer.cwd)
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| short_path(path));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let positions = fuzzy::match_positions(query, path).unwrap_or_default();
    let name_start = rel.chars().count().saturating_sub(name.chars().count());

    let mut spans = vec![Span::styled(
        format!("{} ", icon),
        Style::default().fg(class_color(app, class)),
    )];
    for (i, ch) in rel.chars().enumerate() {
        let hit = i >= name_start && positions.contains(&(i - name_start));
        if hit {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(app.theme.file_fg),
            ));
        }
    }
    ListItem::new(Line::from(spans))
}

/// Centered, scrollable help popup with the full keybinding reference.
fn draw_help(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let popup = centered_rect(64, 78, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Title::from(Line::from(Span::styled(
            " Help ",
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))))
        .title(Title::from(
            Line::from(Span::styled(
                " q/Esc close ",
                Style::default().fg(app.theme.size_fg),
            ))
            .alignment(Alignment::Right),
        ));
    // Render the popup's borders + title (the paragraph below draws without
    // a block, so the block must be rendered explicitly).
    f.render_widget(&block, popup);
    let inner = block.inner(popup);
    app.help_area = Some(inner);

    let lines = help_lines(app);
    let visible = inner.height as usize;
    app.help_scroll = (app.help_scroll as usize)
        .min(lines.len().saturating_sub(visible)) as u16;

    let para = Paragraph::new(lines).scroll((app.help_scroll, 0));
    f.render_widget(para, inner);
}

/// The help popup's content: every keybinding and command.
fn help_lines(app: &App) -> Vec<Line<'static>> {
    let section = |t: &str| {
        Line::from(Span::styled(
            t.to_string(),
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    };
    let kv = |k: &str, v: &str| {
        Line::from(vec![
            Span::styled(
                format!("  {:<20}", k),
                Style::default().fg(app.theme.dir_fg),
            ),
            Span::styled(v.to_string(), Style::default().fg(app.theme.file_fg)),
        ])
    };
    let blank = || Line::from("");

    vec![
        section("NORMAL MODE"),
        kv("j / k  ↑ / ↓", "move selection (5j = 5 rows)"),
        kv("h / l  ← / →", "parent directory / enter"),
        kv("g / G  Home/End", "go to top / bottom"),
        kv("PgUp / PgDn", "page up / down"),
        kv("Ctrl+d / Ctrl+u", "half page down / up"),
        kv("Ctrl+f / Ctrl+b", "full page down / up"),
        kv("Enter", "enter directory"),
        kv("e", "edit in $EDITOR"),
        kv("o", "open with the system opener"),
        kv("f", "fuzzy find a file under this directory"),
        kv("Ctrl+o / Tab", "directory history: back / forward"),
        kv("/  n / N", "search; next / previous match"),
        kv("y / x / p", "yank / cut / paste"),
        kv("d d", "move to trash (:restore undoes)"),
        kv("D D", "delete permanently"),
        kv("r", "rename (current name prefilled)"),
        kv("n / N", "new file / new directory"),
        kv("v", "visual mode (multi-select)"),
        kv("m + key  ' + key", "bookmark / jump to bookmark"),
        kv("w", "bookmarks popup"),
        kv("s", "cycle sort (name/size/ext/date)"),
        kv("S", "toggle disk-usage sizes"),
        kv("t", "cycle theme"),
        kv(".", "toggle hidden files"),
        kv("R", "refresh directory + git"),
        kv("Ctrl+j / Ctrl+k", "scroll the preview (or wheel over it)"),
        kv("`", "toggle plugin UI"),
        kv("?", "this help"),
        kv("q  :q", "quit"),
        blank(),
        section("COMMAND MODE  (↑/↓ history, Tab completes)"),
        kv("cd <path>", "change directory"),
        kv("mkdir <name>", "create directory"),
        kv("touch <name>", "create file"),
        kv("rm <name>", "remove permanently"),
        kv("trash [name]", "move to the OS trash"),
        kv("restore", "restore the last trashed item"),
        kv("rename <name>", "rename the selected entry"),
        kv("search <pattern>", "filter files"),
        kv("sort [name|size|ext|date]", "set the sort mode"),
        kv("theme [name]", "set the theme"),
        kv("hidden", "toggle hidden files"),
        kv("copy / cut / paste", "clipboard operations"),
        kv("open", "open with the system opener"),
        kv("q / quit  help", "quit / this help"),
        blank(),
        section("VISUAL MODE"),
        kv("j / k", "extend / shrink the selection"),
        kv("y / x / d d", "yank / cut / trash the selection"),
        kv("v / Esc", "exit visual mode"),
    ]
}

/// Color for a git status glyph.
fn git_glyph_color(app: &App, st: FileStatus) -> Color {
    match st {
        FileStatus::Added => app.theme.exec_fg,
        FileStatus::Modified => app.theme.source_fg,
        FileStatus::Deleted => app.theme.archive_fg,
        FileStatus::Renamed => app.theme.symlink_fg,
        FileStatus::Other => app.theme.archive_fg,
    }
}

/// A rect centered within `r` at the given percentages.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}

// ── helpers -----------------------------------------------------------------

fn mode_label(app: &App) -> &'static str {
    match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Visual => "VISUAL",
        Mode::Search => "SEARCH",
        Mode::Command => "COMMAND",
    }
}

/// Right-hand header stats, contextual to the mode.
fn header_stats(app: &App) -> String {
    match app.mode {
        Mode::Visual => {
            let sel = app.explorer.selected.min(app.explorer.entries.len().saturating_sub(1));
            let (a, b) = (app.visual_anchor.min(sel), app.visual_anchor.max(sel));
            format!("{} selected", b - a + 1)
        }
        Mode::Search => format!("{} match(es)", app.explorer.entries.len()),
        _ => {
            let mut stats = format!(
                "{} item(s) · sort: {}",
                app.explorer.entries.len(),
                app.explorer.sort_mode.label()
            );
            // Git: branch + number of changed files.
            if let Some(g) = &app.git_status {
                if let Some(branch) = &g.branch {
                    if g.dirty_count > 0 {
                        stats.push_str(&format!(" · {} ({} changed)", branch, g.dirty_count));
                    } else {
                        stats.push_str(&format!(" · {}", branch));
                    }
                }
            }
            // Disk-usage total for the current directory.
            if app.du_enabled {
                match app.dir_sizes.as_ref().and_then(|d| d.size_of(&d.root)) {
                    Some(total) => {
                        stats.push_str(&format!(" · du {}", explorer::format_size(total)))
                    }
                    None => stats.push_str(" · du …"),
                }
            }
            if !app.explorer.clipboard.is_empty() {
                stats.push_str(&format!(
                    " · {} {}",
                    if app.explorer.cut_mode { "cut" } else { "yank" },
                    app.explorer.clipboard.len()
                ));
            }
            if app.explorer.show_hidden {
                stats.push_str(" · hidden");
            }
            stats
        }
    }
}

fn hints_for(app: &App) -> &'static str {
    match app.mode {
        Mode::Normal => "↑↓/PgUp move · ⏎/l open · f find · ? help · ⇥ sidebar · y yank · x cut · p paste · dd trash · X extract · : cmd",
        Mode::Visual => "j/k extend · y yank · x cut · d trash · v/Esc exit",
        Mode::Search => "type to filter · Enter accept · Esc cancel",
        Mode::Command => "type command · ↑/↓ history · ⇥ complete · Enter run · Esc cancel",
    }
}

/// Breadcrumbs for the current path: `~ › Documents › projects`.
fn breadcrumbs(p: &Path) -> String {
    let s = short_path(p);
    if s == "/" {
        return "/".to_string();
    }
    s.split('/').filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" › ")
}

/// eza-style color per icon class.
fn class_color(app: &App, class: IconClass) -> Color {
    match class {
        IconClass::Dir => app.theme.dir_fg,
        IconClass::Symlink => app.theme.symlink_fg,
        IconClass::Exec => app.theme.exec_fg,
        IconClass::Source => app.theme.source_fg,
        IconClass::Doc => app.theme.doc_fg,
        IconClass::Archive => app.theme.archive_fg,
        IconClass::Config => app.theme.config_fg,
        IconClass::Media => app.theme.media_fg,
        IconClass::Audio => app.theme.audio_fg,
        IconClass::Book => app.theme.book_fg,
        IconClass::Text => app.theme.file_fg,
    }
}

/// Style for an entry's name (eza colors the icon; names stay muted, with
/// bold for directories and source files).
fn entry_style(app: &App, e: &Entry) -> Style {
    if e.is_dir {
        Style::default()
            .fg(app.theme.dir_fg)
            .add_modifier(Modifier::BOLD)
    } else if e.is_symlink {
        Style::default().fg(app.theme.symlink_fg)
    } else if e.is_exec {
        Style::default().fg(app.theme.exec_fg)
    } else {
        let mut s = Style::default().fg(app.theme.file_fg);
        if explorer::icon(e).1 == IconClass::Source {
            s = s.add_modifier(Modifier::BOLD);
        }
        s
    }
}

fn display_name(e: &Entry) -> String {
    let mut name = e
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| e.path.display().to_string());
    if e.is_dir {
        name.push('/');
    } else if e.is_symlink {
        name.push('@');
    }
    name
}

/// Build list items for the visible slice: icon + name (padded) + size.
/// `offset` is the index of the first entry in `entries`.
fn build_items(app: &App, entries: &[Entry], offset: usize, width: usize) -> Vec<ListItem<'static>> {
    let sel = app.explorer.selected;
    let visual_range = if app.mode == Mode::Visual {
        let sel = sel.min(app.explorer.entries.len().saturating_sub(1));
        Some((app.visual_anchor.min(sel), app.visual_anchor.max(sel)))
    } else {
        None
    };

    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let idx = offset + i;
            let mut style = entry_style(app, e);
            let (icon, class) = explorer::icon(e);
            let name = display_name(e);
            // Disk-usage mode: directories show their recursive size.
            let size = if app.du_enabled && e.is_dir {
                match app.dir_sizes.as_ref().and_then(|d| d.size_of(&e.path)) {
                    Some(s) => explorer::format_size(s),
                    None => "…".to_string(),
                }
            } else {
                explorer::format_size(e.size)
            };
            // Git status glyph (only inside a repo); a fixed 1-char cell
            // keeps the size column aligned across rows.
            let git_status = app
                .git_status
                .as_ref()
                .and_then(|g| g.effective_status(&e.path));
            let git_pad = if git_status.is_some() { 2 } else { 0 };
            // Modified-date column (hidden on narrow panes) fills the space
            // between names and sizes.
            let show_date = width >= 56;
            let date = if show_date {
                explorer::format_date_short(e.mtime)
            } else {
                String::new()
            };
            let date_pad = if show_date { 1 + date.len() } else { 0 };

            // Reserve: symbol(2) + icon+space(2) + gap(1) + git(2) + size +
            // date, so the size/date columns right-align. Display-width aware.
            let size_w = UnicodeWidthStr::width(size.as_str());
            let name_width =
                width.saturating_sub(size_w + 5 + git_pad + date_pad);
            let shown = truncate_width(&name, name_width);
            let shown_w = UnicodeWidthStr::width(shown.as_str());
            let pad = name_width.saturating_sub(shown_w);

            let in_visual = visual_range.is_some_and(|(a, b)| idx >= a && idx <= b);
            if in_visual {
                style = app.theme.visual_style();
            }

            let mut spans = vec![
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(class_color(app, class)),
                ),
                Span::styled(format!("{}{}", shown, " ".repeat(pad)), style),
                Span::raw(" "),
            ];
            if let Some(st) = git_status {
                spans.push(Span::styled(
                    format!("{} ", st.glyph()),
                    Style::default().fg(git_glyph_color(app, st)),
                ));
            }
            spans.push(Span::styled(size, Style::default().fg(app.theme.size_fg)));
            if show_date {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(date, Style::default().fg(app.theme.size_fg)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Unicode-safe truncation with an ellipsis.
fn truncate_width(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// Shorten the home directory to `~` for display (shared with the config).
fn short_path(p: &Path) -> String {
    crate::config::Config::shorten_path(p)
}

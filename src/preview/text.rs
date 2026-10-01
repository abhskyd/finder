//! Preview: text with syntax highlighting, plus archive contents listing.
//!
//! Slow work (archive listing, syntax highlighting of big files) runs on a
//! background thread and is sent back over a channel, so the UI never
//! blocks. The host polls `poll()` each loop iteration; results for a stale
//! selection are discarded.

use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style};
use syntect::util::LinesWithEndings;
use ratatui::style::{Color, Style as TuiStyle};
use ratatui::text::{Span, Line, Text};
use std::path::{Path, PathBuf};
use std::io::Read;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::file::explorer::{self, Entry};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Cap the preview so highlighting stays fast while scrolling.
const MAX_PREVIEW_SIZE: u64 = 100 * 1024;

/// What a background preview load produced.
pub enum LoadResult {
    /// Fully decorated text + the syntax name (for the title).
    Text(Text<'static>, String),
    /// A placeholder message (binary, too large, unreadable, …).
    /// (Only the cfg-gated archive renderer produces this.)
    #[allow(dead_code)]
    Message(String),
}

pub struct Preview {
    ss: Arc<SyntaxSet>,
    ts: Arc<ThemeSet>,
    theme_name: String,
    content: Text<'static>,
    /// Name of the syntax used for the current preview (e.g. "Rust").
    syntax_name: String,
    /// Pending background load: the path it was started for + the receiver.
    pending: Option<(PathBuf, Receiver<LoadResult>)>,
}

impl Preview {
    pub fn new() -> Self {
        let ss = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        Self {
            ss: Arc::new(ss),
            ts: Arc::new(ts),
            theme_name: "base16-ocean.dark".into(),
            content: Text::raw(""),
            syntax_name: String::new(),
            pending: None,
        }
    }

    /// Preview a regular file. Metadata comes from the Entry, no re-stat.
    /// Slow work happens in the background.
    pub fn load(&mut self, e: &Entry) {
        self.syntax_name.clear();
        let path = e.path.as_path();

        // Archive → background contents listing.
        if explorer::archive_kind(path).is_some() {
            self.load_archive(path, e);
            return;
        }

        // Text: the size/binary checks are cheap, so they stay synchronous —
        // the syntax highlighting (the slow part) runs in the background.
        let size = e.size;
        if size > MAX_PREVIEW_SIZE {
            self.content = Text::raw(format!(
                "File too large for preview ({})\n\nPress e to edit it",
                explorer::format_size(size)
            ));
            return;
        }

        // Peek at the first 8 KiB: NUL bytes mean binary content.
        let binary = std::fs::File::open(path)
            .map(|mut f| {
                let mut buf = [0u8; 8192];
                let n = f.read(&mut buf).unwrap_or(0);
                buf[..n].contains(&0)
            })
            .unwrap_or(true);
        if binary {
            self.content = Text::raw(format!(
                "Binary file ({}) — no preview",
                explorer::format_size(size)
            ));
            return;
        }

        match std::fs::read_to_string(path) {
            Ok(data) => {
                let ss = Arc::clone(&self.ss);
                let ts = Arc::clone(&self.ts);
                let theme_name = self.theme_name.clone();
                let p = path.to_path_buf();
                let pc = p.clone();
                let meta = e.clone();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(highlight_text(&data, &pc, &ss, &ts, &theme_name, &meta));
                });
                self.pending = Some((p, rx));
            }
            Err(_) => self.content = Text::raw("Unable to read file"),
        }
    }

    /// Archive preview: contents listing on a background thread.
    fn load_archive(&mut self, path: &Path, e: &Entry) {
        #[cfg(feature = "archive")]
        {
            let p = path.to_path_buf();
            let size = e.size;
            let pc = p.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::preview::archive::render(&pc, size));
            });
            self.pending = Some((p, rx));
        }
        #[cfg(not(feature = "archive"))]
        {
            let _ = (path, e);
            self.content =
                Text::raw("[Archive preview not enabled. Compile with 'archive' feature]");
        }
    }

    /// Apply a finished background load to the preview state.
    fn apply(&mut self, result: LoadResult) {
        match result {
            LoadResult::Text(text, syntax) => {
                self.syntax_name = syntax;
                self.content = text;
            }
            LoadResult::Message(msg) => {
                self.syntax_name.clear();
                self.content = Text::raw(msg);
            }
        }
    }

    /// Poll a pending background load without blocking. Applies the result
    /// when it's still for the currently selected entry; stale results are
    /// discarded. Returns true when the preview content changed.
    pub fn poll(&mut self, current: Option<&Path>) -> bool {
        let Some((path, rx)) = &mut self.pending else {
            return false;
        };
        match rx.try_recv() {
            Ok(result) => {
                let path = std::mem::take(path);
                self.pending = None;
                if current == Some(path.as_path()) {
                    self.apply(result);
                    true
                } else {
                    false
                }
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                false
            }
        }
    }

    /// Preview a directory: item count, inspector line, contents peek.
    /// (Cheap enough to stay synchronous.)
    pub fn load_dir(&mut self, path: &Path) {
        self.syntax_name.clear();
        self.pending = None;

        let count = std::fs::read_dir(path).map(|d| d.count()).unwrap_or(0);
        let peek: Vec<Entry> = std::fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .take(50)
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        Some(Entry {
                            path: e.path(),
                            is_dir: meta.is_dir(),
                            is_symlink: e.file_type().map(|t| t.is_symlink()).unwrap_or(false),
                            is_exec: false,
                            size: meta.len(),
                            mtime: 0,
                            mode: 0,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut text = format!("{} item(s)", count);
        // Inspector line, consistent with the file preview.
        if let Ok(meta) = std::fs::metadata(path) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            #[cfg(unix)]
            let perms = format_mode(meta.permissions().mode());
            #[cfg(not(unix))]
            let perms = String::new();
            let mut info = format!("modified {}", format_mtime(mtime));
            if !perms.is_empty() {
                info.push_str(&format!(" · {}", perms));
            }
            text.push_str(&format!("\n{}\n", info));
        } else {
            text.push('\n');
        }
        if !peek.is_empty() {
            text.push('\n');
            for e in &peek {
                let name = e
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let suffix = if e.is_dir { "/" } else { "" };
                text.push_str(&format!("  {} {}{}\n", explorer::icon(e).0, name, suffix));
            }
            if count > peek.len() {
                text.push_str(&format!("\n  … {} more", count - peek.len()));
            }
        }
        text.push_str("\nPress Enter or l to open");
        self.content = Text::raw(text);
    }

    pub fn clear(&mut self) {
        self.content = Text::raw("");
        self.syntax_name.clear();
        self.pending = None;
    }

    pub fn widget(&self) -> ratatui::widgets::Paragraph<'_> {
        ratatui::widgets::Paragraph::new(self.content.clone())
            // trim: false keeps code indentation intact
            .wrap(ratatui::widgets::Wrap { trim: false })
    }

    /// Syntax language used for the current preview ("" if none).
    pub fn language(&self) -> &str {
        &self.syntax_name
    }

    /// Total number of lines in the current preview (for the scrollbar).
    pub fn line_count(&self) -> usize {
        self.content.lines.len()
    }
}

/// Syntax-highlight a file's contents and build the fully decorated preview
/// (inspector line + line numbers). Runs on a background thread.
fn highlight_text(
    data: &str,
    path: &Path,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    theme_name: &str,
    meta: &Entry,
) -> LoadResult {
    let (size, mtime, mode) = (meta.size, meta.mtime, meta.mode);
    let syntax = ss
        .find_syntax_for_file(path)
        .unwrap_or(None)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = syntect::easy::HighlightLines::new(syntax, &ts.themes[theme_name]);

    let mut spans: Vec<Line> = Vec::new();
    for line in LinesWithEndings::from(data) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, ss).unwrap_or_default();
        let mut line_spans: Vec<Span> = Vec::with_capacity(ranges.len() + 1);
        line_spans.extend(ranges.into_iter().map(|(style, text)| {
            Span::styled(
                text.to_string(),
                TuiStyle::default().fg(Color::Rgb(
                    style.foreground.r,
                    style.foreground.g,
                    style.foreground.b,
                )),
            )
        }));
        spans.push(Line::from(line_spans));
    }

    // Inspector line: size · modified · permissions, then numbered lines.
    let info = format!(
        "{} · modified {} · {}",
        explorer::format_size(size),
        format_mtime(mtime),
        format_mode(mode),
    );
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(info, TuiStyle::default().fg(Color::DarkGray))),
        Line::from(""),
    ];
    for (i, line) in spans.into_iter().enumerate() {
        let mut line_spans: Vec<Span> = vec![Span::styled(
            format!("{:>4}  ", i + 1),
            TuiStyle::default().fg(Color::DarkGray),
        )];
        line_spans.extend(line.spans);
        lines.push(Line::from(line_spans));
    }
    LoadResult::Text(Text::from(lines), syntax.name.clone())
}

/// Format a Unix timestamp as `YYYY-MM-DD HH:MM` (UTC, no extra deps).
fn format_mtime(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, mi) = (rem / 3600, (rem % 3600) / 60);

    // Howard Hinnant's days-from-civil inverse.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, h, mi)
}

/// Format Unix permission bits as `rwxr-xr-x` (empty on non-unix).
fn format_mode(mode: u32) -> String {
    if mode == 0 {
        return String::new();
    }
    let bit = |b: u32, c: char| if mode & b != 0 { c } else { '-' };
    format!(
        "{}{}{}{}{}{}{}{}{}",
        bit(0o400, 'r'), bit(0o200, 'w'), bit(0o100, 'x'),
        bit(0o040, 'r'), bit(0o020, 'w'), bit(0o010, 'x'),
        bit(0o004, 'r'), bit(0o002, 'w'), bit(0o001, 'x'),
    )
}

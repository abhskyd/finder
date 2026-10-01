//! Archive preview: contents listing for zip / tar / tar.gz.
//!
//! `render` runs on a background thread (see `Preview::load_archive`) and
//! builds the fully decorated preview text, so the UI never blocks on I/O.

use std::path::Path;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};

/// One entry inside an archive.
struct ArchiveEntry {
    name: String,
    size: u64,
    is_dir: bool,
}

/// A listing of an archive's contents.
struct ArchiveInfo {
    /// "zip" | "tar" | "tar.gz"
    kind: &'static str,
    entries: Vec<ArchiveEntry>,
}

/// Cap on listed entries so a huge archive can't stall the preview.
const MAX_ENTRIES: usize = 500;

/// List the contents of an archive and build the preview text.
pub fn render(path: &Path, size: u64) -> crate::preview::text::LoadResult {
    use crate::preview::text::LoadResult;

    let Some(info) = list_zip(path)
        .or_else(|| list_tar(path, false))
        .or_else(|| list_tar(path, true))
    else {
        return LoadResult::Message("Unable to read archive contents".into());
    };

    let header = Line::from(Span::styled(
        format!(
            "{} archive · {} entr{} · {}",
            info.kind.to_uppercase(),
            info.entries.len(),
            if info.entries.len() == 1 { "y" } else { "ies" },
            crate::file::explorer::format_size(size)
        ),
        Style::default().fg(Color::DarkGray),
    ));
    let mut lines: Vec<Line> = vec![header, Line::from("")];

    for entry in info.entries.iter().take(MAX_ENTRIES) {
        // Icon by the entry's extension (dummy path lookup).
        let dummy = crate::file::explorer::Entry {
            path: std::path::PathBuf::from(&entry.name),
            is_dir: entry.is_dir,
            is_symlink: false,
            is_exec: false,
            size: entry.size,
            mtime: 0,
            mode: 0,
        };
        let name = if entry.is_dir && !entry.name.ends_with('/') {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", crate::file::explorer::icon(&dummy).0),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                name,
                Style::default().fg(if entry.is_dir {
                    Color::Blue
                } else {
                    Color::Reset
                }),
            ),
            Span::styled(
                format!(" {}", crate::file::explorer::format_size(entry.size)),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if info.entries.len() > MAX_ENTRIES {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("… {} more entries", info.entries.len() - MAX_ENTRIES),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "extract with o",
        Style::default().fg(Color::DarkGray),
    )));

    LoadResult::Text(
        Text::from(lines),
        info.kind.to_uppercase().to_string(),
    )
}

/// List the entries of a zip archive.
fn list_zip(path: &Path) -> Option<ArchiveInfo> {
    let file = std::fs::File::open(path).ok()?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut entries = Vec::new();
    for i in 0..z.len() {
        let f = z.by_index(i).ok()?;
        entries.push(ArchiveEntry {
            name: f.name().to_string(),
            size: f.size(),
            is_dir: f.is_dir(),
        });
        if entries.len() >= MAX_ENTRIES {
            break;
        }
    }
    Some(ArchiveInfo {
        kind: "zip",
        entries,
    })
}

/// List the entries of a tar archive, optionally gzipped.
fn list_tar(path: &Path, gz: bool) -> Option<ArchiveInfo> {
    let file = std::fs::File::open(path).ok()?;
    let reader: Box<dyn std::io::Read> = if gz {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut ar = tar::Archive::new(reader);
    let mut entries = Vec::new();
    for e in ar.entries().ok()? {
        let e = e.ok()?;
        let header = e.header();
        // Strip the leading "./" that tars created with relative paths have.
        let raw_name = e.path().ok()?.display().to_string();
        let name = raw_name.strip_prefix("./").unwrap_or(&raw_name).to_string();
        if name.is_empty() {
            continue;
        }
        entries.push(ArchiveEntry {
            name,
            size: header.size().ok()?,
            is_dir: header.entry_type().is_dir(),
        });
        if entries.len() >= MAX_ENTRIES {
            break;
        }
    }
    if !gz && entries.is_empty() {
        return None; // not a tar
    }
    Some(ArchiveInfo {
        kind: if gz { "tar.gz" } else { "tar" },
        entries,
    })
}

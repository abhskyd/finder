//! Theme handling: soft, muted accent palettes curated for dark, transparent
//! terminals. The terminal's own background (and any blur behind it) is never
//! repainted — only foreground accents are applied.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub name: &'static str,
    pub file_fg: Color,
    pub dir_fg: Color,
    pub symlink_fg: Color,
    pub exec_fg: Color,
    pub border_color: Color,
    pub size_fg: Color,
    pub accent: Color,
    /// Muted selection background (softer than reverse video).
    pub selection_bg: Color,
    // eza-style per-type icon colors
    pub source_fg: Color,
    pub doc_fg: Color,
    pub archive_fg: Color,
    pub config_fg: Color,
    pub media_fg: Color,
    pub audio_fg: Color,
    pub book_fg: Color,
}

/// All palettes are foreground-only: no background is ever painted, so a
/// transparent + blurred terminal keeps showing the wallpaper everywhere.
pub const PALETTES: [Palette; 5] = [
    // Catppuccin Mocha — the default: soft pastels tuned for dark screens
    Palette {
        name: "Mocha",
        file_fg: Color::Rgb(205, 214, 244),   // text
        dir_fg: Color::Rgb(137, 180, 250),    // blue
        symlink_fg: Color::Rgb(245, 194, 231),// pink
        exec_fg: Color::Rgb(166, 227, 161),   // green
        border_color: Color::Rgb(88, 91, 112),// surface2
        size_fg: Color::Rgb(108, 112, 134),   // overlay0
        accent: Color::Rgb(203, 166, 247),    // mauve
        selection_bg: Color::Rgb(69, 71, 90),
        source_fg: Color::Rgb(249, 226, 175), // yellow
        doc_fg: Color::Rgb(166, 227, 161),    // green
        archive_fg: Color::Rgb(243, 139, 168),// red
        config_fg: Color::Rgb(148, 226, 213), // teal
        media_fg: Color::Rgb(245, 194, 231),  // pink
        audio_fg: Color::Rgb(137, 220, 235),  // sky
        book_fg: Color::Rgb(180, 190, 254),   // lavender
    },
    // Catppuccin Macchiato — a slightly warmer, lighter sibling
    Palette {
        name: "Macchiato",
        file_fg: Color::Rgb(202, 211, 245),   // text
        dir_fg: Color::Rgb(138, 173, 244),    // blue
        symlink_fg: Color::Rgb(245, 189, 230),// pink
        exec_fg: Color::Rgb(166, 218, 149),   // green
        border_color: Color::Rgb(115, 121, 148),// overlay1
        size_fg: Color::Rgb(110, 115, 141),   // overlay0
        accent: Color::Rgb(198, 160, 246),    // mauve
        selection_bg: Color::Rgb(73, 77, 100),
        source_fg: Color::Rgb(238, 212, 159), // yellow
        doc_fg: Color::Rgb(166, 218, 149),    // green
        archive_fg: Color::Rgb(237, 135, 150),// red
        config_fg: Color::Rgb(139, 213, 202), // teal
        media_fg: Color::Rgb(245, 189, 230),  // pink
        audio_fg: Color::Rgb(145, 215, 227),  // sky
        book_fg: Color::Rgb(183, 189, 248),   // lavender
    },
    // Tokyo Night — calm indigo tones
    Palette {
        name: "Tokyo Night",
        file_fg: Color::Rgb(192, 202, 245),   // fg
        dir_fg: Color::Rgb(122, 162, 247),    // blue
        symlink_fg: Color::Rgb(187, 154, 247),// purple
        exec_fg: Color::Rgb(158, 206, 106),   // green
        border_color: Color::Rgb(86, 95, 137),// bg_highlight
        size_fg: Color::Rgb(86, 95, 137),     // dark comment
        accent: Color::Rgb(224, 175, 104),    // orange
        selection_bg: Color::Rgb(41, 46, 66),
        source_fg: Color::Rgb(224, 175, 104), // yellow
        doc_fg: Color::Rgb(158, 206, 106),    // green
        archive_fg: Color::Rgb(247, 118, 142),// red
        config_fg: Color::Rgb(115, 218, 202), // teal
        media_fg: Color::Rgb(187, 154, 247),  // purple
        audio_fg: Color::Rgb(125, 207, 255),  // cyan
        book_fg: Color::Rgb(192, 202, 245),   // fg
    },
    // Rosé Pine — muted rose and foam
    Palette {
        name: "Rose Pine",
        file_fg: Color::Rgb(224, 222, 244),   // text
        dir_fg: Color::Rgb(156, 207, 216),    // foam
        symlink_fg: Color::Rgb(196, 167, 231),// iris
        exec_fg: Color::Rgb(246, 193, 119),   // gold
        border_color: Color::Rgb(82, 79, 103),// subtle
        size_fg: Color::Rgb(144, 140, 170),   // muted
        accent: Color::Rgb(235, 188, 186),    // rose
        selection_bg: Color::Rgb(64, 61, 82),
        source_fg: Color::Rgb(246, 193, 119), // gold
        doc_fg: Color::Rgb(156, 207, 216),    // foam
        archive_fg: Color::Rgb(235, 111, 146),// love
        config_fg: Color::Rgb(196, 167, 231), // iris
        media_fg: Color::Rgb(235, 188, 186),  // rose
        audio_fg: Color::Rgb(156, 207, 216),  // foam
        book_fg: Color::Rgb(224, 222, 244),   // text
    },
    // Nord — cool, desaturated blues
    Palette {
        name: "Nord",
        file_fg: Color::Rgb(216, 222, 233),   // nord4
        dir_fg: Color::Rgb(129, 161, 193),    // nord9
        symlink_fg: Color::Rgb(180, 142, 173),// nord15
        exec_fg: Color::Rgb(163, 190, 140),   // nord14
        border_color: Color::Rgb(76, 86, 106),// nord3
        size_fg: Color::Rgb(76, 86, 106),     // nord3
        accent: Color::Rgb(235, 203, 139),    // nord13
        selection_bg: Color::Rgb(67, 76, 94),
        source_fg: Color::Rgb(235, 203, 139), // nord13
        doc_fg: Color::Rgb(143, 188, 187),    // nord7
        archive_fg: Color::Rgb(191, 97, 106), // nord11
        config_fg: Color::Rgb(136, 192, 208), // nord8
        media_fg: Color::Rgb(180, 142, 173),  // nord15
        audio_fg: Color::Rgb(129, 161, 193),  // nord9
        book_fg: Color::Rgb(216, 222, 233),   // nord4
    },
];

pub struct Theme {
    name: String,
    pub file_fg: Color,
    pub dir_fg: Color,
    pub symlink_fg: Color,
    pub exec_fg: Color,
    pub border_color: Color,
    pub size_fg: Color,
    pub accent: Color,
    pub selection_bg: Color,
    pub source_fg: Color,
    pub doc_fg: Color,
    pub archive_fg: Color,
    pub config_fg: Color,
    pub media_fg: Color,
    pub audio_fg: Color,
    pub book_fg: Color,
}

impl Theme {
    pub fn default() -> Self {
        Self::from_palette(0)
    }

    fn from_palette(idx: usize) -> Self {
        let p = PALETTES[idx];
        Self {
            name: p.name.into(),
            file_fg: p.file_fg,
            dir_fg: p.dir_fg,
            symlink_fg: p.symlink_fg,
            exec_fg: p.exec_fg,
            border_color: p.border_color,
            size_fg: p.size_fg,
            accent: p.accent,
            selection_bg: p.selection_bg,
            source_fg: p.source_fg,
            doc_fg: p.doc_fg,
            archive_fg: p.archive_fg,
            config_fg: p.config_fg,
            media_fg: p.media_fg,
            audio_fg: p.audio_fg,
            book_fg: p.book_fg,
        }
    }

    /// Cycle to the next accent palette.
    pub fn cycle(&mut self) {
        let idx = PALETTES
            .iter()
            .position(|p| p.name == self.name)
            .unwrap_or(0);
        *self = Self::from_palette((idx + 1) % PALETTES.len());
    }

    /// Select a palette by (case-insensitive) name. Returns false if not found.
    pub fn set_by_name(&mut self, name: &str) -> bool {
        let lower = name.to_lowercase();
        for (i, p) in PALETTES.iter().enumerate() {
            if p.name.to_lowercase() == lower {
                *self = Self::from_palette(i);
                return true;
            }
        }
        false
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Cursor highlight: a muted selection background + bold text. Softer
    /// than reverse video — the entry keeps its own color on the bar.
    pub fn highlight_style(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .add_modifier(Modifier::BOLD)
    }

    /// Visual-mode selection highlight (no bold, so the cursor stands out).
    pub fn visual_style(&self) -> Style {
        Style::default().bg(self.selection_bg)
    }
}

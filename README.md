# finder

A modern, fast TUI (Terminal User Interface) file manager written in Rust using [ratatui](https://github.com/ratatui/ratatui), with nvim-style keybindings. Inspired by superfile — with git integration, fully asynchronous previews (archive listing and syntax highlighting render on background threads), disk-usage mode and a single static binary.

## Features

- **Sidebar**: Places (home, desktop, documents, downloads, pictures, music, videos), mounted **disks with free space** (background `df` query), and your **bookmarks** — click to jump, `Tab` to focus and `j/k`/`Enter` to navigate
- **Extract**: `X` extracts the selected zip/tar/tar.gz into a directory named after it (or `:extract <dir>`)
- **Compress**: `:zip <name>` / `:targz <name>` build archives from the selected entry or the visual selection (extension auto-added)
- **Shell mode**: `! <cmd>` runs a shell command in the current directory with the TUI suspended, so the output is fully visible
- **Fuzzy Finder**: Press `f` for an fzf-style popup over every file under the current directory — smart subsequence scoring (filename hits beat path hits), matched characters highlighted, respects `.gitignore`, jump with Enter, mouse-friendly
- **Git Integration**: Background `git status` — branch + changed-count badge in the header, per-file status glyphs (`M` modified, `+` added/untracked, `–` deleted, `R` renamed), never blocks the UI; `R` forces a refresh
- **Directory History**: `Ctrl+o` back, `Tab` forward — like nvim's jump list, with status-line breadcrumbs
- **Help Popup**: `?` opens a scrollable popup with the full keybinding + command reference
- **Disk Usage**: `S` toggles recursive directory sizes — computed on a background thread, shown in the list, in the preview and as a total in the header
- **Bookmarks**: Mark directories (`m` + key), jump back (`'` + key), or browse them in a clickable, scrollable popup (`w`) — persisted in the config
- **Trash**: `dd` moves to the OS trash (macOS `~/.Trash` / XDG trash), `:restore` undoes; `D`/`:rm` delete permanently
- **Config file**: `~/.config/finder/config.toml` — start dir, hidden-files default, sort order, theme, bookmarks (sort + hidden-file state are saved on exit too)
- **Mouse Support**: Click to select, double-click to open, scroll wheel per pane (list scrolls, preview scrolls under the pointer), popups clickable
- **Standard Navigation**: Arrow keys, PgUp/PgDn, Home/End — plus vim-style motions as a bonus
- **nvim-style Modes**: Normal, Visual (multi-select), Search, and Command modes with a mode badge
- **Vim Motions**: `h/j/k/l`, `g`/`G`, counts (`5j`), half-page and full-page scrolling (`Ctrl+d/u/f/b`)
- **Live Search**: Press `/` and type to filter as you go; `n`/`N` cycle matches, `Esc` clears
- **Visual Multi-select**: Press `v`, extend with `j/k`, then yank/cut/delete the whole range
- **File Operations**: Yank (`y`), Cut (`x`), Paste (`p`), Delete (`dd`, with confirmation), Rename (`r` — current name prefilled)
- **Create**: New file (`n`), New directory (`N`) — both prompt for a name
- **Command Mode Extras**: `↑`/`↓` walk the command history, `Tab` completes commands and file names (nested paths too)
- **Open Files**: Edit in `$EDITOR` (`e`/`Enter`), open with the system opener (`o`)
- **Sorting**: Cycle name → size → ext → date with `s` (directories always first)
- **eza-style Icons**: Nerd Font icons per file type matching eza's icon set (🦀→``, languages, archives, git files, …), each colored by type — requires a Nerd Font (e.g. JetBrainsMono Nerd Font)
- **Scrollbars**: Position indicators on the list and preview borders when content overflows
- **Breadcrumbs**: Header shows the path as `~ › Documents › projects`
- **File Inspector**: Preview shows size · modified date · permissions; directories preview their contents
- **Scrollable Preview**: `Ctrl+j`/`Ctrl+k` (or the wheel over the preview) scroll long files with line numbers
- **Syntax Highlighting**: Preview text files with syntax highlighting via syntect, with line numbers — source, config, markdown and more get language-aware colors; highlighting of big files runs on a background thread
- **Archive Preview**: zip / tar / tar.gz contents are listed with per-entry icons and sizes, on a background thread
- **Binary Handling**: Binary files are detected and say so
- **File Indicators**: `ls`-style glyphs — directories (`/`), symlinks (`@`), executables colored — plus right-aligned size and modified-date columns (the date hides on narrow panes)
- **Smooth Rendering**: Redraws only on change (near-zero idle CPU) — every slow preview (image, PDF, archive, big-file highlighting) loads on a background thread, so moving the cursor stays instant; status messages auto-expire to "Ready", live terminal cursor while typing
- **Respects Your Terminal Theme**: No outer frame and no pane-background repaints — transparent + blurred terminals keep showing the wallpaper everywhere except the selection bar; accents are foreground-only pastels (Catppuccin Mocha by default, cycle with `t`)
- **Hidden Files**: Toggle with `.`
- **Smart Cursor**: The cursor follows a file through refreshes; going up a directory lands on the directory you came from
- **Plugin System**: Dynamic plugin loading via libloading

## Keybindings

### Normal Mode
| Key | Action |
|-----|--------|
| `q`, `:q` | Quit |
| `↑` / `↓` | Move selection (mouse wheel also works) |
| `←` / `→` | Go to parent directory / enter |
| `PgUp` / `PgDn` | Page up / down |
| `Home` / `End` | Go to top / bottom |
| **Mouse click** | Select entry |
| **Mouse double-click** | Enter directory / open file |
| **Mouse wheel** | Scroll the list (or the preview when over it) |
| `f` | Fuzzy finder popup (jump to any file below the current directory) |
| `?` | Help popup (scrollable full reference) |
| `Ctrl+o` / `Alt+←` | Directory history: back |
| `Alt+→` | Directory history: forward |
| `Tab` | Focus the sidebar (`j/k` + `Enter`, `Esc` back) |
| `X` | Extract the selected zip/tar/tar.gz |
| `!` | Shell command (prompts with `!`) |
| `Ctrl+j` / `Ctrl+k` | Scroll the preview pane |
| `R` | Refresh directory + git status |
| `j` / `k` | Move down / up (vim-style) |
| `h` / `l` | Parent directory / enter (vim-style) |
| `g` / `G` | Go to top / bottom (vim-style) |
| `Ctrl+d` / `Ctrl+u` | Half page down / up |
| `Ctrl+f` / `Ctrl+b` | Full page down / up |
| `y` / `c` | Yank (copy) selected |
| `x` | Cut selected |
| `p` | Paste |
| `d` `d` | Move to trash (press twice; `:restore` to undo) |
| `D` `D` | Delete permanently (press twice) |
| `r` | Rename selected (prompts with the current name prefilled) |
| `n` | Create new file (prompts for a name) |
| `N` | Create new directory (prompts for a name) |
| `v` | Enter visual (multi-select) mode |
| `m` + key | Bookmark the current directory |
| `'` + key | Jump to a bookmark |
| `w` | Open the bookmarks popup |
| `/` | Live search mode |
| `:` | Command mode |
| `.` | Toggle hidden files |
| `t` | Cycle accent palette (Mocha, Macchiato, Tokyo Night, Rose Pine, Nord) |
| `s` | Cycle sort mode (name/size/ext/date) |
| `S` | Toggle disk-usage mode (recursive directory sizes) |
| `e` | Edit selected in `$EDITOR` |
| `o` | Open selected with the system opener |
| `` ` `` | Toggle plugin UI |
| `Esc` | Clear an active search |

### Visual Mode
| Key | Action |
|-----|--------|
| `j` / `k` | Extend/shrink the selection |
| `y` | Yank the selection |
| `x` | Cut the selection |
| `d` `d` | Delete the selection (press twice to confirm) |
| `v` / `Esc` | Exit visual mode |

### Search Mode
Press `/`, then type to filter files live.

| Key | Action |
|-----|--------|
| type | Filter entries as you type |
| `Enter` | Accept the search (stays filtered) |
| `n` / `N` | Next / previous match (back in normal mode) |
| `Esc` | Cancel and clear the search |

### Command Mode
Press `:`, then type. `↑`/`↓` walk the command history and `Tab` completes commands and file names (including nested paths, e.g. `cd src/ui<Tab>`).

| Command | Description |
|---------|-------------|
| `cd <path>` | Change directory (`~` supported) |
| `mkdir <name>` | Create directory |
| `touch <name>` | Create file |
| `rm <name>` | Remove file/directory permanently |
| `trash [name]` | Move to the OS trash (selected entry if no name) |
| `restore` | Restore the most recently trashed item |
| `rename <name>` | Rename the selected entry |
| `search <pattern>` | Search for files |
| `sort [name\|size\|ext\|date]` | Set the sort mode (no arg = cycle) |
| `theme [name]` | Set the theme (no arg = cycle) |
| `hidden` | Toggle hidden files |
| `sidebar` | Toggle the sidebar pane |
| `extract [dir]` | Extract the selected archive |
| `zip <name>` / `targz <name>` / `tar <name>` | Compress the selection into an archive |
| `! <cmd>` | Run a shell command in the current directory |
| `copy` / `cut` / `paste` | Clipboard operations |
| `open` | Open the selected file with the system opener |
| `q` / `quit` | Quit |
| `help` | Show commands |

Press `Enter` to execute, `Esc` to cancel.

## Installation

### Homebrew (recommended — prebuilt binary, installs in seconds)

```bash
brew tap abhskyd/tap
brew install finder
```

Or in one line: `brew install abhskyd/tap/finder`.

Apple Silicon gets a prebuilt binary (no toolchain needed). Intel macs and
Linux build from source with `cargo` (Rust is installed automatically as a
build dependency). Homebrew 7+ may ask you to trust the tap once — that's a
one-time confirmation (see [Tap Trust](https://docs.brew.sh/Tap-Trust)).

### One-line script (no Homebrew needed)

```bash
curl -fsSL https://raw.githubusercontent.com/abhskyd/finder/main/install.sh | sh
```

Downloads the prebuilt binary to `/opt/homebrew/bin` (or `~/.local/bin`) and
prints a PATH note if needed. macOS Apple Silicon only — other platforms
should use Homebrew (source build) or build manually.

### From source

```bash
git clone https://github.com/abhskyd/finder
cd finder
cargo build --release
```

The binary will be at `target/release/finder`.

### Default (archive previews + extract/compress)
```bash
cargo build --release
```

### Without the archive feature
```bash
cargo build --release --no-default-features
```

Features: `archive` (zip/tar/tar.gz contents listing, extraction and compression). Optional and pure Rust.

## Requirements

- Rust 1.70+
- A terminal with Unicode support
- **A Nerd Font** (e.g. JetBrainsMono Nerd Font) for the file-type icons

## Configuration

The app reads and writes `~/.config/finder/config.toml`:

```toml
# finder configuration
# theme: mocha | macchiato | tokyo night | rose pine | nord
# bookmarks: `<key> = "<path>"` — jump with ' + key

[general]
start_dir = "."
show_hidden = false
sort = "name"
sidebar = true

[theme]
name = "mocha"

[bookmarks]
p = "~/projects"
```

- `general`: `start_dir` (where the app opens), `show_hidden`, `sort` (name/size/ext/date), `sidebar` (the places/disks/bookmarks pane, auto-hidden on narrow terminals)
- `theme`: accent palette; the app never repaints the terminal's own background (transparent + blurred terminals keep showing the wallpaper — only the selection bar is painted)
- `bookmarks`: created with `m` + key, jumped to with `'` + key or the `w` popup; saved automatically

The app respects your terminal's own color scheme — backgrounds are never repainted (only the selection bar is), so transparent and blurred terminals keep showing your wallpaper. Soft pastel accent palettes (fg-only) are built in and cycled with `t` or set with `:theme <name>`: Mocha, Macchiato, Tokyo Night, Rose Pine, Nord.

## Development

```bash
cargo test          # unit tests (fuzzy scorer, git parsing, sort modes, …)
python3 scripts/smoke_test.py all   # end-to-end smoke tests in a pty
```

The smoke tests run the real binary in a pseudo-terminal, drive it with
keyscripts and assert on the rendered screen — popups, git badges, fuzzy
results, preview scrolling and more.

## Plugin Development

Plugins are dynamic libraries (.so/.dylib/.dll) placed in the `./plugins` directory. They must export a `plugin_entry` function that returns a pointer to a `Plugin` trait object.

See `src/plugin/api.rs` for the plugin API.

## License

MIT

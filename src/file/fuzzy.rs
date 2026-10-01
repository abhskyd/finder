//! Fuzzy finder: an fzf-style popup for jumping to any file under the
//! current directory.
//!
//! Candidates are gathered recursively with the `ignore` crate (so
//! `.gitignore`d and hidden files are skipped by default) and filtered with a
//! smart subsequence scorer: filename hits beat path hits, consecutive and
//! start-of-word matches score higher, exact matches win outright.

use std::path::{Path, PathBuf};

/// Cap on gathered candidates so huge trees don't stall the UI.
const MAX_CANDIDATES: usize = 10_000;
/// Cap per directory depth: a deep `target/` or `node_modules/` must not
/// crowd out shallow source files in BFS walk order.
const PER_DEPTH_CAP: usize = 1500;

/// One fuzzy-find candidate: a path plus its match score.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    /// Higher is better. Only set when the path matches the query.
    pub score: i32,
}

/// State of the fuzzy finder popup.
pub struct FuzzyFinder {
    /// The current query (typed as you go).
    pub query: String,
    /// All gathered candidates (score 0 until the query is applied).
    pub all: Vec<Candidate>,
    /// Candidates matching the current query, best first.
    pub matches: Vec<Candidate>,
    /// Selected index into `matches`.
    pub selected: usize,
    /// First visible match (scroll position of the results list).
    pub scroll: usize,
}

impl FuzzyFinder {
    /// Recursively gather candidate files under `start` and show them all.
    pub fn open(start: &Path) -> Self {
        let mut me = Self {
            query: String::new(),
            all: gather(start),
            matches: Vec::new(),
            selected: 0,
            scroll: 0,
        };
        me.set_query("");
        me
    }

    /// Append a character to the query and re-filter.
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        let q = self.query.clone();
        self.set_query(&q);
    }

    /// Remove the last query character and re-filter.
    pub fn pop_char(&mut self) {
        self.query.pop();
        let q = self.query.clone();
        self.set_query(&q);
    }

    /// Re-filter for the current query (as-you-type).
    pub fn set_query(&mut self, query: &str) {
        if query.is_empty() {
            self.matches = self.all.clone();
        } else {
            let mut scored: Vec<Candidate> = self
                .all
                .iter()
                .filter_map(|c| {
                    fuzzy_score(query, &c.path).map(|s| Candidate {
                        path: c.path.clone(),
                        score: s,
                    })
                })
                .collect();
            scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
            self.matches = scored;
        }
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.matches.len() {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Keep the popup's scroll offset so the selection stays visible.
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

    /// The currently selected candidate, if any.
    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.matches.get(self.selected).map(|c| &c.path)
    }
}

/// Gather files under `start` recursively, skipping hidden files and
/// `.gitignore`d paths (the `ignore` crate's defaults).
///
/// Entries are sorted by path (so `src/` is walked before `target/`) and
/// each depth is capped, so a huge deep tree can't crowd out shallow
/// source files in the candidate list.
fn gather(start: &Path) -> Vec<Candidate> {
    let walker = ignore::WalkBuilder::new(start)
        .follow_links(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .build();
    let mut out = Vec::new();
    let mut depth_count = 0usize;
    let mut last_depth = 0usize;
    for entry in walker.flatten() {
        if out.len() >= MAX_CANDIDATES {
            break;
        }
        let depth = entry.depth();
        if depth != last_depth {
            depth_count = 0;
            last_depth = depth;
        }
        depth_count += 1;
        if depth_count > PER_DEPTH_CAP {
            continue;
        }
        let path = entry.path();
        // Only files: directories are navigated, not jumped to.
        if entry.file_type().is_none_or(|t| t.is_dir()) {
            continue;
        }
        out.push(Candidate {
            path: path.to_path_buf(),
            score: 0,
        });
    }
    out
}

/// Score how well `query` matches the candidate path. Returns None if the
/// query isn't a (case-insensitive) subsequence of the path.
pub fn fuzzy_score(query: &str, path: &Path) -> Option<i32> {
    let full = path.to_str()?.to_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase())
        .unwrap_or_default();
    let q = query.to_lowercase();

    // Exact matches win outright.
    if name == q {
        return Some(10_000);
    }
    if name.starts_with(&q) {
        return Some(5_000 - full.len() as i32);
    }

    // The query must be a subsequence of the filename, or of the full path
    // (so things like `src/app` find src/app.rs).
    let name_score = subsequence_score(&q, &name);
    let path_score = subsequence_score(&q, &full);

    match name_score {
        // Filename hits beat path hits; prefer shorter paths.
        Some(ns) => {
            let ps = path_score.unwrap_or(0);
            Some(ns * 4 + ps - full.len() as i32 / 4)
        }
        // Path-only hit: rank by score, penalizing longer paths.
        None => path_score.map(|ps| ps - full.len() as i32 / 2),
    }
}

/// Score a case-insensitive subsequence match. Higher is better.
///
/// Consecutive matches and matches at word starts (after `/ _ - .  ` or the
/// start of the string) score extra; each gap costs a little.
fn subsequence_score(query: &str, text: &str) -> Option<i32> {
    let mut score = 0;
    let mut ti = text.char_indices();
    let mut prev: Option<char> = None;
    let mut last_hit: Option<usize> = None;

    for qc in query.chars() {
        loop {
            let (i, c) = ti.next()?;
            if c == qc {
                // Consecutive match bonus.
                if last_hit.is_some_and(|l| i == l + 1) {
                    score += 8;
                }
                // Word-start bonus: at the start or after a separator.
                let word_start = prev.is_none_or(|p| {
                    matches!(p, '/' | '_' | '-' | '.' | ' ' | '(')
                });
                if word_start {
                    score += 12;
                }
                score += 2;
                last_hit = Some(i);
                prev = Some(c);
                break;
            }
            score -= 1;
            prev = Some(c);
        }
    }
    Some(score)
}

/// Char-index positions of the query characters in the path's file name,
/// greedily matched (case-insensitive). Used to highlight matched characters
/// in the fuzzy finder results. Returns None if the name doesn't contain the
/// query as a subsequence.
pub fn match_positions(query: &str, path: &Path) -> Option<Vec<usize>> {
    let name = path
        .file_name()?
        .to_str()?
        .to_lowercase();
    let q = query.to_lowercase();
    let mut positions = Vec::new();
    let mut chars = name.chars().enumerate();
    for qc in q.chars() {
        loop {
            let (i, c) = chars.next()?;
            if c == qc {
                positions.push(i);
                break;
            }
        }
    }
    Some(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    #[test]
    fn subsequence_requires_all_chars_in_order() {
        assert!(subsequence_score("abc", "axbxc").is_some());
        assert!(subsequence_score("ac", "abc").is_some());
        assert!(subsequence_score("ab", "bca").is_none());
        assert!(subsequence_score("zz", "file").is_none());
    }

    #[test]
    fn scoring_prefers_consecutive_and_word_starts() {
        // "read" — the consecutive, word-start match in "readme.md" beats
        // the scattered letters elsewhere.
        let scattered = subsequence_score("read", "rxe.mx_da_nd").unwrap();
        let word_start = subsequence_score("read", "readme.md").unwrap();
        assert!(word_start > scattered);
    }

    #[test]
    fn exact_filename_wins_over_anything_else() {
        let exact = fuzzy_score("main.rs", &p("src/ui/app/main.rs")).unwrap();
        let partial = fuzzy_score("main.rs", &p("src/main.rs.bak")).unwrap();
        assert!(exact > partial);
    }

    #[test]
    fn shorter_paths_score_higher() {
        let a = fuzzy_score("app", &p("src/app.rs")).unwrap();
        let b = fuzzy_score("app", &p("a/b/c/very/deep/nested/app.rs")).unwrap();
        assert!(a > b);
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("APP", &p("src/app.rs")).is_some());
        assert!(fuzzy_score("app", &p("src/APP.rs")).is_some());
    }

    #[test]
    fn match_positions_aligns_with_display_name() {
        let pos = match_positions("ap", &p("src/app.rs")).unwrap();
        assert_eq!(pos, vec![0, 1]);
        // No match → None.
        assert!(match_positions("zz", &p("src/app.rs")).is_none());
    }
}

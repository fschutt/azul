//! The command palette (Cmd+K) as plain data: what it lists for what was typed. With nothing
//! typed, the recent searches, the places visited last and a few commands; with a query, the
//! folders whose name matches, then the files, then the commands whose words match, and last
//! "Search all drives" for the query. A name matches as the search's names do (`rank`): every
//! term in it, case and accents folded. The arrows move the hot row, Return runs it.
//!
//! The rows carry what running them does as `T` (the app's action); no window types here.

use crate::rank;

/// The recent searches and the places an empty palette offers, at most.
pub const RECENT: usize = 4;
/// The commands an empty palette offers, at most.
pub const COMMANDS_AT_REST: usize = 8;
/// The folders, files and commands a query lists, at most.
pub const FOLDERS: usize = 4;
pub const FILES: usize = 7;
pub const COMMANDS: usize = 5;

/// The group a row is in; the palette shows each group under its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    RecentSearches,
    RecentPlaces,
    Folders,
    Files,
    Commands,
    /// "Search all drives" for the query.
    SearchAll,
}

/// One row: its group, its words, where it is (a place's folders, a command's keys), and what
/// running it does.
#[derive(Debug, Clone, PartialEq)]
pub struct Row<T> {
    pub section: Section,
    pub label: String,
    pub detail: String,
    pub target: T,
}

/// What the palette can list: each candidate's words, its detail and its target.
pub struct Candidates<'a, T> {
    pub recent_searches: &'a [(String, T)],
    pub recent_places: &'a [(String, String, T)],
    pub folders: &'a [(String, String, T)],
    pub files: &'a [(String, String, T)],
    pub commands: &'a [(String, String, T)],
}

/// The rows for `query` from `candidates`; `search_all` makes the target of "Search all drives"
/// for the query.
#[must_use]
pub fn rows<T: Clone>(
    query: &str,
    candidates: &Candidates<'_, T>,
    search_all: impl Fn(&str) -> T,
) -> Vec<Row<T>> {
    let row = |section, label: &str, detail: &str, target: &T| Row {
        section,
        label: label.to_string(),
        detail: detail.to_string(),
        target: target.clone(),
    };
    let mut out = Vec::new();
    let query = query.trim();
    if rank::tokens(query).is_empty() {
        out.extend(
            candidates
                .recent_searches
                .iter()
                .take(RECENT)
                .map(|(text, target)| row(Section::RecentSearches, text, "", target)),
        );
        out.extend(
            candidates
                .recent_places
                .iter()
                .take(RECENT)
                .map(|(label, detail, target)| row(Section::RecentPlaces, label, detail, target)),
        );
        out.extend(
            candidates
                .commands
                .iter()
                .take(COMMANDS_AT_REST)
                .map(|(label, detail, target)| row(Section::Commands, label, detail, target)),
        );
        return out;
    }
    let matching = |list: &[(String, String, T)], section, limit| -> Vec<Row<T>> {
        list.iter()
            .filter(|(label, _, _)| rank::name_matches(label, query))
            .take(limit)
            .map(|(label, detail, target)| row(section, label, detail, target))
            .collect()
    };
    out.extend(matching(candidates.folders, Section::Folders, FOLDERS));
    out.extend(matching(candidates.files, Section::Files, FILES));
    out.extend(matching(candidates.commands, Section::Commands, COMMANDS));
    out.push(Row {
        section: Section::SearchAll,
        label: query.to_string(),
        detail: String::new(),
        target: search_all(query),
    });
    out
}

/// The hot row after an arrow (`delta` -1 or 1) among `count` rows: it holds at the ends.
#[must_use]
pub fn step(hot: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let last = count as isize - 1;
    (hot.min(count) as isize + delta).clamp(0, last) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(names: &[&str]) -> Vec<(String, String, usize)> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.to_string(), String::from("Home"), i))
            .collect()
    }

    fn sections(rows: &[Row<usize>]) -> Vec<Section> {
        rows.iter().map(|row| row.section).collect()
    }

    #[test]
    fn an_empty_query_offers_recent_searches_recent_places_and_commands() {
        let searches: Vec<(String, usize)> = ["Steuer", "Gardasee", "Rechnung", "Miete", "Urlaub"]
            .iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i))
            .collect();
        let places = named(&["Documents", "Pictures"]);
        let commands = named(&[
            "New folder",
            "Copy",
            "Paste",
            "Rename",
            "Delete",
            "Select all",
            "Preview pane",
            "Navigation pane",
            "Properties",
            "Refresh",
        ]);
        let candidates = Candidates {
            recent_searches: &searches,
            recent_places: &places,
            folders: &named(&["Steuer"]),
            files: &named(&["Steuer.pdf"]),
            commands: &commands,
        };
        let rows = rows("  ", &candidates, |_| 99);
        let mut want = vec![Section::RecentSearches; RECENT];
        want.extend([Section::RecentPlaces; 2]);
        want.extend([Section::Commands; COMMANDS_AT_REST]);
        assert_eq!(sections(&rows), want);
        assert_eq!(rows[0].label, "Steuer");
    }

    #[test]
    fn a_query_lists_folders_then_files_then_commands_then_a_search_of_every_drive() {
        let folders = named(&[
            "Steuer 2024",
            "Steuer 2025",
            "Urlaub",
            "Steuerbelege",
            "Steuer alt",
            "Steuerberater",
        ]);
        let files = named(&["Einkommensteuer.pdf", "notes.txt", "Steuer.xlsx"]);
        let commands = named(&["Steuer erklären", "Copy"]);
        let candidates = Candidates {
            recent_searches: &[],
            recent_places: &[],
            folders: &folders,
            files: &files,
            commands: &commands,
        };
        let rows = rows("steuer", &candidates, |q| q.len() + 1000);
        let mut want = vec![Section::Folders; FOLDERS];
        want.extend([Section::Files; 2]);
        want.push(Section::Commands);
        want.push(Section::SearchAll);
        assert_eq!(sections(&rows), want);
        assert_eq!(rows[0].label, "Steuer 2024", "in the order handed in");
        assert!(rows
            .iter()
            .all(|row| row.label != "Urlaub" && row.label != "notes.txt"));
        let last = rows.last().expect("a row");
        assert_eq!(last.target, 1006, "the search of every drive for the query");
        assert!(last.label.contains("steuer"));
    }

    #[test]
    fn the_hot_row_moves_with_the_arrows_and_holds_at_the_ends() {
        assert_eq!(step(0, 1, 3), 1);
        assert_eq!(step(2, 1, 3), 2);
        assert_eq!(step(0, -1, 3), 0);
        assert_eq!(step(5, -1, 3), 2, "a hot row past the end comes back");
        assert_eq!(step(0, 1, 0), 0);
    }
}

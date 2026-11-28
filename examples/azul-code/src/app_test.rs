use super::*;
use crate::search::Hit;

fn hit(line: usize) -> Hit {
    Hit {
        line,
        start: 0,
        end: 6,
        preview: "picked".to_string(),
        preview_start: 0,
        preview_end: 6,
    }
}

#[test]
fn the_search_results_list_each_file_then_its_matches_unless_folded() {
    crate::l10n::in_english();
    let mut s = FolderSearch {
        query: "picked".to_string(),
        complete: true,
        ..FolderSearch::default()
    };
    assert_eq!(s.summary(), "No results found.");
    s.results = vec![
        FileHits {
            key: "Cargo.toml".to_string(),
            hits: vec![hit(1)],
        },
        FileHits {
            key: "src/main.rs".to_string(),
            hits: vec![hit(0), hit(4)],
        },
    ];
    assert_eq!(
        s.rows(),
        vec![
            ResultRow::File(0),
            ResultRow::Hit(0, 0),
            ResultRow::File(1),
            ResultRow::Hit(1, 0),
            ResultRow::Hit(1, 1)
        ]
    );
    assert_eq!(s.summary(), "3 results in 2 files");
    s.folded.insert("src/main.rs".to_string());
    assert_eq!(s.rows().len(), 3, "a folded file shows no matches");
    s.complete = false;
    assert_eq!(s.summary(), "3 results in 2 files (the first ones)");
    s.query.clear();
    assert_eq!(s.summary(), "", "no query, no summary");
}

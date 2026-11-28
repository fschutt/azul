use super::*;

fn folder(shown: usize) -> Facts {
    Facts {
        place: Where::Folder,
        loading: false,
        done: true,
        shown,
        selected: 0,
        selected_bytes: None,
        drives: 3,
        pins: 2,
        search: None,
        clipboard: 0,
        transfers: String::new(),
    }
}

fn said(f: &Facts) -> String {
    crate::l10n::in_english();
    line_of(f)
}

/// A folder at rest says how many items it holds, and while it is read, that it is.
#[test]
fn a_folder_says_its_items_and_while_it_loads_that_it_does() {
    assert_eq!(said(&folder(12)), "12 items");
    assert_eq!(said(&folder(1)), "1 item");
    assert_eq!(said(&folder(0)), "0 items");
    let reading = Facts {
        done: false,
        ..folder(40)
    };
    assert_eq!(said(&reading), "40 items so far");
    let empty = Facts {
        loading: true,
        done: false,
        ..folder(0)
    };
    assert_eq!(said(&empty), "Loading\u{2026}");
}

/// A selection says how many of the shown items it is, and the size of its files.
#[test]
fn a_selection_says_its_count_and_its_size() {
    let one = Facts {
        selected: 1,
        selected_bytes: Some(2048),
        ..folder(12)
    };
    let line = said(&one);
    assert!(line.starts_with("12 items  \u{00B7}  1 of 12 selected  \u{00B7}  "), "{line}");
    let size = line.rsplit("  \u{00B7}  ").next().unwrap_or_default();
    assert!(size.contains('2') && !size.contains("selected"), "the size of the file: {line}");
    let folders_only = Facts {
        selected: 2,
        selected_bytes: None,
        ..folder(12)
    };
    assert_eq!(said(&folders_only), "12 items  \u{00B7}  2 of 12 selected");
    let empty_files = Facts {
        selected: 1,
        selected_bytes: Some(0),
        ..folder(12)
    };
    assert_eq!(said(&empty_files), "12 items  \u{00B7}  1 of 12 selected", "no size for an empty file");
}

/// The overviews count what they list; the clipboard and the transfer join the line.
#[test]
fn the_overviews_the_clipboard_and_a_transfer_have_their_words() {
    let pc = Facts {
        place: Where::ThisPc,
        ..folder(0)
    };
    assert_eq!(said(&pc), "3 drives");
    let quick = Facts {
        place: Where::QuickAccess,
        ..folder(0)
    };
    assert_eq!(said(&quick), "2 pinned folders");
    let busy = Facts {
        clipboard: 4,
        transfers: String::from("Copying 3 items - 1 of 3 (25%)"),
        ..folder(5)
    };
    assert_eq!(
        said(&busy),
        "5 items  \u{00B7}  4 on the clipboard  \u{00B7}  Copying 3 items - 1 of 3 (25%)"
    );
}

/// While a search is open its own words replace the folder's count.
#[test]
fn an_open_search_says_how_far_it_got() {
    let searching = Facts {
        search: Some(String::from("Searching\u{2026} 7 found")),
        ..folder(500)
    };
    assert_eq!(said(&searching), "Searching\u{2026} 7 found");
}

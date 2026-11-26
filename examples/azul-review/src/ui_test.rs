//! The empty page and the window around it, built without a window.

use std::path::PathBuf;

use azul::{dom::VirtualKeyCode, menu::{MenuItem, StringMenuItem}, prelude::*, str::String as AzString};

use super::*;

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

fn has_id(dom: &Dom, id: &str) -> bool {
    let id = AzString::from(id);
    nodes(dom).into_iter().any(|n| n.root.has_id(id.clone()))
}

fn empty(root: &str) -> AppState {
    crate::l10n::in_english();
    AppState::create(PathBuf::from(root), Vec::new(), RefAny::new(0u8), PathBuf::new())
}

#[test]
fn with_no_folder_the_empty_page_says_to_choose_one_and_has_the_button() {
    let page = sheet(&empty(""), &RefAny::new(0u8));
    let said = texts(&page).join(" ");
    assert!(said.contains("Choose a folder"), "{said}");
    assert!(!said.contains("No reviewable files under"), "no folder to look under: {said}");
    assert!(has_id(&page, "__azreview_open-folder-empty"), "the way out is in the empty page");
}

#[test]
fn a_folder_without_reviewable_files_is_named_in_the_empty_page() {
    let page = sheet(&empty("/work/empty"), &RefAny::new(0u8));
    let said = texts(&page).join(" ");
    assert!(said.contains("No reviewable files under /work/empty"), "{said}");
    assert!(has_id(&page, "__azreview_open-folder-empty"));
}

// ---- the title row: the way to a folder, and which folder it is ----

fn reviewing(root: &str) -> AppState {
    crate::l10n::in_english();
    let file = crate::code::SourceFile {
        path: PathBuf::from(root).join("src/lib.rs"),
        display: "src/lib.rs".to_string(),
        lines: vec!["fn a() {}".to_string()],
        big: None,
    };
    AppState::create(PathBuf::from(root), vec![file], RefAny::new(0u8), PathBuf::new())
}

#[test]
fn the_title_row_has_a_button_that_says_open_folder() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert!(has_id(&row, "__azreview_open-folder-bar"), "the button is in the bar the pen and the pages are in");
    assert!(
        texts(&row).iter().any(|t| t.contains("Open folder")),
        "it says what it does, not only a folder icon: {:?}",
        texts(&row)
    );
}

#[test]
fn the_title_row_names_the_folder_and_the_file_under_review() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert!(has_id(&row, "__azreview_folder-chip"));
    let said = texts(&row);
    assert!(said.iter().any(|t| t == "proj"), "the folder's own name: {said:?}");
    assert!(said.iter().any(|t| t.contains("src/lib.rs")), "the file open: {said:?}");
}

#[test]
fn with_no_folder_the_title_row_names_none_but_still_offers_to_open_one() {
    let row = top_bar(&empty(""), &RefAny::new(0u8), 0.0);
    assert!(!has_id(&row, "__azreview_folder-chip"));
    assert!(has_id(&row, "__azreview_open-folder-bar"));
}

#[test]
fn a_folder_is_named_by_its_own_name_and_its_tooltip_is_the_whole_path() {
    assert_eq!(
        folder_names(std::path::Path::new("/work/proj")),
        Some(("proj".to_string(), "/work/proj".to_string()))
    );
    assert_eq!(folder_names(std::path::Path::new("")), None, "no folder");
    assert_eq!(
        folder_names(std::path::Path::new("/")),
        Some(("/".to_string(), "/".to_string())),
        "a root has no name of its own: it is its own name"
    );
}

#[test]
fn the_session_menu_opens_a_folder_with_the_primary_modifier_and_o() {
    crate::l10n::in_english();
    let menu = menu_bar(&RefAny::new(0u8));
    let item = find_item(menu.items.as_ref(), "Open folder...").expect("Session > Open folder...");
    let OptionVirtualKeyCodeCombo::Some(combo) = &item.accelerator else {
        panic!("the item shows its key");
    };
    let keys = combo.keys.as_ref().to_vec();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(matches!(keys[1], VirtualKeyCode::O), "{keys:?}");
    let primary = if cfg!(target_os = "macos") { VirtualKeyCode::LWin } else { VirtualKeyCode::LControl };
    assert_eq!(format!("{:?}", keys[0]), format!("{primary:?}"), "Cmd on a Mac, Ctrl elsewhere");
}

fn find_item<'a>(items: &'a [MenuItem], label: &str) -> Option<&'a StringMenuItem> {
    for item in items {
        if let MenuItem::String(s) = item {
            if s.label.as_str() == label {
                return Some(s);
            }
            if let Some(found) = find_item(s.children.as_ref(), label) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn the_empty_page_explains_what_the_app_does_and_has_a_large_open_folder_button() {
    let page = sheet(&empty(""), &RefAny::new(0u8));
    let said = texts(&page);
    assert!(said.iter().any(|t| t.contains("pages")), "a word about what AzReview shows: {said:?}");
    assert!(said.iter().any(|t| t.contains("Open folder")), "the button says so: {said:?}");
    assert!(has_id(&page, "__azreview_open-folder-empty"));
}

// ---- the pens in the top bar ----

fn pen_keys(dom: &Dom) -> Vec<&Dom> {
    nodes(dom)
        .into_iter()
        .filter(|n| {
            n.root.callbacks.as_slice().iter().any(|c| c.callback.cb == crate::on_pick_semantic as usize)
        })
        .collect()
}

#[test]
fn the_top_bar_has_one_key_per_pen_and_no_button_for_the_marker_or_the_comments() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    assert_eq!(pen_keys(&row).len(), crate::pens::PENS.len(), "a key for each pen");
    let said = texts(&row);
    assert!(!said.iter().any(|t| t == "marker" || t == "pen" || t == "audio pen"), "no nib button: {said:?}");
    assert!(!has_id(&row, "__azreview_notes-toggle"), "no comment button");
}

#[test]
fn a_pen_key_takes_the_pen_on_a_click_and_the_next_nib_on_a_double_click() {
    let row = top_bar(&reviewing("/work/proj"), &RefAny::new(0u8), 0.0);
    for key in pen_keys(&row) {
        let handlers: Vec<(EventFilter, usize)> =
            key.root.callbacks.as_slice().iter().map(|c| (c.event, c.callback.cb)).collect();
        assert!(
            handlers.contains(&(EventFilter::Hover(HoverEventFilter::Click), crate::on_pick_semantic as usize)),
            "a click takes the pen: {handlers:?}"
        );
        assert!(
            handlers.contains(&(
                EventFilter::Hover(HoverEventFilter::DoubleClick),
                crate::on_pen_double_click as usize
            )),
            "a double click cycles its nib: {handlers:?}"
        );
    }
}

#[test]
fn only_the_pen_in_hand_shows_its_nib_and_every_key_is_named_with_its_meaning_and_nib() {
    let mut s = reviewing("/work/proj");
    s.pens.double_click(2);
    let row = top_bar(&s, &RefAny::new(0u8), 0.0);
    let keys = pen_keys(&row);
    // a pen's key is its meaning's icon and its digit; the pen in hand has the icon of its nib besides
    let parts: Vec<usize> = keys.iter().map(|k| k.children.as_slice().len()).collect();
    assert_eq!(parts, vec![2, 2, 3, 2, 2], "only the question pen, in hand, shows its nib");
    for key in keys {
        assert!(!key.root.accessibility.is_null(), "a pen key without a name");
    }
}

// ---- the code column the whiteboard's cutouts share with the page ----

#[test]
fn the_code_column_is_a_row_per_line_numbered_from_the_pages_first_line() {
    let lines = vec!["fn a() {}".to_string(), "fn b() {}".to_string(), "fn c() {}".to_string()];
    let col = page_code_column(41, &lines);
    assert_eq!(col.children.as_slice().len(), 3, "a row for each line");
    let said = texts(&col);
    assert_eq!(said, vec!["41", "fn a() {}", "42", "fn b() {}", "43", "fn c() {}"], "the number, then the line");
}

#[test]
fn the_numbers_the_cutout_renderer_lays_out_by_are_the_pages() {
    // One place for the page's measures: a cutout is as wide and as high per line as the page it was taken from.
    assert_eq!(LINE_H, 15.0);
    assert_eq!(PAGE_W, 1000.0);
    assert_eq!(GUTTER_W, 52.0);
    assert_eq!(PAGE_PAD, 32.0);
    assert_eq!(CODE_W, PAGE_W - MARGIN_GUTTER_W - PAGE_PAD);
}

// ---- the words a slash took ----

#[test]
fn a_line_with_hidden_words_is_the_same_rows_with_those_words_in_the_colour_of_the_page() {
    let lines = vec!["    abcdefghij klm".to_string(), "fn b() {}".to_string()];
    let plain = page_code_column(7, &lines);
    let cut = page_code_column_hidden(7, &lines, &[vec![(4, 14)], vec![]]);
    assert_eq!(cut.children.as_slice().len(), plain.children.as_slice().len(), "a row for each line, still");
    assert_eq!(
        texts(&cut.children.as_slice()[0]),
        vec!["7", "abcdefghij", " klm"],
        "the number and the pieces of the line (the blank piece is no word)"
    );
    assert_eq!(
        texts(&cut.children.as_slice()[1]),
        texts(&plain.children.as_slice()[1]),
        "a line with nothing hidden is as it was"
    );
    // the hidden piece has the colour of the page, the others the ink of the code
    assert!(piece_css(true).contains(&format!("color: {PAGE_BG};")), "{}", piece_css(true));
    assert!(piece_css(false).contains("color: #1f1f1f;"), "{}", piece_css(false));
    assert!(piece_css(true).contains("white-space: pre;") && piece_css(false).contains("white-space: pre;"));
}

#[test]
fn a_page_with_no_hidden_words_is_built_as_it_always_was() {
    let lines = vec!["fn a() {}".to_string(), "fn b() {}".to_string()];
    let none = vec![Vec::new(); 2];
    let a = page_code_column(1, &lines);
    let b = page_code_column_hidden(1, &lines, &none);
    assert_eq!(texts(&a), texts(&b));
    assert_eq!(nodes(&a).len(), nodes(&b).len(), "one piece to a line: not a node more");
}

#[test]
fn the_measures_a_word_is_found_by_are_the_pages() {
    // the code of a page starts under its padding and the gutter; a column is as wide as the page's font is
    assert_eq!(CODE_X, PAGE_PAD + GUTTER_W);
    assert!((CHAR_W - 6.6).abs() < f32::EPSILON, "the monospace 11 px of the page: 6.6 px a column");
}

// ---- the bottom bar of the agents ----

fn with_chips(files: usize, count: usize) -> AppState {
    let mut s = reviewing("/work/proj");
    let _ = files;
    let names: Vec<String> = s.tree.file_paths(100);
    s.chips = crate::chips::mock(count, &names);
    s
}

fn chip_keys(dom: &Dom) -> Vec<&Dom> {
    nodes(dom)
        .into_iter()
        .filter(|n| n.root.callbacks.as_slice().iter().any(|c| c.callback.cb == crate::on_chip_click as usize))
        .collect()
}

#[test]
fn with_no_conversation_the_bar_says_no_agent_is_working() {
    let s = with_chips(1, 0);
    let bar = bottom_bar(&s, &RefAny::new(0u8));
    assert!(texts(&bar).iter().any(|t| t.contains("No agents are working")), "{:?}", texts(&bar));
    assert!(chip_keys(&bar).is_empty());
}

#[test]
fn the_bar_has_a_chip_per_conversation_of_the_open_file_each_named_by_its_words_and_its_state() {
    let s = with_chips(1, 12);
    let bar = bottom_bar(&s, &RefAny::new(0u8));
    let keys = chip_keys(&bar);
    assert_eq!(keys.len(), 12, "the folder's only file holds all twelve");
    for key in keys {
        assert!(!key.root.accessibility.is_null(), "a chip without a name");
    }
    assert!(!texts(&bar).iter().any(|t| t.contains("No agents are working")));
}

#[test]
fn a_file_is_marked_in_the_tree_by_the_state_of_its_conversations() {
    let row_of = |s: &AppState| {
        let row = s.rows.iter().find(|r| r.name == "lib.rs").expect("the file's row").clone();
        tree_row(s, &RefAny::new(0u8), &row)
    };
    let s = with_chips(1, 3);
    assert!(has_id(&row_of(&s), "__azreview_file-mark-working"), "an agent is at work on the file");
    let none = reviewing("/work/proj");
    assert!(!has_id(&row_of(&none), "__azreview_file-mark-working"), "no conversation, no mark");
}

// ---- what the window as a whole listens to ----

fn window_events(dom: &Dom, handler: usize) -> Vec<EventFilter> {
    dom.root.callbacks.as_slice().iter().filter(|c| c.callback.cb == handler).map(|c| c.event).collect()
}

#[test]
fn a_stroke_ends_when_the_window_loses_the_focus_not_when_an_element_does() {
    // `FocusLost` of a window listener is the BLUR of the element that had the focus: a press on the page blurs the file
    // that was picked in the tree, and ended the stroke at its first point. The window losing the focus is
    // `WindowFocusLost`.
    let body = with_window_listeners(Dom::create_body(), &RefAny::new(0u8));
    let events = window_events(&body, crate::on_focus_lost as usize);
    assert_eq!(events, vec![EventFilter::Window(WindowEventFilter::WindowFocusLost)], "{events:?}");
}

#[test]
fn the_window_hears_the_keys_the_pointer_leaving_and_the_button_let_go_anywhere() {
    let body = with_window_listeners(Dom::create_body(), &RefAny::new(0u8));
    assert_eq!(
        window_events(&body, crate::on_key as usize),
        vec![EventFilter::Window(WindowEventFilter::VirtualKeyDown)]
    );
    assert_eq!(
        window_events(&body, crate::on_window_up as usize),
        vec![EventFilter::Window(WindowEventFilter::LeftMouseUp)]
    );
    assert_eq!(
        window_events(&body, crate::on_window_left as usize),
        vec![EventFilter::Window(WindowEventFilter::MouseLeave)]
    );
}

#[test]
fn the_edge_of_a_panel_is_named_for_the_screen_reader() {
    // Its role is a grip (azul's lint refuses a named node of an unknown role: the e2e reads its output).
    let s = reviewing("/work/proj");
    let view = main_view(&s, &RefAny::new(0u8), 1280.0);
    let grip = nodes(&view)
        .into_iter()
        .find(|n| n.root.has_id(crate::ids::TREE_GRIP.clone()))
        .expect("the tree's grip");
    assert!(!grip.root.accessibility.is_null(), "named");
}

// ---- the places other modules plug into ----

#[test]
fn the_right_panel_holds_the_placeholder_until_the_whiteboard_is_mounted() {
    let content = right_panel_content(&reviewing("/work/proj"), &RefAny::new(0u8));
    assert!(texts(&content).iter().any(|t| t.contains("whiteboard")), "{:?}", texts(&content));
}

#[test]
fn the_presenter_and_the_picture_in_picture_have_their_slots_and_nothing_is_in_them_yet() {
    let s = reviewing("/work/proj");
    assert!(presenter_slot(&s, &RefAny::new(0u8)).is_none(), "the presenter is not mounted");
    assert!(overlay_slot(&s, &RefAny::new(0u8)).is_none(), "no picture in picture");
}

#[test]
fn the_settings_page_has_the_dictation_section_and_room_for_the_others() {
    let s = reviewing("/work/proj");
    let sections = settings_sections(&s, &RefAny::new(0u8));
    assert_eq!(sections.len(), 1, "the dictation's; the agents', recorder's and speech panels come after it");
}

// ---- a page of long lines, a file of many pages ----

#[test]
fn a_line_is_shown_as_far_as_the_page_reaches_and_no_further() {
    let long = "x".repeat(100_000);
    let col = page_code_column(1, &[long.clone(), "short".to_string()]);
    let rows = col.children.as_slice();
    let shown = texts(&rows[0]);
    assert_eq!(shown[0], "1");
    assert_eq!(shown[1].chars().count(), CODE_COLUMNS, "not a hundred thousand characters to shape");
    assert_eq!(texts(&rows[1]), vec!["2", "short"]);
    assert!(CODE_COLUMNS >= 100 && CODE_COLUMNS <= 130, "about the columns the page has: {CODE_COLUMNS}");
    // a line with multi-byte characters is cut at a character
    let wide = "\u{2192}".repeat(1_000);
    assert_eq!(texts(&page_code_column(1, &[wide]).children.as_slice()[0])[1].chars().count(), CODE_COLUMNS);
}

#[test]
fn the_strip_of_page_numbers_of_a_file_of_thousands_of_pages_is_a_couple_of_dozen_keys() {
    let mut s = reviewing("/work/proj");
    s.open = Some(crate::code::SourceFile::big(
        PathBuf::from("big.rs"),
        "big.rs".to_string(),
        crate::code::BigText::new((0..46 * 15_000).flat_map(|i| format!("l{i}\n").into_bytes()).collect()),
    ));
    s.visible_page = 7_000;
    let strip = page_strip(&s, &RefAny::new(0u8));
    let keys = strip.children.as_slice().len();
    assert!(keys <= 25, "{keys} keys");
    let said = texts(&strip);
    assert!(said.contains(&"1".to_string()) && said.contains(&"15000".to_string()) && said.contains(&"7001".to_string()), "{said:?}");
    assert!(said.iter().any(|t| t == "\u{2026}"), "a way to the pages between");
}

// ---- how much is reviewed ----

fn progressing() -> AppState {
    crate::l10n::in_english();
    let a: String = (0..100).map(|i| format!("fn a{i}() {{}}\n")).collect();
    let folder = crate::lib_test::scratch_folder("ui-progress", &[("a.rs", a.as_str()), ("src/b.rs", "fn b() {}\n")]);
    AppState::on_disk(folder, RefAny::new(0u8), PathBuf::new())
}

#[test]
fn numbers_are_grouped_by_thousands() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1_000), "1,000");
    assert_eq!(grouped(12_345), "12,345");
    assert_eq!(grouped(3_100_000), "3,100,000");
}

#[test]
fn the_foot_of_the_tree_says_how_much_of_the_workspace_is_reviewed_and_that_some_files_are_not_counted_yet() {
    let mut s = progressing();
    for _ in 0..4 {
        s.watch_pages(0.0, 1_200.0, 400);
    }
    s.refresh_rollup();
    let line = reviewed_line(&s).expect("a workspace with files");
    let said = texts(&line).join(" ");
    assert!(said.contains("0 of 100 lines reviewed (0%)") && said.contains("46 seen"), "{said}");
    assert!(said.contains("Still counting"), "src is not listed yet: {said}");
    assert!(has_id(&line, "__azreview_reviewed"));
    assert!(reviewed_line(&AppState::create(PathBuf::new(), Vec::new(), RefAny::new(0u8), PathBuf::new())).is_none(), "no files, no sentence");
}

#[test]
fn a_folder_row_says_the_part_of_its_lines_that_is_reviewed_and_a_file_row_says_nothing_of_the_kind() {
    let mut s = progressing();
    let src = s.tree.find("src").expect("src");
    assert_eq!(folder_trail(&s, src), None, "its files are not counted: no lines to take a part of");
    let list = s.source.list("src").expect("listed");
    s.take_listing(s.workspace, "src", Ok(list));
    let b = s.tree.find("src/b.rs").expect("b.rs");
    s.tree.set_lines(b, 10);
    s.refresh_rollup();
    assert_eq!(folder_trail(&s, src), Some("0%".to_string()));
    let row = s.rows.iter().find(|r| r.name == "src").expect("the folder's row").clone();
    assert!(texts(&tree_row(&s, &RefAny::new(0u8), &row)).contains(&"0%".to_string()));
    let file = s.rows.iter().find(|r| r.name == "a.rs").expect("the file's row").clone();
    assert!(!texts(&tree_row(&s, &RefAny::new(0u8), &file)).iter().any(|t| t.ends_with('%')));
}

#[test]
fn the_key_that_holds_a_file_back_is_among_the_shortcuts() {
    assert!(crate::SHORTCUTS.iter().any(|s| s.keys == "Mod+Shift+R"), "the shortcuts page lists it");
}

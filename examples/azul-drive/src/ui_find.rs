//! The search view (the design's search behaviour B): from the first key typed into the search
//! box until Escape on an empty box, the leaf shows the search instead of the folder.
//!
//! - Its bar: the scope chips (This folder, This drive, All drives), the refine chips (Kind,
//!   Date modified, Size: each a menu, a chosen one shows its choice) and File contents, then
//!   how many were found and Save search (only with text in the box).
//! - With an empty box: "Type to search" and the recent and saved searches as chips.
//! - The results, ranked (`rank`) and grouped by their folder under a header ("Home › Documents
//!   · 2 items"), the folder of the best match first; each row its icon and name with the terms
//!   marked, two lines of the text its contents matched on (the terms marked), its date and size,
//!   and where something syncs its sync state. A click on a column's header sorts them flat (a
//!   click on Match puts them back as ranked). F2 renames a result in place; every row takes the
//!   clicks, the menu and the drag of a folder's item.
//! - Nothing found: what was searched, and suggestions: a spelling the drive knows ("did you
//!   mean"), all drives, the files' contents, no filters.
//!
//! A cloud drive's results carry a note: where its names and contents come from.

use azul::{dom::TabIndex, prelude::*, str::String as AzString};
use azul_appkit::l10n::{label, t, t_args, t_label, t_phrase, Arg};

use crate::{
    actions::{action_ref, on_action, Action, Toggle},
    browse::{self, Column, Entry},
    find::{self, SearchScope},
    ids, look,
    model::ViewLayout,
    rank, ui_view, DriveState,
};

/// The columns: their headers, their widths (px), the column a click sorts by (`None`: Match
/// puts the results back in the ranked order).
/// Match takes the room the others leave (its width is its least), so Size ends at the list's
/// right edge in the design's 300 px preview beside a 228 px navigation pane.
const COLUMNS: [(&str, f32, Option<Column>); 4] = [
    ("azdrive-find-column-name", 240.0, Some(Column::Name)),
    ("azdrive-find-column-match", 290.0, None),
    ("azdrive-find-column-modified", 150.0, Some(Column::Modified)),
    ("azdrive-find-column-size", 80.0, Some(Column::Size)),
];

/// The Status column (where something syncs): its header and width (px).
const STATUS: (&str, f32) = ("azdrive-find-column-status", 170.0);

/// A result's row: two lines of snippet under the row's height.
pub(crate) fn row_px(s: &DriveState) -> f32 {
    s.settings.density.row_px() + 18.0
}

/// The width the rows draw in: the columns (each with its padding) and the check boxes'.
pub(crate) fn width(s: &DriveState) -> f32 {
    let checks = if s.settings.item_checkboxes { 28.0 } else { 0.0 };
    let status = if s.sync.syncs() { STATUS.1 + 8.0 } else { 0.0 };
    COLUMNS.iter().map(|(_, w, _)| w + 8.0).sum::<f32>() + checks + status
}

/// A cell of `width` px holding `content`, cut with an ellipsis.
fn cell(width: f32, extra: &str, content: Dom) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "width: {width}px; min-width: {width}px; flex-shrink: 0; padding-left: 8px; \
             overflow: hidden; white-space: nowrap; text-overflow: ellipsis; {extra}"
        ))
        .with_child(content)
}

/// `text` as spans, the `marks` (byte ranges of it) in the search's mark colour.
pub(crate) fn marked(text: &str, marks: &[(usize, usize)], css: &str) -> Dom {
    let span = |part: &str| Dom::create_span_with_text(AzString::from(part));
    let mut out = Dom::create_div().with_css(css);
    let mut at = 0;
    for &(start, end) in marks {
        let (Some(before), Some(hit)) = (text.get(at..start), text.get(start..end)) else {
            continue;
        };
        if !before.is_empty() {
            out.add_child(span(before));
        }
        out.add_child(span(hit).with_css(look::FIND_MARK));
        at = end;
    }
    if let Some(rest) = text.get(at..).filter(|rest| !rest.is_empty()) {
        out.add_child(span(rest));
    }
    out
}

// ==== The bar ====

/// A chip of the bar: its icon and words; `on` (a scope chosen) in the accent, `set` (a filter
/// that narrows) outlined in it; `menu` adds the drop-down arrow.
fn chip(app: &RefAny, icon: &str, text: &str, on: bool, set: bool, menu: bool, action: Action) -> Dom {
    let css = if on {
        look::CHIP_ON
    } else if set {
        look::CHIP_SET
    } else {
        ""
    };
    let mut chip = Dom::create_div()
        .with_css(format!("{} {css}", look::CHIP))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(AzString::from(text))
        .with_child(Dom::create_icon(AzString::from(icon)).with_css("font-size: 15px; margin-right: 4px;"))
        .with_child(Dom::create_span_with_text(AzString::from(text)));
    if menu {
        chip.add_child(Dom::create_icon(AzString::from("arrow_drop_down")).with_css("font-size: 15px;"));
    }
    chip.with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        action_ref(app, action),
        on_action,
    )
}

/// The search's bar over the view: the scope, the refine, File contents, the count, Save search.
pub(crate) fn bar(s: &DriveState, app: &RefAny) -> Dom {
    let scope = s.settings.search_scope;
    let refines = &s.refines;
    let scope_chip = |which: SearchScope, icon: &str, text: &str| {
        chip(app, icon, &t(text), scope == which, false, false, Action::SearchScope(which))
    };
    let refine_chip = |icon: &str, name: &str, chosen: Option<&str>, action: Action| {
        let text = chosen.map_or_else(|| t(name), t_label);
        chip(app, icon, &text, false, chosen.is_some(), true, action)
    };
    let mut bar = Dom::create_div()
        .with_id(ids::FIND_BAR)
        .with_css(look::FIND_BAR)
        .with_child(scope_chip(SearchScope::Folder, "folder", "azdrive-scope-folder"))
        .with_child(scope_chip(SearchScope::Drive, "storage", "azdrive-scope-drive"))
        .with_child(scope_chip(SearchScope::AllDrives, "travel_explore", "azdrive-scope-all"))
        .with_child(Dom::create_div().with_css(look::FIND_BAR_SEPARATOR))
        .with_child(refine_chip(
            "category",
            "azdrive-ribbon-kind",
            (refines.kind != find::KindRefine::Any).then(|| refines.kind.label()),
            Action::RefineKindMenu,
        ))
        .with_child(refine_chip(
            "event",
            "azdrive-ribbon-date-modified",
            (refines.date != find::DateRefine::Any).then(|| refines.date.label()),
            Action::RefineDateMenu,
        ))
        .with_child(refine_chip(
            "straighten",
            "azdrive-ribbon-size",
            (refines.size != find::SizeRefine::Any).then(|| refines.size.label()),
            Action::RefineSizeMenu,
        ))
        .with_child(chip(
            app,
            "find_in_page",
            &t("azdrive-ribbon-file-contents"),
            false,
            s.settings.search_contents,
            false,
            Action::Toggle(Toggle::SearchContents),
        ))
        .with_child(Dom::create_div().with_css("flex-grow: 1;"));
    if let Some(find) = &s.find {
        let mut said = vec![t_phrase(&find.status_text())];
        // The drive's index, when it has one: how far its update got, what it holds.
        if let Some(drive_id) = s
            .current_drive_id()
            .filter(|id| s.settings.indexed_drives.contains(id))
        {
            said.push(s.indexes.get(&drive_id).map_or_else(
                || t("azdrive-index-status-never"),
                |info| t_phrase(&info.status_text()),
            ));
        }
        bar.add_child(
            Dom::create_div()
                .with_id(ids::FIND_STATUS)
                .with_css(look::FIND_STATUS)
                .with_child(Dom::create_span_with_text(AzString::from(said.join(", ")))),
        );
    }
    if !s.search.trim().is_empty() {
        bar.add_child(
            Dom::create_div()
                .with_id(ids::FIND_SAVE)
                .with_css(look::QUIET_BUTTON)
                .with_tab_index(TabIndex::Auto)
                .with_child(Dom::create_span_with_text(label("azdrive-ribbon-save-search")))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    action_ref(app, Action::SaveSearch),
                    on_action,
                ),
        );
    }
    bar
}

// ==== With an empty box, and with nothing found ====

/// A row of chips, centred under an empty state.
fn chip_row(chips: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(look::CHIP_ROW)
        .with_children(DomVec::from(chips))
}

/// The search view with an empty box: what to do, the recent and the saved searches.
pub(crate) fn start(s: &DriveState, app: &RefAny) -> Dom {
    let recent: Vec<Dom> = s
        .settings
        .recent_searches
        .iter()
        .map(|text| chip(app, "history", text, false, false, false, Action::SearchFor(text.clone())))
        .collect();
    let saved: Vec<Dom> = s
        .settings
        .saved_searches
        .iter()
        .enumerate()
        .map(|(at, saved)| {
            chip(app, "bookmark_border", &saved.name, false, false, false, Action::RunSavedSearch(at))
        })
        .collect();
    Dom::create_div()
        .with_id(ids::FIND_START)
        .with_css(look::FIND_EMPTY_STATE)
        .with_child(Dom::create_icon(AzString::from("manage_search")).with_css(look::FIND_EMPTY_ICON))
        .with_child(
            Dom::create_div()
                .with_css(look::FIND_EMPTY_TITLE)
                .with_child(Dom::create_span_with_text(label("azdrive-find-type-to-search"))),
        )
        .with_child(chip_row(recent))
        .with_child(chip_row(saved))
}

/// The words a "did you mean" may suggest: the open folder's names, the folders the source
/// list has listed, the recent and saved searches.
fn vocabulary(s: &DriveState) -> Vec<String> {
    let mut words: Vec<String> = s.entries.iter().map(|e| e.name.clone()).collect();
    for folders in s.tree.loaded.values() {
        words.extend(folders.iter().map(|f| azul_storage::key::last_segment(f).to_string()));
    }
    words.extend(s.settings.recent_searches.iter().cloned());
    words.extend(s.settings.saved_searches.iter().map(|saved| saved.query.clone()));
    words
}

/// No result (yet): the search says what it does, or what it looked at and what to try.
pub(crate) fn nothing(s: &DriveState, app: &RefAny, find: &find::FindState) -> Dom {
    let (title, detail) = find.empty_text();
    if find.running() {
        return Dom::create_div()
            .with_id(ids::FIND_EMPTY)
            .with_css("padding: 16px; opacity: 0.7;")
            .with_child(Dom::create_span_with_text(label(title)));
    }
    let mut suggestions = Vec::new();
    let words = vocabulary(s);
    if let Some(word) = rank::did_you_mean(&find.query, words.iter().map(String::as_str)) {
        let text = t_args("azdrive-find-did-you-mean", &[("word", Arg::from(word.as_str()))]);
        let query = suggestion_query(&find.query, &word);
        suggestions.push(chip(app, "auto_awesome", &text, false, false, false, Action::SearchFor(query)));
    }
    if s.settings.search_scope != SearchScope::AllDrives {
        suggestions.push(chip(
            app,
            "travel_explore",
            &t("azdrive-find-search-everywhere"),
            false,
            false,
            false,
            Action::SearchScope(SearchScope::AllDrives),
        ));
    }
    if !s.settings.search_contents {
        suggestions.push(chip(
            app,
            "find_in_page",
            &t("azdrive-find-search-contents"),
            false,
            false,
            false,
            Action::SearchContentsToo,
        ));
    }
    if !s.refines.is_any() {
        suggestions.push(chip(
            app,
            "filter_list",
            &t("azdrive-find-clear-filters"),
            false,
            false,
            false,
            Action::ClearRefines,
        ));
    }
    Dom::create_div()
        .with_id(ids::FIND_EMPTY)
        .with_css(look::FIND_EMPTY_STATE)
        .with_child(Dom::create_icon(AzString::from("search_off")).with_css(look::FIND_EMPTY_ICON))
        .with_child(
            Dom::create_div()
                .with_css(look::FIND_EMPTY_TITLE)
                .with_child(Dom::create_span_with_text(label(title))),
        )
        .with_child(
            Dom::create_div()
                .with_css(look::FIND_EMPTY_DETAIL)
                .with_child(Dom::create_span_with_text(label(detail))),
        )
        .with_child(chip_row(suggestions).with_id(ids::FIND_SUGGESTIONS))
}

/// The query with its last term replaced by the suggested `word` ("2025 stuer" -> "2025
/// steuer").
fn suggestion_query(query: &str, word: &str) -> String {
    let mut terms: Vec<&str> = query.split_whitespace().collect();
    terms.pop();
    terms.push(word);
    terms.join(" ")
}

// ==== The results ====

/// The column headers on the Details header's raised face: the results come ranked and
/// grouped; a click on a header sorts them by its column (again: the other way, the sorted one
/// tinted with its arrow), one on Match puts them back as ranked.
pub(crate) fn header(s: &DriveState, app: &RefAny) -> Dom {
    let sort = s.find.as_ref().and_then(|f| f.sort);
    let mut row = Dom::create_div()
        .with_id(ids::FIND_HEADER)
        .with_css(look::DETAILS_HEADER);
    if s.settings.item_checkboxes {
        row.add_child(Dom::create_div().with_css("width: 28px; flex-shrink: 0;"));
    }
    for (header, width, column) in COLUMNS {
        let sorted = column.is_some() && sort.map(|s| s.column) == column;
        let mut cell = Dom::create_div()
            .with_class(ids::COLUMN_CLASS)
            .with_css(format!(
                "{} width: {width}px; min-width: {width}px; {} {}",
                look::COLUMN,
                if column.is_none() { "flex-grow: 1;" } else { "" },
                if sorted { look::COLUMN_SORTED } else { "" }
            ))
            .with_child(Dom::create_span_with_text(label(header)).with_css(
                "flex-grow: 1; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
            ));
        if let (true, Some(sort)) = (sorted, sort) {
            cell.add_child(
                Dom::create_icon(AzString::from(if sort.descending {
                    "arrow_drop_down"
                } else {
                    "arrow_drop_up"
                }))
                .with_css("font-size: 16px; flex-shrink: 0;"),
            );
        }
        row.add_child(cell.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            action_ref(app, Action::SortResults(column)),
            on_action,
        ));
    }
    if s.sync.syncs() {
        let (header, width) = STATUS;
        row.add_child(
            Dom::create_div()
                .with_class(ids::COLUMN_CLASS)
                .with_css(format!(
                    "{} width: {width}px; min-width: {width}px;",
                    look::COLUMN
                ))
                .with_child(Dom::create_span_with_text(label(header))),
        );
    }
    row
}

/// The header of a folder's results: its icon, the folders from the drive's root in the
/// accent, how many results it holds, a rule to the edge.
pub(crate) fn group_header(text: &str, count: usize) -> Dom {
    Dom::create_div()
        .with_class(ids::FIND_GROUP_CLASS)
        .with_css(format!("height: {}px; {}", crate::listing::HEADER_PX, look::FIND_GROUP))
        .with_accessibility_name(AzString::from(text))
        .with_child(Dom::create_icon(AzString::from("folder")).with_css("font-size: 16px; margin-right: 8px;"))
        .with_child(Dom::create_span_with_text(AzString::from(text)))
        .with_child(
            Dom::create_span_with_text(AzString::from(format!(
                " \u{00B7} {}",
                t_args("azdrive-find-group-count", &[("count", Arg::from(count))])
            )))
            .with_css(look::FIND_GROUP_COUNT),
        )
        .with_child(Dom::create_div().with_css(look::FIND_GROUP_RULE))
}

/// The header words of the results' folder `folder` (a key prefix; on This PC's search the row
/// key names the drive): the drive's name and the folders from its root.
pub(crate) fn group_text(s: &DriveState, folder: &str) -> String {
    let drive_name = |id: &str| {
        s.slot_index(id)
            .map_or_else(|| id.to_string(), |i| s.slots[i].name())
    };
    match find::split_pc_key(folder) {
        Some((drive, rest)) => rank::group_label(&drive_name(drive), rest),
        None => rank::group_label(&s.drive_name(&s.place), folder),
    }
}

/// The Status cell: the sync's state of the result, its icon and its words (empty where its
/// drive does not sync).
fn status_cell(s: &DriveState, entry: &Entry) -> Dom {
    let state = find::result_sync(&*s.sync, s.current_drive_id().as_deref(), &entry.key);
    let mut content = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; opacity: 0.8;");
    if let Some((icon, words)) = state.map(crate::sync_lookup::SyncState::badge) {
        content.add_child(
            Dom::create_icon(AzString::from(icon))
                .with_css("font-size: 14px; margin-right: 4px; flex-shrink: 0;"),
        );
        content.add_child(Dom::create_span_with_text(label(words)));
    }
    content
}

/// The note over a cloud drive's results: its names come from a listing of every file below the
/// folder - slower than a folder on this computer -, and its files are not read.
pub(crate) fn cloud_note(find: &find::FindState) -> Dom {
    Dom::create_div()
        .with_id(ids::FIND_NOTE)
        .with_css(look::FIND_NOTE)
        .with_child(
            Dom::create_icon(AzString::from("cloud_queue"))
                .with_css("font-size: 14px; margin-right: 6px; flex-shrink: 0;"),
        )
        .with_child(Dom::create_span_with_text(label(find.cloud_note_text())))
}

/// The Match cell: two lines of the text the result's contents matched on, the terms marked;
/// a result found by its name says what it is instead.
fn snippet_cell(s: &DriveState, entry: &Entry, line: Option<&find::FoundLine>, query: &str) -> Dom {
    let found = line.and_then(|line| rank::snippet(&line.text, query));
    match found {
        Some((text, marks)) => marked(&text, &marks, look::SNIPPET).with_class(ids::FIND_MATCH_CLASS),
        None => Dom::create_div()
            .with_class(ids::FIND_MATCH_CLASS)
            .with_css(look::SNIPPET_KIND)
            .with_child(Dom::create_span_with_text(AzString::from(crate::model::cell_text(
                entry,
                Column::Type,
                s.settings.show_extensions,
            )))),
    }
}

/// A result's row: its name, its snippet, its date and its size (and its sync state) - every
/// other one (`alt`) a quiet stripe, under the hover and the selection, one height as the
/// virtual view places it.
pub(crate) fn row(s: &DriveState, app: &RefAny, entry: &Entry, alt: bool) -> Dom {
    let shade = if alt { look::STRIPE } else { "" };
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: {}px; \
         box-sizing: border-box; flex-shrink: 0; {shade} {}",
        ui_view::line_px(s, ViewLayout::Details),
        ui_view::item_paint(s, entry)
    ));
    if alt {
        row.add_class(ids::ROW_ALT_CLASS);
    }
    if s.settings.item_checkboxes {
        row.add_child(ui_view::check_box(s, app, entry));
    }
    let query = s.find.as_ref().map_or("", |f| f.query.as_str());
    let line = s.find.as_ref().and_then(|f| f.lines.get(&entry.key));
    let [name, matched, modified, size] = COLUMNS.map(|(_, width, _)| width);
    row.add_child(cell(name, "", ui_view::name_cell_marked(s, app, entry, 16.0, query)));
    row.add_child(cell(
        matched,
        "white-space: normal; flex-grow: 1;",
        snippet_cell(s, entry, line, query),
    ));
    row.add_child(cell(
        modified,
        "opacity: 0.8; font-variant-numeric: tabular-nums;",
        Dom::create_span_with_text(AzString::from(browse::format_modified(
            entry.modified,
            &chrono::Local,
        ))),
    ));
    row.add_child(cell(
        size,
        "opacity: 0.8; display: flex; flex-direction: row; justify-content: flex-end; \
         padding-right: 12px; font-variant-numeric: tabular-nums;",
        Dom::create_span_with_text(AzString::from(ui_view::size_text(s, entry))),
    ));
    if s.sync.syncs() {
        row.add_child(cell(STATUS.1, "", status_cell(s, entry)));
    }
    ui_view::interactive(s, app, entry, row)
}

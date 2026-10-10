//! The search's results in the folder view: the Details layout's virtual view of rows with the
//! search's own columns - Name (the icon and the name, F2 renames it in place), Folder (where
//! the result is, from the drive's root), Match (the line its contents matched on: its number,
//! the line with the match marked), Date modified and Size (the rows in view are stat'ed as a
//! folder's are), and - where something syncs - Status (the sync's state of the file). Every
//! row takes the clicks, the menu and the drag of a folder's item. A cloud drive's results carry
//! a note: where its names and contents come from.

use azul::{prelude::*, str::String as AzString};
use azul_appkit::l10n::label;

use crate::{
    actions::{action_ref, on_action, Action},
    browse::{self, Column, Entry},
    find, ids, look,
    model::ViewLayout,
    ui_view, DriveState,
};

/// The columns: their headers, their widths (px), the column a click sorts by (`None`: Match
/// puts the results back in the order they were found).
const COLUMNS: [(&str, f32, Option<Column>); 5] = [
    ("azdrive-find-column-name", 260.0, Some(Column::Name)),
    ("azdrive-find-column-folder", 200.0, Some(Column::Path)),
    ("azdrive-find-column-match", 380.0, None),
    ("azdrive-find-column-modified", 140.0, Some(Column::Modified)),
    ("azdrive-find-column-size", 90.0, Some(Column::Size)),
];

/// The Status column (where something syncs): its header and width (px).
const STATUS: (&str, f32) = ("azdrive-find-column-status", 170.0);

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

/// The column headers on the Details header's raised face: the results come in the order they
/// were found (the names first); a click on a header sorts them by its column (again: the other
/// way, the sorted one tinted with its arrow), one on Match puts them back as found.
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
                "{} width: {width}px; min-width: {width}px; {}",
                look::COLUMN,
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

/// The Folder cell: where the result is, from its drive's root - the drive's name for its
/// root, and on This PC the drive's name before the path.
fn folder_text(s: &DriveState, row: &str) -> String {
    let drive_name = |id: &str| {
        s.slot_index(id)
            .map_or_else(|| id.to_string(), |i| s.slots[i].entry.name.clone())
    };
    match find::split_pc_key(row) {
        Some((drive, key)) => match find::folder_of(key) {
            "" => drive_name(drive),
            folder => format!("{}/{folder}", drive_name(drive)),
        },
        None => match find::folder_of(row) {
            "" => s.drive_name(&s.place),
            folder => folder.to_string(),
        },
    }
}

/// The Match cell: "12:" and the line with the match marked; empty for a result found by its
/// name alone.
fn match_cell(line: Option<&find::FoundLine>) -> Dom {
    let mut text = Dom::create_div()
        .with_class(ids::FIND_MATCH_CLASS)
        .with_css("display: flex; flex-direction: row; align-items: center; min-width: 0px;");
    let Some(line) = line else {
        return text;
    };
    let (before, matched, after) = line.preview();
    text.add_child(
        Dom::create_span_with_text(AzString::from(format!("{}:", line.line)))
            .with_css("opacity: 0.6; margin-right: 6px; flex-shrink: 0;"),
    );
    text.add_child(
        Dom::create_div()
            .with_css(
                "min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
            )
            .with_child(Dom::create_span_with_text(AzString::from(before)))
            .with_child(Dom::create_span_with_text(AzString::from(matched)).with_css(look::FIND_MARK))
            .with_child(Dom::create_span_with_text(AzString::from(after))),
    );
    text
}

/// A result's row: Name, Folder, Match, Date modified, Size - every other one (`alt`) a quiet
/// stripe, under the hover and the selection, one height as the virtual view places it.
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
    let folder = folder_text(s, &entry.key);
    let line = s.find.as_ref().and_then(|f| f.lines.get(&entry.key));
    let [name, place, matched, modified, size] = COLUMNS.map(|(_, width, _)| width);
    row.add_child(cell(name, "", ui_view::name_cell(s, app, entry, 16.0)));
    row.add_child(cell(
        place,
        "opacity: 0.8;",
        Dom::create_span_with_text(AzString::from(folder)).with_class(ids::FIND_FOLDER_CLASS),
    ));
    row.add_child(cell(matched, "", match_cell(line)));
    row.add_child(cell(
        modified,
        "opacity: 0.8;",
        Dom::create_span_with_text(AzString::from(browse::format_modified(
            entry.modified,
            &chrono::Local,
        ))),
    ));
    row.add_child(cell(
        size,
        "opacity: 0.8; display: flex; flex-direction: row; justify-content: flex-end; \
         padding-right: 12px;",
        Dom::create_span_with_text(AzString::from(ui_view::size_text(s, entry))),
    ));
    if s.sync.syncs() {
        row.add_child(cell(STATUS.1, "", status_cell(s, entry)));
    }
    ui_view::interactive(s, app, entry, row)
}

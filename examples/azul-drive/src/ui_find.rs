//! The search's results in the folder view: the Details layout's virtual view of rows with the
//! search's own columns - Name (the icon and the name, F2 renames it in place), Folder (where
//! the result is, from the drive's root), Match (the line its contents matched on: its number,
//! the line with the match marked), Date modified and Size (the rows in view are stat'ed as a
//! folder's are). Every row takes the clicks, the menu and the drag of a folder's item. A cloud
//! drive's results carry a note: searched by name, slower.

use azul::{prelude::*, str::String as AzString};

use crate::{
    browse::{self, Entry},
    find, ids, look,
    model::ViewLayout,
    ui_view, DriveState,
};

/// The columns: their headers and their widths (px).
const COLUMNS: [(&str, f32); 5] = [
    ("Name", 260.0),
    ("Folder", 200.0),
    ("Match", 380.0),
    ("Date modified", 140.0),
    ("Size", 90.0),
];

/// The width the rows draw in: the columns (each with its padding) and the check boxes'.
pub(crate) fn width(s: &DriveState) -> f32 {
    let checks = if s.settings.item_checkboxes { 28.0 } else { 0.0 };
    COLUMNS.iter().map(|(_, w)| w + 8.0).sum::<f32>() + checks
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

/// The column headers on the Details header's raised face (the results come in the order they
/// were found - the names first -, so a header does not sort).
pub(crate) fn header(s: &DriveState) -> Dom {
    let mut row = Dom::create_div()
        .with_id(ids::FIND_HEADER)
        .with_css(look::DETAILS_HEADER);
    if s.settings.item_checkboxes {
        row.add_child(Dom::create_div().with_css("width: 28px; flex-shrink: 0;"));
    }
    for (label, width) in COLUMNS {
        row.add_child(
            Dom::create_div()
                .with_class(ids::COLUMN_CLASS)
                .with_css(format!(
                    "{} width: {width}px; min-width: {width}px;",
                    look::COLUMN
                ))
                .with_child(Dom::create_span_with_text(AzString::from(label)).with_css(
                    "flex-grow: 1; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
                )),
        );
    }
    row
}

/// The note over a cloud drive's results: its names come from a listing of every file below the
/// folder - slower than a folder on this computer -, and its files are not read.
pub(crate) fn cloud_note() -> Dom {
    Dom::create_div()
        .with_id(ids::FIND_NOTE)
        .with_css(look::FIND_NOTE)
        .with_child(
            Dom::create_icon(AzString::from("cloud_queue"))
                .with_css("font-size: 14px; margin-right: 6px; flex-shrink: 0;"),
        )
        .with_child(Dom::create_span_with_text(AzString::from(
            "A cloud drive is searched by name, over a listing of every file below this folder: \
             slower than a folder on this computer, and file contents are not searched.",
        )))
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
    let folder = find::folder_of(&entry.key);
    let folder = if folder.is_empty() {
        s.drive_name(&s.place)
    } else {
        folder.to_string()
    };
    let line = s.find.as_ref().and_then(|f| f.lines.get(&entry.key));
    let [name, place, matched, modified, size] = COLUMNS.map(|(_, width)| width);
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
    ui_view::interactive(s, app, entry, row)
}

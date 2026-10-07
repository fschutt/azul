//! The side bar's EXPLORER: the workspace's tree, VIRTUALIZED - a
//! `VirtualView` that builds only the rows in view (and a screen either
//! side), whatever the folder holds: a `node_modules` opened is ten thousand
//! rows, of which ~40 are nodes. The rows are the workspace's
//! ([`crate::workspace::Workspace::rows`], depth first, kept as the
//! listings arrive); a folder opens as it is listed (one level, on a drive
//! Thread), a click on a file opens it in a tab.
//!
//! The keys (the tree has the focus): Up / Down move the selection, Right
//! opens a folder (or goes into it), Left closes it (or goes to its
//! folder), Enter / Space opens the file or opens / closes the folder.
//!
//! Without a workspace: VSCode's empty explorer - "You have not yet opened
//! a folder.", Open Folder, the recent folders.

use azul::{
    callbacks::ButtonOnClickCallbackType,
    dom::{AccessibilityInfo, AccessibilityRole, DomId, NodeId, TabIndex, VirtualKeyCode},
    prelude::*,
    widgets::{Button, ButtonType},
};

use crate::{
    app::AppState,
    commands, ids,
    ui::{self, column, label, side_title},
    workspace::{parent_folder, Row},
};

/// A row's height, px (VSCode's).
pub const ROW_HEIGHT: f32 = 22.0;
/// The indent of one level, px.
const INDENT: f32 = 8.0;

/// What the explorer's view hands its callback: the app.
struct ExplorerRef {
    app: RefAny,
}

/// What a row's click acts on: the app and the row's key.
struct RowRef {
    app: RefAny,
    key: String,
}

/// The explorer: its title row, the workspace's name, the virtualized tree;
/// without a workspace, VSCode's empty state.
pub fn explorer(app: &RefAny, st: &AppState) -> Dom {
    let Some(w) = st.workspace.as_ref() else {
        return column(vec![side_title("EXPLORER", Vec::new()), no_folder(app, st)]);
    };
    let actions = vec![
        ui::icon_button(ids::REFRESH, "refresh", "Refresh Explorer", app.clone(), on_refresh),
        ui::icon_button(ids::COLLAPSE, "unfold_less", "Collapse Folders in Explorer", app.clone(), on_collapse),
    ];
    let folder = Dom::create_div()
        .with_id(ids::EXPLORER_FOLDER)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; height: 22px; flex-shrink: 0; \
             padding: 0px 8px 0px 4px; font-size: 11px; font-weight: 700;",
        )
        .with_child(Dom::create_icon("expand_more").with_css("font-size: 16px; padding-right: 2px;"))
        .with_child(Dom::create_span_with_text(w.root.name.to_uppercase()));
    let tree = Dom::create_virtual_view(RefAny::new(ExplorerRef { app: app.clone() }), render_rows)
        .with_id(ids::EXPLORER)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo::named(format!("Files of {}", w.root.name), AccessibilityRole::Outline))
        .with_callback(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            app.clone(),
            on_tree_key,
        );
    column(vec![side_title("EXPLORER", actions), folder, tree])
}

/// A rect of the view, px.
fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// The rows `first..end` of `count` that a view `height` px tall scrolled to
/// `y` builds: the rows in view and a screen either side.
#[must_use]
pub fn window_of(count: usize, y: f32, height: f32) -> (usize, usize) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let row = |px: f32| (px.max(0.0) / ROW_HEIGHT) as usize;
    let first = row(y - height).min(count);
    let end = (row(y + 2.0 * height) + 1).min(count);
    (first, end.max(first))
}

/// The tree's `VirtualView`: the rows in view and a screen either side.
extern "C" fn render_rows(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some(mut app) = data.downcast_ref::<ExplorerRef>().map(|e| e.app.clone()) else {
        return VirtualViewReturn::default();
    };
    let handle = app.clone();
    let Some(guard) = app.downcast_ref::<AppState>() else {
        return VirtualViewReturn::default();
    };
    let Some(w) = guard.workspace.as_ref() else {
        return VirtualViewReturn::default();
    };
    let rows = w.rows();
    let size = info.bounds.get_logical_size();
    let width = size.width.max(1.0);
    let height = size.height.max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let total = (rows.len() as f32 * ROW_HEIGHT).max(1.0);
    let (first, end) = window_of(rows.len(), info.scroll_offset.y, height);
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px;"
    ));
    for row in &rows[first..end] {
        let selected = w.selected.as_deref() == Some(row.key.as_str());
        root.add_child(row_dom(&handle, row, selected));
    }
    #[allow(clippy::cast_precision_loss)]
    let top = first as f32 * ROW_HEIGHT;
    #[allow(clippy::cast_precision_loss)]
    let built = ((end - first) as f32 * ROW_HEIGHT).max(1.0);
    VirtualViewReturn::with_dom(root, rect(0.0, top, width, built), rect(0.0, 0.0, width, total))
}

/// One row: its indent, a folder's chevron, its icon and name; a click opens
/// the file, or opens / closes the folder.
fn row_dom(app: &RefAny, row: &Row, selected: bool) -> Dom {
    #[allow(clippy::cast_precision_loss)]
    let indent = 8.0 + row.depth as f32 * INDENT;
    let look = if selected {
        "background: rgba(128, 128, 128, 0.22);"
    } else {
        ":hover { background: rgba(128, 128, 128, 0.12); }"
    };
    let chevron = if !row.folder {
        Dom::create_div().with_css("width: 16px; flex-shrink: 0;")
    } else if row.expanded {
        Dom::create_icon("expand_more").with_css("font-size: 16px; width: 16px; flex-shrink: 0;")
    } else {
        Dom::create_icon("chevron_right").with_css("font-size: 16px; width: 16px; flex-shrink: 0;")
    };
    let icon = match (row.folder, row.expanded) {
        (true, true) => "folder_open",
        (true, false) => "folder",
        _ => "description",
    };
    let state = match (row.folder, row.expanded) {
        (true, true) => ", open folder",
        (true, false) => ", folder",
        _ => "",
    };
    let mut dom = Dom::create_div()
        .with_id(ids::tree_row(&row.key))
        .with_class(ids::TREE_ROW_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: {ROW_HEIGHT}px; \
             flex-shrink: 0; padding-left: {indent}px; padding-right: 8px; cursor: pointer; \
             white-space: nowrap; overflow: hidden; font-size: 13px; {look}"
        ))
        .with_accessibility_info(AccessibilityInfo::named(format!("{}{state}", row.name), AccessibilityRole::OutlineItem))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(RowRef {
                app: app.clone(),
                key: row.key.clone(),
            }),
            on_row_click,
        )
        .with_child(chevron)
        .with_child(Dom::create_icon(icon).with_css("font-size: 16px; padding: 0px 6px 0px 2px; opacity: 0.8;"))
        .with_child(label(&row.name, "min-width: 0px; overflow: hidden;"));
    if selected {
        dom = dom.with_class(ids::TREE_SELECTED_CLASS);
    }
    dom
}

/// No folder yet: VSCode's empty explorer - what to do, Open Folder, the
/// recent folders.
fn no_folder(app: &RefAny, st: &AppState) -> Dom {
    let mut out = Dom::create_div()
        .with_id(ids::NO_FOLDER)
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
             padding: 4px 20px 12px 20px;",
        )
        .with_child(label(
            "You have not yet opened a folder.",
            "font-size: 13px; padding: 8px 0px 12px 0px;",
        ))
        .with_child(
            // A column stretches the button to the side bar's width.
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(
                    Button::with_type("Open Folder", ButtonType::Primary)
                        .with_on_click(app.clone(), ui::on_open_folder as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::OPEN_FOLDER),
                ),
        )
        .with_child(label(
            &format!(
                "Or press {}, or start AzCode with a folder: AzCode ~/my-project. Its files open \
                 in the editor; quick open ({}) finds them by name.",
                commands::keys("Mod+K Mod+O"),
                commands::keys("Mod+P"),
            ),
            "font-size: 12px; opacity: 0.7; padding-top: 10px;",
        ));
    if !st.recent.is_empty() {
        out.add_child(label("RECENT", "font-size: 11px; opacity: 0.8; padding: 20px 0px 6px 0px;"));
        for (i, folder) in st.recent.iter().enumerate() {
            out.add_child(ui::recent_row(app, ids::recent(i), i, folder));
        }
    }
    out
}

/// What a click on row `key` does: a file opens, a folder opens or closes.
fn activate(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str) {
    let Some(row) = st
        .workspace
        .as_ref()
        .and_then(|w| w.index_of(key).map(|i| w.rows()[i].clone()))
    else {
        return;
    };
    if let Some(w) = st.workspace.as_mut() {
        w.selected = Some(row.key.clone());
    }
    if row.folder {
        commands::toggle_folder(st, info, app, &row.key, !row.expanded);
    } else {
        commands::open_file(st, info, app, &row.key);
    }
}

extern "C" fn on_row_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data.downcast_ref::<RowRef>().map(|r| (r.app.clone(), r.key.clone())) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| activate(st, info, app, &key))
}

extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, commands::refresh_explorer)
}

extern "C" fn on_collapse(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, _, _| {
        if let Some(w) = st.workspace.as_mut() {
            w.collapse_all();
        }
    })
}

/// The tree's keys (WAI-ARIA's tree): Up / Down, Right / Left, Enter /
/// Space, Home / End.
extern "C" fn on_tree_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    if m.primary_down() || m.alt {
        return Update::DoNothing;
    }
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let Some(w) = st.workspace.as_ref() else {
        return Update::DoNothing;
    };
    let rows = w.rows();
    if rows.is_empty() {
        return Update::DoNothing;
    }
    let at = w.selected.as_deref().and_then(|k| w.index_of(k));
    let last = rows.len() - 1;
    let target = match key {
        VirtualKeyCode::Down => Some(at.map_or(0, |i| (i + 1).min(last))),
        VirtualKeyCode::Up => Some(at.map_or(0, |i| i.saturating_sub(1))),
        VirtualKeyCode::Home => Some(0),
        VirtualKeyCode::End => Some(last),
        VirtualKeyCode::Right => {
            let Some(i) = at else {
                return Update::DoNothing;
            };
            let row = rows[i].clone();
            if row.folder && !row.expanded {
                info.prevent_default();
                commands::toggle_folder(st, &mut info, &handle, &row.key, true);
                return Update::RefreshDom;
            }
            // An open folder: into it.
            (row.folder && i < last && rows[i + 1].depth > row.depth).then_some(i + 1)
        }
        VirtualKeyCode::Left => {
            let Some(i) = at else {
                return Update::DoNothing;
            };
            let row = rows[i].clone();
            if row.folder && row.expanded {
                info.prevent_default();
                commands::toggle_folder(st, &mut info, &handle, &row.key, false);
                return Update::RefreshDom;
            }
            // Out to its folder.
            parent_folder(&row.key).and_then(|parent| w.index_of(parent))
        }
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter | VirtualKeyCode::Space => {
            let Some(i) = at else {
                return Update::DoNothing;
            };
            let row_key = rows[i].key.clone();
            info.prevent_default();
            activate(st, &mut info, &handle, &row_key);
            return Update::RefreshDom;
        }
        _ => return Update::DoNothing,
    };
    let Some(i) = target else {
        info.prevent_default();
        return Update::DoNothing;
    };
    info.prevent_default();
    let key_now = rows[i].key.clone();
    if let Some(w) = st.workspace.as_mut() {
        w.selected = Some(key_now);
    }
    reveal_row(&mut info, i);
    Update::RefreshDom
}

/// Scrolls the tree so row `index` is in view (the tree is the node the key
/// went to).
fn reveal_row(info: &mut CallbackInfo, index: usize) {
    let hit = info.get_hit_node();
    let raw = hit.node.into_raw();
    if raw == 0 {
        return;
    }
    let node = NodeId { inner: raw - 1 };
    let dom: DomId = hit.dom;
    let Some(size) = info.get_node_size(hit).into_option() else {
        return;
    };
    let y = info
        .get_scroll_offset_for_node(dom, node)
        .into_option()
        .map_or(0.0, |p| p.y);
    #[allow(clippy::cast_precision_loss)]
    let top = index as f32 * ROW_HEIGHT;
    let to = if top < y {
        top
    } else if top + ROW_HEIGHT > y + size.height {
        top + ROW_HEIGHT - size.height
    } else {
        return;
    };
    info.scroll_to(dom, hit.node, LogicalPosition::create(0.0, to.max(0.0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_builds_the_rows_in_view_and_a_screen_either_side() {
        // 10,000 rows, a 440 px view (20 rows) at the top: 41 rows built.
        assert_eq!(window_of(10_000, 0.0, 440.0), (0, 41));
        // Scrolled to row 5,000: from a screen above to a screen below.
        let y = 5_000.0 * ROW_HEIGHT;
        let (first, end) = window_of(10_000, y, 440.0);
        assert_eq!((first, end), (4_980, 5_041));
        assert!(end - first <= 61, "never more than three screens");
        assert_eq!(window_of(3, 0.0, 440.0), (0, 3), "a short tree: all of it");
        assert_eq!(window_of(0, 0.0, 440.0), (0, 0));
        assert_eq!(window_of(10, 9_999.0, 440.0), (10, 10), "scrolled past the end: nothing");
    }
}

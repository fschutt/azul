//! Virtual views in a mounted document (E2E-M1).
//!
//! A `mount` is XML, and the XML parser has no VirtualView: `<virtual-view>`,
//! `<iframe>` and `data-virtual-*` attributes mount as a plain `<div>` with the
//! attributes dropped, so a scenario about a 5000-row list (`gen-e2e` batch 1,
//! lines 7, 8, 16, 45-50) could only be written with a plain `div` of 5000 rows or
//! a 99,840 px spacer - the code under test, the view's windowing and its
//! callback, was never reached.
//!
//! `mount` takes `virtual_views`: for each entry the one EMPTY element its
//! `selector` names becomes a VirtualView whose callback ([`render_e2e_rows`])
//! is deterministic - `rows` rows of `row_height` px, row `i` built from
//! `row_html` (`{i}` its zero-based index, `{n}` its number from 1) - and
//! windowed like an app's list: it materializes the visible rows with a screen
//! of rows either side and is asked again as the view scrolls near an edge of
//! what it materialized, so a scenario can scroll through 5000 rows and watch
//! how few nodes exist.
//!
//! The views are kept with the window ([`super::e2e_set_mount_views`]) and put
//! into every document the mount parses - the first one, and a re-parse after a
//! theme switch - by [`apply_mount_views`], which both hosts call right after
//! they parse the mounted XML: the in-process runner and the dll's shells.

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, IdOrClass, NodeData},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    id::NodeId,
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::TextLines;
use crate::window::LayoutWindow;

/// The most rows one view may have: the row index is a `usize`, but a document of
/// ten million rows of 24 px is 240 million px and the scroll offsets are `f32`.
pub const MOUNT_VIEW_MAX_ROWS: usize = 10_000_000;

/// The tallest row, in px.
pub const MOUNT_VIEW_MAX_ROW_HEIGHT: f32 = 100_000.0;

/// The fewest rows a view materializes either side of the visible ones, so a
/// short view is not asked again for every row it scrolls.
const MIN_OVERSCAN_ROWS: usize = 5;

/// One virtual view of a `mount`: the empty element `selector` names becomes a
/// list of `rows` rows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountView {
    /// CSS selector of the EMPTY element that becomes the view (`#list`); it must
    /// match exactly one element of the mounted document. Give the element its
    /// size in the mount's CSS (`#list { width: 300px; height: 200px; }`): the
    /// rows are as wide as the box, and the box scrolls them.
    pub selector: String,
    /// How many rows the document has (1 to [`MOUNT_VIEW_MAX_ROWS`]).
    pub rows: usize,
    /// The height of every row in px (above 0, up to [`MOUNT_VIEW_MAX_ROW_HEIGHT`]).
    pub row_height: f32,
    /// The markup of one row, one array element per line (or a single string);
    /// `{i}` is the row's zero-based index and `{n}` its number from 1. Each row
    /// sits in a `<div class="e2e-row" id="row-{i}">` of `row_height` px that clips it.
    pub row_html: TextLines,
    /// Stylesheet of the view's content, one array element per line (or a single
    /// string): the mount's own `css` does not reach into a view's document.
    #[serde(default)]
    pub row_css: TextLines,
}

/// What the callback of a view renders from (its `RefAny`).
struct E2eRowsView {
    rows: usize,
    row_height: f32,
    row_html: String,
    row_css: String,
}

/// The rows a view of `rows` rows of `row_height` px materializes for a box
/// `height` px tall scrolled to `offset_y`, as `(first, end)`: the visible rows
/// and as many either side (at least [`MIN_OVERSCAN_ROWS`]). Never more than the
/// document has, whatever the offset: a scroll past the end shows the last rows.
#[must_use]
pub fn rows_window(rows: usize, row_height: f32, offset_y: f32, height: f32) -> (usize, usize) {
    let row = if row_height.is_finite() && row_height > 0.0 {
        row_height
    } else {
        1.0
    };
    let offset = if offset_y.is_finite() {
        offset_y.max(0.0)
    } else {
        0.0
    };
    let height = if height.is_finite() {
        height.max(0.0)
    } else {
        0.0
    };
    // `as usize` saturates: a huge offset lands on the last row below.
    let first_visible = ((offset / row).floor() as usize).min(rows.saturating_sub(1));
    let visible = ((height / row).ceil() as usize).saturating_add(1);
    let overscan = visible.max(MIN_OVERSCAN_ROWS);
    let first = first_visible.saturating_sub(overscan);
    let end = first_visible
        .saturating_add(visible)
        .saturating_add(overscan)
        .min(rows);
    (first.min(end), end)
}

/// The markup of row `i`: `row_html` with its placeholders filled, parsed as the
/// content of a `<body>`. A row that does not parse shows why (the `mount`
/// parses row 0 before it lands, so this is the row whose index breaks it).
fn row_content(row_html: &str, i: usize) -> Dom {
    let html = row_html
        .replace("{i}", &i.to_string())
        .replace("{n}", &(i + 1).to_string());
    match crate::xml::parse_xml(&format!("<html><body>{html}</body></html>")) {
        Ok(xml) => crate::xml::dom_fragment_from_parsed_xml(xml),
        Err(e) => Dom::create_p_with_text(format!("row {i}: {e:?}")),
    }
}

/// The callback of a mounted view: the window of rows the scroll offset asks
/// for. Deterministic - the same offset and box give the same document.
extern "C" fn render_e2e_rows(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let Some(view) = data.downcast_ref::<E2eRowsView>() else {
        return VirtualViewReturn::default();
    };
    let width = info.bounds.logical_size.width.max(0.0);
    let height = info.bounds.logical_size.height;
    let (first, end) = rows_window(view.rows, view.row_height, info.scroll_offset.y, height);

    let mut root = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("e2e-rows".into())].into())
        .with_css(&format!(
            ".e2e-rows {{ width: {width}px; }} \
             .e2e-row {{ height: {}px; overflow: hidden; }} {}",
            view.row_height, view.row_css
        ));
    for i in first..end {
        root.add_child(
            Dom::create_div()
                .with_ids_and_classes(
                    vec![
                        IdOrClass::Id(format!("row-{i}").into()),
                        IdOrClass::Class("e2e-row".into()),
                    ]
                    .into(),
                )
                .with_child(row_content(&view.row_html, i)),
        );
    }

    let row = view.row_height;
    VirtualViewReturn::with_dom(
        root,
        // What this call materialized, where it sits in the document ...
        LogicalRect::new(
            LogicalPosition::new(0.0, first as f32 * row),
            LogicalSize::new(width, (end - first) as f32 * row),
        ),
        // ... and the whole document, which the scroll bar stands for.
        LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width, view.rows as f32 * row),
        ),
    )
}

/// The elements of `styled_dom` a CSS selector matches, in document order.
fn matching_elements(styled_dom: &StyledDom, css_path: &azul_css::css::CssPath) -> Vec<NodeId> {
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let cascade_info = styled_dom.cascade_info.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .filter(|&n| {
            azul_core::style::matches_html_element(
                css_path,
                n,
                &node_hierarchy,
                &node_data,
                &cascade_info,
                None,
            )
        })
        .collect()
}

/// Check `views` and put each into `styled_dom`: the empty element its selector
/// names becomes a VirtualView (its ids, classes, attributes and style stay).
///
/// This is what the `mount` op runs on its own parse before it lets the document
/// land, so a view that cannot be mounted is refused at the step that asked for it.
///
/// # Errors
///
/// A message naming the view (`virtual_views[N]`) and what is wrong: a field out
/// of range, a row that is not valid markup, a selector that is not CSS, matches
/// no element or several, or names an element that has children.
pub fn apply_views(styled_dom: &mut StyledDom, views: &[MountView]) -> Result<(), String> {
    for (n, view) in views.iter().enumerate() {
        let what = format!("mount: virtual_views[{n}] ('{}')", view.selector);
        if view.selector.trim().is_empty() {
            return Err(format!("mount: virtual_views[{n}] has an empty `selector`"));
        }
        if view.rows == 0 || view.rows > MOUNT_VIEW_MAX_ROWS {
            return Err(format!(
                "{what}: rows is {}, which is not a whole number from 1 to {MOUNT_VIEW_MAX_ROWS}",
                view.rows
            ));
        }
        if !(view.row_height.is_finite()
            && view.row_height > 0.0
            && view.row_height <= MOUNT_VIEW_MAX_ROW_HEIGHT)
        {
            return Err(format!(
                "{what}: row_height is {}, which is not a number above 0 up to \
                 {MOUNT_VIEW_MAX_ROW_HEIGHT} (px)",
                view.row_height
            ));
        }
        let row_html = view.row_html.join();
        if row_html.trim().is_empty() {
            return Err(format!(
                "{what}: `row_html` is empty - say what a row holds"
            ));
        }
        if let Err(e) = crate::xml::parse_xml(&format!(
            "<html><body>{}</body></html>",
            row_html.replace("{i}", "0").replace("{n}", "1")
        )) {
            return Err(format!(
                "{what}: `row_html` is not valid markup ({e:?}); write a well-formed fragment, \
                 e.g. <div class=\"row\">Row {{n}}</div>"
            ));
        }

        let css_path = azul_css::parser2::parse_css_path(&view.selector)
            .map_err(|e| format!("{what}: not a valid CSS selector ({e:?})"))?;
        let nodes = matching_elements(styled_dom, &css_path);
        let [node] = nodes.as_slice() else {
            return Err(format!(
                "{what}: the selector must match exactly one element of the mounted document, \
                 it matches {}",
                nodes.len()
            ));
        };
        let node = *node;
        if styled_dom.node_hierarchy.as_container()[node]
            .last_child_id()
            .is_some()
        {
            return Err(format!(
                "{what}: the element has content - a view's rows replace it, so it has to be \
                 empty (`<div id=\"list\"></div>`)"
            ));
        }

        let data = RefAny::new(E2eRowsView {
            rows: view.rows,
            row_height: view.row_height,
            row_html,
            row_css: view.row_css.join(),
        });
        let mut container = styled_dom.node_data.as_container_mut();
        let old = &container[node];
        let mut host =
            NodeData::create_virtual_view(data, VirtualViewCallback::create(render_e2e_rows));
        host.set_ids_and_classes(old.get_ids_and_classes());
        host.set_attributes(old.get_attributes());
        host.set_style(old.get_style().clone());
        host.flags = old.flags;
        container[node] = host;
    }
    Ok(())
}

/// Put the virtual views the last `mount` asked for into the document a host just
/// parsed from the mounted XML. Called by the in-process runner and by the dll's
/// shells after they parse it, so every document the mount produces - the first,
/// and one re-parsed after a theme switch - has them.
///
/// The `mount` op refused any view that could not land on its own parse of the
/// same XML, so a failure here is a document that no longer parses the way it
/// did: it is logged and the document is left as parsed.
pub fn apply_mount_views(layout_window: &LayoutWindow, styled_dom: &mut StyledDom) {
    let views = super::e2e_mount_views(layout_window);
    if views.is_empty() {
        return;
    }
    if let Err(why) = apply_views(styled_dom, &views) {
        super::log(
            super::LogLevel::Error,
            super::LogCategory::DebugServer,
            format!("[E2E] the mounted document no longer takes its virtual views: {why}"),
            None,
        );
    }
}

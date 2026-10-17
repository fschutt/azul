//! A receipt's price column sits beside its labels under a full-width rule.
//!
//! The AzMail exploration (scripts/ideas/AZMAIL_EXPLORATION_2026_09_30.md,
//! section 1.3, sample 04) rendered a receipt whose right column - every
//! price - was missing, with its `<hr>` divider running past the table to the
//! window edge. Bisected to the probe `bis_D`: a `<td colspan="2"><hr></td>`
//! row followed by a two-cell row.
//!
//! An `<hr>` is `width: 100%` and holds no text, so its cell has no content
//! width of its own. The table measured that cell's min/max-content width
//! against a FINITE sentinel (`f32::MAX / 2`) that the containing-block type
//! read as a real length: the rule came out ~1.7e38 px wide, both spanned
//! columns ~0.85e38, and the price column started ~0.85e38 px to the right.
//!
//! Correct (CSS 2.2 section 17.5.2.2, css-sizing-3 section 5.2.1): a
//! percentage width against the indefinite measurement behaves as `auto`, so
//! the rule contributes nothing to the columns; the columns are as wide as
//! "Order ID" and "ML4X", and the rule fills the spanning cell - the two
//! columns - and no more.
//!
//! Not compiled by the author (house rule); expected RED.

use azul_core::{
    dom::{DomId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 600.0;

/// `bis_D.html`: a spanning divider row, then a label / price row.
const RECEIPT: &str = "<html><head></head><body><div><table>\
<tr><td colspan=\"2\"><hr/></td></tr>\
<tr><td>Order ID</td><td>ML4X</td></tr>\
</table></div></body></html>";

/// The receipt laid out in a `WIDTH` x `HEIGHT` window.
fn laid_out() -> (LayoutWindow, DisplayList) {
    let parsed = azul_layout::xml::parse_xml(RECEIPT).expect("the receipt parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the receipt lays out");
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    (lw, dl)
}

/// The border-box widths of every laid-out node of one element type, in
/// layout-tree order.
fn widths_of(lw: &LayoutWindow, is: fn(&NodeType) -> bool) -> Vec<f32> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let node_data = result.styled_dom.node_data.as_container();
    result
        .layout_tree
        .nodes
        .iter()
        .filter(|n| {
            n.dom_node_id
                .is_some_and(|id| is(node_data[id].get_node_type()))
        })
        .filter_map(|n| n.used_size.map(|s| s.width))
        .collect()
}

#[test]
fn the_price_column_is_painted_right_of_the_labels_inside_the_window() {
    let (_, dl) = laid_out();
    // (leftmost, rightmost) glyph x of every text run.
    let runs: Vec<(f32, f32)> = dl
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text { glyphs, .. } if !glyphs.is_empty() => Some((
                glyphs
                    .iter()
                    .map(|g| g.point.x)
                    .fold(f32::INFINITY, f32::min),
                glyphs
                    .iter()
                    .map(|g| g.point.x)
                    .fold(f32::NEG_INFINITY, f32::max),
            )),
            _ => None,
        })
        .collect();
    assert!(
        runs.len() >= 2,
        "the label and the price both paint a text run: {runs:?}"
    );
    let label = runs
        .iter()
        .copied()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("a label run");
    let price = runs
        .iter()
        .copied()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .expect("a price run");
    assert!(
        price.0 > label.1,
        "the price starts right of the label's last glyph: label {label:?}, price {price:?}"
    );
    assert!(
        price.1 < WIDTH,
        "the price is painted inside the {WIDTH} px window, not beside a runaway column: \
         {price:?}"
    );
}

#[test]
fn the_full_width_rule_spans_both_columns_and_no_further() {
    let (lw, _) = laid_out();
    let mut cells = widths_of(&lw, |t| matches!(t, NodeType::Td));
    let rules = widths_of(&lw, |t| matches!(t, NodeType::Hr));
    assert_eq!(cells.len(), 3, "three cells are laid out: {cells:?}");
    assert_eq!(rules.len(), 1, "one rule is laid out: {rules:?}");

    // The spanning cell is the widest; the other two sit below it.
    cells.sort_by(f32::total_cmp);
    let (label, price, spanning) = (cells[0], cells[1], cells[2]);
    assert!(
        spanning <= WIDTH,
        "the table fits its {WIDTH} px window: cells {cells:?}"
    );
    assert!(
        (spanning - (label + price)).abs() <= 4.0,
        "the spanning cell is as wide as the two cells below it (plus their spacing): \
         {cells:?}"
    );
    let rule = rules[0];
    assert!(
        rule <= spanning && rule >= spanning - 4.0,
        "the rule fills its cell's content box: a {rule} px rule in a {spanning} px cell"
    );
}

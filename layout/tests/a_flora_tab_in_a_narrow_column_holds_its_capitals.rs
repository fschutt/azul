//! A flora tab in a narrow column holds its capitals: AzCalculator's
//! History / Memory tabs, a 290px side column, under flora.
//!
//! FLORA16's screenshot review (`graph-flora-light` / `-dark`): the second
//! tab read "MEMOR" - its last capital cut - while flat fit. Flora sets a tab
//! label in EB Garamond capitals, 12px semibold tracked .08em (`tab_caps`),
//! a curve's width of padding on each side (`australis_tab_box`): the two
//! tabs need about 18 + 95 + 96 = 209px of the column's 288, so nothing has
//! to give. Chrome lays the same CSS out with every tab as wide as its
//! capitals (a flex item's base size is its max-content width, and its
//! min-content width - one word - is its floor). So the tab's box, and the
//! clip its text is painted in, must hold every capital of its label, on one
//! line.
//!
//! Both ways a widget gets its look are checked: pinned to flora in a flat
//! window, and following the app theme flora, as the calculator's tabs do.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    app_theme::ThemeScope,
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    widgets::{tabs::TabHeader, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The calculator's side column (`calc/ui.rs`: `width: 290px`).
const COLUMN: f32 = 290.0;
/// A flora tab's padding beside its label, border included
/// (`themes::flora::TAB_SIDE`, the curve's width).
const TAB_SIDE: f32 = 18.0;
const LABELS: [&str; 2] = ["History", "Memory"];

/// One painted glyph run: its glyphs' pen positions (x, baseline y), its
/// font size and its clip.
struct Run {
    glyphs: Vec<(f32, f32)>,
    font_size: f32,
    clip: LogicalRect,
}

/// The column, laid out: every tab's border box (in `LABELS` order) and
/// every text run painted.
struct Laid {
    tabs: Vec<LogicalRect>,
    runs: Vec<Run>,
}

/// The calculator's side panel - a column of the tab row over the tape - in
/// a `COLUMN`-wide box, the tab row built by `header` and laid out in a
/// window whose app theme is `app_theme`.
fn laid_out(app_theme: &str, header: impl FnOnce() -> Dom) -> Laid {
    let dom = {
        let _scope = ThemeScope::enter(AzString::from(app_theme));
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css(&format!("position: relative; width: {COLUMN}px; height: 300px;"))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "position: absolute; top: 0px; right: 0px; bottom: 0px; left: 0px; \
                             display: flex; flex-direction: column; min-width: 0px; \
                             min-height: 0px; border-radius: 3px; overflow: hidden; \
                             background: #F2F1ED; border: 1px solid #D8D5CE;",
                        )
                        .with_child(header())
                        .with_child(
                            Dom::create_div().with_css("flex-grow: 1; background: #FBFAF6;"),
                        ),
                ),
        )
    };
    let styled = StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    lw.app_theme = AzString::from(app_theme);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 320.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the column lays out");

    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("the root DOM is laid out");
    let nodes = result.styled_dom.node_data.as_container();
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let tabs = LABELS
        .iter()
        .map(|label| {
            let text = (0..nodes.len())
                .find(|i| {
                    matches!(
                        nodes[NodeId::new(*i)].get_node_type(),
                        NodeType::Text(t) if t.as_str() == *label
                    )
                })
                .unwrap_or_else(|| panic!("the {label} tab's text"));
            let tab = hierarchy[NodeId::new(text)]
                .parent_id()
                .unwrap_or_else(|| panic!("the {label} tab"));
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(tab)),
            })
            .unwrap_or_else(|| panic!("the {label} tab is laid out"))
        })
        .collect();
    let runs = result
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                clip_rect,
                ..
            } => Some(Run {
                glyphs: glyphs.iter().map(|g| (g.point.x, g.point.y)).collect(),
                font_size: *font_size_px,
                clip: clip_rect.0,
            }),
            _ => None,
        })
        .collect();
    Laid { tabs, runs }
}

/// Every tab holds its label: the glyphs painted inside its border box are
/// its whole label, on one baseline, and the last one starts at least
/// 0.4em (narrower than any capital) before the right edge of the tab's
/// content box and of its run's clip.
fn assert_every_tab_holds_its_capitals(case: &str, laid: &Laid) {
    for (label, tab) in LABELS.iter().zip(&laid.tabs) {
        let (left, right) = (tab.origin.x, tab.origin.x + tab.size.width);
        let mine: Vec<(&Run, (f32, f32))> = laid
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(move |g| (run, *g)))
            .filter(|(_, (x, _))| *x >= left && *x < right)
            .collect();
        assert_eq!(
            mine.len(),
            label.chars().count(),
            "{case}: the {label} tab paints its whole label inside its box {tab:?} ({} glyphs)",
            mine.len()
        );
        let (_, (_, first_y)) = mine[0];
        assert!(
            mine.iter().all(|(_, (_, y))| (y - first_y).abs() < 0.5),
            "{case}: the {label} tab's label is one line: {:?}",
            mine.iter().map(|(_, g)| *g).collect::<Vec<_>>()
        );
        let (run, (last_x, _)) = mine[mine.len() - 1];
        let ink_end = last_x + 0.4 * run.font_size;
        let content_right = right - TAB_SIDE;
        assert!(
            ink_end <= content_right + 0.5,
            "{case}: the {label} tab's content box ends at {content_right:.2}, its last capital \
             starts at {last_x:.2} - the box is narrower than its capitals (tab {tab:?})"
        );
        let clip_right = run.clip.origin.x + run.clip.size.width;
        assert!(
            ink_end <= clip_right + 0.5,
            "{case}: the {label} tab's text is clipped at {clip_right:.2}, its last capital starts \
             at {last_x:.2}"
        );
        assert!(
            right <= COLUMN + 0.5,
            "{case}: the {label} tab ends inside the column: {right:.2} (tabs {:?}, glyph x of each \
             run {:?})",
            laid.tabs,
            laid.runs.iter().map(|r| r.glyphs.iter().map(|g| g.0).collect::<Vec<_>>()).collect::<Vec<_>>()
        );
    }
}

fn labels() -> StringVec {
    StringVec::from_vec(LABELS.iter().map(|l| AzString::from(*l)).collect())
}

#[test]
fn a_flora_tab_in_a_narrow_column_holds_its_capitals() {
    let pinned = laid_out("flat", || {
        TabHeader::create(labels())
            .with_active_tab(0)
            .with_theme(UiTheme::Flora)
            .dom()
    });
    assert_every_tab_holds_its_capitals("pinned to flora", &pinned);

    let following = laid_out("flora", || TabHeader::create(labels()).with_active_tab(0).dom());
    assert_every_tab_holds_its_capitals("following the app theme flora", &following);
}

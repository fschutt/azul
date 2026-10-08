//! A flora tab row, PAINTED: its gold is flora.css's metal - one ribbon in
//! three values, as on the website - and not one flat colour.
//!
//! The user, on the flora tabs: "the gold doesn't properly repeat and fade in
//! / out like on the site". The rule closing the strip is `--fl-rule-metal-bg`
//! (brass at half alpha at both ends of the strip, the glint at a third and
//! at two thirds, a darker roll between); the selected tab's metal is
//! `--fl-rolled-tab` (lit at its head, turning to `--fl-metal-turn` toward the
//! foot), the S of each curve cut from the same bead, the rule eased into the
//! turn colour beside each foot; and the tab is open at its foot - its stone
//! covers the rule, no metal line under it. Every one of those used to be
//! #C6B279.
//!
//! The probes sit on the strip's 2px foot (the rule), on the selected tab's
//! head and under it, at places the layout gives - nothing depends on the
//! machine's fonts beyond where the tabs end.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    widgets::{tabs::TabHeader, themes::UiTheme},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 60.0;

/// The painted tab row and where its strip and its selected tab are.
struct Row {
    pixels: Vec<u8>,
    width: usize,
    strip: LogicalRect,
    tab: LogicalRect,
}

impl Row {
    fn rgb(&self, x: f32, y: f32) -> (u8, u8, u8) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (x.floor() as usize, y.floor() as usize);
        let i = (y * self.width + x) * 4;
        (self.pixels[i], self.pixels[i + 1], self.pixels[i + 2])
    }

    /// The row of pixels along the middle of the strip's 2px foot.
    fn rule_y(&self) -> f32 {
        self.strip.origin.y + self.strip.size.height - 1.0
    }
}

fn close(a: (u8, u8, u8), b: (u8, u8, u8), tolerance: u8) -> bool {
    a.0.abs_diff(b.0) <= tolerance && a.1.abs_diff(b.1) <= tolerance && a.2.abs_diff(b.2) <= tolerance
}

/// `rgba(r, g, b, a)` over white.
fn over_white((r, g, b): (f32, f32, f32), a: f32) -> (u8, u8, u8) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mix = |c: f32| (c * a + 255.0 * (1.0 - a)).round() as u8;
    (mix(r), mix(g), mix(b))
}

/// `One | Two | Three | Four`, the second one selected, in flora, on a white
/// page in a 900 x 60 window.
fn row() -> Row {
    let labels = StringVec::from_vec(
        ["One", "Two", "Three", "Four"]
            .iter()
            .map(|s| AzString::from(*s))
            .collect(),
    );
    let header = TabHeader::create(labels)
        .with_active_tab(1)
        .with_theme(UiTheme::Flora)
        .dom();
    let dom = Dom::create_body()
        .with_css("margin: 0; background: #ffffff;")
        .with_child(header);
    let styled = StyledDom::create_from_dom(dom);
    let with_class = |name: &str| {
        styled
            .node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
            })
            .unwrap_or_else(|| panic!("the row has a {name}"))
    };
    let (strip, tab) = (
        with_class("__azul-native-tabs-header"),
        with_class("__azul-native-tabs-tab-active"),
    );

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .expect("the row lays out");
    let rect = |n: usize| {
        lw.get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
        })
        .expect("laid out")
    };
    let (strip, tab) = (rect(strip), rect(tab));
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref()
        .clone();
    let mut gc = GlyphCache::new();
    let pm = cpurender::render_with_font_manager(
        &dl,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut gc,
    )
    .expect("the row paints");
    Row {
        pixels: pm.data().to_vec(),
        width: pm.width() as usize,
        strip,
        tab,
    }
}

#[test]
fn the_rule_fades_to_half_brass_at_both_ends_and_glints_along_the_strip() {
    let row = row();
    assert!(
        (row.strip.size.width - WIDTH).abs() < 0.5 && (row.strip.size.height - 32.0).abs() < 0.5,
        "the strip runs the window's width, 32px tall: {:?}",
        row.strip
    );
    let y = row.rule_y();
    // Both ends: `rgba(122, 112, 82, 0.5)` over the white page.
    let end = over_white((122.0, 112.0, 82.0), 0.5);
    for x in [1.0, WIDTH - 2.0] {
        assert!(
            close(row.rgb(x, y), end, 8),
            "the rule fades out at x = {x}: {:?}, expected about {end:?}",
            row.rgb(x, y)
        );
    }
    // The glint at two thirds, `rgba(239, 235, 211, 1)`, and the darker roll
    // at the middle, `rgba(162, 146, 95, 0.9)`, under unselected tabs or past
    // them: the rule runs under every tab but the selected one.
    let glint = (239, 235, 211);
    let x = WIDTH * 0.68;
    assert!(
        close(row.rgb(x, y), glint, 8),
        "the rule glints at two thirds: {:?}",
        row.rgb(x, y)
    );
    let roll = over_white((162.0, 146.0, 95.0), 0.9);
    let x = WIDTH * 0.5;
    assert!(
        close(row.rgb(x, y), roll, 8),
        "the rule rolls darker at the middle: {:?}, expected about {roll:?}",
        row.rgb(x, y)
    );
}

#[test]
fn the_selected_tab_is_lit_along_its_head_and_open_at_its_foot() {
    let row = row();
    let mid = row.tab.origin.x + row.tab.size.width / 2.0;
    // `--fl-rolled-tab` at its head: #FFFDF3 rolling toward #E4DCB8 - pale
    // cream, nothing like the turn colour #C6B279 it reaches at the feet.
    let head = row.rgb(mid, row.tab.origin.y);
    assert!(
        close(head, (253, 250, 238), 8),
        "the head is the lit bead: {head:?}"
    );
    // The foot: the stone reaches over the strip's rule and hides it - the
    // tab opens onto what lies below with no metal line under it.
    let foot = row.rgb(mid, row.rule_y());
    assert!(
        u16::from(foot.2) > u16::from(foot.0) + 40 && foot.0 < 150,
        "the stone, not the metal, under the selected tab: {foot:?}"
    );
}

#[test]
fn the_rule_turns_into_the_s_in_the_turn_colour_beside_each_foot() {
    let row = row();
    let y = row.rule_y();
    let turn = (198, 178, 121);
    let curve = 18.0;
    for (x, what) in [
        (row.tab.origin.x - curve, "the left S's foot"),
        (row.tab.origin.x - curve - 1.0, "the left run-out's end"),
        (row.tab.origin.x + row.tab.size.width + curve - 1.0, "the right S's foot"),
        (row.tab.origin.x + row.tab.size.width + curve, "the right run-out's end"),
    ] {
        assert!(
            close(row.rgb(x, y), turn, 10),
            "{what} (x = {x}) is the turn colour: {:?}",
            row.rgb(x, y)
        );
    }
}

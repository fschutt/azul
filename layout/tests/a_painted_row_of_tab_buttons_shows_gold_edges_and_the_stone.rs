//! A row of tab buttons, PAINTED by the CPU renderer at 1x: every button is
//! edged in gold, lighter on top (#DCC17C) than at its foot (#A3873F), as in
//! the design's shots; the selected one is the accent stone (blue), the
//! others a pale face. The probes sit on the borders and beside the label,
//! where the layout puts them - nothing depends on the fonts beyond where
//! the buttons end.

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
    widgets::{
        tabs::{TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 40.0;

struct Painted {
    pixels: Vec<u8>,
    width: usize,
    selected: LogicalRect,
    plain: LogicalRect,
}

impl Painted {
    fn rgb(&self, x: f32, y: f32) -> (u8, u8, u8) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (x.floor() as usize, y.floor() as usize);
        let i = (y * self.width + x) * 4;
        (self.pixels[i], self.pixels[i + 1], self.pixels[i + 2])
    }
}

fn close(a: (u8, u8, u8), b: (u8, u8, u8), tolerance: u8) -> bool {
    a.0.abs_diff(b.0) <= tolerance && a.1.abs_diff(b.1) <= tolerance && a.2.abs_diff(b.2) <= tolerance
}

fn paint() -> Painted {
    let labels = StringVec::from_vec(
        ["File", "Home", "Share", "View"]
            .iter()
            .map(|s| AzString::from(*s))
            .collect(),
    );
    let header = TabHeader::create(labels)
        .with_active_tab(1)
        .with_theme(UiTheme::Flora)
        .with_style(TabStyle::Pills)
        .dom();
    let dom = Dom::create_body()
        .with_css("margin: 0; background: #ffffff;")
        .with_child(header);
    let styled = StyledDom::create_from_dom(dom);
    let nth_with = |name: &str, nth: usize| {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
            })
            .nth(nth)
            .map(|(i, _)| i)
            .unwrap_or_else(|| panic!("the row has a {name}"))
    };
    let selected = nth_with("__azul-native-tabs-tab-active", 0);
    // The last unselected button: "View".
    let plain = nth_with("__azul-native-tabs-tab-not-active", 2);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut None)
        .expect("the row lays out");
    let rect = |n: usize| {
        lw.get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
        })
        .expect("laid out")
    };
    let (selected, plain) = (rect(selected), rect(plain));
    let dl = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
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
    Painted {
        pixels: pm.data().to_vec(),
        width: pm.width() as usize,
        selected,
        plain,
    }
}

#[test]
fn every_tab_button_is_edged_in_gold_lighter_on_top_than_at_its_foot() {
    let p = paint();
    for button in [p.plain, p.selected] {
        // The middle of the top and the bottom border, 4px in from the left
        // (clear of the 3px corner and of the label).
        let x = button.origin.x + 4.5;
        let top = p.rgb(x, button.origin.y + 0.5);
        let foot = p.rgb(x, button.origin.y + button.size.height - 0.5);
        assert!(close(top, (0xDC, 0xC1, 0x7C), 14), "the gold on top of {button:?}: {top:?}");
        assert!(close(foot, (0xA3, 0x87, 0x3F), 14), "the gold at the foot of {button:?}: {foot:?}");
    }
}

#[test]
fn the_selected_tab_is_the_blue_stone_and_the_others_a_pale_face() {
    let p = paint();
    let beside_label = |b: &LogicalRect| p.rgb(b.origin.x + 5.5, b.origin.y + b.size.height / 2.0);
    let (r, g, b) = beside_label(&p.selected);
    assert!(
        u16::from(b) > u16::from(r) + 30 && b > g,
        "the selected tab is the accent stone: {:?}",
        (r, g, b)
    );
    let (r, g, b) = beside_label(&p.plain);
    assert!(r > 200 && g > 200 && b > 190, "an unselected tab is a pale face: {:?}", (r, g, b));
}

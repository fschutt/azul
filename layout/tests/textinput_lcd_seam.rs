//! Pins for the 2026-08-31 TextInput repaint seam - its OWN test binary
//! because it forces the macOS glyph mode (`AZ_TEXT_HINTING=0`, hinting
//! off => hinting ppem 0) process-wide before the first glyph is rendered,
//! which must not leak into the shared `all` binary.
//!
//! - A. An incremental repaint of the CARET BAND (1 logical px wide, the
//!   damage a caret move / blink produces) must be pixel-identical to a full
//!   repaint. The LCD per-glyph cull used the HINTING ppem as its ink bound;
//!   with hinting off that bound is 0, and every glyph whose pen sat left of
//!   the band was dropped - a white notch through the text at every caret
//!   position, cut runs, stray glyphs.
//! - B. The caret paints OVER the text: emitted before the glyphs, an opaque
//!   LCD pre-blended tile erased the caret segment inside the last glyph's
//!   box.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::TextCursor,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayListItem,
    widgets::text_input::TextInput,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > container(1) > label-p(2) > text(3). The prompt is an
/// ATTRIBUTE on the value line, not a node (2026-08-31).
const CONTAINER: usize = 1;
const LABEL_P: usize = 2;

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

/// Must run before ANY glyph is rendered in this process (read-once caches).
fn force_macos_glyph_mode() {
    std::env::set_var("AZ_TEXT_HINTING", "0");
}

struct Harness {
    glyph_cache: GlyphCache,
    lw: LayoutWindow,
    renderer_resources: RendererResources,
    window_state: FullWindowState,
}

impl Harness {
    fn new(width: f32, height: f32, text: &str) -> Self {
        let mut dom =
            Dom::create_body().with_child(TextInput::create().with_text(AzString::from(text)).dom());
        let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
        lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = window_state.clone();
        let renderer_resources = RendererResources::default();
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let mut dbg = Some(Vec::new());
        lw.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();
        Self {
            glyph_cache: GlyphCache::new(),
            lw,
            renderer_resources,
            window_state,
        }
    }

    fn end_cursor(&self) -> TextCursor {
        let tree = &self
            .lw
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .layout_tree;
        let idx = tree
            .dom_to_layout
            .get(&NodeId::new(LABEL_P))
            .and_then(|v| v.first())
            .expect("label <p> has a layout box");
        tree.materialized_inline_layout_for_node(idx.index())
            .expect("label <p> establishes an inline layout")
            .get_last_cluster_cursor()
            .expect("the label has at least one cluster")
    }

    fn start_editing_at_end(&mut self) {
        let end = self.end_cursor();
        self.lw
            .focus_manager
            .set_focused_node(Some(dnid(CONTAINER)));
        self.lw.text_edit_manager.initialize_editing(
            end,
            DomId::ROOT_ID,
            NodeId::new(LABEL_P),
            0,
        );
        self.lw.text_edit_manager.blink.set_visibility(true);
        self.lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    }

    fn dl(&self) -> std::sync::Arc<azul_layout::solver3::display_list::DisplayList> {
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .clone()
    }

    fn render(&mut self, dpi: f32) -> AzulPixmap {
        let dl = self.dl();
        let opts = RenderOptions {
            width: self.window_state.size.dimensions.width,
            height: self.window_state.size.dimensions.height,
            dpi_factor: dpi,
        };
        cpurender::render_with_font_manager(
            &dl,
            &self.renderer_resources,
            &self.lw.font_manager,
            opts,
            &mut self.glyph_cache,
        )
        .unwrap()
    }

    fn render_damaged(&mut self, pixmap: &mut AzulPixmap, dpi: f32, damage: &[LogicalRect]) {
        let dl = self.dl();
        let state = cpurender::CpuRenderState::new(Default::default());
        cpurender::render_display_list_damaged(
            &dl,
            pixmap,
            dpi,
            &self.renderer_resources,
            &self.lw.font_manager,
            &mut self.glyph_cache,
            &state,
            damage,
        )
        .unwrap();
    }

    fn caret(&self) -> Option<(LogicalRect, azul_css::props::basic::ColorU)> {
        self.dl().items.iter().find_map(|item| match item {
            DisplayListItem::CursorRect { bounds, color, .. } if color.a > 0 => {
                Some((bounds.0, *color))
            }
            _ => None,
        })
    }
}

fn pixel_diff_count(a: &AzulPixmap, b: &AzulPixmap) -> usize {
    let (ad, bd) = (a.data(), b.data());
    (0..ad.len())
        .step_by(4)
        .filter(|&i| ad[i] != bd[i] || ad[i + 1] != bd[i + 1] || ad[i + 2] != bd[i + 2])
        .count()
}

#[test]
fn a_caret_band_repaint_is_pixel_identical_to_a_full_repaint() {
    force_macos_glyph_mode();
    let dpi = 2.0;
    let mut h = Harness::new(300.0, 60.0, "asdfasdfasdfasdf");
    h.start_editing_at_end();
    let (caret_rect, _) = h.caret().expect("the focused field paints a caret");

    // The frame everyone else painted...
    let mut incremental = h.render(dpi);
    // ...then the damage a caret move / blink produces: the caret's own
    // rect, 1 logical px wide. No glyph starts inside it, so the old cull
    // (ink bound 0 with hinting off) rasterised NOTHING there and left a
    // white notch through the run.
    h.render_damaged(&mut incremental, dpi, &[caret_rect]);
    let full = h.render(dpi);

    let diff = pixel_diff_count(&incremental, &full);
    assert_eq!(
        diff, 0,
        "repainting the caret band must reproduce the full frame; {diff} px differ \
         (caret rect {caret_rect:?})"
    );
}

#[test]
fn the_caret_paints_over_the_last_glyph() {
    force_macos_glyph_mode();
    let dpi = 2.0;
    let mut h = Harness::new(300.0, 60.0, "ljölkjöljöljölk");
    h.start_editing_at_end();
    let (caret_rect, caret_color) = h.caret().expect("caret");
    let full = h.render(dpi);

    // Sample the caret's centre column across its whole height: every row
    // must be the caret colour. Painted under the glyphs, the opaque LCD
    // tile of the last glyph overwrote the rows inside its box.
    let w = full.width() as usize;
    let data = full.data();
    let cx = ((caret_rect.origin.x + caret_rect.size.width / 2.0) * dpi) as usize;
    let y0 = (caret_rect.origin.y * dpi).ceil() as usize + 1;
    let y1 = ((caret_rect.origin.y + caret_rect.size.height) * dpi).floor() as usize - 1;
    let mut wrong = Vec::new();
    for y in y0..y1 {
        let i = (y * w + cx) * 4;
        let px = (data[i], data[i + 1], data[i + 2]);
        if px != (caret_color.r, caret_color.g, caret_color.b) {
            wrong.push((y, px));
        }
    }
    assert!(
        wrong.is_empty(),
        "caret column x={cx} must be the caret colour on every row {y0}..{y1}; wrong rows: {wrong:?}"
    );
}

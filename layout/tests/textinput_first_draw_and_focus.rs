//! Pins for the 2026-08-31 TextInput device report:
//!
//! - A. FIRST DRAW: the placeholder rendered as a ~3px strip of glyph tops.
//!   Every text run's clip in the FIRST display list must be at least a line
//!   tall, and a no-op relayout must not change any text clip or any pixel -
//!   the first frame is not allowed to differ from the settled one.
//! - B. FOCUS THEN RELAYOUT: a caret session created through the real focus
//!   path must survive a relayout. The display-list cache key ignored caret /
//!   selection inputs, so the relayout served the pre-caret list verbatim.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
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

const CONTAINER: usize = 1;

fn dnid(node: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
    }
}

struct Harness {
    glyph_cache: GlyphCache,
    lw: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
}

impl Harness {
    fn new_empty_with_placeholder(width: f32, height: f32) -> Self {
        let mut dom = Dom::create_body().with_child(
            TextInput::create()
                .with_placeholder("Type something".into())
                .dom(),
        );
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
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state,
        }
    }

    /// A production-shaped relayout: the OLD layout result stays in place until
    /// the new one replaces it (that is what the shells do), so state that is
    /// resolved against the previous result - `caret_editable_is_focused`
    /// walks the hierarchy through it - keeps working mid-pass.
    fn relayout(&mut self) {
        let Some(styled_dom) = self
            .lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .map(|lr| lr.styled_dom.clone())
        else {
            return;
        };
        let mut dbg = Some(Vec::new());
        self.lw
            .layout_and_generate_display_list(
                styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .unwrap();
    }

    fn text_clips(&self) -> Vec<LogicalRect> {
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::Text { clip_rect, glyphs, .. } if !glyphs.is_empty() => {
                    Some(clip_rect.0)
                }
                _ => None,
            })
            .collect()
    }

    fn caret_count(&self) -> usize {
        self.lw
            .get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .items
            .iter()
            .filter(|item| matches!(item, DisplayListItem::CursorRect { color, .. } if color.a > 0))
            .count()
    }

    fn render(&mut self) -> AzulPixmap {
        let dl = self
            .lw
            .get_layout_result(&DomId::ROOT_ID)
            .unwrap()
            .display_list
            .clone();
        let opts = RenderOptions {
            width: self.window_state.size.dimensions.width,
            height: self.window_state.size.dimensions.height,
            dpi_factor: 1.0,
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
}

fn pixel_diff_count(a: &AzulPixmap, b: &AzulPixmap) -> usize {
    let (ad, bd) = (a.data(), b.data());
    (0..ad.len())
        .step_by(4)
        .filter(|&i| ad[i] != bd[i] || ad[i + 1] != bd[i + 1] || ad[i + 2] != bd[i + 2])
        .count()
}

#[test]
fn the_first_frame_paints_the_placeholder_like_the_settled_frame() {
    let mut h = Harness::new_empty_with_placeholder(400.0, 120.0);
    let first_clips = h.text_clips();
    assert!(!first_clips.is_empty(), "the placeholder run is in the first list");
    for c in &first_clips {
        assert!(
            c.size.height >= 8.0,
            "a text run's clip must be at least a line tall on the FIRST layout, got {c:?}"
        );
    }
    let first = h.render();

    h.relayout();
    let settled_clips = h.text_clips();
    assert_eq!(
        first_clips, settled_clips,
        "a no-op relayout must not move or resize any text clip"
    );
    let settled = h.render();
    assert_eq!(
        pixel_diff_count(&first, &settled),
        0,
        "the first frame must be pixel-identical to the settled one"
    );
}

#[test]
fn a_caret_created_by_the_real_focus_path_survives_a_relayout() {
    let mut h = Harness::new_empty_with_placeholder(400.0, 120.0);
    assert_eq!(h.caret_count(), 0, "premise: no caret before focus");

    // The click's focus path, without initialize_editing shortcuts.
    h.lw.focus_manager.set_focused_node(Some(dnid(CONTAINER)));
    let ws = h.window_state.clone();
    let _ = h
        .lw
        .handle_focus_change_for_cursor_blink(Some(dnid(CONTAINER)), &ws);
    assert!(
        h.lw.finalize_pending_focus_changes(),
        "the deferred focus becomes an editing session"
    );

    // What the shells do next: a relayout. The old cache key knew nothing
    // about the caret and served the pre-focus list.
    h.relayout();
    assert!(
        h.caret_count() > 0,
        "the relayout after focusing must emit the caret"
    );
}

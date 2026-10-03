//! The animation channel of the GPU value cache holds only what an
//! animation drives: a slide that is dropped, or re-keyed to another node,
//! takes its transform with it.
//!
//! `tick_animations` writes each slide's transform under its node in the
//! ANIMATION channel and released it only when the slide FINISHED, through
//! `anim_key_to_node`. A rebuild mid-slide (a theme or mode switch while the
//! backstage slides out) rebuilds that map wholesale and drops the slides
//! whose node it cannot place - and the values they had written stayed in
//! the cache for good: a reference frame with a stranger's offset, painted
//! by every frame after. Found on the prebuilt AzDrive (FILE > Options >
//! Appearance > Flora / Flat, Escape, then `set_theme flora` +
//! `set_mode dark` without waiting): settled, with no animation left, the
//! screenshot's transform map still held 15 values (-658 px, -649 px, ...)
//! and the address bar's back button sat in the window's corner while the
//! search box was gone (MEETDRIVE6 item 7 d: "pieces of the closed backstage
//! stay painted over the ribbon"; SHEETSHOW6's ghost labels after a theme
//! switch).

use azul_core::{
    animation::{AnimKey, FlipTransform, InterpolationMode},
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > first(1), second(2).
const FIRST: NodeId = NodeId::new(1);
const SECOND: NodeId = NodeId::new(2);
const KEY: AnimKey = AnimKey(7);

/// `FIRST` mid-slide, its transform published.
fn sliding() -> LayoutWindow {
    let page = Dom::create_body()
        .with_child(Dom::create_div().with_css("height: 20px; background: red;"))
        .with_child(Dom::create_div().with_css("height: 20px; background: blue;"));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    let flip = FlipTransform {
        translate_x: -300.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
    };
    lw.animations
        .start_or_retarget_move(KEY, flip, InterpolationMode::default());
    lw.anim_key_to_node.insert(KEY, FIRST);
    let _ = lw.tick_animations(0.0);
    assert!(
        holds(&lw, FIRST),
        "harness: the slide's transform is published"
    );
    lw
}

fn holds(lw: &LayoutWindow, node: NodeId) -> bool {
    lw.gpu_state_manager
        .caches
        .get(&DomId::ROOT_ID)
        .is_some_and(|c| {
            c.anim_current_transform_values.contains_key(&node)
                || c.anim_transform_keys.contains_key(&node)
        })
}

#[test]
fn a_dropped_slide_takes_its_transform_with_it() {
    let mut lw = sliding();
    // The rebuild could not place the slide's node and dropped the slide.
    let _ = lw.animations.cancel(KEY);
    let _ = lw.tick_animations(0.016);
    assert!(
        !holds(&lw, FIRST),
        "no animation drives the node: its reference frame must go, not keep the last offset"
    );
}

#[test]
fn a_slide_re_keyed_to_another_node_leaves_nothing_behind() {
    let mut lw = sliding();
    // The rebuild matched the slide's identity to another node.
    lw.anim_key_to_node.insert(KEY, SECOND);
    let _ = lw.tick_animations(0.016);
    assert!(holds(&lw, SECOND), "the slide now drives the other node");
    assert!(
        !holds(&lw, FIRST),
        "the node the slide left keeps no transform of it"
    );
}

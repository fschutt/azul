//! A solid background's colour fade is a property value per frame, not a new
//! display list; a fade the list carries in place is owed to WebRender.
//!
//! The frames of a colour fade patched the display list in place: the CPU
//! renderer found the patched rect in its diff, but the GPU path took its
//! "nothing was rebuilt" transaction and never sent the patched list, and a
//! list per frame would make every frame of the fade a scene build anyway.
//! Now the first frame binds the box's background rect to a colour key
//! (`DisplayList::color_bindings`) and every later frame changes only the
//! key's value: WebRender's colour binding of the rect, the CPU renderer's
//! bound colour, damage of the rect alone. A gradient face cannot be bound
//! (WebRender binds the colour of a rect only): its patched list is owed to
//! the renderer (`GpuStateManager::display_lists_unsubmitted`).

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    events::ProcessEventResult,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::ColorU,
    property::CssProperty,
    style::{StyleBackgroundContent, StyleBackgroundContentVec},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{
        color_value_damage, extract_gpu_colors, extract_gpu_values, item_with_bound_color,
        CpuRenderState, ScrollOffsetMap,
    },
    overlay::ContentChange,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const REST: ColorU = ColorU {
    r: 0xee,
    g: 0xee,
    b: 0xee,
    a: 0xff,
};
const LIT: ColorU = ColorU {
    r: 0x33,
    g: 0x66,
    b: 0xff,
    a: 0xff,
};

fn window_with(css: &str) -> (LayoutWindow, NodeId) {
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div().with_css("height: 40px; background: #ffcc00;"))
        .with_child(Dom::create_div().with_class("face".into()).with_css(css));
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 200.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("the page lays out");
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = sd.node_data.as_container();
    let face = (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == "face"))
        })
        .expect("the face exists");
    (lw, face)
}

fn list(lw: &LayoutWindow) -> Arc<DisplayList> {
    lw.layout_results[&DomId::ROOT_ID].display_list.clone()
}

/// What the renderers fill `node`'s box with, and the rect: the display
/// list's rect in its bound colour.
fn painted(lw: &LayoutWindow, node: NodeId) -> Option<(ColorU, azul_core::geom::LogicalRect)> {
    let dl = list(lw);
    let state = CpuRenderState::from_gpu_cache(
        lw.gpu_state_manager.get_cache(DomId::ROOT_ID),
        DomId::ROOT_ID,
        &ScrollOffsetMap::new(),
    );
    dl.items.iter().enumerate().find_map(|(i, item)| {
        if dl.node_mapping.get(i).copied().flatten() != Some(node) {
            return None;
        }
        match &*item_with_bound_color(item, dl.color_binding(i), &state) {
            DisplayListItem::Rect { color, bounds, .. } => Some((*color, *bounds.inner())),
            _ => None,
        }
    })
}

fn solid(c: ColorU) -> CssProperty {
    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
        StyleBackgroundContent::Color(c),
    ]))
}

/// The frame driver's work, as the shells do it: a frame that owes a
/// rebuild rebuilds the list.
fn frame_work(lw: &mut LayoutWindow) -> ProcessEventResult {
    let work = lw.take_animation_frame_work();
    if work == ProcessEventResult::ShouldUpdateDisplayListCurrentWindow {
        lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    }
    work
}

/// WebRender receives the lists, as `set_display_lists` records them.
fn submit(lw: &mut LayoutWindow) {
    let lists: Vec<(DomId, Arc<DisplayList>)> = lw
        .layout_results
        .iter()
        .map(|(d, r)| (*d, r.display_list.clone()))
        .collect();
    lw.gpu_state_manager
        .note_display_lists_submitted(lists.iter().map(|(d, l)| (*d, l)));
}

fn unsubmitted(lw: &LayoutWindow) -> bool {
    lw.gpu_state_manager.display_lists_unsubmitted(
        lw.layout_results
            .iter()
            .map(|(d, r)| (*d, &r.display_list)),
    )
}

/// An N-frame fade of a solid background sends WebRender one display list
/// (the frame that binds the rect) and a colour per frame; the CPU frame of
/// each later step damages the box's rect and nothing else.
#[test]
fn an_n_frame_background_fade_sends_one_display_list_and_n_colours() {
    let (mut lw, face) = window_with(
        "width: 80px; height: 24px; background: #eeeeee; animation: background 200ms linear;",
    );
    submit(&mut lw);
    assert_eq!(painted(&lw, face).map(|p| p.0), Some(REST), "harness: the face rests");
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: face,
        props: vec![solid(LIT)],
        override_only: false,
    });
    assert_eq!(lw.css_transitions.len(), 1, "harness: a tween for the background");

    let mut lists_sent = 0usize;
    let mut colours_sent = 0usize;
    let mut frames = 0usize;
    let mut previous_colours = extract_gpu_colors(lw.gpu_state_manager.get_cache(DomId::ROOT_ID));
    let mut previous_paint = painted(&lw, face).expect("the face paints").0;
    for frame in 0..200 {
        if lw.css_transitions.is_empty() {
            break;
        }
        frames += 1;
        let before = list(&lw);
        let _ = lw.tick_animations(0.016);
        let work = frame_work(&mut lw);
        assert_eq!(
            work,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "frame {frame}: a background fade frame owes a repaint (pending css dirt: {:?})",
            lw.pending_css_dirty
        );
        if unsubmitted(&lw) {
            lists_sent += 1;
            submit(&mut lw);
        }
        let colours = extract_gpu_colors(lw.gpu_state_manager.get_cache(DomId::ROOT_ID));
        if colours != previous_colours {
            colours_sent += 1;
        }
        let (paint, rect) = painted(&lw, face).expect("the face paints");
        if frame > 0 {
            assert!(
                Arc::ptr_eq(&before, &list(&lw)),
                "frame {frame}: the fade changed the display list - a later frame of a bound \
                 background is a colour value"
            );
            if paint != previous_paint {
                let (transforms, _) =
                    extract_gpu_values(lw.gpu_state_manager.get_cache(DomId::ROOT_ID), DomId::ROOT_ID);
                let damage = color_value_damage(
                    &list(&lw),
                    &previous_colours,
                    &colours,
                    &transforms,
                    &ScrollOffsetMap::new(),
                );
                assert!(!damage.needs_full, "frame {frame}: a colour step needs no full repaint");
                assert_eq!(
                    damage.rects,
                    vec![rect],
                    "frame {frame}: the CPU frame damages the face's rect and nothing else"
                );
            }
        }
        previous_colours = colours;
        previous_paint = paint;
    }
    assert!(frames >= 8, "harness: a 200 ms fade runs many frames, ran {frames}");
    assert_eq!(
        lists_sent, 1,
        "the fade sent WebRender {lists_sent} display lists in {frames} frames - the one that \
         binds the rect is all it needs"
    );
    assert!(
        colours_sent + 1 >= frames,
        "every frame after the first sends its colour: {colours_sent} colours in {frames} frames"
    );
    assert_eq!(
        painted(&lw, face).map(|p| p.0),
        Some(LIT),
        "the face is painted in its hover colour once the fade is over"
    );
    // A list built later (a caret blink, a relayout) drops the key and
    // paints the settled colour itself.
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let dl = list(&lw);
    assert!(
        dl.color_bindings.iter().all(Option::is_none),
        "a list built after the fade binds nothing"
    );
    assert_eq!(painted(&lw, face).map(|p| p.0), Some(LIT));
}

/// A gradient face's fade patches the list in place (it cannot be bound): the
/// patched list is owed to WebRender on the frame that rebuilt nothing.
#[test]
fn a_list_patched_in_place_is_owed_to_webrender() {
    let (mut lw, face) = window_with(
        "width: 80px; height: 24px; background: linear-gradient(to bottom, #eeeeee, #cccccc); \
         animation: background 150ms linear;",
    );
    submit(&mut lw);
    assert!(!unsubmitted(&lw), "harness: WebRender has the list");
    let mut lit = list(&lw)
        .items
        .iter()
        .find_map(|i| match i {
            DisplayListItem::LinearGradient { gradient, .. } => Some(gradient.clone()),
            _ => None,
        })
        .expect("harness: the face paints a gradient");
    let mut stops = lit.stops.as_ref().to_vec();
    for stop in &mut stops {
        stop.color = azul_css::props::basic::color::ColorOrSystem::color(LIT);
    }
    lit.stops = azul_css::props::style::NormalizedLinearColorStopVec::from_vec(stops);
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: face,
        props: vec![CssProperty::const_background_content(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::LinearGradient(lit)]),
        )],
        override_only: false,
    });
    for frame in 0..3 {
        let _ = lw.tick_animations(0.016);
        let work = frame_work(&mut lw);
        assert_eq!(work, ProcessEventResult::ShouldReRenderCurrentWindow, "harness: patched");
        assert!(
            unsubmitted(&lw),
            "frame {frame}: the gradient was patched into the list - WebRender does not have it"
        );
        submit(&mut lw);
    }
}

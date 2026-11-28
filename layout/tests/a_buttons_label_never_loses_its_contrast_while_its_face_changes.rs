//! A flora button's label - and a tab pill's - is readable at EVERY frame of a change of its face.
//!
//! A button that became primary showed a white label on a face that was still cream, one that stopped
//! being primary a dark label on a face that was still navy, for the better part of a second (QA, the iOS
//! Simulator's recording of the tab strip and of the page's phone row, 2026-11-28). The cause is in the
//! theme's transition tables, which one rebuild reads from the node it REPLACES: a stone's table had no
//! entry for the label, so the label took its new colour at the first frame while the face took 1200 ms
//! to follow; a paper's table faded the label together with the face, on the same curve, and two colours
//! that cross at the same instant leave one frame in which the label is the colour of its face (cream to
//! blue under dark to white: 1.08:1 at a third of the way).
//!
//! A label does not fade with its face: it HOLDS its old colour and switches, in one step, where the old
//! label and the new one read equally well on the face as it is then.
//!
//! The check lays a button (or a row of pills) of one kind out, rebuilds the page with the next kind, runs
//! the frames the way the frame driver does (60 a second), and at each frame holds the colour the label is
//! painted in (the cascade's answer, which includes what a running transition shows) against a pixel of
//! the face (the CPU renderer's frame, at the left padding, where no glyph is) to a 3:1 contrast, in both
//! directions, light and dark. The frames nearest to 0, 25, 50 and 100 % of the face's way are named in
//! the message; `AZ_TEST_SHOW_FRAMES=1` prints every frame.

use azul_core::{
    app_theme::ThemeScope,
    dom::{Dom, DomId, DomNodeId, NodeId},
    events::ProcessEventResult,
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::NodeHierarchyItemId,
    task::Instant,
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_css::{
    props::{basic::color::ColorU, property::CssPropertyType},
    system::defaults,
    AzString, StringVec,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, RenderOptions},
    glyph_cache::GlyphCache,
    widgets::{
        button::{Button, ButtonType},
        tabs::{TabHeader, TabStyle},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use std::sync::Arc;

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 100.0;

/// What the guidelines ask of large text and of a control's label: 3:1.
const FLOOR: f32 = 3.0;

/// WCAG relative luminance of an sRGB colour.
fn luminance(c: ColorU) -> f32 {
    let lin = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: ColorU, b: ColorU) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// body > `content`, built the way an app under the flora theme builds it.
fn page(content: impl FnOnce() -> Dom) -> Dom {
    let _scope = ThemeScope::enter(AzString::from(UiTheme::Flora.name()));
    Dom::create_body().with_css("margin: 0;").with_child(content())
}

fn state(dark: bool) -> FullWindowState {
    let mut ws = FullWindowState {
        mode: if dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        },
        ..Default::default()
    };
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    ws
}

/// A window under the flora theme, light or `dark`, with `first` laid out in it.
fn window(first: Dom, dark: bool) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(if dark {
        defaults::macos_modern_dark()
    } else {
        defaults::macos_modern_light()
    }));
    lw.mode = OptionDarkLightMode::None;
    lw.app_theme = AzString::from(UiTheme::Flora.name());
    let ws = state(dark);
    lw.current_window_state = ws.clone();
    let styled = lw.style_user_dom(first);
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// The app's rebuild with `next`, installed as the shells install one: the reconciliation (which starts
/// the transitions the OLD nodes declare), the layout, its completion.
fn rebuild(lw: &mut LayoutWindow, next: Dom, dark: bool) {
    let ws = state(dark);
    let mut next = lw.style_user_dom(next);
    let pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());
    lw.layout_new_generation(
        next,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the next page lays out");
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
}

/// The nodes of the page that carry one of the classes `classes`, in document order.
fn nodes_with_class(lw: &LayoutWindow, classes: &[&str]) -> Vec<NodeId> {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    (0..styled.node_data.as_ref().len())
        .map(NodeId::new)
        .filter(|n| {
            styled.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if classes.contains(&s)))
        })
        .collect()
}

/// One frame of the animation driver: the clock steps by `dt`, then the frame does the work the tick
/// says it owes, as the shells do it.
fn frame(lw: &mut LayoutWindow, dt: f32) {
    let _ = lw.tick_animations(dt);
    match lw.take_animation_frame_work() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
            lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
        }
        ProcessEventResult::ShouldReRenderCurrentWindow | ProcessEventResult::DoNothing => {}
        other => panic!("a face fade moves no box, yet its frame owes {other:?}"),
    }
}

/// The colour the label of `node` is painted in at this frame: the cascade's answer for `color`, which
/// is where a running transition shows its value.
fn label(lw: &LayoutWindow, node: NodeId) -> ColorU {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node_data = styled.node_data.as_container();
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    styled
        .get_css_property_cache()
        .get_text_color_or_default(&node_data[node], &node, &state)
        .inner
}

/// A pixel of the face of `node` at this frame: the CPU renderer's picture, 5 px in from the left edge,
/// half way down - inside the padding, where the label's glyphs never reach.
fn face(lw: &LayoutWindow, node: NodeId) -> ColorU {
    let list = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref();
    let mut glyphs = GlyphCache::new();
    let picture = cpurender::render_with_font_manager(
        list,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut glyphs,
    )
    .expect("the page paints");
    let rect = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .expect("laid out");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y) = (
        (rect.origin.x + 5.0) as usize,
        (rect.origin.y + rect.size.height / 2.0) as usize,
    );
    let at = (y * picture.width() as usize + x) * 4;
    let px = picture.data();
    ColorU::new(px[at], px[at + 1], px[at + 2], 255)
}

/// How long the face of `node` takes to change, ms: its `background` transition.
fn face_ms(lw: &LayoutWindow, node: NodeId) -> f32 {
    lw.css_transitions
        .iter()
        .filter(|t| t.node == node && t.prop_type == CssPropertyType::BackgroundContent)
        .map(|t| t.duration_s * 1000.0)
        .fold(0.0, f32::max)
}

/// One frame of a change: when (ms), the label's colour, the face's, and their contrast.
type Row = (f32, ColorU, ColorU, f32);

/// The page `first` that the app rebuilds as `next`: for each node `which` names in the new page, the time
/// its face takes and the frames of the change.
fn the_change(
    first: Dom,
    next: Dom,
    dark: bool,
    which: impl Fn(&LayoutWindow) -> Vec<NodeId>,
) -> Vec<(f32, Vec<Row>)> {
    let mut lw = window(first, dark);
    rebuild(&mut lw, next, dark);
    let nodes = which(&lw);
    assert!(!nodes.is_empty(), "harness: the page has the nodes whose label is watched");
    let totals: Vec<f32> = nodes.iter().map(|n| face_ms(&lw, *n)).collect();
    for (node, total) in nodes.iter().zip(&totals) {
        assert!(
            *total > 0.0,
            "harness: the rebuild fades the face of node {} (started: {:?})",
            node.index(),
            lw.css_transitions
                .iter()
                .map(|t| (t.node.index(), t.prop_type.to_str()))
                .collect::<Vec<_>>()
        );
    }
    let longest = totals.iter().copied().fold(0.0, f32::max);
    let mut frames: Vec<Vec<Row>> = nodes.iter().map(|_| Vec::new()).collect();
    let mut ms = 0.0_f32;
    loop {
        for (node, rows) in nodes.iter().zip(&mut frames) {
            let (ink, ground) = (label(&lw, *node), face(&lw, *node));
            rows.push((ms, ink, ground, contrast(ink, ground)));
        }
        if ms > longest + 120.0 {
            break;
        }
        frame(&mut lw, 1.0 / 60.0);
        ms += 1000.0 / 60.0;
    }
    totals.into_iter().zip(frames).collect()
}

/// Every frame of every node of the change reads at 3:1 or better.
fn holds_the_floor(what: &str, changes: &[(f32, Vec<Row>)]) {
    for (which, (total, frames)) in changes.iter().enumerate() {
        let report = || {
            frames
                .iter()
                .map(|(ms, ink, ground, ratio)| {
                    format!(
                        "{ms:5.0} ms ({:3.0} %): label #{:02x}{:02x}{:02x} on #{:02x}{:02x}{:02x} = {ratio:.2}:1",
                        ms / total * 100.0,
                        ink.r,
                        ink.g,
                        ink.b,
                        ground.r,
                        ground.g,
                        ground.b
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        if std::env::var_os("AZ_TEST_SHOW_FRAMES").is_some() {
            eprintln!("{what}, node {which}: the face takes {total:.0} ms\n{}", report());
        }
        for percent in [0.0, 25.0, 50.0, 100.0] {
            let target = total * percent / 100.0;
            let (ms, _, _, ratio) = frames
                .iter()
                .copied()
                .min_by(|a, b| (a.0 - target).abs().total_cmp(&(b.0 - target).abs()))
                .expect("frames");
            assert!(
                ratio >= FLOOR,
                "{what}, node {which}: the frame at {ms:.0} ms, nearest to {percent} % of the face's \
                 {total:.0} ms: the label reads at {ratio:.2}:1\n{}",
                report()
            );
        }
        for (ms, _, _, ratio) in frames {
            assert!(
                *ratio >= FLOOR,
                "{what}, node {which}: at {ms:.0} ms of the face's {total:.0}: the label reads at \
                 {ratio:.2}:1\n{}",
                report()
            );
        }
    }
}

fn pinned_by_env() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

/// A button of kind `from` that the app rebuilds as one of kind `to`.
fn button_becomes(from: ButtonType, to: ButtonType, dark: bool) {
    let button = |kind: ButtonType| move || Button::with_type(AzString::from("Go"), kind).dom();
    let changes = the_change(page(button(from)), page(button(to)), dark, |lw| {
        nodes_with_class(lw, &["__azul-native-button"])
    });
    holds_the_floor(&format!("{from:?} becomes {to:?} (dark: {dark})"), &changes);
}

/// A row of three pills whose selection moves from the first to the second: the first stops being
/// the stone, the second becomes it.
fn selection_moves(style: TabStyle, dark: bool) {
    let row = |active: usize| {
        move || {
            TabHeader::create(StringVec::from_vec(
                ["Home", "Insert", "Design"].iter().map(|s| AzString::from(*s)).collect(),
            ))
            .with_active_tab(active)
            .with_theme(UiTheme::Flora)
            .with_style(style)
            .dom()
        }
    };
    let changes = the_change(page(row(0)), page(row(1)), dark, |lw| {
        // The first two pills: the one that was the stone and the one that becomes it.
        nodes_with_class(
            lw,
            &["__azul-native-tabs-tab-active", "__azul-native-tabs-tab-not-active"],
        )
        .into_iter()
        .take(2)
        .collect()
    });
    holds_the_floor(&format!("the selection of a {style:?} row moves (dark: {dark})"), &changes);
}

#[test]
fn a_button_that_becomes_primary_is_never_a_white_label_on_cream() {
    button_becomes(ButtonType::Default, ButtonType::Primary, false);
}

#[test]
fn a_button_that_stops_being_primary_is_never_a_dark_label_on_blue() {
    button_becomes(ButtonType::Primary, ButtonType::Default, false);
}

#[test]
fn the_other_stones_and_the_illuminated_command_keep_their_labels_too() {
    for kind in [ButtonType::Success, ButtonType::Danger, ButtonType::Info] {
        button_becomes(ButtonType::Default, kind, false);
        button_becomes(kind, ButtonType::Default, false);
    }
    // The metal-edged command is paper in a metal edge: its face and its ink are the paper's, so
    // against paper nothing changes but the edge, and against a stone both change.
    button_becomes(ButtonType::Illuminated, ButtonType::Primary, false);
    button_becomes(ButtonType::Primary, ButtonType::Illuminated, false);
}

#[test]
fn by_night_the_label_holds_the_floor_in_both_directions_too() {
    if pinned_by_env() {
        return;
    }
    button_becomes(ButtonType::Default, ButtonType::Primary, true);
    button_becomes(ButtonType::Primary, ButtonType::Default, true);
}

#[test]
fn the_pills_of_a_tab_row_keep_their_labels_while_the_selection_moves() {
    selection_moves(TabStyle::Pills, false);
    if !pinned_by_env() {
        selection_moves(TabStyle::Pills, true);
    }
}

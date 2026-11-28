//! A button's label - and a tab pill's, a segment's - is readable at EVERY frame of a change of its face,
//! in flora and in flat, light and dark.
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
//! label and the new one read equally well on the face as it is then. The rule is one (`decl::LabelHold`,
//! `decl::fade_switching_the_label`), each theme gives the numbers its faces and its curve need (flora:
//! cream paper and a steel-blue stone on `--fl-ease` over 420 / 1200 ms; flat: a light grey and the
//! bootstrap colours on ease-out over 120 ms). A segmented control declares no fade at all: its face and
//! its label change together, in one frame.
//!
//! The check lays a button (or a row of pills, or a segmented control) of one kind out, rebuilds the page
//! with the next kind, runs the frames the way the frame driver does (60 a second), and at each frame
//! holds the colour the label is painted in (the text run of the display list, over the face by its alpha:
//! what a running transition shows) against a pixel of the face (the CPU renderer's frame, painted with the
//! values the GPU cache binds - a solid face that fades is one of them - at the left padding, where no
//! glyph is) to a 3:1 contrast, in both directions, light and dark. A failure names the worst frame of
//! each change that dips under the floor (when, how far into the face's time, the pair);
//! `AZ_TEST_SHOW_FRAMES=1` prints every frame.
//!
//! Every change also ends in the picture of a page built in the new state (the settled face and label of each
//! watched node), and is run as the phone path runs it: at a Pixel 8's scale, with the app's layout asked for
//! again a frame after the first rebuild, and with the window laying its page out again while the faces fade
//! (QA's Android frames, 2026-11-28: the faces of rebuilt buttons never repainted while their text colours
//! did).
//!
//! NOT covered, and not coverable by one step: a button that is HELD DOWN when it changes (a toggle clicked
//! off) starts its fade from the sunken face, which is darker than the resting one. For a stone that becomes
//! paper the dark ink reads from about a fifth of the way and the light ink of the leaf and amber stones has
//! lost its 3:1 by 7.5 %, so the one step of the label has no point that serves both starts: the label reads
//! at 2.3:1 for about 100 ms in that one change (DECISIONS_FOR_REVIEW item 44, (7)).

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
    cpurender::{self, CpuRenderState, RenderOptions, ScrollOffsetMap, WindowGpuValues},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayListItem,
    widgets::{
        button::{Button, ButtonType},
        segmented::Segmented,
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

/// The scales every check runs at: a desktop's 1 and a Pixel 8's 2.625 (a display list is in logical
/// pixels and the transitions patch it by what it shows, so a scale must change nothing).
const SCALES: [f32; 2] = [1.0, 2.625];

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

/// The two themes every check runs in.
const THEMES: [UiTheme; 2] = [UiTheme::Flora, UiTheme::Flat];

/// body > `content`, built the way an app under `theme` builds it.
fn page(theme: UiTheme, content: impl FnOnce() -> Dom) -> Dom {
    let _scope = ThemeScope::enter(AzString::from(theme.name()));
    Dom::create_body().with_css("margin: 0;").with_child(content())
}

fn state(dark: bool, scale: f32) -> FullWindowState {
    let mut ws = FullWindowState {
        mode: if dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        },
        ..Default::default()
    };
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let dpi = (96.0 * scale).round() as u32;
    ws.size.dpi = dpi;
    ws
}

/// A window under `theme`, light or `dark`, at `scale`, with `first` laid out in it.
fn window(theme: UiTheme, first: Dom, dark: bool, scale: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(if dark {
        defaults::macos_modern_dark()
    } else {
        defaults::macos_modern_light()
    }));
    lw.mode = OptionDarkLightMode::None;
    lw.app_theme = AzString::from(theme.name());
    let ws = state(dark, scale);
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
fn rebuild(lw: &mut LayoutWindow, next: Dom) {
    let ws = lw.current_window_state.clone();
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

/// The window lays its page out again, as it does when its size or its insets move: over the styled DOM
/// it holds (no rebuild), its display list built anew from that DOM's styles.
fn relayout(lw: &mut LayoutWindow) {
    let ws = lw.current_window_state.clone();
    let styled = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .map(|result| result.styled_dom)
        .expect("laid out");
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

/// One frame of the animation driver: the clock steps by `dt`, then the frame does the work the tick says it
/// owes, as the shells do it: the list it owes is rebuilt by the driver (`serve_animation_frame_work`), because a
/// shell presents the list it has. With `AZ_TEST_NO_REBUILD` the frame is the decision alone, which is what a
/// driver that hands it to a shell that only presents leaves on the screen (QA's Android frames: the old face).
fn frame(lw: &mut LayoutWindow, dt: f32) {
    let _ = lw.tick_animations(dt);
    let work = if std::env::var_os("AZ_TEST_NO_REBUILD").is_some() {
        lw.take_animation_frame_work()
    } else {
        lw.serve_animation_frame_work()
    };
    match work {
        // A press moves a box by a pixel: the frame owes the relayout the shells do.
        ProcessEventResult::ShouldIncrementalRelayout => relayout(lw),
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
        | ProcessEventResult::ShouldReRenderCurrentWindow
        | ProcessEventResult::DoNothing => {}
        other => panic!("a face fade moves no box, yet its frame owes {other:?}"),
    }
}

/// The colour the label of `node` is painted in at this frame, as the eye gets it: the colour of the text
/// run the display list holds inside the node, laid over `ground` by its alpha (a dark theme's label is
/// white at 85 %, and a transition's running value shows here, not in the cascade: a `system:` colour is
/// a marker there until the painter resolves it).
fn label(lw: &LayoutWindow, node: NodeId, ground: ColorU) -> ColorU {
    let rect = lw
        .get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        })
        .expect("laid out");
    let list = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref();
    let inside = |glyph: &azul_core::ui_solver::GlyphInstance| {
        glyph.point.x >= rect.origin.x
            && glyph.point.x <= rect.origin.x + rect.size.width
            && glyph.point.y >= rect.origin.y
            && glyph.point.y <= rect.origin.y + rect.size.height
    };
    let ink = list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Text { glyphs, color, .. } if glyphs.first().is_some_and(inside) => Some(*color),
            _ => None,
        })
        .expect("harness: the display list paints a text run inside the node");
    let over = |top: u8, under: u8| {
        let alpha = f32::from(ink.a) / 255.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mixed = (f32::from(top) * alpha + f32::from(under) * (1.0 - alpha)).round() as u8;
        mixed
    };
    ColorU::new(over(ink.r, ground.r), over(ink.g, ground.g), over(ink.b, ground.b), 255)
}

/// A pixel of the face of `node` at this frame: the CPU renderer's picture, painted as the window paints
/// it (the display list with the values the GPU cache binds to it: a SOLID face that fades is a colour
/// the list does not carry, a bound rect the renderer paints in the cache's value, and a `system:` colour
/// in a gradient is resolved against the window's system style), 5 px in from the left edge, half way
/// down - inside the padding, where the label's glyphs never reach.
fn face(lw: &LayoutWindow, node: NodeId) -> ColorU {
    #[allow(clippy::cast_precision_loss)] // a dpi is a few hundred
    let scale = lw.current_window_state.size.dpi as f32 / 96.0;
    let list = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .as_ref();
    let mut glyphs = GlyphCache::new();
    let paint = CpuRenderState::new(ScrollOffsetMap::new())
        .with_gpu_values(WindowGpuValues::of_doms(&lw.gpu_state_manager, [DomId::ROOT_ID]))
        .with_system_style(lw.system_style.clone());
    let picture = cpurender::render_with_font_manager_and_scroll(
        list,
        &RendererResources::default(),
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: scale,
        },
        &mut glyphs,
        &paint,
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
        ((rect.origin.x + 5.0) * scale) as usize,
        ((rect.origin.y + rect.size.height / 2.0) * scale) as usize,
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

/// What the check expects of the face of the nodes it watches: it is what keeps a check from passing
/// because nothing moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    /// It takes time (the theme's transition runs on the node) and the label steps inside it.
    Fades,
    /// It changes in one frame, as the segments of a segmented control do (they declare no fade): there
    /// is no time in which the label and the face could disagree, and the new pair must read.
    Instant,
    /// Whatever the two kinds do: a pair of kinds can wear the same face.
    Any,
}

/// How one run of a change is set up.
#[derive(Debug, Clone, Copy)]
struct Run {
    theme: UiTheme,
    dark: bool,
    scale: f32,
    /// How many times the app rebuilt, a frame apart (its layout asked for again right after the first).
    rebuilds: usize,
    /// How many times the window laid the page out again while it faded, a frame apart (its size or its
    /// insets moved: a phone's bars hide when the preset is on).
    relayouts: usize,
}

impl Run {
    fn name(self) -> String {
        format!(
            "{:?}, {}, scale {}{}{}",
            self.theme,
            if self.dark { "dark" } else { "light" },
            self.scale,
            if self.rebuilds > 1 { format!(", rebuilt {} times", self.rebuilds) } else { String::new() },
            if self.relayouts > 0 { format!(", laid out again {} times", self.relayouts) } else { String::new() }
        )
    }
}

/// The runs of `dark` and `rebuilds` in every theme at every scale of `scales`.
fn runs(dark: bool, rebuilds: usize, scales: &[f32]) -> Vec<Run> {
    THEMES
        .iter()
        .flat_map(|theme| {
            scales.iter().map(move |scale| Run {
                theme: *theme,
                dark,
                scale: *scale,
                rebuilds,
                relayouts: 0,
            })
        })
        .collect()
}

/// What a change showed: the frames of each watched node, and what the SETTLED page showed that a page
/// built in the new state does not.
struct Changed {
    nodes: Vec<(f32, Vec<Row>)>,
    unsettled: Vec<String>,
}

/// Two colours a rounding apart.
fn alike(a: ColorU, b: ColorU) -> bool {
    a.r.abs_diff(b.r) <= 2 && a.g.abs_diff(b.g) <= 2 && a.b.abs_diff(b.b) <= 2
}

/// The page `first` that the app rebuilds as `next`, as `run` says: for each node `which` names in the new
/// page, the time its face takes and the frames of the change; and, once everything has settled, whether
/// the picture is the one of a page that was built in the new state (the face and the label of each watched
/// node), which a fade that was never finished, or a list that was patched and then rebuilt from stale
/// values, is not.
fn the_change(
    run: Run,
    first: &dyn Fn() -> Dom,
    next: &dyn Fn() -> Dom,
    expect: Face,
    which: &dyn Fn(&LayoutWindow) -> Vec<NodeId>,
) -> Changed {
    let mut lw = window(run.theme, first(), run.dark, run.scale);
    rebuild(&mut lw, next());
    for _ in 1..run.rebuilds {
        // The app's layout is asked for again a frame later (the window state the first rebuild changed).
        frame(&mut lw, 1.0 / 60.0);
        rebuild(&mut lw, next());
    }
    for _ in 0..run.relayouts {
        // The window lays the page out again while it fades (its insets moved).
        frame(&mut lw, 1.0 / 60.0);
        relayout(&mut lw);
    }
    let nodes = which(&lw);
    assert!(!nodes.is_empty(), "harness: the page has the nodes whose label is watched");
    let totals: Vec<f32> = nodes.iter().map(|n| face_ms(&lw, *n)).collect();
    for (node, total) in nodes.iter().zip(&totals) {
        let as_expected = match expect {
            Face::Fades => *total > 0.0,
            Face::Instant => *total <= 0.0,
            Face::Any => true,
        };
        assert!(
            as_expected,
            "harness: the rebuild changes the face of node {} in {total:.0} ms, expected {expect:?} \
             (started: {:?})",
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
            let ground = face(&lw, *node);
            let ink = label(&lw, *node, ground);
            rows.push((ms, ink, ground, contrast(ink, ground)));
        }
        if ms > longest + 120.0 {
            break;
        }
        frame(&mut lw, 1.0 / 60.0);
        ms += 1000.0 / 60.0;
    }
    // Settled: the same page, built in the new state, is the picture the change must end in.
    let fresh = window(run.theme, next(), run.dark, run.scale);
    let mut unsettled = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let (ground, fresh_ground) = (face(&lw, *node), face(&fresh, *node));
        let (ink, fresh_ink) = (label(&lw, *node, ground), label(&fresh, *node, fresh_ground));
        if !alike(ground, fresh_ground) || !alike(ink, fresh_ink) {
            unsettled.push(format!(
                "node {i}: settled on a face #{:02x}{:02x}{:02x} with a label #{:02x}{:02x}{:02x}, a page built in the \
                 new state shows #{:02x}{:02x}{:02x} with #{:02x}{:02x}{:02x}",
                ground.r, ground.g, ground.b, ink.r, ink.g, ink.b, fresh_ground.r, fresh_ground.g, fresh_ground.b,
                fresh_ink.r, fresh_ink.g, fresh_ink.b
            ));
        }
    }
    Changed {
        nodes: totals.into_iter().zip(frames).collect(),
        unsettled,
    }
}

/// The frames of a change, one line each: when (and how far into the face's time), the ink, the face,
/// their contrast.
fn report(total: f32, frames: &[Row]) -> String {
    frames
        .iter()
        .map(|(ms, ink, ground, ratio)| {
            let share = if total > 0.0 {
                format!("{:3.0} %", ms / total * 100.0)
            } else {
                "     ".to_string()
            };
            format!(
                "{ms:5.0} ms ({share}): label #{:02x}{:02x}{:02x} on #{:02x}{:02x}{:02x} = {ratio:.2}:1",
                ink.r, ink.g, ink.b, ground.r, ground.g, ground.b
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every frame of every node of the change reads at 3:1 or better, and the page ends as a page built in the
/// new state looks: nothing when both hold, else a line for the node whose worst frame reads the worst (when,
/// how far into the face's time, the pair) and a line for each node that did not settle where it should.
/// `AZ_TEST_SHOW_FRAMES=1` prints every frame of every change.
fn breach_of(what: &str, changed: &Changed) -> Option<String> {
    let show = std::env::var_os("AZ_TEST_SHOW_FRAMES").is_some();
    let mut lines: Vec<String> = Vec::new();
    let mut worst: Option<String> = None;
    let mut worst_ratio = FLOOR;
    for (which, (total, frames)) in changed.nodes.iter().enumerate() {
        if show {
            eprintln!("{what}, node {which}: the face takes {total:.0} ms\n{}", report(*total, frames));
        }
        for (ms, ink, ground, ratio) in frames {
            if *ratio < worst_ratio {
                worst_ratio = *ratio;
                let share = if *total > 0.0 {
                    format!("{:.0} % of the face's {total:.0} ms", ms / total * 100.0)
                } else {
                    "the face changes in one frame".to_string()
                };
                worst = Some(format!(
                    "{what}, node {which}: {ratio:.2}:1 at {ms:.0} ms ({share}), label #{:02x}{:02x}{:02x} on #{:02x}{:02x}{:02x}",
                    ink.r, ink.g, ink.b, ground.r, ground.g, ground.b
                ));
            }
        }
    }
    lines.extend(worst);
    lines.extend(changed.unsettled.iter().map(|line| format!("{what}, {line}")));
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The test fails with every breach listed (a matrix of changes is read whole, not one failure at a time).
fn no_breach(breaches: &[String]) {
    assert!(
        breaches.is_empty(),
        "{} change(s) break the floor of {FLOOR}:1 or do not settle as a page built in the new state \
         (AZ_TEST_SHOW_FRAMES=1 prints the frames):\n{}",
        breaches.len(),
        breaches.join("\n")
    );
}

fn pinned_by_env() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

/// Every kind of button that has a face: the kinds a change goes between.
const KINDS: [ButtonType; 8] = [
    ButtonType::Default,
    ButtonType::Primary,
    ButtonType::Secondary,
    ButtonType::Success,
    ButtonType::Danger,
    ButtonType::Warning,
    ButtonType::Info,
    ButtonType::Illuminated,
];

/// A phone's scale, alone: the runs that vary the way the app rebuilds do not repeat at every scale.
const PHONE: [f32; 1] = [2.625];

/// A button of kind `from` that the app rebuilds as one of kind `to`, as `run` says: the breach, if any.
fn button_becomes(run: Run, from: ButtonType, to: ButtonType, expect: Face) -> Option<String> {
    let button = |kind: ButtonType| move || Button::with_type(AzString::from("Go"), kind).dom();
    let changed = the_change(
        run,
        &|| page(run.theme, button(from)),
        &|| page(run.theme, button(to)),
        expect,
        &|lw| nodes_with_class(lw, &["__azul-native-button"]),
    );
    breach_of(&format!("{}: {from:?} becomes {to:?}", run.name()), &changed)
}

/// A row of three pills whose selection moves from the first to the second: the first stops being
/// the stone, the second becomes it. The breach, if any.
fn selection_moves(run: Run, style: TabStyle) -> Option<String> {
    let row = |active: usize| {
        move || {
            TabHeader::create(StringVec::from_vec(
                ["Home", "Insert", "Design"].iter().map(|s| AzString::from(*s)).collect(),
            ))
            .with_active_tab(active)
            .with_theme(run.theme)
            .with_style(style)
            .dom()
        }
    };
    let changed = the_change(
        run,
        &|| page(run.theme, row(0)),
        &|| page(run.theme, row(1)),
        Face::Fades,
        // The first two pills: the one that was the stone and the one that becomes it.
        &|lw| {
            nodes_with_class(
                lw,
                &["__azul-native-tabs-tab-active", "__azul-native-tabs-tab-not-active"],
            )
            .into_iter()
            .take(2)
            .collect()
        },
    );
    breach_of(&format!("{}: the selection of a {style:?} row moves", run.name()), &changed)
}

/// A segmented control of three segments whose selection moves from segment `from` to segment `to`: every
/// segment is watched (the one that loses the selection, the one that gains it, the one that has none).
/// The breach, if any.
fn selection_of_segments_moves(run: Run, from: usize, to: usize) -> Option<String> {
    let control = |selected: usize| {
        move || {
            let mut control = Segmented::create(StringVec::from_vec(
                ["Day", "Week", "Month"].iter().map(|s| AzString::from(*s)).collect(),
            ))
            .with_theme(run.theme);
            control.segmented_state.inner.selected_index = selected;
            control.dom()
        }
    };
    let changed = the_change(
        run,
        &|| page(run.theme, control(from)),
        &|| page(run.theme, control(to)),
        Face::Instant,
        &|lw| nodes_with_class(lw, &["__azul-native-segmented-item"]),
    );
    breach_of(
        &format!("{}: the selection of a segmented control moves from {from} to {to}", run.name()),
        &changed,
    )
}

/// Every kind of button that becomes every other, in each of `runs`.
fn every_kind_becomes_every_other(set: &[Run]) -> Vec<String> {
    let mut breaches = Vec::new();
    for run in set {
        for from in KINDS {
            for to in KINDS {
                if from != to {
                    breaches.extend(button_becomes(*run, from, to, Face::Any));
                }
            }
        }
    }
    breaches
}

#[test]
fn a_button_that_becomes_primary_is_never_a_light_label_on_a_light_face() {
    let mut breaches = Vec::new();
    for run in runs(false, 1, &SCALES) {
        breaches.extend(button_becomes(run, ButtonType::Default, ButtonType::Primary, Face::Fades));
    }
    no_breach(&breaches);
}

#[test]
fn a_button_that_stops_being_primary_is_never_a_dark_label_on_a_dark_face() {
    let mut breaches = Vec::new();
    for run in runs(false, 1, &SCALES) {
        breaches.extend(button_becomes(run, ButtonType::Primary, ButtonType::Default, Face::Fades));
    }
    no_breach(&breaches);
}

#[test]
fn a_button_keeps_its_label_readable_while_it_becomes_any_other_kind_of_button() {
    no_breach(&every_kind_becomes_every_other(&runs(false, 1, &SCALES)));
}

#[test]
fn a_layout_that_is_asked_for_again_a_frame_later_ends_in_the_picture_of_the_new_state() {
    no_breach(&every_kind_becomes_every_other(&runs(false, 2, &PHONE)));
}

#[test]
fn a_window_that_lays_its_page_out_again_while_the_faces_fade_ends_in_the_picture_of_the_new_state() {
    // The phone path of the AzWidgets page (Android emulator, 2026-11-28): the faces of the rebuilt buttons
    // were never repainted while their text colours were, for as long as the window was looked at.
    let mut all = Vec::new();
    for relayouts in [1, 3] {
        for rebuilds in [1, 2] {
            all.extend(runs(false, rebuilds, &PHONE).into_iter().map(|run| Run { relayouts, ..run }));
        }
    }
    no_breach(&every_kind_becomes_every_other(&all));
}

#[test]
fn by_night_the_label_holds_the_floor_in_every_change_too() {
    if pinned_by_env() {
        return;
    }
    let mut all = runs(true, 1, &SCALES);
    all.extend(runs(true, 2, &PHONE));
    no_breach(&every_kind_becomes_every_other(&all));
}

#[test]
fn the_pills_of_a_tab_row_keep_their_labels_while_the_selection_moves() {
    let mut breaches = Vec::new();
    for dark in [false, true] {
        if dark && pinned_by_env() {
            continue;
        }
        let mut all = runs(dark, 1, &SCALES);
        all.extend(runs(dark, 2, &PHONE));
        for run in all {
            breaches.extend(selection_moves(run, TabStyle::Pills));
        }
    }
    no_breach(&breaches);
}

#[test]
fn the_segments_of_a_segmented_control_read_in_both_states_and_in_both_themes() {
    let mut breaches = Vec::new();
    for dark in [false, true] {
        if dark && pinned_by_env() {
            continue;
        }
        for run in runs(dark, 1, &SCALES) {
            for (from, to) in [(0, 1), (1, 0), (1, 2), (2, 0)] {
                breaches.extend(selection_of_segments_moves(run, from, to));
            }
        }
    }
    no_breach(&breaches);
}

#[test]
fn the_labels_step_is_a_step_not_a_fade_in_every_table_that_has_one() {
    use azul_css::props::property::CssProperty;
    let mut tables = 0;
    for theme in THEMES {
        for kind in KINDS {
            let dom = Button::with_type(AzString::from("Go"), kind).with_theme(theme).dom();
            for (property, _) in dom.root.style.iter_inline_properties() {
                let CssProperty::Animation(value) = property else { continue };
                let Some(list) = value.get_property() else { continue };
                for entry in list.as_ref().iter().filter(|e| e.name.as_str() == "color") {
                    tables += 1;
                    assert!(
                        entry.duration.inner <= 5,
                        "{theme:?} {kind:?}: the label's colour steps, it does not fade ({} ms)",
                        entry.duration.inner
                    );
                    // A frame that falls inside the step's millisecond shows (nearly) the old colour: no grey
                    // between two inks.
                    let at_nine_tenths = entry.timing.evaluate(0.9);
                    assert!(
                        at_nine_tenths < 0.2,
                        "{theme:?} {kind:?}: at nine tenths of its time the step is {at_nine_tenths:.2} of the way"
                    );
                    assert!(
                        (entry.timing.evaluate(1.0) - 1.0).abs() < 1e-3,
                        "{theme:?} {kind:?}: the step ends on the new colour"
                    );
                }
            }
        }
    }
    assert!(tables >= 2 * KINDS.len(), "harness: the buttons' tables hold the label's step ({tables} found)");
}

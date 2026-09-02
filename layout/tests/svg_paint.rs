//! `<svg>` in a DOM has to PAINT, through the ordinary CSS pipeline.
//!
//! The XML parser already maps `<path>`, `<circle>`, `<use>`,
//! `<linearGradient>` and `<stop>` onto real DOM nodes, and an `<svg>` now
//! carries its viewBox and its intrinsic size - so an icon, a chart or a logo
//! can be a subtree like any other. What was missing is the last step: the
//! display list consumed `SvgNodeData::Path` ONLY as a clip mask, and nothing
//! read it to draw anything. An SVG injected as a DOM therefore laid out
//! correctly and rendered zero pixels.
//!
//! The fix is deliberately not a second rasteriser: the shape is a CLIP over
//! an ordinary CSS background, so `fill` is a colour the cascade resolves and
//! every existing background feature - a gradient, an image, a hover state -
//! comes along for free.
//!
//! These tests render through `render_dom_to_rgba`, the same entry the icon
//! pack uses, and COUNT PIXELS: "it laid out" is exactly the evidence that
//! was misleading before.

use azul_css::{css::Css, props::basic::color::ColorU};
use azul_layout::cpurender::render_dom_to_rgba;

const TRANSPARENT: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// Render markup at `size`x`size` logical px on a transparent backdrop.
///
/// A BARE FRAGMENT is passed through deliberately - no `<html>` wrapper - so
/// these also pin that an `<svg>` root parses on its own, the way a browser
/// takes one.
fn render(markup: &str, size: f32) -> azul_layout::cpurender::ComponentPreviewResult {
    let markup = format!(
        "<style>body {{ margin: 0; padding: 0; }}</style>{markup}"
    );
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the fixture parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    render_dom_to_rgba(dom, Css::empty(), size, size, 1.0, TRANSPARENT).expect("renders")
}

/// (opaque pixel count, total pixel count)
fn coverage(r: &azul_layout::cpurender::ComponentPreviewResult) -> (usize, usize) {
    let total = (r.pixel_width * r.pixel_height) as usize;
    let painted = r.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
    (painted, total)
}

/// The colour at the centre of the frame.
fn centre(r: &azul_layout::cpurender::ComponentPreviewResult) -> [u8; 4] {
    let x = r.pixel_width / 2;
    let y = r.pixel_height / 2;
    let i = ((y * r.pixel_width + x) * 4) as usize;
    [r.rgba[i], r.rgba[i + 1], r.rgba[i + 2], r.rgba[i + 3]]
}

/// How many pixels are exactly `want`.
///
/// COUNTED, not sampled at a fixed offset: an `<svg>` is inline-level and
/// sits on a text baseline, so its box lands at a fractional y that shifts
/// with the font. A test that samples one pixel breaks for reasons that have
/// nothing to do with what it checks.
fn count_of(r: &azul_layout::cpurender::ComponentPreviewResult, want: [u8; 4]) -> usize {
    r.rgba.chunks_exact(4).filter(|p| *p == want).count()
}

/// THE regression: a filled path must produce pixels.
#[test]
fn a_filled_path_paints() {
    let r = render(
        r##"<svg viewBox="0 0 16 16" width="16" height="16">
             <path fill="#ff0000" d="M 2,2 L 14,2 L 14,14 L 2,14 Z"/>
           </svg>"##,
        16.0,
    );
    let (painted, total) = coverage(&r);
    assert!(
        painted * 4 > total,
        "the 12x12 square covers over half of a 16x16 frame; got {painted}/{total}"
    );
    assert_eq!(
        centre(&r),
        [255, 0, 0, 255],
        "and it is painted in the colour `fill` asked for"
    );
}

/// `fill` is a COLOUR THE CASCADE RESOLVES, not an attribute read by a
/// bespoke parser - that is what makes a stylesheet, a hover state or a
/// gradient work on an SVG shape without any further plumbing.
#[test]
fn fill_comes_through_css_in_every_spelling() {
    const GREEN: [u8; 4] = [0, 255, 0, 255];

    // Presentation attribute.
    let attr = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
             <rect fill="#00ff00" x="0" y="0" width="8" height="8"/>
           </svg>"##,
        24.0,
    );
    assert_eq!(count_of(&attr, GREEN), 64, "fill=\"...\"");

    // The `style` attribute, which is how every Breeze icon is authored.
    let inline = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
             <rect style="fill:#00ff00" x="0" y="0" width="8" height="8"/>
           </svg>"##,
        24.0,
    );
    assert_eq!(count_of(&inline, GREEN), 64, "style=\"fill:...\"");

    // A stylesheet rule, which is what `class="ColorScheme-Text"` resolves
    // through.
    let styled = render(
        r##"<html><head><style>body { margin: 0; padding: 0; } .ink { fill: #00ff00; }</style></head>
           <body><svg viewBox="0 0 8 8" width="8" height="8">
             <rect class="ink" x="0" y="0" width="8" height="8"/>
           </svg></body></html>"##,
        24.0,
    );
    assert_eq!(count_of(&styled, GREEN), 64, "a stylesheet rule");
}

/// The viewBox is a COORDINATE SYSTEM: geometry drawn in it has to scale into
/// whatever box the element ends up with, or an icon designed at 16 units
/// paints a sixteenth of a 256px slot.
#[test]
fn geometry_scales_from_the_view_box_into_the_painted_box() {
    let r = render(
        r##"<svg viewBox="0 0 16 16" width="64" height="64">
             <path fill="#0000ff" d="M 0,0 L 16,0 L 16,16 L 0,16 Z"/>
           </svg>"##,
        64.0,
    );
    let (painted, total) = coverage(&r);
    assert!(
        painted * 10 > total * 9,
        "a full-viewBox square must fill the whole 64x64 box, not a 16x16 \
         corner of it; got {painted}/{total}"
    );
    assert_eq!(centre(&r), [0, 0, 255, 255]);
}

/// An SVG's OWN `<style>` is a stylesheet, not content, and it is scoped to
/// the SVG.
///
/// This is how every freedesktop icon carries its colours
/// (`.ColorScheme-Text { color: … }` inside `<defs>`), and in azul a
/// stylesheet is an ATTRIBUTE of a node rather than a node of its own - so it
/// has to be recognised at the input and hung on the element that contains
/// it. Left as a node, the CSS source rendered as visible text; hung on the
/// `<defs>` that holds it, it scoped to a subtree that draws nothing.
///
/// Counted rather than sampled: the shape's position depends on the baseline
/// an inline-block sits on, and a fixed pixel offset is a test that breaks for
/// reasons that have nothing to do with what it checks.
#[test]
fn an_svgs_own_style_element_styles_it_and_nothing_else() {
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    let green = |r: &azul_layout::cpurender::ComponentPreviewResult| {
        r.rgba.chunks_exact(4).filter(|p| *p == GREEN).count()
    };

    let alone = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
              <defs><style>.ink { fill: #00ff00; }</style></defs>
              <rect class="ink" x="0" y="0" width="8" height="8"/>
            </svg>"##,
        48.0,
    );
    assert_eq!(
        green(&alone),
        64,
        "the SVG's own stylesheet must reach its shapes: an 8x8 rect is 64 px"
    );

    // The SAME sheet, with a div outside the SVG carrying the same class. If
    // the sheet were document-scoped the div would fill too and the count
    // would grow.
    let with_outsider = render(
        r##"<div class="ink" style="width: 8px; height: 8px;"></div>
            <svg viewBox="0 0 8 8" width="8" height="8">
              <defs><style>.ink { fill: #00ff00; }</style></defs>
              <rect class="ink" x="0" y="0" width="8" height="8"/>
            </svg>"##,
        48.0,
    );
    assert_eq!(
        green(&with_outsider),
        64,
        "an SVG's stylesheet is scoped to the SVG - the div outside it must \
         stay unpainted"
    );
}

/// A STROKE is its own paint, and it is not a background.
///
/// A stroked outline has no interior to fill: the shape's box clipped to the
/// path leaves a hairline at best, which is what a line drawn with
/// `fill="none"` looked like before - a pale smear instead of a 4px rule.
#[test]
fn a_stroked_path_paints_its_outline() {
    let r = render(
        r##"<svg viewBox="0 0 16 16" width="16" height="16">
              <path fill="none" stroke="#ff0000" stroke-width="4" d="M 0,8 L 16,8"/>
            </svg>"##,
        16.0,
    );
    let red = r
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60 && p[3] > 200)
        .count();
    // A 16-long, 4-wide rule is 64 px; allow for the anti-aliased ends.
    assert!(
        (48..=96).contains(&red),
        "a 16x4 stroke should cover about 64 px, got {red}"
    );
}

/// Fill and stroke are INDEPENDENT paints of the same geometry - the PDF
/// model. A shape can have both, and the stroke sits on top.
#[test]
fn fill_and_stroke_are_independent_paints() {
    let r = render(
        r##"<svg viewBox="0 0 16 16" width="16" height="16">
              <rect x="4" y="4" width="8" height="8"
                    fill="#00ff00" stroke="#ff0000" stroke-width="2"/>
            </svg>"##,
        16.0,
    );
    let count = |pred: fn(&&[u8]) -> bool| r.rgba.chunks_exact(4).filter(pred).count();
    let green = count(|p| p[1] > 200 && p[0] < 60 && p[3] > 200);
    let red = count(|p| p[0] > 200 && p[1] < 60 && p[3] > 200);
    assert!(green > 0, "the fill must paint");
    assert!(red > 0, "the stroke must paint too");
}

/// `stroke` and `stroke-width` are CSS, like `fill` - so a stylesheet rule
/// reaches them, which is how a themed icon restyles its outlines.
#[test]
fn stroke_comes_through_css_as_well_as_the_attribute() {
    let r = render(
        r##"<style>.rule { stroke: #ff0000; stroke-width: 4px; }</style>
            <svg viewBox="0 0 16 16" width="16" height="16">
              <path class="rule" fill="none" d="M 0,8 L 16,8"/>
            </svg>"##,
        16.0,
    );
    let red = r
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] < 60 && p[3] > 200)
        .count();
    assert!(red > 32, "a stylesheet stroke must paint, got {red}");
}

/// A shape with no fill paints NOTHING - `fill="none"` is a real value, and
/// defaulting it to black would put a black box behind every stroked outline.
#[test]
fn fill_none_paints_nothing() {
    let r = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
             <rect fill="none" x="0" y="0" width="8" height="8"/>
           </svg>"##,
        8.0,
    );
    let (painted, _) = coverage(&r);
    assert_eq!(painted, 0, "fill=\"none\" must not paint");
}


// ============================================================================
// Hit testing — a clip path clips POINTER TARGETS too
// ============================================================================

/// A shape occupies a rectangular BOX in layout, but the thing the user aims
/// at is the PATH. Hit-testing the box makes the transparent corners of a
/// shape clickable and, worse, lets them SHADOW whatever is behind them -
/// which is exactly what a clip path is asked to prevent.
#[test]
fn a_clip_path_clips_the_pointer_target_not_just_the_pixels() {
    use azul_core::{
        dom::{DomId, NodeId},
        geom::{LogicalPosition, LogicalSize},
        resources::RendererResources,
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;

    // A triangle filling the LOWER-LEFT half of a 16x16 box. The upper-right
    // corner is inside the box and outside the shape.
    let markup = r##"<style>html, body { margin: 0; padding: 0; }</style>
        <svg viewBox="0 0 16 16" width="16" height="16">
          <path fill="#ff0000" d="M 0,0 L 0,16 L 16,16 Z"/>
        </svg>"##;
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(dom);

    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(64.0, 64.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("lays out");

    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("result");
    let shape = (0..result.styled_dom.node_data.len())
        .map(NodeId::new)
        .find(|id| {
            *result.styled_dom.node_data.as_container()[*id].get_node_type()
                == azul_core::dom::NodeType::SvgPath
        })
        .expect("the document has a <path>");

    let mut hit_tester = azul_layout::headless::CpuHitTester::new();
    hit_tester.rebuild_from_layout(&lw.layout_results);
    let hits = |x: f32, y: f32| {
        hit_tester
            .hit_test(LogicalPosition::new(x, y))
            .into_iter()
            .any(|(_, n)| n == shape)
    };

    let svg = (0..result.styled_dom.node_data.len())
        .map(NodeId::new)
        .find(|id| {
            *result.styled_dom.node_data.as_container()[*id].get_node_type()
                == azul_core::dom::NodeType::Svg
        })
        .expect("the document has an <svg>");
    let hits_svg = |x: f32, y: f32| {
        hit_tester
            .hit_test(LogicalPosition::new(x, y))
            .into_iter()
            .any(|(_, n)| n == svg)
    };

    // Deep inside the triangle.
    assert!(hits(3.0, 13.0), "a point inside the shape must hit it");
    // Inside the BOX, outside the triangle - the corner the clip removed.
    assert!(
        !hits(13.0, 3.0),
        "a point in the clipped-away corner must NOT hit the shape - the box \
         is not the target, the path is"
    );
    // THE CONTROL: that same corner is inside the `<svg>`, which has no clip
    // path, and still hits it. Without this the assertion above would pass
    // just as well if the hit tester simply never reached that point.
    assert!(
        hits_svg(13.0, 3.0),
        "the clipped-away corner is still inside the <svg> box, so the test \
         above is about the PATH and not about the point being unreachable"
    );
}

/// PSEUDO-STATES reach an SVG shape.
///
/// This is why the icon loader leaves `.ColorScheme-*` as CASCADE RULES rather
/// than baking each element's colour: an inline declaration beats a class
/// rule, so a baked fill would pin the glyph's colour and defeat every
/// `:hover` a widget puts around it - which is exactly why a desktop's close
/// button could never turn red on hover.
#[test]
fn hover_and_focus_rules_reach_an_svg_shape() {
    use azul_core::{id::NodeId, styled_dom::StyledNodeState};

    let markup = r##"<style>
            .ink { fill: #00ff00; }
            .ink:hover { fill: #ff0000; }
            .ink:focus { fill: #0000ff; }
        </style>
        <svg viewBox="0 0 8 8" width="8" height="8">
          <rect class="ink" x="0" y="0" width="8" height="8"/>
        </svg>"##;
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let styled_dom = azul_core::styled_dom::StyledDom::create_from_dom(dom);

    let shape = (0..styled_dom.node_data.len())
        .map(NodeId::new)
        .find(|id| {
            *styled_dom.node_data.as_container()[*id].get_node_type()
                == azul_core::dom::NodeType::SvgRect
        })
        .expect("the document has a <rect>");

    // The same call the display list makes when it paints the shape.
    let fill_in = |state: StyledNodeState| {
        azul_layout::solver3::getters::get_background_contents(&styled_dom, shape, &state)
            .into_iter()
            .find_map(|bg| match bg {
                azul_css::props::style::StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            })
    };

    let resting = StyledNodeState::default();
    let mut hovered = StyledNodeState::default();
    hovered.hover = true;
    let mut focused = StyledNodeState::default();
    focused.focused = true;

    assert_eq!(
        fill_in(resting),
        Some(ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255
        }),
        "the resting fill comes from the class rule"
    );
    assert_eq!(
        fill_in(hovered),
        Some(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }),
        "a :hover rule must reach the shape - this is what a baked inline \
         fill made impossible"
    );
    assert_eq!(
        fill_in(focused),
        Some(ColorU {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }),
        ":focus too"
    );
}

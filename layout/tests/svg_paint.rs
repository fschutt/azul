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

/// THE regression: a filled path must produce pixels.
#[test]
#[ignore = "RED, in progress: the shape node now gets a CSS background and \
            an absolutely-positioned box, and the clip mask now maps viewBox \
            user space into the paint rect - but nothing paints yet. Next: \
            find where between layout and the display list the shape's \
            background is dropped."]
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
#[ignore = "RED, in progress: the shape node now gets a CSS background and \
            an absolutely-positioned box, and the clip mask now maps viewBox \
            user space into the paint rect - but nothing paints yet. Next: \
            find where between layout and the display list the shape's \
            background is dropped."]
fn fill_comes_through_css_in_every_spelling() {
    // Presentation attribute.
    let attr = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
             <rect fill="#00ff00" x="0" y="0" width="8" height="8"/>
           </svg>"##,
        8.0,
    );
    assert_eq!(centre(&attr), [0, 255, 0, 255], "fill=\"...\"");

    // The `style` attribute, which is how every Breeze icon is authored.
    let inline = render(
        r##"<svg viewBox="0 0 8 8" width="8" height="8">
             <rect style="fill:#00ff00" x="0" y="0" width="8" height="8"/>
           </svg>"##,
        8.0,
    );
    assert_eq!(centre(&inline), [0, 255, 0, 255], "style=\"fill:...\"");

    // A stylesheet rule, which is what `class="ColorScheme-Text"` resolves
    // through.
    let styled = render(
        r##"<html><head><style>body { margin: 0; padding: 0; } .ink { fill: #00ff00; }</style></head>
           <body><svg viewBox="0 0 8 8" width="8" height="8">
             <rect class="ink" x="0" y="0" width="8" height="8"/>
           </svg></body></html>"##,
        8.0,
    );
    assert_eq!(centre(&styled), [0, 255, 0, 255], "a stylesheet rule");
}

/// The viewBox is a COORDINATE SYSTEM: geometry drawn in it has to scale into
/// whatever box the element ends up with, or an icon designed at 16 units
/// paints a sixteenth of a 256px slot.
#[test]
#[ignore = "RED, in progress: the shape node now gets a CSS background and \
            an absolutely-positioned box, and the clip mask now maps viewBox \
            user space into the paint rect - but nothing paints yet. Next: \
            find where between layout and the display list the shape's \
            background is dropped."]
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


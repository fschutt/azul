//! Every ATOMIC INLINE paints its background exactly ONCE.
//!
//! An atomic inline (`inline-block`, `inline-flex`, `inline-table`,
//! `inline-grid` — CSS Display 3 §2.2) is inline-level but opaque: it takes part
//! in its parent's IFC as a single unit, so text3 builds one `InlineShape` for
//! it and the parent paints it from there. Two places have to agree on that set,
//! and they sit on opposite sides of the same seam:
//!
//!   * `display_list.rs` must SKIP painting the box itself (the parent already
//!     did it) — otherwise the box appears twice, once at the shape's position
//!     and once at the parent's content origin;
//!   * `getters.rs::get_style_properties_for_state` must SUPPLY the background
//!     and border to the shape — otherwise the box appears not at all.
//!
//! Both used to name `InlineBlock` alone, so `inline-flex` (which is what every
//! `Button` is) fell through them in opposite directions. Found on the frontpage
//! `opengl` screenshot 2026-09-08: a `Button` composited over a callback image
//! painted twice, once with its label and once without.
//!
//! This test walks all four display values so neither side of the seam can be
//! narrowed to a subset again without a failure that names the value.

use azul_core::{
    dom::{Dom, IdOrClass},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Number of opaque rects painted with exactly this colour.
fn count_rects(lw: &LayoutWindow, (r, g, b): (u8, u8, u8)) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for result in lw.layout_results.values() {
        for item in &result.display_list.items {
            if let DisplayListItem::Rect { bounds, color, .. } = item {
                if color.r == r && color.g == g && color.b == b && color.a > 0 {
                    let o = bounds.origin();
                    out.push((o.x, o.y));
                }
            }
        }
    }
    out
}

fn layout(css_str: &str, dom: Dom) -> LayoutWindow {
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let mut dom = dom;
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);

    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw
}

/// The four atomic inline display values, each with a background colour that
/// appears nowhere else in the document.
const ATOMIC_INLINES: &[(&str, (u8, u8, u8))] = &[
    ("inline-block", (0xff, 0x00, 0x00)),
    ("inline-flex", (0x00, 0x00, 0xff)),
    ("inline-grid", (0xff, 0xff, 0x00)),
    ("inline-table", (0xff, 0x00, 0xff)),
];

#[test]
fn every_atomic_inline_paints_its_background_exactly_once() {
    for (display, rgb) in ATOMIC_INLINES {
        let (r, g, b) = *rgb;
        let css = format!(
            "body {{ margin: 0; padding: 0; }}
             p {{ margin: 0; }}
             .mk {{ display: {display}; width: 100px; height: 20px;
                    background: rgb({r}, {g}, {b}); }}"
        );

        let dom = Dom::create_body().with_child(
            Dom::create_p().with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![IdOrClass::Class("mk".into())].into())
                    .with_child(Dom::create_span_with_text("x")),
            ),
        );

        let hits = count_rects(&layout(&css, dom), *rgb);
        assert_eq!(
            hits.len(),
            1,
            "an atomic inline with `display: {display}` painted its background \
             {} time(s), at {hits:?}. 0 = both the display-list guard and the \
             inline-shape style lookup skipped it; 2 = neither did.",
            hits.len()
        );
    }
}

/// The same four values as a child of a REPLACED element, which is the shape
/// the frontpage `opengl` example composits (`img > button`) and where the
/// duplicate was first seen.
#[test]
fn every_atomic_inline_over_an_image_paints_exactly_once() {
    use azul_core::resources::{ImageRef, RawImageFormat};

    for (display, rgb) in ATOMIC_INLINES {
        let (r, g, b) = *rgb;
        let css = format!(
            "body {{ margin: 0; padding: 0; }}
             img {{ width: 400px; height: 300px; }}
             .mk {{ display: {display}; width: 100px; height: 20px;
                    margin-top: 50px; margin-left: 50px;
                    background: rgb({r}, {g}, {b}); }}"
        );

        let dom = Dom::create_body().with_child(
            Dom::create_image(ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new()))
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("mk".into())].into())
                        .with_child(Dom::create_span_with_text("x")),
                ),
        );

        let hits = count_rects(&layout(&css, dom), *rgb);
        assert_eq!(
            hits.len(),
            1,
            "`display: {display}` over an <img> painted {} time(s), at {hits:?}",
            hits.len()
        );
    }
}

/// The root cause behind the frontpage duplicate: whether the `<img>` was a
/// FLEX ITEM decided how its own box was classified. In normal flow it keeps
/// `display: inline-block` and an independent formatting context; as a flex item
/// it is blockified to `Block`, and a block box whose children are all
/// inline-level establishes an IFC — so the image became an inline formatting
/// context over its own overlay children. That bogus IFC painted the child a
/// second time, and the interior run dropped the child's margin, so the two
/// copies landed 50px apart.
///
/// The child's painted position must not depend on the parent's participation in
/// its own parent's layout, so all four shapes below agree.
#[test]
fn an_overlay_child_is_painted_once_whether_or_not_the_image_is_a_flex_item() {
    use azul_core::resources::{ImageRef, RawImageFormat};

    // (name, body css, img css, expected top-left of the child)
    let variants: &[(&str, &str, &str, (f32, f32))] = &[
        (
            "image in normal flow",
            "body { margin: 0; padding: 0; }",
            "img { width: 400px; height: 300px; }",
            (50.0, 50.0),
        ),
        (
            "image as a flex item",
            "body { display: flex; flex-direction: column; padding: 10px;
                    width: 100%; height: 100%; box-sizing: border-box; }",
            "img { flex-grow: 1; width: 100%; }",
            (68.0, 68.0),
        ),
        (
            "image with a border in normal flow",
            "body { margin: 0; padding: 0; }",
            "img { width: 400px; height: 300px; border: 5px solid #00ff00;
                   border-radius: 50px; box-sizing: border-box; }",
            (55.0, 55.0),
        ),
        (
            "image with a border as a flex item",
            "body { display: flex; flex-direction: column; padding: 10px;
                    width: 100%; height: 100%; box-sizing: border-box; }",
            "img { flex-grow: 1; width: 100%; border: 5px solid #00ff00;
                   border-radius: 50px; box-sizing: border-box; }",
            (73.0, 73.0),
        ),
    ];

    for (name, body_css, img_css, expected) in variants {
        let css = format!(
            "{body_css} {img_css}
             .mk {{ display: inline-block; width: 100px; height: 20px;
                    margin-top: 50px; margin-left: 50px;
                    background: rgb(255, 0, 0); }}"
        );

        let dom = Dom::create_body().with_child(
            Dom::create_image(ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new()))
                .with_child(
                    Dom::create_div()
                        .with_ids_and_classes(vec![IdOrClass::Class("mk".into())].into())
                        .with_child(Dom::create_span_with_text("x")),
                ),
        );

        let hits = count_rects(&layout(&css, dom), (0xff, 0x00, 0x00));
        assert_eq!(
            hits.len(),
            1,
            "with the {name}, the overlay child was painted {} time(s), at {hits:?}",
            hits.len()
        );

        let (x, y) = hits[0];
        let (ex, ey) = *expected;
        assert!(
            (x - ex).abs() < 0.5 && (y - ey).abs() < 0.5,
            "with the {name}, the overlay child painted at ({x}, {y}), expected \
             ({ex}, {ey}) — its 50px margins were dropped by the interior run"
        );
    }
}

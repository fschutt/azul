//! An SVG stroke without a `stroke-width` is one user unit wide (SVG 2
//! s13.5.1: the initial value of `stroke-width` is 1), as in Chrome. The
//! shape's stroke rides its border slots, and with no width declared the
//! slot was empty and nothing was drawn at all (G7).

use azul_css::{css::Css, props::basic::color::ColorU};
use azul_layout::cpurender::render_dom_to_rgba;

/// The red pixels of `markup` rendered in a `size` px square.
fn red_pixels(markup: &str, size: f32) -> usize {
    let markup = format!(
        "<style>body {{ margin: 0; padding: 0; }} svg {{ display: block; }}</style>{markup}"
    );
    let parsed = azul_layout::xml::parse_xml(&markup).expect("the fixture parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let transparent = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    let r = render_dom_to_rgba(dom, Css::empty(), size, size, 1.0, transparent).expect("renders");
    r.rgba
        .chunks_exact(4)
        .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100 && p[3] > 100)
        .count()
}

#[test]
fn a_stroke_without_a_width_draws_a_one_unit_line() {
    let red = red_pixels(
        r##"<svg viewBox="0 0 16 16" width="16" height="16">
              <path fill="none" stroke="#ff0000" d="M 0,8 L 16,8"/>
            </svg>"##,
        16.0,
    );
    // A 16-long, 1-wide rule is 16 px; its antialiased edge may split it
    // over two rows.
    assert!(
        (8..=40).contains(&red),
        "a stroke with no width is one unit wide: {red} red pixels"
    );
}

#[test]
fn a_width_without_a_stroke_still_draws_nothing() {
    let red = red_pixels(
        r##"<svg viewBox="0 0 16 16" width="16" height="16">
              <path fill="none" stroke-width="4" d="M 0,8 L 16,8"/>
            </svg>"##,
        16.0,
    );
    assert_eq!(red, 0, "stroke is none by default");
}

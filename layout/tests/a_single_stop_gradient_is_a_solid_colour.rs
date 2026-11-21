//! A gradient with a single colour stop paints that colour (CSS Images 4
//! s3.4: a one-stop list is valid; with nothing to interpolate to, the
//! gradient is the stop's colour everywhere).
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, E-GRAD,
//! css-images/gradient/gradient-single-stop-001..003): the red box behind
//! the gradient showed through - one stop normalized to one stop, and every
//! renderer paints nothing for fewer than two.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, GREEN, RED};

fn painted_over_red(gradient: &str) -> crate::painted::Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }} \
             .under {{ width: 100px; height: 100px; background: red; }} \
             .over {{ width: 100px; height: 100px; background-image: {gradient}; }}\
             </style></head><body><div class=\"under\"><div class=\"over\"></div></div>\
             </body></html>"
        ),
        120,
        120,
    )
}

fn assert_solid_green(gradient: &str) {
    let page = painted_over_red(gradient);
    let red = page.count((0, 0, 100, 100), RED, 30);
    let green = page.count((0, 0, 100, 100), GREEN, 30);
    assert!(
        red == 0 && green > 9_500,
        "`{gradient}` covers the red box in green: {green} green and {red} red pixels of 10000"
    );
}

#[test]
fn a_gradient_of_one_colour_paints_it() {
    assert_solid_green("linear-gradient(green)");
}

#[test]
fn a_gradient_of_one_positioned_stop_paints_it_on_both_sides() {
    assert_solid_green("linear-gradient(to right, green 90%)");
}

#[test]
fn a_repeating_gradient_of_one_stop_paints_it() {
    assert_solid_green("repeating-linear-gradient(green 50px)");
}

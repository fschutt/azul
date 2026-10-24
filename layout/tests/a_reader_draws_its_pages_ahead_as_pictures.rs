//! A reader draws its pages ahead as pictures.
//!
//! An e-reader turns pages instantly only if the pages around the one on
//! screen are drawn before they are asked for, off the UI thread, at the
//! size they are shown at. `cpurender::render_pages` lays a document out on
//! pages with a break policy - the pages `Pdf::compute_pagination_with_policy`
//! counts - and draws a run of them with the CPU renderer, one under the
//! other in one picture.
//!
//! Not compiled by the author (house rule).

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use azul_core::{dom::Dom, resources::ImageCache, styled_dom::StyledDom};
use azul_css::props::basic::color::ColorU;
use azul_layout::{
    cpurender::{render_pages, AzulPixmap, PageRenderOptions},
    solver3::page_breaks::BreakPolicy,
    text3::cache::FontManager,
};
use rust_fontconfig::FcFontCache;

/// Three 100px blocks, red, green and blue, 50px wide.
fn document() -> StyledDom {
    let block = |color: &str| {
        Dom::create_div().with_css(&format!(
            "display: block; width: 50px; height: 100px; margin: 0px; background: {color};"
        ))
    };
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 0px; padding: 0px;")
            .with_child(block("rgb(255, 0, 0)"))
            .with_child(block("rgb(0, 255, 0)"))
            .with_child(block("rgb(0, 0, 255)")),
    )
}

fn draw(first_page: usize, page_count: usize, dpi_factor: f32) -> AzulPixmap {
    let mut fonts = FontManager::new(FcFontCache::default()).expect("a font manager");
    render_pages(
        &document(),
        &mut fonts,
        &ImageCache::default(),
        BreakPolicy::default(),
        PageRenderOptions {
            page_width: 50.0,
            page_height: 100.0,
            dpi_factor,
            background_color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            first_page,
            page_count,
        },
    )
    .expect("the pages are drawn")
}

fn rgb(pm: &AzulPixmap, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * pm.width + x) * 4) as usize;
    let d = pm.data();
    [d[i], d[i + 1], d[i + 2]]
}

#[test]
fn the_second_and_third_page_are_drawn_one_under_the_other() {
    let pm = draw(1, 2, 1.0);
    assert_eq!((pm.width, pm.height), (50, 200), "two pages of 50 x 100");
    assert_eq!(rgb(&pm, 25, 50), [0, 255, 0], "the second page is the green block");
    assert_eq!(rgb(&pm, 25, 150), [0, 0, 255], "the third page is the blue block");
}

#[test]
fn pages_past_the_end_are_left_out() {
    let pm = draw(2, 4, 1.0);
    assert_eq!((pm.width, pm.height), (50, 100), "only the last page is left to draw");
    assert_eq!(rgb(&pm, 25, 50), [0, 0, 255]);
}

#[test]
fn pages_are_drawn_at_the_device_pixels_they_are_shown_at() {
    let pm = draw(0, 1, 2.0);
    assert_eq!((pm.width, pm.height), (100, 200), "a page at 2x is 100 x 200 pixels");
    assert_eq!(rgb(&pm, 50, 100), [255, 0, 0]);
}

//! A chart's series PAINT through the engine's own SVG path
//! (`widgets/chart.rs`): the plot declares a user space of one unit per px,
//! a line is a STROKE along its series' path (the node's border width and
//! colour), bars and wedges are a FILL clipped to it (the node's
//! background). No second renderer - so if these pixels are missing, the
//! engine's SVG paint is what broke, not the chart.
//!
//! Rendered with the CPU renderer on a transparent backdrop and COUNTED,
//! like `svg_paint.rs`: "it laid out" is not evidence that it painted.

use azul_core::dom::Dom;
use azul_css::{css::Css, props::basic::color::ColorU, AzString, StringVec};
use azul_layout::{
    cpurender::{render_dom_to_rgba, ComponentPreviewResult},
    widgets::{
        chart::{Chart, ChartKind, ChartPoint, ChartPointVec, ChartSeries, CHART_PALETTE},
        themes::UiTheme,
    },
};

const TRANSPARENT: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// The chart, pinned to flat, in a body without margin, rendered at its size.
fn render(chart: Chart) -> ComponentPreviewResult {
    let (w, h) = (chart.width, chart.height);
    let dom = Dom::create_body()
        .with_css_props(
            azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(vec![
                azul_css::dynamic_selector::CssPropertyWithConditions::simple(
                    azul_css::props::property::CssProperty::const_margin_top(
                        azul_css::props::layout::LayoutMarginTop::const_px(0),
                    ),
                ),
                azul_css::dynamic_selector::CssPropertyWithConditions::simple(
                    azul_css::props::property::CssProperty::const_margin_left(
                        azul_css::props::layout::LayoutMarginLeft::const_px(0),
                    ),
                ),
            ]),
        )
        .with_child(chart.with_theme(UiTheme::Flat).dom());
    render_dom_to_rgba(dom, Css::empty(), w, h, 1.0, TRANSPARENT).expect("the chart renders")
}

/// How many opaque pixels are within `tol` of `c` in every channel.
fn near(r: &ComponentPreviewResult, c: ColorU, tol: i32) -> usize {
    r.rgba
        .chunks_exact(4)
        .filter(|p| {
            p[3] > 200
                && (i32::from(p[0]) - i32::from(c.r)).abs() <= tol
                && (i32::from(p[1]) - i32::from(c.g)).abs() <= tol
                && (i32::from(p[2]) - i32::from(c.b)).abs() <= tol
        })
        .count()
}

fn series(name: &str, ys: &[f64]) -> ChartSeries {
    ChartSeries::create(
        AzString::from(name),
        ChartPointVec::from_vec(
            ys.iter()
                .enumerate()
                .map(|(i, y)| ChartPoint::create(i as f64, *y))
                .collect(),
        ),
    )
}

fn categories(names: &[&str]) -> StringVec {
    StringVec::from_vec(names.iter().map(|s| AzString::from(*s)).collect())
}

/// A 2 px line across a ~330 px plot covers several hundred pixels of its
/// colour - and only its outline: a line is never filled.
#[test]
fn a_line_series_paints_its_stroke_in_its_colour_and_nothing_inside() {
    let r = render(
        Chart::create(ChartKind::Line, 420.0, 260.0)
            .with_added_series(series("Up and down", &[0.0, 10.0, 0.0])),
    );
    let blue = near(&r, CHART_PALETTE[0].light, 48);
    assert!(
        blue > 150,
        "the line's stroke must paint, got {blue} px of its colour"
    );
    assert!(
        blue < 5000,
        "a line is a stroke, not a filled triangle: got {blue} px of its colour"
    );
}

/// Bars are the series node's FILL clipped to the bars' path: two series,
/// two colours, each a few thousand pixels.
#[test]
fn bars_paint_their_fill_one_colour_per_series() {
    let r = render(
        Chart::create(ChartKind::Bar, 420.0, 260.0)
            .with_categories(categories(&["A", "B", "C"]))
            .with_added_series(series("North", &[50.0, 80.0, 65.0]))
            .with_added_series(series("South", &[40.0, 70.0, 90.0])),
    );
    let blue = near(&r, CHART_PALETTE[0].light, 24);
    let orange = near(&r, CHART_PALETTE[1].light, 24);
    assert!(
        blue > 1500,
        "the first series' bars must paint, got {blue} px"
    );
    assert!(
        orange > 1500,
        "the second series' bars must paint, got {orange} px"
    );
}

/// A pie paints a wedge per category in the palette's order.
#[test]
fn a_pie_paints_a_wedge_per_category() {
    let r = render(
        Chart::create(ChartKind::Pie, 320.0, 320.0)
            .with_categories(categories(&["A", "B", "C", "D"]))
            .with_added_series(series("Share", &[1.0, 1.0, 1.0, 1.0])),
    );
    for (k, c) in CHART_PALETTE.iter().take(4).enumerate() {
        let n = near(&r, c.light, 24);
        assert!(
            n > 1500,
            "slice {k} must paint in its palette colour, got {n} px"
        );
    }
}

/// The plot's gridlines and the baseline are plain boxes; the series sit
/// over them, and the chart's own sheet is the page colour.
#[test]
fn the_chart_sits_on_its_own_sheet() {
    let r = render(
        Chart::create(ChartKind::Line, 300.0, 200.0).with_added_series(series("Flat", &[1.0, 1.0])),
    );
    let white = near(&r, ColorU::rgb(255, 255, 255), 2);
    let total = (r.pixel_width * r.pixel_height) as usize;
    assert!(
        white > total / 2,
        "the flat chart is a white sheet: {white} of {total} px"
    );
}

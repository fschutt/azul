//! The pie marker at a size of its own: the disc, its hit area and the geometry scale together, a
//! size outside the limits is held to them, and the default stays the 11 px design.

use super::*;

fn class_of(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

fn find<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
    if class_of(dom, class) {
        return Some(dom);
    }
    dom.children.as_ref().iter().find_map(|c| find(c, class))
}

fn width_of(dom: &Dom) -> Option<f32> {
    dom.root.style.iter_inline_properties().filter_map(|(p, _)| match p {
        CssProperty::Width(v) => v.get_property().and_then(|w| match w {
            LayoutWidth::Px(px) => Some(px.number.get()),
            _ => None,
        }),
        _ => None,
    }).last()
}

fn height_of(dom: &Dom) -> Option<f32> {
    dom.root.style.iter_inline_properties().filter_map(|(p, _)| match p {
        CssProperty::Height(v) => v.get_property().and_then(|h| match h {
            LayoutHeight::Px(px) => Some(px.number.get()),
            _ => None,
        }),
        _ => None,
    }).last()
}

#[test]
fn a_pie_has_the_designs_size_until_it_is_given_one() {
    assert!((PieMarker::create(0.5, PieTone::Accent).size - PIE_SIZE).abs() < 1e-6);
}

#[test]
fn the_size_is_held_to_its_limits_and_a_bad_number_is_the_default() {
    assert!((clamp_pie_size(18.0) - 18.0).abs() < 1e-6);
    assert!((clamp_pie_size(1.0) - PIE_SIZE_MIN).abs() < 1e-6);
    assert!((clamp_pie_size(1000.0) - PIE_SIZE_MAX).abs() < 1e-6);
    assert!((clamp_pie_size(f32::NAN) - PIE_SIZE).abs() < 1e-6);
    assert!((clamp_pie_size(-4.0) - PIE_SIZE).abs() < 1e-6);
    assert!((clamp_pie_size(f32::INFINITY) - PIE_SIZE).abs() < 1e-6);
    assert!((PieMarker::create(0.5, PieTone::Accent).with_size(2.0).size - PIE_SIZE_MIN).abs() < 1e-6);
}

#[test]
fn a_bigger_disc_has_a_bigger_hit_area_around_it() {
    let dom = PieMarker::create(0.3, PieTone::Accent).with_size(18.0).dom();
    assert_eq!(width_of(&dom), Some(22.0));
    assert_eq!(height_of(&dom), Some(32.0));
    let disc = find(&dom, PIE_MARKER_DISC_CLASS).expect("the disc");
    assert_eq!(width_of(disc), Some(18.0));
    assert_eq!(height_of(disc), Some(18.0));
}

#[test]
fn the_default_marker_is_still_15_by_25_around_an_11_px_disc() {
    let dom = PieMarker::create(0.3, PieTone::Accent).dom();
    assert_eq!(width_of(&dom), Some(15.0));
    assert_eq!(height_of(&dom), Some(25.0));
}

#[test]
fn the_wedge_scales_with_the_disc() {
    let small = wedge_shape(11.0, 0.25).expect("a quarter");
    let big = wedge_shape(22.0, 0.25).expect("a quarter");
    let end = |shape: &SvgMultiPolygon| shape.rings.as_ref()[0].items.as_ref().iter().map(|e| e.get_start()).last().expect("a point");
    let (a, b) = (end(&small), end(&big));
    assert!((b.x - 2.0 * a.x).abs() < 1e-2 && (b.y - 2.0 * a.y).abs() < 1e-2, "{a:?} {b:?}");
}

#[test]
fn the_bare_disc_has_no_hit_area_and_no_button() {
    let disc = disc_dom(18.0, 0.4, PieTone::Good);
    assert!(class_of(&disc, PIE_MARKER_DISC_CLASS));
    assert!(!class_of(&disc, PIE_MARKER_CLASS));
    assert!(disc.root.get_tab_index().is_none());
    assert!(find(&disc, PIE_MARKER_WEDGE_CLASS).is_some());
}

#[test]
fn a_pie_that_is_no_number_is_an_empty_disc_not_a_crash() {
    let nan = disc_dom(18.0, f32::NAN, PieTone::Accent);
    assert!(find(&nan, PIE_MARKER_WEDGE_CLASS).is_none());
    let over = disc_dom(18.0, 7.0, PieTone::Accent);
    assert!(find(&over, PIE_MARKER_WEDGE_CLASS).is_some(), "more than full is full");
}

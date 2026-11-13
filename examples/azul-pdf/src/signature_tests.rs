//! The signature pad's drawing and where a signature lands on a page: no window.

use crate::signature::{place, Pad, PlacedSignature, SIGNATURE_WIDTH_PT};

#[test]
fn a_pad_keeps_its_strokes_and_writes_them_as_an_svg_of_paths() {
    let mut pad = Pad::new(300.0, 120.0);
    assert!(pad.is_empty());
    assert_eq!(pad.svg(), None, "nothing drawn, nothing to sign with");
    pad.begin(10.0, 20.0);
    pad.extend(10.2, 20.1); // under a pixel from the last point: not kept
    pad.extend(40.0, 60.0);
    pad.extend(80.0, 30.0);
    pad.end();
    pad.begin(100.0, 100.0);
    pad.extend(120.0, 90.0);
    pad.end();
    assert_eq!(pad.strokes().len(), 2);
    assert_eq!(pad.strokes()[0].len(), 3);
    let svg = pad.svg().expect("a drawing");
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 300 120\""), "{svg}");
    assert!(svg.contains("<path d=\"M10 20 L40 60 L80 30\""), "{svg}");
    assert!(svg.contains("<path d=\"M100 100 L120 90\""), "{svg}");
    assert!(svg.contains("fill=\"none\""), "a stroke, not a filled shape: {svg}");
    // A tap is a dot: a stroke of one point still draws.
    pad.begin(200.0, 50.0);
    pad.end();
    assert!(pad.svg().unwrap().contains("M200 50 L200.5 50"), "a dot is a short line");
    pad.clear();
    assert!(pad.is_empty());
    // A point outside the pad is kept on its edge.
    pad.begin(-5.0, 500.0);
    pad.extend(400.0, 60.0);
    pad.end();
    assert!(pad.svg().unwrap().contains("M0 120 L300 60"));
}

#[test]
fn a_signature_lands_centred_on_the_click_and_inside_the_page() {
    // A US Letter page (612 x 792 pt) shown 918 px wide: 1.5 px per point.
    let page_pt = (612.0, 792.0);
    let page_px = (918.0, 1188.0);
    // A 300 x 120 drawing: SIGNATURE_WIDTH_PT wide, its height by its shape.
    let r = place((459.0, 594.0), page_px, page_pt, (300.0, 120.0));
    assert!((r.width - SIGNATURE_WIDTH_PT).abs() < 1e-3);
    assert!((r.height - SIGNATURE_WIDTH_PT * 120.0 / 300.0).abs() < 1e-3);
    assert!((r.x + r.width / 2.0 - 306.0).abs() < 1e-3, "centred on the click: {r:?}");
    assert!((r.y + r.height / 2.0 - 396.0).abs() < 1e-3, "{r:?}");
    // A click at the corner: the signature stays on the page.
    let corner = place((917.0, 1187.0), page_px, page_pt, (300.0, 120.0));
    assert!(corner.x + corner.width <= 612.0 + 1e-3 && corner.y + corner.height <= 792.0 + 1e-3);
    let origin = place((0.0, 0.0), page_px, page_pt, (300.0, 120.0));
    assert!(origin.x >= 0.0 && origin.y >= 0.0);
    let placed = PlacedSignature { page: 2, rect: r };
    assert_eq!(placed.page, 2);
}

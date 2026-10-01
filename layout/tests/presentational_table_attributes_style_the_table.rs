//! Presentational table attributes style the table.
//!
//! HTML's rendering section maps the legacy attributes of table elements to
//! CSS ("presentational hints"): `width` / `height` to the dimension
//! properties (ignoring zero), `cellspacing` to `border-spacing`,
//! `cellpadding` to the padding of the table's cells, `border` to the
//! table's border (outset) and a 1px inset border on its cells, `bgcolor` to
//! `background-color` (legacy colour parsing: `ff0000` without the `#` is
//! red), `align` on a table to its margins / float and on a cell to
//! `text-align`, `valign` to `vertical-align`, `nowrap` to
//! `white-space: nowrap`, `bordercolor` to `border-color`.
//!
//! The bug: the XML loader dropped every one of them (the attribute table
//! had no entry), so a mail's `<table width="600" cellpadding="0"
//! cellspacing="0" border="0" bgcolor="#ffffff">` laid out as an auto table
//! with the UA spacing and padding and no background (REFCI E-LEGACY,
//! `exploration/08`: `<TABLE WIDTH=300>` 78 px wide). AzMail's sanitizer
//! rewrote a few of them app-side; the engine must read them itself.
//!
//! Not compiled by the author (house rule); expected RED before the fix.

use crate::table_markup::{block, body, glyph_runs, near, rect, rects_of_color, right, words};

#[test]
fn the_width_attribute_sets_the_tables_width_in_pixels_or_percent() {
    let lw = body(&format!(
        "<table id=\"px\" width=\"300\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{b}</td></tr></table>\
         <div style=\"width: 600px\"><table id=\"pc\" width=\"50%\" cellspacing=\"0\" cellpadding=\"0\">\
         <tr><td>{b}</td></tr></table></div>\
         <table id=\"zero\" width=\"0\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{b}</td></tr></table>",
        b = block(50)
    ));
    assert!(near(rect(&lw, "px").size.width, 300.0, 0.5), "width=300");
    assert!(
        near(rect(&lw, "pc").size.width, 300.0, 0.5),
        "width=50% of 600"
    );
    assert!(
        near(rect(&lw, "zero").size.width, 50.0, 0.5),
        "width=0 is ignored (HTML: 'ignoring zero'): {:?}",
        rect(&lw, "zero")
    );
}

#[test]
fn cellspacing_is_the_border_spacing() {
    let lw = body(&format!(
        "<table id=\"t\" cellspacing=\"7\" cellpadding=\"0\"><tr><td id=\"a\">{}</td></tr></table>",
        block(100)
    ));
    let (t, a) = (rect(&lw, "t"), rect(&lw, "a"));
    assert!(
        near(a.origin.x - t.origin.x, 7.0, 0.5),
        "the cell starts 7px in: {a:?} {t:?}"
    );
    assert!(near(t.size.width, 114.0, 0.5), "100 + 2 x 7: {t:?}");
}

#[test]
fn cellpadding_pads_the_tables_own_cells_only() {
    let lw = body(&format!(
        "<table cellspacing=\"0\" cellpadding=\"6\"><tr><td id=\"a\">{}\
         <table cellspacing=\"0\"><tr><td id=\"inner\">{}</td></tr></table>\
         </td></tr></table>",
        block(100),
        block(10)
    ));
    let a = rect(&lw, "a");
    assert!(near(a.size.width, 112.0, 0.5), "100 + 2 x 6 padding: {a:?}");
    let inner = rect(&lw, "inner");
    assert!(
        near(inner.size.width, 12.0, 0.5),
        "a nested table without cellpadding keeps the UA's 1px: {inner:?}"
    );
}

#[test]
fn bgcolor_paints_the_background_with_a_legacy_colour() {
    let lw = body(&format!(
        "<table id=\"t\" bgcolor=\"#00ff00\" cellspacing=\"4\" cellpadding=\"0\">\
         <tr><td id=\"a\" bgcolor=\"ff0000\">{}</td></tr></table>",
        block(100)
    ));
    let (t, a) = (rect(&lw, "t"), rect(&lw, "a"));
    let green = rects_of_color(&lw, (0, 255, 0));
    assert!(
        green
            .iter()
            .any(|r| near(r.size.width, t.size.width, 0.5) && near(r.origin.x, t.origin.x, 0.5)),
        "the table's bgcolor fills the table {t:?}: {green:?}"
    );
    let red = rects_of_color(&lw, (255, 0, 0));
    assert!(
        red.iter()
            .any(|r| near(r.size.width, a.size.width, 0.5) && near(r.origin.x, a.origin.x, 0.5)),
        "bgcolor=ff0000 (no #) is red on the cell {a:?}: {red:?}"
    );
}

#[test]
fn border_gives_the_table_and_its_cells_a_border() {
    let lw = body(&format!(
        "<table id=\"t\" border=\"1\" cellspacing=\"0\" cellpadding=\"0\"><tr><td id=\"a\">{b}</td></tr></table>\
         <table id=\"t0\" border=\"0\" cellspacing=\"0\" cellpadding=\"0\"><tr><td id=\"a0\">{b}</td></tr></table>",
        b = block(100)
    ));
    let (t, a) = (rect(&lw, "t"), rect(&lw, "a"));
    assert!(
        near(a.size.width, 102.0, 0.5),
        "a 1px border on each cell side: {a:?}"
    );
    assert!(
        near(t.size.width, 104.0, 0.5),
        "the cell plus the table's 1px border: {t:?}"
    );
    let (t0, a0) = (rect(&lw, "t0"), rect(&lw, "a0"));
    assert!(
        near(a0.size.width, 100.0, 0.5),
        "border=0: no cell border: {a0:?}"
    );
    assert!(
        near(t0.size.width, 100.0, 0.5),
        "border=0: no table border: {t0:?}"
    );
}

#[test]
fn align_center_centers_the_table_and_align_right_floats_it() {
    let lw = body(&format!(
        "<div style=\"width: 600px\">\
         <table id=\"c\" align=\"center\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{b}</td></tr></table>\
         <table id=\"r\" align=\"right\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{b}</td></tr></table>\
         </div>",
        b = block(100)
    ));
    let c = rect(&lw, "c");
    assert!(
        near(c.origin.x, 250.0, 0.5),
        "align=center: margins auto: {c:?}"
    );
    let r = rect(&lw, "r");
    assert!(
        near(right(&r), 600.0, 0.5),
        "align=right: float right: {r:?}"
    );
}

#[test]
fn align_and_valign_on_a_cell_align_its_content() {
    let lw = body(&format!(
        "<table id=\"t\" width=\"300\" cellspacing=\"0\" cellpadding=\"0\"><tr>\
         <td align=\"right\" valign=\"bottom\">x</td>\
         <td style=\"width: 100px\"><i style=\"display: inline-block; width: 10px; height: 100px\"></i></td>\
         </tr></table>"
    ));
    let t = rect(&lw, "t");
    let runs = glyph_runs(&lw);
    let x = runs
        .iter()
        .flatten()
        .copied()
        .next()
        .unwrap_or_else(|| panic!("the x is painted: {runs:?}"));
    // The first column is 200px wide: right-aligned, the glyph starts in
    // its last 20px; bottom-aligned, its baseline is in the row's last 30px.
    assert!(
        x.0 > t.origin.x + 180.0 && x.0 < t.origin.x + 200.0,
        "align=right: the x is at the right of its 200px column: {x:?}, table {t:?}"
    );
    assert!(
        x.1 > t.origin.y + 70.0,
        "valign=bottom: the x is at the bottom of the 100px row: {x:?}, table {t:?}"
    );
}

#[test]
fn nowrap_keeps_a_cell_on_one_line() {
    let lw = body(&format!(
        "<table id=\"t\" width=\"100\" cellspacing=\"0\" cellpadding=\"0\"><tr>\
         <td id=\"c\" nowrap=\"nowrap\">{}</td></tr></table>",
        words(3, 50)
    ));
    let t = rect(&lw, "t");
    assert!(
        t.size.width >= 150.0,
        "a nowrap cell's minimum is its whole line, so the table grows: {t:?}"
    );
    let c = rect(&lw, "c");
    assert!(c.size.height < 30.0, "one line: {c:?}");
}

#[test]
fn the_height_attribute_sets_the_tables_height() {
    let lw = body(&format!(
        "<table id=\"t\" height=\"200\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{}</td></tr></table>",
        block(50)
    ));
    let t = rect(&lw, "t");
    assert!(near(t.size.height, 200.0, 0.5), "height=200: {t:?}");
}

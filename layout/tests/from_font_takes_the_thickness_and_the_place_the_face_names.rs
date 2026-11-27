//! `text-decoration-thickness: from-font` and `text-underline-position: from-font` (CSS Text
//! Decoration 3 and 4) take what the face of the text names: the `post` table's underline
//! thickness for an underline and an overline, the OS/2 table's strike-through size for a
//! line-through, and the `post` table's `underlinePosition` for the place of an underline, all in
//! the font size of the box that sets the line. A face that names none - the test faces - gives
//! `auto`.
//!
//! The face is `azul-mock-features.ttf` with its `post` and OS/2 values set in memory: an underline
//! at -100 units, 60 thick, a strike-through 90 thick (1000 units to the em), so that at 20 px the
//! underline's top edge lies 2 px below the baseline and is 1.2 px thick, where `auto` is 2.4 px
//! and 1.6 px, and a line-through is 1.8 px thick where `auto` is 1.6 px.
//!
//! Not compiled by the author (house rule).

use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::text_rig::{block, face_bytes, lay_out, painted, span, text, window, MONO};

const LINED: &str = "Azul Mock Lined";

/// The offset of the table `tag` in the face `bytes`.
fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    (0..count)
        .map(|i| 12 + 16 * i)
        .find(|&entry| &bytes[entry..entry + 4] == tag)
        .map(|entry| {
            u32::from_be_bytes([
                bytes[entry + 8],
                bytes[entry + 9],
                bytes[entry + 10],
                bytes[entry + 11],
            ]) as usize
        })
        .unwrap_or_else(|| panic!("premise: the face has a {} table", String::from_utf8_lossy(tag)))
}

fn put(bytes: &mut [u8], at: usize, value: i16) {
    bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

/// The mock face with an underline of its own and a strike-through of its own.
fn lined_face() -> Vec<u8> {
    let mut bytes = face_bytes("azul-mock-features.ttf");
    let post = table_offset(&bytes, b"post");
    put(&mut bytes, post + 8, -100); // underlinePosition
    put(&mut bytes, post + 10, 60); // underlineThickness
    let os2 = table_offset(&bytes, b"OS/2");
    put(&mut bytes, os2 + 26, 90); // yStrikeoutSize
    put(&mut bytes, os2 + 28, 300); // yStrikeoutPosition
    bytes
}

/// One line of the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    y: f32,
    height: f32,
}

fn lines(dl: &DisplayList) -> Vec<Line> {
    dl.items
        .iter()
        .filter_map(|item| {
            let bounds = match item {
                DisplayListItem::Underline { bounds, .. }
                | DisplayListItem::Overline { bounds, .. }
                | DisplayListItem::Strikethrough { bounds, .. } => bounds,
                _ => return None,
            };
            let r = bounds.inner();
            Some(Line { y: r.origin.y, height: r.size.height })
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-2
}

/// The baseline of the first glyph and the lines of a block of `ab` in `family` under `css`.
fn under(family: &str, css: &str) -> (f32, Vec<Line>) {
    let mut lw = window();
    assert!(lw.font_manager.registered_fonts.register_font(LINED, &lined_face()), "premise");
    let dl = lay_out(&mut lw, block(family, css, vec![text("ab")]), 400.0);
    (painted(&dl)[0].y, lines(&dl))
}

#[test]
fn from_font_thickness_is_the_thickness_the_face_names() {
    let (base, found) = under(LINED, "text-decoration: underline; text-decoration-thickness: from-font");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(close(found[0].height, 1.2), "60 units of 1000 at 20 px: {found:?}");
    assert!(close(found[0].y - base, 2.4), "the place stays `auto`'s: {found:?}");

    let (_, found) = under(LINED, "text-decoration: overline; text-decoration-thickness: from-font");
    assert!(close(found[0].height, 1.2), "an overline takes the underline's: {found:?}");

    let (base, found) =
        under(LINED, "text-decoration: line-through; text-decoration-thickness: from-font");
    assert!(close(found[0].height, 1.8), "90 units of 1000 at 20 px: {found:?}");
    assert!(close(found[0].y - base, -6.0 - 0.1), "grown both ways around the middle: {found:?}");
}

#[test]
fn from_font_position_puts_the_underline_where_the_face_does() {
    let (base, found) = under(LINED, "text-decoration: underline; text-underline-position: from-font");
    assert!(close(found[0].y - base, 2.0), "100 units of 1000 at 20 px: {found:?}");
    assert!(close(found[0].height, 1.6), "the thickness stays `auto`'s: {found:?}");
    // moved by the offset as the other places are
    let (base, found) = under(
        LINED,
        "text-decoration: underline; text-underline-position: from-font; text-underline-offset: 3px",
    );
    assert!(close(found[0].y - base, 5.0), "{found:?}");
    // an overline and a line-through do not move
    let (base, found) = under(LINED, "text-decoration: overline; text-underline-position: from-font");
    assert!(close(found[0].y - base, -17.0), "{found:?}");
}

#[test]
fn both_are_parts_of_the_font_size_of_the_box_that_sets_the_line() {
    let (base, found) = under(
        LINED,
        "font-size: 40px; text-decoration: underline; text-decoration-thickness: from-font; \
         text-underline-position: from-font",
    );
    assert!(close(found[0].y - base, 4.0) && close(found[0].height, 2.4), "{found:?}");

    // under a smaller span the line of the block is still the block's, from the face of the run
    let mut lw = window();
    assert!(lw.font_manager.registered_fonts.register_font(LINED, &lined_face()), "premise");
    let dom = block(
        LINED,
        "text-decoration: underline; text-decoration-thickness: from-font; \
         text-underline-position: from-font",
        vec![text("ab"), span("font-size: 10px", "cd")],
    );
    let dl = lay_out(&mut lw, dom, 400.0);
    let base = painted(&dl)[0].y;
    let found = lines(&dl);
    assert_eq!(found.len(), 2, "{found:?}");
    for line in &found {
        assert!(close(line.y - base, 2.0) && close(line.height, 1.2), "{found:?}");
    }
}

/// A face that names no underline gives `auto`: the mock faces have none in their `post` table (0
/// and 0), and `Azul Mock Mono` names a strike-through of 40 units, 0.8 px at 20 px, which is
/// drawn 1 px thick, the least a line is.
#[test]
fn a_face_that_names_none_gives_auto() {
    let (base, found) = under(
        MONO,
        "text-decoration: underline; text-decoration-thickness: from-font; \
         text-underline-position: from-font",
    );
    assert!(close(found[0].y - base, 2.4) && close(found[0].height, 1.6), "{found:?}");
    let (_, found) = under(MONO, "text-decoration: overline; text-decoration-thickness: from-font");
    assert!(close(found[0].height, 1.6), "{found:?}");
    let (_, found) = under(
        MONO,
        "text-decoration: line-through; text-decoration-thickness: from-font",
    );
    assert!(close(found[0].height, 1.0), "40 units of 1000 at 20 px, at least 1 px: {found:?}");
}

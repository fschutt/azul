//! `text-shadow` reaches the display list: the text of a block with a shadow is painted between a
//! `PushTextShadow` that carries the shadow (its offsets, its blur, its colour) and a
//! `PopTextShadow`, so that both rasterizers draw it under the glyphs; a block without a shadow has
//! neither item.
//!
//! The face is `Azul Mock Mono`.

use azul_layout::solver3::display_list::{DisplayList, DisplayListItem};

use crate::text_rig::{block, laid_out, lay_out, span, window, MONO};

/// The positions of the shadow pushes, the pops and the text runs of the list.
fn marks(dl: &DisplayList) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let at = |wanted: fn(&DisplayListItem) -> bool| {
        dl.items
            .iter()
            .enumerate()
            .filter(|(_, item)| wanted(item))
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    (
        at(|item| matches!(item, DisplayListItem::PushTextShadow { .. })),
        at(|item| matches!(item, DisplayListItem::PopTextShadow)),
        at(|item| matches!(item, DisplayListItem::Text { .. })),
    )
}

/// The shadow of the first push of the list: `(x, y, blur, red, green, blue, alpha)`.
fn first_shadow(dl: &DisplayList) -> (f32, f32, f32, u8, u8, u8, u8) {
    dl.items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::PushTextShadow { shadow } => Some((
                shadow.offset_x.inner.number.get(),
                shadow.offset_y.inner.number.get(),
                shadow.blur_radius.inner.number.get(),
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            )),
            _ => None,
        })
        .expect("the list has a text shadow")
}

#[test]
fn a_block_without_a_shadow_has_no_shadow_items() {
    for css in ["", "text-shadow: none"] {
        let (_lw, dl) = laid_out(MONO, css, "ab", 400.0);
        let (pushes, pops, texts) = marks(&dl);
        assert!(!texts.is_empty(), "`{css}`: the text is painted");
        assert!(pushes.is_empty() && pops.is_empty(), "`{css}`: {pushes:?} {pops:?}");
    }
}

#[test]
fn a_shadow_carries_its_offsets_its_blur_and_its_colour() {
    let (_lw, dl) = laid_out(MONO, "text-shadow: 2px 3px 4px #ff0000", "ab", 400.0);
    assert_eq!(first_shadow(&dl), (2.0, 3.0, 4.0, 255, 0, 0, 255));
    let (_lw, dl) = laid_out(MONO, "text-shadow: -1px 5px rgba(0, 0, 255, 0.5)", "ab", 400.0);
    let (x, y, blur, r, g, b, a) = first_shadow(&dl);
    assert_eq!((x, y, blur, r, g, b), (-1.0, 5.0, 0.0, 0, 0, 255));
    assert!((120..=135).contains(&a), "half transparent: {a}");
}

/// The text is painted between the push and the pop.
#[test]
fn the_text_is_painted_between_the_push_and_the_pop() {
    let (_lw, dl) = laid_out(MONO, "text-shadow: 1px 1px 0px #00ff00", "ab", 400.0);
    let (pushes, pops, texts) = marks(&dl);
    assert_eq!((pushes.len(), pops.len()), (1, 1), "{pushes:?} {pops:?}");
    assert!(
        texts.iter().all(|&text| pushes[0] < text && text < pops[0]),
        "the push at {}, the text at {texts:?}, the pop at {}",
        pushes[0],
        pops[0]
    );
}

/// The shadow of a block is the shadow of everything in its line: the text of its spans too.
#[test]
fn a_shadow_covers_the_text_of_the_boxes_inside() {
    let mut lw = window();
    let dom = block(MONO, "text-shadow: 1px 1px 0px #00ff00", vec![span("", "ab"), span("", "cd")]);
    let dl = lay_out(&mut lw, dom, 400.0);
    let (pushes, pops, texts) = marks(&dl);
    assert_eq!((pushes.len(), pops.len()), (1, 1));
    assert!(texts.iter().all(|&text| pushes[0] < text && text < pops[0]), "{texts:?}");
}

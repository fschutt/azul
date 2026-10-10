//! A layout blit repaints what an ancestor paints over the mover: the focus
//! ring.
//!
//! A patched layout pass that moves a block of boxes by one delta is
//! presented by moving the block's pixels (`cpurender::
//! execute_translate_blit`) and repainting only what the move cannot
//! produce. `compute_patch_move_summary` leaves the movers' ANCESTORS out of
//! that repaint - an ancestor paints its background below its descendants.
//! Not everything, though: the engine inserts the focus ring at the end of
//! its frame, over the content (CSS 2.2 Appendix E step 10, outlines, is
//! the same layer), and a scroll container paints its bar after its
//! content. The blit repainted the bars (S1 item 5) and dragged the ring:
//! where the ring crosses the mover, its unchanged pixels moved with the
//! mover and no diff damaged them.
//!
//! The frame the backends present - the old frame, the blit, a damage-only
//! repaint of the new list - must look like a full repaint of the new list.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{ColorU, PixelValue},
        style::{
            border_radius::StyleBorderRadius, BorderStyle, LayoutBorderBottomWidth,
            LayoutBorderLeftWidth, LayoutBorderRightWidth, LayoutBorderTopWidth,
            StyleBorderBottomColor, StyleBorderBottomStyle, StyleBorderLeftColor,
            StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle,
            StyleBorderTopColor, StyleBorderTopStyle,
        },
    },
};
use azul_layout::{
    cpurender::{self, ScrollOffsetMap, TranslateHint},
    solver3::display_list::{
        DisplayList, DisplayListItem, StyleBorderColors, StyleBorderStyles, StyleBorderWidths,
        WindowLogicalRect,
    },
};

use super::a_scroll_box_keeps_its_blit_on_a_scrolled_page::{
    fill, first_difference, rect, rgb, Raster,
};

/// The engine's focus ring (`LayoutWindow::apply_text_tweens`): a 2px solid
/// accent border around the focused box inflated by 2px, square here.
fn focus_ring(r: azul_core::geom::LogicalRect) -> DisplayListItem {
    let accent = ColorU {
        r: 43,
        g: 87,
        b: 154,
        a: 255,
    };
    let solid = BorderStyle::Solid;
    let px = PixelValue::px(2.0);
    DisplayListItem::Border {
        bounds: WindowLogicalRect(r),
        widths: StyleBorderWidths {
            top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth { inner: px })),
            right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth {
                inner: px,
            })),
            bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth {
                inner: px,
            })),
            left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth { inner: px })),
        },
        colors: StyleBorderColors {
            top: Some(CssPropertyValue::Exact(StyleBorderTopColor {
                inner: accent,
            })),
            right: Some(CssPropertyValue::Exact(StyleBorderRightColor {
                inner: accent,
            })),
            bottom: Some(CssPropertyValue::Exact(StyleBorderBottomColor {
                inner: accent,
            })),
            left: Some(CssPropertyValue::Exact(StyleBorderLeftColor {
                inner: accent,
            })),
        },
        styles: StyleBorderStyles {
            top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                inner: solid,
            })),
            right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                inner: solid,
            })),
            bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                inner: solid,
            })),
            left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                inner: solid,
            })),
        },
        border_radius: StyleBorderRadius {
            top_left: PixelValue::px(0.0),
            top_right: PixelValue::px(0.0),
            bottom_left: PixelValue::px(0.0),
            bottom_right: PixelValue::px(0.0),
        },
    }
}

/// A focused list box (20,20 160x120) holding a card at `y` that overflows
/// its bottom edge, then the list's focus ring - inserted last, over the
/// card. The ring's bottom side (y 140..142) crosses the card.
fn list_with_the_card_at(y: f32) -> DisplayList {
    DisplayList {
        items: vec![
            fill(rect(0.0, 0.0, 200.0, 200.0), rgb(255, 255, 255)),
            fill(rect(20.0, 20.0, 160.0, 120.0), rgb(230, 230, 230)),
            fill(rect(40.0, y, 120.0, 60.0), rgb(30, 60, 200)),
            focus_ring(rect(18.0, 18.0, 164.0, 124.0)),
        ],
        ..Default::default()
    }
}

/// The card moves 10px down (a box above it grew); the ring does not move.
#[test]
fn a_layout_blit_repaints_the_focus_ring_it_dragged() {
    let old = list_with_the_card_at(100.0);
    let new = list_with_the_card_at(110.0);
    let at_rest = ScrollOffsetMap::new();
    let mover = rect(40.0, 100.0, 120.0, 60.0);
    let hint = TranslateHint {
        delta: (0.0, 10.0),
        region_old: mover,
    };

    let mut raster = Raster::new();
    let mut presented = raster.full(&old, &at_rest);
    let blit = cpurender::execute_translate_blit(&mut presented, &hint, &[], &[mover], &new, 1.0);
    raster.repaint(&new, &mut presented, &at_rest, &blit.damage);
    let expected = raster.full(&new, &at_rest);

    assert_eq!(
        first_difference(&presented, &expected),
        None,
        "the blit moved the card and the ring's bottom side over it by 10px; the ring did not \
         move and must be repainted where it is and where its pixels were dragged to (damage \
         {:?})",
        blit.damage
    );
}

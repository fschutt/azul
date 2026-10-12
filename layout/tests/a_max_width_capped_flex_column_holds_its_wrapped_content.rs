//! A flex column whose width its max-width caps is as tall as its content
//! laid out at that capped width.
//!
//! A modal's panel: a column flex item (width auto, max-width) of a
//! centring flex row, holding text wider than the cap. Chrome finds the
//! panel's width first - its max-content width, capped by max-width - and
//! then lays the content out at that width: the text wraps and the panel
//! grows with it. azul measured the panel's width with its content laid out
//! at the uncapped width (the text on one line), cached that size, and
//! served its height for the capped width: the panel stayed short by the
//! wrapped lines and its last row - the dialog's OK - hung out of it
//! (AzShow's About box). Box sizes come from fixed-size inline-blocks, so
//! the numbers do not depend on the fonts.

use crate::table_markup::{body, near, rect, words};

#[test]
fn a_max_width_capped_flex_column_holds_its_wrapped_content() {
    let lw = body(&format!(
        "<div style=\"position: absolute; top: 0; left: 0; width: 100%; height: 100%; \
         display: flex; flex-direction: row; justify-content: center; align-items: center\">\
         <div id=\"panel\" style=\"display: flex; flex-direction: column; max-width: 300px; \
         padding: 0 10px 10px 10px; border: 1px solid black\">\
         <div style=\"height: 20px\"></div>\
         <div style=\"display: block; flex-grow: 1\">\
         <div style=\"display: flex; flex-direction: column\">\
         <div id=\"text\">{}</div>\
         <div id=\"footer\" style=\"height: 30px\"></div>\
         </div></div></div></div>",
        words(16, 50)
    ));
    let panel = rect(&lw, "panel");
    let text = rect(&lw, "text");
    let footer = rect(&lw, "footer");
    assert!(
        near(panel.size.width, 322.0, 0.5),
        "the panel is capped at 300px plus its padding and border: {}",
        panel.size.width
    );
    assert!(
        text.size.height >= 20.0,
        "sixteen 50px words wrap at 300px into several lines: {}",
        text.size.height
    );
    let content_bottom = panel.origin.y + panel.size.height - 1.0 - 10.0;
    let footer_bottom = footer.origin.y + footer.size.height;
    assert!(
        footer_bottom <= content_bottom + 0.5,
        "the last row stays inside the panel: it ends at {footer_bottom}, the panel's content \
         box at {content_bottom} (the panel is {} tall, its text {})",
        panel.size.height,
        text.size.height
    );
}

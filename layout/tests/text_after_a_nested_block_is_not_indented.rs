//! Text after a nested block is not indented (CSS 2.1 s16.1, CSS Text 3
//! s8.1: `text-indent` applies to the FIRST formatted line of a block
//! container). Found by TEXT7 (wave 7), handed to LAYOUT7.
//!
//! `<div style="text-indent: 40px">first<div>nested</div>after</div>`:
//! "first" is the container's first line (indented), the nested block
//! inherits `text-indent` and indents its own first line, and "after" - in
//! the anonymous block after the nested one - is no first line of anything:
//! not indented. The anonymous block borrowed the container's `text-indent`
//! whole, so azul indented it too.
//!
//! Measured in headless Chrome 154 (fixed-size inline boxes: font-free x).
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

fn boxed(id: &str) -> String {
    format!("<i id=\"{id}\" style=\"display: inline-block; width: 10px; height: 10px\"></i>")
}

#[test]
fn the_text_after_a_nested_block_starts_at_the_content_edge() {
    let lw = body(&format!(
        "<div style=\"width: 300px; text-indent: 40px; line-height: 20px\">{}<div>{}</div>{}\
         </div>",
        boxed("a"),
        boxed("b"),
        boxed("c")
    ));
    let a = rect(&lw, "a");
    assert!(
        near(a.origin.x, 40.0, 0.5),
        "the first line is indented (Chrome 40): {a:?}"
    );
    let b = rect(&lw, "b");
    assert!(
        near(b.origin.x, 40.0, 0.5),
        "the nested block inherits text-indent for its own first line (Chrome 40): {b:?}"
    );
    let c = rect(&lw, "c");
    assert!(
        near(c.origin.x, 0.0, 0.5),
        "the line after the nested block is no first line (Chrome 0): {c:?}"
    );
}

#[test]
fn a_container_that_starts_with_a_block_indents_no_anonymous_line() {
    let lw = body(&format!(
        "<div style=\"width: 300px; text-indent: 40px; line-height: 20px\"><div>{}</div>{}\
         </div>",
        boxed("b"),
        boxed("c")
    ));
    let c = rect(&lw, "c");
    assert!(
        near(c.origin.x, 0.0, 0.5),
        "the first formatted line was the nested block's (Chrome 0): {c:?}"
    );
}

//! A VirtualView's hit test must place its child DOM where the RENDERER puts
//! it — same three rects, same three terms.
//!
//! A VirtualView involves three rectangles and it is easy to use two:
//!
//!   outer        `bounds`        — where the VirtualView itself sits
//!   materialized `content_offset` — which slice of the document was built,
//!                                  minus how far the user has scrolled
//!   virtual      `virtual_rect`   — the whole document, for scrollbar maths
//!
//! The display list carries all three. cpurender/raster.rs places child content
//! at `bounds.origin + content_offset` (it pushes
//! `scroll - vv_origin - content_offset` onto the offset stack). The hit-test
//! placement in headless.rs used `bounds.origin` ALONE and discarded
//! `content_offset` through a `..` pattern.
//!
//! The result was not a missed hit — the correct text node was found and a
//! caret was produced — but at the wrong CHARACTER, off by exactly
//! `materialized_origin - scroll_offset`. That is zero on the first screenful
//! and grows as you scroll, so clicking looked fine at the top of a document
//! and progressively wrong further down, and dragging selected the wrong range.
//!
//! This asserts the two sites still agree, by reading the source: the renderer
//! and the hit test derive placement independently, in different crates'
//! modules, and nothing else ties them together.

const RASTER: &str = include_str!("../src/cpurender/raster.rs");
const HEADLESS: &str = include_str!("../src/headless.rs");

/// The window of `src` around the first occurrence of `marker`, with COMMENT
/// lines removed.
///
/// Stripping comments is the whole point. The first version of this test
/// searched the raw text for "content_offset" and passed even with the fix
/// reverted — because the explanatory comment above the code says
/// "content_offset" a dozen times. A guard that matches its own prose cannot
/// fail, which is the same defect as the `[reuse]` branch and the `xcrun -p`
/// probe this codebase has already been bitten by twice.
fn around(src: &str, marker: &str, lines: usize) -> String {
    let at = src
        .find(marker)
        .unwrap_or_else(|| panic!("marker {marker:?} not found — did the code move?"));
    src[at..]
        .lines()
        .take(lines)
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_renderer_places_virtualview_content_by_bounds_and_content_offset() {
    // Renderer side: the offset stack subtracts BOTH the VirtualView origin and
    // the content offset. If this stops being true, the test below is measuring
    // agreement with something that no longer exists.
    let body = around(RASTER, "scroll_offset_stack.push((", 6);
    assert!(
        body.contains("vv_origin.x") && body.contains("content_offset.x"),
        "the renderer no longer places VirtualView content by \
         `bounds.origin + content_offset`; found:\n{body}"
    );
}

#[test]
fn the_hit_test_places_virtualview_content_the_same_way() {
    let body = around(HEADLESS, "I::VirtualView {", 40);
    assert!(
        body.contains("content_offset.x") && body.contains("content_offset.y"),
        "the VirtualView hit-test placement dropped `content_offset`. The \
         renderer draws the child at `bounds.origin + content_offset`; a \
         placement built from `bounds.origin` alone maps clicks as though the \
         materialized window began at row 0 and nothing had scrolled. Clicks \
         still land on a text node and still produce a caret — at the wrong \
         character, off by `materialized_origin - scroll_offset`, growing as \
         the user scrolls.\n\nfound:\n{body}"
    );
}

//! Tests of which line a list marker rides and where the text beside it starts: on the first
//! line of the item's content, at the line's start, whatever white space the markup puts between
//! the marker and the text.

use std::path::Path;

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources};
use rust_fontconfig::{FcFontCache, UnicodeRange};

use crate::{
    callbacks::ExternalSystemCallbacks,
    text3::cache::{MemoryFontTier, ShapedItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// The clusters of the one inline formatting context that holds a marker: the marker's, as
/// `(line index, x)`, and the text's, as `(text, line index, x)`.
struct Placed {
    marker: Vec<(usize, f32)>,
    text: Vec<(String, usize, f32)>,
}

impl Placed {
    /// The first letter of the text: its line and its x. (A white space the line kept in front of
    /// it is a cluster of the text too; it is not what these tests look for.)
    fn first_letter(&self) -> (usize, f32) {
        let (_, line, x) = self
            .text
            .iter()
            .find(|(text, _, _)| text == "x")
            .expect("the item holds an `x`");
        (*line, *x)
    }

    /// How many lines hold a letter.
    fn lines_of_letters(&self) -> usize {
        self.text
            .iter()
            .filter(|(text, _, _)| text == "x")
            .map(|(_, line, _)| *line)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

/// Lays `body` out in a window 800 px wide, `::marker { font-family: inherit }` as the web
/// platform tests write it, with the proportional test font (no system font is read), and
/// returns the clusters of the one inline formatting context that holds a marker.
fn place(body: &str) -> Placed {
    let xml = format!(
        "<html><head><style>::marker {{ font-family: inherit; }}</style></head><body>{body}</body></html>"
    );
    let styled = crate::xml::parse_xml_to_styled_dom(&xml).expect("the page parses");
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let font = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/azul-mock-prop.ttf"))
        .expect("the proportional test font");
    let ascii = || vec![UnicodeRange { start: 0x20, end: 0x7E }];
    lw.font_manager.register_named_font("Azul Mock Prop", &font, ascii());
    for generic in ["serif", "sans-serif", "monospace", "system-ui", "cursive", "fantasy"] {
        lw.font_manager
            .register_named_font_in_tier(generic, &font, ascii(), MemoryFontTier::Fallback);
    }
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");

    let result = &lw.layout_results[&DomId::ROOT_ID];
    let mut placed = Placed { marker: Vec::new(), text: Vec::new() };
    for warm in &result.layout_tree.warm {
        let Some(inline) = warm.inline_layout_result.as_ref() else {
            continue;
        };
        let holds_marker = inline.layout.items.iter().any(|positioned| {
            matches!(&positioned.item, ShapedItem::Cluster(c) if c.marker_position_outside.is_some())
        });
        if !holds_marker {
            continue;
        }
        for item in &inline.layout.items {
            if let ShapedItem::Cluster(c) = &item.item {
                if c.marker_position_outside.is_some() {
                    placed.marker.push((item.line_index, item.position.x));
                } else {
                    placed.text.push((c.text().to_string(), item.line_index, item.position.x));
                }
            }
        }
    }
    assert!(!placed.marker.is_empty(), "the list item has a marker in one of its lines");
    placed
}

/// `<li><p>\n  xx<br/>xx\n</p></li>` in a box that is as wide as its text (`display: inline-block`):
/// the white space between the marker and the first `xx` is at the start of the line, and the box
/// was sized without it (the intrinsic widths remove it). The line kept it: it measured a space
/// wider than the box, the first `xx` wrapped under the marker, and the marker stood alone on a
/// first line of its own (WPT css-lists/list-marker-alignment).
#[test]
fn a_marker_shares_its_line_with_the_text_that_follows_white_space_in_a_shrink_wrapped_box() {
    let placed = place(
        "<div style=\"display:inline-block;\"><ul><li>\n<p>\n  xx<br/>xx\n</p>\n</li></ul></div>",
    );
    let (line, x) = placed.first_letter();
    assert_eq!(line, placed.marker[0].0, "marker {:?}, text {:?}", placed.marker, placed.text);
    assert_eq!(x, 0.0, "the text starts the line, with no space before it");
    // The item is two lines tall, one per `xx`, not three.
    assert_eq!(placed.lines_of_letters(), 2, "text {:?}", placed.text);
}

/// The text of a list item starts at the line's start, as the text of a paragraph does: the white
/// space the markup puts after the marker is removed, not kept as a space in front of the text -
/// in the item itself, and in a paragraph in it, and in a box as wide as the page.
#[test]
fn the_text_of_a_list_item_starts_its_line_whatever_white_space_follows_the_marker() {
    for (what, body) in [
        ("an item holding text", "<ul><li>\n  xx<br/>xx\n</li></ul>"),
        ("an item holding a paragraph", "<ul><li>\n<p>\n  xx<br/>xx\n</p>\n</li></ul>"),
        (
            "a shrink-wrapped item holding text",
            "<div style=\"display:inline-block;\"><ul><li>\n  xx<br/>xx\n</li></ul></div>",
        ),
    ] {
        let placed = place(body);
        let (line, x) = placed.first_letter();
        assert_eq!(line, placed.marker[0].0, "{what}: marker {:?}, text {:?}", placed.marker, placed.text);
        assert_eq!(x, 0.0, "{what}: the text starts the line, with no space before it");
    }
}

/// The same item with no white space at all: where it always was.
#[test]
fn a_marker_shares_its_line_with_the_text_that_follows_it_directly() {
    let placed = place("<div style=\"display:inline-block;\"><ul><li><p>xx<br/>xx</p></li></ul></div>");
    let (line, x) = placed.first_letter();
    assert_eq!(line, placed.marker[0].0, "marker {:?}, text {:?}", placed.marker, placed.text);
    assert_eq!(x, 0.0);
}

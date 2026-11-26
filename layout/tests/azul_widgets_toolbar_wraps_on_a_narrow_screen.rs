//! The AzWidgets toolbar wraps on a phone.
//!
//! The bar under the titlebar (Mode: System / Light / Dark + its note, Theme: Flat / Flora) is a flex row.
//! Written as `flex-shrink: 0` with a 24 px gap and no `flex-wrap`, it is as wide as its children side by
//! side, about 800 px, on a screen 411 dp wide: the Theme group ran off the right edge of the Android
//! emulator (run 45766660267, 1080 x 2400 at 2.625) and nothing could reach it. A row that may not shrink
//! must be allowed to wrap, in the bar, in each of its groups, and in the control of the Mode group.
//!
//! The demo is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_widgets_demo_follows_the_theme.rs` does: every flex row the `toolbar` function styles says
//! `flex-wrap: wrap`, and the bar itself has a row gap for the second line.

/// `examples/azul-widgets/src/lib.rs`, verbatim, at compile time.
const DEMO: &str = include_str!("../../examples/azul-widgets/src/lib.rs");

/// The text of `fn toolbar(` up to the closing brace at column 0.
fn toolbar_source() -> &'static str {
    let start = DEMO.find("fn toolbar(").expect("the demo has a `fn toolbar(`");
    let rest = &DEMO[start..];
    let end = rest.find("\n}\n").expect("the toolbar function ends with a brace at column 0");
    &rest[..end]
}

/// The string literals of `src` that style a flex row (`display: flex; flex-direction: row`).
fn flex_row_styles(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut literal = String::new();
    let mut in_str = false;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_str {
            if c == '"' {
                in_str = true;
                literal.clear();
            }
            continue;
        }
        match c {
            '"' => {
                in_str = false;
                let flat: String = literal.split_whitespace().collect::<Vec<_>>().join(" ");
                if flat.contains("display: flex") && flat.contains("flex-direction: row") {
                    out.push(flat);
                }
            }
            '\\' => match chars.next() {
                // a `\` at the end of a line continues the literal on the next one
                Some('\n') => {
                    while chars.peek().is_some_and(|n| n.is_whitespace()) {
                        chars.next();
                    }
                }
                Some(other) => literal.push(other),
                None => {}
            },
            _ => literal.push(c),
        }
    }
    out
}

#[test]
fn the_toolbar_has_flex_rows_to_check() {
    // the three rows: the bar, a labelled group, the Mode group's control
    assert!(
        flex_row_styles(toolbar_source()).len() >= 3,
        "the reader found {:?}: the toolbar's styles changed shape, update this test",
        flex_row_styles(toolbar_source())
    );
}

#[test]
fn every_flex_row_of_the_toolbar_wraps() {
    for style in flex_row_styles(toolbar_source()) {
        assert!(
            style.contains("flex-wrap: wrap"),
            "a toolbar row that cannot wrap runs off a 411 dp screen: `{style}`"
        );
    }
}

#[test]
fn the_bar_has_a_row_gap_for_its_second_line() {
    let bar = flex_row_styles(toolbar_source())
        .into_iter()
        .find(|s| s.contains("border-bottom") && s.contains("padding: 8px 24px"))
        .expect("the bar's own style (padding 8px 24px, a bottom border)");
    assert!(bar.contains("row-gap:"), "the wrapped line sits on the first one: `{bar}`");
}

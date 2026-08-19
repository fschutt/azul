//! The widget label-convention manifest must not silently lose coverage.
//!
//! Seam-audit R4 ("enforce the P-wrap convention in the DOM builder, not in
//! review") landed as three pieces: the renamed raw-text constructor
//! `Dom::create_text_do_not_use_without_block_level_wrapper`, the `dom_lint`
//! pass, and two workspace tests that walk every widget's `dom()` —
//! `widgets::label_convention::no_widget_attaches_state_to_a_rect_less_text_node`
//! and `dom_lint::every_widget_dom_is_warning_free`.
//!
//! Both of those walk whatever `every_widget_dom()` hands them, and
//! `every_widget_dom()` is a HAND-WRITTEN `vec![…]`. That is the hole this file
//! closes: a widget module added to `layout/src/widgets/mod.rs` and never
//! registered in that vec is not "failing the convention test", it is invisible
//! to it — both tests stay green while coverage quietly shrinks. Neither test
//! can notice, because neither has any idea how many widgets there are supposed
//! to be.
//!
//! So this file compares the two lists at the only place they both exist as
//! ground truth: the source text of `widgets/mod.rs`. It deliberately links
//! nothing from the crate — it is pure `include_str!` plus string matching, so
//! it stays honest even if the widget API changes shape.
//!
//! TO TURN IT RED: add `pub mod fancy_new_widget;` to
//! `layout/src/widgets/mod.rs` without adding a `("fancy_new_widget", …)` entry
//! to `every_widget_dom()`.

/// `layout/src/widgets/mod.rs`, verbatim, at compile time.
const WIDGETS_MOD_SRC: &str = include_str!("../src/widgets/mod.rs");

/// The signature that opens the manifest. If this stops matching, the tests
/// below fail loudly rather than silently scanning an empty string.
const MANIFEST_FN: &str = "pub(super) fn every_widget_dom()";

/// Widget modules deliberately absent from the manifest.
///
/// An entry here is a claim that the module CANNOT violate the label
/// convention — normally because it emits no text node at all. The claim is
/// re-checked by `the_exemption_list_has_no_stale_entries`.
const EXEMPT: &[(&str, &str)] = &[
    (
        "capture_common",
        "shared capture plumbing: declares no widget type and no dom()",
    ),
    (
        "camera",
        "single replaced node, zero create_text_* calls; camera.rs pins children == 0",
    ),
    (
        "microphone",
        "single replaced node, zero create_text_* calls; microphone.rs pins children == 0",
    ),
    (
        "screencap",
        "single replaced node, zero create_text_* calls; screencap.rs pins children == 0",
    ),
    ("video", "single replaced node, zero create_text_* calls"),
];

/// Every `pub mod x;` / `mod x;` declared in `widgets/mod.rs`.
///
/// Inline modules (`mod label_convention {`) are excluded by requiring the
/// trailing semicolon, which is what keeps the test harness itself out of the
/// widget list.
fn declared_widget_modules() -> Vec<&'static str> {
    WIDGETS_MOD_SRC
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("mod "))?;
            rest.strip_suffix(';')
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// The body of `every_widget_dom()`, from its signature to its closing brace.
fn manifest_body() -> &'static str {
    let start = WIDGETS_MOD_SRC.find(MANIFEST_FN).unwrap_or_else(|| {
        panic!(
            "could not find `pub(super) fn every_widget_dom()` in layout/src/widgets/mod.rs — it \
             was renamed or moved; update MANIFEST_FN in this test rather than deleting the test"
        )
    });
    let rest = &WIDGETS_MOD_SRC[start..];
    let end = rest.find("\n    }\n").unwrap_or_else(|| {
        panic!(
            "could not find the closing brace of `pub(super) fn every_widget_dom()` — the file \
             was reindented; update this test rather than deleting it"
        )
    });
    &rest[..end]
}

/// Does the manifest register this module?
///
/// Every entry is a `(&'static str, Dom)` pair whose name is the module name,
/// either exactly (`"button"`) or as a prefix when one module contributes
/// several fixtures (`"tabs (header)"`).
fn is_registered(body: &str, module: &str) -> bool {
    body.contains(format!("\"{module}\"").as_str())
        || body.contains(format!("\"{module} ").as_str())
}

#[test]
fn every_widget_module_is_registered_in_the_lint_manifest() {
    let modules = declared_widget_modules();
    // A zero is not a measurement: a parser that finds nothing would make this
    // test pass vacuously, which is the exact failure mode it exists to prevent.
    assert!(
        modules.len() >= 40,
        "parsed only {} widget modules out of layout/src/widgets/mod.rs — the parser broke, and \
         an empty list would make this whole test pass while proving nothing",
        modules.len()
    );

    let body = manifest_body();
    assert!(
        body.len() > 1000,
        "the extracted manifest body is only {} bytes — the extraction broke",
        body.len()
    );

    let missing: Vec<&str> = modules
        .iter()
        .copied()
        .filter(|&m| !EXEMPT.iter().any(|&(name, _)| name == m) && !is_registered(body, m))
        .collect();

    assert!(
        missing.is_empty(),
        "widget module(s) missing from `every_widget_dom()` in layout/src/widgets/mod.rs: \
         {missing:?}\n\nThe label-convention walk and the dom_lint warning check both iterate \
         that manifest, so an unregistered widget is not merely untested — it is INVISIBLE to \
         both, and they stay green while its text nodes go unwatched. Add a `(\"name\", \
         Widget::create(..).dom())` entry, or, if the widget provably emits no text node, add it \
         to EXEMPT in this file with the reason."
    );
}

#[test]
fn the_exemption_list_has_no_stale_entries() {
    let modules = declared_widget_modules();
    let body = manifest_body();

    for &(name, reason) in EXEMPT {
        assert!(
            modules.contains(&name),
            "`{name}` is exempted from the widget lint manifest but is no longer a widget module \
             — delete the exemption (reason on file: {reason})"
        );
        assert!(
            !is_registered(body, name),
            "`{name}` is now registered in `every_widget_dom()`, so its exemption is stale and \
             is masking nothing — delete it from EXEMPT in this file (reason on file: {reason})"
        );
    }
}

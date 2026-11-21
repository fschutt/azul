//! The renderer-side resource counters (`fonts`, `font_hash_map`, `images`,
//! `image_key_map`) of a host with no renderer count what the display lists
//! draw, driven through the real op dispatch (`run_e2e_test`) on a headless
//! window, and straight on a layout window (E2E-R8).
//!
//! The in-process runner paints on the CPU and registers nothing with a
//! renderer, so those four tables stayed empty whatever a scenario did:
//! `"fonts": "eq"` was `0 == 0` (a leak check that could not fail), and the
//! `"images": "gt"` control that proves the image was ever loaded could not
//! pass (`gen-e2e` batch 1, scenarios 13-18: `set_node_image`,
//! `add_image_to_cache` and `background-image: url(id)` moved no counter).

use std::collections::BTreeSet;

use azul_core::{
    dom::Dom,
    resources::{
        FontKey, IdNamespace, ImageCache, ImageKey, ImageRef, ImageRefHash, RawImage, RawImageData,
        RawImageFormat, RendererResources,
    },
    styled_dom::StyledDom,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        layout::dimensions::{LayoutHeight, LayoutWidth},
        property::CssProperty,
    },
};
use rust_fontconfig::FcFontCache;

use super::{
    e2e_resource_counts, e2e_set_virtual_renderer, run_e2e_test, E2eTest, E2eTestResult,
    VirtualRenderer,
};
use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

/// Run `steps` as one headless scenario on a 400x300 window. A red step does not
/// end the run, so every assertion is read.
fn run(name: &str, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every red step, one line each - what a failing run prints.
fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(result)
    );
}

/// A mounted page that draws neither text nor a picture.
fn bare_page() -> serde_json::Value {
    serde_json::json!({
        "op": "mount",
        "html": ["<div id=\"root\"></div>"],
        "css": ["#root { width: 100px; height: 40px; background-color: blue; }"],
    })
}

/// E2E-R8: text in a font makes a font; the page that no longer draws it makes it
/// go. (The old counters read 0 on all three assertions: the `gt` could never
/// pass, and the two `eq` were `0 == 0`.)
#[test]
fn e2e_r8_a_page_with_text_holds_a_font_and_the_page_without_it_holds_none() {
    let result = run(
        "e2e_r8_font",
        vec![
            bare_page(),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "snapshot_resources", "as": "bare" }),
            // A page with nothing to draw holds nothing.
            serde_json::json!({
                "op": "assert_resource_counts", "fonts": 0, "font_hash_map": 0,
            }),
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"root\"><p id=\"txt\">hello</p></div>"],
                "css": [
                    "#root { width: 100px; height: 40px; background-color: blue; }",
                    "#txt { font-family: \"Azul Mock Mono\"; font-size: 20px; color: black; }",
                ],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "assert_exists", "selector": "#txt" }),
            // The liveness half: the font is there.
            serde_json::json!({
                "op": "assert_resource_counts", "vs": "bare",
                "fonts": "gt", "font_hash_map": "gt",
            }),
            bare_page(),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "assert_not_exists", "selector": "#txt" }),
            // The leak half: no list draws it, so it is gone - at once, not after
            // the frames a shell keeps a font for.
            serde_json::json!({
                "op": "assert_resource_counts", "vs": "bare",
                "fonts": "eq", "font_hash_map": "eq",
            }),
        ],
    );
    assert_passes(&result);
}

/// E2E-R8: `set_node_image` puts an image in the list, and a swap replaces it
/// (the red picture is not held on to next to the lime one).
#[test]
fn e2e_r8_a_node_image_holds_one_image_and_a_swap_does_not_hold_two() {
    let result = run(
        "e2e_r8_node_image",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"root\"><img id=\"pic\"/></div>"],
                "css": ["#pic { width: 80px; height: 60px; }"],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "snapshot_resources", "as": "bare" }),
            // The `<img>` placeholder is a null image: it uploads nothing.
            serde_json::json!({
                "op": "assert_resource_counts", "images": 0, "image_key_map": 0,
            }),
            serde_json::json!({
                "op": "set_node_image", "selector": "#pic",
                "width": 80, "height": 60, "color": "red",
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "assert_resource_counts", "vs": "bare",
                "images": "gt", "image_key_map": "gt",
            }),
            serde_json::json!({
                "op": "assert_resource_counts", "images": 1, "image_key_map": 1,
            }),
            serde_json::json!({ "op": "snapshot_resources", "as": "red" }),
            serde_json::json!({
                "op": "set_node_image", "selector": "#pic",
                "width": 80, "height": 60, "color": "lime",
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "assert_resource_counts", "vs": "red",
                "images": "eq", "image_key_map": "eq",
            }),
        ],
    );
    assert_passes(&result);
}

/// E2E-R8: an image registered under a css id is ONE image however many boxes
/// draw it, and it goes when the registration does.
#[test]
fn e2e_r8_a_css_id_image_is_one_image_for_every_box_that_draws_it() {
    let result = run(
        "e2e_r8_css_id_image",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"root\"><div id=\"a\" class=\"tile\"></div><div id=\"b\" class=\"tile\"></div></div>"],
                "css": [
                    "#root { display: flex; }",
                    ".tile { width: 40px; height: 40px; background-color: blue; \
                     background-image: url(\"e2e-r8-shared\"); }",
                ],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            // The id is not registered yet: no picture resolves.
            serde_json::json!({
                "op": "assert_resource_counts", "images": 0, "image_key_map": 0,
            }),
            serde_json::json!({
                "op": "add_image_to_cache", "css_id": "e2e-r8-shared",
                "width": 16, "height": 16, "color": "red",
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            // Two boxes, one picture.
            serde_json::json!({
                "op": "assert_resource_counts", "images": 1, "image_key_map": 1,
            }),
            serde_json::json!({ "op": "remove_image_from_cache", "css_id": "e2e-r8-shared" }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "assert_resource_counts", "images": 0, "image_key_map": 0,
            }),
        ],
    );
    assert_passes(&result);
}

/// E2E-R8: a counter that did not move is still `eq` (the control of the three
/// above: the counters are not noisy), and `gt` on it is red - the old
/// "all-zero" tables could not tell a flat counter from a dead one, now a flat
/// one is flat.
#[test]
fn e2e_r8_a_page_that_draws_the_same_things_holds_the_same_things() {
    let result = run(
        "e2e_r8_flat",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": ["<div id=\"root\"><p id=\"txt\">hello</p><img id=\"pic\"/></div>"],
                "css": [
                    "#txt { font-family: \"Azul Mock Mono\"; font-size: 20px; color: black; }",
                    "#pic { width: 40px; height: 40px; }",
                ],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({
                "op": "set_node_image", "selector": "#pic",
                "width": 40, "height": 40, "color": "red",
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({ "op": "snapshot_resources", "as": "drawn" }),
            serde_json::json!({ "op": "tick_ms", "ms": 1000 }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "assert_resource_counts", "vs": "drawn",
                "fonts": "eq", "font_hash_map": "eq", "images": "eq", "image_key_map": "eq",
            }),
        ],
    );
    assert_passes(&result);
}

/// A 4x4 solid RGBA picture.
fn raw_image(rgba: [u8; 4]) -> ImageRef {
    let pixels: Vec<u8> = rgba.iter().copied().cycle().take(4 * 4 * 4).collect();
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: 4,
        height: 4,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: b"e2e-r8".to_vec().into(),
    })
    .expect("a 4x4 RGBA picture")
}

/// A laid-out window whose body is `images`, one 20x20 picture node each.
fn window_drawing(images: Vec<ImageRef>) -> LayoutWindow {
    let mut body = Dom::create_body();
    for image in images {
        body = body.with_child(
            Dom::create_image(image).with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(20.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(20.0))),
                ]
                .into(),
            ),
        );
    }
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut dom = body;
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = azul_core::geom::LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// E2E-R8: the registry holds each decoded picture once (a picture drawn twice
/// is one upload), and holds no picture a renderer would not upload (a null
/// image) - and every picture it holds is one the image GC's scan knows.
#[test]
fn e2e_r8_the_registry_holds_each_decoded_picture_once() {
    let red = raw_image([255, 0, 0, 255]);
    let blue = raw_image([0, 0, 255, 255]);
    let null = ImageRef::null_image(0, 0, RawImageFormat::RGBA8, Vec::new());
    let lw = window_drawing(vec![red.clone(), red.clone(), blue.clone(), null]);

    let held = VirtualRenderer::of(&lw);
    assert_eq!(
        held.images,
        BTreeSet::from([red.get_hash(), blue.get_hash()]),
        "two decoded pictures, one of them drawn twice: two uploads, and the null one is none"
    );
    let scanned = lw.scan_used_images(&ImageCache::default());
    assert!(
        held.images.is_subset(&scanned),
        "what the registry holds is what the GC's scan finds drawn: {:?} vs {:?}",
        held.images,
        scanned
    );
}

/// E2E-R8: a window that draws no text and no picture holds nothing.
#[test]
fn e2e_r8_a_window_that_draws_nothing_holds_nothing() {
    let lw = window_drawing(Vec::new());
    assert_eq!(VirtualRenderer::of(&lw), VirtualRenderer::default());
}

/// E2E-R8: the tables of a renderer are read when the host has one, and are NOT
/// read once the host declares it has none - the declaration is what switches
/// the four counters from the (empty) tables to the display lists, in both
/// directions.
#[test]
fn e2e_r8_a_host_reads_the_tables_of_its_renderer_until_it_declares_it_has_none() {
    let mut lw = window_drawing(Vec::new());
    // What a renderer that registered one font and one picture would hold.
    lw.renderer_resources.font_hash_map.insert(
        7,
        FontKey {
            namespace: IdNamespace(1),
            key: 7,
        },
    );
    lw.renderer_resources.image_key_map.insert(
        ImageKey {
            namespace: IdNamespace(1),
            key: 9,
        },
        ImageRefHash { inner: 9 },
    );

    let own = e2e_resource_counts(&lw);
    assert_eq!(own["font_hash_map"], 1, "the renderer's own table: {own:?}");
    assert_eq!(own["image_key_map"], 1, "the renderer's own table: {own:?}");

    e2e_set_virtual_renderer(&lw);
    let virtual_counts = e2e_resource_counts(&lw);
    assert_eq!(
        virtual_counts["font_hash_map"], 0,
        "the lists draw no text: {virtual_counts:?}"
    );
    assert_eq!(
        virtual_counts["image_key_map"], 0,
        "the lists draw no picture: {virtual_counts:?}"
    );
}

/// E2E-R8: the counters a renderer never had stay 0 on a host without one - the
/// two legacy tables are not made up.
#[test]
fn e2e_r8_the_legacy_font_tables_stay_zero() {
    let lw = window_drawing(vec![raw_image([1, 2, 3, 255])]);
    e2e_set_virtual_renderer(&lw);
    let counts = e2e_resource_counts(&lw);
    assert_eq!(counts["font_id_map"], 0, "{counts:?}");
    assert_eq!(counts["font_families_map"], 0, "{counts:?}");
    assert_eq!(
        counts["images"], 1,
        "the one picture the list draws: {counts:?}"
    );
}

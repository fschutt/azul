//! The FFI `MapWidget::dom()` must route through the function that wires the
//! tile-fetch worker.
//!
//! `azul_layout::widgets::map::MapWidget::dom()` is a PLACEHOLDER: its own doc
//! says "No tile-fetch worker is wired - tiles render as placeholders". The
//! worker cannot live in azul-layout, because it pulls the MVT/Mercator tree
//! (mvt-reader, geo-types, proj4rs, geojson) that the mobile builds must not
//! carry. So the widget lives in layout, the worker lives here, and
//! `unified::map::map_widget_dom` is what marries them.
//!
//! api.json bound `MapWidget.dom` to `object.dom()` — the placeholder — so every
//! desktop build panned a map that could never paint a tile, while
//! `map_widget_dom` sat there with zero callers documenting itself as "the
//! single entry point the FFI MapWidget::dom() shims to".
//!
//! Nothing about that was visible from either side: the layout method is real
//! and compiles, the wiring function is real and compiles, and only the string
//! in api.json decides which one ships. This test is that decision, asserted.

const API_JSON: &str = include_str!("../../api.json");

/// Find the `fn_body` of `MapWidget.dom` without a JSON dependency: locate the
/// `"MapWidget"` block, then the `"dom"` key inside it, then its `fn_body`.
fn map_widget_dom_fn_body() -> String {
    let mw = API_JSON
        .find("\"MapWidget\"")
        .expect("api.json must define MapWidget");
    let block = &API_JSON[mw..];
    let dom_at = block
        .find("\"dom\": {")
        .expect("MapWidget must expose a `dom` function");
    let after = &block[dom_at..];
    let body_at = after
        .find("\"fn_body\"")
        .expect("MapWidget.dom must declare a fn_body");
    let tail = &after[body_at..];
    let open = tail[tail.find(':').expect("fn_body must have a value") + 1..].trim_start();
    let quoted = open
        .strip_prefix('"')
        .expect("fn_body must be a JSON string");
    let end = quoted.find('"').expect("unterminated fn_body");
    quoted[..end].to_string()
}

#[test]
fn ffi_map_dom_is_wired_to_the_tile_fetch_worker() {
    let body = map_widget_dom_fn_body();

    assert!(
        body.contains("map_widget_dom"),
        "api.json binds MapWidget::dom() to `{body}`, which does not go through \
         map_widget_dom. If this is `object.dom()` it is the azul-layout \
         PLACEHOLDER: the map will pan and never paint a tile, on every desktop \
         platform, with no error anywhere. Reproduce with:\n  \
         AZ_MAP_DEBUG=1 AZ_BACKEND=headless ./azul-maps\n  \
         -> `spawn_pending: ABORT - no fetch_callback on the cache`"
    );

    assert!(
        !body.contains("object.dom()"),
        "MapWidget::dom() must not call the placeholder directly, got `{body}`"
    );
}

#[test]
fn the_wiring_function_still_exists_under_the_name_api_json_calls() {
    // A rename on the Rust side with api.json left pointing at the old path
    // fails the build, not this test — but if someone "fixes" that by pointing
    // api.json back at object.dom(), the test above catches it. This one pins
    // the module path so the two stay legible together.
    let body = map_widget_dom_fn_body();
    assert!(
        body.contains("unified::map::map_widget_dom"),
        "expected the unified module path so wasm and desktop share one entry \
         point, got `{body}`"
    );
}

/// Every dll-side `*_widget_dom` wiring function must actually be reachable.
///
/// The audit that produced this: exactly two widgets keep their worker in
/// azul-dll rather than azul-layout, because the worker drags a dependency tree
/// the mobile builds must not carry — `map_widget_dom` (MVT/Mercator) and
/// `video_widget_dom` (MP4 demux + hardware decode). BOTH were bound to the
/// layout placeholder in api.json, so the map never painted a tile and the
/// video never showed anything but its built-in test pattern.
///
/// camera / screencapture / microphone are deliberately NOT in this list: they
/// have no dll-side wiring function, they build their state and start their
/// workers inside azul-layout, so `object.dom()` is correct for them.
#[test]
fn every_dll_side_wiring_function_is_reachable_from_api_json() {
    // (widget type in api.json, the wiring fn its `dom` must route through)
    const WIRED: &[(&str, &str)] = &[
        ("MapWidget", "map_widget_dom"),
        ("VideoWidget", "video_widget_dom"),
    ];

    for (widget, wiring_fn) in WIRED {
        let at = API_JSON
            .find(&alloc_fmt(widget))
            .unwrap_or_else(|| panic!("api.json must define {widget}"));
        let block = &API_JSON[at..];
        let dom_at = block
            .find("\"dom\": {")
            .unwrap_or_else(|| panic!("{widget} must expose `dom`"));
        let tail = &block[dom_at..];
        let body_at = tail
            .find("\"fn_body\"")
            .unwrap_or_else(|| panic!("{widget}.dom must declare a fn_body"));
        let rest = &tail[body_at..];
        let open = rest[rest.find(':').unwrap() + 1..].trim_start();
        let quoted = open.strip_prefix('"').expect("fn_body must be a string");
        let end = quoted.find('"').expect("unterminated fn_body");
        let body = &quoted[..end];

        assert!(
            body.contains(wiring_fn),
            "{widget}.dom is bound to `{body}`, which bypasses {wiring_fn}. That \
             function exists precisely because the worker cannot live in \
             azul-layout, and binding past it ships a widget that renders but \
             never receives data — silently, with no error on any platform."
        );
    }
}

fn alloc_fmt(widget: &str) -> String {
    let mut s = String::with_capacity(widget.len() + 2);
    s.push('"');
    s.push_str(widget);
    s.push('"');
    s
}

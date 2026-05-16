//! Hydration payload: single postcard envelope containing everything
//! the wasm client needs to reconstruct the App at bootstrap.
//!
//! Per `scripts/M8_7_HYDRATION_PLAN_2026_05_16.md` addendum 2 (user
//! direction): server serializes the entire `HeadlessApp` via
//! postcard, embeds the bytes as base64 in
//! `<script id="az-state" type="application/octet-stream">`, the
//! wasm `AzStartup_init` postcard-deserializes to reconstruct.
//!
//! Wrapper-type approach: rather than adding serde derives to every
//! upstream struct (AppConfig, FullWindowState, FcFontCache,
//! CompactDom, NodeData, ...) which would be a multi-week change
//! touching azul-core/azul-layout/rust-fontconfig, this module
//! defines *narrow projection* types with only the fields the wasm
//! client actually needs + serde derives. `From`/`Into` impls
//! bridge between the upstream types and these wrappers.
//!
//! Trade-off: any field we forget at this stage becomes inaccessible
//! to the wasm-side dispatch. M8.7d-onward will expand the wrappers
//! as new dispatch features need new fields.

use serde::{Deserialize, Serialize};

/// Top-level payload. Versioned so server + client agree on the shape.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydrationPayload {
    /// Format version. Bump when the shape changes incompatibly.
    pub version: u32,
    /// Window state subset the wasm side needs.
    pub window: HydratedWindow,
    /// Root layout callback (resolved by dladdr on the server).
    pub layout_cb: HydratedCallbackRef,
    /// User's RefAny serialized to JSON via the registered
    /// `<Type>_toJson`. Embedded as a raw string so the wasm side
    /// can hand it back to `<Type>_fromJson` (via the cb table)
    /// without parsing on the wasm side.
    pub refany_json: String,
    /// CompactDom-like projection: nodes + parent/child links + per-node
    /// callback bindings + hit-test bboxes.
    pub dom: HydratedDom,
}

pub const HYDRATION_PAYLOAD_VERSION: u32 = 1;

/// Window state subset used by wasm-side dispatch (hit-test, focus,
/// etc.).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydratedWindow {
    pub width: f32,
    pub height: f32,
    pub dpi: f32,
    pub theme: u8, // 0 = light, 1 = dark
}

/// Callback identity: dladdr-resolved symbol name (= the URL
/// component for the lifted wasm) + content hash + raw fn-addr
/// (= the key for `__az_resolve_callback`).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydratedCallbackRef {
    pub symbol_name: String,
    pub content_hash: String,
    pub fn_addr: u64,
}

/// DOM tree projection. Flat node array indexed by `az_id` (the
/// synthetic `az_N` ID that html_render emits). Parent/child links
/// + hit-test bboxes (computed at server-render time + cached so
/// the wasm side doesn't need to re-layout for the initial paint).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydratedDom {
    pub nodes: Vec<HydratedNode>,
    pub root_az_id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydratedNode {
    pub az_id: u32,
    /// Tag name from `node_type` ("div", "button", "__text__", ...).
    pub tag: String,
    /// Inline text content (for `__text__` nodes only).
    pub text: Option<String>,
    pub parent: Option<u32>,
    pub first_child: Option<u32>,
    pub last_child: Option<u32>,
    pub prev_sibling: Option<u32>,
    pub next_sibling: Option<u32>,
    /// (event_kind, cb_fn_addr) bindings — empty for non-interactive
    /// nodes. Multiple entries when the node has e.g. both
    /// `on_click` and `on_hover`.
    pub callbacks: Vec<HydratedNodeCallback>,
    /// Hit-test bounding box (CSS pixels, relative to viewport).
    /// Server fills these from the rendered StyledDom's layout
    /// output. `(0,0,0,0)` if the node wasn't laid out (off-screen,
    /// display:none, etc.).
    pub bbox: HydratedBbox,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HydratedNodeCallback {
    /// Event-kind discriminator (matches `event_kind::*` constants
    /// in `eventloop.rs`).
    pub event_kind: u32,
    /// Native fn-addr — the key the wasm side uses for
    /// `__az_resolve_callback` to find the per-callback wasm in the
    /// JS-owned table.
    pub fn_addr: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Copy)]
pub struct HydratedBbox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl HydratedBbox {
    /// Standard rectangle hit-test.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w
            && py >= self.y && py < self.y + self.h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> HydrationPayload {
        HydrationPayload {
            version: HYDRATION_PAYLOAD_VERSION,
            window: HydratedWindow { width: 400.0, height: 300.0, dpi: 1.0, theme: 0 },
            layout_cb: HydratedCallbackRef {
                symbol_name: "layout".to_string(),
                content_hash: "9c4f784aa5ce135f".to_string(),
                fn_addr: 0x1234_5678_9abc_def0,
            },
            refany_json: "5".to_string(),
            dom: HydratedDom {
                nodes: vec![
                    HydratedNode {
                        az_id: 0,
                        tag: "body".to_string(),
                        text: None,
                        parent: None,
                        first_child: Some(1),
                        last_child: Some(3),
                        prev_sibling: None,
                        next_sibling: None,
                        callbacks: vec![],
                        bbox: HydratedBbox { x: 0.0, y: 0.0, w: 400.0, h: 300.0 },
                    },
                    HydratedNode {
                        az_id: 3,
                        tag: "button".to_string(),
                        text: Some("Increase counter".to_string()),
                        parent: Some(0),
                        first_child: None,
                        last_child: None,
                        prev_sibling: Some(1),
                        next_sibling: None,
                        callbacks: vec![HydratedNodeCallback {
                            event_kind: 0, // CLICK
                            fn_addr: 0xCAFE_BABE_DEAD_BEEF,
                        }],
                        bbox: HydratedBbox { x: 10.0, y: 50.0, w: 200.0, h: 40.0 },
                    },
                ],
                root_az_id: 0,
            },
        }
    }

    #[test]
    fn payload_roundtrips_via_postcard() {
        let original = sample_payload();
        let bytes = postcard::to_stdvec(&original).expect("serialize");
        let decoded: HydrationPayload =
            postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.version, original.version);
        assert_eq!(decoded.window.width, 400.0);
        assert_eq!(decoded.refany_json, "5");
        assert_eq!(decoded.dom.nodes.len(), 2);
        assert_eq!(decoded.dom.nodes[1].callbacks[0].fn_addr, 0xCAFE_BABE_DEAD_BEEF);
    }

    #[test]
    fn payload_size_for_hello_world_is_compact() {
        let bytes = postcard::to_stdvec(&sample_payload()).expect("serialize");
        // Hello-world's 2-node payload should fit in well under 1 KiB.
        // (Sanity check that postcard isn't accidentally bloating.)
        assert!(bytes.len() < 1024,
                "hello-world payload {} bytes (expected < 1024)", bytes.len());
    }

    #[test]
    fn bbox_hit_test() {
        let b = HydratedBbox { x: 10.0, y: 50.0, w: 200.0, h: 40.0 };
        assert!(b.contains(15.0, 60.0));
        assert!(!b.contains(5.0, 60.0));   // left of x
        assert!(!b.contains(15.0, 45.0));  // above y
        assert!(!b.contains(211.0, 60.0)); // right of x+w
        assert!(!b.contains(15.0, 91.0));  // below y+h
    }
}

//! A permission-bearing node subscribes its capability under ITS OWN node id
//! (TEXT7, wave 7).
//!
//! The layout pass (`common::layout::regenerate_layout`, step 7) paired every
//! `GeolocationProbe` with `NodeId::from_usize(i)` of its `enumerate` index -
//! the 1-based FFI DECODER (0 = no node, n = node n - 1) - so the
//! subscription named the node BEFORE the probe, and no node at all for a
//! probe at index 0: a `PermissionChanged` aimed at the probe landed on its
//! previous sibling. MAILENG6 fixed the same off-by-one in the font
//! collection (`getters::collect_font_stacks_from_styled_dom`).
//!
//! Linux only: a probe also starts the OS location service for the PROCESS
//! (step 7b) - CoreLocation on macOS, the COM Location API on Windows. On
//! Linux it is a GeoClue thread that returns at once where the service is
//! missing.
#![cfg(target_os = "linux")]

use super::*;

/// `body(0) > [div(1), GeolocationProbe(2)]`.
extern "C" fn probe_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
        .with_child(Dom::create_div())
        .with_child(Dom::create_geolocation_probe(
            azul_core::geolocation::GeolocationProbeConfig::default(),
        ))
}

#[test]
fn a_geolocation_probe_subscribes_under_its_own_node_id() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, probe_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();

    let lw = window.common.layout_window.as_ref().expect("layout window");
    let (dom, index) = lw
        .layout_results
        .iter()
        .find_map(|(dom_id, result)| {
            result
                .styled_dom
                .node_data
                .as_ref()
                .iter()
                .position(|nd| {
                    matches!(
                        nd.get_node_type(),
                        azul_core::dom::NodeType::GeolocationProbe(_)
                    )
                })
                .map(|index| (*dom_id, index))
        })
        .expect("premise: the probe is laid out");
    let entry = lw
        .permission_manager
        .statuses
        .get(&azul_layout::managers::permission::Capability::Geolocation)
        .expect("the probe subscribed geolocation");
    assert_eq!(entry.refcount, 1, "one probe, one subscription");
    assert_eq!(
        entry.last_subscriber,
        Some(azul_core::dom::DomNodeId {
            dom,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                azul_core::dom::NodeId::new(index)
            )),
        }),
        "the subscription names the probe (node {index}), not the node before it"
    );
}

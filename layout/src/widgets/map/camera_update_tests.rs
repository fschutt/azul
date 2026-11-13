//! An app moves the map's camera and swaps its overlays from any callback,
//! in place: the map's own view is drawn again and the tiles the new camera
//! needs are queued, but the app's DOM is not rebuilt. This is what lets a
//! navigation follow the car every second without a window rebuild.

use azul_core::{
    dom::{DomId, DomNodeId, Dom},
    geom::OptionLogicalPosition,
    refany::RefAny,
    styled_dom::NodeHierarchyItemId,
};

use super::{
    autotest_generated::{layer_zoom, view, with_callback_info_at},
    move_camera, replace_overlays, MapLatLon, MapMarker, MapOverlay, MapOverlayVec, MapTileCache,
    MapTileId, MapViewport, MapWidget,
};
use crate::callbacks::CallbackChange;

/// A map built at Vienna, zoom 10, whose view was last drawn 800 x 600 px.
fn drawn_map() -> RefAny {
    let mut dom = MapWidget::create(layer_zoom(0, 19))
        .with_viewport(view(48.21, 16.37, 10.0))
        .dom();
    let mut data = dom.root.get_dataset_mut().expect("the map's dataset").clone();
    if let Some(mut cache) = data.downcast_mut::<MapTileCache>() {
        cache.view_size = (800.0, 600.0);
    }
    data
}

fn redraws_the_view(changes: &[CallbackChange]) -> bool {
    changes.iter().any(|c| {
        matches!(
            c,
            CallbackChange::UpdateVirtualView { .. } | CallbackChange::UpdateAllVirtualViews
        )
    })
}

#[test]
fn a_camera_update_moves_the_map_and_redraws_only_its_view() {
    let mut data = drawn_map();
    let berlin = MapViewport {
        centre_lat_deg: 52.52,
        centre_lon_deg: 13.40,
        zoom: 15.0,
        bearing_deg: 30.0,
        pitch_deg: 20.0,
    };
    let (moved, changes) = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        move_camera(&mut data, &mut info, berlin)
    });
    assert!(moved, "the dataset is a map's: the camera moves");
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    assert_eq!(cache.viewport, berlin, "the map shows the new camera");
    assert!(redraws_the_view(&changes), "the map's view is drawn again in place");
}

#[test]
fn a_camera_update_queues_the_tiles_the_new_camera_needs() {
    let mut data = drawn_map();
    let berlin = view(52.52, 13.40, 15.0);
    let _ = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        move_camera(&mut data, &mut info, berlin)
    });
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    let wanted = cache.wanted.expect("the new camera's tiles are wanted at once");
    // The tile under Berlin's centre at zoom 15.
    let centre = MapTileId {
        z: 15,
        x: 17_603,
        y: 10_746,
    };
    assert!(wanted.contains(centre), "{wanted:?} holds the tile under the new centre");
}

#[test]
fn a_camera_update_keeps_the_camera_inside_what_the_map_can_show() {
    let mut data = drawn_map();
    let wild = MapViewport {
        centre_lat_deg: 52.52,
        centre_lon_deg: 13.40,
        zoom: 40.0,
        bearing_deg: 450.0,
        pitch_deg: 89.0,
    };
    let _ = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        move_camera(&mut data, &mut info, wild)
    });
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    assert_eq!(cache.viewport.zoom, 19.0, "the layer's deepest zoom");
    assert_eq!(cache.viewport.pitch_deg, super::MAX_PITCH_DEG, "the steepest tilt");
    assert_eq!(cache.viewport.bearing_deg, 90.0, "a turn is normalised");
}

#[test]
fn a_camera_update_of_something_that_is_not_a_map_changes_nothing() {
    let mut data = RefAny::new(5_u32);
    let (moved, changes) = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        move_camera(&mut data, &mut info, view(1.0, 2.0, 3.0))
    });
    assert!(!moved);
    assert!(changes.is_empty(), "nothing is drawn: {changes:?}");
}

#[test]
fn a_camera_update_of_a_node_the_window_does_not_have_says_so() {
    let nowhere = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
    let (moved, changes) = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        MapWidget::update_camera(&mut info, nowhere, view(1.0, 2.0, 3.0))
    });
    assert!(!moved);
    assert!(changes.is_empty());
}

#[test]
fn an_overlay_update_moves_a_marker_and_redraws_only_the_maps_view() {
    let mut data = drawn_map();
    let car = |lat: f64| {
        MapOverlay::Marker(MapMarker::create(
            7,
            MapLatLon {
                lat_deg: lat,
                lon_deg: 16.37,
            },
            Dom::create_div(),
        ))
    };
    let _ = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        replace_overlays(&mut data, &mut info, MapOverlayVec::from_vec(vec![car(48.21)]))
    });
    let (replaced, changes) = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        replace_overlays(&mut data, &mut info, MapOverlayVec::from_vec(vec![car(48.22)]))
    });
    assert!(replaced);
    let cache = data.downcast_ref::<MapTileCache>().expect("cache");
    match cache.overlays.as_slice() {
        [MapOverlay::Marker(m)] => assert_eq!(m.position.lat_deg, 48.22, "the marker moved"),
        other => panic!("one marker expected, got {other:?}"),
    }
    assert!(redraws_the_view(&changes), "the map's view is drawn again in place");
}

#[test]
fn an_overlay_update_of_a_node_the_window_does_not_have_says_so() {
    let nowhere = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::NONE,
    };
    let (replaced, _) = with_callback_info_at(OptionLogicalPosition::None, |mut info| {
        MapWidget::update_overlays(&mut info, nowhere, MapOverlayVec::from_const_slice(&[]))
    });
    assert!(!replaced);
}

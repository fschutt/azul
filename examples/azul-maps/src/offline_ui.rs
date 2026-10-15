//! Settings > Navigation > Offline areas: the area shown on the map kept for routes without
//! the network.

use azul::{prelude::*, widgets::MapViewport};
use azul_routing::offline::Area;

/// The rectangle the map shows in a view of `size` px at `viewport`.
pub(crate) fn shown_area(_viewport: MapViewport, _size: (f32, f32)) -> Area {
    Area { south: 0.0, west: 0.0, north: 0.0, east: 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vienna(zoom: f32, bearing_deg: f32) -> MapViewport {
        MapViewport { centre_lat_deg: 48.2082, centre_lon_deg: 16.3738, zoom, bearing_deg, pitch_deg: 0.0 }
    }

    #[test]
    fn the_area_shown_is_the_rectangle_around_the_maps_corners() {
        let area = shown_area(vienna(12.0, 0.0), (1100.0, 720.0));
        assert!(area.south < 48.2082 && 48.2082 < area.north, "{area:?} holds the centre");
        assert!(area.west < 16.3738 && 16.3738 < area.east, "{area:?} holds the centre");
        // 1100 px at zoom 12 are 1100 / (256 * 4096) of the world's 360 degrees.
        let wide = 1100.0 / (256.0 * 4096.0) * 360.0;
        assert!(((area.east - area.west) - wide).abs() < wide * 0.01, "{area:?} is {wide} degrees wide");
    }

    #[test]
    fn a_turned_map_shows_more_than_its_unturned_rectangle() {
        let flat = shown_area(vienna(12.0, 0.0), (1100.0, 720.0));
        let turned = shown_area(vienna(12.0, 45.0), (1100.0, 720.0));
        assert!(turned.north - turned.south > flat.north - flat.south, "{turned:?} against {flat:?}");
    }
}

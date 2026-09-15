---
slug: widgets/maps
title: Maps
language: en
canonical_slug: widgets/maps
audience: external
maturity: beta
guide_order: 113
topic_only: false
short_desc: A slippy map widget with pan/zoom, tap-to-pin hooks, and lat/lon projection
prerequisites: [widgets, events/callbacks]
tracked_files:
  - layout/src/widgets/map.rs
  - examples/azul-maps/src/main.rs
last_generated_rev: 47525d4250000000000000000000000000000000
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - MapWidget
  - MapViewport
  - MapLatLon
  - MapTileLayer
  - latlon_at_px
  - px_at_latlon
  - on_pin_tap
  - on_viewport_changed
  - on_mount
  - MapSetup
---

# Maps

## Introduction

`MapWidget` is a slippy-map widget: a Web-Mercator tile grid you can pan and
zoom, with hooks for tap-to-pin and viewport changes, and projection helpers to
place your own markers. It is the AzulMaps (P3) goal app. Like the other
widgets it owns its state in a `RefAny` dataset that survives relayout via a
merge callback - you just `create` it, configure it, and call `.dom()`.

## Creating a map

```rust
use azul::widgets::{MapWidget, MapViewport, MapTileLayer};

let map = MapWidget::create(MapTileLayer::default())
    .with_viewport(MapViewport {
        centre_lat_deg: 37.7749,   // San Francisco
        centre_lon_deg: -122.4194,
        zoom: 11.0,
        bearing_deg: 0.0,
        pitch_deg: 0.0,
    })
    .dom();
```

`MapTileLayer` configures the tile source (and attribution). The widget computes
the visible XYZ tile grid from the viewport and renders one GPU-translated tile
div per tile. **Pan (drag) and pinch-zoom are handled by the widget itself** -
no wiring needed.

## Hooks: observing taps and the viewport

Two backreference-DI hooks (see [architecture](../architecture.md)) let your app
react without globals - the callback receives the data and writes back to *your*
state:

- `with_on_pin_tap(data, cb)` - fires when the user taps the map (a press +
  release with no drag), with the tapped [`MapLatLon`].
- `with_on_viewport_changed(data, cb)` - fires on pan/zoom, with the new
  [`MapViewport`] (persist it, sync a minimap, etc.).

```rust
use azul::dom::MapPinTapCallback;
use azul::option::OptionRefAny;

let map = MapWidget::create(layer)
    .with_viewport(viewport)
    .with_on_pin_tap(
        state.clone(),
        MapPinTapCallback { cb: on_pin_tap, callable: OptionRefAny::None },
    )
    .dom();

extern "C" fn on_pin_tap(mut data: RefAny, _info: CallbackInfo, coord: MapLatLon) -> Update {
    if let Some(mut s) = data.downcast_mut::<MyState>() {
        s.pins.push((coord.lat_deg, coord.lon_deg));   // drop a pin
    }
    Update::RefreshDom
}
```

The widget does the tap-vs-drag detection and the projection for you, so you
just store the lat/lon.

## Projection: placing your own markers

To draw markers/overlays on top of the map, convert between lat/lon and screen
pixels with the two static helpers (small-angle Web Mercator):

```rust
// lat/lon -> screen position (place a pin div here):
let p = MapWidget::px_at_latlon(viewport, MapLatLon { lat_deg, lon_deg }, container_size);

// screen position -> lat/lon (inverse):
let coord = MapWidget::latlon_at_px(viewport, px, container_size);
```

`container_size` is the map node's pixel size (e.g. from
`CallbackInfo::get_hit_node_rect().size`). Re-projecting each layout makes your
pins track the map as it pans and zooms. `examples/azul-maps` uses exactly this
to render dropped pins + a coordinate readout.

## Sharing connections and threads

By default each tile opens its own connection and downloads on its own thread.
That is fine for a map shown now and then. An app that keeps a map open, or
shows several, can give the maps a shared `HttpClient` (open connections to the
tile server are reused instead of a TLS handshake per tile) and a shared
`ThreadPool` (a fixed number of download threads).

That is configuration, so it does not belong in `layout()`. Create the pools
once with your app state, and hand them to the map in its `on_mount` hook. The
hook runs when the map enters the tree, receives the map's current `MapSetup`
and returns the one to use:

```rust
use azul::dom::MapMountCallback;
use azul::http::{HttpClient, HttpClientConfig};
use azul::task::ThreadPool;
use azul::widgets::MapSetup;

struct MyState {
    viewport: MapViewport,
    tiles: HttpClient,     // HttpClient::create(HttpClientConfig::create())
    workers: ThreadPool,   // ThreadPool::create(4)
}

// in layout(): UI only
let map = MapWidget::create(layer)
    .with_viewport(viewport)
    .with_on_mount(
        state.clone(),
        MapMountCallback { cb: on_map_mount, callable: OptionRefAny::None },
    )
    .dom();

extern "C" fn on_map_mount(mut data: RefAny, _info: CallbackInfo, setup: MapSetup) -> MapSetup {
    let Some(s) = data.downcast_ref::<MyState>() else { return setup };
    setup
        .with_http_client(s.tiles.clone())
        .with_thread_pool(s.workers.clone())
        .with_max_in_flight(8)   // at most 8 tiles downloading at once
}
```

The map keeps its setup through every relayout (the merge callback carries it)
and drops its clones when it leaves the tree. If the map is mounted again, the
hook runs again with the setup it returned last time. `examples/c/async.c`
shows the same flow in C.

## Geolocation

To show "you are here", drop a `Dom::create_geolocation_probe(...)` into the map
subtree and read the fix with `CallbackInfo::get_location_fix` - see
[Device Input](../system/device-input.md#geolocation).

## Notes

- Tile fetch + MVT decode + MapCSS-to-SVG are behind the `map-tiles` Cargo
  feature; with it off, the grid + viewport bookkeeping still work (the demo
  shows the projection math without live tiles).
- The tile cache lives in the widget's dataset `RefAny` and is carried across
  relayout by a `DatasetMergeCallback`, so panning doesn't refetch everything.

## See also

- [widgets](..md) - the widget model.
- [callbacks](../events/callbacks.md) - the hook + `RefAny` mechanism.
- [Device Input](../system/device-input.md) - geolocation for the "you are here" dot.

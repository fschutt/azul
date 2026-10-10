# Routing in AzMaps (design note, nothing here is built yet)

Two constraints shape it:

- **Plain file hosting.** The routing data is static files (GitHub, or any object store / CDN).
  There is no routing server, ours or anyone else's.
- **The route is computed on the client.** The hosted files are a coarse pre-pass. AzMaps works
  out the actual route itself, from the tiles near the route.

What exists today: the travel panel has a start (empty means "where you are" once the location is
known), a destination (`lat, lon`, a dropped pin's Directions button fills it in) and a mode (car,
walk, bike, transit). The map shows the straight line between start and destination, and the panel
shows its great-circle distance. `AZMAPS_TRAVEL <mode> <from> <to>` goes to stdout on every change.

The plan has five steps. The rest of this note works through them in order.

## 1. A straight-line estimate

This is the line the map already draws: start and destination as `(lat, lon)`, the great-circle
distance (`model::distance_km`), and a first time estimate from it. Distance × a detour factor
(about 1.3 by road), divided by a typical speed for the mode, gives the estimate. The panel shows it
straight away as "~ 5 h". Step 2 needs this line too: it decides which tiles to fetch.

## 2. Download the routing tiles along that line

### Which tiles: routable tiles vs. Valhalla tiles, with static hosting only

| | Routable Tiles (openplannerteam) | Valhalla graph tiles |
|---|---|---|
| Format | JSON-LD per tile: the OSM nodes (lat / lon) and the routing-relevant ways (`highway=*`, `route=*`, turn-restriction relations) with their tags. An edge is in every tile it has a vertex in. | Valhalla's binary `.gph` tile: nodes, directed edges, edge info, restrictions, admin data. These are C++ structs, bit-packed, and the layout changes between Valhalla versions. |
| Tiling | One XYZ zoom, **z14** (about 2.4 km × cos(lat) per side; about 1.6 km at 50° N). | Three levels on a lat/lon grid: **level 0** = 4° tiles (motorway, trunk, primary), **level 1** = 1° (secondary, tertiary), **level 2** = 0.25° (everything else). Paths look like `2/000/756/425.gph`. |
| Static hosting? | Yes. The tiles are plain files. | Yes for the files. Valhalla's own reader loads missing tiles from `tile_url` (`http://host/path/{tilePath}`, optionally gzipped) or from a tar extract. That is a client-side reader over static files, so no server is needed - **if the client is Valhalla**. |
| What the client implements | A JSON parser, building a graph from nodes and ways (oneway, access, maxspeed), snapping to the nearest way, costing per mode, A*. Small, all pure Rust. | Either embed Valhalla's C++ library (a C++ toolchain and a large dependency in every target, mobile included, next to the 27 bindings), or re-implement the `.gph` reader in Rust against one pinned Valhalla version. The hierarchy and the costing come with it. |
| Coarse pre-pass | None built in (z14 only). The README describes "along tiles" and customizable contraction hierarchies as extensions. | Built in: levels 0 and 1 are the coarse network. |
| Tile sizes | Small, and only roads. | A level-2 tile in a city can be several MB. |
| Licence | Data: ODbL (OSM), with attribution. Generator: C# / .NET (`routable-tiles`, a CLI that filters with osmosis and writes a tiled database). Check the code licence before reusing the generator. | Valhalla: MIT. Data: ODbL (OSM). |

**Recommendation: the routable-tiles model, generated and hosted by us, plus our own coarse levels
in the same format.** It is the only option that keeps the client small, pure Rust and stable
across upstream releases. Valhalla's tiles are the better graph, but using them client-side means
shipping Valhalla (C++) or chasing its binary format. Valhalla's three-level hierarchy is still the
right idea, so we copy it:

- **z14**: every routable way (what routable tiles are).
- **z11**: secondary and up, plus the links between them.
- **z8**: motorway, trunk, primary.

A node where a higher class meets a lower one appears in both levels with the same OSM id. That is
the "level change" A* uses.

Generation runs in CI from a Geofabrik extract (or the planet), with a small Rust tool in `scripts/`.
It filters `highway=*` and restrictions, cuts the result into tiles and writes gzipped JSON in the
routable-tiles shape. A binary encoding can come later behind the same tile interface. Planet-build
date versioning works like the MVT URL (`.../routing/<build>/14/x/y.json.gz`).

**Hosting on GitHub, honestly:** GitHub Pages publishes at most about 1 GB per site, and a repository
should stay well under a few GB. These are estimates to verify. Germany alone at z14 is about 150k
tiles, which is probably a few GB gzipped. So:

- the coarse levels (z8, z11) for a region fit on GitHub;
- z14 for more than a city does not fit as loose files.

The way out that keeps "plain file hosting" is ONE archive file per level and region (PMTiles-style:
a directory, then the tiles, read with HTTP `Range` requests). It goes on a GitHub release asset (2
GB per file; check that its CDN serves `Range`) or in any S3-compatible bucket (the Hetzner storage
the Azlin apps already plan for). Millions of loose files are the thing to avoid, not GitHub as
such.

### Finding the tiles for start and destination

Work in tile space at each level (`x = (lon + 180) / 360 · 2^z`, the Mercator `y`, the same
functions as `layout/src/widgets/map.rs`):

1. **The ends:** the z14 tile of the start and of the destination, plus their 8 neighbours. The
   neighbours matter because the nearest road can be across a tile edge.
2. **The corridor:** walk the straight line through the tile grid (a supercover DDA, every tile the
   line touches) and grow it by `b` tiles on each side.
   - Short trips (< ~15 km) use z14 along the whole line.
   - Medium trips use z11 in the middle and z14 near the ends.
   - Long trips use z8 in the middle, z11 for ~30 km around each end, and z14 for ~3 km.

   This keeps a Berlin to Munich query (~500 km) at a few hundred small tiles instead of thousands.
3. Fetch the missing ones in parallel on the map's `ThreadPool` and `HttpClient` (`MapSetup`, the
   same pool the vector tiles use), nearest to the ends first.

## 3. Find the route with A* on those tiles

Build one graph from the fetched tiles:

- **Nodes:** OSM node ids.
- **Edges:** consecutive way nodes. Respect `oneway` for cars and bikes, `access=*` per mode, and
  turn restrictions when the relations are present.
- **Cost:** travel time in integer milliseconds. Length / speed, where speed comes from `maxspeed`
  or a per-mode default per `highway` class (walking ignores oneway; bikes avoid motorway / trunk).
- **Snapping:** start and destination snap to the nearest point on a usable way in their tile.
  That point becomes a virtual node splitting that edge.

**Crate: `pathfinding`** (MIT / Apache-2.0, pure Rust). Its
`astar(start, successors, heuristic, success) -> Option<(Vec<N>, C)>` takes the neighbours as a
closure. That closure reads our tile graph (a `HashMap<NodeId, Vec<(NodeId, u32)>>`) directly, so
there is no second graph structure to build. Costs only need `Zero + Ord + Copy`, which integer
milliseconds are. The heuristic is great-circle distance / the mode's top speed, which is
admissible.

Alternatives considered:

- `petgraph::algo::astar`: fine, but it needs the graph copied into a petgraph `Graph` first.
- `fast_paths` (contraction hierarchies): fast for many queries on one fixed graph, but its
  preprocessing per corridor costs more than one A* query saves. It becomes interesting if we
  preprocess the coarse levels in CI.

## 4. No route? Widen the corridor and try again

A* fails when the corridor has no connected path: a river with the bridge outside the buffer, a
detour around a lake. Then:

- widen the buffer (`b` = 1, then 3, then 8 tiles) and widen the fine-zoom zones around the ends;
- fetch only the new tiles (the graph grows, nothing is refetched);
- run A* again.

After the last widening the panel says "No route found". If start and destination have no road at
all in their own tiles, it says that instead. Every fetched tile stays cached:

- in memory, for the session;
- in the data tree (`maps/routing/<build>/<z>/<x>/<y>.json.gz`, size-capped, LRU), so a second
  route through the same area costs no network.

## 5. Refinements: public transport (bahn.de and friends)

Once the road route works, the transit mode (and "alternatives" for the other modes) asks timetable
providers. These are online APIs, not tiles. Only the two places and the time are sent: no account,
no user id, no personal data. The User-Agent names the app and its version, nothing else.

- **Transitous** (community-run, MOTIS-based, Europe-wide GTFS, no key). A good default for
  door-to-door transit journeys. Check its endpoint (`/api/v1/plan?fromPlace=lat,lon&toPlace=…`)
  and its usage policy.
- **Deutsche Bahn API Marketplace** (`developers.deutschebahn.com`). Free plans with a client id and
  API key (`DB-Client-Id` / `DB-Api-Key` headers): **Timetables** (departures / arrivals and
  changes per station) and **StaDa / RIS::Stations** (stations and their positions). Journey search
  itself is not an open product there. Check what the key we get includes. The unofficial bahn.de
  (HAFAS / vendo) endpoints behind community clients like `db-rest` are not an option for a shipped
  app.
- **Others later:** Entur (Norway, open GraphQL, needs an `ET-Client-Name` header), OpenTripPlanner
  instances, Navitia (key). For flights and long-distance buses there is no open schedule API
  (Amadeus has a keyed self-service tier). These stay links out for now.

**The key, baked in at build time:**

```rust
/// The DB API Marketplace credentials of this build (CI sets them from secrets);
/// `None` in a local or forked build, where the DB provider is hidden.
const DB_CLIENT_ID: Option<&str> = option_env!("AZMAPS_DB_CLIENT_ID");
const DB_API_KEY: Option<&str> = option_env!("AZMAPS_DB_API_KEY");
```

- **CI:** only the release workflow sets them, from repository secrets:
  `env: AZMAPS_DB_API_KEY: ${{ secrets.AZMAPS_DB_API_KEY }}` on the build step. Pull-request builds
  (forks get no secrets) compile with `None`. rustc records `option_env!` reads as build inputs,
  so cargo rebuilds when the value changes.
- **Override at run time:** a key in `settings.json` (`db_api_key`), or the environment variable of
  the same name at run time, wins over the baked one.
- **A key in a binary is public:** anyone can read it with `strings`. Use a key whose plan is free
  and rate-limited, and keep a remote kill switch: a tiny static JSON next to the routing tiles
  that lists the providers to use. Never use a key that bills.

## 6. How the travel panel calls it

```text
travel panel change  ->  RouteRequest { from, to, mode, depart_at }
                     ->  a Thread (the map's ThreadPool), cancelled by the next change
                           1. corridor tiles (step 2), fetched / cached
                           2. graph + A* (step 3), widen and retry (step 4)
                           3. transit providers for Transit / alternatives (step 5)
                     <-  write-back RouteResult { polyline, distance_m, duration_s, steps,
                                                   transit: Vec<Journey> }
app state            ->  the panel shows the time / distance / alternatives, the map draws the
                         polyline, stdout: AZMAPS_ROUTE <mode> <km> <minutes> (for the E2E)
```

The request goes out:

- when both ends become places, or the mode changes;
- after a short debounce (300 ms) while a field is being typed in.

The straight line stays on the map until the result arrives. Drawing: a short route can use the
same overlay the straight line uses now (one rotated div per segment, clipped to the view). A real
route has hundreds of segments, so it wants a polyline layer inside `MapWidget` (see below).

## Engine notes (for the central session)

- **A route / overlay layer in `MapWidget`.** Something like
  `MapWidget::with_overlay(Vec<MapPolyline>)`: projected and drawn in the tile grid's own render, so
  it pans and zooms with the tiles without an app DOM rebuild per frame. Today the app rebuilds its
  whole DOM on every viewport change, and pins and lines are placed with `px_at_latlon`, which
  ignores bearing and pitch.
- **A geocoder** (place names into the fields) needs either a hosted search index (static too:
  per-prefix JSON shards of OSM names) or the labels the tiles already carry. Out of scope here.

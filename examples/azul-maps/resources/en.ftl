# AzMaps' words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## AzMaps

azmaps-about-summary = A map with pins on azul's MapWidget: OpenStreetMap vector tiles, your pins kept in your data folder.
azmaps-shortcuts-map = Map
azmaps-pan-west = Pan west
azmaps-pan-east = Pan east
azmaps-pan-north = Pan north
azmaps-pan-south = Pan south
azmaps-zoom-in = Zoom in
azmaps-zoom-out = Zoom out
azmaps-hide-sidebar = Hide sidebar
azmaps-show-sidebar = Show sidebar
azmaps-settings = Settings
azmaps-my-location = My Location
azmaps-from = From
azmaps-start = Start
azmaps-to = To
azmaps-destination = Destination
azmaps-swap = Swap start and destination
azmaps-directions = Directions
azmaps-recents = Recents
azmaps-clear-recents = Clear recents
azmaps-sidebar = Sidebar
azmaps-dropped-pin = Dropped Pin
azmaps-remove = Remove
azmaps-you-are-here = You are here
azmaps-heading = Heading { $direction } { $degrees }°
azmaps-direction-n = N
azmaps-direction-ne = NE
azmaps-direction-e = E
azmaps-direction-se = SE
azmaps-direction-s = S
azmaps-direction-sw = SW
azmaps-direction-w = W
azmaps-direction-nw = NW
azmaps-location-unavailable = Location unavailable
azmaps-show-location = Show my location
azmaps-pins-unreadable = The pins file could not be read: { $why }
azmaps-pins-unsaved = The pins could not be saved: { $why }
azmaps-not-pins-file = not a pins file: { $why }
azmaps-not-a-place = { $lat }, { $lon } is not a place on the map
azmaps-mode-drive = Drive
azmaps-mode-walk = Walk
azmaps-mode-cycle = Cycle
azmaps-mode-transit = Public transport
azmaps-duration-min = ~ { $min } min
azmaps-duration-h = ~ { $h } h
azmaps-duration-h-min = ~ { $h } h { $min } min
azmaps-route-searching = finding the road route ({ $done } of { $total } tiles)
azmaps-route-none = No route found
azmaps-route-no-road-start = No road near the start
azmaps-route-no-road-destination = No road near the destination
azmaps-route-tiles-failed = The routing tiles could not be loaded
azmaps-route-offline = The routing service cannot be reached
azmaps-route-busy = The routing service is busy, try again in a moment
azmaps-route-service-failed = The routing service gave no route
azmaps-route-asking = asking the routing service
azmaps-route-estimate = straight-line estimate
azmaps-route-privacy = Start and destination are sent to openstreetmap.de's routing service.
azmaps-route-steps = Turn-by-turn steps
azmaps-routing-credit = Routing: Valhalla, FOSSGIS / © OpenStreetMap contributors
azmaps-suggestions = Suggestions
azmaps-no-places = No place called “{ $query }” is known

## Public transport

azmaps-transit-now = Leave now
azmaps-transit-depart = Depart at
azmaps-transit-arrive = Arrive by
azmaps-span-min = { $min } min
azmaps-span-h = { $h } h
azmaps-span-h-min = { $h } h { $min } min
azmaps-transfers-none = direct
azmaps-transfers-one = 1 change
azmaps-transfers-many = { $count } changes
azmaps-leg-walk = Walk { $distance }
azmaps-leg-bike = Cycle { $distance }
azmaps-leg-car = Drive { $distance }

## Navigation

azmaps-voice-arrived = You have arrived.
azmaps-voice-in-m = In { $n } metres: { $instruction }
azmaps-voice-in-km = In { $n } kilometres: { $instruction }
azmaps-voice-rerouting = Recalculating the route.

## Position source

azmaps-gps-no-fix = No fix
azmaps-gps-fix = { $dimension }D fix
azmaps-gps-satellites = { $count } satellites
azmaps-gps-uncertainty = ±{ $metres } m

## Search sources

azmaps-source-places = AzMaps' list of cities
azmaps-source-transitous = Transitous · © OpenStreetMap contributors
azmaps-source-saved = Saved in { $list }
azmaps-transit-time = Time
azmaps-transit-off = No public transport service is set up (--transit off).
azmaps-transit-asking = asking Transitous for journeys …
azmaps-transit-offline = Transitous cannot be reached, and no journeys are kept for this question.
azmaps-transit-busy = Transitous is busy, try again in a moment.
azmaps-transit-none = No journey found.
azmaps-transit-failed = Transitous gave no journeys.
azmaps-transit-offline-shown = Offline: kept journeys, their times may have changed.
azmaps-transit-earlier = Earlier
azmaps-transit-later = Later
azmaps-transit-credit = Public transport: { $provider }
azmaps-transit-sources = Sources
azmaps-transit-platform = Pl. { $platform }
azmaps-transit-cancelled = cancelled
azmaps-offline = offline

## Navigation on screen

azmaps-nav-start = Start
azmaps-nav-end = End
azmaps-nav-banner = Next maneuver
azmaps-nav-arrived = You have arrived.
azmaps-nav-waiting = Waiting for the position …
azmaps-nav-destination = Your destination
azmaps-nav-rerouting = Finding a new route …
azmaps-nav-follow = Follow the route
azmaps-nav-summary = { $arrival } · { $left } · { $distance }
azmaps-nav-heading-up = Heading up
azmaps-nav-north-up = North up
azmaps-nav-voice = Voice prompts
azmaps-nav-car-mode = Car layout

## Settings: Location

azmaps-category-navigation = Navigation
azmaps-gps-title = Location
azmaps-gps-source = Position from
azmaps-gps-platform = This device
azmaps-gps-gpsd = gpsd
azmaps-gps-device = GPS receiver
azmaps-gps-replay = Replay file
azmaps-gps-gpsd-address = gpsd's address (host:port)
azmaps-gps-replay-file = An NMEA file
azmaps-gps-apply = Use
azmaps-gps-no-devices = No GPS receiver is plugged in (a USB or Bluetooth receiver shows up as a serial port).
azmaps-gps-waiting = Waiting for the receiver …
azmaps-gps-ended = The source ended.
azmaps-gps-failed = The source failed: { $why }
azmaps-gps-simulated = Simulated drive
azmaps-gps-about = A Mac has no GPS: its position comes from Wi-Fi (or a nearby iPhone), coarse or absent on a motorway. A USB or Bluetooth GPS receiver gives the car's position - directly as NMEA, or through gpsd. Phones use their own GPS.
azmaps-routing-tiles-title = Routing tiles
azmaps-routing-keep = Keep downloaded roads for
azmaps-routing-keep-suggested = As the tile set suggests
azmaps-routing-keep-days = { $days } days
azmaps-routing-keep-about = Road tiles downloaded for routes are kept on this computer and fetched again when they are older than this. Without a network the old ones still route. Applies from the next start.
azmaps-offline-title = Offline areas
azmaps-offline-about = Keeps the roads of the area shown on the map on this computer, so routes there work without a network. Frame the area on the map first. The map itself keeps the map tiles it has shown.
azmaps-offline-needs-tiles = Offline areas keep downloaded routing tiles: start AzMaps with routing tiles (--routing) to keep one.
azmaps-offline-name = Name of the area
azmaps-offline-download = Keep the area on the map
azmaps-offline-stop = Stop
azmaps-offline-delete = Delete
azmaps-offline-planning = Finding the roads of the area...
azmaps-offline-progress = { $done } of { $total } road tiles ({ $size })
azmaps-offline-kept = Kept { $name }: { $tiles } road tiles, { $size }
azmaps-offline-kept-failed = Kept { $name }: { $tiles } road tiles, { $size }. { $failed } could not be downloaded and are tried again on the next route.
azmaps-offline-failed = The area could not be kept: { $why }
azmaps-offline-empty = The routing tiles have no roads in the area on the map.
azmaps-offline-no-name = Name the area first.
azmaps-offline-stopped = The download was stopped; nothing was kept.
azmaps-offline-row = { $name } · { $tiles } road tiles · { $size }

## Saved places, imports and exports

azmaps-category-places = Places
azmaps-saved = Saved
azmaps-list-favourites = Favourites
azmaps-list-recents = Recent destinations
azmaps-clear-recent-destinations = Clear recent destinations and searches
azmaps-favourite = Favourite
azmaps-lists-unsaved = The saved places could not be written: { $why }
azmaps-places-title = Import and export
azmaps-import-file = Import a file …
azmaps-import-folder = Import a folder …
azmaps-import-formats = GPX (OsmAnd favourites too), KML or KMZ (Google My Maps, Organic Maps, MAPS.ME), GeoJSON, CSV with latitude and longitude, or the unpacked Google Takeout folder (Saved Places, Labeled places, the Saved lists).
azmaps-import-apple = Apple Maps cannot export its places: share a place from it and paste the link here.
azmaps-import-link = Paste a shared Apple Maps or Google Maps link
azmaps-import-link-add = Add
azmaps-import-busy = Importing …
azmaps-import-done = Imported { $places } places in { $lists } lists; { $review } to review.
azmaps-import-failed = The import failed: { $why }
azmaps-review-head = { $count } places came without coordinates: check what the search found.
azmaps-review-match = Found: { $place }
azmaps-review-none = Not found by the search.
azmaps-review-use = Use
azmaps-review-next = Other match
azmaps-review-skip = Skip
azmaps-export-gpx = Export as GPX
azmaps-export-geojson = Export as GeoJSON
azmaps-export-offered = Exported; a copy is kept as { $file }
azmaps-export-kept = Kept as { $file } in your data folder
azmaps-export-failed = The export could not be written: { $why }
azmaps-speak-destination = Speak the destination

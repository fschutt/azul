# AzMaps' words in German (src/l10n.rs): Apple Maps' and Google Maps' terms, "du".

## AzMaps

azmaps-about-summary = Eine Karte mit Pins auf azuls MapWidget: OpenStreetMap-Vektorkacheln, deine Pins in deinem Datenordner.
azmaps-shortcuts-map = Karte
azmaps-pan-west = Nach Westen schieben
azmaps-pan-east = Nach Osten schieben
azmaps-pan-north = Nach Norden schieben
azmaps-pan-south = Nach Süden schieben
azmaps-zoom-in = Vergrößern
azmaps-zoom-out = Verkleinern
azmaps-hide-sidebar = Seitenleiste ausblenden
azmaps-show-sidebar = Seitenleiste einblenden
azmaps-settings = Einstellungen
azmaps-my-location = Mein Standort
azmaps-from = Von
azmaps-start = Start
azmaps-to = Nach
azmaps-destination = Ziel
azmaps-swap = Start und Ziel tauschen
azmaps-directions = Route
azmaps-recents = Zuletzt verwendet
azmaps-clear-recents = Zuletzt verwendete löschen
azmaps-sidebar = Seitenleiste
azmaps-dropped-pin = Markierte Stelle
azmaps-remove = Entfernen
azmaps-you-are-here = Du bist hier
azmaps-heading = Richtung { $direction } { $degrees }°
azmaps-direction-n = N
azmaps-direction-ne = NO
azmaps-direction-e = O
azmaps-direction-se = SO
azmaps-direction-s = S
azmaps-direction-sw = SW
azmaps-direction-w = W
azmaps-direction-nw = NW
azmaps-location-unavailable = Standort nicht verfügbar
azmaps-show-location = Meinen Standort zeigen
azmaps-pins-unreadable = Die Pin-Datei konnte nicht gelesen werden: { $why }
azmaps-pins-unsaved = Die Pins konnten nicht gespeichert werden: { $why }
azmaps-not-pins-file = keine Pin-Datei: { $why }
azmaps-not-a-place = { $lat }, { $lon } ist kein Ort auf der Karte
azmaps-mode-drive = Auto
azmaps-mode-walk = Zu Fuß
azmaps-mode-cycle = Fahrrad
azmaps-mode-transit = Öffentliche Verkehrsmittel
azmaps-duration-min = ~ { $min } Min.
azmaps-duration-h = ~ { $h } Std.
azmaps-duration-h-min = ~ { $h } Std. { $min } Min.
azmaps-route-searching = Straßenroute wird gesucht ({ $done } von { $total } Kacheln)
azmaps-route-none = Keine Route gefunden
azmaps-route-no-road-start = Keine Straße nahe dem Start
azmaps-route-no-road-destination = Keine Straße nahe dem Ziel
azmaps-route-tiles-failed = Die Routing-Kacheln konnten nicht geladen werden
azmaps-route-offline = Der Routing-Dienst ist nicht erreichbar
azmaps-route-busy = Der Routing-Dienst ist ausgelastet, bitte gleich noch einmal versuchen
azmaps-route-service-failed = Der Routing-Dienst hat keine Route geliefert
azmaps-route-asking = der Routing-Dienst wird gefragt
azmaps-route-estimate = Luftlinien-Schätzung
azmaps-route-privacy = Start und Ziel werden an den Routing-Dienst von openstreetmap.de gesendet.
azmaps-route-steps = Abbiegehinweise
azmaps-routing-credit = Routing: Valhalla, FOSSGIS / © OpenStreetMap-Mitwirkende
azmaps-suggestions = Vorschläge
azmaps-no-places = Kein Ort namens „{ $query }“ bekannt

## Öffentlicher Verkehr

azmaps-transit-now = Jetzt losfahren
azmaps-transit-depart = Abfahrt um
azmaps-transit-arrive = Ankunft bis
azmaps-span-min = { $min } Min.
azmaps-span-h = { $h } Std.
azmaps-span-h-min = { $h } Std. { $min } Min.
azmaps-transfers-none = direkt
azmaps-transfers-one = 1 Umstieg
azmaps-transfers-many = { $count } Umstiege
azmaps-leg-walk = Fußweg { $distance }
azmaps-leg-bike = Rad { $distance }
azmaps-leg-car = Auto { $distance }

## Navigation

azmaps-voice-arrived = Sie haben Ihr Ziel erreicht.
azmaps-voice-in-m = In { $n } Metern: { $instruction }
azmaps-voice-in-km = In { $n } Kilometern: { $instruction }
azmaps-voice-rerouting = Die Route wird neu berechnet.

## Positionsquelle

azmaps-gps-no-fix = Keine Position
azmaps-gps-fix = { $dimension }D-Fix
azmaps-gps-satellites = { $count } Satelliten
azmaps-gps-uncertainty = ±{ $metres } m

## Suchquellen

azmaps-source-places = AzMaps' Städteliste
azmaps-source-transitous = Transitous · © OpenStreetMap-Mitwirkende
azmaps-source-saved = Gespeichert in { $list }
azmaps-transit-time = Uhrzeit
azmaps-transit-off = Kein Dienst für öffentlichen Verkehr eingerichtet (--transit off).
azmaps-transit-asking = Transitous wird nach Verbindungen gefragt …
azmaps-transit-offline = Transitous ist nicht erreichbar, und für diese Frage sind keine Verbindungen gespeichert.
azmaps-transit-busy = Transitous ist ausgelastet, bitte gleich noch einmal versuchen.
azmaps-transit-none = Keine Verbindung gefunden.
azmaps-transit-failed = Transitous hat keine Verbindungen geliefert.
azmaps-transit-offline-shown = Offline: gespeicherte Verbindungen, die Zeiten können sich geändert haben.
azmaps-transit-earlier = Früher
azmaps-transit-later = Später
azmaps-transit-credit = Öffentlicher Verkehr: { $provider }
azmaps-transit-sources = Quellen
azmaps-transit-platform = Gl. { $platform }
azmaps-transit-cancelled = fällt aus
azmaps-offline = offline

## Navigation auf dem Bildschirm

azmaps-nav-start = Starten
azmaps-nav-end = Beenden
azmaps-nav-banner = Nächstes Manöver
azmaps-nav-arrived = Sie haben Ihr Ziel erreicht.
azmaps-nav-waiting = Warte auf die Position …
azmaps-nav-destination = Ihr Ziel
azmaps-nav-rerouting = Neue Route wird gesucht …
azmaps-nav-follow = Der Route folgen
azmaps-nav-summary = { $arrival } · { $left } · { $distance }
azmaps-nav-heading-up = In Fahrtrichtung
azmaps-nav-north-up = Norden oben
azmaps-nav-voice = Sprachansagen
azmaps-nav-car-mode = Autoansicht

## Einstellungen: Ort

azmaps-category-navigation = Navigation
azmaps-gps-title = Ort
azmaps-gps-source = Position aus
azmaps-gps-platform = Diesem Gerät
azmaps-gps-gpsd = gpsd
azmaps-gps-device = GPS-Empfänger
azmaps-gps-replay = Aufzeichnung
azmaps-gps-gpsd-address = Adresse von gpsd (Host:Port)
azmaps-gps-replay-file = Eine NMEA-Datei
azmaps-gps-apply = Verwenden
azmaps-gps-no-devices = Kein GPS-Empfänger angeschlossen (ein USB- oder Bluetooth-Empfänger erscheint als serieller Anschluss).
azmaps-gps-waiting = Warte auf den Empfänger …
azmaps-gps-ended = Die Quelle ist zu Ende.
azmaps-gps-failed = Die Quelle ist ausgefallen: { $why }
azmaps-gps-simulated = Simulierte Fahrt
azmaps-gps-about = Ein Mac hat kein GPS: Seine Position kommt aus dem WLAN (oder von einem iPhone in der Nähe), ungenau oder auf der Autobahn gar nicht. Ein GPS-Empfänger per USB oder Bluetooth liefert die Position des Autos - direkt als NMEA oder über gpsd. Telefone nutzen ihr eigenes GPS.
azmaps-routing-tiles-title = Routing-Kacheln
azmaps-routing-keep = Geladene Straßen behalten für
azmaps-routing-keep-suggested = Wie der Kachelsatz es vorschlägt
azmaps-routing-keep-days = { $days } Tage
azmaps-routing-keep-about = Für Routen geladene Straßenkacheln bleiben auf diesem Computer und werden neu geladen, wenn sie älter sind. Ohne Netz routen die alten weiter. Gilt ab dem nächsten Start.
azmaps-offline-title = Offline-Gebiete
azmaps-offline-about = Behält die Straßen des Gebiets auf der Karte auf diesem Computer, damit Routen dort ohne Netz gehen. Rahme das Gebiet zuerst auf der Karte ein. Die Karte selbst behält die Kartenkacheln, die sie gezeigt hat.
azmaps-offline-needs-tiles = Offline-Gebiete behalten geladene Routing-Kacheln: Starte AzMaps mit Routing-Kacheln (--routing), um eines zu behalten.
azmaps-offline-name = Name des Gebiets
azmaps-offline-download = Gebiet auf der Karte behalten
azmaps-offline-stop = Anhalten
azmaps-offline-delete = Löschen
azmaps-offline-planning = Die Straßen des Gebiets werden gesucht ...
azmaps-offline-progress = { $done } von { $total } Straßenkacheln ({ $size })
azmaps-offline-kept = { $name } behalten: { $tiles } Straßenkacheln, { $size }
azmaps-offline-kept-failed = { $name } behalten: { $tiles } Straßenkacheln, { $size }. { $failed } konnten nicht geladen werden und werden bei der nächsten Route erneut versucht.
azmaps-offline-failed = Das Gebiet konnte nicht behalten werden: { $why }
azmaps-offline-empty = Die Routing-Kacheln haben im Gebiet auf der Karte keine Straßen.
azmaps-offline-no-name = Gib dem Gebiet zuerst einen Namen.
azmaps-offline-stopped = Der Download wurde angehalten; nichts wurde behalten.
azmaps-offline-row = { $name } · { $tiles } Straßenkacheln · { $size }

## Gespeicherte Orte, Import und Export

azmaps-category-places = Orte
azmaps-saved = Gespeichert
azmaps-list-favourites = Favoriten
azmaps-list-recents = Letzte Ziele
azmaps-clear-recent-destinations = Letzte Ziele und Suchen löschen
azmaps-favourite = Favorit
azmaps-lists-unsaved = Die gespeicherten Orte konnten nicht geschrieben werden: { $why }
azmaps-places-title = Import und Export
azmaps-import-file = Datei importieren …
azmaps-import-folder = Ordner importieren …
azmaps-import-formats = GPX (auch OsmAnd-Favoriten), KML oder KMZ (Google My Maps, Organic Maps, MAPS.ME), GeoJSON, CSV mit Breite und Länge, oder der entpackte Google-Takeout-Ordner (Gespeicherte Orte, markierte Orte, die gespeicherten Listen).
azmaps-import-apple = Apple Karten kann seine Orte nicht exportieren: Teilen Sie einen Ort daraus und fügen Sie den Link hier ein.
azmaps-import-link = Geteilten Link von Apple Karten oder Google Maps einfügen
azmaps-import-link-add = Hinzufügen
azmaps-import-busy = Importiere …
azmaps-import-done = { $places } Orte in { $lists } Listen importiert; { $review } zu prüfen.
azmaps-import-failed = Der Import ist fehlgeschlagen: { $why }
azmaps-review-head = { $count } Orte kamen ohne Koordinaten: Prüfen Sie, was die Suche gefunden hat.
azmaps-review-match = Gefunden: { $place }
azmaps-review-none = Von der Suche nicht gefunden.
azmaps-review-use = Übernehmen
azmaps-review-next = Anderer Treffer
azmaps-review-skip = Überspringen
azmaps-export-gpx = Als GPX exportieren
azmaps-export-geojson = Als GeoJSON exportieren
azmaps-export-offered = Exportiert; eine Kopie liegt als { $file }
azmaps-export-kept = Als { $file } in Ihrem Datenordner abgelegt
azmaps-export-failed = Der Export konnte nicht geschrieben werden: { $why }
azmaps-speak-destination = Ziel sprechen

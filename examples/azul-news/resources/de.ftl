# AzNews' words in German (src/l10n.rs): Mail's and Outlook's terms, "du".

## The window

aznews-about-summary = Deine Feeds - RSS, Atom und JSON Feed - in Ordnern, rücksichtsvoll aktualisiert (ETag, Last-Modified), gelesen in einer aufgeräumten Leseansicht. Die Liste ist eine OPML-Datei, die Artikel und deine Markierungen sind einfache Dateien. Teil der Azlin-Apps, gebaut mit azul.
aznews-articles = Artikel
aznews-sources = Quellen
aznews-feeds = Feeds
aznews-panes = Bereiche

## The toolbar

aznews-get-news = Neuigkeiten abrufen
aznews-fetching = Wird abgerufen…
aznews-mark-read = Gelesen
aznews-mark-unread = Ungelesen
aznews-star = Stern
aznews-unstar = Stern entfernen
aznews-read-later = Später lesen
aznews-not-later = Nicht später
aznews-open = Öffnen
aznews-copy-link = Link kopieren
aznews-mark-all-read = Alle gelesen
aznews-toolbar-add-feed = Feed hinzufügen
aznews-search = Suchen
aznews-search-articles = Artikel durchsuchen

## The source list

aznews-all-articles = Alle Artikel
aznews-unread = Ungelesen
aznews-starred = Mit Stern
aznews-broken-feeds = Fehlerhafte Feeds
aznews-all = Alle
aznews-paused = pausiert
aznews-menu-refresh = Aktualisieren
aznews-mark-all-as-read = Alle als gelesen markieren
aznews-menu-edit = Bearbeiten…
aznews-follow = Verfolgen
aznews-stop-following = Nicht mehr verfolgen
aznews-menu-remove = Entfernen…
aznews-news-activity = AKTIVITÄT
aznews-fetching-feeds = Feeds werden abgerufen…
aznews-feeds-done = { $done } von { $total } Feeds
aznews-activity-summary = { $unread } ungelesen · { $followed } von { $count } Feeds verfolgt
aznews-add-a-feed = Feed hinzufügen
aznews-hide-activity = Aktivität ausblenden
aznews-show-activity = Aktivität einblenden
aznews-actions = Aktionen
aznews-manage-sources = Quellen verwalten…
aznews-menu-add-feed = Feed hinzufügen…
aznews-menu-import = Abonnements importieren (OPML)…
aznews-menu-export = Abonnements exportieren (OPML)
aznews-menu-settings = Einstellungen…

## Ages and dates

aznews-age-now = jetzt
aznews-age-min = { $n } Min.
aznews-age-h = { $n } Std.
aznews-age-d = { $n } T.
aznews-just-now = gerade eben
aznews-ago = vor { $age }
aznews-never = nie
aznews-not-refreshed = Noch nicht aktualisiert
aznews-updated = Aktualisiert { $when }
aznews-asked = Abgefragt { $when }

## The article list

aznews-view-all = Alle Artikel
aznews-view-later = Später lesen
aznews-view-broken = Fehlerhafte Feeds
aznews-no-title = (kein Titel)
aznews-reading-feeds = Deine Feeds werden gelesen…
aznews-no-feeds = Noch keine Feeds
aznews-no-feeds-detail = Füge einen Feed über seine Adresse oder die einer Website hinzu, oder importiere eine OPML-Datei aus einem anderen Reader.
aznews-add-feed = Feed hinzufügen
aznews-nothing-here = Hier ist nichts
aznews-no-articles-in-view = Keine Artikel in dieser Ansicht.
aznews-no-match = Kein Artikel passt zu „{ $query }“.
aznews-count-unread =
    { $count ->
        [one] 1 Artikel
       *[other] { $count } Artikel
    }, { $unread } ungelesen
aznews-mark-all-ask =
    { $count ->
        [one] Den Artikel hier als gelesen markieren?
       *[other] Die { $count } Artikel hier als gelesen markieren?
    }
aznews-mark-as-read = Als gelesen markieren
aznews-col-title = Titel
aznews-col-source = Quelle
aznews-col-date = Datum
aznews-sort-unread = Nach ungelesen sortieren
aznews-sort-starred = Nach Stern sortieren
aznews-sort-title = Nach Titel sortieren
aznews-sort-source = Nach Quelle sortieren
aznews-sort-date = Nach Datum sortieren

## The reading pane

aznews-no-article = Kein Artikel ausgewählt
aznews-no-article-detail = Wähle einen Artikel in der Liste aus oder füge einen Feed hinzu.
aznews-reading-time = Lesezeit
aznews-minutes = { $n } Min.
aznews-author = Autor
aznews-link = Link
aznews-pictures-not-shown =
    { $count ->
        [one] 1 Bild wird nicht angezeigt (Einstellungen, Lesen, Bilder).
       *[other] { $count } Bilder werden nicht angezeigt (Einstellungen, Lesen, Bilder).
    }
aznews-pictures-not-loaded =
    { $count ->
        [one] 1 Bild wurde nicht geladen: Die Website, auf der es liegt, erfährt nichts davon, was du liest.
       *[other] { $count } Bilder wurden nicht geladen: Die Websites, auf denen sie liegen, erfahren nichts davon, was du liest.
    }
aznews-load-pictures = Bilder laden
aznews-summary-only = Der Feed enthält eine Zusammenfassung dieses Artikels.
aznews-show-full-text = Volltext anzeigen
aznews-loading-full-text = Der Volltext wird von der Seite des Artikels geladen...
aznews-full-text-shown = Der Volltext, von der Seite des Artikels.
aznews-show-feed-text = Text des Feeds anzeigen
aznews-no-full-text = Die Seite des Artikels enthält keinen Text, der sich hier zeigen lässt.
aznews-full-text-failed = Der Volltext konnte nicht geladen werden: { $why }
aznews-try-again = Erneut versuchen
aznews-image = [Bild]
aznews-image-alt = [Bild: { $alt }]

## Add a feed

aznews-website-or-feed = Website oder Feed
aznews-find = Suchen
aznews-looking-for-feeds = Feeds werden gesucht…
aznews-found-feeds =
    { $count ->
        [one] 1 Feed gefunden
       *[other] { $count } Feeds gefunden
    }
aznews-count-articles =
    { $count ->
        [one] 1 Artikel
       *[other] { $count } Artikel
    }
aznews-folder = Ordner
aznews-no-folder = Kein Ordner
aznews-subscribe = Abonnieren
aznews-type-address = Gib die Adresse einer Website oder eines Feeds ein.
aznews-not-web-address = „{ $address }“ ist keine Webadresse.
aznews-no-feed-on = Auf { $url } gibt es keinen Feed.
aznews-feeds-unreadable = { $url } nennt Feeds, aber keiner davon ließ sich lesen.
aznews-not-modified-first = Der Server hat auf eine erste Anfrage mit „nicht geändert“ geantwortet.

## Import

aznews-import-title = Abonnements importieren (OPML)
aznews-import-dialog = Abonnements importieren
aznews-file = Datei
aznews-opml-placeholder = /Pfad/zu/abonnements.opml
aznews-choose = Auswählen…
aznews-read-file = Lesen
aznews-reading-file = Die Datei wird gelesen…
aznews-import-summary = { $count } Feeds, { $new } neu, { $known } schon abonniert
aznews-row-subscribed = abonniert
aznews-row-new = neu
aznews-import = Importieren
aznews-import-no-feed = Die Datei nennt keinen Feed.
aznews-import-not-opml = Das ist keine OPML-Datei.
aznews-file-missing = „{ $file }“ gibt es nicht.
aznews-file-unreadable = Die Datei konnte nicht gelesen werden: { $why }
aznews-not-a-file = „{ $path }“ ist keine Datei.
aznews-type-path = Gib den Pfad einer .opml-Datei ein oder wähle eine aus.
aznews-imported =
    { $count ->
        [one] 1 Feed importiert
       *[other] { $count } Feeds importiert
    }

## A feed's page and the sources page

aznews-name = Name
aznews-feed-title-placeholder = Der eigene Titel des Feeds
aznews-address = Adresse
aznews-website = Website
aznews-format = Format
aznews-articles-unread = { $count } ({ $unread } ungelesen)
aznews-followed = Verfolgt
aznews-followed-yes = ja
aznews-followed-no = nein - nicht aktualisiert, nicht in „Alle Artikel“ (Quellen)
aznews-last-asked = Zuletzt abgefragt
aznews-refresh-failed = Die letzte Aktualisierung ist fehlgeschlagen: { $why }
aznews-error-http = Der Server hat mit HTTP { $status } geantwortet
aznews-error-not-a-feed = Das ist kein Feed
aznews-error-web-page = Diese Adresse ist eine Webseite, kein Feed
aznews-refresh-now = Jetzt aktualisieren
aznews-show-articles = Seine Artikel anzeigen
aznews-unsubscribe = Abbestellen
aznews-unsubscribe-delete = Abbestellen und seine Artikel löschen
aznews-done = Fertig
aznews-sources-note = { $count } Feeds, { $followed } verfolgt. Ein Feed, dem du nicht folgst, behält seine Artikel, wird aber nicht aktualisiert, und seine Artikel erscheinen nicht unter „Alle Artikel“ und „Ungelesen“.
aznews-import-opml = OPML importieren…
aznews-export-opml = OPML exportieren
aznews-row-no-folder = kein Ordner
aznews-not-followed = nicht verfolgt
aznews-remove-delete = Entfernen und seine Artikel löschen
aznews-feed-removed = Der Feed wurde entfernt.

## Settings

aznews-category-reading = Lesen
aznews-category-refresh = Aktualisieren
aznews-font-size = Schriftgröße
aznews-line-width = Zeilenbreite
aznews-measure-narrow = Schmal
aznews-measure-medium = Mittel
aznews-measure-wide = Breit
aznews-paper = Papier
aznews-paper-theme = Wie das Design
aznews-paper-sepia = Sepia
aznews-pictures = Bilder
aznews-pictures-always = Immer
aznews-pictures-click = Auf Klick
aznews-pictures-never = Nie
aznews-strip-tracking = Tracking entfernen
aznews-reading-note = Artikel werden in einer Serifenschrift in der gewählten Größe gesetzt; ihre Farben folgen dem Design der App und ihrem hellen oder dunklen Modus. Tracking-Parameter (utm_*, fbclid, ...) werden aus Links entfernt.
aznews-refresh-on-start = Beim Start aktualisieren
aznews-refresh-every = Automatisch aktualisieren
aznews-every-never = Nie
aznews-every-15-min = Alle 15 Min.
aznews-every-hour = Stündlich
aznews-every-4-hours = Alle 4 Stunden
aznews-keep-articles = Artikel behalten
aznews-keep-week = Eine Woche
aznews-keep-month = Einen Monat
aznews-keep-three-months = Drei Monate
aznews-refresh-note = Eine Aktualisierung fragt jeden verfolgten Feed mit dem, was er beim letzten Mal gesagt hat (ETag, Last-Modified): Ein Feed ohne Neuigkeiten kostet nichts. Deine Feeds, Artikel und Markierungen sind Dateien in { $folder }.

## Shortcuts

aznews-shortcut-next = Nächster Artikel
aznews-shortcut-previous = Vorheriger Artikel
aznews-shortcut-star = Dem Artikel einen Stern geben oder den Stern entfernen
aznews-shortcut-read = Den Artikel als gelesen oder ungelesen markieren
aznews-shortcut-later = Später lesen
aznews-shortcut-open = Das Original im Browser öffnen
aznews-shortcut-get-news = Neuigkeiten abrufen: jeden verfolgten Feed aktualisieren
aznews-shortcut-import = Eine OPML-Datei importieren
aznews-shortcut-export = Die Abonnements als OPML exportieren
aznews-shortcut-leave-form = Ein Formular verlassen
aznews-shortcut-panes = Nächster / vorheriger Bereich

## Notices

aznews-nothing-to-refresh = Nichts zu aktualisieren.
aznews-followed-again = { $name } wird wieder verfolgt
aznews-no-longer-followed = { $name } wird nicht mehr verfolgt
aznews-files-not-read =
    { $count ->
        [one] 1 Datei konnte nicht vollständig gelesen werden - siehe Protokoll
       *[other] { $count } Dateien konnten nicht vollständig gelesen werden - siehe Protokoll
    }
aznews-files-not-written =
    { $count ->
        [one] 1 Datei konnte nicht gespeichert werden - siehe Protokoll
       *[other] { $count } Dateien konnten nicht gespeichert werden - siehe Protokoll
    }
aznews-list-not-written = Die Abonnementliste konnte nicht gespeichert werden - siehe Protokoll. Schließe das Fenster noch einmal, um ohne sie zu beenden.
aznews-exported-to = Exportiert nach { $path }
aznews-marked-read =
    { $count ->
        [one] 1 Artikel als gelesen markiert
       *[other] { $count } Artikel als gelesen markiert
    }
aznews-no-link = Dieser Artikel hat keinen Link.
aznews-link-copied = Der Link des Artikels ist in der Zwischenablage.

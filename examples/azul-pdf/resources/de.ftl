# AzPdf's words in German (src/l10n.rs): the terms of PDF viewers, "du".

## The window

azpdf-about-summary = Ein PDF-Betrachter: Seitenminiaturen, die Gliederung, Zoom und Suche. azul liest das PDF und zeichnet jede Seite mit seinem SVG-Renderer. Die zuletzt geöffneten Dokumente werden in deinem Datenordner aufbewahrt.
azpdf-opening-empty = Wird geöffnet…
azpdf-could-not-open = Das PDF konnte nicht geöffnet werden
azpdf-no-document = Kein Dokument geöffnet
azpdf-no-document-detail = Öffne ein PDF, zieh eines auf dieses Fenster oder wähle ein zuletzt geöffnetes Dokument.
azpdf-open-pdf-action = PDF öffnen…
azpdf-recent = Zuletzt geöffnet
azpdf-recent-page = S. { $page } von { $count }

## The toolbar

azpdf-document = Dokument
azpdf-open = Öffnen
azpdf-previous-page = Vorherige Seite
azpdf-next-page = Nächste Seite
azpdf-first-page-now = Das ist die erste Seite
azpdf-last-page-now = Das ist die letzte Seite
azpdf-page = Seite
azpdf-page-count-name = Seitenzahl
azpdf-zoom = Zoom
azpdf-zoom-in = Vergrößern
azpdf-zoom-out = Verkleinern
azpdf-fit-width = Seitenbreite
azpdf-fit-page = Ganze Seite
azpdf-find = Im Dokument suchen
azpdf-settings = Einstellungen
azpdf-export-filled = Ausgefülltes PDF exportieren
azpdf-save-signed = Unterschriebenes PDF speichern
azpdf-sign = Unterschreiben
azpdf-sign-title = Ihre Unterschrift
azpdf-sign-pad = Unterschriftenfeld
azpdf-sign-hint = Zeichnen Sie Ihre Unterschrift mit der Maus, einem Stift oder dem Finger.
azpdf-sign-saved-shown = Ihre gespeicherte Unterschrift. Für eine neue zeichnen Sie in das Feld.
azpdf-sign-here = Hier unterschreiben
azpdf-sign-clear = Löschen
azpdf-sign-cancel = Abbrechen
azpdf-sign-use = Unterschrift verwenden
azpdf-sign-nothing-drawn = Zeichnen Sie zuerst Ihre Unterschrift.
azpdf-sign-click-page = Klicken Sie auf eine Seite, wo die Unterschrift hin soll.
azpdf-sign-placed = Unterschrift auf Seite { $page } gesetzt.
azpdf-sign-placed-name = Unterschrift
azpdf-sign-remove = Unterschrift entfernen
azpdf-sign-not-kept = Die Unterschrift konnte nicht für das nächste Mal gespeichert werden (Schlüsselbund).

## The pages, the outline, the search

azpdf-pages = Seiten
azpdf-outline = Gliederung
azpdf-thumbnails = Seitenminiaturen
azpdf-no-outline = Dieses Dokument hat keine Gliederung.
azpdf-page-n = Seite { $page }
azpdf-searching = Wird gesucht…
azpdf-no-matches = Keine Treffer
azpdf-matches =
    { $count ->
        [one] 1 Treffer
       *[other] { $count } Treffer
    }
azpdf-no-matches-for = Keine Treffer für „{ $query }“
azpdf-close-search = Die Suche schließen
azpdf-and-more = … und { $count } weitere
azpdf-landscape = quer

## Shortcuts

azpdf-open-pdf = Ein PDF öffnen
azpdf-shortcuts-view = Ansicht
azpdf-shortcuts-search = Suche
azpdf-shortcut-actual-size = Originalgröße (100 %)
azpdf-shortcut-next-previous = Die nächste / vorherige Seite
azpdf-shortcut-first-last = Die erste / letzte Seite
azpdf-shortcut-find = Im Dokument suchen

## Notices

azpdf-making-sample = Das Beispieldokument wird erstellt…
azpdf-opening = { $title } wird geöffnet…
azpdf-page-count =
    { $count ->
        [one] 1 Seite
       *[other] { $count } Seiten
    }
azpdf-pages-skipped =
    { $count ->
        [one] 1 Seite — { $skipped } Teile der Datei wurden übersprungen
       *[other] { $count } Seiten — { $skipped } Teile der Datei wurden übersprungen
    }
azpdf-not-opened = { $title } konnte nicht geöffnet werden
azpdf-saved = { $name } gespeichert
azpdf-not-filled = Das Formular konnte nicht ausgefüllt werden: { $why }
azpdf-sample-not-made = Das Beispiel konnte nicht erstellt werden: { $why }
azpdf-recent-not-saved = Die Liste der zuletzt geöffneten Dokumente konnte nicht gespeichert werden: { $why }
azpdf-sample-not-saved = Das Beispiel konnte nicht gespeichert werden: { $why }
azpdf-pdf-documents = PDF-Dokumente
azpdf-gone = { $title } ist nicht mehr da
azpdf-reading-text = Der Text der Seiten wird gelesen…
azpdf-filling-form = Das Formular wird ausgefüllt…

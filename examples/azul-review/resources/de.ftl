# AzReview's words in German (src/l10n.rs), "du".

## AzReview

azreview-about-summary = Code-Review mit Tinte zuerst: die Dateien eines Ordners als Seiten, mit einem Stift markiert, mit Sprachclips. Sitzungen werden in deinem Datenordner aufbewahrt.
azreview-shortcuts-review = Review
azreview-shortcut-save = Die Sitzung jetzt speichern
azreview-shortcut-meaning = Die Bedeutung der Tinte wählen
azreview-shortcut-previous-nib = Die vorherige Feder
azreview-voice-note = { $count ->
    [one] { $count } Sample gesprochener Begründung
   *[other] { $count } Samples gesprochener Begründung
 }
azreview-no-files = Keine prüfbaren Dateien unter { $folder }
azreview-save-failed = Die Sitzung wurde nicht gespeichert: { $why }
azreview-menu-ink = Tinte
azreview-cycle-nib = Feder wechseln
azreview-toggle-recording = Aufnahme starten / beenden
azreview-menu-session = Sitzung
azreview-save-now = Jetzt speichern
azreview-reveal-archive = Archivordner zeigen
azreview-files = Dateien
azreview-stop-recording = Aufnahme beenden
azreview-record = Aufnehmen
azreview-save = Speichern
azreview-settings = Einstellungen
azreview-files-switch = Dateien ein- oder ausblenden
azreview-page = Seite { $n }
azreview-dismiss = Schließen
azreview-open-file = Öffne eine Datei, um zu beginnen
azreview-meaning-scope = Bereich
azreview-meaning-issue = Problem
azreview-meaning-question = Frage
azreview-meaning-duplicate = Duplikat
azreview-meaning-praise = Lob
azreview-nib-marker = Marker
azreview-nib-pen = Stift
azreview-nib-audio-pen = Audiostift

## Whiteboard

azreview-whiteboard-notice-too-many-items = Das Whiteboard fasst { $limit } Elemente, mehr hat nicht Platz: nimm zuerst eines weg.
azreview-whiteboard-notice-pictures-too-big = Die Bilder auf dem Whiteboard belegen so viel Speicher wie erlaubt ({ $limit } Pixel): nimm zuerst einen Ausschnitt weg.
azreview-whiteboard-notice-picture-too-big = Das Bild dieses Ausschnitts hat { $px } Pixel, mehr als die { $limit }, die ein Ausschnitt haben darf: kreise weniger Code ein.
azreview-whiteboard-notice-picture-invalid = Das Bild des Ausschnitts ließ sich nicht verwenden: seine Bytes passen nicht zu seiner Größe.
azreview-whiteboard-notice-scale-lowered = Dieser Ausschnitt ist groß: sein Bild wird mit { $used } % Maßstab statt { $wanted } % gezeichnet, damit es in den Speicher passt, und ist deshalb etwas weniger scharf.
azreview-whiteboard-notice-label-shortened = Eine Beschriftung hat höchstens { $limit } Zeichen: der Rest wurde abgeschnitten.
azreview-whiteboard-notice-no-such-item = Auf dem Whiteboard gibt es kein Element { $id }.
azreview-whiteboard-notice-arrow-to-itself = Ein Pfeil braucht zwei verschiedene Ausschnitte: er kann nicht am selben beginnen und enden.
azreview-whiteboard-notice-arrow-needs-a-cutout = Ein Pfeil muss an einem Ausschnitt beginnen oder enden.
azreview-whiteboard-notice-arrow-repeated = Es gibt schon einen solchen Pfeil.
azreview-whiteboard-notice-board-repaired = { $dropped } Elemente des gespeicherten Whiteboards waren nicht brauchbar und wurden ausgelassen.
azreview-whiteboard-jump-file-changed = Die Datei hat sich seit dem Einkreisen geändert; diese Zeilen sind unverändert.
azreview-whiteboard-jump-lines-moved = Diese Zeilen sind verschoben: sie stehen jetzt in den Zeilen { $first } bis { $last }.
azreview-whiteboard-jump-lines-changed = Diese Zeilen haben sich seit dem Einkreisen geändert.
azreview-whiteboard-jump-file-missing = Die Datei gibt es nicht mehr.
azreview-whiteboard-store-not-a-board = Diese Datei ist kein Whiteboard, das AzReview öffnen kann ({ $why }). Sie bleibt, wie sie ist, und das Whiteboard beginnt leer.
azreview-whiteboard-store-too-new = Dieses Whiteboard wurde von einem neueren AzReview gespeichert (Format { $found }). Es bleibt, wie es ist, und das Whiteboard beginnt leer.
azreview-whiteboard-store-too-large = Die Datei dieses Whiteboards ist weit größer, als ein Whiteboard sein kann ({ $bytes } Bytes). Sie bleibt, wie sie ist, und das Whiteboard beginnt leer.
azreview-whiteboard-surface = Whiteboard
azreview-whiteboard-empty = Kreise Code mit dem Stift ein und ziehe den Kreis in diese Leiste, um ihn hier zu behalten. Zeichne Pfeile zwischen den Stücken, um zu zeigen, wie sie zusammengehören.
azreview-whiteboard-drawing = Das Bild wird gezeichnet...
azreview-whiteboard-picture-missing = Das Bild ist nicht verfügbar. Das sind die Zeilen, die es zeigt:
azreview-whiteboard-cutout-name = { $file }, Zeilen { $first } bis { $last }
azreview-whiteboard-label-placeholder = Beschriftung hinzufügen
azreview-whiteboard-label-name = Beschriftung von { $file }, Zeilen { $first } bis { $last }

## Diktieren und die Seitenspalte der Notizen

azreview-shortcut-dictate = Den Stift auf dem Code ruhen lassen, um eine Notiz zu diktieren; erneutes Ruhen, Wegbewegen oder Zeichnen beendet es
azreview-notes = Notizen
azreview-notes-switch = Notizen ein- oder ausblenden
azreview-notes-hint-pen = Lass den Stift { $seconds } s auf dem Code ruhen und sprich: Deine Worte werden hier zur Notiz. Startet nichts, meldet das System deinen Stift vielleicht nicht als Stift: Wähle unter Einstellungen > Diktieren "Stift oder Maus".
azreview-notes-hint-any = Lass Stift oder Maus { $seconds } s auf dem Code ruhen und sprich: Deine Worte werden hier zur Notiz.
azreview-notes-off = Diktieren ist aus. Einschalten unter Einstellungen > Diktieren.
azreview-listening = Hört zu
azreview-finishing = Schließt ab
azreview-dictation-stop = Stopp
azreview-note-place = Seite { $page }, Zeile { $line }
azreview-note-waiting = Warte auf Worte ...
azreview-note-placeholder = Notiz tippen oder diktieren
azreview-note-name = Notiz { $n }
azreview-note-delete = Notiz { $n } löschen
azreview-dictation-denied = Diktieren braucht Spracherkennung, die für AzReview nicht erlaubt ist ({ $why }). Erlaube sie in den Systemeinstellungen unter Datenschutz & Sicherheit.
azreview-dictation-unavailable = Diktieren ist hier nicht verfügbar: { $why }
azreview-dictation-failed = Diktieren ist fehlgeschlagen: { $why }
azreview-load-failed = Die gespeicherte Sitzung konnte nicht gelesen werden: { $why }
azreview-category-dictation = Diktieren
azreview-dictation-when = Start durch Ruhen
azreview-dictation-off = Aus
azreview-dictation-pen = Stift
azreview-dictation-any = Stift oder Maus
azreview-dictation-rest = Ruhezeit
azreview-dictation-quiet = Stopp nach Stille
azreview-dictation-note = Den Stift auf dem Code ruhen zu lassen startet eine Notiz, die zuhört. Zeichnen, Wegbewegen, erneutes Ruhen, Escape oder eine Weile Stille beendet sie. Die Worte bleiben in der Notiz, die du bearbeiten kannst, und werden mit der Sitzung gespeichert.
azreview-rest-0-8 = 0,8 s
azreview-rest-1-2 = 1,2 s
azreview-rest-2-0 = 2 s
azreview-rest-3-0 = 3 s
azreview-quiet-5 = 5 s
azreview-quiet-10 = 10 s
azreview-quiet-20 = 20 s
azreview-quiet-40 = 40 s

## Ordner öffnen, Rückgängig, die Seiten

azreview-open-folder = Ordner öffnen ...
azreview-choose-folder = Den Ordner zum Prüfen wählen
azreview-undo = Rückgängig
azreview-redo = Wiederholen
azreview-pages = Seiten der Datei
azreview-shortcut-open-folder = Einen anderen Ordner zum Prüfen öffnen
azreview-shortcut-undo = Den letzten Strich oder das letzte Radieren zurücknehmen (Mod+Umschalt+Z holt es zurück)
azreview-ink-surface = Zeichenfläche der Seite { $n }
azreview-recording-level = Aufnahmepegel
azreview-no-folder = Wähle einen Ordner zum Prüfen.

# AzReview's words in German (src/l10n.rs), "du".

## AzReview

azreview-about-summary = Code-Review mit Tinte zuerst: die Dateien eines Ordners als Seiten, mit einem Stift markiert, mit Sprachclips. Sitzungen werden in deinem Datenordner aufbewahrt.
azreview-shortcuts-review = Review
azreview-shortcut-save = Die Sitzung jetzt speichern
azreview-shortcut-meaning = Einen Stift nehmen (dieselbe Taste noch einmal: seine nächste Feder)
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

## Aufnahme (das Feld des Recorders, src/recorder_panel.rs)

azreview-rec-status-idle = Keine Aufnahme
azreview-rec-status-recording = Aufnahme läuft
azreview-rec-status-paused-user = Von dir pausiert
azreview-rec-status-paused-idle = Pausiert: länger ist nichts passiert
azreview-rec-status-paused-quota = Pausiert: der Datenträger ist fast voll
azreview-rec-status-paused-locked = Pausiert: der Bildschirm ist gesperrt
azreview-rec-described-idle = Keine Aufnahme
azreview-rec-described-recording = Aufnahme läuft, { $time } aufgenommen
azreview-rec-described-paused = Aufnahme pausiert, { $time } aufgenommen. { $why }
azreview-rec-start = Aufnahme starten
azreview-rec-stop = Aufnahme beenden
azreview-rec-pause = Pause
azreview-rec-resume = Fortsetzen
azreview-rec-pause-described = Aufnahme pausieren (Tastenkürzel { $hotkey })
azreview-rec-resume-described = Aufnahme fortsetzen (Tastenkürzel { $hotkey })
azreview-rec-track-screen = Bildschirm
azreview-rec-track-camera = Kamera
azreview-rec-track-audio = Mikrofon
azreview-rec-problem = { $track }: { $why }
azreview-rec-disk-free = { $free } frei auf dem Datenträger.
azreview-rec-disk-low = Der Datenträger wird voll: { $free } frei.
azreview-rec-disk-full = Der Datenträger ist voll ({ $free } frei): die Aufnahme bleibt pausiert, bis wieder Platz da ist.
azreview-rec-disk-unknown = Der freie Platz des Datenträgers lässt sich nicht lesen: deshalb wird nichts pausiert.
azreview-rec-minutes-left = { $minutes ->
    [one] Noch etwa { $minutes } Minute.
   *[other] Noch etwa { $minutes } Minuten.
 }
azreview-rec-hours-left = { $hours ->
    [one] Noch etwa { $hours } Stunde.
   *[other] Noch etwa { $hours } Stunden.
 }
azreview-rec-dropped = { $count ->
    [one] { $count } Bild wurde verworfen: der Encoder kam nicht nach.
   *[other] { $count } Bilder wurden verworfen: der Encoder kam nicht nach.
 }
azreview-rec-will-display = Zeichnet alles auf dem Bildschirm auf, solange es läuft: jede App, jedes Fenster und den Mauszeiger.
azreview-rec-will-window = Zeichnet das Fenster von AzReview auf und sonst nichts auf dem Schreibtisch.
azreview-rec-will-own = Zeichnet nur auf, was AzReview selbst zeigt, sonst nichts auf dem Schreibtisch.
azreview-rec-will-camera = Zeichnet die Kamera auf.
azreview-rec-will-voice = Zeichnet das Mikrofon auf.
azreview-rec-will-events = Führt ein Protokoll über den Stift, die geöffneten Dateien und das Gesagte.
azreview-rec-denied = { $track } wird nicht aufgezeichnet: das System hat { $who } nicht erlaubt ({ $why })
azreview-rec-access-allowed = { $track }: erlaubt.
azreview-rec-access-not-asked = { $track }: das System fragt, wenn die Aufnahme beginnt.
azreview-rec-access-denied = { $track }: für { $who } ausgeschaltet. Schalte es in den Datenschutz-Einstellungen des Systems ein und starte dann erneut.
azreview-rec-access-restricted = { $track }: auf diesem Rechner eingeschränkt (ein Profil oder die Kindersicherung).
azreview-rec-notice-disk-low = Der Datenträger wird voll: { $free } frei, und die Aufnahme pausiert unter { $min }.
azreview-rec-notice-resumed = Die Aufnahme läuft weiter.
azreview-rec-notice-ending = Die Aufnahme erreicht ihre längste Dauer in { $time }.
azreview-rec-notice-length-reached = Die Aufnahme hat ihre längste Dauer erreicht und wurde beendet.
azreview-rec-notice-screen-gap = Der Bildschirm war eine Weile weg ({ $why }): das Video hat dort eine Lücke.
azreview-rec-notice-manifest-error = Die Notizen der Aufnahme konnten nicht geschrieben werden: { $why }
azreview-rec-error-busy = Es läuft schon eine Aufnahme.
azreview-rec-error-nothing = Bildschirm, Kamera und Mikrofon sind in den Einstellungen alle aus: es gibt nichts aufzunehmen.
azreview-rec-error-disk-full = Auf dem Datenträger sind { $free } frei, weniger als die { $min }, die der Recorder frei hält: schaffe Platz oder senke diese Grenze in den Einstellungen.
azreview-rec-error-io = Die Aufnahme konnte nicht angelegt werden: { $why }
azreview-rec-error-window-source = Ein einzelnes Fenster aufzunehmen braucht die Nummer des Fensters im System, die es noch nicht gibt: wähle in den Einstellungen den Bildschirm oder die eigenen Bilder von AzReview. Der ganze Bildschirm wird nicht stattdessen aufgenommen.
azreview-rec-notice-saved = Gespeichert: { $name } ({ $length }, { $size })
azreview-rec-notice-recovered = { $count ->
    [one] { $count } Aufnahme, die beim Ende der App abgebrochen wurde, ist wieder in Ordnung.
   *[other] { $count } Aufnahmen, die beim Ende der App abgebrochen wurden, sind wieder in Ordnung.
 }
azreview-rec-error-delete = { $name } wurde nicht gelöscht: { $why }
azreview-rec-error-recover = { $name } konnte nicht wiederhergestellt werden: { $why }
azreview-rec-window-title = Aufnahme
azreview-rec-hotkey-description = Die Aufnahme pausieren oder fortsetzen
azreview-rec-hotkey-unavailable = Die Pausentaste ist nicht verfügbar: ein anderes Programm hat sie.
azreview-rec-preview-on = Kamera zeigen
azreview-rec-preview-off = Kamera ausblenden
azreview-rec-preview = Kameravorschau
azreview-rec-list-title = Aufnahmen
azreview-rec-reveal = Ordner zeigen
azreview-rec-delete = Löschen
azreview-rec-notice-display-missing = Der Bildschirm { $id } aus den Einstellungen ist nicht da: aufgenommen wird der Bildschirm dieses Fensters.
azreview-menu-recorder = Aufnahme...
azreview-rec-who-unknown = das Programm, das AzReview gestartet hat
azreview-rec-state-complete = Vollständig
azreview-rec-state-recovered = Nach dem Ende der App wiederhergestellt
azreview-rec-state-unfinished = Nicht abgeschlossen
azreview-rec-state-damaged = Nicht lesbar
azreview-rec-row-problem = { $why }
azreview-rec-export = Video exportieren
azreview-rec-export-cancel = Export abbrechen
azreview-rec-export-open = Video öffnen
azreview-rec-export-running = Das Video wird erstellt: { $percent } %
azreview-rec-export-done = Video: { $name } ({ $size })
azreview-rec-export-failed = Das Video konnte nicht erstellt werden: { $why }
azreview-rec-export-cancelled = Der Export wurde abgebrochen.
azreview-rec-export-note = Hinweis: { $note }
azreview-rec-error-export = { $name } wurde nicht exportiert: { $why }

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
azreview-whiteboard-empty = Kreise Code mit dem Stift ein, ziehe dann den eingekreisten Teil in diese Leiste oder drücke darunter "An das Whiteboard senden", um ihn hier zu behalten. Zeichne Pfeile zwischen den Stücken, um zu zeigen, wie sie zusammengehören.
azreview-whiteboard-drawing = Das Bild wird gezeichnet...
azreview-whiteboard-picture-missing = Das Bild ist nicht verfügbar. Das sind die Zeilen, die es zeigt:
azreview-whiteboard-cutout-name = { $file }, Zeilen { $first } bis { $last }
azreview-whiteboard-label-placeholder = Beschriftung hinzufügen
azreview-whiteboard-label-name = Beschriftung von { $file }, Zeilen { $first } bis { $last }
azreview-whiteboard-arrow-label-name = Beschriftung des Pfeils
azreview-whiteboard-grip = Ziehe mich auf das Whiteboard
azreview-whiteboard-send = An das Whiteboard senden
azreview-whiteboard-send-name = Zeilen { $first } bis { $last } von { $file } an das Whiteboard senden
azreview-whiteboard-armed-name = Eingekreiste Zeilen { $first } bis { $last }: noch { $seconds } Sekunden, um sie auf das Whiteboard zu bringen
azreview-whiteboard-tip = Tipp: Kreise etwas Code ein und ziehe den eingekreisten Teil in die rechte Leiste
azreview-whiteboard-help-title = Das Whiteboard benutzen
azreview-whiteboard-help-circle = Kreise einige Codezeilen mit dem Stift ein. Der Kreis bleibt ein paar Sekunden bereit; der Ring um seinen Griff zeigt, wie viele noch übrig sind.
azreview-whiteboard-help-lift = Nimm den eingekreisten Teil am Griff (dem kleinen Quadrat an seiner oberen rechten Ecke), oder drücke und halte innerhalb des Kreises.
azreview-whiteboard-help-drop = Lass ihn auf der Leiste rechts fallen (gehe an den Fensterrand, um sie zu öffnen), oder drücke "An das Whiteboard senden", dann geht er von selbst dorthin.
azreview-whiteboard-help-jump = Doppelklick auf einen Ausschnitt springt zurück zu seinen Zeilen im Code.
azreview-whiteboard-help-boards = Ein Ordner kann mehrere Whiteboards haben (der Name oben in der Leiste); "Snapshot..." hält ein Board fest, wie es ist, mit dem Grund dafür.
azreview-whiteboard-notice-too-many-boards = Dieser Ordner hat { $limit } Whiteboards, so viele wie Platz ist: Lösche zuerst eines.
azreview-whiteboard-notice-board-name-cut = Der Name eines Whiteboards hat höchstens { $limit } Zeichen: der Rest wurde abgeschnitten.
azreview-whiteboard-notice-board-name-empty = Ein Whiteboard braucht einen Namen.
azreview-whiteboard-notice-no-such-board = In diesem Ordner gibt es dieses Whiteboard nicht.
azreview-whiteboard-notice-board-busy = Das Whiteboard wird gerade gespeichert oder geöffnet: einen Moment.
azreview-whiteboard-notice-history-full = Die Geschichte dieses Whiteboards ist voll ({ $snapshots } Schnappschüsse oder { $items } Elemente oder { $megabytes } MB Text in allen zusammen, höchstens): sie behält, was sie hat.
azreview-whiteboard-notice-no-such-snapshot = Diesen Schnappschuss gibt es auf diesem Whiteboard nicht.
azreview-whiteboard-notice-pictures-pending = Ein Bild wird noch gezeichnet: einen Moment, dann den Schnappschuss machen.
azreview-whiteboard-boards-not-an-index = Die Liste der Whiteboards dieses Ordners konnte nicht gelesen werden ({ $why }). Sie bleibt, wie sie ist, und eine neue Liste wird begonnen.
azreview-whiteboard-boards-too-new = Die Liste der Whiteboards dieses Ordners wurde von einem neueren AzReview gespeichert (Format { $found }). Sie bleibt, wie sie ist.
azreview-whiteboard-boards-too-large = Die Liste der Whiteboards dieses Ordners ist weit größer, als eine Liste ist ({ $bytes } Bytes). Sie bleibt, wie sie ist, und eine neue Liste wird begonnen.
azreview-whiteboard-board-default = Board { $n }
azreview-whiteboard-board-copy = { $name } Kopie
azreview-whiteboard-boards-switcher = Whiteboards, geöffnet: { $name }
azreview-whiteboard-board-new = Neues Whiteboard
azreview-whiteboard-board-rename = Umbenennen
azreview-whiteboard-board-duplicate = Duplizieren
azreview-whiteboard-board-delete = Löschen
azreview-whiteboard-board-up = Nach oben
azreview-whiteboard-board-down = Nach unten
azreview-whiteboard-board-undo = Rückgängig
azreview-whiteboard-board-deleted = Whiteboard "{ $name }" gelöscht
azreview-whiteboard-board-rename-name = Name des Whiteboards { $name }
azreview-whiteboard-board-loading = Das Whiteboard wird geöffnet ...
azreview-whiteboard-board-imported = Importiertes Whiteboard
azreview-whiteboard-export = Exportieren ...
azreview-whiteboard-export-name = Das geöffnete Whiteboard als JSON-Datei exportieren
azreview-whiteboard-import = Importieren ...
azreview-whiteboard-import-name = Eine Whiteboard-Datei als neues Whiteboard öffnen
azreview-whiteboard-export-title = "{ $name }" als JSON-Datei exportieren
azreview-whiteboard-export-what = Eine Datei mit den Ausschnitten und Pfeilen und zu jedem Ausschnitt dem Code, den er zeigt, einem Link auf diese Zeilen in der Revision, in der sie eingekreist wurden, und dem Bild. Ein Web-Viewer kann sie öffnen.
azreview-whiteboard-export-author = Die Datei nennt ihren Autor: { $author } (der Name, mit dem git eingerichtet ist; die Einstellung whiteboard-author ändert ihn oder lässt ihn weg).
azreview-whiteboard-export-author-none = Die Datei nennt keinen Autor (die Einstellung whiteboard-author steht auf off, oder git hat keinen Namen).
azreview-whiteboard-export-pictures-inside = Bilder in der Datei
azreview-whiteboard-export-pictures-beside = Bilder in einem Ordner neben der Datei
azreview-whiteboard-export-scope-history = Board und Geschichte ({ $n } Schnappschüsse)
azreview-whiteboard-export-scope-board = Nur das Board, ohne Geschichte
azreview-whiteboard-export-scope-snapshot = Nur Schnappschuss { $id }
azreview-whiteboard-snapshot = Schnappschuss ...
azreview-whiteboard-snapshot-name = Einen Schnappschuss des Whiteboards machen: das Board festhalten, wie es ist, mit dem Grund dafür
azreview-whiteboard-history = Geschichte ({ $n })
azreview-whiteboard-history-name = Die Schnappschüsse dieses Whiteboards
azreview-whiteboard-restore-why = Das Board von Schnappschuss { $id } wiederhergestellt.
azreview-whiteboard-summary-first = Der erste Schnappschuss dieses Whiteboards: alles darauf ist neu.
azreview-whiteboard-summary-none = Seit { $parent } hat sich nichts geändert.
azreview-whiteboard-summary-counts = Seit { $parent }: { $added } hinzugekommen, { $removed } entfernt, { $moved } verschoben, { $changed } geändert.
azreview-whiteboard-sheet-title = Einen Schnappschuss machen
azreview-whiteboard-sheet-restore-title = Schnappschuss { $id } wiederherstellen
azreview-whiteboard-sheet-code = Der Code steht auf { $rev } in { $branch }.
azreview-whiteboard-sheet-code-none = Der Code liegt in keinem git-Repository: der Schnappschuss nennt keine Revision.
azreview-whiteboard-sheet-dirty = Der Arbeitsbaum hat Änderungen, die nicht committet sind: die Revision sagt nicht alles.
azreview-whiteboard-sheet-pending = { $n } Bilder werden noch gezeichnet: der Schnappschuss wartet darauf.
azreview-whiteboard-sheet-why-placeholder = Warum ist die Architektur jetzt so? Warum hat sie sich seit dem letzten Schnappschuss geändert?
azreview-whiteboard-sheet-why-name = Der Grund für den Schnappschuss
azreview-whiteboard-sheet-why = Warum? Dafür ist die Geschichte da: der Grund, warum das Board so ist.
azreview-whiteboard-sheet-tags-name = Schlagwörter des Schnappschusses
azreview-whiteboard-sheet-tags = Schlagwörter: Entscheidungsnummern, Links auf Issues und Pull Requests, durch Kommas getrennt
azreview-whiteboard-sheet-save = Schnappschuss speichern
azreview-whiteboard-sheet-cancel = Abbrechen
azreview-whiteboard-history-empty = Noch keine Schnappschüsse. Mach einen, um das Board festzuhalten, wie es ist, mit dem Grund dafür.
azreview-whiteboard-snapshot-open-name = Schnappschuss { $id } ansehen
azreview-whiteboard-snapshot-no-why = (kein Grund angegeben)
azreview-whiteboard-snapshot-unverified = Dieser Schnappschuss wurde nach dem Schreiben geändert: er passt nicht zu seinem Hash.
azreview-whiteboard-viewing = Schnappschuss { $id } bei { $rev }, { $date } UTC: schreibgeschützt
azreview-whiteboard-compare-legend = Gegen { $parent }: grüner Rahmen, { $added } hinzugekommen; roter Geist, { $removed } entfernt; oranger Pfeil, { $moved } verschoben; { $changed } geändert.
azreview-whiteboard-compare-first = Der erste Schnappschuss: alles darin ist neu.
azreview-whiteboard-compare-on = Vergleichen
azreview-whiteboard-compare-name = Diesen Schnappschuss mit dem vorigen vergleichen
azreview-whiteboard-compare-off = Vergleich beenden
azreview-whiteboard-restore = Wiederherstellen ...
azreview-whiteboard-back = Zurück
azreview-whiteboard-back-name = Zurück zum Board
azreview-whiteboard-head-moved = Der Code ist von { $from } nach { $to } weitergezogen: mach einen neuen Schnappschuss, um zu sagen, warum sich die Architektur geändert hat.
azreview-whiteboard-head-material = { $n } Dinge, die seit dem letzten Schnappschuss getan wurden (erledigte Gespräche, angewendete Patches), beginnen den Grund.
azreview-whiteboard-head-take = Schnappschuss machen ...
azreview-whiteboard-head-dismiss = Jetzt nicht
azreview-whiteboard-material-intro = Seit dem letzten Schnappschuss:
azreview-whiteboard-material-more = ... und { $n } weitere
azreview-whiteboard-summary-references = Der Code, auf den die Ausschnitte zeigen: { $moved } verschoben, { $changed } geändert, { $vanished } verschwunden.
azreview-whiteboard-flag-moved = verschoben
azreview-whiteboard-flag-changed = geändert
azreview-whiteboard-flag-vanished = weg
azreview-whiteboard-flag-moved-name = Der Code, auf den dieser Ausschnitt zeigt, ist verschoben: er steht jetzt in den Zeilen { $first } bis { $last }
azreview-whiteboard-flag-changed-name = Der Code, auf den dieser Ausschnitt zeigt, hat sich geändert: er steht jetzt in den Zeilen { $first } bis { $last }, nicht wie das Bild ihn zeigt
azreview-whiteboard-flag-vanished-name = Der Code, auf den dieser Ausschnitt zeigt, ist verschwunden
azreview-whiteboard-export-go = Datei wählen ...
azreview-whiteboard-export-cancel = Abbrechen
azreview-whiteboard-exported = Exportiert nach { $path }: { $pictures } Bilder, { $links } Links auf Code.
azreview-whiteboard-export-failed = Der Export ist fehlgeschlagen: { $why }
azreview-whiteboard-imported = "{ $name }" aus { $file } geöffnet.
azreview-whiteboard-import-failed = Diese Datei konnte nicht geöffnet werden: { $why }

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
azreview-conv-chip-name = Gespräch { $n }: { $words }: { $state }
azreview-conv-empty = (noch keine Worte)
azreview-conv-more = { $n } weitere: die Leiste unten hat sie
azreview-conv-you = Du
azreview-conv-agent = Agent
azreview-conv-working = Der Agent arbeitet ...
azreview-conv-suggestions =
    { $n ->
        [one] Ein Vorschlag wartet
       *[other] { $n } Vorschläge warten
    }
azreview-conv-done = Erledigt
azreview-conv-title = { $n }: { $where }
azreview-conv-close = Das Gespräch schließen
azreview-conv-reply-placeholder = Dem Agenten antworten
azreview-conv-reply-name = Antwort an den Agenten
azreview-conv-reopen = Das Gespräch wieder öffnen
azreview-conv-resolve = Das Gespräch als erledigt markieren
azreview-conv-send = An den Agenten senden
azreview-conv-ask-again = Den Agenten noch einmal fragen
azreview-conv-heard-as = Verstanden als: { $words }
azreview-sugg-insert =
    { $n ->
        [one] Eine Zeile nach Zeile { $line } einfügen
       *[other] { $n } Zeilen nach Zeile { $line } einfügen
    }
azreview-sugg-replace = { $old } durch die Wörter darunter ersetzen (Zeile { $line })
azreview-sugg-remove = { $old } entfernen (Zeile { $line })
azreview-sugg-accept = Diesen Vorschlag übernehmen
azreview-sugg-reject = Diesen Vorschlag ablehnen
azreview-sugg-undo = Neu entscheiden
azreview-sugg-accept-all = Alle offenen übernehmen
azreview-sugg-reject-all = Alle offenen ablehnen
azreview-sugg-taken = Übernommen
azreview-sugg-refused = Abgelehnt
azreview-conv-understood = Verstanden (nach den Namen des Ordners) als: { $words }
azreview-conv-earlier =
    { $n ->
        [one] Ein früherer Beitrag wird nicht gezeigt
       *[other] { $n } frühere Beiträge werden nicht gezeigt
    }
azreview-dictation-denied = Diktieren braucht Spracherkennung, die für AzReview nicht erlaubt ist ({ $why }). Erlaube sie in den Systemeinstellungen unter Datenschutz & Sicherheit.
azreview-dictation-unavailable = Diktieren ist hier nicht verfügbar: { $why }
azreview-dictation-failed = Diktieren ist fehlgeschlagen: { $why }
azreview-load-failed = Die gespeicherte Sitzung konnte nicht gelesen werden: { $why }
azreview-category-dictation = Diktieren
azreview-menu-whiteboard-help = Das Whiteboard benutzen...
azreview-category-whiteboard = Whiteboard
azreview-whiteboard-armed-when = Ein Kreis wartet darauf, mitgenommen zu werden
azreview-armed-8 = 8 s
azreview-armed-15 = 15 s
azreview-armed-30 = 30 s
azreview-armed-60 = 60 s
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
azreview-save-pointer-log = Zeigerprotokoll speichern
azreview-no-folder-hint = AzReview zeigt die Dateien eines Ordners als Seiten, die du mit Stift oder Maus markieren kannst.

## Agenten (die Backends, die antworten: claude -p, Gemini, die APIs; die Fehler in Worten)

azreview-agent-task-exploration = Erkunden
azreview-agent-task-editing = Bearbeiten
azreview-agent-task-export = Export
azreview-agent-backend-claude-print = Claude Code (claude -p)
azreview-agent-backend-anthropic-api = Anthropic-API
azreview-agent-backend-gemini-cli = Gemini-CLI
azreview-agent-backend-gemini-api = Gemini-API
azreview-agent-backend-mock = Mock (Skript)
azreview-agent-error-not-installed = { $program } ist nicht installiert (gesucht in { $where })
azreview-agent-error-not-logged-in = { $program } ist nicht angemeldet: Terminal öffnen, { $program } starten und anmelden, dann erneut versuchen
azreview-agent-error-key-missing = Der Schlüssel { $reference } ist nicht gesetzt: lege ihn in diese Umgebungsvariable oder dieses Schlüsselbund-Objekt (niemals in die Einstellungen)
azreview-agent-error-key-denied = Der Schlüssel { $reference } konnte nicht gelesen werden: { $why }
azreview-agent-error-timeout = Zeitüberschreitung nach { $secs } s
azreview-agent-error-cancelled = Abgebrochen
azreview-agent-error-busy = { $running } Agenten arbeiten bereits (höchstens { $cap } gleichzeitig): warte, bis einer fertig ist
azreview-agent-error-spawn = { $program } konnte nicht gestartet werden: { $why }
azreview-agent-error-exit = { $program } wurde mit Exit-Code { $code } beendet: { $detail }
azreview-agent-error-http = Der Dienst antwortete { $status } { $kind }: { $detail }
azreview-agent-error-rate-limited = Der Dienst ist ausgelastet oder sein Limit ist erreicht: { $detail }
azreview-agent-error-network = Der Dienst ist nicht erreichbar: { $why }
azreview-agent-no-reply = Der Agent hat etwas geantwortet, das keine Antwort auf eine Durchsicht ist.
azreview-agent-no-answer = Der Agent hat nicht rechtzeitig geantwortet.
azreview-agent-error-budget = Der Aufruf würde mehr kosten als das Limit von { $limit }: erhöhe es in den Einstellungen
azreview-agent-error-max-turns = Der Agent hat alle { $turns } Runden verbraucht, ohne zu antworten: erhöhe das Limit in den Einstellungen oder frage enger
azreview-agent-error-refused = Der Agent hat abgelehnt: { $detail }
azreview-agent-error-bad-output = Die Antwort konnte nicht gelesen werden ({ $why }): { $excerpt }
azreview-agent-error-too-large = { $what } ist zu groß ({ $size } von { $limit }): kürze es oder erhöhe das Limit in den Einstellungen
azreview-agent-error-settings = Die Agenten-Einstellungen sind nicht brauchbar: { $why }
azreview-agent-error-unsupported = Hier nicht unterstützt: { $why }
azreview-agent-field-backend = Backend
azreview-agent-hint-backend = Wer antwortet: Claude Code (claude -p, ohne Schlüssel), die Anthropic-API, die Gemini-CLI, die Gemini-API oder ein Mock mit Skript.
azreview-agent-field-model = Modell
azreview-agent-hint-model = Ein Modellname (ein Alias wie haiku, sonnet oder opus folgt dem neuesten Modell seiner Familie). Leer: das Standardmodell des Backends.
azreview-agent-field-timeout = Zeitlimit (Sekunden)
azreview-agent-hint-timeout = Wie lange ein Aufruf dauern darf, bevor er abgebrochen wird.
azreview-agent-field-max-tokens = Längste Antwort (Tokens)
azreview-agent-hint-max-tokens = Das Meiste, was der Dienst in einer Antwort schreiben darf.
azreview-agent-field-budget = Kostenlimit pro Aufruf (US-Cent, 0 = keins)
azreview-agent-hint-budget = Claude Code bricht einen Aufruf ab, der mehr kosten würde.
azreview-agent-field-max-turns = Höchstens Runden (0 = unbegrenzt)
azreview-agent-hint-max-turns = Wie viele Schritte Claude Code machen darf (eine Datei zu lesen ist ein Schritt).
azreview-agent-field-key = Schlüssel (env:NAME oder keychain:EINTRAG)
azreview-agent-hint-key = Der Name einer Umgebungsvariable oder eines Schlüsselbund-Eintrags, der den Schlüssel enthält. Der Schlüssel selbst steht nie in den Einstellungen. Leer: die eigene Anmeldung des Programms.
azreview-agent-field-program = Programm
azreview-agent-hint-program = Der Pfad des Programms. Leer: im PATH und an den üblichen Orten suchen.
azreview-agent-field-base-url = Adresse des Dienstes
azreview-agent-hint-base-url = Leer: die übliche Adresse. Nur für einen Proxy oder einen Testserver.
azreview-agent-field-args = Zusätzliche Argumente
azreview-agent-hint-args = Werden an die Befehlszeile des Programms angehängt. Argumente, die erweitern würden, was der Agent darf, werden abgelehnt.
azreview-agent-field-max-concurrent = Agenten, die gleichzeitig arbeiten
azreview-agent-hint-max-concurrent = 1 bis 8. Mehr als zwei können viel von deinem Kontingent verbrauchen.
azreview-agent-field-limit-scale = Größe der Grenzen
azreview-agent-hint-limit-scale = 1 bis 8 mal die üblichen Grenzen für das, was gesendet und gelesen wird. Erhöhe es nur für eine große Aufgabe.
azreview-agent-problem-unknown-backend = { $field }: „{ $word }“ ist kein Backend: nimm claude-print, anthropic-api, gemini-cli, gemini-api oder mock
azreview-agent-problem-not-a-number = { $field }: „{ $word }“ ist keine Zahl
azreview-agent-problem-out-of-range = { $field }: { $value } liegt außerhalb von { $min } bis { $max }, verwendet wird { $used }
azreview-agent-problem-key-invalid = { $field }: schreibe den Schlüssel als env:NAME (eine Umgebungsvariable) oder keychain:EINTRAG
azreview-agent-problem-key-secret = { $field }: das sieht nach einem Schlüssel selbst aus: lege niemals einen Schlüssel in die Einstellungen; halte ihn in einer Umgebungsvariable oder im Schlüsselbund und schreibe hier nur deren Namen
azreview-agent-problem-arg-forbidden = { $field }: { $arg } ist hier nicht erlaubt (es würde erweitern, was der Agent darf)
azreview-agent-problem-model-required = { $field }: nenne ein Modell
azreview-agent-problem-url-invalid = { $field }: { $text } ist keine http://- oder https://-Adresse
azreview-agent-problem-unknown-field = { $field }: keine solche Einstellung ({ $word })

## Das Layout: der Dateibaum, die rechte Leiste, die untere Leiste

azreview-right-bar = Whiteboard ein- oder ausblenden
azreview-agents-none = Kein Agent arbeitet.
azreview-resize-tree = Breite des Dateibaums ändern
azreview-resize-panel = Breite des Whiteboards ändern
azreview-menu-view = Ansicht
azreview-speech-chain = Spracherkenner in der Reihenfolge, in der sie gefragt werden
azreview-speech-making = Die Sprachsitzung wird vorbereitet.
azreview-speech-failed = Die Sprachsitzung konnte nicht erstellt werden ({ $why }): Ein Diktat nutzt den eigenen Erkenner der Engine, ohne die Wörter der Seite als Wortschatz.
azreview-page-skip = Weiter zu Seite { $n }
azreview-reviewed-line = { $reviewed } von { $total } Zeilen geprüft ({ $percent } %), { $seen } gesehen
azreview-reviewed-counting = Der Ordner wird noch gezählt ...
azreview-shortcut-not-reviewed = Die Datei als nicht geprüft markieren oder wieder als geprüft (auch ein Rechtsklick im Baum)
azreview-shortcut-continue = Weiter prüfen: zurück dorthin, wo der Sprung zu einem Gespräch begann
azreview-back-label = Zurück zu { $place }
azreview-not-shown = { $file } wird nicht angezeigt: { $why }
azreview-not-shown-binary = Es ist kein Text.
azreview-not-shown-large = Es hat { $size }, mehr als die { $limit }, die ein Seitenstreifen aufnimmt.
azreview-not-shown-unreadable = Es konnte nicht gelesen werden ({ $why }).
azreview-tree-loading = Der Ordner wird gelesen ...
azreview-dictation-opening = Mikrofon wird geöffnet
azreview-mic-line = Mikrofon: { $state }
azreview-mic-cold = geschlossen
azreview-mic-warm = geschlossen, der Spracherkenner ist geladen
azreview-mic-armed = offen und wartet (die Mikrofonanzeige des Systems ist an)
azreview-mic-listening = offen, hört zu
azreview-standby-when = Mikrofon bereithalten
azreview-standby-off = Aus
azreview-standby-near = Wenn der Stift nah ist
azreview-standby-always = Immer
azreview-standby-idle = Schließen, wenn der Stift weg ist
azreview-idle-15 = 15 s
azreview-idle-30 = 30 s
azreview-idle-120 = 2 Min.
azreview-idle-600 = 10 Min.
azreview-standby-note = Das Öffnen des Mikrofons dauert eine Drittelsekunde. Bereitgehalten, beginnt ein Diktat in dem Moment, in dem der Stift geruht hat, und behält die Worte, die kurz davor gesprochen wurden. "Wenn der Stift nah ist" öffnet das Mikrofon, sobald der Stift einer Seite nahekommt, und schließt es, wenn der Stift die Zeit oben weg war oder das Fenster nach hinten geht. Die Mikrofonanzeige des Systems ist an, solange das Mikrofon offen ist; bei "Immer" bleibt sie an, solange AzReview läuft.
azreview-chip-working = ein Agent arbeitet
azreview-chip-suggested = ein Vorschlag wartet
azreview-chip-done = erledigt
azreview-chip-failed = der Agent konnte nicht antworten
azreview-chip-draft = noch nicht gefragt
azreview-chip-more-done = { $n } weitere erledigte: anzeigen
azreview-chip-fewer-done = Die erledigten einklappen
azreview-shortcut-files = Dateien ein- oder ausblenden
azreview-shortcut-whiteboard = Whiteboard ein- oder ausblenden
azreview-agent-field-effort = Aufwand
azreview-agent-hint-effort = Wie gründlich das Modell nachdenkt: low ist schnell und billig, max gründlich. Leer: der Standard des Modells. Ein Modell, das Aufwand nicht kennt, lehnt ihn ab: leere dann die Zeile.
azreview-agent-problem-not-a-choice = { $field }: „{ $word }“ gehört nicht zu den Möglichkeiten ({ $choices })

## Der Symbolindex: was der Prüfer sagt, dem Code zugeordnet (symbols*.rs)
azreview-symbols-status-walking = Suche Code-Dateien: { $found } gefunden, { $done } gelesen
azreview-symbols-status-indexing = Indiziere Code: { $done } von { $found } Dateien
azreview-symbols-status-ready = { $found } Code-Dateien indiziert
azreview-symbols-note-file-capped = { $file } hat mehr Definitionen, als der Index behält ({ $cap }): die ersten wurden indiziert.
azreview-symbols-note-index-full = Der Index enthält so viele Definitionen, wie er darf ({ $cap }): der Rest des Arbeitsbereichs ist nicht indiziert.
azreview-symbols-note-memory-full = Der Index hat den Speicher erreicht, den er belegen darf ({ $mb } MB): der Rest des Arbeitsbereichs ist nicht indiziert.
azreview-symbols-note-too-many-files = Der Arbeitsbereich hat mehr Dateien, als der Index behält ({ $cap }): der Rest ist nicht indiziert.
azreview-symbols-skip-unknown-type = keine Dateiart, die der Index liest
azreview-symbols-skip-too-large = größer, als der Index liest
azreview-symbols-skip-binary = kein Text
azreview-symbols-skip-generated = von einem Werkzeug erzeugt
azreview-symbols-skip-minified = Zeilen, die niemand von Hand geschrieben hat
azreview-symbols-skip-unreadable = konnte nicht gelesen werden

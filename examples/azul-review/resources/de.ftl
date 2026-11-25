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
azreview-agent-field-effort = Aufwand
azreview-agent-hint-effort = Wie gründlich das Modell nachdenkt: low ist schnell und billig, max gründlich. Leer: der Standard des Modells. Ein Modell, das Aufwand nicht kennt, lehnt ihn ab: leere dann die Zeile.
azreview-agent-problem-not-a-choice = { $field }: „{ $word }“ gehört nicht zu den Möglichkeiten ({ $choices })

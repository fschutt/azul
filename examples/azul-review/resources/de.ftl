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

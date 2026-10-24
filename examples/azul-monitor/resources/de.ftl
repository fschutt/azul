# AzMonitor's words in German (src/l10n.rs): the Windows Task Manager's terms, "du".

## The window

azmonitor-about-summary = Was die CPU, den Arbeitsspeicher, die Festplatte und das Netzwerk belegt - und eine Möglichkeit, es zu beenden. Messungen einmal pro Sekunde in einem Hintergrund-Thread, eine Minute Verlauf und ein Fenster, das sich dafür nie neu anordnet. Teil der Azlin-Apps, gebaut mit azul.
azmonitor-category-monitor = Überwachung
azmonitor-tab-processes = Prozesse
azmonitor-tab-performance = Leistung
azmonitor-tab-networking = Netzwerk
azmonitor-tab-users = Benutzer

## Processes

azmonitor-col-name = Name
azmonitor-col-pid = PID
azmonitor-col-user = Benutzername
azmonitor-col-cpu = CPU
azmonitor-col-memory = Arbeitsspeicher
azmonitor-col-disk = Datenträger
azmonitor-col-status = Status
azmonitor-col-description = Beschreibung
azmonitor-end-process = Prozess beenden
azmonitor-select-first = Wähle zuerst einen Prozess aus
azmonitor-filter = Nach Name, Benutzer oder PID filtern
azmonitor-filter-processes = Prozesse filtern
azmonitor-reading-system = Das System wird gelesen…
azmonitor-waiting-sample = Die Beispielmaschine (--sample).
azmonitor-waiting-live = Prozesse, CPU, Arbeitsspeicher, Datenträger und Netzwerke, in einem Hintergrund-Thread.
azmonitor-process-of-user = { $name } ({ $pid }), Benutzer { $user }
azmonitor-end-detail = Nicht gespeicherte Arbeit darin geht verloren. „Erzwungen beenden“ beendet ihn, ohne ihn zum Beenden aufzufordern.
azmonitor-end-needs-admin = Er gehört { $user }: Zum Beenden sind Administratorrechte nötig.
azmonitor-end-ask = { $name } beenden?
azmonitor-end-button = Prozess beenden
azmonitor-kill = Erzwungen beenden
azmonitor-killed = { $name } ({ $pid }) erzwungen beendet
azmonitor-ended = { $name } ({ $pid }) beendet
azmonitor-not-ended = { $name } ({ $pid }) konnte nicht beendet werden: Er gehört einem anderen Benutzer (Administratorrechte sind nötig) oder wurde gerade beendet.
azmonitor-cannot-end = { $name } ({ $pid }) kann auf diesem System nicht beendet werden.
azmonitor-needs-admin = Zum Beenden von { $name } ({ $pid }) sind Administratorrechte nötig.
azmonitor-no-process = Es läuft kein Prozess { $pid }.

## Performance

azmonitor-cpu-usage = CPU-Auslastung
azmonitor-cpu-history = Verlauf der CPU-Auslastung
azmonitor-cpu-history-of = Verlauf der CPU-Auslastung - { $cpu }
azmonitor-memory = Arbeitsspeicher
azmonitor-memory-history = Verlauf der Arbeitsspeichernutzung
azmonitor-totals = Insgesamt
azmonitor-cores = Kerne
azmonitor-up-time = Betriebszeit
azmonitor-clock = Takt
azmonitor-physical-memory-k = Physischer Speicher (K)
azmonitor-swap-k = Auslagerung (K)
azmonitor-total = Insgesamt
azmonitor-in-use = Belegt
azmonitor-available = Verfügbar
azmonitor-free = Frei
azmonitor-read = Lesen
azmonitor-write = Schreiben

## Networking and users

azmonitor-network-caption = Netzwerkauslastung (Skala { $scale })
azmonitor-received-rate = Empfangen { $rate }
azmonitor-sent-rate = Gesendet { $rate }
azmonitor-adapter = Adapter
azmonitor-received = Empfangen
azmonitor-sent = Gesendet
azmonitor-all-interfaces = Alle Netzwerkschnittstellen
azmonitor-user = Benutzer
azmonitor-unknown-user = (unbekannt)
azmonitor-active = Aktiv

## The status bar

azmonitor-status-processes = Prozesse: { $count }
azmonitor-status-processes-shown = Prozesse: { $shown } von { $count }
azmonitor-status-cpu = CPU-Auslastung: { $percent }
azmonitor-status-memory = Physischer Speicher: { $percent }
azmonitor-sample-machine = Beispielmaschine
azmonitor-this-computer = Dieser Computer
azmonitor-paused = Angehalten
azmonitor-updated-every = Aktualisiert alle { $seconds } s

## Settings

azmonitor-speed-half = 0,5 s
azmonitor-speed-one = 1 s
azmonitor-speed-two = 2 s
azmonitor-update-speed = Aktualisierungsgeschwindigkeit
azmonitor-history = Verlauf
azmonitor-export-minute = Die letzte Minute exportieren
azmonitor-export-note = Der Export ist eine CSV-Datei im Datenordner: monitor/history/<Datum>.csv.
azmonitor-source-sample = Die Messwerte kommen von der Beispielmaschine (--sample): Auf diesem Computer wird nichts beendet.
azmonitor-source-live = Die Messwerte kommen von diesem Computer (Prozesse, CPU, Arbeitsspeicher, Datenträger, Netzwerke).
azmonitor-export-failed = Der Verlauf konnte nicht exportiert werden: { $why }
azmonitor-exported = { $key } exportiert

## Shortcuts

azmonitor-shortcut-select = Einen Prozess auswählen
azmonitor-shortcut-screen = Um eine Bildschirmseite Prozesse blättern
azmonitor-shortcut-first-last = Der erste / letzte Prozess
azmonitor-shortcut-sort = Nach einer Spalte sortieren, einen zweiten Schlüssel hinzufügen
azmonitor-shortcut-end = Den ausgewählten Prozess beenden (fragt vorher)
azmonitor-shortcut-copy = Die ausgewählten Zeilen kopieren
azmonitor-shortcut-read-now = Das System jetzt lesen
azmonitor-shortcut-close-question = Die Frage schließen

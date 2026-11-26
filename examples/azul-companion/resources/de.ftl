# Die Wörter des AzReview-Begleiters, Deutsch. Jeder Schlüssel steht auch in en.ftl (l10n_test.rs prüft das).
# Ein { $name } füllt der Bildschirm aus. Eine Zeile je Schlüssel.

app-name = AzReview-Begleiter

# ---- navigation
nav-back = Zurück
nav-close = Schließen
nav-settings = Einstellungen
tab-remote = Fernbedienung
tab-files = Dateien
tab-buttons = Tasten
tabs-name = Bildschirme

# ---- the connection
conn-connected = Verbunden
conn-searching = Suche läuft
conn-lost = Verbindung verloren
conn-offline = Nicht verbunden
conn-latency = { $ms } ms

# ---- pairing (screen A)
pair-title = Mit AzReview verbinden
pair-intro = Diese App ist eine Fernbedienung für AzReview auf deinem Mac. Koppele sie einmal: Wähle in AzReview "Telefon koppeln", kopiere den Kopplungscode und füge ihn hier ein. Dann verschiebst und zoomst du die Seite, öffnest Dateien und schickst deine wiederkehrenden Anweisungen von hier.
pair-permission-ios = Dein iPhone fragt, ob die App Geräte in deinem lokalen Netzwerk finden darf. Tippe auf Erlauben: ohne das sieht die App deinen Mac nicht.
pair-permission-android = Halte dein Telefon im selben WLAN wie deinen Mac.
pair-find = Meinen Mac suchen
pair-paste = Kopplungscode einfügen
pair-scan = QR-Code scannen
pair-cable = Telefon und Mac müssen im selben WLAN sein.
pair-searching-title = Suche nach AzReview
pair-searching-body = AzReview muss auf deinem Mac geöffnet sein, und Telefon und Mac müssen im selben WLAN sein.
pair-attempt = Versuch { $n }
pair-cancel = Abbrechen
pair-found-title = { $mac } gefunden
pair-found-body = Prüfe, ob AzReview auf deinem Mac dieselbe Zahl zeigt, und bestätige sie dort.
pair-code-label = Kopplungscode
pair-waiting = Warte auf deine Bestätigung am Mac
pair-connected-title = Mit { $mac } verbunden
pair-connected-body = Von hier aus verschiebst und zoomst du die Seite, blätterst durch die Dateien und nutzt deine Schnelltasten.
pair-start = Los geht's
pair-back = Zurück
pair-scan-soon = Das Scannen des Codes kommt mit der nächsten Version. Kopiere den Kopplungscode vorerst in AzReview und füge ihn hier ein.
pair-demo = Ohne Mac ausprobieren
pair-paste-empty = Noch nichts zum Einfügen. Kopiere zuerst den Kopplungscode in AzReview.
pair-paste-not-offer = Das ist kein AzReview-Kopplungscode. Kopiere den Kopplungscode in AzReview und versuche es erneut.
pair-paste-version = Dieser Kopplungscode stammt aus einer anderen Version. Aktualisiere den Begleiter oder AzReview.
pair-paste-damaged = Dieser Kopplungscode ist beschädigt. Kopiere ihn in AzReview erneut.
pair-paste-toolong = Das ist zu lang für einen Kopplungscode.
settings-leave-demo = Demo beenden
settings-demo-note = Das ist eine Demo: ein vorgetäuschter Mac antwortet. Es wird nichts an deinen echten Mac geschickt.

# ---- when it does not work
fail-retry = Erneut versuchen
fail-paste = Neuen Code einfügen
fail-settings = Einstellungen öffnen
fail-nopermission-title = Lokales Netzwerk ist aus
fail-nopermission-body = Die App muss deinen Mac im WLAN finden. Öffne die Einstellungen, wähle diese App und schalte Lokales Netzwerk ein.
fail-notfound-title = AzReview nicht gefunden
fail-notfound-body = Prüfe, ob AzReview auf deinem Mac läuft, ob der Mac wach ist und ob dein Telefon im selben WLAN ist.
fail-updatecompanion-title = Begleiter aktualisieren
fail-updatecompanion-body = AzReview auf deinem Mac ist neuer als diese App. Öffne in AzReview Einstellungen, Geräte und installiere den Begleiter erneut über das Kabel.
fail-updateazreview-title = AzReview aktualisieren
fail-updateazreview-body = Diese App ist neuer als AzReview auf deinem Mac. Aktualisiere AzReview und versuche es dann erneut.
fail-declined-title = Kopplung abgelehnt
fail-declined-body = Die Kopplung wurde an deinem Mac abgelehnt. Starte neu, falls das ein Versehen war.
fail-wrongcode-title = Der Code hat nicht funktioniert
fail-wrongcode-body = Der Kopplungscode ist beschädigt oder wurde schon benutzt. Zeige in AzReview einen neuen Code und füge ihn erneut ein.
fail-expired-title = Der Code ist abgelaufen
fail-expired-body = Ein Kopplungscode gilt fünf Minuten. Zeige in AzReview einen neuen an und füge ihn erneut ein.
fail-forgotten-title = AzReview hat dieses Telefon vergessen
fail-forgotten-body = Der Mac kennt dieses Telefon nicht mehr. Öffne in AzReview Einstellungen, Geräte, Telefon hinzufügen und koppele erneut.
fail-busy-title = AzReview ist beschäftigt
fail-busy-body = Ein anderes Telefon wird gerade gekoppelt, oder es sind schon vier Telefone gekoppelt. Versuche es in einer Minute erneut oder entferne eines in AzReview.
fail-addresschanged-title = Mac nicht erreichbar
fail-addresschanged-body = AzReview hat unter der Adresse, die dieses Telefon gespeichert hat, nicht geantwortet. Entweder ist AzReview geschlossen, oder die Adresse deines Macs im WLAN hat sich geändert. Öffne AzReview und koppele dann erneut: scanne seinen Code oder schließe das Kabel an.
fail-pairagain = Erneut koppeln
fail-timeout-title = AzReview antwortet nicht mehr
fail-timeout-body = Der Mac hat nicht rechtzeitig geantwortet. Prüfe, ob er wach und im selben WLAN ist, und versuche es erneut.

# ---- the remote (screen B)
remote-no-file = In AzReview ist keine Datei geöffnet
remote-page = Seite { $n } von { $m }
remote-line = Zeile { $n }
remote-pad = Touchfläche für die Seite in AzReview
remote-hint = Ziehen verschiebt. Zwei Finger zoomen um den Stift. Doppeltippen passt die Breite an.
remote-zoom = { $percent } %
remote-fit = Breite anpassen
remote-reset = Ansicht zurücksetzen
remote-prev = Vorherige Seite
remote-next = Nächste Seite
remote-picture = Die Seite in AzReview, der Teil auf dem Mac ist markiert
remote-page-strip = Wischen für die nächste oder vorherige Seite
remote-context-line = Die Tasten gelten für Zeile { $n }
remote-context-region = Die Tasten gelten für den markierten Bereich
remote-context-page = Die Tasten gelten für Seite { $n }
remote-context-board = Die Tasten gelten für das Whiteboard
remote-context-none = Öffne in AzReview eine Datei, um die Tasten zu nutzen
remote-lost = Verbindung verloren. Neuer Versuch
remote-hint-board = Ziehen verschiebt das Board. Zwei Finger zoomen. Doppeltippen passt das Board an.
remote-picture-board = Das Whiteboard in AzReview, der Teil auf dem Mac ist markiert
remote-overview = Übersicht des Whiteboards
surface-code = Code-Review
surface-board = Whiteboard
surface-board-named = Whiteboard { $name }
board-items = { $n } Elemente
board-item = 1 Element
board-empty = Leeres Board
return-back = Zurück zu { $where }
return-where = { $file } S{ $page } Z{ $line }
return-where-nolines = { $file } S{ $page }
return-continue = Weiter prüfen
return-name = Weiter prüfen: zurück zu { $where }
finished-done = Agent fertig: { $where }
finished-suggestion = Agent hat einen Vorschlag: { $where }
finished-noaction = Agent hat nichts zu tun gefunden: { $where }
finished-failed = Agent konnte nicht antworten: { $where }

# ---- the four quick buttons
button-state-sending = Sende
button-state-received = Gesendet
button-state-working = Arbeitet
button-state-suggested = Vorschlag wartet
button-state-done = Fertig
button-state-failed = Fehlgeschlagen
button-name = { $label }: schickt deine Anweisung an AzReview

# ---- files (screen C)
files-root = Arbeitsbereich
files-empty = Dieser Ordner ist leer
files-loading = Lädt
files-more = Mehr laden
files-error = Der Ordner ließ sich nicht laden: { $why }
files-why-not-connected = das Telefon ist nicht mit dem Mac verbunden
files-why-no-answer = der Mac hat nicht geantwortet
files-why-connection-lost = die Verbindung wurde unterbrochen
files-why-outside-workspace = dieser Ordner liegt nicht im Arbeitsbereich
files-retry = Erneut versuchen
files-opened = In AzReview geöffnet
files-under-review = In Prüfung
files-folder = Ordner
files-file = Datei
files-state-clean = Nichts offen
files-state-unresolved = Offene Vorschläge
files-state-working = Ein Agent arbeitet
files-breadcrumb = Ordner

# ---- the buttons editor (screen D)
editor-title = Schnelltasten
editor-save = Sichern
editor-reset = Auf Standard zurücksetzen
editor-slot = Taste { $n }
editor-label = Beschriftung
editor-text = Anweisung
editor-count = { $n } / { $max }
editor-hint = Ein Tipp schickt die Anweisung mit dem Kontext am Stift an AzReview: der markierte Bereich, sonst die Zeile unter dem Stift, sonst die Seite. Die vier Tasten liegen in den Einstellungen von AzReview.
editor-capped = Auf { $max } Zeichen gekürzt
editor-saved = In AzReview gesichert
editor-label-name = Beschriftung der Taste { $n }
editor-text-name = Anweisung der Taste { $n }

# ---- settings and about (screen E)
settings-title = Einstellungen
settings-mac = Dein Mac
settings-connected = Verbunden, { $ms } ms
settings-not-connected = Nicht verbunden
settings-quick-buttons = Schnelltasten
settings-quick-buttons-hint = Halte eine Taste auf der Fernbedienung, um sie zu ändern
settings-forget = Diesen Mac vergessen
settings-forget-ask = { $mac } vergessen? Du musst dann neu koppeln.
settings-forget-yes = Vergessen
settings-forget-no = Behalten
settings-about = Über
settings-version = Version { $v }
settings-protocol = Protokoll { $p }
settings-licence = MIT-Lizenz. Teil von AzReview.
settings-no-mac = Es ist kein Mac gekoppelt

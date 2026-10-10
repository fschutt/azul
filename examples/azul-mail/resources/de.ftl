# AzMail's words in German (azul's Fluent; the keys are `azmail-<area>-<what>`).
# Every key the source names is here and in the other language's file (l10n_tests.rs).

azmail-tab-home = Start

## The main window: the ribbon, its menus and notices

azmail-tab-send-receive = Senden/Empfangen
azmail-tab-folder = Ordner
azmail-tab-view = Ansicht
azmail-tab-file = Datei
azmail-group-new = Neu
azmail-group-delete = Löschen
azmail-group-respond = Antworten
azmail-group-quick-steps = QuickSteps
azmail-group-move = Verschieben
azmail-group-tags = Kategorien
azmail-group-find = Suchen
azmail-group-send-receive = Senden und Empfangen
azmail-group-download = Herunterladen
azmail-group-server = Server
azmail-group-actions = Aktionen
azmail-group-clean-up = Aufräumen
azmail-group-properties = Eigenschaften
azmail-group-arrangement = Anordnung
azmail-group-layout = Layout
azmail-group-message = Nachricht
azmail-cmd-new-mail = Neue E-Mail
azmail-cmd-new-items = Neue Elemente
azmail-cmd-ignore = Ignorieren
azmail-cmd-clean-up = Aufräumen
azmail-cmd-junk = Junk-E-Mail
azmail-cmd-delete = Löschen
azmail-cmd-archive = Archivieren
azmail-cmd-reply = Antworten
azmail-cmd-reply-all = Allen antworten
azmail-cmd-forward = Weiterleiten
azmail-cmd-meeting = Besprechung
azmail-cmd-more = Weitere
azmail-cmd-move = Verschieben
azmail-cmd-rules = Regeln
azmail-cmd-unread-read = Ungelesen/ Gelesen
azmail-cmd-categorize = Kategorisieren
azmail-cmd-follow-up = Zur Nachverfolgung
azmail-cmd-address-book = Adressbuch
azmail-cmd-filter = E-Mail filtern
azmail-cmd-cancel-all = Alle abbrechen
azmail-cmd-send-receive-all = Alle Ordner senden/empfangen
azmail-cmd-update-folder = Ordner aktualisieren
azmail-cmd-send-all = Alle senden
azmail-cmd-send-receive-groups = Senden-Empfangen-Gruppen
azmail-cmd-show-progress = Status anzeigen
azmail-cmd-download-headers = Kopfzeilen herunterladen
azmail-cmd-mark-download = Zum Herunterladen markieren
azmail-cmd-unmark-download = Markierung zum Herunterladen aufheben
azmail-cmd-process-headers = Markierte Kopfzeilen verarbeiten
azmail-cmd-new-folder = Neuer Ordner
azmail-cmd-new-search-folder = Neuer Suchordner
azmail-cmd-rename-folder = Ordner umbenennen
azmail-cmd-copy-folder = Ordner kopieren
azmail-cmd-move-folder = Ordner verschieben
azmail-cmd-delete-folder = Ordner löschen
azmail-cmd-mark-all-read = Alle als gelesen markieren
azmail-cmd-run-rules = Regeln jetzt ausführen
azmail-cmd-clean-up-folder = Ordner aufräumen
azmail-cmd-delete-all = Alle löschen
azmail-cmd-folder-properties = Ordnereigenschaften
azmail-cmd-date = Datum
azmail-cmd-reverse-sort = Sortierung umkehren
azmail-cmd-unread-only = Nur ungelesene
azmail-cmd-navigation-pane = Navigationsbereich
azmail-cmd-reading-pane = Lesebereich
azmail-cmd-todo-bar = Aufgabenleiste
azmail-cmd-plain-text = Nur-Text
azmail-quick-move-to = Verschieben nach: ?
azmail-quick-team = Team-E-Mail
azmail-quick-reply-delete = Antworten und löschen
azmail-quick-to-manager = An Vorgesetzte(n)
azmail-quick-done = Erledigt
azmail-quick-create-new = Neu erstellen
azmail-menu-mail-message = E-Mail-Nachricht
azmail-menu-appointment = Termin
azmail-menu-meeting = Besprechung
azmail-menu-contact = Kontakt
azmail-menu-task = Aufgabe
azmail-menu-forward-attachment = Als Anlage weiterleiten
azmail-menu-reply-meeting = Mit Besprechung antworten
azmail-menu-flag = Kennzeichnung setzen/löschen
azmail-menu-all-mail = Alle E-Mails
azmail-menu-unread = Ungelesen
azmail-menu-all-accounts = Alle Konten
azmail-menu-normal = Normal
azmail-menu-minimized = Minimiert
azmail-menu-right = Rechts
azmail-menu-off = Aus
azmail-find-contact = Kontakt suchen
azmail-notice-read-only = AzMail lässt die Ordner des Servers, wie sie sind (es empfängt nur lesend); Löschen, Verschieben und Ablegen kommen mit der Synchronisierung in beide Richtungen.
azmail-notice-quick-steps = Die QuickSteps von AzMail stehen fest: Verschieben nach, Team-E-Mail, Antworten und löschen, An Vorgesetzte(n), Erledigt.
azmail-notice-whole-messages = AzMail lädt ganze Nachrichten herunter: Es gibt keine Kopfzeilen zum Markieren.
azmail-notice-meetings = Besprechungen gehören zu AzCalendar: Plane dort eine.
azmail-notice-categories = Kategorien kommen mit der Synchronisierung in beide Richtungen.
azmail-notice-by-date = Die Nachrichten sind nach Datum angeordnet.
azmail-nothing-running = Es wird nichts gesendet oder empfangen.

## The main window: the list, the reading pane, the status bar, the To-Do bar, the panes

azmail-message-list = Nachrichtenliste
azmail-about-summary = Deine E-Mails als einfache Dateien: Jeder Ordner deines IMAP-Kontos wird mit diesem Computer synchronisiert, im Layout von Outlook 2010 gelesen, im Rich-Text-Editor von azul geschrieben und direkt oder über einen SMTP-Server gesendet. Teil der Azlin-Apps, gebaut mit azul.
azmail-about-credits = Gebaut mit
azmail-category-mail = E-Mail
azmail-newest-on-top = Neueste oben
azmail-option-plain-text = Als Nur-Text lesen
azmail-option-note = Die Schalter der Registerkarte „Ansicht“; AzMail merkt sie sich. Die Konten stehen unter Datei > Informationen.
azmail-notice-outbox-first = Die E-Mails im Postausgang sind noch nicht im Laufwerk: Sie werden zuerst gesendet.
azmail-notice-already-there = Die Nachrichten sind bereits in diesem Ordner.
azmail-notice-select-first = Wähle zuerst eine Nachricht aus.
azmail-error-download = Diese Nachricht konnte nicht heruntergeladen werden: { $why }
azmail-notice-marks-wait = Die Lese- und Kennzeichnungsmarkierungen warten auf das nächste Senden/Empfangen: { $why }
azmail-notice-moved = { $count ->
    [one] 1 Nachricht nach „{ $folder }“ verschoben.
   *[other] { $count } Nachrichten nach „{ $folder }“ verschoben.
 }
azmail-notice-not-moved = Konnte nicht nach „{ $folder }“ verschieben: { $why }
azmail-notice-deleted = { $count ->
    [one] 1 Nachricht endgültig gelöscht.
   *[other] { $count } Nachrichten endgültig gelöscht.
 }
azmail-notice-not-deleted = Konnte nicht löschen: { $why }
azmail-progress = Senden/Empfangen: { $status } ({ $percent } %).
azmail-progress-last = Letztes Senden/Empfangen: { $text }
azmail-no-folder = Kein Ordner
azmail-folder-facts = { $folder }: { $items } Elemente, { $unread } ungelesen.
azmail-filter-applied = Filter angewendet
azmail-status-items = Elemente: { $count }
azmail-status-unread = Ungelesen: { $count }
azmail-status-syncing = { $status } ({ $percent } %)
azmail-no-account = Kein Konto
azmail-up-to-date-azlin = Alle Ordner sind auf dem neuesten Stand.   Verbunden mit dem Azlin-Laufwerk { $drive }
azmail-up-to-date-server = Alle Ordner sind auf dem neuesten Stand.   Verbunden mit { $server }
azmail-up-to-date = Alle Ordner sind auf dem neuesten Stand.
azmail-todo-no-appointments = Keine bevorstehenden Termine.
azmail-todo-new-task = Neue Aufgabe eingeben
azmail-task-not-saved = Die Aufgabe konnte nicht gespeichert werden: { $why }
azmail-module-mail = E-Mail
azmail-module-calendar = Kalender
azmail-module-contacts = Kontakte
azmail-module-tasks = Aufgaben
azmail-favorites-hint = Favoritenordner hierher ziehen
azmail-no-account-yet = Noch kein Konto
azmail-no-account-detail = Füge ein E-Mail-Konto hinzu, um E-Mails zu empfangen. AzMail bewahrt eine Kopie jedes Ordners als Dateien auf diesem Computer auf. Zum Schreiben brauchst du kein Konto: Eine neue Nachricht wird von diesem Computer gesendet, und die lokalen Ordner behalten, was du schreibst.
azmail-add-account = Konto hinzufügen…
azmail-list-to = An: { $name }
azmail-no-sender = (kein Absender)
azmail-no-subject = (kein Betreff)
azmail-search-folder = { $folder } durchsuchen
azmail-arrange-by = Anordnen nach:
azmail-oldest-on-top = Älteste oben
azmail-module-calendar-detail = Termine stehen in AzCalendar.
azmail-module-contacts-detail = Das Adressbuch gehört noch nicht zu AzMail.
azmail-module-tasks-detail = Die Aufgaben dieses Laufs stehen in der Aufgabenleiste.
azmail-select-item = Wähle ein Element zum Lesen aus
azmail-select-item-detail = Klicke auf eine Nachricht in der Liste, um sie hier zu sehen.
azmail-field-sent = Gesendet
azmail-field-to = An
azmail-field-cc = Cc
azmail-see-more-about = Mehr über: { $name }.
azmail-some-pictures = einige Bilder
azmail-pictures-held = Klicke hier, um Bilder herunterzuladen. Zum Schutz deiner Privatsphäre hat AzMail das automatische Herunterladen von { $held } in dieser Nachricht verhindert.
azmail-download-pictures = Bilder herunterladen
azmail-more-lines = ({ $count } weitere Zeilen)
azmail-html-not-shown = Der HTML-Teil konnte nicht angezeigt werden: { $why }
azmail-attachment-later = { $name } ist in der Nachrichtendatei; das Speichern von Anlagen kommt als Nächstes.

## The folders

azmail-folder-inbox = Posteingang
azmail-folder-drafts = Entwürfe
azmail-folder-sent = Gesendete Elemente
azmail-folder-trash = Gelöschte Elemente
azmail-folder-junk = Junk-E-Mail
azmail-folder-archive = Archiv
azmail-folder-all = Alle E-Mails
azmail-folder-flagged = Gekennzeichnet
azmail-folder-outbox = Postausgang
azmail-local-folders = Lokale Ordner

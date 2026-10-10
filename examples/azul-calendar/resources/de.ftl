# AzCalendar's words in German (azul's Fluent; the keys are `azcalendar-<area>-<what>`).
# Every key the source names is here and in the other language's file (l10n_tests.rs).

azcalendar-tab-home = START

## The window: the ribbon, the panes, FILE's pages

azcalendar-view-day = Tag
azcalendar-view-work-week = Arbeitswoche
azcalendar-view-week = Woche
azcalendar-view-month = Monat
azcalendar-view-schedule = Zeitachsenansicht
azcalendar-view-list = Liste
azcalendar-colour-blue = Blau
azcalendar-colour-green = Grün
azcalendar-colour-purple = Violett
azcalendar-colour-orange = Orange
azcalendar-colour-red = Rot
azcalendar-colour-teal = Petrol
azcalendar-colour-olive = Oliv
azcalendar-colour-grey = Grau
azcalendar-print-daily = Tagesformat
azcalendar-print-weekly = Wochenagendaformat
azcalendar-print-monthly = Monatsformat
azcalendar-file-info = Informationen
azcalendar-file-open = Öffnen und exportieren
azcalendar-file-print = Drucken
azcalendar-file-calendars = Kalender
azcalendar-file-options = Optionen
azcalendar-file-about = Info
azcalendar-module-mail = E-Mail
azcalendar-module-calendar = Kalender
azcalendar-module-contacts = Kontakte
azcalendar-module-tasks = Aufgaben
azcalendar-new-appointment = Neuer Termin
azcalendar-new-meeting = Neue Besprechung
azcalendar-today = Heute
azcalendar-next-7-days = Nächste 7 Tage
azcalendar-open-calendar = Kalender öffnen
azcalendar-share-calendar = Kalender freigeben
azcalendar-navigation-pane = Navigationsbereich
azcalendar-todo-bar = Aufgabenleiste
azcalendar-calendar-information = Kalenderinformationen
azcalendar-meeting-server = Besprechungsserver
azcalendar-import-icalendar-file-ics = Eine iCalendar-Datei importieren (.ics)
azcalendar-export-calendar-as-icalendar = Einen Kalender als iCalendar-Datei exportieren
azcalendar-new-calendar = Neuer Kalender
azcalendar-keyboard-shortcuts = Tastenkombinationen
azcalendar-events-from-outlook-google = Termine aus Outlook, Google Kalender, Apple Kalender und anderen. Ein erneut importierter Termin wird aktualisiert, nicht doppelt hinzugefügt.
azcalendar-press-enter-name-rename = Drücke die Eingabetaste in einem Namen, um seinen Kalender umzubenennen. Beim Entfernen eines Kalenders wandern seine Termine in den ersten.
azcalendar-azmeet-links-are-made = AzMeet-Links werden auf diesem Computer erstellt und funktionieren deshalb auch offline; dieser Server erhält sie, sobald er antwortet.
azcalendar-calendar-like-outlook-s = Ein Kalender wie der von Outlook, auf dem GUI-Toolkit azul: Termine sind Dateien, AzMeet-Links entstehen offline, .ics-Dateien kommen herein und gehen hinaus.
azcalendar-sync-meeting-links-now = Besprechungslinks jetzt synchronisieren
azcalendar-browse = Durchsuchen…
azcalendar-sync-now = Jetzt synchronisieren
azcalendar-import = Importieren
azcalendar-export = Exportieren
azcalendar-add = Hinzufügen
azcalendar-save = Speichern
azcalendar-file-import = Zu importierende Datei
azcalendar-calendar-ics-exports-folder = calendar.ics (im Ordner „exports“)
azcalendar-file-name-export = Dateiname für den Export
azcalendar-name = Name
azcalendar-new-calendar-s-name = Name des neuen Kalenders
azcalendar-import-into = Importieren in
azcalendar-calendar-export = Zu exportierender Kalender
azcalendar-new-calendar-named-after = Ein neuer Kalender, benannt nach der Datei
azcalendar-all-calendars = Alle Kalender
azcalendar-show-todo-bar = Aufgabenleiste anzeigen
azcalendar-show-navigation-pane = Navigationsbereich anzeigen
azcalendar-tab-view = ANSICHT
azcalendar-arrange = Anordnen
azcalendar-new = Neu
azcalendar-go-to = Gehe zu
azcalendar-manage-calendars = Kalender verwalten
azcalendar-share = Freigeben
azcalendar-current-view = Aktuelle Ansicht
azcalendar-layout = Layout
azcalendar-look = Aussehen
azcalendar-tab-file = DATEI
azcalendar-navigation-pane-label = Navigationsbereich
azcalendar-date-navigator = Datumsnavigator
azcalendar-my-calendars = Meine Kalender
azcalendar-into = In
azcalendar-theme-mode = Design und Modus
azcalendar-remove = Entfernen
azcalendar-no-upcoming-appointments = Keine bevorstehenden Termine.
azcalendar-type-new-task = Neue Aufgabe eingeben
azcalendar-appearance = Darstellung
azcalendar-status-sending = { $count ->
    [one] 1 Besprechungslink wird gesendet…
   *[other] { $count } Besprechungslinks werden gesendet…
 }
azcalendar-status-links-done = Besprechungslinks aktuell
azcalendar-status-links-unreached = { $count ->
    [one] 1 Besprechungslink wartet: Der Server ist nicht erreichbar
   *[other] { $count } Besprechungslinks warten: Der Server ist nicht erreichbar
 }
azcalendar-status-links-waiting = { $count ->
    [one] 1 Besprechungslink wartet auf den Server
   *[other] { $count } Besprechungslinks warten auf den Server
 }
azcalendar-status-items = Elemente: { $count }
azcalendar-all-day-lower = ganztägig
azcalendar-sync-all-there = Jeder Besprechungslink ist auf dem Besprechungsserver.
azcalendar-sync-sending = { $count ->
    [one] 1 Besprechungslink wird an den Besprechungsserver gesendet.
   *[other] { $count } Besprechungslinks werden an den Besprechungsserver gesendet.
 }
azcalendar-sync-waiting = { $count ->
    [one] 1 Besprechungslink wartet: { $why }
   *[other] { $count } Besprechungslinks warten: { $why }
 }
azcalendar-info-counts = { $events } Termine ({ $repeating } wiederkehrend) in { $calendars } Kalendern, { $tasks } Aufgaben.
azcalendar-info-data-folder = Datenordner: { $folder }
azcalendar-calendar-name-of = Name von { $name }
azcalendar-calendar-colour-of = Farbe von { $name }
azcalendar-keys-new-appointment = Neuer Termin
azcalendar-keys-new-meeting = Neue Besprechung
azcalendar-keys-views = Tag, Arbeitswoche, Woche, Monat, Zeitachsenansicht, Liste
azcalendar-keys-back-forward = Zurück, vor
azcalendar-keys-pane = Der nächste / vorige Bereich
azcalendar-keys-save-close = Speichern & schließen, im Terminfenster
azcalendar-share-later = Das Freigeben von Kalendern kommt später. Bis dahin speichert DATEI > Öffnen und exportieren einen Kalender als .ics-Datei, die jeder importieren kann, und DATEI > Drucken erstellt eine PDF davon.
azcalendar-opening = { $app } wird geöffnet…
azcalendar-app-not-started = { $app } konnte nicht gestartet werden: { $why }
azcalendar-app-missing = { $app } ist nicht neben AzCalendar installiert.
azcalendar-this-module = Dieses Modul
azcalendar-module-not-built = { $module } gehört noch nicht zu diesem Build.
azcalendar-import-dialog = Eine iCalendar-Datei importieren
azcalendar-import-give-file = Gib die zu importierende Datei an oder suche sie mit „Durchsuchen“.
azcalendar-reading = { $file } wird gelesen…
azcalendar-imported-name = Importiert
azcalendar-import-left-out = „{ $title }“ wird ausgelassen: { $why }.
azcalendar-imported = { $added } neue und { $updated } aktualisierte Termine aus { $file } importiert.
azcalendar-exported = { $count ->
    [one] 1 Termin nach { $path } exportiert.
   *[other] { $count } Termine nach { $path } exportiert.
 }
azcalendar-calendar-needs-name = Ein Kalender braucht einen Namen.
azcalendar-calendar-give-name = Gib dem neuen Kalender einen Namen.
azcalendar-calendar-exists = Es gibt bereits einen Kalender namens „{ $name }“.
azcalendar-server-give-address = Gib die Adresse des Besprechungsservers an, etwa https://meet.example.com oder http://127.0.0.1:8787.

## Views and printing

azcalendar-more = +{ $count } weitere
azcalendar-agenda-today = Heute, { $day }
azcalendar-agenda-tomorrow = Morgen, { $day }
azcalendar-dismiss = Schließen
azcalendar-empty-calendar = Dieser Kalender hat noch keine Termine: Klicke auf eine Uhrzeit, um einen anzulegen, oder importiere eine iCalendar-Datei (.ics).
azcalendar-import-more = Importieren…
azcalendar-back = Zurück
azcalendar-forward = Vor
azcalendar-open-day = { $day } öffnen
azcalendar-more-on = { $count } weitere am { $day }
azcalendar-agenda-empty = Nichts in diesen sieben Tagen
azcalendar-agenda-empty-detail = Die Termine der angezeigten Kalender erscheinen hier, Tag für Tag.
azcalendar-print-pages-landscape = { $pages ->
    [one] 1 Seite, A4 quer
   *[other] { $pages } Seiten, A4 quer
 }
azcalendar-print-pages-portrait = { $pages ->
    [one] 1 Seite, A4 hoch
   *[other] { $pages } Seiten, A4 hoch
 }
azcalendar-all-day = Ganztägig
azcalendar-print-week = KW { $week }
azcalendar-print-no-pdf = azul hat keine PDF erstellt: Dieser Build von azul hat keinen PDF-Writer (sein Feature `pdf`).
azcalendar-print-style = Druckformat
azcalendar-print-range = Druckbereich
azcalendar-print-start = Beginn
azcalendar-print-end = Ende
azcalendar-print-what = „Drucken“ speichert den Ausdruck als PDF-Datei, um ihn von dort zu drucken oder aufzubewahren.
azcalendar-print-date-of = { $what } des Ausdrucks
azcalendar-print-making-preview = Die Vorschau wird erstellt…
azcalendar-print-updating-preview = Die Vorschau wird aktualisiert…
azcalendar-print-page-of = Seite { $page } von { $pages }
azcalendar-print-preview-first = Die Vorschau zeigt die ersten { $shown } Seiten; „Drucken“ speichert alle { $pages }.
azcalendar-print-not-saved = Der Ausdruck wurde nicht gespeichert.
azcalendar-print-saved = { $name } gespeichert ({ $what }).
azcalendar-print-printed = Gedruckt am { $day } - AzCalendar

## The week, meetings, files and reminders

azcalendar-meet-will-mint = Beim Speichern wird ein neuer AzMeet-Link erstellt. Das geht auch offline: Der Meeting-Server bekommt ihn, sobald er antwortet.
azcalendar-event-all-day = { $title }, ganztägig
azcalendar-join-meeting = Meeting beitreten
azcalendar-meet-link-waits = Der AzMeet-Link wartet auf den Server
azcalendar-add-title = Titel hinzufügen
azcalendar-more-options = Weitere Optionen
azcalendar-add-meet-link = AzMeet-Link hinzufügen
azcalendar-saved-with-link = „{ $title }“ mit dem AzMeet-Link { $link } gespeichert
azcalendar-saved-event = „{ $title }“ gespeichert.
azcalendar-meet-unreadable = Der Meeting-Server hat eine Antwort geschickt, die AzCalendar nicht lesen kann ({ $why }).
azcalendar-meet-wrong-link = Der Meeting-Server hat einen Link geschickt, der nicht der AzMeet-Raum ist, den er erstellt hat: { $link }
azcalendar-meet-own-room = Der Meeting-Server unter { $server } hat einen eigenen Raum erstellt, statt den hier erstellten Link zu registrieren; er braucht ein Update, um in AzCalendar erstellte Links zu registrieren.
azcalendar-meet-unreachable = Der Meeting-Server unter { $server } ist nicht erreichbar: { $why }
azcalendar-meet-rate-limited = Zu viele neue Meetings aus diesem Netzwerk; versuche es in ein paar Minuten noch einmal.
azcalendar-meet-answered-with = Der Meeting-Server hat mit { $status } geantwortet: { $message }
azcalendar-meet-answered = Der Meeting-Server hat mit { $status } geantwortet.
azcalendar-meet-timed-out = Zeitüberschreitung
azcalendar-meet-no-answer = keine Antwort
azcalendar-read-no-file = { $path } konnte nicht gelesen werden: Diese Datei gibt es nicht.
azcalendar-read-failed-why = { $path } konnte nicht gelesen werden: { $why }
azcalendar-read-failed = { $path } konnte nicht gelesen werden.
azcalendar-write-failed = { $path } konnte nicht geschrieben werden: { $why }. Es wird gleich noch einmal versucht.
azcalendar-changes-not-written = { $count ->
    [one] 1 Änderung konnte nicht geschrieben werden. Schließe das Fenster noch einmal, um ohne sie zu beenden.
   *[other] { $count } Änderungen konnten nicht geschrieben werden. Schließe das Fenster noch einmal, um ohne sie zu beenden.
 }
azcalendar-untitled = (Kein Titel)
azcalendar-menu-open = Öffnen und Exportieren…
azcalendar-menu-print = Drucken…
azcalendar-menu-calendars = Kalender…
azcalendar-menu-view = Ansicht
azcalendar-menu-go-to-today = Gehe zu Heute
azcalendar-menu-settings = Einstellungen
azcalendar-menu-meeting-server = Meeting-Server…
azcalendar-reminder-on = Erinnerung: { $title } ist am { $day }{ $place }.
azcalendar-reminder-at = Erinnerung: { $title } beginnt um { $time }{ $place }.
azcalendar-reminder-day-at = Erinnerung: { $title } beginnt am { $day } um { $time }{ $place }.
azcalendar-program-missing = { $path } existiert nicht
azcalendar-program-folder-unknown = der Ordner des AzCalendar-Programms ist unbekannt
azcalendar-opening-azmeet = AzMeet wird für „{ $title }“ geöffnet…
azcalendar-azmeet-not-started = AzMeet konnte nicht gestartet werden ({ $why }), daher wurde der Meeting-Link kopiert: { $link }

## The event window

azcalendar-reminder-none = Keine
azcalendar-reminder-at-start = Zu Beginn
azcalendar-reminder-5-minutes = 5 Minuten vorher
azcalendar-reminder-10-minutes = 10 Minuten vorher
azcalendar-reminder-15-minutes = 15 Minuten vorher
azcalendar-reminder-30-minutes = 30 Minuten vorher
azcalendar-reminder-1-hour = 1 Stunde vorher
azcalendar-reminder-1-day = 1 Tag vorher
azcalendar-not-an-occurrence = Dieser Termin ist kein Vorkommen einer Serie.
azcalendar-repeat-at-least-once = Wiederhole ihn mindestens einmal.
azcalendar-repeat-ends-before = Die Wiederholung muss am ersten Tag des Termins oder danach enden.
azcalendar-times-end-that-day = Ein Termin mit Uhrzeiten endet am Tag, an dem er beginnt: Mache ihn ganztägig, damit er mehrere Tage umfasst.
azcalendar-untitled-window = Unbenannt
azcalendar-kind-meeting = Besprechung
azcalendar-kind-appointment = Termin
azcalendar-editor-title = { $title } - { $kind }
azcalendar-give-a-title = Gib dem Termin einen Titel.
azcalendar-end-after-start = Der Termin muss nach seinem Beginn enden.
azcalendar-end-on-first-day = Der Termin muss an seinem ersten Tag oder danach enden.
azcalendar-cannot-save = Dieser Termin kann nicht gespeichert werden: { $why }.
azcalendar-not-an-address = { $who } ist keine E-Mail-Adresse.
azcalendar-repeat-never = Wiederholt sich nicht
azcalendar-repeat-weekdays = Jeden Werktag (Montag bis Freitag)
azcalendar-repeat-custom-rule = Benutzerdefiniert: { $rule }
azcalendar-repeat-custom = Benutzerdefiniert
azcalendar-editor-already-open = Ein Termin ist in einem eigenen Fenster geöffnet: Speichere oder schließe ihn zuerst.
azcalendar-editor-closed = Dieser Termin ist geschlossen.
azcalendar-editor-actions = Aktionen
azcalendar-save-close = Speichern & schließen
azcalendar-delete = Löschen
azcalendar-delete-occurrence = Dieses Vorkommen löschen
azcalendar-ribbon-add-meet = AzMeet-Link hinzufügen
azcalendar-close = Schließen
azcalendar-tab-meeting = BESPRECHUNG
azcalendar-tab-appointment = TERMIN
azcalendar-occurrence-only = Nur dieses Vorkommen - wähle „Die ganze Serie“, um die Wiederholung zu ändern.
azcalendar-replace = Ersetzen
azcalendar-editor-edit = Bearbeiten
azcalendar-this-occurrence = Dieses Vorkommen ({ $day })
azcalendar-whole-series = Die ganze Serie
azcalendar-editor-subject = Betreff
azcalendar-add-a-title = Titel hinzufügen
azcalendar-editor-attendees = Teilnehmer
azcalendar-editor-location = Ort
azcalendar-where = Wo?
azcalendar-start-date = Startdatum
azcalendar-start-time = Startzeit
azcalendar-editor-start = Beginn
azcalendar-end-date = Enddatum
azcalendar-end-time = Endzeit
azcalendar-editor-end = Ende
azcalendar-editor-repeat = Wiederholen
azcalendar-editor-reminder = Erinnerung
azcalendar-editor-calendar = Kalender
azcalendar-link-waits = { $link } (wartet auf den Meeting-Server)
azcalendar-notes = Notizen
azcalendar-series-gone = Die Serie dieses Vorkommens gibt es nicht mehr: Sie wurde inzwischen gelöscht.
azcalendar-occurrence-removed = Das Vorkommen am { $day } wurde entfernt.
azcalendar-deleted-event = „{ $title }“ gelöscht.

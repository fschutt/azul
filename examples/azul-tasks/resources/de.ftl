# AzTasks' words in German (src/l10n.rs): Microsoft To Do's and Outlook's terms, "du".

## The window
aztasks-tasks = Aufgaben

## Commands, views, the ribbon and the status bar

aztasks-smart-today = Heute
aztasks-smart-upcoming = Demnächst
aztasks-smart-scheduled = Geplant
aztasks-smart-flagged = Markiert
aztasks-smart-all = Alle
aztasks-smart-completed = Erledigt
aztasks-overdue = Überfällig
aztasks-completed-count = Erledigt ({ $count })
aztasks-other-list = Andere
aztasks-column-to-do = Zu erledigen
aztasks-column-doing = In Arbeit
aztasks-column-done = Erledigt
aztasks-cmd-new-task = Neue Aufgabe
aztasks-cmd-new-list = Neue Liste
aztasks-cmd-complete = Erledigen
aztasks-cmd-flag = Markieren
aztasks-cmd-delete = Löschen
aztasks-cmd-move-up = Nach oben
aztasks-cmd-move-down = Nach unten
aztasks-cmd-show = { $view } anzeigen
aztasks-cmd-search = Suchen
aztasks-cmd-settings = Einstellungen
aztasks-cmd-shortcuts = Tastenkombinationen
aztasks-cmd-about = Info zu AzTasks
aztasks-cmd-todo-bar = Aufgabenleiste
aztasks-cmd-navigation = Navigationsbereich
aztasks-cmd-completed = Erledigte Aufgaben in Listen
aztasks-cmd-sort-manual = Manuell sortieren
aztasks-cmd-sort-due = Nach Fälligkeitsdatum sortieren
aztasks-cmd-sort-priority = Nach Priorität sortieren
aztasks-cmd-sort-title = Nach Titel sortieren
aztasks-cmd-sort-created = Nach Erstellung sortieren
aztasks-cmd-theme-flat = Design Flat
aztasks-cmd-theme-flora = Design Flora
aztasks-cmd-mode-light = Heller Modus
aztasks-cmd-mode-dark = Dunkler Modus
aztasks-cmd-mode-system = Modus des Systems
aztasks-cmd-palette = Befehlspalette
aztasks-category-task = Aufgabe
aztasks-category-go = Gehe zu
aztasks-category-app = App
aztasks-category-view = Ansicht
aztasks-move-to = Verschieben nach...
aztasks-move-selected-to = Die ausgewählten Aufgaben verschieben nach
aztasks-tab-home = START
aztasks-group-new = Neu
aztasks-group-manage = Verwalten
aztasks-group-arrange = Anordnen
aztasks-group-move = Verschieben
aztasks-tab-view = ANSICHT
aztasks-group-sort-by = Sortieren nach
aztasks-group-show = Anzeigen
aztasks-group-appearance = Darstellung
aztasks-tab-file = DATEI
aztasks-status-due-today = { $count } heute fällig
aztasks-status-overdue = { $count } überfällig
aztasks-status-not-saved = { $count } nicht gespeichert - erneut versuchen
aztasks-status-saving = { $count } werden gespeichert...
aztasks-status-reading = Wird gelesen...
aztasks-status-saved = Gespeichert
aztasks-status-open = { $count } offen
aztasks-todo-new-today = Neue Aufgabe für heute
aztasks-todo-new-for = Neue Aufgabe für { $day }
aztasks-todo-no-reminders = Keine Erinnerungen an diesem Tag.
aztasks-palette-placeholder = Gib einen Befehl ein
aztasks-confirm-delete-list = Die Liste „{ $name }“ und { $count ->
    [one] ihre 1 Aufgabe
   *[other] ihre { $count } Aufgaben
 } löschen? Das kann nicht rückgängig gemacht werden.
aztasks-confirm-clear-completed = { $count ->
    [one] Die 1 Aufgabe löschen, die vor mehr als 30 Tagen erledigt wurde?
   *[other] Die { $count } Aufgaben löschen, die vor mehr als 30 Tagen erledigt wurden?
 }

## The panes, the settings and the notices

aztasks-page-about = Info
aztasks-settings-general = Allgemein
aztasks-settings-reminders = Erinnerungen
aztasks-settings-data = Daten
aztasks-settings-find = Eine Einstellung suchen
aztasks-settings-default-list = Standardliste
aztasks-settings-default-list-what = wohin eine neue Aufgabe aus Heute, Alle oder einem Tag kommt
aztasks-settings-week-starts = Woche beginnt am
aztasks-settings-completed = Erledigte Aufgaben
aztasks-settings-completed-name = Erledigte Aufgaben unter einer Liste anzeigen
aztasks-settings-completed-what = sie unter den offenen Aufgaben einer Liste anzeigen (eingeklappt)
aztasks-settings-os-shows = Dieses System zeigt sie an ({ $why }).
aztasks-settings-os-none = Nicht auf diesem System: { $why }. Erinnerungen erscheinen nur im Fenster.
aztasks-settings-reminder-time = Erinnerungszeit
aztasks-settings-reminder-time-what = für Aufgaben, die an einem Tag ohne Uhrzeit fällig sind, und eine neue Fälligkeitszeit
aztasks-settings-sounds = Töne
aztasks-settings-sounds-name = Bei einer Erinnerung einen Ton abspielen
aztasks-settings-sounds-what = den Ton des Systems bei einer Erinnerung abspielen
aztasks-settings-notifications = Benachrichtigungen
aztasks-settings-notifications-name = Erinnerungen als Benachrichtigungen anzeigen
aztasks-settings-notifications-what = eine Erinnerung auch als Benachrichtigung des Systems anzeigen
aztasks-settings-contents = { $lists ->
    [one] 1 Liste
   *[other] { $lists } Listen
 }, { $tasks ->
    [one] 1 Aufgabe
   *[other] { $tasks } Aufgaben
 } ({ $open } offen), { $skipped ->
    [one] 1 Datei ausgelassen
   *[other] { $skipped } Dateien ausgelassen
 }
aztasks-settings-data-folder = Datenordner
aztasks-settings-data-folder-what = Eine Datei pro Aufgabe: tasks/<Liste>/<Aufgabe>.json, die tasks/<Liste>/list.json einer Liste, ihre Anhänge neben der Aufgabe. Dieselbe Anordnung wie im S3-Bucket, in den die Dateien umziehen können.
aztasks-settings-contents-title = Inhalt
aztasks-settings-import-export = Import und Export
aztasks-settings-sample = Beispiel
aztasks-settings-add-sample = Die Beispielaufgaben hinzufügen
aztasks-settings-sample-what = Listen und Aufgaben zum Ausprobieren von AzTasks
aztasks-settings-import-file = Zu importierende iCalendar-Datei
aztasks-settings-browse = Durchsuchen...
aztasks-settings-import = Importieren
aztasks-settings-import-what = Die Aufgaben einer iCalendar-Datei (Outlook, Apple Erinnerungen, Thunderbird) kommen in die Standardliste.
aztasks-settings-export = Exportieren
aztasks-settings-export-what = die angezeigte Liste (oder jede Aufgabe) als iCalendar-Datei in aztasks/exports
aztasks-keys-up-down = Pfeil auf / ab
aztasks-keys-up-down-what = Die Aufgabe darüber / darunter auswählen (Umschalt erweitert)
aztasks-keys-click = Klick / Umschalt+Klick / Cmd+Klick
aztasks-keys-click-what = Eine Aufgabe / einen Bereich auswählen / eine hinzufügen
aztasks-keys-drag = Eine Aufgabe ziehen
aztasks-keys-drag-what = Vor eine andere legen (auf die Aufgabe einer anderen Liste: dorthin verschieben)
aztasks-keys-enter = Eingabe (Schnelleingabe)
aztasks-keys-enter-what = Die eingegebene Aufgabe hinzufügen; ein Klick auf einen Chip behält seine Wörter
aztasks-keys-esc = Esc
aztasks-keys-esc-what = Die Palette oder die Backstage schließen; die Schnelleingabe leeren
aztasks-keys-lists = Cmd+7 .. Cmd+9
aztasks-keys-lists-what = Deine ersten drei Listen
aztasks-keys-note = Cmd ist die Befehlstaste unter macOS und sonst Strg. Einzelne Tasten wirken, solange kein Textfeld den Fokus hat.
aztasks-about-version = Version { $version }
aztasks-about-summary = Aufgaben und Erinnerungen: intelligente Listen, Listen in Gruppen, Tags, Schnelleingabe in einfachen Wörtern (Englisch und Deutsch), wiederkehrende Aufgaben, Erinnerungen, Schritte, Notizen und Dateien.
aztasks-about-data = Datenordner: { $folder }
aztasks-about-notifications = Benachrichtigungen: { $why }
aztasks-about-no-notifications = Benachrichtigungen: nicht auf diesem System ({ $why })
aztasks-about-built = Gebaut mit azul. MIT-Lizenz.
aztasks-import-dialog = Aufgaben aus einer iCalendar-Datei importieren
aztasks-import-give-file = Gib die zu importierende iCalendar-Datei an, oder suche sie mit „Durchsuchen“.
aztasks-import-reading = { $path } wird gelesen...
aztasks-exported = { $count ->
    [one] 1 Aufgabe nach { $path } exportiert.
   *[other] { $count } Aufgaben nach { $path } exportiert.
 }
aztasks-load-failed = Die Aufgaben konnten nicht gelesen werden: { $why }
aztasks-attach-failed = „{ $name }“ konnte nicht angehängt werden: { $why }
aztasks-files-failed = Das Verschieben oder Löschen der Anhänge ist fehlgeschlagen: { $why }
aztasks-imported = { $count ->
    [one] 1 Aufgabe aus { $file } importiert.
   *[other] { $count } Aufgaben aus { $file } importiert.
 }
aztasks-imported-some = { $count ->
    [one] 1 Aufgabe
   *[other] { $count } Aufgaben
 } aus { $file } importiert; { $missed } nicht gelesen: { $first }
aztasks-import-unreadable = { $file } konnte nicht gelesen werden: { $why }
aztasks-all-tasks = Alle Aufgaben
aztasks-on-this-computer = Auf diesem Computer
aztasks-no-tags = Noch keine Tags
aztasks-all-tags = Alle Tags
aztasks-search = Aufgaben suchen
aztasks-my-tasks = Meine Aufgaben
aztasks-my-lists = Meine Listen
aztasks-tags = Tags
aztasks-lists = Listen
aztasks-layout-month = Monat
aztasks-layout-board = Board
aztasks-layout-list = Liste
aztasks-month-previous = Vorheriger Monat
aztasks-month-next = Nächster Monat
aztasks-month-this = Dieser Monat
aztasks-planned-month = Geplanter Monat
aztasks-more = +{ $count } weitere
aztasks-board-of = Board { $list }
aztasks-board-empty-to-do = Hier gibt es nichts zu tun.
aztasks-board-empty-doing = Ziehe eine Karte hierher, wenn du sie beginnst.
aztasks-board-empty-done = Ziehe eine Karte hierher, wenn sie erledigt ist.
aztasks-completed-on = Erledigt { $day }
aztasks-import-no-list = Es gibt keine Liste, in die die Aufgaben importiert werden können: Lege zuerst eine Liste an.
aztasks-deleted-one = „{ $title }“ gelöscht.
aztasks-deleted-many = { $count } Aufgaben gelöscht.
aztasks-deleted-list = Die Liste „{ $name }“ wurde gelöscht.
aztasks-cleared = { $count ->
    [one] 1 erledigte Aufgabe entfernt.
   *[other] { $count } erledigte Aufgaben entfernt.
 }
aztasks-sample-added = Beispiellisten und -aufgaben wurden hinzugefügt.
aztasks-sample-not-added = Der Datenordner hat schon Aufgaben; --sample fügt nichts hinzu.
aztasks-notification-due = { $title } (fällig { $due })
aztasks-reminder = Erinnerung
aztasks-snooze = 10 Min. schlummern
aztasks-reminder-none = Keine
aztasks-reminder-at-due = Zur Fälligkeit
aztasks-reminder-5-minutes = 5 Minuten vorher
aztasks-reminder-15-minutes = 15 Minuten vorher
aztasks-reminder-1-hour = 1 Stunde vorher
aztasks-reminder-1-day = 1 Tag vorher
aztasks-reminder-on-date = An einem Datum...
aztasks-banner-one = Erinnerung: { $title }
aztasks-banner-two = 2 Erinnerungen: { $a }, { $b }
aztasks-banner-many = { $count } Erinnerungen: { $a }, { $b } und { $more } weitere
aztasks-dismiss = Schließen
aztasks-list-settings = Listeneinstellungen
aztasks-clear-older-than-30 = Ältere als 30 Tage entfernen
aztasks-undo = Rückgängig
aztasks-add = Hinzufügen
aztasks-close-notice = Den Hinweis schließen
aztasks-add-task = Eine Aufgabe hinzufügen
aztasks-enter-add-click-part = Eingabe fügt hinzu · ein Klick auf einen Teil behält seine Wörter
aztasks-reading-your-tasks = Deine Aufgaben werden gelesen...
aztasks-no-tasks-yet = Noch keine Aufgaben
aztasks-type-one-above-like = Gib oben eine ein, etwa „Miete zahlen morgen 9 Uhr #zuhause !hoch“, oder probiere die Beispiellisten aus.
aztasks-delete-list = Liste löschen...
aztasks-list-name = Listenname
aztasks-no-group-type-one = Keine Gruppe (oder gib eine ein: „Azlin-Start“)
aztasks-group = Gruppe
aztasks-new-tasks-outside-list = Neue Aufgaben außerhalb einer Liste kommen hierher
aztasks-move = Verschieben nach
aztasks-ctrl-cmd-click-adds = Strg / Cmd + Klick fügt eine Aufgabe hinzu, Umschalt + Klick einen Bereich; Leertaste erledigt, Entf löscht.
aztasks-add-time = Uhrzeit hinzufügen
aztasks-no-time = Keine Uhrzeit
aztasks-clear = Entfernen
aztasks-open = Öffnen
aztasks-attach-file = Eine Datei anhängen...
aztasks-title = Titel
aztasks-notes = Notizen
aztasks-add-step = Einen Schritt hinzufügen
aztasks-add-tag = Einen Tag hinzufügen
aztasks-due-date = Fälligkeitsdatum
aztasks-due-time = Fälligkeitszeit
aztasks-repeat = Wiederholen
aztasks-custom-repeat = Eigene Wiederholung
aztasks-reminder-date = Erinnerungsdatum
aztasks-notes-2 = NOTIZEN
aztasks-set-due-date-reminder = Lege für diese Erinnerung ein Fälligkeitsdatum fest
aztasks-files = DATEIEN
aztasks-drop-files-window = oder Dateien auf das Fenster ziehen
aztasks-no-task-selected = Keine Aufgabe ausgewählt
aztasks-select-task-see-steps = Wähle eine Aufgabe, um ihre Schritte, Daten, Wiederholung, Erinnerung, Tags, Notizen und Dateien zu sehen.
aztasks-count-completed = { $count } erledigt
aztasks-tagged-tasks = Getaggte Aufgaben
aztasks-search-title = Suche: { $query }
aztasks-search-what = Titel, Notizen, Tags und Schritte
aztasks-quick-add-to = Zu { $list } hinzufügen: „Miete zahlen morgen 9 Uhr #zuhause !hoch“
aztasks-quick-add = Eine Aufgabe hinzufügen: „Miete zahlen morgen 9 Uhr #zuhause !hoch“
aztasks-empty-today = Heute nichts fällig
aztasks-empty-today-what = Genieße den Tag, oder füge oben eine Aufgabe hinzu.
aztasks-empty-upcoming = Nichts in den nächsten 7 Tagen
aztasks-empty-upcoming-what = Aufgaben mit einem Fälligkeitsdatum in dieser Woche erscheinen hier.
aztasks-empty-scheduled = Nichts geplant
aztasks-empty-scheduled-what = Gib einer Aufgabe ein Fälligkeitsdatum, um sie hier zu sehen.
aztasks-empty-flagged = Keine markierten Aufgaben
aztasks-empty-flagged-what = Markiere eine Aufgabe („!“ in der Schnelleingabe), um sie hier zu behalten.
aztasks-empty-all = Alles erledigt
aztasks-empty-all-what = Jede Aufgabe ist erledigt.
aztasks-empty-completed = Noch nichts erledigt
aztasks-empty-completed-what = Erledigte Aufgaben werden hier aufbewahrt.
aztasks-empty-list = Keine Aufgaben in { $list }
aztasks-empty-list-what = Füge oben eine hinzu.
aztasks-empty-tag = Keine offenen Aufgaben mit #{ $tag }
aztasks-empty-tag-what = Tags sind Wörter mit einem # in der Schnelleingabe.
aztasks-empty-search = Keine Aufgaben passen zu „{ $query }“
aztasks-empty-search-what = Die Suche durchsucht Titel, Notizen, Tags und Schritte.
aztasks-complete-task = { $title } erledigen
aztasks-priority-of = Priorität { $priority }
aztasks-reminder-when = Erinnerung { $when }
aztasks-attachments = { $count ->
    [one] 1 Anhang
   *[other] { $count } Anhänge
 }
aztasks-colour = Farbe
aztasks-name = Name
aztasks-default = Standard
aztasks-selected = { $count ->
    [one] 1 Aufgabe ausgewählt
   *[other] { $count } Aufgaben ausgewählt
 }
aztasks-open-again = Wieder öffnen
aztasks-unflag = Markierung entfernen
aztasks-repeat-never = Nie
aztasks-repeat-daily = Täglich
aztasks-repeat-weekdays = Werktags
aztasks-repeat-weekly = Wöchentlich
aztasks-repeat-two-weeks = Alle 2 Wochen
aztasks-repeat-monthly = Monatlich
aztasks-repeat-yearly = Jährlich
aztasks-repeat-custom = Benutzerdefiniert...
aztasks-steps-of = SCHRITTE · { $done } VON { $total }
aztasks-steps = SCHRITTE
aztasks-step-done = Erledigt: { $step }
aztasks-step-remove = Den Schritt { $step } entfernen
aztasks-due = Fällig
aztasks-next-week = Nächste Woche
aztasks-remind-me = Erinnere mich
aztasks-reminds = Erinnert { $when }
aztasks-remove-file = { $name } entfernen
aztasks-created-on = Erstellt { $when }
aztasks-attach-bad-name = „{ $name }“ kann kein Dateiname im Datenordner sein.
aztasks-attach-dialog = Eine Datei anhängen
aztasks-attach-select-first = Wähle eine Aufgabe, an die die gezogenen Dateien angehängt werden.

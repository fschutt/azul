# AzContacts' words in German (src/l10n.rs): Outlook's terms, "du".

## The toolbar
azcontacts-new = Neu

## What the address book tells you

azcontacts-problem-no-name = Ein Kontakt braucht einen Namen oder eine Firma.
azcontacts-problem-not-email = „{ $value }“ ist keine E-Mail-Adresse.
azcontacts-problem-not-a-date = „{ $value }“ ist kein Datum (TT.MM.JJJJ, oder TT.MM. ohne Jahr).
azcontacts-vcard-no-end = Zeile { $line }: eine Karte ohne END:VCARD
azcontacts-vcard-no-begin = Zeile { $line }: END:VCARD ohne BEGIN
azcontacts-vcard-read-as-3 = Zeile { $line }: vCard { $version } als 3.0 gelesen
azcontacts-vcard-bad-line = Zeile { $line }: { $why }
azcontacts-vcard-last-no-end = die letzte Karte hat kein END:VCARD
azcontacts-load-not-a-file-name = { $file }: kein Dateiname eines Kontakts
azcontacts-load-no-vcard = { $file }: keine vCard in der Datei
azcontacts-load-many-cards = { $file }: { $count } Karten, die erste wird verwendet
azcontacts-import-new = { $count } neu
azcontacts-import-duplicates = { $count ->
    [one] 1 mögliches Duplikat
   *[other] { $count } mögliche Duplikate
 }
azcontacts-import-updates = { $count ->
    [one] 1 Aktualisierung
   *[other] { $count } Aktualisierungen
 }
azcontacts-dupe-same-name = gleicher Name
azcontacts-dupe-same-email = gleiche E-Mail-Adresse { $email }
azcontacts-dupe-same-phone = gleiche Telefonnummer
azcontacts-field-skip = Nicht importieren
azcontacts-first-name = Vorname
azcontacts-last-name = Nachname
azcontacts-field-full-name = Vollständiger Name
azcontacts-nickname = Spitzname
azcontacts-field-email = E-Mail
azcontacts-field-mobile-phone = Mobiltelefon
azcontacts-field-work-phone = Telefon geschäftlich
azcontacts-field-home-phone = Telefon privat
azcontacts-company = Firma
azcontacts-department = Abteilung
azcontacts-job-title = Position
azcontacts-birthday = Geburtstag
azcontacts-street = Straße
azcontacts-city = Ort
azcontacts-field-region = Bundesland / Region
azcontacts-field-postal-code = Postleitzahl
azcontacts-country = Land
azcontacts-field-web-page = Webseite
azcontacts-notes = Notizen
azcontacts-groups = Gruppen
azcontacts-csv-birthday-no-date = Zeile { $row }: Der Geburtstag „{ $value }“ ist kein Datum; der Kontakt wird ohne ihn importiert.
azcontacts-import-not-a-file = „{ $path }“ ist keine Datei.
azcontacts-files-not-read = { $count ->
    [one] 1 Kontaktdatei konnte nicht vollständig gelesen werden
   *[other] { $count } Kontaktdateien konnten nicht vollständig gelesen werden
 }
azcontacts-file-missing = „{ $file }“ existiert nicht.
azcontacts-import-no-person = Keine Zeile der Datei nennt eine Person: Ordne die Spalten unten zu.
azcontacts-import-no-vcard = Die Datei enthält keine vCard.
azcontacts-import-type-path = Gib den Pfad einer .vcf-Datei ein, oder wähle eine aus.

## The window

azcontacts-about-summary = Deine Kontakte als einfache vCard-Dateien, eine pro Person, mit Gruppen, Suche, Import, Export und einer Duplikatsuche. Teil der Azlin-Apps, gebaut mit azul.
azcontacts-contacts = Kontakte
azcontacts-new-contact = Neuer Kontakt
azcontacts-shortcut-edit = Den ausgewählten Kontakt bearbeiten
azcontacts-shortcut-save = Den bearbeiteten Kontakt speichern
azcontacts-shortcut-cancel = Das Bearbeiten abbrechen
azcontacts-shortcut-up-down = Vorheriger / nächster Kontakt in der Liste
azcontacts-shortcut-import = Eine vCard-Datei importieren
azcontacts-shortcut-export = Die Liste als vCard exportieren
azcontacts-possible-duplicates = Mögliche Duplikate
azcontacts-shortcut-panes = Bereiche
azcontacts-shortcut-panes-next = Nächster / vorheriger Bereich
azcontacts-category-contacts = Kontakte
azcontacts-label-mobile = Mobil
azcontacts-label-work = Geschäftlich
azcontacts-label-home = Privat
azcontacts-label-main = Haupt
azcontacts-label-fax = Fax
azcontacts-label-other = Weitere
azcontacts-no-name = (kein Name)
azcontacts-all-contacts-count = Alle Kontakte ({ $count })
azcontacts-favourites-count = Favoriten ({ $count })
azcontacts-duplicates-count = Mögliche Duplikate ({ $count })
azcontacts-contacts-and-groups = Kontakte und Gruppen
azcontacts-all-contacts = Alle Kontakte
azcontacts-favourites = Favoriten
azcontacts-search = Kontakte durchsuchen
azcontacts-reading-contacts = Deine Kontakte werden gelesen…
azcontacts-no-contacts = Noch keine Kontakte
azcontacts-no-contacts-detail = Lege einen an, oder importiere eine vCard-Datei.
azcontacts-nobody-here = Niemand hier
azcontacts-group-empty = Diese Gruppe hat keine Kontakte.
azcontacts-no-match = Kein Kontakt passt zu „{ $query }“.
azcontacts-edit = Bearbeiten
azcontacts-favourite = Favorit
azcontacts-copy-vcard = vCard kopieren
azcontacts-export = Exportieren
azcontacts-mail = E-Mail
azcontacts-delete = Löschen
azcontacts-delete-ask = { $name } löschen? Die Datei wird mit gelöscht.
azcontacts-keep = Behalten
azcontacts-label-birthday = Geburtstag
azcontacts-label-nickname = Spitzname
azcontacts-label-groups = Gruppen
azcontacts-label-photo = Foto
azcontacts-label-notes = Notizen
azcontacts-picture-in-card = ein Bild in der Karte
azcontacts-file = Datei: { $file }
azcontacts-label = Bezeichnung
azcontacts-add-birthday = Geburtstag hinzufügen
azcontacts-year-unknown = Jahr unbekannt
azcontacts-year = Jahr
azcontacts-birth-year = Geburtsjahr
azcontacts-edit-contact = Kontakt bearbeiten
azcontacts-save = Speichern
azcontacts-change-photo = Foto ändern…
azcontacts-remove-photo = Foto entfernen
azcontacts-photo-set = Ein Foto ist gesetzt.
azcontacts-discard-ask = Deine Änderungen verwerfen?
azcontacts-discard = Verwerfen
azcontacts-keep-editing = Weiter bearbeiten
azcontacts-name = Name
azcontacts-phone = Telefon
azcontacts-add-phone = Telefon hinzufügen
azcontacts-email = E-Mail
azcontacts-add-email = E-Mail hinzufügen
azcontacts-postcode = PLZ
azcontacts-region = Region
azcontacts-add-address = Adresse hinzufügen
azcontacts-address = Adresse
azcontacts-birthday-format = TT.MM.JJJJ, oder TT.MM. ohne Jahr
azcontacts-add-to-a-group = Zu einer Gruppe hinzufügen
azcontacts-add = Hinzufügen
azcontacts-field-name = Feldname
azcontacts-value = Wert
azcontacts-add-field = Feld hinzufügen
azcontacts-more-fields = Weitere Felder
azcontacts-status-new = neu
azcontacts-status-updates = aktualisiert { $name }
azcontacts-status-duplicate = Duplikat von { $name } ({ $score })
azcontacts-columns = Spalten
azcontacts-column = Spalte { $header }
azcontacts-import-contacts = Kontakte importieren
azcontacts-import-path = Pfad zu einer .vcf- oder .csv-Datei
azcontacts-read = Lesen
azcontacts-choose-file = Datei auswählen…
azcontacts-reading = Wird gelesen…
azcontacts-import-one = { $name } importieren
azcontacts-add-to-group = Zur Gruppe hinzufügen
azcontacts-group-optional = Gruppe (optional)
azcontacts-import = Importieren
azcontacts-import-what = vCard-3.0- und -4.0-Dateien mit einer oder vielen Karten, oder eine CSV-Datei (von Outlook oder Google, ihre Spalten den Feldern des Kontakts zugeordnet). Nichts wird importiert, bevor du auf „Importieren“ klickst.
azcontacts-no-duplicates = Keine möglichen Duplikate
azcontacts-no-duplicates-detail = Keine zwei Kontakte haben denselben Namen, dieselbe E-Mail-Adresse oder dieselbe Telefonnummer.
azcontacts-left = Links
azcontacts-right = Rechts
azcontacts-pair-of = { $pair } von { $pairs }
azcontacts-pair = { $a } ↔ { $b }   Ähnlichkeit { $score } ({ $why })
azcontacts-photo = Foto
azcontacts-keep-both-notes = Beide Notizen behalten
azcontacts-kept-from-both = Von beiden übernommen
azcontacts-not-a-duplicate = Kein Duplikat
azcontacts-merge-contacts = Kontakte zusammenführen
azcontacts-none-selected = Kein Kontakt ausgewählt
azcontacts-none-selected-detail = Wähle jemanden in der Liste, oder lege einen neuen Kontakt an.
azcontacts-duplicates = Duplikate
azcontacts-settings = Einstellungen
azcontacts-status-contacts = { $count ->
    [one] 1 Kontakt
   *[other] { $count } Kontakte
 }
azcontacts-status-duplicates = { $count ->
    [one] 1 mögliches Duplikat – prüfen
   *[other] { $count } mögliche Duplikate – prüfen
 }
azcontacts-list-and-files = Liste und Dateien
azcontacts-sort-by = Sortieren nach
azcontacts-export-as = Exportieren als
azcontacts-files-note = Jeder Kontakt ist eine vCard-Datei in { $folder }. { $count ->
    [one] 1 Paar als kein Duplikat markiert.
   *[other] { $count } Paare als kein Duplikat markiert.
 }
azcontacts-nothing-to-export = Nichts zu exportieren.
azcontacts-exporting = { $count ->
    [one] 1 Kontakt wird nach { $path } exportiert
   *[other] { $count } Kontakte werden nach { $path } exportiert
 }
azcontacts-imported-group = Importiert
azcontacts-exported = Exportiert nach { $path }
azcontacts-not-written = { $count ->
    [one] 1 Datei konnte nicht geschrieben werden - siehe das Protokoll
   *[other] { $count } Dateien konnten nicht geschrieben werden - siehe das Protokoll
 }
azcontacts-copied-vcard = { $name } als vCard { $version } kopiert
azcontacts-copied = { $what } kopiert
azcontacts-deleted = { $name } gelöscht
azcontacts-choose-photo = Ein Foto auswählen
azcontacts-reading-file = { $path } wird gelesen
azcontacts-photo-done = Foto gesetzt
azcontacts-photo-too-large = Dieses Bild ist größer als 2 MB.
azcontacts-imported = { $count ->
    [one] 1 Kontakt importiert
   *[other] { $count } Kontakte importiert
 }
azcontacts-merged = Zusammengeführt in { $name }

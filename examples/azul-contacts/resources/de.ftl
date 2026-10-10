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

# AzERP's words in German (src/l10n.rs): the terms of German asset accounting (Anlagenbuchhaltung,
# AfA), "du". The views' labels are the last section.

## The window

azerp-about-summary = Die Anlagenverwaltung des ERP auf azul: Anlagevermögen mit linearer und degressiver Abschreibung, Kategorien, Standorte, ein Wartungsprotokoll, Ausleihe und Rückgabe, CSV-Import und -Export sowie Berichte. Die Ansichten kommen aus den Ansichtsdefinitionen des ERP; jeder Datensatz ist eine Datei in deinem Datenordner. Teil der Azlin-Apps, gebaut mit azul.
azerp-close-question = Das geöffnete Formular speichern?
azerp-close-text = Deine Änderungen gehen verloren, wenn du sie nicht speicherst.
azerp-close-discard = Nicht speichern
azerp-save = Speichern
azerp-back = Zurück
azerp-next = Weiter
azerp-none = Keine.
azerp-status-assets =
    { $count ->
        [one] 1 Anlage
       *[other] { $count } Anlagen
    }
azerp-status-book-value = Buchwert { $value }
azerp-status-saving =
    { $count ->
        [one] 1 Datei wird gespeichert...
       *[other] { $count } Dateien werden gespeichert...
    }
azerp-reading-register = Das Anlagenverzeichnis wird gelesen...
azerp-reading-register-detail = Die Datensätze sind Dateien in deinem Datenordner.
azerp-nothing-here = Hier ist nichts.
azerp-no-screen = Diese Seite hat noch keine Ansicht.
azerp-panel-not-built = Dieser Bereich ist noch nicht gebaut.
azerp-not-built = „{ $name }“ ist noch nicht gebaut.
azerp-tab-table-missing = Die Tabelle dieser Registerkarte fehlt.
azerp-table-no-records = Diese Tabelle liest keine Datensätze.
azerp-nothing-recorded-asset = Für diese Anlage ist noch nichts erfasst.
azerp-add-first = Füge den ersten mit der Schaltfläche oben hinzu.
azerp-nothing-recorded = Es ist noch nichts erfasst.
azerp-none-yet = { $title }: noch keine
azerp-date-placeholder = JJJJ-MM-TT
azerp-type-to-find = Zum Suchen tippen: { $what }
azerp-choice-none = (keine)
azerp-delete-ask = { $number } löschen?
azerp-delete-what = { $number } { $name } mit seinen Wartungs- und Ausleihdatensätzen.
azerp-delete-detail = Die Dateien werden aus dem Datenordner entfernt. Das kann nicht rückgängig gemacht werden.
azerp-delete = Löschen

## The records' words

azerp-method-straight-line = Linear
azerp-method-declining-balance = Degressiv
azerp-status-in-use = In Gebrauch
azerp-status-in-storage = Eingelagert
azerp-status-checked-out = Ausgeliehen
azerp-status-in-repair = In Reparatur
azerp-status-disposed = Abgegangen
azerp-maintenance-service = Wartung
azerp-maintenance-inspection = Prüfung
azerp-maintenance-repair = Reparatur
azerp-maintenance-calibration = Kalibrierung
azerp-maintenance-other = Sonstiges

## The overview and the schedule

azerp-book-value-today = Buchwert heute
azerp-status = Status
azerp-custodian = Verantwortlich
azerp-next-service = Nächste Wartung
azerp-gain-on-disposal = Abgangsgewinn
azerp-loss-on-disposal = Abgangsverlust
azerp-col-year = Jahr
azerp-col-months = Monate
azerp-col-opening = Anfangswert
azerp-col-depreciation = Abschreibung
azerp-col-accumulated = Kumuliert
azerp-col-closing = Endwert
azerp-nothing-to-depreciate = Nichts abzuschreiben.
azerp-nothing-to-depreciate-detail = Der Restwert ist so hoch wie die Anschaffungskosten, oder die Anlage ging vor ihrem ersten Monat ab.
azerp-schedule = Abschreibungsplan
azerp-chart-year-end-value = Buchwert am Ende jedes Jahres
azerp-book-value = Buchwert

## The reports

azerp-assets-on-books = Anlagen im Bestand
azerp-cost = Anschaffungskosten
azerp-depreciation-year = Abschreibung { $year }
azerp-checked-out = Ausgeliehen
azerp-disposed = Abgegangen
azerp-chart-by-category = Buchwert nach Kategorie
azerp-chart-by-year = Abschreibung nach Jahr
azerp-services-due = Wartungen in den nächsten { $days } Tagen
azerp-service-due = { $number } { $name } - { $when }
azerp-service-overdue = { $number } { $name } - { $when } (überfällig)
azerp-overdue-checkouts = Überfällige Ausleihen
azerp-checkout-overdue =
    { $days ->
        [one] { $number } { $name } - { $custodian } - fällig { $due }, 1 Tag überfällig
       *[other] { $number } { $name } - { $custodian } - fällig { $due }, { $days } Tage überfällig
    }

## The depreciation run

azerp-step = Schritt { $step } von { $steps }: { $title }
azerp-nothing-depreciated = Im Jahr { $year } wird keine Anlage abgeschrieben.
azerp-total-year = Summe { $year }
azerp-run-and-post = Ausführen und buchen

## The import

azerp-choose-csv = CSV-Datei auswählen...
azerp-import-help = Eine CSV-Datei aus einer Tabellenkalkulation oder einem anderen Anlagenverzeichnis: eine Kopfzeile mit den Spaltennamen, eine Anlage pro Zeile (Komma, Semikolon oder Tabulator). Eine Zeile, deren Anlagennummer im Verzeichnis steht, aktualisiert diese Anlage.
azerp-import-summary = { $file }: { $rows } Zeilen - { $new } neu, { $changed } geändert, { $left } ausgelassen
azerp-rows-left-out = Ausgelassene Zeilen
azerp-import-assets =
    { $count ->
        [one] 1 Anlage importieren
       *[other] { $count } Anlagen importieren
    }
azerp-import-dialog = Anlagen aus CSV importieren
azerp-cannot-read = { $path } kann nicht gelesen werden.
azerp-import-field-skip = (überspringen)
azerp-import-field-number = Anlagennummer
azerp-import-field-name = Name
azerp-import-field-category = Kategorie
azerp-import-field-location = Standort
azerp-import-field-serial = Seriennummer
azerp-import-field-acquired = Anschaffungsdatum
azerp-import-field-cost = Anschaffungskosten
azerp-import-field-residual = Restwert
azerp-import-field-life = Nutzungsdauer (Jahre)
azerp-import-field-method = AfA-Methode
azerp-import-field-rate = Degressiver Satz (%)
azerp-import-field-status = Status
azerp-import-field-custodian = Verantwortlich
azerp-import-field-maintenance-months = Wartungsintervall (Monate)
azerp-import-field-disposed = Abgangsdatum
azerp-import-field-disposal-amount = Abgangserlös
azerp-import-field-book-value = Buchwert
azerp-import-field-notes = Notizen
azerp-import-number-twice = Zeile { $row }: Die Anlagennummer { $number } steht zweimal in der Datei; die Zeile wird ausgelassen.
azerp-import-row = Zeile { $row }: { $problems }
azerp-import-acquired-not-a-day = das Anschaffungsdatum „{ $text }“ ist kein Tag
azerp-import-no-acquisition-date = es gibt kein Anschaffungsdatum
azerp-import-no-acquisition-cost = es gibt keine Anschaffungskosten
azerp-import-life-not-years = die Nutzungsdauer „{ $text }“ ist keine Zahl von Jahren
azerp-import-no-useful-life = es gibt keine Nutzungsdauer
azerp-import-unknown-method = die AfA-Methode „{ $text }“ kennt AzERP nicht
azerp-import-rate-not-percent = der degressive Satz „{ $text }“ ist kein Prozentsatz
azerp-import-unknown-status = den Status „{ $text }“ kennt AzERP nicht
azerp-import-interval-not-months = das Wartungsintervall „{ $text }“ ist keine Zahl von Monaten
azerp-import-disposal-not-a-day = das Abgangsdatum „{ $text }“ ist kein Tag

## Problems

azerp-problem-asset-needs-name = Die Anlage braucht einen Namen.
azerp-problem-asset-needs-number = Die Anlage braucht eine Anlagennummer.
azerp-problem-cost-negative = Die Anschaffungskosten können nicht negativ sein.
azerp-problem-residual-range = Der Restwert muss zwischen 0 und den Anschaffungskosten liegen.
azerp-problem-life-at-most = Die Nutzungsdauer beträgt höchstens { $years } Jahre.
azerp-problem-rate-at-most = Der degressive Satz beträgt höchstens 100 %.
azerp-problem-disposal-negative = Der Abgangserlös kann nicht negativ sein.
azerp-problem-disposal-before-acquisition = Das Abgangsdatum liegt vor dem Anschaffungsdatum.
azerp-problem-not-an-amount = „{ $text }“ ist kein Betrag: { $why }
azerp-amount-two-minus-signs = zwei Minuszeichen
azerp-amount-no-digits = keine Ziffern
azerp-amount-only-digits = nur Ziffern, ein Dezimaltrennzeichen und Tausendertrennzeichen
azerp-amount-two-decimal-separators = zwei Dezimaltrennzeichen
azerp-amount-more-than-two-decimals = mehr als zwei Nachkommastellen
azerp-amount-not-grouped-by-three = die Tausender sind nicht in Dreiergruppen
azerp-amount-too-large = zu groß
azerp-problem-not-a-day = „{ $text }“ ist kein Tag
azerp-problem-not-a-whole-number = „{ $text }“ ist keine ganze Zahl
azerp-problem-not-a-method = „{ $text }“ ist keine AfA-Methode
azerp-problem-not-a-rate = „{ $text }“ ist kein Satz
azerp-problem-not-a-status = „{ $text }“ ist kein Status
azerp-problem-not-a-maintenance-kind = „{ $text }“ ist keine Wartungsart
azerp-problem-required = { $label } ist erforderlich.
azerp-problem-at-least = { $label } muss mindestens { $min } sein.
azerp-problem-at-most = { $label } darf höchstens { $max } sein.
azerp-problem-whole-number = { $label } muss eine ganze Zahl sein.
azerp-problem-field-amount = { $label }: { $why }
azerp-problem-field-day = { $label } muss ein Tag sein (JJJJ-MM-TT).
azerp-problem-field-choice = { $label } muss eine der Auswahlmöglichkeiten sein.

## Notices

azerp-no-page = Unter { $path } gibt es keine Seite.
azerp-form-not-savable = Dieses Formular kann noch nicht gespeichert werden.
azerp-record-gone = Der Datensatz ist nicht mehr da.
azerp-asset-gone = Die Anlage ist nicht mehr da.
azerp-saved = Gespeichert.
azerp-number-taken = { $number } ist schon die Nummer von { $name }.
azerp-saved-asset = { $number } { $name } gespeichert.
azerp-maintenance-logged = Wartung von { $number } erfasst.
azerp-checked-out-already = { $number } ist schon ausgeliehen; nimm es zuerst zurück.
azerp-disposed-no-checkout = Eine abgegangene Anlage kann nicht ausgeliehen werden.
azerp-checked-out-to = { $number } ist an { $custodian } ausgeliehen.
azerp-disposed-of = { $number } ist abgegangen.
azerp-not-checked-out = Die Anlage ist nicht ausgeliehen.
azerp-checked-in = { $number } ist zurückgegeben.
azerp-deleted = { $number } gelöscht.
azerp-exported-register = Das Verzeichnis wurde nach { $key } exportiert.
azerp-exported-schedule = Der Plan wurde nach { $key } exportiert.
azerp-run-posted = Der AfA-Lauf { $year } ({ $total }) wurde nach { $key } gebucht.
azerp-files-unreadable =
    { $count ->
        [one] 1 Datei konnte nicht gelesen werden.
       *[other] { $count } Dateien konnten nicht gelesen werden.
    }
azerp-sample-in = Das Beispielverzeichnis ist da: { $count } Anlagen.
azerp-imported = { $new } neue und { $changed } geänderte Anlagen aus { $file } importiert.
azerp-imported-left-out = { $new } neue und { $changed } geänderte Anlagen aus { $file } importiert; { $left } Zeilen ausgelassen.
azerp-not-saved = { $key } konnte nicht gespeichert werden: { $why }
azerp-file-missing = { $key } gibt es nicht.

## Shortcuts

azerp-shortcuts-register = Verzeichnis
azerp-shortcuts-form = Formular
azerp-shortcut-move = Durch die Anlagen gehen
azerp-shortcut-open = Die Anlage öffnen
azerp-shortcut-sort = Nach einer Spalte sortieren, einen zweiten Schlüssel hinzufügen
azerp-shortcut-filter = Die Spalte des Cursors filtern
azerp-shortcut-close-form = Das Formular ohne Speichern schließen
azerp-shortcut-copy = Die ausgewählten Zeilen kopieren

## The views' labels (views/en.json, by the ERP's keys)

azerp-label-actions-add-asset = Neue Anlage
azerp-label-actions-add-category = Neue Kategorie
azerp-label-actions-add-location = Neuer Standort
azerp-label-actions-cancel = Abbrechen
azerp-label-actions-check-in = Zurückgeben
azerp-label-actions-check-out = Ausleihen
azerp-label-actions-delete = Löschen
azerp-label-actions-depreciation-run = AfA-Lauf
azerp-label-actions-dispose = Abgang buchen
azerp-label-actions-edit = Bearbeiten
azerp-label-actions-export-csv = CSV exportieren
azerp-label-actions-export-schedule = Plan exportieren
azerp-label-actions-import-csv = CSV importieren
azerp-label-actions-log-maintenance = Wartung erfassen
azerp-label-actions-run-and-post = Ausführen und buchen
azerp-label-actions-save = Speichern
azerp-label-actions-view = Öffnen
azerp-label-actions-view-asset = Anlage öffnen
azerp-label-depreciation-declining-balance = Degressiv
azerp-label-depreciation-straight-line = Linear
azerp-label-fields-acquisition-cost-label = Anschaffungskosten
azerp-label-fields-acquisition-date-label = Angeschafft
azerp-label-fields-address-label = Adresse
azerp-label-fields-as-of-label = Stand
azerp-label-fields-asset-count-label = Anlagen
azerp-label-fields-asset-name-label = Name
azerp-label-fields-asset-number-label = Anlagen-Nr.
azerp-label-fields-book-value-label = Buchwert
azerp-label-fields-category-label = Kategorie
azerp-label-fields-category-name-label = Kategorie
azerp-label-fields-checked-in-label = Zurückgegeben
azerp-label-fields-checked-out-label = Ausgeliehen
azerp-label-fields-custodian-label = Verantwortlich
azerp-label-fields-declining-rate-percent-label = Degressiver Satz (%)
azerp-label-fields-depreciation-method-label = AfA-Methode
azerp-label-fields-description-label = Beschreibung
azerp-label-fields-disposal-amount-label = Abgangserlös
azerp-label-fields-disposal-date-label = Abgangsdatum
azerp-label-fields-due-date-label = Fällig
azerp-label-fields-location-label = Standort
azerp-label-fields-location-name-label = Standort
azerp-label-fields-maintenance-cost-label = Kosten
azerp-label-fields-maintenance-date-label = Datum
azerp-label-fields-maintenance-interval-months-label = Wartung alle (Monate)
azerp-label-fields-maintenance-kind-label = Art
azerp-label-fields-note-label = Notiz
azerp-label-fields-notes-label = Notizen
azerp-label-fields-performed-by-label = Durchgeführt von
azerp-label-fields-residual-value-label = Restwert
azerp-label-fields-run-date-label = Laufdatum
azerp-label-fields-run-year-label = Jahr
azerp-label-fields-serial-number-label = Seriennummer
azerp-label-fields-status-label = Status
azerp-label-fields-useful-life-years-label = Nutzungsdauer (Jahre)
azerp-label-maintenance-calibration = Kalibrierung
azerp-label-maintenance-inspection = Prüfung
azerp-label-maintenance-other = Sonstiges
azerp-label-maintenance-repair = Reparatur
azerp-label-maintenance-service = Wartung
azerp-label-menu-asset-categories = Kategorien
azerp-label-menu-asset-locations = Standorte
azerp-label-menu-asset-reports = Berichte
azerp-label-menu-checkouts = Ausleihen
azerp-label-menu-fixed-assets = Anlagenverzeichnis
azerp-label-menu-maintenance = Wartung
azerp-label-steps-parameters = Parameter
azerp-label-steps-preview = Vorschau
azerp-label-tab-checkouts = Ausleihen
azerp-label-tab-depreciation-schedule = Abschreibungsplan
azerp-label-tab-maintenance = Wartung
azerp-label-tab-overview = Übersicht
azerp-label-view-asset-categories-title = Anlagenkategorien
azerp-label-view-asset-edit-title = Anlage bearbeiten
azerp-label-view-asset-import-title = Anlagen aus CSV importieren
azerp-label-view-asset-locations-title = Standorte
azerp-label-view-asset-new-title = Neue Anlage
azerp-label-view-asset-reports-title = Anlagenberichte
azerp-label-view-category-edit-title = Kategorie bearbeiten
azerp-label-view-category-new-title = Neue Kategorie
azerp-label-view-checkout-new-title = Ausleihen
azerp-label-view-checkouts-title = Ausleihen
azerp-label-view-depreciation-run-new-title = AfA-Lauf
azerp-label-view-dispose-title = Abgang der Anlage buchen
azerp-label-view-fixed-assets-title = Anlagevermögen
azerp-label-view-location-edit-title = Standort bearbeiten
azerp-label-view-location-new-title = Neuer Standort
azerp-label-view-maintenance-title = Wartungsprotokoll
azerp-label-view-maintenance-new-title = Wartung erfassen

# AzDashboard's words in German (src/l10n.rs): Excel's and Power BI's terms, "du". The column
# titles are the last section.

## The window

azdashboard-about-summary = Eine halbe Million Bestellungen in azuls virtualisierter DataTable - sortiert, gefiltert und bearbeitet, ohne das Fenster zu blockieren - und Diagramme derselben Daten. Teil der Azlin-Apps, gebaut mit azul.
azdashboard-orders = Bestellungen
azdashboard-orders-count = { $rows } Bestellungen
azdashboard-sorting = Wird sortiert...
azdashboard-showing = { $rows } angezeigt
azdashboard-unsorted = Nicht sortiert
azdashboard-sorted-by = Sortiert nach { $keys }
azdashboard-clear-filters = Filter löschen
azdashboard-clear-sort = Sortierung aufheben
azdashboard-generating = { $rows } Bestellungen werden erzeugt...
azdashboard-generating-detail = Jedes Mal dieselben Bestellungen, in einem Hintergrund-Thread.
azdashboard-not-loaded = Die Bestellungen sind noch nicht geladen.
azdashboard-copied-rows =
    { $count ->
        [one] 1 Zeile kopiert
       *[other] { $rows } Zeilen kopiert
    }

## Editing

azdashboard-no-such-order = Diese Bestellung gibt es nicht.
azdashboard-not-a-date = „{ $text }“ ist kein Datum (JJJJ-MM-TT).
azdashboard-priority-one-of = Die Priorität ist eine von { $choices }.
azdashboard-quantity-range = Die Menge ist eine ganze Zahl von 1 bis 999.
azdashboard-unit-price-range = Der Stückpreis ist ein Betrag von 0,01 bis 99.999,99.
azdashboard-discount-range = Der Rabatt ist ein ganzer Prozentsatz von 0 bis 90.
azdashboard-returned-yes-no = Retourniert ist Yes oder No.
azdashboard-rating-range = Die Bewertung ist 1, 2, 3, 4 oder 5.
azdashboard-not-editable = { $column } kann nicht bearbeitet werden.
azdashboard-this-column = Diese Spalte

## The charts

azdashboard-other = Sonstige
azdashboard-picked-row = Zeile { $row }: { $column } { $value }
azdashboard-picked-bar = { $name }: { $column } { $total } insgesamt
azdashboard-charted =
    { $count ->
        [one] 1 Zeile im Diagramm. Klicke auf einen Punkt oder einen Balken (oder geh mit Tab zu einem Diagramm und drücke die Eingabetaste), um ihn auszuwählen.
       *[other] { $count } Zeilen im Diagramm. Klicke auf einen Punkt oder einen Balken (oder geh mit Tab zu einem Diagramm und drücke die Eingabetaste), um ihn auszuwählen.
    }
azdashboard-by-row = { $column } nach Zeile
azdashboard-row-axis = Zeile (Tabellenreihenfolge)
azdashboard-by-category = { $column } nach { $category }
azdashboard-total-of = { $column } gesamt

## Shortcuts

azdashboard-group-table = Tabelle
azdashboard-group-edit = Bearbeiten
azdashboard-group-sort = Sortieren
azdashboard-group-filter = Filtern
azdashboard-shortcut-move = Den Zellcursor bewegen
azdashboard-shortcut-extend = Die Zeilenauswahl erweitern
azdashboard-shortcut-screen = Um eine Bildschirmseite Zeilen blättern
azdashboard-shortcut-first-last-column = Die erste / letzte Spalte
azdashboard-shortcut-first-last-row = Die erste / letzte Zeile
azdashboard-shortcut-select-all = Alle angezeigten Zeilen auswählen
azdashboard-shortcut-copy = Die ausgewählten Zeilen kopieren
azdashboard-shortcut-edit = Die Zelle bearbeiten
azdashboard-shortcut-keep = Die Bearbeitung übernehmen (Tab geht nach rechts)
azdashboard-shortcut-cancel = Die Bearbeitung abbrechen
azdashboard-shortcut-sort = Nach einer Spalte sortieren, einen zweiten Schlüssel hinzufügen
azdashboard-shortcut-filter = Die Spalte des Cursors filtern

## The column titles (the data set's English names)
azdashboard-col-order = Bestellung
azdashboard-col-order-date = Bestelldatum
azdashboard-col-ship-date = Versanddatum
azdashboard-col-region = Region
azdashboard-col-country = Land
azdashboard-col-city = Stadt
azdashboard-col-customer = Kunde
azdashboard-col-segment = Segment
azdashboard-col-category = Kategorie
azdashboard-col-sub-category = Unterkategorie
azdashboard-col-product = Produkt
azdashboard-col-ship-mode = Versandart
azdashboard-col-priority = Priorität
azdashboard-col-quantity = Menge
azdashboard-col-unit-price = Stückpreis
azdashboard-col-discount = Rabatt %
azdashboard-col-sales = Umsatz
azdashboard-col-cost = Kosten
azdashboard-col-shipping = Versand
azdashboard-col-profit = Gewinn
azdashboard-col-margin = Marge %
azdashboard-col-days-to-ship = Tage bis Versand
azdashboard-col-returned = Retourniert
azdashboard-col-rating = Bewertung
azdashboard-col-sales-rep = Vertriebsmitarbeiter

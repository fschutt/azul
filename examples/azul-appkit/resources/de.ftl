# azul-appkits eigene Texte: die Einstellungsseite aller Azlin-Apps, das Info-Fenster.
# Dieselben Schlüssel wie resources/en.ftl.

## Schaltflächen
kit-button-ok = OK
kit-button-cancel = Abbrechen

## Die Kategorien der Einstellungsseite und die Zeile darüber
kit-category-general = Allgemein
kit-category-data = Daten
kit-category-shortcuts = Tastenkombinationen
kit-category-about = Info
kit-header-app = Optionen für „{ $category }“ in { $app }.
kit-header-general = Allgemeine Optionen für die Arbeit mit { $app }.
kit-header-data = Wo { $app } deine Daten speichert.
kit-header-shortcuts = Die Tastenkombinationen von { $app }.
kit-header-about = Version, Lizenz und Datenordner von { $app }.

## Allgemein: Darstellung und Sprache
kit-section-appearance = Darstellung
kit-general-theme = Design
kit-general-theme-pinned = { $app } verwendet immer { $theme }. Der Modus darunter gilt für alle Azlin-Apps.
kit-general-stone = Stein
kit-general-mode = Modus
kit-general-language = Sprache
kit-general-switch-overrides = Ein Schalter --theme, --mode oder --language setzt diese Einstellungen bis zum Neustart der App außer Kraft.
kit-general-az-theme = AZ_THEME={ $value } legt das Design jeder App fest, die damit gestartet wird.
kit-mode-system = System
kit-mode-light = Hell
kit-mode-dark = Dunkel
kit-language-system = System
kit-language-english = English
kit-language-german = Deutsch
kit-theme-flat = Flat
kit-theme-flora = Flora
kit-theme-flora-green = Flora, Grün
kit-theme-flora-red = Flora, Rot
kit-theme-flora-purple = Flora, Lila
kit-theme-flora-gold = Flora, Gold
kit-theme-flora-rose = Flora, Rosé
kit-stone-blue = Blau
kit-stone-green = Grün
kit-stone-red = Rot
kit-stone-purple = Lila
kit-stone-gold = Gold
kit-stone-rose = Rosé

## Daten
kit-section-data = Deine Daten
kit-data-folder = Datenordner
kit-data-note = Deine Daten sind einfache Dateien in diesem Ordner, ein Ordner pro App. Ein S3-Laufwerk kann später an die Stelle des Ordners treten, ohne dass sich an ihnen etwas ändert.

## Die Einstellungsdatei
kit-settings-read-partly = Die Einstellungsdatei konnte nicht vollständig gelesen werden ({ $problem }).
kit-settings-unreadable = Die Einstellungsdatei konnte nicht gelesen werden: { $detail }
kit-settings-not-saved = Die Einstellungen konnten nicht gespeichert werden: { $detail }

## Info
kit-about-title = Info zu { $app }
kit-about-open = Info zu { $app }…
kit-about-version = Version
kit-about-license = Lizenz
kit-about-data-folder = Datenordner
kit-about-built-with = Erstellt mit
kit-about-icons = Symbole

## Tastenkombinationen: die eigenen des Kits und die Namen der Tasten
kit-shortcut-group-window = Fenster
kit-shortcut-settings = Einstellungen öffnen
kit-shortcut-shortcuts = Tastenkombinationen anzeigen
kit-shortcut-cancel-settings = Einstellungen abbrechen (schließen, Änderungen verwerfen)
kit-key-ctrl = Strg
kit-key-cmd = Befehl
kit-key-shift = Umschalt
kit-key-alt = Alt
kit-key-option = Wahl
kit-key-escape = Esc
kit-key-enter = Eingabe
kit-key-space = Leertaste
kit-key-tab = Tab
kit-key-delete = Entf
kit-key-backspace = Rücktaste
kit-key-insert = Einfg
kit-key-home = Pos1
kit-key-end = Ende
kit-key-pageup = Bild auf
kit-key-pagedown = Bild ab
kit-key-up = Nach oben
kit-key-down = Nach unten
kit-key-left = Nach links
kit-key-right = Nach rechts

## Numbers

# The separator of a big number's groups of three digits.
kit-number-group-separator = .
# The mark between a number's whole part and its decimals.
kit-number-decimal-separator = ,
# An amount of money: the currency's code (EUR) and the amount with its decimals.
kit-money = { $amount } { $currency }

## Dates: a list's date groups, the weekdays (long and short) and the months (azul-pim
## names them by these ids)

kit-date-today = Heute
kit-date-yesterday = Gestern
kit-date-last-week = Letzte Woche
kit-date-two-weeks-ago = Vor zwei Wochen
kit-date-three-weeks-ago = Vor drei Wochen
kit-date-last-month = Letzter Monat
kit-date-older = Älter
kit-weekday-monday = Montag
kit-weekday-tuesday = Dienstag
kit-weekday-wednesday = Mittwoch
kit-weekday-thursday = Donnerstag
kit-weekday-friday = Freitag
kit-weekday-saturday = Samstag
kit-weekday-sunday = Sonntag
kit-weekday-short-mon = Mo
kit-weekday-short-tue = Di
kit-weekday-short-wed = Mi
kit-weekday-short-thu = Do
kit-weekday-short-fri = Fr
kit-weekday-short-sat = Sa
kit-weekday-short-sun = So
kit-month-january = Januar
kit-month-february = Februar
kit-month-march = März
kit-month-april = April
kit-month-may = Mai
kit-month-june = Juni
kit-month-july = Juli
kit-month-august = August
kit-month-september = September
kit-month-october = Oktober
kit-month-november = November
kit-month-december = Dezember
kit-month-short-jan = Jan.
kit-month-short-feb = Feb.
kit-month-short-mar = März
kit-month-short-apr = Apr.
kit-month-short-may = Mai
kit-month-short-jun = Juni
kit-month-short-jul = Juli
kit-month-short-aug = Aug.
kit-month-short-sep = Sept.
kit-month-short-oct = Okt.
kit-month-short-nov = Nov.
kit-month-short-dec = Dez.
# The date styles of l10n::date_text: the names are the words above, the numbers as they are.
kit-date-style-day-long = { $weekday }, { $day }. { $month } { $year }
kit-date-style-month-year = { $month } { $year }
kit-date-style-date = { $day }. { $month } { $year }
kit-date-style-day-month = { $day }. { $month }
kit-date-style-weekday-day-month = { $weekday }, { $day }. { $month }
kit-date-style-weekday = { $weekday }
kit-date-style-short-weekday-day = { $wd }. { $day }.
kit-date-style-short-date = { $wd }. { $day }. { $mon }
kit-date-style-day-short-month = { $day }. { $mon }
kit-date-style-day-only = { $day }.

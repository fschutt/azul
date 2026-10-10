# azul-appkit's own words: the settings page every Azlin app shares, the About box.
# One message per key the source names (`kit-...`); resources/de.ftl has the same keys.

## Buttons
kit-button-ok = OK
kit-button-cancel = Cancel

## The categories of the settings page and the line over each
kit-category-general = General
kit-category-data = Data
kit-category-shortcuts = Shortcuts
kit-category-about = About
kit-header-app = { $category } options for working with { $app }.
kit-header-general = General options for working with { $app }.
kit-header-data = Where { $app } keeps your data.
kit-header-shortcuts = The keyboard shortcuts of { $app }.
kit-header-about = The version, the licence and the data folder of { $app }.

## General: the appearance and the language
kit-section-appearance = Appearance
kit-general-theme = Theme
kit-general-theme-pinned = { $app } is always set in { $theme }. The mode below is shared with every Azlin app.
kit-general-stone = Stone
kit-general-mode = Mode
kit-general-language = Language
kit-general-switch-overrides = A --theme, --mode or --language switch overrides these settings until the app restarts.
kit-general-az-theme = AZ_THEME={ $value } overrides the theme of every app it runs.
kit-mode-system = System
kit-mode-light = Light
kit-mode-dark = Dark
kit-language-system = System
kit-language-english = English
kit-language-german = Deutsch
kit-theme-flat = Flat
kit-theme-flora = Flora
kit-theme-flora-green = Flora, green
kit-theme-flora-red = Flora, red
kit-theme-flora-purple = Flora, purple
kit-theme-flora-gold = Flora, gold
kit-theme-flora-rose = Flora, rose
kit-stone-blue = Blue
kit-stone-green = Green
kit-stone-red = Red
kit-stone-purple = Purple
kit-stone-gold = Gold
kit-stone-rose = Rose

## Data
kit-section-data = Your data
kit-data-folder = Data folder
kit-data-note = Your data are plain files in this folder, one folder per app. An S3 drive can take the place of the folder later without changing them.

## The settings file
kit-settings-read-partly = The settings file could not be read fully ({ $problem }).
kit-settings-unreadable = The settings file could not be read: { $detail }
kit-settings-not-saved = The settings could not be saved: { $detail }

## About
kit-about-title = About { $app }
kit-about-open = About { $app }…
kit-about-version = Version
kit-about-license = License
kit-about-data-folder = Data folder
kit-about-built-with = Built with
kit-about-icons = Icons

## Shortcuts: the kit's own, and the names of the keys
kit-shortcut-group-window = Window
kit-shortcut-settings = Open the settings
kit-shortcut-shortcuts = Show the keyboard shortcuts
kit-shortcut-cancel-settings = Cancel the settings (close them, the changes undone)
kit-key-ctrl = Ctrl
kit-key-cmd = Cmd
kit-key-shift = Shift
kit-key-alt = Alt
kit-key-option = Option
kit-key-escape = Escape
kit-key-enter = Enter
kit-key-space = Space
kit-key-tab = Tab
kit-key-delete = Delete
kit-key-backspace = Backspace
kit-key-insert = Insert
kit-key-home = Home
kit-key-end = End
kit-key-pageup = Page Up
kit-key-pagedown = Page Down
kit-key-up = Up
kit-key-down = Down
kit-key-left = Left
kit-key-right = Right

## Numbers

# The separator of a big number's groups of three digits.
kit-number-group-separator = ,
# The mark between a number's whole part and its decimals.
kit-number-decimal-separator = .
# An amount of money: the currency's code (EUR) and the amount with its decimals.
kit-money = { $currency } { $amount }

## Dates: a list's date groups, the weekdays (long and short) and the months (azul-pim
## names them by these ids)

kit-date-today = Today
kit-date-yesterday = Yesterday
kit-date-last-week = Last Week
kit-date-two-weeks-ago = Two Weeks Ago
kit-date-three-weeks-ago = Three Weeks Ago
kit-date-last-month = Last Month
kit-date-older = Older
kit-weekday-monday = Monday
kit-weekday-tuesday = Tuesday
kit-weekday-wednesday = Wednesday
kit-weekday-thursday = Thursday
kit-weekday-friday = Friday
kit-weekday-saturday = Saturday
kit-weekday-sunday = Sunday
kit-weekday-short-mon = Mon
kit-weekday-short-tue = Tue
kit-weekday-short-wed = Wed
kit-weekday-short-thu = Thu
kit-weekday-short-fri = Fri
kit-weekday-short-sat = Sat
kit-weekday-short-sun = Sun
kit-month-january = January
kit-month-february = February
kit-month-march = March
kit-month-april = April
kit-month-may = May
kit-month-june = June
kit-month-july = July
kit-month-august = August
kit-month-september = September
kit-month-october = October
kit-month-november = November
kit-month-december = December
kit-month-short-jan = Jan
kit-month-short-feb = Feb
kit-month-short-mar = Mar
kit-month-short-apr = Apr
kit-month-short-may = May
kit-month-short-jun = Jun
kit-month-short-jul = Jul
kit-month-short-aug = Aug
kit-month-short-sep = Sep
kit-month-short-oct = Oct
kit-month-short-nov = Nov
kit-month-short-dec = Dec
# The date styles of l10n::date_text: the names are the words above, the numbers as they are.
kit-date-style-day-long = { $weekday }, { $day } { $month } { $year }
kit-date-style-month-year = { $month } { $year }
kit-date-style-date = { $day } { $month } { $year }
kit-date-style-day-month = { $day } { $month }
kit-date-style-weekday-day-month = { $weekday } { $day } { $month }
kit-date-style-weekday = { $weekday }
kit-date-style-short-weekday-day = { $wd } { $day }
kit-date-style-short-date = { $wd } { $day } { $mon }
kit-date-style-day-short-month = { $day } { $mon }
kit-date-style-day-only = { $day }
# A list: l10n::and_list.
kit-list-and = { $first } and { $last }
# What a repeat rule does (azul-pim's Rule::description, l10n::t_said): "Every 2 weeks on Monday
# and Friday, 10 times".
kit-rule-every-weekday = Every weekday
kit-rule-daily = { $n ->
    [one] Daily
   *[other] Every { $n } days
 }
kit-rule-weekly = { $n ->
    [one] Weekly
   *[other] Every { $n } weeks
 }
kit-rule-monthly = { $n ->
    [one] Monthly
   *[other] Every { $n } months
 }
kit-rule-yearly = { $n ->
    [one] Yearly
   *[other] Every { $n } years
 }
kit-rule-on = { $every } on { $on }
kit-rule-nth-weekday = the { $nth } { $day }
kit-rule-ordinal = { $n ->
    [1] first
    [2] second
    [3] third
    [4] fourth
    [5] fifth
   *[other] { $n }th
 }
kit-rule-ordinal-last = { $n ->
    [1] last
    [2] second to last
   *[other] { $n }th to last
 }
kit-rule-month-day = day { $day }
kit-rule-last-day = the last day
kit-rule-day-from-end = the { $nth } day from the end
kit-rule-days-of-month = { $days } of { $month }
kit-rule-day-of-months = { $day } { $months }
kit-rule-times = { $count ->
    [one] { $rule }, once
   *[other] { $rule }, { $count } times
 }
kit-rule-until = { $rule }, until { $date }

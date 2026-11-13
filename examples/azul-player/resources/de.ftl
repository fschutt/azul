# AzPlayer's words in German (src/l10n.rs). Every key the source names is here and in en.ftl.

## The names AzPlayer keeps in English (crate::l10n::word: azplayer-word-<name>)

azplayer-word-extras = extras
azplayer-word-pictures-videos = bilder + videos
azplayer-word-music = musik
azplayer-word-movies = filme
azplayer-word-tv = tv
azplayer-word-tasks = aufgaben
azplayer-word-extras-library = extras-bibliothek
azplayer-word-recently-played = zuletzt wiedergegeben
azplayer-word-picture-library = bildbibliothek
azplayer-word-play-favorites = favoriten wiedergeben
azplayer-word-video-library = videobibliothek
azplayer-word-music-library = musikbibliothek
azplayer-word-play-all = alle wiedergeben
azplayer-word-radio = radio
azplayer-word-search = suchen
azplayer-word-movie-library = filmbibliothek
azplayer-word-open-a-file = datei öffnen
azplayer-word-open-an-address = adresse öffnen
azplayer-word-recorded-tv = aufgezeichnetes tv
azplayer-word-live-tv-setup = live-tv einrichten
azplayer-word-settings = einstellungen
azplayer-word-media-only = nur medien
azplayer-word-refresh-libraries = bibliotheken aktualisieren
azplayer-word-about = info
azplayer-word-close = schließen
azplayer-word-pictures = bilder
azplayer-word-videos = videos
azplayer-word-albums = alben
azplayer-word-artists = interpreten
azplayer-word-genres = genres
azplayer-word-songs = titel
azplayer-word-folders = ordner
azplayer-word-date-taken = aufnahmedatum
azplayer-word-date-recorded = aufzeichnungsdatum
azplayer-word-date-added = hinzugefügt am
azplayer-word-date = datum
azplayer-word-title = titel
azplayer-word-play-slide-show = diashow wiedergeben
azplayer-word-general = allgemein
azplayer-word-library-setup = bibliotheken einrichten
azplayer-word-start-up-window = start und fenster
azplayer-word-save = speichern
azplayer-word-cancel = abbrechen
azplayer-word-ok = ok

## AzPlayer

azplayer-blurb-general = Die Uhr, die Zurück-Schaltfläche und die Steuerelemente, das Aussehen, die Sprache.
azplayer-blurb-pictures = Was die Bildbibliothek zuerst zeigt; die Diashow.
azplayer-blurb-music = Was die Musikbibliothek zuerst zeigt; alle wiedergeben.
azplayer-blurb-videos = Was die Videobibliothek zuerst zeigt; fortsetzen, springen.
azplayer-blurb-tv = Aufgezeichnetes TV; Live-TV.
azplayer-blurb-library = Die Ordner, aus denen jede Bibliothek gelesen wird.
azplayer-blurb-startup = Womit AzPlayer öffnet; nur Medien.
azplayer-blurb-about = AzPlayer, seine Daten, die Tasten.
azplayer-no-folder = kein Ordner
azplayer-setting-pointer = der zeiger
azplayer-setting-hide-chrome = zurück-schaltfläche und steuerelemente ausblenden, wenn der zeiger ruht
azplayer-setting-top-of-window = der obere fensterrand
azplayer-setting-show-clock = die uhr anzeigen
azplayer-setting-look = das aussehen
azplayer-setting-look-note = AzPlayer ist immer im tiefen Blau des Media Centers. Die anderen Azlin-Apps zeigen { $theme }, { $mode } – ihr Aussehen wählst du in jeder von ihnen.
azplayer-setting-language = die sprache
azplayer-language-system = die sprache des systems
azplayer-setting-pictures-open-on = die bildbibliothek öffnet mit
azplayer-setting-slide-for = diashow: jedes bild steht
azplayer-seconds = { $count ->
    [one] 1 sekunde
   *[other] { $count } sekunden
 }
azplayer-setting-ken-burns = schwenken und zoomen (sonst eine einfache überblendung)
azplayer-setting-music-open-on = die musikbibliothek öffnet mit
azplayer-setting-shuffle-all = die titel mischen (sonst album für album)
azplayer-setting-videos-open-on = die videobibliothek öffnet mit
azplayer-setting-playing = wiedergabe
azplayer-setting-resume = ein video dort fortsetzen, wo es angehalten wurde
azplayer-setting-skip-back = zurückspringen
azplayer-setting-skip-forward = vorspringen
azplayer-setting-a-minute = eine minute
azplayer-setting-live-tv = live-tv
azplayer-setting-no-tuner = Dieser Computer hat keinen TV-Tuner: Live-TV braucht einen. Aufgezeichnetes TV wird aus den Ordnern unter „bibliotheken einrichten“ gelesen.
azplayer-setting-tv-open-on = aufgezeichnetes tv öffnet mit
azplayer-setting-folders-of = ordner für { $library }
azplayer-setting-add-folder = einen ordner zu { $library } hinzufügen
azplayer-setting-libraries = die bibliotheken
azplayer-setting-rescan = die ordner erneut durchsuchen
azplayer-setting-folders-note = Wenn du einen Ordner entfernst, bleiben seine Dateien, wo sie sind. Fotomediatheken und versteckte Dateien werden ausgelassen.
azplayer-setting-opens-on = azplayer öffnet mit
azplayer-setting-start-screen = der startbildschirm
azplayer-setting-window = das fenster
azplayer-setting-start-fullscreen = in „nur medien“ öffnen (vollbild)
azplayer-setting-about-title = azplayer { $version }
azplayer-setting-about-note = Ein Media Center im Aussehen des Windows Media Centers: Musik, Bilder und Diashows, Videos und Filme. Teil der Azlin-Apps, gebaut mit azul. MIT-Lizenz.
azplayer-setting-about-counts = { $counts }. Die Daten liegen in { $data }.
azplayer-count-songs = { $count ->
    [one] 1 titel
   *[other] { $count } titel
 }
azplayer-count-pictures = { $count ->
    [one] 1 bild
   *[other] { $count } bilder
 }
azplayer-count-videos = { $count ->
    [one] 1 video
   *[other] { $count } videos
 }
azplayer-count-recordings = { $count ->
    [one] 1 aufzeichnung
   *[other] { $count } aufzeichnungen
 }
azplayer-setting-keys = die tasten
azplayer-setting-keys-note = Die Pfeiltasten bewegen, Eingabe öffnet, Rücktaste oder Escape geht zurück, Pos1 ist der Startbildschirm. Tab geht zur Zurück-Schaltfläche, zu den Steuerelementen und zu dem, was gerade läuft.
azplayer-setting-keys-note-more = Strg+D, I oder die Menütaste: weitere Infos. Leertaste spielt und pausiert, M schaltet stumm, F ist Vollbild, Strg+, die Einstellungen.
azplayer-unknown-artist = unbekannter interpret
azplayer-unknown-album = unbekanntes album
azplayer-unknown-genre = unbekanntes genre
azplayer-in-no-folder = (kein ordner)
azplayer-no-date = kein datum
azplayer-count-items = { $count ->
    [one] 1 element
   *[other] { $count } elemente
 }
azplayer-never-extras = Auf diesem Computer sind keine Extras installiert.
azplayer-never-radio = Dieser Computer hat keinen Radio-Tuner.
azplayer-never-tv = Dieser Computer hat keinen TV-Tuner.
azplayer-no-music-in = In { $folder } ist keine Musik.
azplayer-no-pictures-in = In { $folder } sind keine Bilder.
azplayer-address-hint = Eine Adresse beginnt mit http:// oder https:// und nennt ein MP4- oder MOV-Video.
azplayer-shortcuts-moving = Bewegen
azplayer-shortcuts-playback = Wiedergabe
azplayer-shortcuts-window = Fenster
azplayer-shortcuts-file = Datei
azplayer-shortcut-move-focus = Den Fokus bewegen
azplayer-shortcut-open-play = Öffnen, wiedergeben
azplayer-shortcut-back = Zurück
azplayer-shortcut-start-screen = Der Startbildschirm
azplayer-shortcut-scroll = Die Leiste oder die Galerie blättern
azplayer-shortcut-play-pause = Wiedergabe / Pause
azplayer-shortcut-skip = Zurück- / vorspringen (wie weit: Einstellungen, Videos)
azplayer-shortcut-minute = Eine Minute zurück / vor
azplayer-shortcut-previous-next = Vorheriger / nächster Titel oder vorheriges / nächstes Bild
azplayer-shortcut-volume = Lauter / leiser (während etwas läuft)
azplayer-shortcut-mute = Stummschalten
azplayer-shortcut-fullscreen = Vollbild (nur Medien)
azplayer-shortcut-leave-fullscreen = Vollbild beenden
azplayer-shortcut-open-video = Ein Video öffnen
azplayer-about-summary = Ein Media Center im Aussehen des Windows Media Centers: Musik, Bilder und Diashows, Videos und Filme. Videos laufen mit azuls VideoWidget, ihr Ton mit azuls AudioPlayer, beide zugleich gestartet; zuletzt gespielte Dateien laufen dort weiter, wo du aufgehört hast. Teil der Azlin-Apps, gebaut mit azul.
azplayer-added-to-queue = Zur Warteschlange hinzugefügt.
azplayer-looking-again = Die Ordner werden erneut durchsucht.
azplayer-last-song = Das war der letzte Titel.
azplayer-nothing-playing = Es läuft nichts.
azplayer-no-music = Im Musikordner ist keine Musik.
azplayer-osd-stopped = Gestoppt
azplayer-osd-shuffled = Gemischt
azplayer-video-files = Video
azplayer-pause = Pause
azplayer-play = Wiedergabe
azplayer-sound-on = Ton an
azplayer-mute = Stumm
azplayer-stop-music = Die Musik stoppen
azplayer-slide-music = Musik zur Diashow
azplayer-pause-slide-show = Die Diashow anhalten
azplayer-play-slide-show = Die Diashow wiedergeben
azplayer-previous-picture = Vorheriges Bild
azplayer-next-picture = Nächstes Bild
azplayer-shuffle = Zufällige Wiedergabe
azplayer-previous-song = Vorheriger Titel
azplayer-skip-back = Zurückspringen
azplayer-skip-forward = Vorspringen
azplayer-next-song = Nächster Titel
azplayer-stop = Stopp
azplayer-from-the-start = Von vorn
azplayer-volume-down = Leiser
azplayer-volume-up = Lauter
azplayer-osd-muted = Stumm
azplayer-osd-volume = Lautstärke { $percent } %
azplayer-about-title = über azplayer
azplayer-delete-title = diese datei löschen?
azplayer-delete-note = Sie wird aus { $folder } gelöscht und lässt sich nicht wiederherstellen.
azplayer-delete-yes = ja, löschen
azplayer-choice-no = nein
azplayer-remove-folder-title = diesen ordner aus { $library } entfernen?
azplayer-remove-folder-note = AzPlayer zeigt seinen Inhalt nicht mehr an; die Dateien bleiben, wo sie sind.
azplayer-remove-folder-yes = ja, entfernen
azplayer-paused-title = die musik ist angehalten
azplayer-paused-note = Stoppen, oder für später angehalten lassen (sie bleibt unten links)?
azplayer-paused-stop = die musik stoppen
azplayer-paused-keep = angehalten lassen
azplayer-video-failed-title = dieses video läuft hier nicht
azplayer-address-note = Ein MP4- oder MOV-Video (H.264) auf einem Webserver läuft, während es geladen wird; der Ton beginnt mit dem Bild.
azplayer-choice-play = wiedergeben
azplayer-address-sample = ausprobieren: Big Buck Bunny (10 s, 360p)
azplayer-choice-open = öffnen
azplayer-choice-queue = zur warteschlange hinzufügen
azplayer-choice-view = anzeigen
azplayer-choice-slide-show-from-here = diashow ab hier wiedergeben
azplayer-choice-delete = löschen
azplayer-choice-from-start = von vorn wiedergeben
azplayer-choice-forget = aus „zuletzt wiedergegeben“ entfernen
azplayer-now-playing = aktuelle wiedergabe
azplayer-paused = angehalten
azplayer-choice-pause = pause
azplayer-choice-stop = stopp
azplayer-about-license = { $license }-Lizenz. Die Daten liegen in { $data }.
azplayer-forgotten = Aus „zuletzt wiedergegeben“ entfernt (die Datei bleibt).
azplayer-delete-only-library = Hier wird nur eine Datei einer Bibliothek gelöscht.
azplayer-delete-playing = Sie läuft gerade oder ist in der Warteschlange: stoppe sie zuerst.
azplayer-deleted = { $title } wurde gelöscht.
azplayer-delete-failed = die datei wurde nicht gelöscht
azplayer-opening = { $title } wird geöffnet …
azplayer-not-available = { $item } (nicht verfügbar: { $why })
azplayer-count-artists = { $count ->
    [one] 1 interpret
   *[other] { $count } interpreten
 }
azplayer-count-genres = { $count ->
    [one] 1 genre
   *[other] { $count } genres
 }
azplayer-count-albums = { $count ->
    [one] 1 album
   *[other] { $count } alben
 }
azplayer-count-files-played = { $count ->
    [one] 1 datei wiedergegeben
   *[other] { $count } dateien wiedergegeben
 }
azplayer-count-found = { $count } gefunden
azplayer-status-scanning = der ordner wird durchsucht …
azplayer-status-cut = die ersten { $count } dateien des ordners
azplayer-search-empty = Tippe, um Musik, Bilder und Videos zu durchsuchen.
azplayer-search-none = Nichts gefunden für „{ $query }“.
azplayer-kind-movies = Filme (Videos ab 40 Minuten)
azplayer-kind-music = Musik
azplayer-kind-pictures = Bilder
azplayer-kind-videos = Videos
azplayer-kind-tv = aufgezeichnetes TV
azplayer-empty-looking = Suche nach { $kind } in { $folder } …
azplayer-empty-missing = AzPlayer liest { $kind } aus { $folder }, aber den Ordner gibt es nicht. Füge unter einstellungen, bibliotheken einrichten einen anderen Ordner hinzu.
azplayer-empty-music = In { $folder } ist noch keine Musik. Titel, die dort gespeichert sind (MP3, AAC, FLAC, Ogg, WAV), erscheinen hier.
azplayer-empty-pictures = In { $folder } sind noch keine Bilder. Fotos, die dort gespeichert sind (JPEG, PNG, GIF, WebP), erscheinen hier.
azplayer-empty-videos = In { $folder } sind noch keine { $kind }. MP4- und MOV-Videos (H.264), die dort gespeichert sind, erscheinen hier.
azplayer-empty-tv = Es gibt kein aufgezeichnetes TV. Aufzeichnungen, die als MP4 in { $folder } gespeichert sind, erscheinen hier.
azplayer-resume-at = fortsetzen bei { $time }
azplayer-mp4-or-mov = MP4 oder MOV
azplayer-search-placeholder = tippen, um musik, bilder und videos zu durchsuchen
azplayer-search = Suchen
azplayer-close = Schließen
azplayer-more-info = weitere infos
azplayer-address-name = Die Adresse des Videos
azplayer-enter-removes = Eingabe entfernt ihn
azplayer-nothing-playing-hint = Es läuft nichts. Wähle auf dem Startbildschirm musikbibliothek oder alle wiedergeben.
azplayer-position-song = Position im Titel
azplayer-position-video = Position im Video
azplayer-up-next = als nächstes
azplayer-queue-end = das ende der warteschlange
azplayer-loading-picture = das bild wird geladen …
azplayer-picture-of = { $index } von { $total }
azplayer-menu-open-file = Datei öffnen …
azplayer-menu-open-recent = Zuletzt verwendet
azplayer-menu-start = Start
azplayer-menu-refresh = Bibliotheken aktualisieren
azplayer-menu-settings = Einstellungen …
azplayer-menu-media = Medien
azplayer-menu-play-pause = Wiedergabe / Pause
azplayer-menu-previous = Zurück
azplayer-menu-next = Weiter
azplayer-menu-back-seconds = { $count ->
    [one] 1 Sekunde zurück
   *[other] { $count } Sekunden zurück
 }
azplayer-menu-forward-seconds = { $count ->
    [one] 1 Sekunde vor
   *[other] { $count } Sekunden vor
 }
azplayer-menu-audio = Audio
azplayer-menu-sound-on = Ton an
azplayer-menu-mute = Stummschalten
azplayer-menu-volume-up = Lauter
azplayer-menu-volume-down = Leiser
azplayer-menu-view = Darstellung
azplayer-music = Musik
azplayer-menu-pictures = Bilder
azplayer-menu-videos = Videos
azplayer-menu-movies = Filme
azplayer-menu-recorded-tv = Aufgezeichnetes TV
azplayer-menu-recent = Zuletzt wiedergegeben
azplayer-menu-leave-fullscreen = Vollbild beenden
azplayer-menu-fullscreen = Vollbild
azplayer-pick-folder = Einen Ordner zu { $library } hinzufügen

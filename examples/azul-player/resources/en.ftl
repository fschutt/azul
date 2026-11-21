# AzPlayer's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The names AzPlayer keeps in English (crate::l10n::word: azplayer-word-<name>)

azplayer-word-extras = extras
azplayer-word-pictures-videos = pictures + videos
azplayer-word-music = music
azplayer-word-movies = movies
azplayer-word-tv = tv
azplayer-word-tasks = tasks
azplayer-word-extras-library = extras library
azplayer-word-recently-played = recently played
azplayer-word-picture-library = picture library
azplayer-word-play-favorites = play favorites
azplayer-word-video-library = video library
azplayer-word-music-library = music library
azplayer-word-play-all = play all
azplayer-word-radio = radio
azplayer-word-search = search
azplayer-word-movie-library = movie library
azplayer-word-open-a-file = open a file
azplayer-word-open-an-address = open an address
azplayer-word-recorded-tv = recorded tv
azplayer-word-live-tv-setup = live tv setup
azplayer-word-settings = settings
azplayer-word-media-only = media only
azplayer-word-refresh-libraries = refresh libraries
azplayer-word-about = about
azplayer-word-close = close
azplayer-word-pictures = pictures
azplayer-word-videos = videos
azplayer-word-albums = albums
azplayer-word-artists = artists
azplayer-word-genres = genres
azplayer-word-songs = songs
azplayer-word-folders = folders
azplayer-word-date-taken = date taken
azplayer-word-date-recorded = date recorded
azplayer-word-date-added = date added
azplayer-word-date = date
azplayer-word-title = title
azplayer-word-play-slide-show = play slide show
azplayer-word-general = general
azplayer-word-library-setup = library setup
azplayer-word-start-up-window = start-up & window
azplayer-word-save = save
azplayer-word-cancel = cancel
azplayer-word-ok = ok

## AzPlayer

azplayer-blurb-general = The clock, the back button and the controls, the look, the language.
azplayer-blurb-pictures = What the picture library shows first; the slide show.
azplayer-blurb-music = What the music library shows first; play all.
azplayer-blurb-videos = What the video library shows first; resuming, skipping.
azplayer-blurb-tv = Recorded TV; live TV.
azplayer-blurb-library = The folders each library is read from.
azplayer-blurb-startup = What AzPlayer opens on; media only.
azplayer-blurb-about = AzPlayer, its data, the keys.
azplayer-no-folder = no folder
azplayer-setting-pointer = the pointer
azplayer-setting-hide-chrome = hide the back button and the controls when the pointer rests
azplayer-setting-top-of-window = the top of the window
azplayer-setting-show-clock = show the clock
azplayer-setting-look = the look
azplayer-setting-look-note = AzPlayer is always Media Center's deep blue. The other Azlin apps show { $theme }, { $mode } - their look is chosen in any of them.
azplayer-setting-language = the language
azplayer-language-system = the system's language
azplayer-setting-pictures-open-on = the picture library opens on
azplayer-setting-slide-for = slide show: each picture shows for
azplayer-seconds = { $count ->
    [one] 1 second
   *[other] { $count } seconds
 }
azplayer-setting-ken-burns = pan and zoom (else a plain cross-fade)
azplayer-setting-music-open-on = the music library opens on
azplayer-setting-shuffle-all = shuffle the songs (else album by album)
azplayer-setting-videos-open-on = the video library opens on
azplayer-setting-playing = playing
azplayer-setting-resume = resume a video where it was stopped
azplayer-setting-skip-back = skip back
azplayer-setting-skip-forward = skip forward
azplayer-setting-a-minute = a minute
azplayer-setting-live-tv = live tv
azplayer-setting-no-tuner = There is no TV tuner on this computer: live TV needs one. Recorded TV is read from the folders of library setup.
azplayer-setting-tv-open-on = recorded tv opens on
azplayer-setting-folders-of = { $library } folders
azplayer-setting-add-folder = add a folder to { $library }
azplayer-setting-libraries = the libraries
azplayer-setting-rescan = look through the folders again
azplayer-setting-folders-note = Removing a folder leaves its files where they are. Photo libraries and hidden files are left out.
azplayer-setting-opens-on = azplayer opens on
azplayer-setting-start-screen = the start screen
azplayer-setting-window = the window
azplayer-setting-start-fullscreen = open in media only (full screen)
azplayer-setting-about-title = azplayer { $version }
azplayer-setting-about-note = A media center in the look of Windows Media Center: music, pictures and slide shows, videos and movies. Part of the Azlin apps, built with azul. MIT license.
azplayer-setting-about-counts = { $counts }. The data is in { $data }.
azplayer-count-songs = { $count ->
    [one] 1 song
   *[other] { $count } songs
 }
azplayer-count-pictures = { $count ->
    [one] 1 picture
   *[other] { $count } pictures
 }
azplayer-count-videos = { $count ->
    [one] 1 video
   *[other] { $count } videos
 }
azplayer-count-recordings = { $count ->
    [one] 1 recording
   *[other] { $count } recordings
 }
azplayer-setting-keys = the keys
azplayer-setting-keys-note = The arrows move, Enter opens, Backspace or Escape goes back, Home is the start screen. Tab goes to the back button, the controls and what plays.
azplayer-setting-keys-note-more = Ctrl+D, I or the menu key: more info. Space plays and pauses, M mutes, F is full screen, Ctrl+, the settings.
azplayer-unknown-artist = unknown artist
azplayer-unknown-album = unknown album
azplayer-unknown-genre = unknown genre
azplayer-in-no-folder = (no folder)
azplayer-no-date = no date
azplayer-count-items = { $count ->
    [one] 1 item
   *[other] { $count } items
 }
azplayer-never-extras = No extras are installed on this computer.
azplayer-never-radio = There is no radio tuner on this computer.
azplayer-never-tv = There is no TV tuner on this computer.
azplayer-no-music-in = There is no music in { $folder }.
azplayer-no-pictures-in = There are no pictures in { $folder }.
azplayer-address-hint = An address starts with http:// or https:// and names an MP4 or MOV video.
azplayer-shortcuts-moving = Moving around
azplayer-shortcuts-playback = Playback
azplayer-shortcuts-window = Window
azplayer-shortcuts-file = File
azplayer-shortcut-move-focus = Move the focus
azplayer-shortcut-open-play = Open, play
azplayer-shortcut-back = Back
azplayer-shortcut-start-screen = The start screen
azplayer-shortcut-scroll = Scroll the strip or the gallery
azplayer-shortcut-play-pause = Play / pause
azplayer-shortcut-skip = Skip back / forward (how far: settings, videos)
azplayer-shortcut-minute = Back / forward a minute
azplayer-shortcut-previous-next = Previous / next song or picture
azplayer-shortcut-volume = Volume up / down (while something plays)
azplayer-shortcut-mute = Mute
azplayer-shortcut-fullscreen = Fullscreen (media only)
azplayer-shortcut-leave-fullscreen = Leave fullscreen
azplayer-shortcut-open-video = Open a video
azplayer-about-summary = A media center in the look of Windows Media Center: music, pictures and slide shows, videos and movies. Videos play with azul's VideoWidget, their sound with azul's AudioPlayer, started together; recent files resume where you stopped. Part of the Azlin apps, built with azul.
azplayer-added-to-queue = Added to the queue.
azplayer-looking-again = Looking through the folders again.
azplayer-last-song = That was the last song.
azplayer-nothing-playing = Nothing is playing.
azplayer-no-music = There is no music in the music folder.
azplayer-osd-stopped = Stopped
azplayer-osd-shuffled = Shuffled
azplayer-video-files = Video
azplayer-pause = Pause
azplayer-play = Play
azplayer-sound-on = Sound on
azplayer-mute = Mute
azplayer-stop-music = Stop the music
azplayer-slide-music = Music under the slide show
azplayer-pause-slide-show = Pause the slide show
azplayer-play-slide-show = Play the slide show
azplayer-previous-picture = Previous picture
azplayer-next-picture = Next picture
azplayer-shuffle = Shuffle
azplayer-previous-song = Previous song
azplayer-skip-back = Skip back
azplayer-skip-forward = Skip forward
azplayer-next-song = Next song
azplayer-stop = Stop
azplayer-from-the-start = From the start
azplayer-volume-down = Volume down
azplayer-volume-up = Volume up
azplayer-osd-muted = Muted
azplayer-osd-volume = Volume { $percent } %
azplayer-about-title = about azplayer
azplayer-delete-title = delete this file?
azplayer-delete-note = It is deleted from { $folder } and cannot be brought back.
azplayer-delete-yes = yes, delete it
azplayer-choice-no = no
azplayer-remove-folder-title = remove this folder from { $library }?
azplayer-remove-folder-note = AzPlayer stops showing what is in it; the files stay where they are.
azplayer-remove-folder-yes = yes, remove it
azplayer-paused-title = the music is paused
azplayer-paused-note = Stop it, or keep it paused for later (it stays bottom left)?
azplayer-paused-stop = stop the music
azplayer-paused-keep = keep it paused
azplayer-video-failed-title = this video does not play here
azplayer-address-note = An MP4 or MOV video (H.264) on a web server plays while it downloads; the sound starts with the picture.
azplayer-choice-play = play
azplayer-address-sample = try: Big Buck Bunny (10 s, 360p)
azplayer-choice-open = open
azplayer-choice-queue = add to queue
azplayer-choice-view = view
azplayer-choice-slide-show-from-here = play slide show from here
azplayer-choice-delete = delete
azplayer-choice-from-start = play from the start
azplayer-choice-forget = remove from recently played
azplayer-now-playing = now playing
azplayer-paused = paused
azplayer-choice-pause = pause
azplayer-choice-stop = stop
azplayer-about-license = { $license } license. The data is in { $data }.
azplayer-forgotten = Removed from recently played (the file stays).
azplayer-delete-only-library = Only a file of a library is deleted here.
azplayer-delete-playing = It is playing or queued: stop it first.
azplayer-deleted = { $title } was deleted.
azplayer-delete-failed = the file was not deleted
azplayer-opening = opening { $title }…
azplayer-not-available = { $item } (not available: { $why })
azplayer-count-artists = { $count ->
    [one] 1 artist
   *[other] { $count } artists
 }
azplayer-count-genres = { $count ->
    [one] 1 genre
   *[other] { $count } genres
 }
azplayer-count-albums = { $count ->
    [one] 1 album
   *[other] { $count } albums
 }
azplayer-count-files-played = { $count ->
    [one] 1 file played
   *[other] { $count } files played
 }
azplayer-count-found = { $count } found
azplayer-status-scanning = looking through the folder…
azplayer-status-cut = the first { $count } files of the folder
azplayer-search-empty = Type to search the music, the pictures and the videos.
azplayer-search-none = Nothing found for “{ $query }”.
azplayer-kind-movies = movies (videos of 40 minutes or more)
azplayer-kind-music = music
azplayer-kind-pictures = pictures
azplayer-kind-videos = videos
azplayer-kind-tv = recorded TV
azplayer-empty-looking = Looking for { $kind } in { $folder }…
azplayer-empty-missing = AzPlayer reads { $kind } from { $folder }, which is not there. Add another folder in settings, library setup.
azplayer-empty-music = There is no music in { $folder } yet. Songs saved there (MP3, AAC, FLAC, Ogg, WAV) show here.
azplayer-empty-pictures = There are no pictures in { $folder } yet. Photos saved there (JPEG, PNG, GIF, WebP) show here.
azplayer-empty-videos = There are no { $kind } in { $folder } yet. MP4 and MOV videos (H.264) saved there show here.
azplayer-empty-tv = There is no recorded TV. Recordings saved as MP4 in { $folder } show here.
azplayer-resume-at = resume at { $time }
azplayer-mp4-or-mov = MP4 or MOV
azplayer-search-placeholder = type to search the music, pictures and videos
azplayer-search = Search
azplayer-close = Close
azplayer-more-info = more info
azplayer-address-name = The video's address
azplayer-enter-removes = Enter removes it
azplayer-nothing-playing-hint = Nothing is playing. Choose music library or play all on the start screen.
azplayer-position-song = Position in the song
azplayer-position-video = Position in the video
azplayer-up-next = up next
azplayer-queue-end = the end of the queue
azplayer-loading-picture = loading the picture…
azplayer-picture-of = { $index } of { $total }
azplayer-menu-open-file = Open File…
azplayer-menu-open-recent = Open Recent
azplayer-menu-start = Start
azplayer-menu-refresh = Refresh Libraries
azplayer-menu-settings = Settings…
azplayer-menu-media = Media
azplayer-menu-play-pause = Play / Pause
azplayer-menu-previous = Previous
azplayer-menu-next = Next
azplayer-menu-back-seconds = { $count ->
    [one] Back 1 Second
   *[other] Back { $count } Seconds
 }
azplayer-menu-forward-seconds = { $count ->
    [one] Forward 1 Second
   *[other] Forward { $count } Seconds
 }
azplayer-menu-audio = Audio
azplayer-menu-sound-on = Sound On
azplayer-menu-mute = Mute
azplayer-menu-volume-up = Volume Up
azplayer-menu-volume-down = Volume Down
azplayer-menu-view = View
azplayer-music = Music
azplayer-menu-pictures = Pictures
azplayer-menu-videos = Videos
azplayer-menu-movies = Movies
azplayer-menu-recorded-tv = Recorded TV
azplayer-menu-recent = Recently Played
azplayer-menu-leave-fullscreen = Leave Fullscreen
azplayer-menu-fullscreen = Fullscreen
azplayer-pick-folder = Add a folder to { $library }

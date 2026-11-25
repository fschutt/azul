# AzReview's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## AzReview

azreview-about-summary = Ink-first code review: the files of a folder as pages, marked with a pen, with voice clips. Sessions are kept in your data folder.
azreview-shortcuts-review = Review
azreview-shortcut-save = Save the session now
azreview-shortcut-meaning = Pick the ink's meaning
azreview-shortcut-previous-nib = The previous nib
azreview-voice-note = { $count ->
    [one] { $count } sample of spoken rationale
   *[other] { $count } samples of spoken rationale
 }
azreview-no-files = No reviewable files under { $folder }
azreview-save-failed = The session was not saved: { $why }
azreview-menu-ink = Ink
azreview-cycle-nib = Cycle nib
azreview-toggle-recording = Start / stop recording
azreview-menu-session = Session
azreview-save-now = Save now
azreview-reveal-archive = Reveal archive folder
azreview-files = Files
azreview-stop-recording = Stop recording
azreview-record = Record
azreview-save = Save
azreview-settings = Settings
azreview-files-switch = Show or hide the files
azreview-page = Page { $n }
azreview-dismiss = Dismiss
azreview-open-file = Open a file to begin
azreview-meaning-scope = scope
azreview-meaning-issue = issue
azreview-meaning-question = question
azreview-meaning-duplicate = duplicate
azreview-meaning-praise = praise
azreview-nib-marker = marker
azreview-nib-pen = pen
azreview-nib-audio-pen = audio pen

## Dictation and the side column of notes

azreview-shortcut-dictate = Rest the pen on the code to dictate a note; resting again, moving away or drawing ends it
azreview-notes = Notes
azreview-notes-switch = Show or hide the notes
azreview-notes-hint-pen = Rest the pen on the code for { $seconds } s and speak: your words become a note here. If nothing starts, the system may not report your pen as a pen: choose Pen or mouse under Settings > Dictation.
azreview-notes-hint-any = Rest the pen or the mouse on the code for { $seconds } s and speak: your words become a note here.
azreview-notes-off = Dictation is off. Turn it on under Settings > Dictation.
azreview-listening = Listening
azreview-finishing = Finishing
azreview-dictation-stop = Stop
azreview-note-place = Page { $page }, line { $line }
azreview-note-waiting = Waiting for words...
azreview-note-placeholder = Type or dictate a note
azreview-note-name = Note { $n }
azreview-note-delete = Delete note { $n }
azreview-dictation-denied = Dictation needs speech recognition, which is not allowed for AzReview ({ $why }). Allow it in System Settings > Privacy & Security.
azreview-dictation-unavailable = Dictation is not available here: { $why }
azreview-dictation-failed = Dictation failed: { $why }
azreview-load-failed = The saved session could not be read: { $why }
azreview-category-dictation = Dictation
azreview-dictation-when = Start by resting
azreview-dictation-off = Off
azreview-dictation-pen = Pen
azreview-dictation-any = Pen or mouse
azreview-dictation-rest = Rest time
azreview-dictation-quiet = Stop after quiet
azreview-dictation-note = Resting the pen on the code starts a note that listens. Drawing, moving away, resting again, Escape or a stretch of quiet ends it. The words stay in the note, which you can edit, and are saved with the session.
azreview-rest-0-8 = 0.8 s
azreview-rest-1-2 = 1.2 s
azreview-rest-2-0 = 2 s
azreview-rest-3-0 = 3 s
azreview-quiet-5 = 5 s
azreview-quiet-10 = 10 s
azreview-quiet-20 = 20 s
azreview-quiet-40 = 40 s

## Open a folder, undo, the pages

azreview-open-folder = Open folder...
azreview-choose-folder = Choose the folder to review
azreview-undo = Undo
azreview-redo = Redo
azreview-pages = Pages of the file
azreview-shortcut-open-folder = Open another folder to review
azreview-shortcut-undo = Undo the last stroke or erase (Mod+Shift+Z brings it again)
azreview-ink-surface = Drawing surface of page { $n }
azreview-recording-level = Recording level
azreview-no-folder = Choose a folder to review.

## Agents (the backends that answer: claude -p, Gemini, the APIs; the failures in words)

azreview-agent-task-exploration = Exploration
azreview-agent-task-editing = Editing
azreview-agent-task-export = Export
azreview-agent-backend-claude-print = Claude Code (claude -p)
azreview-agent-backend-anthropic-api = Anthropic API
azreview-agent-backend-gemini-cli = Gemini CLI
azreview-agent-backend-gemini-api = Gemini API
azreview-agent-backend-mock = Mock (scripted)
azreview-agent-error-not-installed = { $program } is not installed (looked in { $where })
azreview-agent-error-not-logged-in = { $program } is not signed in: open a terminal, run { $program } and sign in, then try again
azreview-agent-error-key-missing = The key { $reference } is not set: put the key into that environment variable or keychain item (never into the settings)
azreview-agent-error-key-denied = The key { $reference } could not be read: { $why }
azreview-agent-error-timeout = Timeout after { $secs } s
azreview-agent-error-cancelled = Cancelled
azreview-agent-error-busy = { $running } agents are working already (at most { $cap } at once): wait for one to finish
azreview-agent-error-spawn = { $program } could not be started: { $why }
azreview-agent-error-exit = { $program } stopped with exit code { $code }: { $detail }
azreview-agent-error-http = The service answered { $status } { $kind }: { $detail }
azreview-agent-error-rate-limited = The service is busy or over its limit: { $detail }
azreview-agent-error-network = The service could not be reached: { $why }
azreview-agent-error-budget = The call would cost more than the limit of { $limit }: raise it in the settings
azreview-agent-error-max-turns = The agent used all { $turns } turns without an answer: raise the limit in the settings or ask a narrower question
azreview-agent-error-refused = The agent declined: { $detail }
azreview-agent-error-bad-output = The answer could not be read ({ $why }): { $excerpt }
azreview-agent-error-too-large = { $what } is too large ({ $size } of { $limit }): shorten it or raise the limit in the settings
azreview-agent-error-settings = The agent settings are not usable: { $why }
azreview-agent-error-unsupported = Not supported here: { $why }

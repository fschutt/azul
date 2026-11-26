# AzReview's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## AzReview

azreview-about-summary = Ink-first code review: the files of a folder as pages, marked with a pen, with voice clips. Sessions are kept in your data folder.
azreview-shortcuts-review = Review
azreview-shortcut-save = Save the session now
azreview-shortcut-meaning = Take a pen (the same key again: its next nib)
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

## Whiteboard

azreview-whiteboard-notice-too-many-items = The whiteboard holds { $limit } items, as many as it has room for: remove one first.
azreview-whiteboard-notice-pictures-too-big = The pictures on the whiteboard take as much memory as they may ({ $limit } pixels): remove a cutout first.
azreview-whiteboard-notice-picture-too-big = This cutout's picture has { $px } pixels, more than the { $limit } a cutout may have: circle less code.
azreview-whiteboard-notice-picture-invalid = The cutout's picture could not be used: its bytes do not match its size.
azreview-whiteboard-notice-scale-lowered = This cutout is large: its picture is drawn at { $used }% scale instead of { $wanted }% so that it fits in memory, so it is a little less sharp.
azreview-whiteboard-notice-label-shortened = A label holds { $limit } characters at most: the rest was cut off.
azreview-whiteboard-notice-no-such-item = There is no item { $id } on the whiteboard.
azreview-whiteboard-notice-arrow-to-itself = An arrow needs two different cutouts: it cannot start and end on the same one.
azreview-whiteboard-notice-arrow-needs-a-cutout = An arrow has to start or end on a cutout.
azreview-whiteboard-notice-arrow-repeated = There is an arrow like this one already.
azreview-whiteboard-notice-board-repaired = { $dropped } items of the saved whiteboard could not be used and were left out.
azreview-whiteboard-jump-file-changed = The file has changed since this was circled; these lines are as they were.
azreview-whiteboard-jump-lines-moved = These lines moved: they are now at lines { $first } to { $last }.
azreview-whiteboard-jump-lines-changed = These lines have changed since they were circled.
azreview-whiteboard-jump-file-missing = The file is no longer there.
azreview-whiteboard-store-not-a-board = This file is not a whiteboard AzReview can open ({ $why }). It is kept as it is and the whiteboard starts empty.
azreview-whiteboard-store-too-new = This whiteboard was saved by a newer AzReview (its format is { $found }). It is left as it is and the whiteboard starts empty.
azreview-whiteboard-store-too-large = This whiteboard's file is far larger than a whiteboard is ({ $bytes } bytes). It is kept as it is and the whiteboard starts empty.
azreview-whiteboard-surface = Whiteboard
azreview-whiteboard-empty = Circle some code with the pen and drag the circle to this bar to keep it here. Draw arrows between the pieces to show how they belong together.
azreview-whiteboard-drawing = Drawing the picture...
azreview-whiteboard-picture-missing = The picture is not available. These are the lines it shows:
azreview-whiteboard-cutout-name = { $file }, lines { $first } to { $last }
azreview-whiteboard-label-placeholder = Add a label
azreview-whiteboard-label-name = Label of { $file }, lines { $first } to { $last }

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
azreview-save-pointer-log = Save the pointer log
azreview-no-folder-hint = AzReview shows the files of a folder as pages you can mark with a pen or the mouse.

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
azreview-agent-field-backend = Backend
azreview-agent-hint-backend = Who answers: Claude Code (claude -p, no key needed), the Anthropic API, the Gemini CLI, the Gemini API, or a scripted mock.
azreview-agent-field-model = Model
azreview-agent-hint-model = A model name (an alias like haiku, sonnet or opus follows the newest model of its family). Empty: the backend's own default.
azreview-agent-field-timeout = Timeout (seconds)
azreview-agent-hint-timeout = How long one call may take before it is stopped.
azreview-agent-field-max-tokens = Longest answer (tokens)
azreview-agent-hint-max-tokens = The most the service may write in one answer.
azreview-agent-field-budget = Cost limit per call (US cents, 0 = none)
azreview-agent-hint-budget = Claude Code stops a call that would cost more.
azreview-agent-field-max-turns = Most turns (0 = no limit)
azreview-agent-hint-max-turns = How many steps Claude Code may take (reading a file is a step).
azreview-agent-field-key = Key (env:NAME or keychain:ITEM)
azreview-agent-hint-key = The name of an environment variable or keychain item that holds the key. The key itself is never kept in the settings. Empty: the program's own sign-in.
azreview-agent-field-program = Program
azreview-agent-hint-program = The path of the program. Empty: look for it on the PATH and in the usual places.
azreview-agent-field-base-url = Address of the service
azreview-agent-hint-base-url = Empty: the usual address. Only for a proxy or a test server.
azreview-agent-field-args = Extra arguments
azreview-agent-hint-args = Added to the program's command line. Arguments that would widen what the agent may do are refused.
azreview-agent-field-max-concurrent = Agents at work at once
azreview-agent-hint-max-concurrent = 1 to 8. More than two can use a lot of your quota.
azreview-agent-field-limit-scale = Size of the limits
azreview-agent-hint-limit-scale = 1 to 8 times the usual limits on what is sent and read. Raise it only for a big job.
azreview-agent-problem-unknown-backend = { $field }: "{ $word }" is not a backend: use claude-print, anthropic-api, gemini-cli, gemini-api or mock
azreview-agent-problem-not-a-number = { $field }: "{ $word }" is no number
azreview-agent-problem-out-of-range = { $field }: { $value } is outside { $min } to { $max }, using { $used }
azreview-agent-problem-key-invalid = { $field }: write the key as env:NAME (an environment variable) or keychain:ITEM
azreview-agent-problem-key-secret = { $field }: this looks like a key itself: never put a key in the settings; keep it in an environment variable or the keychain and write its name here
azreview-agent-problem-arg-forbidden = { $field }: { $arg } is not allowed here (it would widen what the agent may do)
azreview-agent-problem-model-required = { $field }: name a model
azreview-agent-problem-url-invalid = { $field }: { $text } is no http:// or https:// address
azreview-agent-problem-unknown-field = { $field }: no such setting ({ $word })

## The layout: the file tree, the right bar, the bottom bar

azreview-right-bar = Show or hide the whiteboard
azreview-whiteboard-placeholder = The whiteboard: circle code with the pen, drag it here and draw arrows between the cutouts.
azreview-agents-none = No agents are working.
azreview-resize-tree = Change the width of the file tree
azreview-resize-panel = Change the width of the whiteboard
azreview-menu-view = View
azreview-speech-chain = Speech recognisers, in the order they are asked
azreview-page-skip = Skip to page { $n }
azreview-reviewed-line = { $reviewed } of { $total } lines reviewed ({ $percent }%), { $seen } seen
azreview-reviewed-counting = Still counting the workspace...
azreview-shortcut-not-reviewed = Say the file is not reviewed, or is again (also a right click on it in the tree)
azreview-not-shown = { $file } is not shown: { $why }
azreview-not-shown-binary = it is no text.
azreview-not-shown-large = it is { $size }, over the { $limit } a page strip holds.
azreview-not-shown-unreadable = it could not be read ({ $why }).
azreview-tree-loading = Reading the folder...
azreview-dictation-opening = Opening the microphone
azreview-mic-line = Microphone: { $state }
azreview-mic-cold = closed
azreview-mic-warm = closed, the recogniser is loaded
azreview-mic-armed = open and waiting (the system's microphone indicator is on)
azreview-mic-listening = open, listening
azreview-standby-when = Microphone ready ahead
azreview-standby-off = Off
azreview-standby-near = When the pen is near
azreview-standby-always = Always
azreview-standby-idle = Close after the pen was away
azreview-idle-15 = 15 s
azreview-idle-30 = 30 s
azreview-idle-120 = 2 min
azreview-idle-600 = 10 min
azreview-standby-note = Opening the microphone takes a third of a second. Kept ready, a dictation starts the moment the pen has rested and keeps the words spoken just before. "When the pen is near" opens the microphone as the pen comes near a page and closes it when the pen has been away for the time above, or the window goes to the back. The system's microphone indicator is on while the microphone is open; with "Always" it stays on as long as AzReview runs.
azreview-chip-working = an agent is working
azreview-chip-suggested = a suggestion waits
azreview-chip-done = done
azreview-shortcut-files = Show or hide the files
azreview-shortcut-whiteboard = Show or hide the whiteboard
azreview-agent-field-effort = Effort
azreview-agent-hint-effort = How hard the model thinks: low is fast and cheap, max is thorough. Empty: the model's own default. A model that does not know effort refuses it: clear the row then.
azreview-agent-problem-not-a-choice = { $field }: "{ $word }" is not one of the choices ({ $choices })

## The symbol index: what the reviewer says tied to the code (symbols*.rs)
azreview-symbols-status-walking = Looking for code files: { $found } found, { $done } read
azreview-symbols-status-indexing = Indexing code: { $done } of { $found } files
azreview-symbols-status-ready = { $found } code files indexed
azreview-symbols-note-file-capped = { $file } has more definitions than the index keeps ({ $cap }): the first ones are indexed.
azreview-symbols-note-index-full = The index holds as many definitions as it may ({ $cap }): the rest of the workspace is not indexed.
azreview-symbols-note-memory-full = The index reached the memory it may use ({ $mb } MB): the rest of the workspace is not indexed.
azreview-symbols-note-too-many-files = The workspace has more files than the index keeps ({ $cap }): the rest is not indexed.
azreview-symbols-skip-unknown-type = not a kind of file the index reads
azreview-symbols-skip-too-large = larger than the index reads
azreview-symbols-skip-binary = not text
azreview-symbols-skip-generated = generated by a tool
azreview-symbols-skip-minified = lines no one wrote by hand
azreview-symbols-skip-unreadable = could not be read

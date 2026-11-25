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

# AzVideoCut's words in English (src/l10n.rs). Every key the source names is here and in de.ftl;
# the edits are said by the undo history's English names (azvideocut-edit-<name>).

azvideocut-about = About
azvideocut-about-azvideocut = About AzVideoCut
azvideocut-about-credits = Credits
azvideocut-about-summary = A video editor on azul's video stack: a media bin, source and program monitors, a timeline of tracks, effects, transitions and H.264 export. Part of the Azlin apps.
azvideocut-active = {" "}(active)
azvideocut-bitrate = Bitrate
azvideocut-cancel-export = Cancel export
azvideocut-cancelling = Cancelling the export...
azvideocut-category-export = Export
azvideocut-category-playback = Playback
azvideocut-close = Close
azvideocut-cross-dissolve = Cross dissolve
azvideocut-dip-to-black = Dip to black
azvideocut-disable-clip = Disable clip
azvideocut-done = Done: { $file }
azvideocut-double-click-an-item-of-the = Double-click an item of the bin to open it here.
azvideocut-edit-effects = Effects
azvideocut-edit-enable = Enable
azvideocut-edit-insert = Insert
azvideocut-edit-lift = Lift
azvideocut-edit-move = Move
azvideocut-edit-overwrite = Overwrite
azvideocut-edit-razor = Razor
azvideocut-edit-ripple-delete = Ripple delete
azvideocut-edit-ripple-trim = Ripple trim
azvideocut-edit-slip = Slip
azvideocut-edit-track-lock = Track lock
azvideocut-edit-track-visibility = Track visibility
azvideocut-edit-transition = Transition
azvideocut-edit-trim = Trim
azvideocut-effect-controls = Effect Controls
azvideocut-enable-clip = Enable clip
azvideocut-encoder = Encoder: { $encoder }
azvideocut-encoder-note = H.264 in MP4, encoded by { $encoder }; a machine without an H.264 encoder writes Y4M.
azvideocut-error-clip-gone = That clip is gone.
azvideocut-error-no-track = That track does not exist.
azvideocut-error-nothing = Nothing to change.
azvideocut-error-track-locked = The track is locked.
azvideocut-error-wrong-track = That clip cannot go on this kind of track.
azvideocut-export = Export
azvideocut-export-failed = Export failed: { $why }
azvideocut-export-now = Export now
azvideocut-export-open = Export...
azvideocut-export-settings-note = Encoder: { $encoder }. Exports go to videocut/<project>/exports/ in the data folder.
azvideocut-exported = Exported { $key } ({ $bytes } bytes).
azvideocut-exporting = Exporting...
azvideocut-exporting-title = Exporting
azvideocut-failed = Failed: { $why }
azvideocut-field-crop-bottom = Crop bottom %
azvideocut-field-crop-left = Crop left %
azvideocut-field-crop-right = Crop right %
azvideocut-field-crop-top = Crop top %
azvideocut-field-frames = Frames
azvideocut-field-opacity = Opacity %
azvideocut-field-position-x = Position X
azvideocut-field-position-y = Position Y
azvideocut-field-scale = Scale %
azvideocut-format = { $width } x { $height } at { $fps } fps
azvideocut-frame = < Frame
azvideocut-frame-2 = Frame >
azvideocut-frames-of = { $done } of { $total } frames - { $how }
azvideocut-group-edit = Edit
azvideocut-group-file = File
azvideocut-group-marks = Marks
azvideocut-group-timeline = Timeline
azvideocut-group-tools = Tools
azvideocut-h264-by = H.264 in MP4 by { $encoder }
azvideocut-import = Import...
azvideocut-import-media = Import media
azvideocut-import-mp4-files-ctrl-i = Import MP4 files (Ctrl+I).
azvideocut-imported =
    { $count ->
        [one] Imported 1 file.
       *[other] Imported { $count } files.
    }
azvideocut-imported-errors =
    { $count ->
        [one] Imported 1 file; { $errors }
       *[other] Imported { $count } files; { $errors }
    }
azvideocut-importing =
    { $count ->
        [one] Importing 1 file...
       *[other] Importing { $count } files...
    }
azvideocut-in-i = In (I)
azvideocut-inf-db = -inf dB
azvideocut-insert = Insert (,)
azvideocut-make-the-sample-project = Make the sample project
azvideocut-mark-in-i = Mark In (I)
azvideocut-mark-out-o = Mark Out (O)
azvideocut-missing-media = (missing media)
azvideocut-mit-license = MIT license
azvideocut-new-empty-project = New empty project
azvideocut-no-audio-decoder = no audio decoder
azvideocut-no-clip-selected = No clip selected
azvideocut-no-media = No media
azvideocut-no-project-yet = No project yet
azvideocut-nothing-to-redo = Nothing to redo.
azvideocut-nothing-to-undo = Nothing to undo.
azvideocut-open-clip-first = Open a clip in the source monitor first.
azvideocut-open-in-source = Open in source
azvideocut-opening-the-project = Opening the project...
azvideocut-out-before-in = The out mark lies before the in mark.
azvideocut-out-o = Out (O)
azvideocut-overwrite = Overwrite (.)
azvideocut-pause = Pause (K)
azvideocut-play = Play (L)
azvideocut-playback-note = Playback is paced by the monitor's refresh: each tick shows the frame that is due. Pictures are composed on the CPU at the monitor's size.
azvideocut-playing-at = Playing at { $speed }x.
azvideocut-program-of = Program: { $name }{ $active }
azvideocut-project = Project
azvideocut-project-of = Project: { $name }
azvideocut-range = Range
azvideocut-range-first-second = First second
azvideocut-range-in-out = In to Out
azvideocut-range-sequence = Sequence
azvideocut-ready = Ready.
azvideocut-save-failed = The project could not be saved: { $why }
azvideocut-changes-not-saved = { $count ->
    [one] 1 save did not land. Close the window again to quit without it.
   *[other] { $count } saves did not land. Close the window again to quit without them.
 }
azvideocut-redo = Redo
azvideocut-redo-edit = Redo { $edit }
azvideocut-reset-effects = Reset effects
azvideocut-select-a-clip-in-the-timeline = Select a clip in the timeline to set its position, scale, opacity, crop and transition.
azvideocut-select-clip-to-delete = Select a clip to delete.
azvideocut-settings = Settings
azvideocut-shortcut-export-into-the-data = Export into the data folder
azvideocut-shortcut-import-media = Import media
azvideocut-shortcut-insert-overwrite-the-source = Insert, overwrite the source range
azvideocut-shortcut-lift-ripple-delete = Lift, ripple delete
azvideocut-shortcut-mark-in-out-the = Mark in, out (the active monitor)
azvideocut-shortcut-one-frame-shift-one = One frame (Shift: one second)
azvideocut-shortcut-play-pause = Play, pause
azvideocut-shortcut-razor-every-track-at = Razor every track at the playhead
azvideocut-shortcut-select-razor-ripple-slip = Select, razor, ripple, slip, hand, zoom
azvideocut-shortcut-shuttle-back-stop-forward = Shuttle back, stop, forward
azvideocut-shortcut-snapping-on-off = Snapping on / off
azvideocut-shortcut-start-end-previous-next = Start, end, previous / next edit
azvideocut-shortcut-undo-redo = Undo, redo
azvideocut-shortcut-zoom-in-out = Zoom in, out
azvideocut-size = Size
azvideocut-snap-off = Snap: off (S)
azvideocut-snap-on = Snap: on (S)
azvideocut-snapping-off = Snapping off
azvideocut-snapping-on = Snapping on
azvideocut-source = Source
azvideocut-source-of = Source: { $name }{ $active }
azvideocut-source-position = Source position
azvideocut-space-plays-and-pauses-j-k = Space plays and pauses; J / K / L shuttle; Home / End; Up / Down jump to edits.
azvideocut-stopped = Stopped.
azvideocut-tool-hand = Hand (H)
azvideocut-tool-razor = Razor (C)
azvideocut-tool-ripple = Ripple (B)
azvideocut-tool-select = Select (V)
azvideocut-tool-slip = Slip (Y)
azvideocut-tool-status = Tool: { $tool }
azvideocut-tool-zoom = Zoom (Z)
azvideocut-transition-at-the-clip-s-head = Transition at the clip's head
azvideocut-transition-none = None
azvideocut-undo = Undo
azvideocut-undo-edit = Undo { $edit }
azvideocut-video-files = Video

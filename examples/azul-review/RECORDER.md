# AzReview's session recorder

AzReview can record a whole review session: the screen (the whole display by default, so the video can
show the browser the person switched to; or one window; or AzReview's own frames, pixel for pixel), the
camera, the voice, and everything the review did (the pen, the navigation, the conversations, the
diffs, the whiteboard), all on ONE clock, to disk, so that a later step can cut them into a video
to share. The video is the supplement: the review's data is the record, and a session without the screen
is a complete session. This file is the contract between the recorder (`recorder*.rs`), the exporter and an
editing agent: the files, the schemas, the events, the cut list.

The code is std-only where it can be (`recorder_clock.rs`, `recorder_mp4.rs`,
`recorder_events.rs`, `recorder_manifest.rs`), so everything below has unit tests that run in
seconds; every number and every kind named here is checked by them.

## 1. One clock

Every time in every file is **microseconds on the session clock**: the time since the recording
began, read from a monotonic clock (`recorder_clock.rs`). The manifest says where that origin is on
the wall clock (`clock.origin_unix_ms`, `origin_iso8601`, `utc_offset_minutes`), so a moment of the
recording can be told in real time (a git commit time, a file date). A pause does not stop the clock:
the time between two runs is simply not recorded (`runs`).

Each source stamps a sample where it captures it: the screen when the frame was rendered, the camera
when its frame arrives, the microphone when its chunk arrives (minus the chunk's length). Inside an
audio segment the packets are sample-exact (a packet is 960 samples at 48 kHz); the segment itself is
placed by the time its first chunk arrived, so the drift between the sound card's clock and the
system's is bounded by one segment (100 ppm over 5 minutes is 30 ms).

## 2. A session on disk

```text
<data root>/review/recordings/<date-time>/       e.g. 2026-11-26_10-00-00 (UTC, sorts by time)
    manifest.json        the clock, the tracks and their segments, the runs, the diagnostics
    events.jsonl         what the review did, one JSON object a line
    transcript.jsonl     what was said, with the time of every word
    screen/0001.mp4 ...  H.264, the display (or a window, or AzReview's own frames), a frame only when it changed
    camera/0001.mp4 ...  H.264, the camera, in chunks of five seconds
    audio/0001.mp4  ...  Opus, the microphone
```

The recordings folder is a setting (an external disk is fine). Nothing is written to the source files
of the review. The default screen track is the whole display and records everything on it (section 8 says
what is left out and how to pause); the `own` source records nothing but the review.

### Segment files

A track is a list of **segment** files, each about five minutes. A segment is a *fragmented MP4*
(`recorder_mp4.rs`): `ftyp` + `moov` first (the track's description, no samples), then a `moof` +
`mdat` pair about every two seconds (a new fragment at the first keyframe after a second, never longer
than four), and, when the segment was closed properly, an `mfra` index at the end. After a crash a
file is whole up to its last complete fragment: the recorder cuts it there at the next start
(`state: "recovered"`, `segments[].recovered: true`) and loses at most the last two seconds of each
track (the last chunk of the camera). Each segment starts at a keyframe and has its own `start_us` on the
session clock.

**The camera is recorded in chunks** (`recorder-camera-chunk-secs`, five seconds by default): the camera's
pictures go to the hardware H.264 encoder as they arrive (the way AzMeet sends them over the wire: packets
leave, raw pictures never do), a keyframe is asked for every chunk, and every chunk is one fragment of the
file (a fragment closes at the keyframe once it spans three quarters of a chunk, and at any picture once it
spans a chunk and a half). A crash costs at most one chunk, and the file is cut at chunks; the camera file
is ordinary fragmented MP4 that ffmpeg and HandBrake read as it is, so combining it with the screen track
is a cheap overlay and not a re-encode of the camera (section 10).

* **H.264** (`avc1`, `avcC`; the parameter sets of the first keyframe; the SPS's colour description
  becomes a `colr` box). Encoded by azul's `VideoEncoder` (VideoToolbox, in hardware; no B-frames, so
  decode order is presentation order). Times are ticks of 90 kHz; a sample lasts until the next
  one's time, so a screen that did not change for twenty seconds is one sample of twenty seconds
  (variable frame rate). After the last change of a burst the recorder sends the same frame three
  more times (+120, +400, +1200 ms) so the encoder refines the text to full sharpness, then nothing
  until the screen changes again.
* **Opus** (`Opus`, `dOps`; 48 kHz mono, 32 kbit/s, packets of 20 ms from azul's `AudioEncoder`).
  AVFoundation (QuickTime) plays it; ffmpeg and VLC do.

Why not one muxed file: the three tracks have their own encoders, start times and failures; a broken
camera must not cost the screen. The exporter puts them together.

## 3. `manifest.json` (schema 1)

Written atomically (temp file + rename) when the recording starts, when a segment opens or closes,
when it pauses and resumes, and when it stops. Reading it (`Manifest::parse`) is strict: a manifest
that says something that cannot be (segments out of order, a path outside the session folder, an
unknown schema) is refused with every problem named. Unknown fields are ignored.

| field | meaning |
| --- | --- |
| `schema` | `1` |
| `app` | `{name, version}` of AzReview |
| `session` | the folder's name |
| `state` | `recording`, `complete` (stopped properly) or `recovered` (found cut off and cut back) |
| `workspace` | `{root, name}`: the folder that was reviewed |
| `clock` | `{unit: "us", origin_unix_ms, origin_iso8601, utc_offset_minutes}` |
| `host` | `{os, arch}` |
| `tracks.screen`, `.camera`, `.audio` | a track, absent when it was not asked for (below) |
| `runs` | `[{start_us, end_us?, end_reason?}]`: the stretches the recorder was recording; a pause, an idle pause, a quota pause and the stop end a run. What lies between two runs was not recorded. At most the last run is open |
| `events`, `transcript` | `{file, count, bytes, dropped}` of the two line files |
| `ended_unix_ms` | when it stopped; absent while it goes on |
| `diagnostics` | `[{t_us, level: info\|warn\|error, text}]`, at most 500: a denied camera, a drop, a recovery |

A **track**: `source` (`frame-tap` = AzReview's own frames, `screencapturekit`, `camera`,
`microphone`, `mock`), `device?`, `codec` (`h264` / `opus`), `encoder`, `hardware`, `width`,
`height` (0 for sound), `scale` (device pixels to a point: 2.0 on a Retina display; the screen
track is recorded at device scale), `fps`, `vfr`, `bitrate_kbps`, `sample_rate`, `channels` (0 for
pictures), `timescale`, `status` (`recording`, `stopped`, `off`, `denied`, `failed`), `note?` (why a
track is off, denied or failed, in words) and `segments`.

A **segment**: `file` (relative to the session folder, in the track's own folder), `start_us`,
`end_us?` (absent while it is written), `bytes`, `samples`, `complete`, `recovered?`, and `width?` /
`height?` when the picture is another size than the track's (resizing the window starts a new
segment: a file has one size).

## 4. `events.jsonl`

One JSON object a line: `{"t": <microseconds>, "k": "<kind>", ...fields}`. `t` and `k` come first;
the fields are merged in after them (a field named `t` or `k` is dropped). Written by ONE call from
any thread, `EventSink::event(kind, fields_json)`, which stamps the event with the session clock where
it is made. A line the recorder could not write is never lost silently: the log says what was
dropped, once a second, as `recorder.drop`.

**Caps** (`Caps`, `KINDS`): every kind has a rate and a burst (a token bucket), all kinds together
500 a second (burst 1000), a line is at most 8 KB (a pen stroke 128 KB), at most 10 000 events wait for
the writer, and the file stops at 200 MB (then only `session.*` and `recorder.*` lines are written,
after one `recorder.events_capped`). An event over a cap is dropped and counted; a kind not in the
table is recorded at 10 a second (burst 20). A kind's name is `a-z`, `0-9`, `_` and `.`.

### The vocabulary

Coordinates are in logical pixels (points); the screen track is at device scale, so multiply by
`tracks.screen.scale` to find a point in the video. Files are workspace-relative paths with `/`.
Lines are 1-based, pages and indices 0-based.

| kind | per s | fields |
| --- | --- | --- |
| `session.start` | 5 | `app`, `version`, `workspace` |
| `session.stop` | 5 | `reason` (`user`, `quit`, `error`) |
| `session.pause` | 5 | `reason` (`user`, `hotkey`, `locked`) |
| `session.resume` | 5 | `reason` (`user`, `hotkey`, `activity`, `unlocked`) |
| `session.idle_pause` | 5 | `idle_ms` |
| `session.quota_pause` | 5 | `free_bytes`, `min_free_bytes` |
| `layout.window` | 2 | `w`, `h`, `scale`: the logical size and the device scale |
| `layout.toggle` | 5 | `panel` (`tree`, `whiteboard`, `agents`, `recorder`, `notes`), `open` |
| `nav.workspace` | 2 | `root`, `files` |
| `nav.file` | 5 | `file`, `lines`: a file was opened (chapters come from these) |
| `nav.page` | 10 | `file`, `page`, `of` |
| `nav.lines` | 5 | `file`, `first`, `last`: the lines in view |
| `pointer.move` | 20 | `x`, `y` (window), `page`, `px`, `py` (on the page), `pen`, `down`: for zooming to the cursor |
| `pointer.global` | 20 | `x`, `y`: points on the desktop from the top left of the primary display, whatever app the pointer is over (at most `recorder-pointer-hz` a second, 10 by default; not when it moved less than a point) |
| `app.front` | 2 | `bundle`, `name`: the app in front changed (so the export can cut chapters at app switches) |
| `pen.select` | 10 | `meaning` (`scope`, `issue`, `question`, `duplicate`, `praise`), `nib` (`marker`, `pen`, `audio`) |
| `pen.stroke.begin` | 30 | `id`, `file`, `page`, `x`, `y` |
| `pen.stroke` | 30 | `id`, `file`, `page`, `meaning`, `nib`, `n`, `t0_us`, `bbox` `[x0,y0,x1,y1]`, `s` `[[dt_ms,x,y,pressure],...]` (at most 2000, decimated) |
| `conv.start` | 5 | `id`, `file`, `line`, `page`, `x`, `y` |
| `conv.word` | 20 | `id`, `w`, `t0_us`, `t1_us`, `p` |
| `conv.working` | 10 | `id`, `task` (`exploration`, `editing`) |
| `conv.suggestion` | 10 | `id`, `op` (`insert`, `replace`, `remove`, `answer`), `file`, `first`, `last` |
| `conv.accept` | 10 | `id`, `op`, `file`, `first`, `last` |
| `conv.reject` | 10 | `id` |
| `conv.done` | 10 | `id` |
| `dictation.start` | 5 | `conv`, `engine` |
| `dictation.end` | 5 | `conv`, `cause` |
| `diff.delete` | 20 | `file`, `line`, `from`, `to`, `text` |
| `diff.accept` | 20 | `file`, `op`, `first`, `last` |
| `wb.open` | 5 | `width` |
| `wb.close` | 5 | `width` |
| `wb.resize` | 10 | `width` |
| `wb.cutout` | 20 | `id`, `file`, `first`, `last` |
| `wb.arrow` | 20 | `id`, `from`, `to` (absent when the arrow hangs on nothing) |
| `wb.edit` | 20 | `op` (`arrow`, `moved`, `resized`, `labelled`, `removed`, `undone`, `redone`), `id`, `ids` (for `removed`) |
| `wb.jump` | 10 | `id`, `file`, `first`, `last` |
| `marker` | 5 | `label`: a chapter the user marked with the hotkey |
| `recorder.track` | 5 | `track`, `status`, `note` |
| `recorder.segment` | 5 | `track`, `file`, `start_us` |
| `recorder.screen_gap` | 2 | `reason`: the screen track's segment ended here (the display slept, was unplugged) and the next picture begins another |
| `recorder.drop` | 2 | `kind`, `n`, `why` (`rate`, `queue`, `size`, `invalid`, `capped`) |
| `recorder.events_capped` | 1 | `bytes` |
| `recorder.note` | 5 | `text` |

## 5. `transcript.jsonl`

One line per stretch of speech: `{t0_us, t1_us, text, lang, engine, conv?, words: [{w, t0_us, t1_us, p?}]}`.
`engine` is the recogniser (`native`, `whistle`, `mock`), `p` its confidence, `conv` the conversation
the speech started. The same words are also events (`conv.word`) so an event summary needs only one
file. A transcript of a killed recording is read up to its torn last line.

## 6. The cut list (EDL) and the export

An export turns the session into a video. It is split in two on purpose, so that a deterministic
exporter ships first and an editing agent plugs in later without changing the renderer:

```text
session folder  -->  EDL generator  -->  cut list (JSON)  -->  renderer (ffmpeg)  -->  mp4
                      v1 identity           ONE schema           one command builder
                      v2 enhancement
                      v3 the editing agent
```

The agent never sees video: it reads `manifest.json`, `transcript.jsonl` and an **event summary**
(counts and spans per kind, the files visited with the time spent in each, the strokes' boxes, the
conversations and their outcomes, the markers) and returns an EDL.

### The cut list (schema 1)

```json
{
  "schema": 1,
  "session": "2026-11-26_10-00-00",
  "title": "Reviewing 3.5 million lines of code in two weeks",
  "output": {"width": 1920, "height": 1080, "fps": 30, "video_kbps": 8000, "audio_kbps": 160},
  "style": {
    "background": "#101010",
    "screen": {"fit": "contain"},
    "camera": {"corner": "bottom_right", "scale": 0.22, "margin": 24, "shape": "rect"}
  },
  "segments": [
    {
      "id": "s1",
      "in_us": 0, "out_us": 93000000,
      "camera": true,
      "speed": 1.0,
      "zoom": {"x": 0, "y": 0, "w": 2880, "h": 1800, "ease_ms": 400},
      "audio": {"gain_db": 0.0, "crossfade_ms": 60},
      "caption": {"text": "this function leaks", "words": [{"w": "this", "t0_us": 1500000, "t1_us": 1700000}]},
      "title": "src/lib.rs"
    }
  ],
  "chapters": [{"t_us": 0, "title": "src/lib.rs"}],
  "captions": {"burn": false, "font_px": 42},
  "generator": {"name": "identity", "version": 1}
}
```

* `in_us` / `out_us` are on the **session clock**; the output is the segments one after the other, a
  segment lasting `(out_us - in_us) / speed`. Pauses fall out by themselves: a segment never spans
  a gap between two runs.
* `zoom` is a rectangle of the **screen track in its own pixels** (the pen's and pointer's events
  say where the action is); the renderer crops and scales it to the output, easing from the previous
  segment's region over `ease_ms`.
* `audio.crossfade_ms` is the length of the audio crossfade into this segment from the one before
  (the segments' sound overlaps by that long; the picture cuts). Every cut of v2 has one, so
  nothing clicks.
* `caption.words` carry the **word-level timing** (session clock), so the renderer can show a
  caption word by word and a later step can move a cut to a word boundary.
* `chapters` become MP4 chapter metadata (v1: one per file the review opened, from `nav.file`).
* A version-3 field is never required: a generator writes what it knows, the renderer takes
  defaults for the rest, and a field it does not know is an error naming the field.

### v1: the deterministic export (built: `recorder_edl.rs`, `recorder_export.rs`)

`identity_edl(manifest, file_changes)` makes the cut list that needs no judgement: one segment for each
run (so a pause is not in the video), the screen as the base, the camera as a picture-in-picture in the
corner where it was recorded, the microphone, a chapter for each file the review opened (`nav.file`), the
picture the screen's shape up to 1920 x 1080. `Edl::parse` reads a cut list strictly (an unknown field is
an error that names it) and checks it against the session: segments inside one run, a zoom inside the
screen track, a crossfade no longer than half a segment, chapters in order.

The renderer plans the export as TEXT first (`build_plan`, a pure function, tested as strings) and then runs
it (`run`). Each segment becomes one ffmpeg command that makes a piece `seg_<id>.mp4`: a background colour
of the cut's length, the screen's slices (one file, or several joined by `concat`) cropped to the zoom,
scaled into the frame and held on their last picture (`tpad`) because a screen that did not change is a
last picture, the camera's slices the same, scaled to the camera's width and overlaid in its corner, the
voice's slices with `aresample`, the delay of a track that began late, the gain, and `atempo` for the speed;
a track that was not recording at all gives silence (`anullsrc`). The picture is composed at the recording's
speed and the speed applied last. All pieces have the same encoding (`h264_videotoolbox`, the Mac's video
hardware, when this ffmpeg has it, else `libx264`; AAC), so one last command joins them with `-c copy` and
gives the file its chapters and its title (`ffmetadata`). Pieces, not one big graph: a session is hours long
and a graph over every file would hold them all open; a piece that fails is named; the pieces are the unit
the later versions' crossfades will work on. A cut that spans a resize of the window is refused in words
(ffmpeg cannot join pictures of two sizes: cut the list at the resize). `ffmpeg` is
`/opt/homebrew/bin/ffmpeg` unless the settings say another. A failed or cancelled export leaves no half-made
video; progress comes from ffmpeg's own `time=` lines. The end-to-end test records a session on stand-in
encoders that replay real H.264 and Opus, makes the cut list, exports it with a real ffmpeg and checks
streams, length, chapters and a clean decode with ffprobe.

### v2: enhancement (designed, not built)

Still deterministic given the transcript. Cuts the filler words, trims long pauses, crossfades every
cut, burns in captions.

* **Fillers.** A per-language list matched on the transcript's words, with bounds on the word's
  probability and duration. Measured with TTS voices on this Mac (indicative; not human speech):
  Whistle emits "um" and "uh" in English by default with word timings in 80 ms steps and
  probabilities (5 of 5 in a test sentence: um 0.80-1.04 p 0.73, uh 2.00-2.40 p 0.74); keyword
  biasing changed nothing there and once derailed the decode. German "äh" / "ähm" are dropped or
  misheard without biasing ("ähm" -> "im" p 0.33, "äh" -> "er" p 0.15) and appear with
  `keywords=[äh, ähm, ähh, ehm, uhm]` and language `de` (4 of 4, p 0.68-0.97). The native
  recogniser drops fillers in both languages, but its word timings leave a GAP where the filler was
  (0.51 -> 1.11 s). So: a word on the list is a candidate; **where an engine drops them, a gap of
  0.2 s or more between consecutive words is a candidate too, and the audio level in the gap is
  checked before cutting** (a gap that is silence is a pause, not a filler); always crossfade.
* **Pauses.** Silence longer than a threshold is shortened to a fixed length; the level comes from
  the audio, not from the transcript.
* **Captions.** Burned in, rendered by azul's own CPU renderer (ffmpeg here has no `drawtext` and no
  `subtitles` filter: no libass, no freetype) as RGBA overlay frames, one per change of caption,
  composited with ffmpeg's `overlay` or by compositing frames and piping them to ffmpeg's stdin. That
  also keeps the look under our control and works with a plain ffmpeg.

### v3: the agent (designed, not built)

An editing task in the agent settings (next to exploration and editing; the backend is the gear's
choice) reads manifest + transcript + event summary and returns an EDL: highlights, structure,
titles, zooms to where the pen worked. The same schema, the same renderer.

## 7. Getting the screen: three sources, one track

The screen is a track of the session like the others, and the review's data (the events, the strokes, the
transcript, the diffs) never depends on it: a session whose screen was refused, or failed halfway, is still
a complete session. The setting `recorder-screen-source` picks where the pictures come from; all three feed
the same track (`recorder_screen.rs`) with the same pacing: a change is sent at most 10 times a second, the
same picture three more times after it for sharpness, a keyframe every two seconds, nothing while the
picture is still. The manifest's `source` says which it was.

* **`display`, the default: the whole display.** What the person sees, whatever app is in front (the video
  is the supplement; the person switches to a browser to show something), the cursor in it. The system's
  capture (ScreenCaptureKit; azul's `ScreenCaptureWidget`); it needs the Screen Recording permission (a
  bare binary started from Terminal is attributed to Terminal, an `.app` to the app, and macOS 15 asks again
  from time to time). The display is the one AzReview's window is on when the session starts, or the one
  named by `recorder-screen-display` (the system's display id), and its name is the track's `device`. The
  picture is scaled by the system to at most `recorder-screen-width` pixels wide (1920 by default) at the
  display's own aspect, so a Retina display's 2560 x 1664 is recorded as 1920 x 1248. Apps in
  `recorder-screen-exclude` (bundle ids; the Notification Center by default, so other people's messages are
  not in the video) and the recorder's own preview window are left out of the capture.
* **`window`**: one window, AzReview's own, from the same capture; nothing else on the desktop is in it.
* **`own`**: AzReview's own presented frames, what the window painted and not what the OS shows: no
  permission of the system, nothing else on the desktop, pixel-perfect at device scale. The engine's frame
  tap (built: `layout/src/frame_tap.rs`, `CallbackInfo::set_frame_tap`) is called by the CPU renderer
  (`CpuBackend::render_frame`, one implementation for macOS, X11, Wayland, Windows and headless) for every
  frame that changed, with the size in device pixels, the stride, the scale, the damaged rectangles (or "the
  whole frame") and the pixels, borrowed for the call; the first frame of a size is told whole, a tap
  registered on a window at rest makes the next frame, a frame that changed nothing is not told. The
  recorder copies only the damaged rows into a shadow frame (`Recorder::tap`) and snapshots it to the encoder
  from its 100 ms timer. Measured on this Mac (Apple M2, loaded): a 400 x 300 damage rectangle 0.013 ms, a
  whole 2880 x 1800 window 0.5 ms. It exists for a window on the CPU renderer only (a window on the GPU has
  no CPU frame; AzReview's window is on the GPU by default on a Mac, so `display` and `window` are the
  sources there).

**The preset** of a display or window capture is modest on purpose: at most 10 frames a second, only when the
picture changed, the same picture three more times for sharpness, a keyframe every two seconds, a target of
2.5 Mbit/s at 1920 x 1200 (`Standard`; 1.25 `Low`, 5 `High`; scaled by the number of pixels, between a quarter
and double), which comes to 2.6 Mbit/s for the 1920 x 1248 picture of a 2560 x 1664 display and 0.7-0.9 GB an
hour of a busy desktop (section 9).

**When the screen goes away.** A display that sleeps or is unplugged is a *gap*, not the end of the track:
`Recorder::screen_gap(reason)` ends the screen track's open segment at that moment, the next picture begins
another at a keyframe, a `recorder.screen_gap` event says why, and the export shows the hole as time that was
not recorded. The other tracks never notice. A screen whose stream stops for good (the permission was
withdrawn, the stream died) is `Recorder::track_failed(Screen, reason)`: the segment ends, the manifest says
`failed` with the reason, a `recorder.track` event and a notice say it, pictures that still come are not
taken, and the camera, the voice and the events go on to the end of the session.

**What the desktop is doing** is polled by the app a few times a second (`Recorder::desktop_sample`) and
needs no permission of the system: the app in front (`app.front`, once for each change, so the export can cut
chapters at app switches), the pointer on the desktop (`pointer.global`, at most `recorder-pointer-hz` times a
second, 10 by default, and not when it moved less than a point, so the export can zoom to the cursor outside
AzReview), whether the screen is locked (a lock pauses the recording, `session.pause` with reason `locked`,
and the unlock goes on, but only after a pause the lock made), and whether the display is asleep (a gap).

## 8. Permissions, quota, privacy

* **The default screen track records everything on the display** while it runs: every app in front, every
  window, whatever is on it. The recorder panel says so in those words; the indicator is always visible; a
  hotkey pauses the recording while another app is in front; a locked screen pauses it by itself
  (`recorder-pause-on-lock`); apps in `recorder-screen-exclude` and the recorder's own camera preview are not
  in the picture (the preview is a window of its own that the capture leaves out, so the camera is not in
  the picture twice). The `own` source records only the review.
* **Screen Recording** (TCC) is attributed to Terminal for a bare binary and to the app for an `.app`; the
  panel's status words name which. A refused or pending permission does not stop anything else: the track is
  `denied` (or `failed` when it was withdrawn later) with the words that say what to do, in the manifest, the
  indicator and a `recorder.track` event, and the review goes on without the video.
* The camera and the microphone are the system's (TCC). A binary started from Terminal is attributed to
  Terminal (the prompt names it); an `.app` needs `NSCameraUsageDescription` and
  `NSMicrophoneUsageDescription` in its Info.plist (`[package.metadata.bundle].usage` in
  `Cargo.toml`: without the camera key macOS ends the app at the first camera access). A track that
  is denied does not stop the others: `status: "denied"` and a `note`, in the manifest, the
  indicator and a `recorder.track` event.
* The quota: the recorder keeps at least `min_free` bytes free on the disk (default 10 GB) and pauses
  the recording (`session.quota_pause`) when it would go under, with a warning before; the panel
  shows the free space and the hours left at the recording's current rate.
* Idle: after N minutes without input, speech or change the recorder pauses itself
  (`session.idle_pause`) and resumes on activity.

## 9. What it costs

Measured on this Mac (Apple M2, 8 GB, other builds running), ffmpeg's VideoToolbox as a proxy for
azul's `VideoEncoder`: 240 frames of 2880 x 1800 scrolling text (the whole window changing 30 times
a second, the worst case): hardware H.264 adds 0.3 s of CPU per 8 s of video (about 3% of a core) at
2.7 Mbit/s for a 3M target; x264 `veryfast` adds 7.3 s (90% of a core). A static text frame is
PSNR 41 dB at 3M at once and 46-47 dB after twenty repeats, which is why a burst is followed by three
repeats. Per hour: screen 0.1-0.4 GB typical (1.2 GB when scrolling the whole time), camera 720p15 at
800 kbit/s 0.36 GB, microphone 0.014 GB.

The recorder's own code, measured by the ignored test
`a_simulated_session_of_ten_minutes_is_priced_in_cpu_seconds_per_hour_and_memory` in `recorder_test.rs`
(`cargo test --release -p AzReview --lib -- --ignored --nocapture a_simulated_session`, macOS): a simulated
600-second session run as fast as it goes at a mock clock with stand-in encoders that keep no picture (so
the encoders' own cost is not in it), a 2880 x 1800 window, a 1280 x 720 camera at 15 pictures a second and
the voice in 20 ms chunks. Two runs on this Mac (Apple M2, 8 GB, other builds running), release build:

| the window | CPU per hour of recording | of one core | peak memory |
| --- | --- | --- | --- |
| the whole of it changes every tick (the worst case) | 82-98 s | 2.3-2.7% | 77-78 MB |
| a pen stroke: 400 x 300 changes in two ticks of ten | 26-32 s | 0.7-0.9% | 77-78 MB |
| nothing changes (camera and voice running) | 1.1-1.2 s | 0.03% | 75-81 MB |

Nearly all of the first two is the copy of the shadow frame that is handed to the encoder (20.7 MB for
each picture sent, at most 10 a second and the three repeats of a change): ten copies a second at about
10 GB/s is the 2%. Lending the encoder a borrowed slice instead of a `RawFrame` would remove it; it is the
first thing to do if these numbers have to go down. With the hardware encoder's measured 3% of a core in
the worst case on top, a recording costs about 6% of a core when the whole window changes all the time
and 1-2% when it is used as a review is used. The peak memory is the test's own window (21 MB), the
recorder's shadow frame (21 MB) and one picture in flight.

**The display capture** (the default screen source), measured on this Mac with a small native
ScreenCaptureKit probe (not azul's widget: nothing recorded or saved, no pixel read, the frames dropped or
handed to VideoToolbox as the sample buffers came), the built-in display of 2560 x 1664 pixels, cursor shown,
15 frames a second at most, 30 s each (15 s capturing alone, 15 s capturing and encoding). **The desktop was
busy** (other builds were scrolling and about 12 frames a second changed), so these are the numbers of a busy
desktop, one run each, not of a quiet review:

| the picture | the probe: capture alone, with VideoToolbox | WindowServer, against 16.5% of a core without any capture | replayd | encoded |
| --- | --- | --- | --- | --- |
| 2560 x 1664, the display's own size | 0.1%, 0.5% of a core | +8% of a core | +1.1% | 2.0 Mbit/s, 0.92 GB an hour |
| 1920 x 1248, the default | 0.2%, 0.4% | +4% | +1.0% | 1.5 Mbit/s, 0.67 GB an hour |

So the capture costs about 9-10% of a core at the display's own size and 5-6% scaled, mostly in
WindowServer, not in AzReview. ScreenCaptureKit's sample buffer (an IOSurface-backed NV12 pixel buffer) went
straight into `VTCompressionSessionEncodeFrame` with no copy at all: 192 pictures, none refused, 7 keyframes in
15.7 s. azul's own path copies it three times (the capture slot, a `Vec`, the encoder's buffer): about 10 MB a
picture at 1920 x 1200, about 1% of a core at 15 frames a second.

azul's own encoders on this Mac (VideoToolbox in hardware, AudioToolbox's Opus), fed a made-up moving picture
and a made-up tone and nothing from any device: 640 x 360 H.264, 60 pictures, 2.00 s, 127,719 bytes
(511 kbit/s); Opus, 48 kHz mono, 2.00 s, 10,107 bytes (40 kbit/s with the container). `avmediainfo` on both
files: "Movie analyzed with 0 error", "System support for decoding this track: Yes"; ffprobe agrees; the
VideoToolbox stream carries its colour tags itself.

## 10. Combining the tracks by hand (ffmpeg, HandBrake)

The exporter (section 6) puts a session together by itself. This is the same thing by hand, for a session of
one run without pauses (a pause is time that was not recorded: concatenating the files drops it from every
track alike, but the tracks then differ from what the app's export makes, which cuts at the session clock).
The three tracks begin together only when they were started together; otherwise put `-itsoffset SECONDS`
before the input that began later (a track's start is `tracks.<name>.segments[0].start_us` in
`manifest.json`, in microseconds).

### By hand: the screen with the camera in the bottom right corner and the voice

```sh
cd "$SESSION"          # the folder of one session, e.g. .../review/recordings/2026-11-26_10-00-00
printf "file '%s'\n" screen/*.mp4 > screen.txt
printf "file '%s'\n" camera/*.mp4 > camera.txt
printf "file '%s'\n" audio/*.mp4 > audio.txt
ffmpeg -f concat -safe 0 -i screen.txt -f concat -safe 0 -i camera.txt -f concat -safe 0 -i audio.txt \
  -filter_complex "[1:v]scale=384:-2[cam];[0:v][cam]overlay=W-w-24:H-h-24[v]" \
  -map "[v]" -map 2:a -c:v h264_videotoolbox -b:v 4M -c:a aac -b:a 128k -movflags +faststart combined.mp4
```

The camera is scaled to 384 pixels wide (a fifth of a 1920-wide screen), put 24 pixels from the bottom and
right edges, and encoded once with the Mac's hardware encoder (`h264_videotoolbox`; elsewhere use `-c:v
libx264 -preset veryfast -crf 23`); the voice becomes AAC. Without the camera there is nothing to overlay, and
the screen files join without re-encoding: `ffmpeg -f concat -safe 0 -i screen.txt -c copy screen.mp4`. A test
(`recorder_export_test.rs`) runs this exact block on a recorded session and checks the result.

### HandBrake

HandBrakeCLI is installed on this Mac (`/opt/homebrew/bin/HandBrakeCLI`). It reads the recorded files, the
H.264 and the Opus audio, but only where a picture is in the file: it opens a file with video and lists its
audio tracks, and it does not open an audio-only MP4 at all ("unrecognized file type", Opus or AAC). So
`screen/NNNN.mp4` and `camera/NNNN.mp4` open, `audio/NNNN.mp4` does not. A scan of a picture and an Opus track
in one MP4 lists the track as "Opus, 1.0 ch, 32 kbps", and a transcode of it decoded every Opus packet and
every picture with 0 decoder errors (output H.264 and AAC). HandBrake transcodes one file; it does not put one
video over another, so the overlay is ffmpeg's. To give HandBrake the camera with the voice:
`ffmpeg -i camera/0001.mp4 -i audio/0001.mp4 -c copy camera-with-voice.mp4` (no re-encoding). The voice inside
the camera file as recorded (one two-track file) is a later slice.

Why Opus and not AAC in the recorded files: ffprobe, ffmpeg (decodes clean), `avmediainfo` ("System support
for decoding this track: Yes", macOS 15.5) and HandBrakeCLI read Opus in MP4, and azul has no AAC encoder (its
`AudioEncoder` is AudioToolbox's Opus). Some players and older systems do not read Opus in MP4, which is why
the export writes AAC.

## 11. Follow-ups (not built)

* The encoder takes a `RawFrame` that owns its bytes, so every picture sent is a copy of the shadow frame
  (20.7 MB at 2880 x 1800). Lending it a borrowed slice removes it (section 9 prices it: most of the recorder's
  own cost when the whole window changes).
* The capture's sample buffer goes into VideoToolbox without a copy (section 9, measured); azul's path copies it
  three times. An `encode` that takes the capture slot's pixel buffer straight, inside the dll, removes them.
* The voice inside the camera file as recorded: a joint two-track fragmented MP4 (H.264 and Opus in one file,
  the audio's fragments interleaved with the picture's at the chunk boundaries), with its crash recovery.
* A readback of the GL window's framebuffer for the `own` source on a window that is on the GPU.

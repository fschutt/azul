# AzReview's session recorder

AzReview can record a whole review session: the screen (its own window, pixel for pixel), the
camera, the voice, and everything the review did (the pen, the navigation, the conversations, the
diffs, the whiteboard), all on ONE clock, to disk, so that a later step can cut them into a video
to share. This file is the contract between the recorder (`recorder*.rs`), the exporter and an
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
    screen/0001.mp4 ...  H.264, AzReview's own frames, a frame only when the screen changed
    camera/0001.mp4 ...  H.264, the camera
    audio/0001.mp4  ...  Opus, the microphone
```

The recordings folder is a setting (an external disk is fine). Nothing is written to the source files
of the review and no other window or desktop content is ever captured by the default screen track.

### Segment files

A track is a list of **segment** files, each about five minutes. A segment is a *fragmented MP4*
(`recorder_mp4.rs`): `ftyp` + `moov` first (the track's description, no samples), then a `moof` +
`mdat` pair about every two seconds (a new fragment at the first keyframe after a second, never longer
than four), and, when the segment was closed properly, an `mfra` index at the end. After a crash a
file is whole up to its last complete fragment: the recorder cuts it there at the next start
(`state: "recovered"`, `segments[].recovered: true`) and loses at most the last two seconds of each
track. Each segment starts at a keyframe and has its own `start_us` on the session clock.

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
| `session.pause` | 5 | `reason` (`user`, `hotkey`) |
| `session.resume` | 5 | `reason` (`user`, `hotkey`, `activity`) |
| `session.idle_pause` | 5 | `idle_ms` |
| `session.quota_pause` | 5 | `free_bytes`, `min_free_bytes` |
| `layout.window` | 2 | `w`, `h`, `scale`: the logical size and the device scale |
| `layout.toggle` | 5 | `panel` (`tree`, `whiteboard`, `agents`, `recorder`, `notes`), `open` |
| `nav.workspace` | 2 | `root`, `files` |
| `nav.file` | 5 | `file`, `lines`: a file was opened (chapters come from these) |
| `nav.page` | 10 | `file`, `page`, `of` |
| `nav.lines` | 5 | `file`, `first`, `last`: the lines in view |
| `pointer.move` | 20 | `x`, `y` (window), `page`, `px`, `py` (on the page), `pen`, `down`: for zooming to the cursor |
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
| `wb.arrow` | 20 | `id`, `from`, `to` |
| `wb.jump` | 10 | `id`, `file`, `first`, `last` |
| `marker` | 5 | `label`: a chapter the user marked with the hotkey |
| `recorder.track` | 5 | `track`, `status`, `note` |
| `recorder.segment` | 5 | `track`, `file`, `start_us` |
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

### v1: the deterministic export (built first)

`identity_edl(manifest, events)` makes the cut list that needs no judgement: the screen as the base,
the camera as a picture-in-picture in the corner, the microphone, chapters from `nav.file`, no cuts
(one segment per run). The renderer (`recorder_export.rs`) turns an EDL and the manifest into
ffmpeg command lines (a pure function, unit-tested as text), runs them, and reports progress: inputs
are the segment files that overlap each cut; the graph is `trim` / `setpts` / `fps` / `scale` /
`overlay` / `concat` for the picture and `atrim` / `aresample` / `acrossfade` / `concat` for the sound;
encoder `h264_videotoolbox` (hardware), `libx264` where it is missing. `ffmpeg` is
`/opt/homebrew/bin/ffmpeg` by default and a setting. The e2e renders a mock cut list.

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

## 7. Getting AzReview's own frames: the frame tap

The screen track is what the window presented, not what the OS shows: no screen-recording permission,
nothing else on the desktop, pixel-perfect at device scale. An additive azul API,
`CallbackInfo::set_frame_tap`, registers a callback that the CPU renderer (`CpuBackend::render_frame`,
one implementation for macOS, X11, Wayland, Windows and headless) calls when a frame changed, with
the size in device pixels, the stride, the format (RGBA8), the scale, the damaged rectangles (or
"everything") and the pixels, borrowed for the call. It is not called when nothing changed.

The recorder copies only the damaged rows into a shadow frame and snapshots it to the encoder from
its 100 ms timer. Measured on this Mac (Apple M2, loaded): a 400 x 300 damage rectangle 0.013 ms, a
whole 2880 x 1800 window 0.5 ms (into a reused buffer; 1.2 ms into a fresh allocation). On the UI
thread a frame costs about 0 when idle, 0.01 ms while inking, at most 1 ms when the whole window
changes. A window on the GPU path is recorded from a ScreenCaptureKit window capture of AzReview's own
window (the existing `ScreenCaptureWidget`; Screen Recording permission), later from a pixel-buffer
readback before the buffers swap.

## 8. Permissions, quota, privacy

* The screen track from the frame tap needs no permission.
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

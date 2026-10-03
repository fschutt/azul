# MEDIA9 progress (branch wt/media9, base e537ddbe2)

Task: audio decoding into AudioSink (symphonia), media widgets (MediaControls, SeekBar, Waveform,
LevelMeter - AzMeet's meter moves into azul), AzMusic, AzPlayer. Never compile. RED first.
Report: scripts/MEDIA9_2026_10_03.md (date = the day it finishes).

## DONE
- f778be57d progress file; dec0b59ae design notes
- A1 df5870677 RED / af26cfbae GREEN playback.rs (Rechunker, LinearResampler, remix, apply_gain,
  chunk_peaks, LevelHistory, TrackClock, RealTimeClock) - 12 tests run standalone with rustc: pass
- A2 99fe01d4f RED / 67ccbdd80 GREEN AudioSink try_play / queued_frames / samples_played / pause /
  resume / clear / config; 25596e1f1 AVF + cpal + ALSA backends implement the seam

- A3 a44e8a586 RED / 1a51a1205 GREEN decode.rs AudioFileDecoder + AudioFileInfo on symphonia 0.6.1
  (feature audio-decode in dll/Cargo.toml + build-dll); 3a56ca085 / 4dd0d1818 waveform peaks
  (layout/src/widgets/waveform.rs WaveformPeaks + resample_peaks). decode.rs compiled with rustc
  against real symphonia (built in /tmp by /tmp/media9_tools/build_symphonia.sh) + stand-in azul
  crates (/tmp/media9_tc/stubs): 5/5 tests pass. Harness: bash /tmp/media9_tools/test_decode.sh

- A4 c6c94e29a RED player.rs: AudioPlayerState, PcmSource / PcmOutput seams (FileSource and
  AudioSink implement them), PlayerCore stubs + 8 tests (player_tests, FakeOutput, Ramp).

- A4 ab12de5a2 GREEN PlayerCore (player.rs + playback.rs: 20 tests pass standalone with rustc;
  harness /tmp/media9_tc/stubs/player_harness.rs, command in the commit message).

- A4 090ca6ea4 RED / 8660cac78 GREEN AudioPlayer handle (thread, Mutex+Condvar commands); b23174f54
  AudioFileInfo + AudioPlayerState moved to core/src/audio.rs (dll re-exports), wasm stubs of
  AudioFileDecoder + AudioPlayer in dll/src/unified/audio.rs. Full rustc harness (29 tests: playback,
  decode with real symphonia, PlayerCore, handle in real time) passes:
  /tmp/media9_tc/stubs/full_harness.rs (rebuild stubs + symphonia: /tmp/media9_tools/build_symphonia.sh).
  PHASE A (engine) DONE.

- B1 12b563ba7 RED layout/src/widgets/level_meter.rs (db_percent, rms_percent, peak_percent,
  zone_widths, LevelMeterOrientation, LevelMeter, LevelMeterThrottle, LevelMeterLook; 8 tests).

- B1 6dfd00c90 GREEN LevelMeter (level_meter.rs + flat/flora appends + manifest/INPUTS).

- B1b b652ce195 AzMeet uses LevelMeter + LevelMeterThrottle (mic_level_percent / meter_moved_ms gone).

- B2 34b36c681 RED layout/src/widgets/seek_bar.rs (types, callback triple, 6 tests).

## IN PROGRESS
- NEXT STEP: GREEN seek_bar.rs (media_time, seek_fraction, time_at, key_target, build + look,
  pointer / key callbacks on the track with a SeekBarWrapper dataset {on_seek, inner, chapters?},
  merge callback keeping `dragging`, update_position, flat/flora appends, manifest INPUTS).
  Original B2 design notes: layout/src/widgets/seek_bar.rs (append `pub mod seek_bar;` after
  `pub mod level_meter;` in widgets/mod.rs). Design: `media_time(seconds) -> String` ("m:ss",
  "h:mm:ss" past an hour, "-" for unknown/NaN; the ONE media clock helper - timeline.rs tick_label
  has the same branches, note the twin); SeekBar { position_s f64, duration_s f64, buffered_s f64,
  chapters F64Vec (chapter start times; check an F64Vec exists, else F32Vec), show_times bool,
  on_seek OptionSeekBarOnSeek (callback triple like Slider's: extern fn(RefAny, CallbackInfo,
  SeekBarState) -> Update), accessibility_name, theme }; SeekBarState { position_s, duration_s,
  dragging bool }. DOM: row [time label p, track (grow 1, height 6, relative: played fill width %,
  buffered fill, chapter ticks (absolute left %), thumb (absolute left % - 6px)), duration label p].
  Pointer: down on the track -> dragging + seek to cursor x / width * duration, move while dragging,
  up ends (report on up with dragging false, so an app seeks the player once on release and moves
  only the thumb while dragging - both reported, state.dragging tells). Keys: Left/Right -5/+5 s
  (Shift: 30 s? use primary_down for 10% like Slider), Home/End; a11y Slider role, value
  "1:12 of 9:22". merge callback carries dragging across rebuilds (like merge_slider_state).
  `SeekBar::update_position(info, node, position_s)` moves fill + thumb + label in place (a player
  ticks it 4x a second without a rebuild). Look struct + flat/flora appends + manifest INPUTS.
- then B3 MediaControls (prev / play-pause / next, optional shuffle / repeat toggles, -15/+30 skip
  (podcast), volume Slider; variants compact / large / overlay; one on_action callback with a
  MediaControlsAction enum + value; Buttons (use existing Button + icons), a11y Toolbar role) and
  B4 the Waveform widget part (bars from peaks, played part in accent, click/drag seeks via the
  same SeekBarState callback type - reuse SeekBar's callback triple, no twin).

## api.json so far (for the report)
- audio.AudioFileDecoder (external azul_dll::unified::audio::AudioFileDecoder, Clone Default Drop,
  struct_fields ptr c_void mutptr / error OptionString / run_destructor bool, repr C):
  constructors open(path: String), create(bytes: U8Vec, extension: String); functions
  backend_name() -> String (static), is_open, error_message -> OptionString, info -> AudioFileInfo,
  next_frame (refmut) -> OptionAudioFrame, seek (refmut, position_s f64) -> bool, position_s -> f64,
  waveform (refmut, buckets u32) -> F32Vec, close (refmut).
- audio.AudioFileInfo (external azul_core::audio::AudioFileInfo, Clone Debug PartialEq Default,
  repr C, fields in order: title artist album album_artist genre date lyrics codec container
  cover_mime (String) cover (U8Vec) duration_s f64 sample_rate track_number track_total
  disc_number u32 channels u16); option.OptionAudioFileInfo.
- audio.AudioPlayerState (external azul_core::audio::AudioPlayerState, Copy Clone Debug PartialEq
  Default, repr C: position_s duration_s f64, track failed_track u64, volume peak_left peak_right
  f32, queued_tracks u32, playing finished has_output bool).
- audio.AudioPlayer (external azul_dll::unified::audio::AudioPlayer, Clone Default Drop, fields ptr
  / run_destructor): constructor create(); functions load_file(path String) -> u64,
  load_bytes(bytes U8Vec, extension String) -> u64, queue_file, queue_bytes (same), clear_queue,
  play, pause, toggle, stop, seek(position_s f64), skip, set_volume(volume f32) (all ref),
  get_state -> AudioPlayerState, error_message -> OptionString, close (refmut).
- audio.AudioSink new functions (all ref): try_play(frame AudioFrame) -> bool, queued_frames ->
  u64, samples_played -> u64, pause -> bool, resume, clear -> bool, config -> AudioConfig.

## NEXT (in order)
- A1 playback.rs pure pieces: Rechunker, LinearResampler, remix (channels), apply_gain,
  LevelHistory, TrackClock (gapless boundaries). RED then GREEN.
- A2 AudioSink: try_play / queued_frames / samples_played / pause / resume / clear; OutputDevice
  defaults; synthetic sink plays in real time; AVF + cpal + ALSA implement. RED then GREEN.
- A3 decode.rs AudioFileDecoder (symphonia; feature `audio-decode`): info + tags + cover, frames,
  seek (trim to required_ts), waveform peaks; Opus packets through AudioDecoder. RED (hand-made
  WAV + FLAC with Vorbis comments in the test) then GREEN.
- A4 player.rs AudioPlayer: PlayerCore::pump (testable step) + thread; gapless queue; state.
- A5 wasm stubs (unified/audio.rs), Cargo.toml feature, mod.rs lines.
- B widgets: LevelMeter (+ AzMeet migration), SeekBar, MediaControls, Waveform (look structs,
  flat/flora appends, manifest, tests).
- C AzMusic (examples/azul-music). D AzPlayer (examples/azul-player). E2E scripts. Report.

## Decisions
- HOUSE RULE (coordinator, 2026-10-03): never send the user's email or any personal data to an
  outside service (User-Agent, URL, payload). For crates.io use the User-Agent
  `azul-build-agent (https://github.com/fschutt/azul)`. (One earlier crates.io API call this run
  used the UA "media9-agent (felix)"; no more such calls are planned.)
- symphonia 0.5 (MPL-2.0, pure Rust: MP3 / AAC / ALAC / FLAC / Vorbis / WAV / PCM, containers
  MP4 / Ogg / MKV / WAV) in dll behind a new feature `audio-decode` (added to build-dll). Opus
  inside Ogg / MKV / MP4 is demuxed by symphonia and decoded by our AudioDecoder (AudioToolbox)
  where it opens; elsewhere an Opus file says "no Opus decoder here".
- New engine handles: AudioFileDecoder (a decoded FILE: info, tags, cover, frames, seek, waveform
  peaks - named apart from AudioDecoder, the Opus PACKET decoder) and AudioPlayer (a decode
  thread feeding an AudioSink: gapless queue, seek, pause, volume, levels, state).
- AudioSink grows try_play / queued_frames / samples_played / pause / resume / clear so a player
  can pace itself, pause instantly and seek without hearing the old queue; the synthetic headless
  sink plays in real time (wall clock) so a headless player's position advances.
- Resampling: a linear resampler (pure) when the device refuses the file's rate or the next
  gapless track has another rate (the sink stays open: no gap). rubato is the quality upgrade.
- AzMusic on MediaShell (S7, music.md: the now-playing bar is always there) with a DataTable per
  library view; the plan's RecordsShell is S6 (records) - MediaShell is the matching shell.
- AzPlayer on MediaShell::create_player + VideoWidget (the picture, its own decode worker) +
  AudioPlayer (the file's audio track); audio follows the video's reported clock (dead band).

## Open questions
- (none)

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

- B2 e174ff797 GREEN SeekBar (seek_bar.rs, flat/flora appends, manifest INPUTS).

- B3 434685558 RED / 08d778b41 GREEN MediaControls (media_controls.rs, looks, manifest INPUTS).

- B4 b30b92f55 RED / 25cf3510b GREEN Waveform widget (on the seek bar's surface: SeekSurface,
  seek_callbacks, move_surface shared - no twin). PHASE B (widgets) DONE.

- C1 c7e499dec RED / 6a99ac291 GREEN examples/azul-music crate + library.rs; C2 b3dbcaa34 RED /
  227752629 GREEN queue.rs (5 tests pass standalone with rustc).

- C3 701e7d5b3 / 2682bafcd playlists.rs; C4 b6233d001 / ead57eeb4 ids.rs + sample.rs + scan.rs (4 tests
  pass standalone: /tmp/media9_tc/music_pure.rs); 8b2313ee8 LevelMeter::peak_level.

- C5 2d47924c0 AzMusic window (app.rs: state/data/scan/playback/transport; ui.rs: MediaShell layout,
  songs DataTable, lists, now-playing bar, settings section); 4c0352d35 SeekBar::media_time;
  f05a9d324 registered (Cargo.toml members, workspace_test_members.txt, rust.yml dll_tests step);
  c3cd9a2e9 scripts/azmusic_e2e.py. PHASE C (AzMusic) DONE (pending the parent's compile).

- D bf6ba792d / ebb4c59de AzPlayer history.rs + sync.rs; 3b45d318d window (app.rs, ui.rs);
  cc4ecf542 registered; 9d20e684a scripts/azplayer_e2e.py. PHASE D (AzPlayer) DONE.

## IN PROGRESS
- NEXT STEP: the report scripts/MEDIA9_2026_10_03.md (what was built, commits, the api.json list
  below, least-sure spots, test commands, twins, what is left). Write it in pieces, commit each.

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
- widgets.LevelMeter (external azul_layout::widgets::level_meter::LevelMeter, Clone Debug
  PartialEq, repr C: accessibility_name OptionString, theme OptionUiTheme, level f32, orientation
  LevelMeterOrientation): create(level f32); set_level / with_level, set_orientation /
  with_orientation, with_accessibility_name(String), set_theme / with_theme, swap_with_default,
  dom; static level_of(frame AudioFrame) -> f32; static update_level(callback_info CallbackInfo,
  node_id DomNodeId, level f32) -> bool (fn_body like ProgressBar.update_progress: `{ let mut
  callback_info = callback_info; ...::update_level(&mut callback_info, node_id, level) }`).
  widgets.LevelMeterOrientation enum (Horizontal, Vertical; repr C). option.OptionLevelMeter.
- widgets.LevelMeterThrottle (Copy, repr C: interval_ms u64, last_ms u64, level f32, min_step f32,
  moved bool): create(interval_ms u64) (const fn), next(refmut, level f32, now_ms u64) -> OptionF32.
- widgets.MediaControls (repr C: on_action OptionMediaControlsOnAction, accessibility_name
  OptionString, theme OptionUiTheme, volume f32, repeat MediaRepeat, playing shuffle show_skip
  show_shuffle_repeat bool): create(playing bool); set_/with_volume(f32), set_/with_shuffle_repeat(
  shuffle bool, repeat MediaRepeat), set_/with_show_skip(bool), set_/with_on_action (callback
  triple MediaControlsOnAction / OptionMediaControlsOnAction / MediaControlsOnActionCallback /
  MediaControlsOnActionCallbackType = extern fn(RefAny, CallbackInfo, MediaControlsEvent) ->
  Update), with_accessibility_name, set_/with_theme, swap_with_default, dom.
  widgets.MediaControlsAction enum (Previous, PlayPause, Next, SkipBack, SkipForward, Shuffle,
  Repeat, Volume), widgets.MediaRepeat enum (Off, All, One) with next() -> MediaRepeat,
  widgets.MediaControlsEvent (Copy, repr C: value f32, action MediaControlsAction).
- widgets.Waveform (repr C: position_s duration_s f64, peaks F32Vec, on_seek OptionSeekBarOnSeek,
  accessibility_name OptionString, theme OptionUiTheme): create(peaks F32Vec, position_s f64,
  duration_s f64); set_/with_on_seek (the SeekBar's callback types), with_accessibility_name,
  set_/with_theme, swap_with_default, dom; static update_position(callback_info, node_id,
  position_s f64) -> bool. Rust-only: WaveformPeaks, resample_peaks (or export later).
- widgets.SeekBar (external ...::seek_bar::SeekBar, repr C: position_s duration_s buffered_s f64,
  chapters F32Vec, on_seek OptionSeekBarOnSeek, accessibility_name OptionString, theme
  OptionUiTheme, show_times bool): create(position_s f64, duration_s f64); set_/with_buffered,
  set_/with_chapters(F32Vec), set_/with_show_times(bool), set_/with_on_seek (callback triple:
  CallbackType fn_body as the Slider's), with_accessibility_name, set_/with_theme,
  swap_with_default, dom; static update_position(callback_info, node_id, position_s f64) -> bool;
  static media_time(seconds f64) -> String (module fn `media_time`; export as a static method).
  widgets.SeekBarState (Copy, repr C: position_s duration_s f64, dragging bool); callback types
  SeekBarOnSeek / OptionSeekBarOnSeek / SeekBarOnSeekCallback / SeekBarOnSeekCallbackType
  (extern fn(RefAny, CallbackInfo, SeekBarState) -> Update), like SliderOnValueChange's entries.

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

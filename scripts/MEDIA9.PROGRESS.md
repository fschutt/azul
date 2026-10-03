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

## IN PROGRESS
- A4 player.rs (AudioPlayer handle + PlayerCore::pump): RED next.

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

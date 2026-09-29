# M2_AZMEET_AUDIO progress

Branch `wt/m2-azmeet-audio` (base d9ce25179). Worker: `/Users/fschutt/Development/azul-apps-m1`, branch
`cf-workers-meet`.

## Findings before building
- Output API exists: `AudioSink::open(AudioConfig) / play(AudioFrame) / is_open / frames_played / close`
  (dll/src/desktop/extra/audio/mod.rs; ALSA write blocks, AVAudioEngine and cpal queue).
- iroh frames are latest-wins per (peer, track) on both ends; messages are reliable and ordered.
- Under `AZ_BACKEND=headless` only `AudioDeviceList::enumerate` is mocked; `MicrophoneWidget` (mic backend
  registered in the layout pass) and `AudioSink::open` would open real devices.

## DONE
- `916764657` test(azmeet): audio.rs tests, stubs (RED).
- `6aa0ba788` feat(azmeet): audio.rs pure logic (GREEN; type-checked alone with rustc --emit=metadata --test).

## IN PROGRESS
- worker DELETE /rooms/<id>/peers/<node_id> (RED first)

## NEXT
- worker DELETE GREEN; two-clients.mjs audio + leave (RED); lib.rs audio, mute/deafen,
  Leave; guide; report.

## Open questions

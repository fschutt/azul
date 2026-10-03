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
- azul-apps `97aced2` test(meet): DELETE /rooms/<id>/peers/<node_id> (RED).
- azul-apps `1b8ef59` feat(meet): the leave route; `node --test`: 50 pass.
- `0379cf912` test(azmeet): two-clients.mjs hears audio, sees a mute, one leaves (RED; dry run vs a Node
  stand-in: PASS).
- `6b2843642` feat(azmeet): audio over iroh, mute/deafen, Leave (lib.rs; new glue type-checked in a stub
  harness, see the report).
- `165e73eb9` docs(guide): realtime-media AzMeet section + backends; networking two sentences.
- Report `scripts/M2_AZMEET_AUDIO_2026_09_29.md`.

## IN PROGRESS
- nothing

## NEXT (for the parent)
- `node --test` in the Worker; `cargo test -p AzMeet --lib`; build AzMeet + libazul with the debug server; run
  two-clients.mjs (see the report).

## Open questions
- AudioSink / MicrophoneWidget are not mocked under headless (the app guards itself); should the library mock
  them like AudioDeviceList?

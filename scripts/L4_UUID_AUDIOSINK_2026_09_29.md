# L4: `Uuid::from_seed` and an honest `AudioSink::open` - report (2026-09-29)

Branch `wt/l4-uuid-audiosink` from `34a8fe46f`. Nothing was compiled with cargo (house rule). Every touched file
passes `rustfmt --check` (as a parse check). One exception to "run nothing": the uuid module was run once with
`rustc --test` on a scratch copy in the scratchpad (with `type AzString = String`, no cargo, no target dir), and
all 8 tests passed. No file under `examples/` was touched.

## What was built

### 1. `Uuid::from_seed(seed: u64)` / `Uuid::short_from_seed(seed: u64)` (layout/src/uuid.rs)

- `from_seed(seed)` gives the canonical v4-shaped UUID (36 chars) as a pure function of `seed`.
  `short_from_seed(seed)` gives the same 128 bits in flickrBase58 (22 chars). Both return `AzString`, like `v4` /
  `short`.
- Bits: the first two splitmix64 outputs for the seed, `hi = mix64(seed + GAMMA)` and
  `lo = mix64(seed + 2*GAMMA)`. `GAMMA` is `0x9E3779B97F4A7C15`, the same finalizer `mix64` the tick uses.
  `from_seed(0)` = `e220a839-7b1d-4cda-bdb9-e279aa86e597` (hi is splitmix64's known first output
  `0xe220a8397b1dcdaf`).
- Neither function reads or writes any state. The tick is never touched.
- **One path (NO DUPLICATION):** `tick_bits(tick)` and `seed_bits(seed)` both feed one stamp, `v4_shape(hi, lo)`.
  Its output goes to one `hex(u128)` formatter and one `base58(u128)` formatter. `v4` / `short` /
  `from_seed` / `short_from_seed` are each one line.
- **The stamp changed, and so did the tick values:** `v4_shape` now *inserts* the version nibble and the
  variant between payload bits instead of writing over 6 of them. So all 64 bits of `hi` survive (the low 6 bits
  of `lo` make room). That makes "distinct seeds give distinct ids" exact, and the test reads `mix64(seed + GAMMA)`
  back out of the id. It also makes the module doc's old claim true for the tick: it said "exactly", but 4 bits of
  the high word used to be overwritten.
  - Side effect: the `v4()` / `short()` sequence values changed. They are still deterministic, and the first id
    still starts `00000000-0000-4000-8`. Nothing in the repo pins those values; the calendar example uses its own
    ids.
- Docs (module and fn): the randomness comes from the caller (an OS source, a hardware RNG, a server). 64 bits of
  seed are enough for app ids (two random seeds are likely to collide only after about 2^32 ids). The id is not a
  security token, because the mix is invertible and the seed can be read back. Never seed it with a secret.

### 2. `AudioSink::open` reports a failed device honestly (dll/src/desktop/extra/audio)

- **Seam:** a new private trait `OutputDevice { fn play(&self, &[f32]) -> bool }`, implemented by the four
  backends: `AlsaPcm`, `CpalSink`, `AAudioSink`, `AvfSink`.
  - `AudioSinkInner` is now `{ device: Option<Box<dyn OutputDevice>>, frames_played }`. `None` is the headless
    synthetic sink, which works as before.
  - This replaces four cfg'd fields, four cfg'd `play` branches and the cfg'd `engine_ok` warning.
  - `platform_output(config) -> Result<Box<dyn OutputDevice>, String>` is the one cfg switch.
  - `open_on(config, output)` builds a sink from that result. Tests open sinks through it.
- **Closed handle with a reason:** each backend's `open` now returns `Result<_, String>`. The reasons:
  - ALSA not installed, no ALSA device, or the device refused `rate x channels` f32 (with the rc).
  - WASAPI has no default output, the device refused the format (cpal's error text), or the stream did not start.
  - AAudio missing (Android < 8.0), the builder failed, no device took the format, or the stream did not start.
  - AVAudioFormat refused the channel count, or AVAudioEngine did not start.
  - `platform_output` adds "this build has no audio output (built without objc2-avf-audio)" on macOS / iOS
    without the feature, and "no audio output backend" on other targets.
  - On failure `open` returns `is_open() == false` and logs one warn line. `play` does nothing and
    `frames_played` stays 0.
- **`error_message()`** reads a new field `AudioSink.error: OptionString`. `None` while open, after `close`, and on
  a default handle. `Clone` copies it.
  - A headless run's closed handle says `AudioSink::open: not available in a headless run ...
    AZ_SYNTHETIC_DEVICES=audio_sink or the mock op {"audio_sink": "count"}`.
  - That text comes from a new `DeviceKind::unavailable_message()` in `layout/src/request.rs`. The mock store's
    stderr line uses the same function, so its output is byte-identical to before.
  - Field placement: `ptr, error, run_destructor`. The new 8-aligned field sits before the trailing 1-byte bool, so
    there is no padding between fields. That is "at the end" as far as the padding rule allows.
- **`frames_played`** counts only frames a device took. These return false and are not counted:
  - AVF: backlog full, buffer not made, empty frame.
  - cpal: queue full or poisoned.
  - ALSA: write failed after the one recover-and-retry.
  - AAudio: write took 0.

  The synthetic sink counts every frame, as before.
- **wasm stub** (`dll/src/unified/audio.rs`): same layout (`ptr, error, run_destructor`). `open` returns a closed
  handle that says there is no audio output on wasm, and `error_message()` exists.
- **NO DUPLICATION:**
  - ALSA's playback open and capture open were twins. Both now use one `open_pcm(rate, channels, stream)`. The
    capture path still returns `None`, now with one warning that gives the reason.
  - The headless message now has one source (above).
- Docs: the audio module doc, the `extra/mod.rs` line, and `doc/guide/en/system/realtime-media.md` (the
  `AudioSink` section and the backends list).

**Test seam, said plainly:** a real device cannot be forced to fail in a test. The new tests therefore go through
`open_on(config, Err(reason))` and a fake `OutputDevice` (`TakesEveryOther`), plus the existing headless path
`open_as(config, MockDevice::Unavailable / Synthetic)`. The platform `open` functions themselves are untested.

## Commits

- `68baa9bb3` test(uuid): a seeded UUID is a pure function of its seed (RED)
- `9e5c006c7` feat(uuid): Uuid::from_seed / short_from_seed, ids that stay deterministic but take their randomness from the caller
- `ce91a3ee1` docs(l4): progress
- `e34430443` test(audio): a sink whose device does not open is closed and says why (RED)
- `c0739e2d3` fix(audio): AudioSink::open is closed when its device does not open, and says why
- `337f9cbb2` docs(audio): the guide says a sink is open only where a device opened, and error_message says why
- this report and the progress file

## api.json (via autofix; NOT edited by hand)

**Required before building the dll:** the Rust `AudioSink` grew a field, so the generated `AzAudioSink` must match.
Otherwise the size/align test and the transmutes break.

1. `uuid.Uuid.functions` (static, no `self`, like `v4` / `short`):
   - `"from_seed"`
     - `fn_args`: `[{"seed": "u64"}]`
     - `returns`: `{"type": "String"}`
     - `fn_body`: `azul_layout::uuid::Uuid::from_seed(seed)`
     - optional: `"priority": 90.0`
     - `doc`: ["The version-4-shaped UUID that is a pure function of `seed`, in canonical hyphenated lowercase form
       (36 characters): the id for a file name, an S3 key or a record that other processes and devices share,
       where the process-local sequence of `v4` would collide.", "", "The randomness comes from the caller: draw
       `seed` from the OS, a hardware RNG or a server. The same seed gives the same id on every platform and in
       every release; distinct seeds give distinct ids, exactly. It never touches the marker tick.", "", "64 bits
       of seed are enough for app ids, but the id is not a security token: the seed can be read back from it, so
       never seed it with a secret."]
   - `"short_from_seed"`
     - `fn_args`: `[{"seed": "u64"}]`
     - `returns`: `{"type": "String"}`
     - `fn_body`: `azul_layout::uuid::Uuid::short_from_seed(seed)`
     - `doc`: ["`from_seed` as a 22-character flickrBase58 string: the same 128 bits, spelled like `short`. Same
       seed, same id."]
   - Doc refresh (stale today): `v4` / `short` say "A fresh random (version 4) UUID". Better: "A fresh
     version-4-shaped UUID ... Deterministic, not random: a pure function of how many ids this process minted. For
     an id other processes share, use `from_seed`." The class doc can list `from_seed` / `short_from_seed` next to
     `v4` / `short`.
2. `AudioSink.struct_fields`, in this order: `ptr` (unchanged), then the new
   `"error": {"type": "OptionString", "doc": ["Why the sink did not open (`error_message`); None while open, after
   `close` and on a default handle."]}`, then `run_destructor` (unchanged).
3. `AudioSink.functions`:
   - new `"error_message"`
     - `fn_args`: `[{"self": "ref"}]`
     - `returns`: `{"type": "OptionString"}`
     - `fn_body`: `object.error_message()`
     - `doc`: ["Why this sink is not open, readable enough to show the user: no output device, no audio backend in
       this build, the device refused the format, or a headless run without the synthetic sink. None while the
       sink is open, after an explicit `close`, and on a default handle."]
4. Doc refresh (no signature change):
   - `AudioSink.open`: "Open an audio output for `config` (sample rate + channels). The handle is open (`is_open()`)
     only if an output device opened; otherwise it is closed and `error_message` says why: no output device, no
     audio backend in this build, or a device that refuses the format. Playing into a closed handle does
     nothing."
   - `play`: "Hands `frame` (interleaved f32 samples in the frame's format) to the output device. Does nothing on
     a closed handle."
   - `frames_played`: "Frames the output device took from `play` so far. A frame the device did not take (its
     queue full, the write failed) is not counted, and a closed handle counts nothing (0). A headless run's
     synthetic sink counts every frame."

`DeviceKind::unavailable_message` is a Rust-only API in layout; it is not in api.json.

## Least sure to compile

1. `audio/mod.rs` `platform_output`: cfg'd blocks where the one that survives is the tail expression. This is the
   same shape as the existing `enumerate_blocking`. Each block maps with `Box::new(x) as Box<dyn OutputDevice>`.
   The no-backend blocks do `let _ = config;`.
2. `impl super::OutputDevice for X` in the child backend modules for a private trait in the parent. Children can
   see the parent's private items. The test module does the same with `use super::{AudioSink, OutputDevice}`.
3. `cpal_sink.rs`: `map_err(|e| format!(.., config.sample_rate.0, config.channels, e))` needs cpal 0.15's
   `BuildStreamError` / `PlayStreamError` to implement Display, which they do via thiserror.
4. `aaudio.rs`:
   - `open_stream` is still an `unsafe fn` (edition 2021: its body calls fn pointers without inner `unsafe`) and
     now uses `?` on an `Option` via `ok_or_else`.
   - `unsafe { open_stream(..) }?`.
   - `Some(f) => unsafe { (f.write)(..) > 0 }` as a match arm.
5. `alsa.rs` `open_pcm`: `let rc = (f.open)(&mut pcm, .., stream, 0)` passes `stream: c_int` straight through;
   `n` is `c_long` and gets reassigned by the retry.
6. `uuid.rs`: `const fn v4_shape` uses `hi as u128` and u128 shifts in a const fn (both fine), and the tests use
   `(0..10_000_u64).chain(edges)` (array by value, edition 2021). The standalone rustc run covers this file's
   logic, but not the real `AzString`.
7. The ALSA / cpal / AAudio backend edits are cfg'd to Linux / Windows / Android. rustfmt parsed them, but no type
   check has ever seen them.

## Test commands for the parent

```sh
# after the api.json autofix (AudioSink grew a field; the dll will not build before it)
cargo test -p azul-layout --lib uuid            # 8 tests: 4 old + 4 new (determinism + pinned values,
                                                # distinct seeds incl. top-bit pairs, v4/variant bits + same bits
                                                # in both spellings, from_seed never advances the tick)
cargo test -p azul-layout --lib request::mock   # the stderr line now comes from DeviceKind::unavailable_message
cargo test -p azul-dll --lib headless_sink_tests  # 5 tests: 2 from L2 + 3 new (closed handle says why;
                                                # frames_played counts only frames the device took; headless
                                                # closed handle says why, synthetic / default say nothing)
```

Do not run the dll tests with `AZ_BACKEND=headless` set (same caveat as L2). No test opens a real device.

## What is left

1. **api.json autofix** (above). It is required for the dll build.
2. `PlatformCapability::audio_output()` (`capability.rs`) still says AVAudioEngine is available on macOS / iOS
   even without `objc2-avf-audio`, and that Windows is available without a probe. It could reuse the same
   reasons as `platform_output`. Not changed here, to stay in scope.
3. The AVAudioEngine start failure does not include the NSError text. There are two existing twins that describe
   an NSError: `screencap/macos.rs:226` (`localizedDescription` for logs) and `notifications/apple.rs:195`. Both
   are apple-only and outside this task. One shared helper could serve all three.
4. AzMeet (examples, other agents) can show `sink.error_message()` when the playout sink does not open. Its own
   headless / `play_audio` guard can go, as L2 already said.
5. `doc/guide/en/debugging/e2e-testing.md` could add that a headless `AudioSink` also says why through
   `error_message()` (it already says `is_open` is false).

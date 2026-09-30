# L4_UUID_AUDIOSINK progress

Branch `wt/l4-uuid-audiosink` from `34a8fe46f`. Nothing compiled with cargo (house rule). The uuid module was
run standalone once with `rustc --test` on a scratch copy (AzString = String): 8/8 pass.

## DONE

- `68baa9bb3` test(uuid): a seeded UUID is a pure function of its seed (RED)
- `9e5c006c7` feat(uuid): Uuid::from_seed / short_from_seed (one stamp `v4_shape`, one `hex`, one `base58`)

## IN PROGRESS

- 2. `AudioSink::open`: RED tests.

## NEXT

- 2. fix: `OutputDevice` seam (backend `open` -> `Result<_, String>`, `play` -> bool), `error: OptionString`
  field + `error_message()`, closed handle on failure, `frames_played` counts only frames a device took.
- Report `scripts/L4_UUID_AUDIOSINK_2026_09_29.md`.

## Open questions

- none yet

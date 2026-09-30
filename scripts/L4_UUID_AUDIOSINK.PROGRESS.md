# L4_UUID_AUDIOSINK progress

Branch `wt/l4-uuid-audiosink` from `34a8fe46f`. Nothing compiled (house rule).

## DONE

## IN PROGRESS

- 1. `Uuid::from_seed` / `Uuid::short_from_seed`: RED tests.

## NEXT

- 1. fix: one v4 stamp (`v4_shape`), one hex + one base58 formatter, fed by the tick or the seed.
- 2. `AudioSink::open`: RED (a device that does not open = closed handle with a reason; `frames_played` counts
  only frames a device took), then the fix (`OutputDevice` seam, `error` field, `error_message()`).
- Report `scripts/L4_UUID_AUDIOSINK_2026_09_29.md`.

## Open questions

- none yet

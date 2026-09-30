# L4_UUID_AUDIOSINK progress

Branch `wt/l4-uuid-audiosink` from `34a8fe46f`. Nothing compiled with cargo (house rule). The uuid module was
run standalone once with `rustc --test` on a scratch copy (AzString = String): 8/8 pass.

## DONE

- `68baa9bb3` test(uuid): a seeded UUID is a pure function of its seed (RED)
- `9e5c006c7` feat(uuid): Uuid::from_seed / short_from_seed (one stamp `v4_shape`, one `hex`, one `base58`)
- `e34430443` test(audio): a sink whose device does not open is closed and says why (RED)
- `c0739e2d3` fix(audio): AudioSink::open is closed when its device does not open, and says why
- `337f9cbb2` docs(audio): realtime-media guide
- Report: `scripts/L4_UUID_AUDIOSINK_2026_09_29.md`

## IN PROGRESS

- nothing

## NEXT

- parent: api.json autofix (AudioSink.error field + error_message; Uuid.from_seed / short_from_seed), then the
  test commands in the report.

## Open questions

- none

# M6_ROOM_TIMES progress

Branch `wt/m6-room-times` (base `34a8fe46f`); Worker work in `/Users/fschutt/Development/azul-apps-m1`,
branch `cf-workers-meet`.

## DONE
- azul-apps `adfaf66` test(meet): a room minted for a meeting keeps its start and end time (RED)
- azul-apps `45fae88` feat(meet): rooms keep their meeting's start and end time (`node --test`: 63/63)
- `553df4102` test(azcalendar): a minted link carries the event's times, in UTC, into the event file (RED)
  (event.rs, meeting.rs, mint-and-join.mjs)
- `9954a4276` feat(azcalendar): minting sends the event's times in UTC, and the event file keeps them
- `ddbfc42ac` refactor(azcalendar): event ids are azul's Uuid::from_seed of a random seed
- dry run of mint-and-join.mjs against Node stand-ins (scratchpad `m6/`): PASS; the no-times variant FAILS as it
  should
- report `scripts/M6_ROOM_TIMES_2026_09_29.md`

## IN PROGRESS
- nothing

## NEXT
- parent: `cargo test -p AzCalendar --lib` once `Uuid::from_seed` is merged; the E2E with real binaries

## Open questions
- none (see the report's "What is left")

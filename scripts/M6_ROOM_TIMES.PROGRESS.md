# M6_ROOM_TIMES progress

Branch `wt/m6-room-times` (base `34a8fe46f`); Worker work in `/Users/fschutt/Development/azul-apps-m1`,
branch `cf-workers-meet`.

## DONE
- azul-apps `adfaf66` test(meet): a room minted for a meeting keeps its start and end time (RED)
- azul-apps `45fae88` feat(meet): rooms keep their meeting's start and end time (`node --test`: 63/63)

## IN PROGRESS
- AzCalendar RED: window JSON round trip (event file version 2, v1 still read), UTC conversion, mint body

## NEXT
1. AzCalendar fix: minting sends the window, the answer's window is stored; ids via `Uuid::from_seed`
2. mint-and-join.mjs: the dev server stored the window, the event file carries it
3. Report `scripts/M6_ROOM_TIMES_2026_09_29.md`

## Open questions
- none yet

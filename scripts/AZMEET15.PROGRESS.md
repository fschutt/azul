# AZMEET15 progress (production AzMeet: no demo, E2E-encrypted chatrooms, meetings, key exchange)

Worktrees: azul `.claude/worktrees/agent-a6f8c1ad0d3e80fa3` (branch worktree-agent-a6f8c1ad0d3e80fa3,
fast-forwarded to fix/input-bugs-2026-09-19 @ 5d78255a4); azul-apps `../azul-apps-wt/meet15`
(branch local/meet15 @ b9d4c72). No cargo / npm test / wrangler (the lead builds and runs).

## DONE
- azul 5203010be progress file; c01509edf CRYPTO.md (the design, before any code).
- azul-apps 3fc8002 Worker RED tests (test/chatrooms.test.mjs, support.mjs, store + entry tests);
  6aff9cc Worker: encrypted rooms (src/auth.js, members / keys / messages / sync routes, signed
  peers in encrypted rooms, schema + ADDED_COLUMNS, README "Encrypted rooms", wrangler vars).
- Test vectors (node:crypto, /tmp/azmeet15/vectors.mjs): seeds 00..1f / 20..3f -> devices
  400061d5.. / 964b7270.., safety "59174 51299 37993 31242"; pinned in chatrooms.test.mjs and
  (next) crypto.rs.

## IN PROGRESS
- crypto.rs

## NEXT
3. AzMeet crypto.rs (pure, test vectors shared with the Worker tests), chatroom.rs (room state
   machine + an in-memory fake Worker test), roomlist.rs (the local room index).
4. Remove the in-process demo; unreachable server = error state with Retry.
5. lib.rs / ui.rs integration: identity in the keyring (--identity-file for tests), signed peers,
   encrypted chat through the Worker, rooms list + unread, room view, knock / admit, meeting times.
6. scripts/azmeet_e2e.py: phase `crypto` with three instances; node scripts follow the names change.

## Open questions
- (none yet)

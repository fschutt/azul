# AZMEET15 progress (production AzMeet: no demo, E2E-encrypted chatrooms, meetings, key exchange)

Worktrees: azul `.claude/worktrees/agent-a6f8c1ad0d3e80fa3` (branch worktree-agent-a6f8c1ad0d3e80fa3,
fast-forwarded to fix/input-bugs-2026-09-19 @ 5d78255a4); azul-apps `../azul-apps-wt/meet15`
(branch local/meet15 @ b9d4c72). No cargo / npm test / wrangler (the lead builds and runs).

## DONE
- (none yet)

## IN PROGRESS
- CRYPTO.md (design before code)

## NEXT
1. examples/azul-meet/CRYPTO.md: threat model, identity, invite secret, sealing, messages, rotation,
   safety codes, multi-device, what the database shows.
2. Worker (azul-apps cf-workers/meet): RED tests for members / keys / messages / sync / signed
   requests, then the routes, schema migration, README.
3. AzMeet crypto.rs (pure, test vectors shared with the Worker tests), chatroom.rs (room state
   machine + an in-memory fake Worker test), roomlist.rs (the local room index).
4. Remove the in-process demo; unreachable server = error state with Retry.
5. lib.rs / ui.rs integration: identity in the keyring (--identity-file for tests), signed peers,
   encrypted chat through the Worker, rooms list + unread, room view, knock / admit, meeting times.
6. scripts/azmeet_e2e.py: phase `crypto` with three instances; node scripts follow the names change.

## Open questions
- (none yet)

# AZDRIVE11 - AzDrive's body as a Finder window in flora (progress)

Worktree: .claude/worktrees/agent-a749f5de9db0ebffe (branch from fix/input-bugs-2026-09-19).
No cargo here; the lead builds and runs everything. An independent read-only review found no
compile errors (every azul call checked against target/codegen/azul.rs and api.json).

## DONE
- 4c26bb20e every setting is a switch: --home / --downloads / --drives / --dialogs (env only
  fills an absent switch); scripts pass the switches.
- ce9dbf666 look.rs, ui_sidebar.rs (source list + keyboard), the content leaf, path bar and
  status line (status bar gone), E2E steps 1 / 1b / 17, browse.py's CLOUD row checks.
- 271a1934d hover rules follow how with_css resolves (opaque washes, a selected state keeps its
  ground, lift on selection only).
- 96615a736 Enter / Space on a button under the list clicks it.
- 642ca4755, then the status line's corners, browse.py's eject id, the row menu's fallback.

## NEXT (the lead)
- build, `cargo test -p AzDrive`, scripts/azdrive_e2e.py (shots 17-*), browse.py.

## Open questions / reported
- IconGrid / Tile hover lift: widget looks (layout/), not app-reachable.
- Engine: a with_css `:hover {}` block is subtree-scoped (`push_front_scope_for` keeps only a
  bare `*` node-only) - it styles every hovered element inside (AzNews' toolbar face).

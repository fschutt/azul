# AZDRIVE11 - AzDrive's body as a Finder window in flora (progress)

Worktree: .claude/worktrees/agent-a749f5de9db0ebffe (branch from fix/input-bugs-2026-09-19).
No cargo here; the lead builds and runs everything.

## DONE
- 4c26bb20e every setting is a switch: --home / --downloads / --drives / --dialogs (env only
  fills an absent switch); scripts pass the switches.
- (next commit) look.rs, ui_sidebar.rs (source list + keyboard), the content leaf, path bar and
  status line (status bar gone), E2E steps 1 / 1b / 17, browse.py's CLOUD row checks.

## IN PROGRESS
- review pass of the new code (no cargo here: read for compile errors).

## NEXT
- report: commits, file:line, widget gaps (IconGrid tile lift; TreeView no longer used).

## Open questions
- IconGrid tiles cannot lift from the app (layout/ widget look) - report as a widget gap.

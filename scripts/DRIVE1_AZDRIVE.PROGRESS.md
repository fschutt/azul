# DRIVE1_AZDRIVE progress

Branch `wt/drive1-azdrive` from `a7e18f4df`. Report: `scripts/DRIVE1_AZDRIVE_2026_09_30.md`.

## DONE
- RED: `examples/azul-storage` skeleton (Drive trait, types, `todo!()` bodies) + unit tests
  (SigV4 vectors, keys, LocalDrive, S3Drive over a fake transport, XML, config, scope, transfer).

## IN PROGRESS
- GREEN: the storage crate's bodies.

## NEXT
1. Python S3 test server (`examples/azul-drive/scripts/s3_server.py`) + unittest, RED first.
2. AzDrive app (`examples/azul-drive`): RED for the pure view logic, then the app.
3. `examples/azul-drive/scripts/browse.py` e2e.
4. Report.

## Open questions
- `<transient-window>` dialogs are not scriptable headless (only the root window has a debug
  timer); the e2e uses `AZDRIVE_DIALOGS=inline`.

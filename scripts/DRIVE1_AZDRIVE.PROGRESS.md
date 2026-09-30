# DRIVE1_AZDRIVE progress

Branch `wt/drive1-azdrive` from `a7e18f4df`. Report: `scripts/DRIVE1_AZDRIVE_2026_09_30.md`.

## DONE
- `6f250c31f` RED: `examples/azul-storage` skeleton + unit tests.
- `7d6900a74` GREEN: the storage crate (LocalDrive, S3Drive + SigV4, config, ScopedDrive,
  transfer, AzulTransport).
- `b7e22683f` RED / `39966255c` GREEN: `examples/azul-drive/scripts/s3_server.py` +
  `test_s3_server.py` (23 tests pass).
- `12ff29be3` RED: AzDrive view model (`browse.rs`) tests; `f8a080f84` GREEN: browse.rs + the app.
  Coordinator ruling done: NoTitle decorations + `Titlebar` title row in the toolbar colour.
- `5ba35f6cd` `browse.py` e2e.
- Report committed.

## IN PROGRESS
- nothing

## NEXT (for the parent)
- Compile and run: `cargo test --release -p azul-storage`, AzDrive build + `--lib` tests,
  `python3 examples/azul-drive/scripts/browse.py --bin target/release/AzDrive`.

## Open questions
- `<transient-window>` dialogs under the headless debug server (only the root window has a debug
  timer); the e2e uses `AZDRIVE_DIALOGS=inline` by default, `--window-dialogs` to try the window.
- Optional api.json addition `HttpRequestConfig::http_request_blocking` (see the report).

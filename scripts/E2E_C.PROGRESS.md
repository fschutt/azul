# E2E-C progress (branch wt/e2e-c, base 05ef3a8f4) - 2026-10-06

Apps: AzReader, AzReview, AzSetup, AzSheets, AzShow, AzTasks, AzTerm, AzVideoCut, AzWriter.
Binaries: /Users/fschutt/Development/azul/target/release/<App> (engine 889dccf30+). Logs: /tmp/e2e-c/<app>.log.

## Shared helper (scripts/azlin_e2e.py)
- SCRIPT: `printed(key)` never matched a bare line (`AZREADER_READY`, `AZWRITER_READY`, `AZTERM_READY` print no
  value; the regex wanted `KEY <value>`), so every "wait for the window" timed out. A bare line now counts as ""
  when the pattern can match an empty value.

## AzReader
- status: IN PROGRESS
- root causes: bare AZREADER_READY (helper, above)

## AzReview / AzSetup / AzSheets / AzShow / AzTasks / AzTerm / AzVideoCut / AzWriter
- status: not run yet

## NEXT
- rerun AzReader

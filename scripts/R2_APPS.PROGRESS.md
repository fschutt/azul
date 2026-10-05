# R2_APPS progress (wave 9 round 2, PKG R2-APPS) - branch wt/r2-apps, base 440991077

## DONE
- A1 AzNews helpers -> azul_appkit::pieces: d40af169e
- A2 AzKeys leftovers (csv dep + Cargo.lock line, row alias, edit_tag_chip): 99c9b9927
- A3 AzReader TempDir -> azul_storage::testing::TempDir: 228d84f7b
- A4 appkit open_external -> Url::open_path (+ no `cmd /C start` without the feature): 65e2e7d86
- A5 AzCalendar chrome.rs line/button/primary -> pieces: 33ea7e0d2
- A6 AzContacts Toolbar: 4396334c9
- A7 AzERP on_reference Clear arm: 1f8e870a0
- A8 AzCode F1 list Mod+O: RED a0157835f, GREEN 9f3bea1a1
- A9 E2E: aznews 988c43243 + 403fd1e01 (rename phase PASS vs prebuilt); azcode 61235aefe
  (folder phase PASS vs prebuilt); azerp cc8757f81 + 11b0bcb69 (Modal window addressing; fails
  today at step 3 on an engine bug: the owner window is not rebuilt after a callback in the
  Modal's transient window)

## IN PROGRESS
- the report scripts/R2_APPS_2026_10_05.md

## NEXT
- write + commit the report, then reply.

## Open questions
(none)

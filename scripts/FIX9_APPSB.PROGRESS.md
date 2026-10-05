# FIX9-APPSB progress (PKG 6 APPS-B: the wave-9 apps) - branch wt/fix9-appsb, base b454da215

Brief: scripts/waves/wave9/SMALL_FIXES.md section "PKG 6 APPS-B". No cargo; parse-check with
`rustfmt --edition 2021 --check` (diffs ignored, only "error" lines matter).
Python edits: always `s.index(x, start)` for the end marker (a monitor edit duplicated code once:
restored from HEAD before committing).

## DONE
- 6.1 AzDashboard one prefix: RED 885f94939, GREEN 83e3d4b8c
- 6.2 Toolbar: AzPdf 584e6bb43, AzNews 485068a85, AzCode find bar 1d720e982
  (ROUND-2 NOTE: the Toolbar gives tools no DOM id; E2E clicks #__azpdf_next etc. need
  toolbar.rs `tool()` to put the item id on the node as its DOM id - the apps pass the prefixed
  DOM-id names as item ids already)
- 6.3 TokenInput AzKeys tags: RED 757946863, GREEN d954697ce (ids::edit_tag_chip now unused)
- 6.4 SKIPPED AzReader: needs app.rs (AppState: the grid's IconGridView, outside the Files line)
  + IconGrid items have no DOM id (azreader_e2e double-clicks #__azreader_book-0).
  AzMusic covers (optional): deferred to the end.
- 6.5 Gauge: AzMonitor ace78760e, AzKeys TOTP 650b94268, AzClock timer 03a2b9d31
- 6.6 AzERP: (c) RED 37eb5afc7 GREEN d1ebf8a7d; (d) cb493bebd; (a) 914006cfe; (b) 89d94bbb1;
  (e) SKIPPED (borderline: Grid filter takes one (field, value); needs a predicate + state + test)
  E2E NOTE (a): azerp_e2e types "2.400,00" into #__azerp_field-acquisition_cost - the MoneyInput
  root is not focusable and en-US refuses that text: script -> selector
  "#__azerp_field-acquisition_cost .__azul-native-text-input-container", text "2,400.00".
- 6.7 AzDashboard grouping: 6a65ac9ea
- 6.8 AzNews close-save + timer re-arm: 613081d51
- 6.9 AzCode Open Folder: d5227af5f (no menu bar in AzCode: Mod+O; F1 list in lib.rs = round 2)
- 6.10 SKIPPED (NOT SMALL in the Files line): the click hook sits in ui_reader.rs `spread()`
  (no app there), Command / Target live in app.rs, exact anchors need a post-layout hook in the
  reader UI - all outside the Files line. Plan in the report.
- 6.11 AzClock city menu: RED f56e80a21, GREEN 94b948600

- 6.12 AzWidgets video demo: RED f7328bb2b, GREEN 6d042bf0e
- 6.4 AzMusic covers (optional): SKIPPED, NOT SMALL (no covers kept by the library today)
- Report: scripts/FIX9_APPSB_2026_10_05.md

## IN PROGRESS
- nothing

## NEXT
- nothing: the package is finished (round-2 notes in the report)

## Open questions

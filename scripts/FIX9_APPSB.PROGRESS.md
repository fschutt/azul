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

## IN PROGRESS
- 6.6 AzERP (a) MoneyInput (b) ReferencePicker (c) delete confirmation (d) format_amount (e) filter bar

## NEXT
- 6.7 .. 6.12 in order, then optional 6.4 AzMusic

## Open questions

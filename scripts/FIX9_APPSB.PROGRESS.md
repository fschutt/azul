# FIX9-APPSB progress (PKG 6 APPS-B: the wave-9 apps) - branch wt/fix9-appsb, base b454da215

Brief: scripts/waves/wave9/SMALL_FIXES.md section "PKG 6 APPS-B". No cargo; parse-check with
`rustfmt --edition 2021 --check` (diffs ignored, only "error" lines matter).

## DONE
- 6.1 AzDashboard one prefix: RED 885f94939, GREEN 83e3d4b8c
- 6.2 Toolbar: AzPdf 584e6bb43, AzNews 485068a85, AzCode find bar 1d720e982
  (ROUND-2 NOTE: the Toolbar gives tools no DOM id; E2E clicks #__azpdf_next etc. need
  toolbar.rs `tool()` to put the item id on the node as its DOM id - the apps pass the prefixed
  DOM-id names as item ids already)

## IN PROGRESS
- 6.3 TokenInput in AzKeys tags (azul-keys/src/ui_item.rs:791)

## NEXT
- 6.4 .. 6.12 in order

## Open questions

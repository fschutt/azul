# FIX9_APPSA progress (wave 9, PKG 5 APPS-A; branch wt/fix9-appsa, base b454da215)

## DONE
- 5.1 one CSV reader: 745d031dc (RED appkit csv), 3ed2aa1d4 (GREEN), c6a43a475 (contacts), 5bc64d6bf (keys),
  d50948ae7 (erp)
- 5.2 one set of DOM helpers: 481d33e9a (appkit pieces), 5ba600103 (contacts), fdc40bead (keys)

## IN PROGRESS
5.3 AzMail sanitizer mechanics -> azul_appkit::css

## NEXT
5.3 -> 5.14 in order (scripts/waves/wave9/SMALL_FIXES.md, PKG 5)

## Round-2 notes so far
- AzKeys/Cargo.toml still lists csv (unused after 5.1): drop it.
- AzKeys ui_item.rs imports `row` from crate::ui (an alias of appkit pieces::flex_row): rename to flex_row.
- AzNews ui.rs:502-540 -> azul_appkit::pieces (R.2).

## Open questions

# FIX9_APPSA progress (wave 9, PKG 5 APPS-A; branch wt/fix9-appsa, base b454da215)

## DONE
- 5.1 one CSV reader: 745d031dc (RED appkit csv), 3ed2aa1d4 (GREEN), c6a43a475 (contacts), 5bc64d6bf (keys),
  d50948ae7 (erp)
- 5.2 one set of DOM helpers: 481d33e9a (appkit pieces), 5ba600103 (contacts), fdc40bead (keys)
- 5.3 mail sanitizer over appkit css: 73d38c4ee (RED content ";"), a62dff23d (GREEN)
- 5.4 one ribbon builder: e7e48ebbd (appkit ribbon), reader / writer / show / sheets 7f1d3b7ef / drive 116f517fc /
  calendar 2c905acb8 / tasks 45570ae46

- 5.5 drive listings: 27c40b516 (RED list_folder_all), 28a984499 (GREEN), fe6e63846 (list_all_paged), 90c529cbd
  (appkit/notes/pim), 29849d33b (photo/videocut), 3fbcbc3e6 (code/mail/sheets)
- 5.6 one TempDir: 566b2cd5b (storage testing + pim re-export), 14bc44b8a (mail), 45b9bdbe7 (appkit/drive)

- 5.7 open with the OS: 9b26f73a5 (appkit open_external -> Url::open for web), 9ce3bb95b (review)
- 5.8 photo a11y: ad6e643fb
- 5.9 videocut rgba_to_nv12: 3d394bc64 (NEEDS api.json RawImage.rgba_to_nv12, see commit message)

- 5.10 compose size: 2477632b6
- 5.11 azmail_e2e submission phase: 613664985 (py_compile only; not run - memory held by a cargo test)

## IN PROGRESS
5.12 sheets thousands grouping -> MoneyInput::format_amount

## NEXT
5.12 -> 5.14 in order (scripts/waves/wave9/SMALL_FIXES.md, PKG 5)

## Round-2 notes so far
- AzKeys/Cargo.toml still lists csv (unused after 5.1): drop it.
- AzKeys ui_item.rs imports `row` from crate::ui (an alias of appkit pieces::flex_row): rename to flex_row.
- AzNews ui.rs:502-540 -> azul_appkit::pieces (R.2).
- AzCalendar chrome.rs:443-465 `line` / `button` / `primary` are pieces twins with a margin (flex_row("margin-top: 8px"),
  pieces::button(..).with_css("margin-right: 8px")).

- AzReader jobs.rs:357 TempDir copy: needs `[dev-dependencies] azul-storage = { .., features = ["testing"] }` in
  examples/azul-reader/Cargo.toml (outside PKG 5), then `use azul_storage::testing::TempDir`.

- appkit open_external: call azul's path variant of Url::open for files / folders once 3.5 names it.

## Open questions

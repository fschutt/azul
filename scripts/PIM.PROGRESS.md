# PIM (wave 5, 2026-10-02) - progress

Branch `wt/pim` from `2e92c759b`. Task: `examples/azul-pim` plain-Rust crate (dates, RRULE, content lines,
mail addresses, search, initials, one task store) + move AzCalendar / AzTasks / AzContacts / AzMail onto it.

## DONE
- 3f32f0e51 crate skeleton + `dates` + registration (Cargo.toml member, workspace_test_members, rust.yml step)
- 884d04bde `rrule` (AzCalendar rrule.rs moved; AzCalendar `pub use azul_pim::rrule;`)
- 236a8f159 `repeat` (AzTasks recur.rs moved; AzTasks `pub use azul_pim::repeat as recur;`)
- 4df836f84 RED / bea800bf3 GREEN: `mail_address`; AzCalendar attendees `"Lovelace, Ada" <..>` (B14, E4)
- e77e7dcbb RED / 0d90994a8 GREEN: AzContacts form refuses `a@b@example.org`
- c7b60675f AzMail compose.rs / account.rs re-export the shared address helpers
- 4b4e9542e `content_line`; 6f3c26d67 AzContacts vcard.rs + AzCalendar ics.rs adopt it (B15)
- 8425c80ad `task` + `task_store` + `testing::TempDir` (AzTasks model.rs / store load / order helpers moved;
  AzTasks re-exports; Task::complete; default_list; DEFAULT_LIST "default" shows as "Tasks")

- f985e12db RED / c0b1d9f34 GREEN (pim) / a9009d359 GREEN (AzCalendar): one task store + migration;
  963c9ac3c RED / e0b745a44 GREEN AzTasks rewrites migrated files on load

- b62b9815e `search` + `initials`; 93f177196 RED / 9de16a522 GREEN Mail/Tasks search folds diacritics,
  Contacts + Mail initials (B16, B24, F5)
- 27f16874f AzCalendar ids + AzContacts uids from azul_storage::ids::random_seed (B1)
- a571c89ac AzMail weekday_name + AzCalendar week_start from azul_pim::dates

## IN PROGRESS
- Repeat <-> Rule bridge in pim repeat.rs (RED with stubs, then GREEN)

## NEXT (in order)
6. Repeat <-> Rule bridge (RED first), then the report scripts/PIM_2026_10_02.md

## Decisions
- One task format = AzTasks' `aztasks.task` v1 (no rename: AzTasks' files need no migration).
- AzCalendar tasks root = azul_appkit::data::data_root(named data dir, AZTASKS_DATA, user data) - with
  --data/AZCAL_DATA everything stays in that folder (tests/E2E), else `<user data>/Azlin` (AzTasks' default).
- Old To-Do bar files go to list `default` (shows as "Tasks" without list.json); new To-Do bar tasks go to
  AzTasks' default list (settings, else first in nav order, else `default`).
- Calendar task writes stay synchronous in callbacks (as its events; B2 still open).
- Helpers renamed in azul_pim::dates: ordinal_word (RRULE words) vs ordinal_suffix ("1st"); weekday_key
  (file "mon") vs weekday_code (RRULE "MO"); add_months_clamped (to-do) vs shift_month (RRULE).
- App modules kept by re-export (`pub use azul_pim::rrule;`, `pub use azul_pim::repeat as recur;`) so the
  apps' paths stay; the app copies are deleted.
- RED commits carry compiling stubs with the old behaviour (as d86ef7c6f did).

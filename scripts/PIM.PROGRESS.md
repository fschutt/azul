# PIM (wave 5, 2026-10-02) - progress

Branch `wt/pim` from `2e92c759b`. Task: `examples/azul-pim` plain-Rust crate (dates, RRULE, content lines,
mail addresses, search, initials, one task store) + move AzCalendar / AzTasks / AzContacts / AzMail onto it.

## DONE
- 3f32f0e51 crate skeleton + `dates` + registration (Cargo.toml member, workspace_test_members, rust.yml step)
- 884d04bde `rrule` (AzCalendar rrule.rs moved; AzCalendar `pub use azul_pim::rrule;`)
- 236a8f159 `repeat` (AzTasks recur.rs moved; AzTasks `pub use azul_pim::repeat as recur;`)

## IN PROGRESS
- mail_address: RED test in AzCalendar editor.rs for `"Lovelace, Ada" <ada@example.org>`

## NEXT (in order)
1. mail_address module + AzCalendar GREEN, then AzMail (compose.rs split/bare/same, account.rs is_email) and
   AzContacts (contact.rs email check) adopt
2. content_line module (vCard Property shape) + AzContacts vcard.rs / AzCalendar ics.rs adopt
3. task + task_store (AzTasks model.rs + store load move; legacy azcalendar.task migration; RED first);
   AzCalendar To-Do bar on the shared store (root: Azlin root unless --data/AZCAL_DATA)
4. search (fold + Query) + initials; Contacts/Tasks/Mail adopt
5. AzCalendar ids via azul_storage::ids::random_seed
6. Repeat <-> Rule bridge (RED first), report

## Decisions
- Helpers renamed in azul_pim::dates: ordinal_word (RRULE words) vs ordinal_suffix ("1st"); weekday_key
  (file "mon") vs weekday_code (RRULE "MO"); add_months_clamped (to-do) vs shift_month (RRULE).
- App modules kept by re-export (`pub use azul_pim::rrule;`, `pub use azul_pim::repeat as recur;`) so the
  apps' paths stay; the app copies are deleted.
- RED commits carry compiling stubs with the old behaviour (as d86ef7c6f did).

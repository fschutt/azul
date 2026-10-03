Task PIM (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/pim` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/pim 2e92c759b`). TASK name: `PIM`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: the shared personal-information logic (DEDUP_EDITORS: read its PIM findings first) as a plain-Rust
crate `examples/azul-pim` (no azul dependency, like azul-storage; register it as a workspace member + test member,
append-only): dates and month math, recurrence rules (RRULE), iCalendar and vCard line folding/unfolding and
escaping, e-mail address and attendee parsing (AzCalendar rejects `"Lovelace, Ada" <ada@example.org>` - it splits
at the comma: RED first), search/filter helpers, avatar initials. Each piece: find every copy in AzCalendar,
AzTasks, AzContacts, AzMail (grep), take the most complete one, RED tests from the copies' behaviours, then move
the apps onto the crate. ONE task store: AzCalendar writes `azcalendar.task` files and AzTasks `aztasks.task`
files though both claim to share one layout - one format (the storage layout in the house rules), both apps read
and write it, with a migration of the other's existing files. Ids: `Uuid::from_seed(azul_storage::ids::random_seed())`
(AzCalendar/AzTasks have their own random_seed copies - use azul_storage::ids). Calendar's DatePicker week-start
call site is APIEXPORT's widget change (coordinate in your report).

Report `scripts/PIM_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.

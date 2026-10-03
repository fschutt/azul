Task MAIL6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/mail6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/mail6 <base>`).
TASK name: `MAIL6`. Report `scripts/MAIL6_2026_10_03.md`, progress `scripts/MAIL6.PROGRESS.md` (commit it after every commit).

FIRST, LOOK (the apps were written blind - "the look is written, not seen" in every report): run your app(s) headless
from the prebuilt binaries (/Users/fschutt/Development/azul/target/release/<App>, built from aa59b2d84) through
scripts/waves/tools/run_capped.sh (it holds a machine-wide lock - one app at a time on this 8 GB Mac), drive them over
the debug server (scripts/azlin_e2e.py is the Python helper; ops in layout/src/e2e/full.rs; `screenshot` writes a PNG
you can read), in flat + flora and light + dark, through the main flows. Write what is broken into your progress
file (with screenshots under your worktree's target/ - not committed), THEN fix it: engine bugs RED first in the
engine where they live (if the engine area belongs to another task, write it into your report for its owner instead),
app bugs RED first in the app. Your app's existing E2E script must cover the flows you fix.
APP FINISH CHECKLIST (each app you own): built on its shell, flat+flora x light+dark correct; `__az<app>_` class/id
prefix constants defined once; on azul-appkit (args, data root, settings remembered across restarts, About via the
standard AboutDialog, shortcuts table); durable data + exports through the Drive into the data tree (no direct file
writes from callbacks; jobs on a Thread); a document app asks "save changes?" via CloseRequested + prevent_window_close
or the CloseGuard widget; ids via Uuid::from_seed(azul_storage::ids::random_seed()); no `ctrl || meta`; no duplicated
helpers (DEDUP reports); sample data + empty state; its E2E script passes against the prebuilt binary where the
behaviour does not depend on your new code.

GOAL: AzMail "like Outlook 2010" - the plan's goal (user, 2026-09-30): log in to the mail provider -> download the
mails to a local folder (S3 sync later) -> display them -> "New mail" / "Reply" open a POPUP WINDOW with the rich editor
-> send (SEND works: micromail 0.2, scripts/azmail_send_test.py 7/7). Look: Outlook 2010 (ribbon tabs Datei / Start /
Senden-Empfangen / Ordner / Ansicht with groups Neu, Loeschen, Antworten, QuickSteps, Verschieben, Kategorien, Suchen,
Senden/Empfangen; left navigation pane: Favoriten, per-account folder trees with unread counts, the module buttons
E-Mail / Kalender / Kontakte / Aufgaben at the bottom; message list: search field, "Anordnen nach: Datum" sort header,
grouped rows icon/from/subject/date/flag; reading pane: subject, sender, info bar "Klicken Sie hier, um Bilder
herunterzuladen", Gesendet/An, the body on paper, people footer; right To-Do bar: mini calendar, appointments, tasks;
status bar: filter, sync status, zoom). Read scripts/MAIL1_AZMAIL_2026_09_30.md, MAIL2_2026_10_01.md, SEND_2026_10_01.md,
MAILVIEW/MAILWIDGETS_2026_09_30.md, MAILHTML_2026_10_02.md, RTE_2026_10_02.md first.
Specific items: the To-Do bar's tasks on azul-pim's shared task store (they are in memory today); the compose window's
close check via CloseRequested + prevent_window_close (ui_compose.rs ~764-767 clears the close flag by hand - DEDUP);
user ruling: the sanitizer keeps a mail's own `class` attributes behind a per-message prefix and rewrites the mail's
class selectors to match (today it drops the classes but keeps their rules, which then never apply), so mail CSS
works and can never reach the app's own `__azmail_` classes; the third character-reference decoder in html.rs
(HYGIENE: use azul's); AzMail's two HTML escapers -> one (HYGIENE proposed `Xml.encode_text` / `encode_attribute`
exports - list them for api.json). Use an IMAP test server for the E2E (scripts/ has the Python IMAP/SMTP sinks MAIL2/
SEND wrote). Mail HTML rendering bugs are MAILENG6's - report them (mail, box, what Chrome does).

Report `scripts/MAIL6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.

# MAIL6 progress (wave 6, 2026-10-03)

Branch `wt/mail6` from `25d78e309`. Brief: scripts/waves/wave6/MAIL6.md. Report: scripts/MAIL6_2026_10_03.md.
POWER: battery warning from the coordinator - commit after every small unit, no long headless runs until told.

## DONE (commits)
- 3059d3be7, e1a707add, 243417ee3 progress + LOOK findings (screenshots target/mail6-look/, not committed;
  drivers in the session scratchpad look.py / look2.py / look3.py)
- cc7040ae5 RED layout test for bug 9 (typing_stays_with_its_field_when_another_page_replaces_it.rs + all.rs)
- 62cf7cd60 RED E2E: scripts/azmail_e2e.py --phase sample (SampleRun) - fills, prefixed ids, To-Do store,
  compose fills, close guard, HTML mail widths, restart keeps tasks; --phase account = the old run
- 13bbb49d1 body height 100% (lib.rs WINDOW_BODY_CSS, both windows)
- 7b777cfcb compose close: prevent_window_close + standard MessageBox in a Modal (own handler reading live
  state; NOT CloseGuard: its `dirty` is the last build's snapshot - an app close after Discard is held. Report it
  for INFRA6)
- bbe5e3533 / c9aeb24dc RED/GREEN account::data_root = AZMAIL_DATA else <Azlin root>/mail; legacy
  <user data>/AzMail moved once (INFRA6 confirmed it does not migrate AzMail's folder)
- aee6084b5 appkit: args.rs = SPEC/ABOUT/SHORTCUTS/Screen::of; start() on kit (create_kit, app_config,
  window_options), on_window_created(--shot); MailApp.kit/.screen
- a90f4016d / 85fdf8cec RED/GREEN todo.rs (task store glue) ; df7697b1d To-Do bar UI on it (file jobs on a Thread)
- a4fb8803f File > Options = kit settings page, File > About = AboutDialog modal, view toggles remembered, Mod+E dropped
- 61e803731 ids.rs __azmail_ constants (needs SMALL6 codegen AzString::from_const_str); E2E scripts updated
- NEXT NOW: step 5 (sanitizer class prefix)

## NEXT (exact)
1. ui_main View tab: drop the Look group (theme/mode buttons) -> File > Options opens the kit settings page
   (kit::open_settings + settings_page as the window content, like AzContacts ui.rs ~1230); kit::handle_key first in
   on_main_key; remember view toggles (reading pane, To-Do bar, nav collapsed, plain text, zoom) via
   azul_appkit::ui::set_value(kit_ref, info, key, value).
2. File > About -> standard AboutDialog in a Modal (MailApp.about_open); remove about_page's hand-written keys.
3. Mod+E focuses the list search (or drop it from SHORTCUTS + the placeholder).
4. ids.rs: `__azmail_` constants (AzString::from_const_str) for every id/class AzMail sets; update E2E scripts
   (sync_e2e.py, azmail_e2e.py account phase: acct-*, send-*, compose-*) and allow `appkit-` ids in
   check_prefixed_ids.
5. html.rs sanitizer: keep mail classes behind a per-message prefix + rewrite class selectors (RED test first).
6. html.rs / compose.rs escapers -> Xml.encode_text / encode_attribute (api.json list), third decoder check.
7. Status bar zoom (StatusBarZoom) for the reading pane.
8. store::LocalFolder over azul_storage::LocalDrive (DEDUP_EDITORS B3) if time.
9. Report scripts/MAIL6_2026_10_03.md.

## Seen broken (LOOK, prebuilt aa59b2d84)
1. APP (fixed 13bbb49d1): window content only ~490 of 860 px; module buttons cut off.
2. ENGINE (solver3 layout cache - MAILENG6): opening the HTML newsletter collapses the PIM shell's inner SplitPane
   (list | reading pane) to 0 px wide panes; plain text fine; a window resize lays it out right. Repro: AzMail
   --sample, click "Garden Weekly: bulbs, frost and a sale" (a `<table width=600>`).
3. HEADLESS (HEADLESS6): the compose window does not follow the debug server's `set_mode` (set_theme reaches it).
4. RENDER (headless screenshot): text smeared (drawn several times at sub-pixel offsets) after several relayouts
   (backstage_flat_light.png, l2_resize.png); the first frame is crisp.
5. APP: the HTML mail's paper in dark mode: black frame around the mail's white table, the 600 px table wider
   than the paper.
6. APP: no zoom in the status bar.
7. APP: About is a backstage page with hand-written shortcut lines; shortcuts listed twice.
8. APP (fixed df7697b1d): To-Do bar tasks in memory.
9. ENGINE (no wave-6 owner; RED cc7040ae5): the Add Account wizard's page-2 TextInputs show page 1's typing
   (host = "Ada Lovelace", port = the e-mail); keystrokes go in at the old caret; the app receives the merged text.
   The prebuilt account E2E fails there ("timed out waiting for the sending page"). Reading core diff.rs: every
   reconcile pass keys by CSS id, window.rs remap drops unmatched overlay entries - cause not found by reading.

## Decisions
- Labels stay English (Outlook 2010 English names; the brief's German names are the same controls).
- AzMail folder = <Azlin data root>/mail (account folders inside as before: mail/<account>/mail/<folder>/...);
  AZMAIL_DATA still overrides it (scripts, tests).
- Compose close: own CloseRequested handler + prevent_window_close + MessageBox/Modal (see 7b777cfcb).
- To-Do bar writes through appkit file jobs on a Thread; the store is read once at start before the window.

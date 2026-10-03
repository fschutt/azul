# MAIL6 progress (wave 6, 2026-10-03)

Branch `wt/mail6` from `25d78e309`. Brief: scripts/waves/wave6/MAIL6.md. Report: scripts/MAIL6_2026_10_03.md.

## DONE
- 3059d3be7 progress file
- cc7040ae5 RED layout test for bug 9 (typing_stays_with_its_field_when_another_page_replaces_it.rs, all.rs appended); root cause NOT found by reading: core diff.rs reconcile_dom keys by CSS id (A1/B1-B3 all include the id), window.rs remap_node_ids drops the overlay of unmatched nodes - so the cause is elsewhere (the path AzMail takes: TextInput on_text_input returns DoNothing, overlay over a stale DOM; check content_overlay GC / layout_new_generation vs regenerate path). Owner: none in wave 6 -> report.
- LOOK run 1 + 2 (prebuilt AzMail aa59b2d84, `--sample`, headless, capped): screenshots in
  target/mail6-look/ (not committed). Drivers: scratchpad look.py / look2.py (main, open message, compose,
  backstage; flat/flora x light/dark via the debug server's set_theme / set_mode).

## IN PROGRESS
- fixing the LOOK list (below), RED first.

## NEXT (exact, in order; last commit: see `git log -1`)
1. RED E2E first: extend scripts/azmail_e2e.py with a `--sample` look phase (RED vs the prebuilt binary):
   status bar bottom == window bottom (main + compose); after opening the newsletter the list and reading pane
   are wider than 0 (engine bug 2 - stays RED until MAILENG6); wizard page 2 fields are empty.
2. App: body `height: 100%` (ui_main.rs layout_main ~line 115, ui_compose.rs layout_compose ~line 363;
   the wizard is inside the backstage of layout_main).
3. (done as RED cc7040ae5; fix later if time, else report)
   decide owner (no wave-6 task owns text_input.rs -> fix RED first in layout/tests, or app: distinct keys).
4. Compose close via the CloseGuard widget (ui_compose.rs on_compose_close_requested ~764 clears
   flags.close_requested by hand - DEDUP_OFFICE A16).
5. To-Do bar on azul_pim task_store (model: examples/azul-calendar/src/tasks.rs; writes on a Thread).
6. __azmail_ prefix constants (ids.rs), appkit (args/data root/settings/About dialog/shortcuts),
   sanitizer class prefix, escapers -> Xml.encode_text/encode_attribute (api.json list), status bar zoom.

## Seen broken (LOOK, prebuilt aa59b2d84)
1. APP: the window content is only ~490 px of the 860 px window (main), ~480 of 680 (compose), ~310 (backstage):
   the body is `display: flex; flex-direction: column` WITHOUT `height: 100%` (AzCalendar / AzMeet set it), so it
   is as tall as its content. The navigation pane's module buttons (Mail / Calendar / Contacts / Tasks) are cut off.
2. ENGINE (solver3 layout cache - MAILENG6): opening the HTML newsletter ("Garden Weekly") collapses the PIM shell's
   inner SplitPane (message list | reading pane): both panes 0 px wide (hierarchy: split 826 px, first 0, second 0;
   every descendant 0 wide, the subject wraps per letter). Plain-text mail is fine; View > Plain Text on the same
   mail is fine; a window RESIZE afterwards lays it out right (list 385, reading 532). So the incremental /
   cached relayout after the DOM change keeps a min-content (0) layout of the split's children. Repro: AzMail
   --sample, click "Garden Weekly: bulbs, frost and a sale" (the mail has a `<table width=600>`).
3. HEADLESS (HEADLESS6?): the compose window (a second window) does not follow the debug server's `set_mode`
   (app-wide `callback_info.set_mode`): it stays light in "dark" while `set_theme` reaches it.
4. RENDER (headless screenshot): after several relayouts the text in screenshots is smeared (drawn several times at
   sub-pixel offsets): backstage_flat_light.png, l2_resize.png. The first frame is crisp.
5. APP: the HTML mail's paper in dark mode: a black frame around the mail's white table, the table's green header
   wider than the paper (600 px table in a narrower paper, no horizontal scroll).
6. APP (Outlook look): no zoom in the status bar (Outlook 2010: zoom slider bottom right).
7. APP: About is a backstage page with hand-written shortcut lines ("Ctrl+N" on macOS too), not the standard
   AboutDialog; the shortcuts are listed twice (About text and the key handler).
8. APP: the To-Do bar's tasks are in memory (lost on restart).

9. APP/ENGINE (LOOK 3; the prebuilt E2E fails here: "timed out waiting for the sending page"): the Add
   Account wizard's page 2 TextInputs inherit page 1's TextInput states by position: IMAP host shows
   "Ada Lovelace" (page 1 Name), port shows "ada@example.org" (page 1 e-mail); typed text goes in at the old
   caret ("Ada Lovelac127.0.0.1e"), and the app's on_text_input gets the merged text -> "The IMAP port
   "ada@example.or1143g" is not a port". Screenshots w4_incoming_typed.png / w5_after_next.png.

## Decisions
- Labels stay English (Outlook 2010 English: File / Home / Send / Receive / Folder / View; groups New, Delete,
  Respond, Quick Steps, Move, Tags, Find, Send/Receive) like every Azlin app; the brief's German names are the
  same Outlook 2010 controls.

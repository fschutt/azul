# E2E-B progress (branch wt/e2e-b, base 05ef3a8f4)

Apps: AzMaps, AzMeet, AzMonitor, AzMusic, AzNews, AzNotes, AzPaint, AzPdf, AzPhoto, AzPlayer.
Binaries: /Users/fschutt/Development/azul/target/release/<App>; libazul.dylib is from 03:14 (engine
889dccf30, BEFORE 30fb9f06b). Logs in /tmp/e2e-b/. Probe tool: /tmp/e2e-b/probe.py <bin> <args> -- <snippet.py>.

## Status per app
- AzMaps: NEEDS REBUILD. Fails at "Escape closes the About box first" (AZMAPS_ABOUT stays open).
  ENGINE (headless), two causes:
  1. HeadlessWindow::is_menu_window matched <transient-window> popups (Menu type too), so the owner's
     menu sweep (dismiss_menu_windows) closed the About modal's WINDOW on Escape behind its node's back
     (no Dismissed, the app never told). RED d6f17738d, GREEN 4bb84a779.
  2. Headless had no deliver_forwarded_keys: a key forwarded to a modal (keyboard owner) child waited
     in the mailbox forever. RED 3c603f3da, GREEN 881e6441b.
  Probe (/tmp/e2e-b/azmaps_probe.py, Escape sent to the modal's window `azul-transient` directly, as
  macOS's key window gets it): the whole script PASSES on today's binary. Script left unchanged.
- AzMeet: PASS (06e2080bc, SCRIPT: clicks bypassed the settle; after the rejoin 40-60 nodes animate and
  the Chat tab click missed).
- AzMonitor: NEEDS REBUILD.
  - SCRIPT 801d8aafc: the table / cards / cores are VirtualViews (DOMs of their own): helper gained
    dom_ids, texts/shows/has/has_id/click(every_dom=True).
  - SCRIPT 3685871a6: the end-process question is a Modal window: click_exact("Kill", window=popup())
    (click(text="Kill") hit the paragraph that mentions Kill). Helper gained window_ids/popup and a
    `window` arg for hierarchy/exact/click_exact/click/settle/dom_ids.
  - ENGINE (layout widgets) 7533ed902 + b954528a4 RED, 4b55ce841 GREEN: TextInput/TextArea
    adopt_engine_text ignored an empty engine read -> clearing the filter (Cmd+A, Backspace) never
    reached on_text_input; "pipewire" then went into the stale "rustc".
  - ENGINE already fixed in base, not in today's libazul: 30fb9f06b (the question's window stays open
    after Kill; the next click in the main window is eaten by the menu sweep).
  - Probes on today's build: steps 1-3 pass, the kill passes (AZMON_END 812 true), steps 6-8 pass.
- AzMusic: todo
- AzNews: todo
- AzNotes: todo
- AzPaint: todo
- AzPdf: todo
- AzPhoto: todo
- AzPlayer: todo

## Commits
see `git log --oneline 05ef3a8f4..HEAD`

## Notes
- AzMaps 2-pin.png caught an animation midway (the pins pane's icon off its button): screenshots
  should settle first (house rule) - consider settle() in azlin_e2e.App.screenshot.
- AzMonitor look: the process table stops at ~810 px of 1280 (columns do not fill the width).

## NEXT
- run azmusic_e2e.py

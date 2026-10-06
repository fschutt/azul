# E2E-B progress (branch wt/e2e-b, base 05ef3a8f4)

Apps: AzMaps, AzMeet, AzMonitor, AzMusic, AzNews, AzNotes, AzPaint, AzPdf, AzPhoto, AzPlayer.
Binaries: /Users/fschutt/Development/azul/target/release/<App> (engine 889dccf30+). Logs in /tmp/e2e-b/.

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
- AzMeet: todo
- AzMonitor: todo
- AzMusic: todo
- AzNews: todo
- AzNotes: todo
- AzPaint: todo
- AzPdf: todo
- AzPhoto: todo
- AzPlayer: todo

## Commits
- 51a163c00 progress file
- d6f17738d RED / 4bb84a779 GREEN headless: a popup child window is not a window-based menu
- 3c603f3da RED / 881e6441b GREEN headless: deliver_forwarded_keys to popup children

## Notes
- AzMaps 2-pin.png caught an animation midway (the pins pane's icon off its button): screenshots
  should settle first (house rule) - consider settle() in azlin_e2e.App.screenshot.

## NEXT
- run azmeet_e2e.py

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
- AzMusic: PASS (no change).
- AzNews: library + sample runs PASS (SCRIPT: click_exact walks to the nearest ancestor with a box,
  helper commit); the rename run passes ALONE but the full script is CAPPED (1542-1616 MB tree RSS):
  AzNews --sample's first layout of its 4868-node DOM (all 891 articles, no virtualization) takes
  2.1 s and RSS climbs 304 MB -> 1.38 GB (peak 1.47 GB, steady 1.07 GB); vmmap: 696 MB resident in the
  nano malloc zone with 14 MB allocated (74 % frag., 50 MB dirty) = a burst of tiny allocations in
  layout_and_dl. ENGINE (layout allocation churn) + APP (unvirtualized list). Not fixed.
- AzNotes: FAILS "the note's file on disk" (the check item "[ ] call the bakery" never reaches the
  model). ENGINE race, not pinned: text typed within ~0.4-1 s after Enter on an EMPTY bullet (the
  editor turns it into a paragraph) is dropped - the caret stays at offset 0 of p#__aznotes_note-body-3,
  no TextChanged, nothing painted; flaky (extra debug ops in between make it pass). With 0.5 s after
  each type/key the WHOLE script passes (probe /tmp/e2e-b/aznotes_probe.py). Second bug seen: Enter
  in a check item, then typing, saves "- [ ] plain\ue835" (the check box's icon glyph in the text).
- AzPaint: PASS (no change).
- AzPdf: FAILS "Page Down goes to page 3" (AZPDF_PAGE 2, 3, then 1). SCRIPT fixed first (commit "AzPdf
  reads its pages..."): pages/thumbs are VirtualView DOMs -> rect/has_id every_dom. ENGINE, not pinned:
  the app's scroll_to on its page VirtualView to page 3 (y ~2640 of 3949) is reset to 0 at once
  (get_scroll_states node 71 scroll_y 0) when the re-invoked slice is pages 2-3 (materialized
  1321..3949, scroll_size 2628, virtual 3949); the first jump (page 2, slice from 0) holds. Next twice
  shows the same. Looks: the page shows only the stroked line - no text, no filled rect; the thumbnail
  rail VirtualView is 0 px wide until a later relayout.
- AzPhoto: FAILS "the undo" - one Cmd+Z undoes TWICE (HISTORY 4 3 Opacity -> 4 2 -> 4 1). Root cause:
  Mod+Z is both the menu bar's Edit > Undo accelerator AND the canvas key handler's shortcut; the
  shared accelerator dispatch (dll common/event.rs dispatch_menu_accelerators) fires the item and
  then still delivers the KeyDown to the DOM ("the key still reaches the DOM afterwards"), where
  AppKit (native menu bar on macOS) and Win32 TranslateAccelerator consume it. (The script's own
  key_up also keeps the chord held - released in a probe, still two undos.)
- AzPlayer: PASS (no change). Look: the time label stays 0:00 while playing.

## Commits
see `git log --oneline 05ef3a8f4..HEAD`

## Notes
- AzMaps 2-pin.png caught an animation midway (the pins pane's icon off its button): screenshots
  should settle first (house rule) - consider settle() in azlin_e2e.App.screenshot.
- AzMonitor look: the process table stops at ~810 px of 1280 (columns do not fill the width).

## NEXT
- decide AzPhoto (engine accelerator consume vs app); write report

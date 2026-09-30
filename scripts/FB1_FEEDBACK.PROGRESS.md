# FB1_FEEDBACK progress (branch wt/fb1-user-feedback, base 66db869f1)

## DONE
- Item 3 AzCalendar light/dark: e9fbed1ed (RED test) + 0dc871134 (fix, app CSS only; Button variants checked, no engine bug).

## IN PROGRESS
- Item 4 (AzBuilder get_mode / set_mode ops + page sync).

## NEXT
- Items 1 / 2 (AzMeet input stretch, stats re-wrap on resize): the headless backend resizes through the
  RESTYLE relayout (full reconcile, every taffy cache cleared by the clone) while macOS / Windows / X11 /
  Wayland resize through the RESIZE fast path (`resize_only_hint`: retained tree, taffy caches kept).
  Headless reproduces nothing (probes: lobby widths 520 / 461.4 stable over typing, focus, status change,
  resize 350..1400, dpi; call view stats re-wrap correctly). Suspect: the taffy bridge's final-layout memo
  (PerformLayout hit) skips subtree side effects (IFC line layout, children's used_size / offsets) that a
  measure pass in the same pass overwrote - only reachable when taffy caches survive a pass (the resize
  fast path; deac0bebb cleared them on the clone for the same class of bug, 4d0aa30c5 then added a path
  without the clone).

## Open questions
- none yet

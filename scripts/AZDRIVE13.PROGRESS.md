# AZDRIVE13 - AzDrive: Explorer 8 ribbon, File menu, breadcrumb, lazy listing (progress)

Worktree: .claude/worktrees/agent-a151d7fb9f3971a28 (branch worktree-agent-a151d7fb9f3971a28,
fast-forwarded to a4d14a02c). No cargo here; the lead builds and runs everything.

## Findings (why the path bar was "a poor text field")
- The trail inside AddressBar is the web Breadcrumb: blue links + "/" separators in a white
  field box -> looks like text typed into a field (layout/src/widgets/address_bar.rs).
- A click on the CURRENT crumb (no callback) bubbled to the field and started an edit; the path
  TextInput never took the keyboard (no autofocus), so Escape / focus-lost could never end the
  edit: the bar stayed a text field until the next navigation.
- Listing: LocalDrive::list reads the WHOLE directory and stats EVERY entry per 500-entry page,
  then sorts; "Load more" repeats it -> quadratic; the tree's expand does the same per folder.
- Views: Details / List / Content / Tiles / Small icons put every row in the DOM; the IconGrid
  builds an IconGridItem per entry per rebuild; thumbnails for the first 120 pictures, not the
  ones in view.

## Plan (commit units)
- W1 (widgets, sub-agent): RED tests for the address bar (path field focus, current crumb).
- W2 (widgets): AddressBar = Explorer's address row (round Back/Forward, recent, Up, box with
  location icon, crumb + chevron segments, overflow, Refresh in the box, search) flat + flora.
- W3 (widgets): RibbonFileMenu (mini backstage in a transient window) + RibbonAppButton.menu hook.
- A1 (app): listing model (batches, counts, visible window, stat requests) + tests.
- A2 (app): streaming scan job (read_dir batches, cancel on navigation), stat-in-view job,
  folders-only tree listing.
- A3 (app): virtualized views (VirtualView rows), IconGrid items on demand.
- A4 (app): the ribbon (Home / Share / View; Computer at This PC), File menu, no title row,
  window title = path.
- A5 (app): address bar usage; A6: E2E.

## DONE
- 77dd5beaf A1 listing model (listing.rs) + allocation-free compare_entries, Entry.known.
- (next commit) A2+A3: Job::Scan (read_dir batches, cancel on navigation), Job::Stat (rows in
  view), Job::Count (read_dir counts: folder sizes column, details pane, This PC tiles),
  Job::Folders (tree, no stat per entry); refresh keeps the old rows until the scan ends;
  virtual folder view (ui_view::folder_view / folder_lines, fixed line heights, group headers as
  lines), IconGrid items on demand, thumbnails only in view, keyboard reveal.

## IN PROGRESS
- A4 ribbon + File menu wiring (waiting for the widget sub-agent's RibbonFileMenu API - names
  fixed in the spec, see the sub-agent prompt).

## NEXT
- A4, A5 (address bar usage: with_icon / with_available_width), A6 E2E.

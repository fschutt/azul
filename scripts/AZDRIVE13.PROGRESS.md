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
- db6aadb6b W1 RED address bar tests (sub-agent); a24b1bd20 W2 AddressBar = Explorer 8's row;
  179a9ce5e W3 RibbonFileMenu + RibbonAppButton.menu hook (sub-agent's progress file:
  scripts/AZDRIVE13_WIDGETS.PROGRESS.md).
- 77dd5beaf A1 listing model (listing.rs) + allocation-free compare_entries, Entry.known.
- ee0f39581 A2+A3: Job::Scan (read_dir batches, cancel on navigation), Job::Stat (rows in
  view), Job::Count (read_dir counts: folder sizes column, details pane, This PC tiles),
  Job::Folders (tree, no stat per entry); refresh keeps the old rows until the scan ends;
  virtual folder view, IconGrid items on demand, thumbnails only in view, keyboard reveal.
- 0246239fa A4: the ribbon (Home / Share / View; Computer at This PC), the File menu, no title
  row (tabs in the titlebar), window title = path, --open, Move to / Copy to recent folders +
  the system folder dialog, Print, Hide selected items.
- 0c29b7c77 A6: scripts/azdrive_e2e.py (ribbon, breadcrumb, File menu, 3,000 files) and
  examples/azul-drive/scripts/browse.py.
- 88a14361c the open folder's own crumb refreshes it; selected_entries' empty fast path.
- 7e1cfb4e2 Share > Copy link: S3Drive::presigned_get_url (SigV4 query signature, 7 days at
  most, checked against AWS's documented example) for the selected files of an S3 drive.
- acae883b4 E2E: `printed` is re.findall - "Name (asc|desc)" answered tuples, never "desc".
- Independent compile reviews (read-only sub-agents): app diff - no compile error found (given
  the api.json entries), four logic bugs; widget diff - no compile error found (api.json /
  codegen must follow: the dll transmutes AddressBar and RibbonAppButton).
- 8d60194e8 RED: a failed stat never answered; lines_of overflow at usize::MAX columns; E2E
  step 20 recount (Counted: 17 files + 6 = 23 after F5).
- 41c829455 a sort by Size / Date always ends with its sort (failed stats answered; an answer
  that changed nothing keeps the chain going).
- 7562b380d F5 counts subfolders again (asked once per listing, not per window).
- 751eaaefd the rows in view from the view's real box (recorded by the virtual view) and the
  window's real height (start, window-created, resize).
- 3f94a4170 a failed open / unlock ends the listing ("This folder could not be read.").
- de303b0e5 saturating line / grid arithmetic; F5 keeps old sizes until the new ones are in.

## NEXT
- the lead: api.json entries (final report), codegen, build, the tests and the E2E.

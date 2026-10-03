# MEETDRIVE6 progress (wave 6, branch wt/meetdrive6 from 25d78e309)

Tools (not committed, under target/md6/): probe.py (start an app headless + exec a snippet with
`app`), drive.sh <snippet> [app args] (AzDrive under run_capped with a temp HOME). Commit messages
go through scratchpad/md6/msg.txt (the scratchpad root is shared with other agents - never use
scratchpad/msg.txt).

## DONE
- 7e7d5b8b4 progress file
- d9cad7751 azdrive_e2e.py step 2 double-clicks the drive tile's icon, not its centre
- 1f87aee56 AzDrive ids.rs (`__azdrive_` + snake_case, const AzString via from_const_str =
  SMALL6's codegen 182655027); d97fdc43d ui_view, 14b234103 ui_panes, bef413cd9 ui_dialogs
  switched; 1bf7dc449 azdrive_e2e.py C()/I() helpers (detects old/new naming); 8596e1fd4 browse.py

## AzDrive plan (in order; each unit RED first where it is behaviour)
1. DONE prefix
2. `.azlin/` never listed (model/browse filter) + an "Azlin" built-in drive at the data root
3. appkit: args (AppArgs + `--layout` split off), data root, settings file under
   `<data root>/drive/settings.json` with theme + mode remembered, About via AboutDialog,
   shortcuts table (appkit Shortcut list = the keys.rs table)
4. MessageBox for ConfirmDelete / ConfirmForget (D12), ProgressDialog for transfers
5. ListSelection instead of model::Selection (D20)
6. dedup: format_size -> DiskSpace::format_bytes (D24), now_secs -> azul_storage::time (D23),
   dead browse::{up, crumbs, upload_key} (N6), ribbon helpers -> with_items (D9)
7. LOOK fixes: "1 drives", Quick access (0) row, nav pane column, disabled ribbon text
## AzMeet plan
1. ids `__azmeet_`; 2. initials via azul_pim; 3. settings + theme/mode via a Drive on a Thread in
`<data root>/meet/`; 4. per-meeting folder `meet/<meeting>/chat.jsonl`; 5. E2E nested-runner fix

## IN PROGRESS
- LOOK (paused: battery warning - no long headless runs until told otherwise)

## NEXT
- when power allows: run scripts/azdrive_e2e.py (prebuilt AzDrive) through run_capped, look at
  every screenshot (target/md6/drive-shots), then AzMeet (run the WHOLE meet E2E under ONE
  run_capped with `--capped ""`: the runner holds a machine-wide lock, nested runners deadlock)
- meanwhile (no runs needed): code work that does not depend on seeing:
  1. AzDrive `ids` module: `__azdrive_` prefixed const AzString ids/classes (from_const_str is
     NOT in the generated azul crate - check `AzString` ctor options first; fallback: `const &str`
     names defined once + `AzString::from(ids::X)`), update azdrive_e2e.py + browse.py
  2. AzMeet `ids` module `__azmeet_`, initials via azul_pim, settings through a Drive on a Thread
  3. both apps on azul-appkit (args via AppArgs + app-only switches split off first)

## Seen broken (LOOK so far)
- AzDrive This PC (flat light, 01-this-pc.png): renders. Broken:
  - ENGINE (owner HEADLESS6, events plumbing dll/src/desktop/shell2/common/event.rs ~9370):
    a Hover event aimed into a VirtualView child DOM never bubbles out to the parent DOM. The
    drive tile's capacity bar is a ProgressBar = VirtualView (DOM 1); a double-click on the bar
    targets DOM 1 node 1, `propagate_event` runs only inside `event.target.dom`, so the Tile's
    DoubleClick (DOM 0) never fires: the Home drive does not open from the middle of its tile.
  - disabled ribbon labels look smeared / double-drawn (zoom: crop-ribbon.png)
  - navigation pane: an empty ~50px column right of the tree with a floating "<" button
  - "Quick access (0)" group still shows a "Quick access" row
  - "1 drives" (status bar and details pane): plural for one
  - a11y warning every frame: nodes 250 and 259 have a callback but no accessible name

## Decisions
- AzMeet keeps android/ios link-static targets: use azul-appkit WITHOUT its `azul` feature
  there (plain args/data/settings/about/shortcuts), so no link-dynamic leaks into mobile builds.

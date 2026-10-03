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

- d7f0db3bd RED / 935dfecd6 GREEN args on azul_appkit::AppArgs + --layout (D2)
- 6c8141ee0 keys::SHORTCUTS (appkit Shortcut table) + test each runs a command
- 9ffa8ca6e AzDrive on appkit: create_kit (data root, saved theme/mode), window_options /
  app_config, --shot, built-in "Azlin" drive (data tree), view settings -> drive/view.json in the
  data tree, FILE > Options = appkit settings_page (View/Navigation/Drives + kit's), About =
  AboutDialog, Mod+, / F1 / Escape via handle_key; AZDRIVE_SETTINGS removed
- 8fc69bb52 E2E: AZLIN_DATA, step 14 saves Flora into drive/settings.json

## AzDrive plan (in order; each unit RED first where it is behaviour)
1. DONE prefix
2. `.azlin/` hiding is INFRA6's LocalDrive (is_reserved_key, never listed); DONE "Azlin" drive.
   NOTE for report: write_sample(home) uses LocalDrive::new -> after INFRA6 it would keep a
   manifest in the user's HOME: switch to LocalDrive::without_manifest (INFRA6 API) - TODO
3. DONE appkit
- 686bd7cad without_manifest for home/upload/download (INFRA6 API)
- 6b1dc4734 MessageBox for ConfirmDelete/Forget; 737343e87 RED / 1d0125531 GREEN queue
  wants_progress_dialog; a848a5d4e ProgressDialog in the transfers popup (auto after 2 s)
- 8f37795b9 dead browse helpers (N6), now_secs -> azul_storage::time (D23)
- 19b08611c RED / ca83a0073 GREEN browse::counted ("1 drive")
- ALREADY DONE before wave 6: ListSelection (model::Selection wraps it), format_bytes, ribbon
  with_items, no ctrl||meta in AzDrive/AzMeet
- DEFERRED (widget, report): ShellNavigationPane flat/flora nav_root width 230px fixed, inside
  BrowserShell's ratio-sized tree pane (~280) -> an empty column + floating "<" chevron
4.-7. mostly DONE; left: E2E client dedup (D29), LOOK items needing a run
4. MessageBox for ConfirmDelete / ConfirmForget (D12), ProgressDialog for transfers
5. ListSelection instead of model::Selection (D20)
6. dedup: format_size -> DiskSpace::format_bytes (D24), now_secs -> azul_storage::time (D23),
   dead browse::{up, crumbs, upload_key} (N6), ribbon helpers -> with_items (D9)
7. LOOK fixes: "1 drives", Quick access (0) row, nav pane column, disabled ribbon text
## AzMeet plan
1. ids `__azmeet_`; 2. initials via azul_pim; 3. settings + theme/mode via a Drive on a Thread in
`<data root>/meet/`; 4. per-meeting folder `meet/<meeting>/chat.jsonl`; 5. E2E nested-runner fix
- 11982ebd8 initials via azul_pim (B24)
- bb1f2c043 RED / 4cb1d8dd1 GREEN ids.rs `__azmeet_`
- 58fb8e816 azmeet_e2e.py / azmeet_cpu.py: App.id() prefixed ids, ONE outer runner
- 5d190cbb3 RED / 4a6ddea0c GREEN store.rs layout (meet/settings.json, meet/<m>/meeting.json,
  meet/<m>/chat.jsonl); 650ef3495 store data_root / load_settings / save on a Thread
  (NOT wired into lib.rs yet)

## Resume 2 (2026-10-03, after the power loss)
- Battery 1% on AC (charging): code work first, the LOOK runs once the battery is above ~20%.
- 3c0755ec3 RED / 0287cec63 GREEN AzMeet args on AppArgs + --name (D2)
- 1454d22b9 RED / 83cde692a GREEN store Prefs (server/name/quality) + single save thread queue
- a628f46ad store wired into lib.rs: files_root() (headless only with --data-dir/AZLIN_DATA),
  saved_settings(), remember(), enter_record(), note_people(), flush_files(); settings.txt gone
- d915a62ee rooms.rs settings.txt encoding removed; 74780b997 demo code from random_seed
- Decision: the legacy <config>/AzMeet/settings.txt (one server URL) is NOT migrated
  (pre-release; the next server that answers is saved again).

## IN PROGRESS
- LOOK (paused: battery warning - no long headless runs until told otherwise)

## NEXT
- AzMeet: About section = AboutDialog (ui.rs settings `_ =>` arm); shortcuts as an appkit
  Shortcut table (keys.rs like AzDrive's) checked by a test; E2E: AZLIN_DATA per app, check
  meet/<room>/chat.jsonl + meet/settings.json (AZMEET_SAVED lines)
- then the LOOK runs (battery permitting)
(older NEXT below)
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

- LOOK run 2 (2026-10-03 ~06:00, prebuilt aa59b2d84, scripts/azdrive_e2e.py): steps 1-4 pass,
  step 5 FAILS at Ctrl+A:
  - ENGINE (owner WRITER6: core/src/events.rs shortcut block, handle_key_down ~5262): with ANY
    node focused (here `main#shell-content`, the OfficeShell pane a click focuses) the
    primary-modifier shortcuts Copy / Cut / Paste / SelectAll are `AddAndSkip`: the app's
    window-level VirtualKeyDown never sees Cmd+A / C / X / V. WRITER6 fixed Undo/Redo only
    (return None -> default action). Same fix needed for the other four when
    `!focus_is_editable` (or as vetoable default actions). Unfocused: Cmd+A works (probe ka.py).
  - FIXED (4aaa20534 RED / 92c40b64f): disabled ribbon labels smeared in LIGHT mode = LCD text
    inside an opacity layer (transparent clear) blended against black + stamped opaque.
    Not seen in dark mode. Remaining (report): the opacity layer composite
    (blit_pixmap_clipped) treats premultiplied layer content as straight alpha - AA edges in a
    faded group come out slightly dark (much milder after the fix).
  - 03-tiles.png after cycling all layouts showed overlapping icons + smeared text; a fresh
    `--layout tiles` start is clean (dl2.png) -> captured mid layout-change animation (opacity
    + moving items), not a layout bug.

## Decisions
- AzMeet keeps android/ios link-static targets: use azul-appkit WITHOUT its `azul` feature
  there (plain args/data/settings/about/shortcuts), so no link-dynamic leaks into mobile builds.

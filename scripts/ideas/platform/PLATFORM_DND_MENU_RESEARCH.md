# Platform integration research — macOS menu + file DnD (Win/X11/Wayland)

Research 2026-06-20 for round-2 items 6–9 of `HIGHLEVEL_1_5_PLAN.md`. Implement BLIND
(no live runtime test here): compile-verify per platform + mirror existing patterns.

**Shared substrate:** `layout/src/managers/file_drop.rs` `FileDropManager` is complete +
platform-agnostic: `set_hovered_file(Some/None)` (L44), `hover_was_cancelled`/
`clear_hover_cancelled` (L53/60), `set_dropped_file` (L75). Events `FileHover`/
`FileHoverCancel`/`FileDrop` are DERIVED from manager state in
`layout/src/event_determination.rs:641-649` (not pushed). Canonical wiring = macOS item-1:
`dll/src/desktop/shell2/macos/events.rs` `handle_file_drag_entered`(827)/`handle_file_drop`(860)/
`handle_file_drag_exited`(888) — each: save-prev-state → `set_*` → `update_hit_test` →
`process_window_events(0)` → route result. B/C/D should each add the same three `handle_file_*`.

---

## A. macOS menu bar + context menu — WIRE-UP (~85% built)
- Context menus FULLY wired: right-click → `try_show_context_menu` (macos/events.rs:~1102) →
  `popUpMenuPositioningItem:atLocation:inView:`. NSMenu conversion complete in
  `macos/menu.rs` (`MenuState`/`update_if_changed`/`build_menu_items`/`set_menu_item_accelerator`,
  `AzulMenuTarget` define_class `menuItemAction:` → `PENDING_MENU_ACTIONS` drained at mod.rs:~5970
  → `handle_menu_action` ~4707). Core types `core/src/menu.rs`; DOM `menu_bar`/`context_menu`
  (core/src/dom.rs:~1808/1810, `NodeData::get_menu_bar()`).
- GAP: app menu bar never populated from DOM. `set_application_menu` (mod.rs:~4922) zero callers;
  ctor empty `MenuState::new()` (TODO mod.rs:~3508); launch = hardcoded Quit stub `setup_main_menu`
  (mod.rs:~1889).
- DO: at ctor read root `get_menu_bar()`, build MenuState, `app.setMainMenu(...)`. Prepend standard
  app submenu (app name + Quit→`terminate:`) — first submenu MUST be the app menu. One helper
  `create_main_menu_bar` for launch + updates; re-apply post-relayout (update_if_changed hash-guard
  is cheap). Bar + context share `command_map`/`next_tag` — always allocate via `next_tag`/
  `merge_callbacks`. Versions: objc2 0.6.0 + objc2-app-kit/foundation 0.3.0.

## B. Windows file DnD — OLE IDropTarget (replaces legacy WM_DROPFILES drop-only)
- Exists: `DragAcceptFiles`+`WM_DROPFILES` (windows/mod.rs:485, arm 3779-3844, `DragQueryFileW`).
  Win32 via dlopen (windows/dlopen.rs) EXCEPT accessibility uses the `windows` crate.
- API: `OleInitialize` (NOT CoInitialize — RegisterDragDrop needs STA else E_OUTOFMEMORY) →
  `RegisterDragDrop(hwnd, &target)`/`RevokeDragDrop`. Implement `IDropTarget`
  (DragEnter/DragOver/DragLeave/Drop) via `#[implement(IDropTarget)]` from `windows::core`;
  `.into::<IDropTarget>()` gives lifetime to COM (don't also Box). Paths: `GetData` with
  `FORMATETC{CF_HDROP,DVASPECT_CONTENT,-1,TYMED_HGLOBAL}` → HDROP → `DragQueryFileW` loop →
  `ReleaseStgMedium`. Set `*pdwEffect = DROPEFFECT_COPY`/`NONE` every call.
- Crate: `windows v0.62` (dll/Cargo.toml:314, optional). ADD features `Win32_System_Ole`,
  `Win32_UI_Shell` (+maybe `Win32_System_Com_StructuredStorage`). Metadata-only → safe for
  windows-gnu cross-compile.
- DO: factor WM_DROPFILES body into `handle_file_drag_entered/exited/drop` (mirror macOS), forward
  from the COM methods, route via `route_main_window_result`. Register at ctor (replace
  `DragAcceptFiles`), `RevokeDragDrop` on WM_DESTROY before HWND dies. REPLACE WM_DROPFILES. `pt`
  is screen coords (reuse cached cursor pos like macOS).

## C. X11 file DnD — XDND protocol (none today)
- Exists: raw Xlib dlopen (x11/dlopen.rs, defines.rs); event loop `handle_event` (mod.rs:2058),
  ClientMessage arm 2072 (only WM_DELETE). Bound: XInternAtom/XSendEvent/XChangeProperty/
  XGetWindowProperty. ADD: XConvertSelection, SelectionNotify=31. Add the three handle_file_* to the
  X11 window.
- Protocol (https://www.accum.se/~vatten/XDND.html): set `XdndAware`=5 (XA_ATOM) on top-level at
  creation. Dispatch on `event.message_type` (NOT data.l[0] = source XID). Seq: XdndEnter (check
  l[2..4]/XdndTypeList for `text/uri-list`) → XdndPosition (l[2]=(x<<16)|y root coords, l[4]=action;
  MUST reply XdndStatus with accept bit or no drop) → XdndDrop → `XConvertSelection(XdndSelection,
  text/uri-list, prop, win, time)`, wait SelectionNotify → XGetWindowProperty, parse uri-list (CRLF,
  skip `#`, percent-decode, strip `file://`) → handle_file_drop → send XdndFinished. XdndLeave →
  set_hovered_file(None). Gotchas: XdndStatus mandatory; coords root-relative (translate); never read
  property synchronously after XConvertSelection (wait for SelectionNotify).

## D. Wayland file DnD — wl_data_device (none today)
- Exists: raw libwayland dlopen (wayland/dlopen.rs, defines.rs, events.rs). Registry handler
  events.rs:260 (bind arms ~270-351), WL_SEAT_LISTENER (85), dispatch via wl_display_dispatch_queue*
  (mod.rs:539/1763). xdg-decoration (mod.rs:1320) is the idiom to copy for get_data_device.
- Protocol (wayland.app / wayland-book.com): bind `wl_data_device_manager` at version.min(3),
  `get_data_device(seat)`. Per drag: data_offer→`offer`(collect MIME, want text/uri-list)→`enter`
  (save serial; MUST `accept(serial,"text/uri-list")` + v3 `set_actions(copy)` or drop rejected;
  set_hovered_file)→`motion`(re-accept)→`leave`(set_hovered_file(None)) or `drop`. On drop:
  `pipe2(O_CLOEXEC)`, `wl_data_offer.receive("text/uri-list", write_fd)`, `wl_display_flush`, close
  write end, read read-fd to EOF, parse uri-list, set_dropped_file, v3 `finish`+`destroy`.
- Crate: raw dlopen libwayland (no Rust wayland crate in dll). wl_data_device_manager/device/offer
  are core wayland.xml — no extra XML. ADD C types/listeners to defines.rs, load *_interface symbols
  (mirror wl_seat_interface), registry arm + try_init_data_device (mirror try_init_tablet).
  Gotchas: respond during enter/motion with the ENTER serial; version-gate set_actions/finish/
  source_actions (v3+) else <v3 proxy disconnects; flush before closing write fd or deadlock;
  listeners must be 'static; coords are wl_fixed (>>8); hover path unknown until drop (placeholder).

---
Verify: macOS `cargo check -p azul-dll --features build-dll` (host); Windows `--target
x86_64-pc-windows-msvc`; Linux `--target x86_64-unknown-linux-gnu` (TARGET-SCOPED env
`CC_x86_64_unknown_linux_gnu=...`/`CXX_*`/`AR_*`/`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=...`,
NOT global CC). ENOSPC: `rm -rf target` if disk fills (~38G).
</content>

# File drag-and-drop — state of the art + a plan for the missing half (2026-08-24)

Question: can a user drag a file **into** and **out of** an azul window, on every
OS, and is it properly done?

## 0. Answer up front

- **Drag-IN (app is the drop target): DONE and clean on all four backends** +
  headless. A file dragged from the OS file manager onto a window fires
  `FileHover` / `FileDrop` / `FileHoverCancel` (Hover-targeted AND Window level),
  and the app reads paths via `CallbackInfo::get_hovered_file(s)` /
  `get_dropped_file(s)`. One modernization nit on macOS (below).
- **Drag-OUT (app is the drag source — drag a file OUT of the window): NOT
  IMPLEMENTED anywhere.** No app API, no source role on any backend.

So: half done. This plan is the drag-out half, plus the macOS nit.

## 1. What already exists (verified in the tree)

Events (`core/src/events.rs`): `EventType::FileHover|FileDrop|FileHoverCancel`
-> filters `HoveredFile|DroppedFile|HoveredFileCancelled`.
Storage (`FullWindowState`): `set_hovered_files` / `set_dropped_files`, read by
`get_hovered_file(s)` / `get_dropped_file(s)`.
Drag manager (`layout/src/managers/drag_drop.rs`): `DragType::{Node,File}`,
`DragState` (the File arm describes an INCOMING file drag).
CallbackInfo: `get_hovered_file(s)`, `get_dropped_file(s)`, `is_file_drag_active`,
`get_dragged_file`, `get_drag_types`, `get_drag_state`.

Per backend (drag-IN):
- **macOS** `macos/events.rs` + `mod.rs`: `NSDraggingDestination` -
  `registerForDraggedTypes:` at view creation, `draggingEntered/Updated/Exited:`
  + `performDragOperation:`. Reads **`NSFilenamesPboardType`** (deprecated).
- **Windows** `windows/dnd.rs`: full **OLE `IDropTarget`** via the `windows`
  crate's `#[implement(IDropTarget)]`; `OleInitialize(None)` on the UI/STA thread
  + `RegisterDragDrop`. The legacy `WM_DROPFILES`/`DragAcceptFiles` path was
  removed on purpose. Reads `CF_HDROP` -> `DragQueryFileW`.
- **X11** `linux/x11/mod.rs` (`XdndState`) + `events.rs`: full **XDND target
  role** - `XdndAware`/Enter/Position/Status/Leave/Drop/Finished, then
  `XConvertSelection(XdndSelection, text/uri-list)` -> `SelectionNotify`, parses
  `file://` URIs, sends `XdndFinished`.
- **Wayland** `linux/wayland/events.rs`: `wl_data_offer` enter/leave/motion/drop,
  `receive(text/uri-list, fd)` read on a WORKER thread through a pipe (the
  `uri_list_from_pipe` helper), parses `file://` URIs.
- **headless**: a synthetic `HeadlessEvent::FileDrop { x, y, paths }`.

Tests: `layout/src/event_determination.rs` has FileHover/FileDrop/FileHoverCancel
unit tests. **No headless e2e, and NO demo exercises it** (a gap worth closing
alongside the drag-out work, since it is the only end-to-end proof).

## 2. What is missing (drag-OUT), and why it is real work

None of the source-role primitives exist:
- no `CallbackInfo::start_file_drag`;
- macOS: no `NSDraggingSource` / `beginDraggingSessionWithItems:` /
  `NSFilePromiseProvider`;
- Windows: no `IDataObject` / `IDropSource` / `DoDragDrop` (the `windows` crate's
  `#[implement(IDataObject)]` is available; only `IDropTarget` is used today);
- X11: no XDND SOURCE role (the target role is complete; `XdndSelection` is
  interned but only ever *converted*, never *owned*);
- Wayland: `wl_data_source` + its listener struct exist but are wired ONLY for
  clipboard copy; the DnD listener arms (`dnd_drop_performed`/`dnd_finished`/
  `action`) are empty stubs, and `wl_data_device.start_drag` is never called.

The genuinely hard part everywhere is **producing the file bytes on demand**
("promised" / "delay-rendered" / "virtual" files): the receiver may be another
app that only pulls the bytes after the drop, to a location it chooses. An app
should be able to drag out a file *that does not exist on disk yet* (a rendered
export, a generated report).

## 3. Design — ONE app API, ONE shared abstraction

### 3a. The app API (in `azul-layout::callbacks::CallbackInfo`)

```
info.start_file_drag(FileDragData)          // starts a native drag-out
```

Called from a `DragStart` (or MouseDown) callback, which already runs INSIDE a
pointer grab - this satisfies the hard constraint that a drag-out may only begin
from a live mouse event (macOS `beginDraggingSession...`, Wayland `start_drag`
needs an input serial from an unreleased button, Windows `DoDragDrop` is called
synchronously from the UI thread). `DragStart` already reaches
`dispatch_events_propagated` and resolves its source at the press point, so the
grip/handle a file hangs off is known.

`FileDragData` (in `azul-core`, FFI-C):
```
enum FileDragItem {
    /// A file that exists: hand the OS the path.
    Path(AzString),
    /// A file produced on demand: name + a byte-producer callback.
    Promised { file_name: AzString, mime: AzString, produce: FileByteProducer },
}
struct FileDragData { items: FileDragItemVec, preferred_action: DragAction }
// DragAction = Copy | Move | Link
```

`FileByteProducer` is a C-ABI callback `fn(RefAny) -> U8Vec` (or a streaming
variant `fn(RefAny, offset, len) -> U8Vec` for large files - phase 2). It runs
when the receiver pulls: macOS `writePromiseToURL:`, Windows
`GetData(CFSTR_FILECONTENTS)`, X11 `SelectionRequest`, Wayland
`wl_data_source.send`. It may run on a WORKER thread (Windows async, Wayland fd
write) - so it takes a `RefAny` payload, not `&CallbackInfo`.

### 3b. The shared engine half (`azul-layout`)

A process-global request channel exactly like the eyedropper's
(`managers::eyedropper`: `push_request`/`drain_requests` + a per-backend trait
hook run inside the event pass):

- new `managers::file_drag`: `push_drag_request(FileDragData, serial_hint)`,
  `drain_drag_requests()`. The producer callbacks + their `RefAny`s live here,
  keyed by a request id, so the backend can call them from any thread.
- `PlatformWindow::start_file_drag(request)` trait hook (default: no-op), driven
  from `process_window_events` the way `dispatch_eyedropper_requests` is. On
  Wayland it carries the input serial from the current pointer grab.
- outcome (`Copy`/`Move`/`Cancelled`) comes back as a window-level
  `EventType::FileDragFinished` (new; mirror `ScreenColorPicked`), read via
  `get_file_drag_result()`. Move semantics: the app deletes the original when it
  sees `Move` (Windows `CFSTR_PERFORMEDDROPEFFECT`, macOS ended-operation,
  Wayland `dnd_finished` action, XDND `XdndFinished` action).

### 3c. The one cross-platform mapping (the whole point)

`FileDragData` -> per backend:
| | real path | promised bytes |
|---|---|---|
| macOS | `NSURL` on the drag pasteboard | `NSFilePromiseProvider` + delegate `writePromiseToURL:completionHandler:` |
| Windows | `CF_HDROP` (`DROPFILES`) | `CFSTR_FILEDESCRIPTOR` + `CFSTR_FILECONTENTS[i]` as `IStream`, `IDataObjectAsyncCapability` for big data |
| X11 | own `XdndSelection`, serve `text/uri-list` = `file://...` | same selection, but `SelectionRequest` runs the producer (INCR-chunk large data) |
| Wayland | `wl_data_source` offering `text/uri-list` | same source; `send(mime, fd)` runs the producer, writes the fd (worker thread, non-blocking) |

`text/uri-list` (CRLF `file://` lines) is the shared carrier on X11+Wayland;
`CF_HDROP` on Windows; file URLs / promise on macOS - one `FileDragData` maps to
all four.

## 4. Per-backend work (dlopen'd, matching the existing style)

- **macOS** (`macos/mod.rs`, new `macos/dnd_source.rs`): conform the view to
  `NSDraggingSource` (`define_class!` + `draggingSession:sourceOperationMask...`),
  build `NSDraggingItem`s (an `NSURL` writer for Path, an `NSFilePromiseProvider`
  + `NSFilePromiseProviderDelegate` for Promised), call
  `beginDraggingSessionWithItems:event:source:` from the mouse handler. Needs the
  AppKit features `NSDraggingItem`, `NSFilePromiseProvider`, `NSPasteboardItem`.
  The delegate + provider must be retained for the session. ~1 day.
- **Windows** (`windows/dnd.rs`): add `#[implement(IDataObject)]` (advertise
  `CF_HDROP` and/or `CFSTR_FILEDESCRIPTOR`+`CFSTR_FILECONTENTS`) and
  `#[implement(IDropSource)]`, call `DoDragDrop` from the UI thread (already
  `OleInitialize`d STA). `GetData(CFSTR_FILECONTENTS[i])` returns an `IStream`
  backed by the producer. `IDataObjectAsyncCapability` for large data (phase 2).
  ~1.5 days (IDataObject is the fiddly one).
- **X11** (`x11/mod.rs`+`events.rs`): add the SOURCE role - own `XdndSelection`
  at drag start, walk the window tree under the cursor (`XQueryPointer`) for
  `XdndAware`, send `XdndEnter/Position`, honour `XdndStatus`, send `XdndDrop`
  on release, serve `text/uri-list` from `SelectionRequest` (INCR for big data),
  finish on `XdndFinished`. This is the most protocol code (mirror of the target
  role we already have). ~2 days.
- **Wayland** (`wayland/mod.rs`+`events.rs`): `create_data_source()`, `offer`,
  `set_actions` (before `start_drag`), `wl_data_device.start_drag(source, origin,
  icon?, serial)` with the serial from the pointer grab; the `send(mime, fd)`
  arm writes the producer's bytes to the fd on a worker thread (reuse the
  clipboard/DnD pipe machinery); destroy on `cancelled`/`dnd_finished`. Optional
  drag icon surface. ~1 day (much scaffolding exists).
- **headless**: a `start_file_drag` that records the request + answers
  `Cancelled` (or a scripted outcome) so the e2e can assert the request shape and
  the producer runs.

## 5. macOS drag-IN modernization (small, do alongside)

`macos/events.rs` reads the deprecated `NSFilenamesPboardType`. Modern macOS
(11+) drags deliver `NSPasteboardTypeFileURL` (an array of `NSURL`s). Register
BOTH and prefer file-URLs, falling back to filenames - so newer senders that no
longer put filenames on the pasteboard still work. ~0.5 day.

## 6. Effort, risks, order

Total ~6-7 days for full drag-out on all four with promised files, + 0.5 for the
macOS nit. Phase it:

1. **API + engine channel + headless** (the `FileDragData` type, the
   `managers::file_drag` channel, `CallbackInfo::start_file_drag`,
   `FileDragFinished`, the trait hook, a headless e2e that drives a `DragStart`
   -> `start_file_drag` with a Path and a Promised item and asserts the producer
   ran). Stop-point: the vocabulary is real and tested with no native code.
2. **Real paths only, per backend** (drag a file that EXISTS out to the file
   manager). Stop-point: on-screen, drag a listed file out on each OS.
3. **Promised files** (the byte-producer path) per backend, Windows async +
   X11 INCR last. Stop-point: drag out a generated file that isn't on disk.
4. **macOS drag-IN file-URL modernization.**

Risks: (a) COM `IDataObject` correctness on Windows (enumerator + per-lindex
FILECONTENTS is the classic footgun - `ReleaseStgMedium` discipline); (b) the
producer runs off the main thread on Windows/Wayland, so it must take a `RefAny`,
never touch the DOM, and be `Send` - enforce by the callback signature; (c)
Wayland/macOS "must start from a mouse event" - solved by requiring
`start_file_drag` from a `DragStart`/MouseDown callback and threading the input
serial; (d) X11 source role is a lot of async selection code - reuse the target
role's selection plumbing.

Verification: headless e2e per phase (request shape + producer invocation +
Move/Copy outcome); on-screen per OS for the real-path and promised paths (macOS
here, Linux/Windows type-checked + whatever the user can run). A first demo:
AzWidgets gets a "drag this file out" chip and a "drop files here" zone, closing
the no-demo gap for BOTH directions.

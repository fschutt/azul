# OS-Facing APIs on the Web Backend — Boundary-Import Design & API Plan

Status: proposal / planning doc · Date: 2026-08-17
Scope: make azul's OS-facing API surface (file dialogs, file I/O, HTTP, clipboard,
timers/threads, sensors, media, future iroh) work **from the same user source code**
on both the desktop backend and the web-lift backend, using the existing
`FnClass::BoundaryImport` interception mechanism as the foundation.

Placement note: this doc lives at `doc/` root next to `webtransport-plan.md` /
`SUPER_PLAN_0.2.0.md` (the repo's established planning location).

---

## 1. Executive summary + recommendation

The web backend lifts the app's native x86-64 code to wasm via remill. Lifted code
can compute anything, but every syscall-shaped leaf (OS DLL import, out-of-image
libc) is stubbed to a no-op that returns 0 (`symbol_table.rs:784-846`, the M10-A1
out-of-image → `Leaf` pass). Today that means **every OS-facing Az API "works" on
web only in the sense that it silently returns None/Err/garbage** — e.g.
`AzFileDialog_openFile` lifts tfd's Rust wrapper until it hits `GetOpenFileNameW`
(a stub) and returns "cancelled"; `AzHttpRequestConfig_httpGet` lifts ureq until
the socket syscalls stub out.

The interception point already exists: under `AZ_ENABLE_SHARDS`, every api.json
`Framework` symbol classifies as `FnClass::BoundaryImport` — it is *not* lifted
into the caller; the caller's wasm gets an `env.sub_<hex>` import, and loader.js
wires that import at instantiate time (`symbol_table.rs:2538-2550`,
`transpiler_remill.rs:7709-7717`, `loader_js.rs:302-304`). Today the wiring target
is always a *lifted shard* of the native body; **nothing prevents the wiring target
from being a real JavaScript implementation instead.** That substitution — per
Az-function, driven by api.json metadata — is the whole interception design.

**The async problem.** The desktop API is synchronous ("open dialog, get path
back"); every browser equivalent except `alert`/`confirm` is async. Four options
were evaluated (§4): (a) callback-based API redesign, (b) JSPI
(`WebAssembly.Suspending`), (c) worker + SharedArrayBuffer + `Atomics.wait`
sync bridge, (d) hybrid.

**Recommendation: (d) hybrid, with (a) callback/continuation APIs as the one
portable surface, and (b) JSPI as a later Chromium-only compatibility layer for
the legacy sync functions.** Rationale:

1. azul is already callback-driven — user code *only ever runs inside callbacks*,
   and the API already contains three async idioms that fit the browser exactly:
   result-callback structs (`FileInputOnPathChangeCallback`), fire-and-forget +
   poll (`request_biometric_auth` → `get_biometric_result`, keyring, geolocation,
   sensors), and background-owner + message + poll-from-timer (`WebTransport::recv`
   "Poll from a timer", `ThreadSender`/`ThreadReceiver`). The redesign is
   *completing an existing pattern*, not inventing one.
2. The web runtime's re-entry mechanism already exists and is synchronous-in,
   synchronous-out (`AzStartup_dispatchEvent`, `loader_js.rs:802-852`) — a
   promise resolution can re-enter lifted wasm through a new
   `AzStartup_completeRequest` export exactly the way a click does today, and the
   M12.7 indirect-call dispatcher (`transpiler_remill.rs:2462-2521`) already
   routes guest fn-pointers (the stored user callback) to lifted bodies.
3. JSPI is real but not portable in 2026: default-on in Chrome 137+, behind a
   flag in Firefox 139, Safari implementation only started after its objection
   was dropped in late 2025 (it is an Interop 2026 focus area, i.e. *not done*).
   It also suspends the whole wasm activation, which creates a re-entrancy hazard
   with `EventloopState` (§4.2). Worth having later as a "legacy sync API works
   unmodified on Chromium" bonus; not a foundation.
4. Worker+SAB gives true sync (and is the *only* way to get sync OPFS reads via
   `createSyncAccessHandle`), but requires cross-origin isolation and a full
   re-architecture of the loader (wasm off-main-thread, DOM patches proxied).
   Escape hatch, not the plan.

Phase 1 (§6) targets the AzWriter demo: `Pdf::from_dom` already lifts (pure
compute); what's missing is "write bytes → browser download" (a *synchronous,
fire-and-forget* JS boundary — no async redesign needed) and a callback-based
`FileDialog::open_file_with` for file-open (+ the `FileInput` widget ported onto
it).

---

## 2. How the boundary mechanism works today

Verified against source at commit `240b11da6` (branch `weblift/x86-lifter-fixes`).

### 2.1 Classification: api.json → ApiFnClass → FnClass

- `dll/src/web/classify.rs` embeds a brotli-compressed api.json
  (`target/codegen/api.json.br`, `classify.rs:13-16`) and walks
  `version → api → module → classes → {constructors,functions}`
  (`classify.rs:94-121`), synthesizing the C name
  `Az{ClassName}_{snake_to_lowerCamel(fn)}` (`classify.rs:113-114`) — the same
  names `cabi_export` emits in `target/codegen/dll_api_internal.rs`.
- `classify_fn` (`classify.rs:144-150`) maps each name to
  `ApiFnClass::{Framework, ServerEntryPoint, ReplaceWithDomPatcher}`:
  `AzApp_run` → ServerEntryPoint; `AzDisplayList_*`/`AzGl_*` →
  ReplaceWithDomPatcher; **everything else → Framework**. There is currently *no
  per-function metadata in api.json itself* that influences web classification
  (fn entries carry only `fn_args`, `fn_body`, `returns`, `doc`).
- `classify_for_name` (`symbol_table.rs:2365`) is the per-symbol master
  classifier. For api.json names (`symbol_table.rs:2538-2550`):
  `Framework` → `FnClass::BoundaryImport` when `shards_enabled()`
  (`AZ_ENABLE_SHARDS` set and `AZ_BUNDLED_LEGACY` unset, `symbol_table.rs:84-87`)
  else `Recursable` (legacy bundled mode); `ServerEntryPoint` → `NeverLift`;
  `ReplaceWithDomPatcher` → `Leaf`.
- Other precedents in the same classifier show the *"replace an OS behavior with
  a web-correct body"* pattern already in production at the std level:
  `FnClass::EnvVarOs` (`symbol_table.rs:169-178`) gives `std::env::var_os` a body
  that writes `None` ("no environment ⇒ variable unset"); `HashmapRandomKeys`
  returns a fixed seed; `ChkStk`, `LibcMemcpy/Memset/Snprintf` replace
  out-of-image libc. The M10-A1 pass (`symbol_table.rs:784-846`) forces `Leaf`
  (typed extern → env import → JS Proxy stub) on every out-of-image symbol —
  which is what all Win32/libc syscall wrappers become.

### 2.2 The boundary-lift flow (M10-D)

- During any transitive lift (cb / layout / mini), reaching a
  `BoundaryImport`-classified symbol stops the BFS and records the canonical
  address in `used_boundaries` (`transpiler_remill.rs:2362-2364` sequential,
  `:2838-2845` batched; surfaced on `CallbackWasm`/`LayoutWasm` via
  `used_boundaries`, `mod.rs:187-189`, `:242-247`).
- The lifted IR gets a `declare` only — no body (`transpiler_remill.rs:7709-7717`).
  wasm-ld with `--allow-undefined` turns the unresolved `sub_<synth_hex>` into a
  **wasm env import**.
- The orchestrator unions `used_boundaries` across all lifts and calls
  `lift_boundary_to_wasm(addr)` per boundary
  (`transpiler_remill.rs:5501-5554`): it re-lifts that one function (plus its
  `Recursable` deps) into its own shard, exporting the raw remill-shape body
  `sub_<synth_hex>` (`body_export`, `:5518`) — signature
  `(state_ptr: i32, pc: i64, mem_token) -> mem_token`. Result struct:
  `BoundaryShard` (`transpiler_remill.rs:4497-4528`) / `BoundaryWasm`
  (`mod.rs:199-220`).
- The server serves each shard at `/az/fn/<canonical_name>.<hash>.wasm`
  (`server.rs:261-272`) and a manifest listing
  `{name, url, body_export}` at `/az/manifest.json` (`server.rs:273-295`).
- loader.js (`generate_loader_js`, `dll/src/web/loader_js.rs`) at bootstrap:
  `azLoadBoundaryShards()` (`loader_js.rs:323-366`) fetches the manifest,
  instantiates every shard (with shared memory + table via
  `azCallbackImports()`), and stores each `sub_<hex>` export in the
  `azBoundarySymbols` map. When any cb/layout wasm instantiates, its env-import
  Proxy resolves `sub_<hex>` **first from `azBoundarySymbols`, else a
  shape-appropriate no-op stub** (`loader_js.rs:290-309`). This map is the
  substitution point: a JS function stored under the same key satisfies the
  import identically.

### 2.3 What a JS implementation of a boundary sees

A boundary body is called with the remill ABI: a pointer to the guest `State`
struct in linear memory, the guest PC, and an opaque memory token. Arguments
live in the State at fixed byte offsets per the pcs module
(`transpiler_remill.rs:86-172`): on x86-64/Win64 `ARG = [2248 (RCX), 2264 (RDX),
2344 (R8), 2360 (R9)]`, return in `RET = 2216 (RAX)`, `SRET = 2248` (aggregate
returns >8B pass a hidden destination pointer in RCX, shifting real args right),
stack args at `[SP]+8+32+8i` (`stack_arg_disp`, `:149-151`), `SP = 2312`. The
`Pcs` enum (`:176-215`) and `signature_for_callback_kind` (`:457+`) document how
aggregates (AzRefAny, AzString) are placed. So a JS boundary impl *can* read
`AzString {ptr,len,…}` args out of guest memory with a DataView and write
results back — loader.js already does exactly this class of guest-memory
read/write in `azRemillIntrinsics()` (`loader_js.rs:133-167`) and `azHydrate()`
(which allocates guest memory via the exported `AzStartup_alloc` and writes the
model's bytes into it, `loader_js.rs:692-710`). §5.2 argues for keeping that
JS-side struct-poking to a minimum via a flat request-buffer protocol.

### 2.4 Re-entry: how JS gets back *into* lifted code

- Events: `azDispatch` allocates a 256-byte event buffer via `AzStartup_alloc`,
  writes flat fields, calls the exported
  `AzStartup_dispatchEvent(state, kind, evt_ptr, len, out_len_ptr)`
  synchronously, then applies the returned TLV patch stream to the DOM
  (`loader_js.rs:802-852`, decoder `:881-972`). The wasm side hit-tests,
  resolves the node's callback, `call_indirect`s it, re-runs layout, and
  returns patches (`eventloop.rs:15-33`, `:2166+`).
- Guest fn-pointers held in data (e.g. `FileInputOnPathChange.callback.cb`
  inside a RefAny) are invocable inside lifted code because the M12.7
  indirect-call dispatcher compiles a `switch i64 %pc` over all lifted synth PCs
  (`transpiler_remill.rs:2462-2521`, `:2539-2544`) — i.e. a *stored user
  callback can be called later by lifted Rust code*, which is what an
  `AzStartup_completeRequest` delivery entry will do.
- JS-implemented extern hooks already exist: `__az_resolve_callback`
  (`FnClass::ResolveCallback`, `symbol_table.rs:154-156`) is an extern symbol
  whose "body" is a JS import bridge implemented in `azMakeMiniImports`
  (`loader_js.rs:198-203`). New `__az_web_*` hooks follow the same route.
- The web eventloop currently has **no timer pump, no thread support, and no
  async-result delivery entry** — the full `AzStartup_*` export list
  (`eventloop.rs:94-2166`) contains layout/hydrate/dispatch/probe entries only.
  `CallbackInfo::add_timer/add_thread` data flows into `EventloopState` on
  desktop via `azul_layout::window` (`tick_timers` `layout/src/window.rs:2289`,
  thread-writeback pump `:4230-4330`), pumped by the OS event loop; the web
  runtime never pumps them.

### 2.5 Two web targets — keep them straight

- **web-lift** (this doc): the *native* binary is lifted; `AZ_BACKEND=web://…`
  turns `App::run` into an HTTP server (`dll/src/lib.rs:178`,
  `dll/src/web/config.rs`). Cargo features of the native build are what get
  lifted (ureq/tfd/dirs are all *in* the binary: `layout/Cargo.toml:84,94,97`).
- **native wasm32 compile** (`dll/src/unified/*`): repr-C-identical stubs so
  azul-dll also *compiles* for `wasm32` (`unified/mod.rs:1-13`); e.g.
  `WebTransport` is a no-op stub there with "web-transport-wasm engine is a
  follow-up" (`unified/webtransport.rs:1-8`). These stubs never run under
  web-lift, but their existence documents intent and keeps struct layouts
  target-stable. The designs below should eventually serve both targets (the
  `__az_web_*` hook seam works for a native wasm32 build too, as plain extern
  imports).

---

## 3. API inventory

Sources: api.json 0.2.0 (root, modules → classes as listed); desktop impls as
cited. C symbol = `Az{Class}_{lowerCamel}` per `classify.rs:113-114`.

**Verdict codes** — what the browser equivalent is:
`PURE` no OS touch (lift as-is) · `SYNC` a sync browser API exists ·
`ASYNC` browser is async ⇒ shape must change · `FIRE` fire-and-forget is
semantically acceptable · `PUSH` browser pushes data; existing poll API fits ·
`NONE` no browser equivalent.

**Disposition codes** — what we do:
`LIFT` lift as-is, no web work · `JS-SYNC` boundary implemented synchronously in
JS · `JS-FIRE` boundary implemented in JS, starts async work, returns
immediately with optimistic/"scheduled" result · `CB-API` needs a new
callback-taking API variant; completion re-enters via `AzStartup_completeRequest`
· `POLL` JS backend fills guest-visible state; existing `get_*`/`recv` poll API
unchanged · `EVENT` handled in the event-dispatch layer (loader.js event
encoding), not a boundary · `STUB` honest failure on web (None/Err/false),
documented · `DEFER` needs later design.

### 3.1 Dialogs (api.json `dialog` module; desktop impl `layout/src/desktop/dialogs.rs` via `tfd`, mobile already no-ops)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 1 | dialog.MsgBox.new (`AzMsgBox_new`) | zero-init handle | n/a | PURE | LIFT | namespace struct only |
| 2 | dialog.MsgBox.ok (`AzMsgBox_ok`) | `tfd` message box | `window.alert()` | SYNC | JS-SYNC | one of the only truly sync browser dialogs; icon ignored |
| 3 | dialog.MsgBox.info (`AzMsgBox_info`) | `tfd` | `window.alert()` | SYNC | JS-SYNC | |
| 4 | dialog.MsgBox.ok_cancel (`AzMsgBox_okCancel`) | `tfd` | `window.confirm()` | SYNC | JS-SYNC | returns Ok/Cancel from confirm's bool |
| 5 | dialog.MsgBox.yes_no (`AzMsgBox_yesNo`) | `tfd` | `window.confirm()` (buttons read OK/Cancel) | SYNC | JS-SYNC | acceptable approximation; styled DOM modal = CB-API later |
| 6 | dialog.FileDialog.new (`AzFileDialog_new`) | zero-init handle | n/a | PURE | LIFT | |
| 7 | dialog.FileDialog.open_file (`AzFileDialog_openFile`) | `tfd::FileDialog::open_file` (blocking) | `<input type=file>` universally; `showOpenFilePicker` Chromium-only | ASYNC | CB-API + STUB | sync form returns None on web with console warn; new `open_file_with` (§5.1) |
| 8 | dialog.FileDialog.open_multiple_files (`AzFileDialog_openMultipleFiles`) | `tfd` | `<input type=file multiple>` | ASYNC | CB-API + STUB | |
| 9 | dialog.FileDialog.open_directory (`AzFileDialog_openDirectory`) | `tfd` | `<input webkitdirectory>`; `showDirectoryPicker` Chromium-only | ASYNC | CB-API + STUB | |
| 10 | dialog.FileDialog.save_file (`AzFileDialog_saveFile`) | `tfd` | `showSaveFilePicker` Chromium-only; portable = Blob download (no real path exists) | ASYNC | CB-API + STUB | web result is a *save target handle*, not a path — see §5.1 |
| 11 | dialog.ColorPickerDialog.new (`AzColorPickerDialog_new`) | zero-init | n/a | PURE | LIFT | |
| 12 | dialog.ColorPickerDialog.open (`AzColorPickerDialog_open`) | `tfd` color chooser | `<input type=color>` + change event | ASYNC | CB-API + STUB | |
| 13 | dialog.DialogAriaInfo.* (5 fns) | pure struct builders | n/a | PURE | LIFT | a11y metadata only |

### 3.2 File I/O (api.json `file` module + `svg.FilePath` (misfiled in svg); desktop `layout/src/desktop/file.rs` (std::fs) and `layout/src/file.rs` (std::fs + `dirs` crate, Cargo `layout/Cargo.toml:94`))

Web model decision (§5.3): paths on web resolve inside a **virtual filesystem**
= OPFS (persistent, origin-scoped) + a **picked-file registry** (`/az/picked/<id>`
virtual paths for browser File handles, which have no real paths). "Write a file
the *user* receives" is not path-based on the web at all — it's a download (row
27) or a save-picker handle (row 10).

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 14 | file.File.open / create (`AzFile_open/_create`) | `std::fs::File` open/create (`desktop/file.rs`) | OPFS `getFileHandle` (async) / picked-file registry lookup (sync) | ASYNC | CB-API + DEFER | main-thread OPFS handle ops are async; sync only in a worker (§4.3) |
| 15 | file.File.read_to_string / read_to_bytes (`AzFile_readToString/…`) | std::fs read | `File.arrayBuffer()` (async); sync for already-materialized picked files if pre-read | ASYNC | CB-API | pre-reading picked files at pick time (Phase 1) makes reads of picked files JS-SYNC |
| 16 | file.File.write_string / write_bytes (`AzFile_writeString/…`) | std::fs write+sync | OPFS write (async) | FIRE | JS-FIRE | optimistic bool; error surfaced via later poll/console |
| 17 | file.File.close (`AzFile_close`) | drop handle | registry release | SYNC | JS-SYNC | |
| 18 | svg.FilePath.create/empty/from_str/from (`AzFilePath_*`) | string wrapper | n/a | PURE | LIFT | 4 fns |
| 19 | svg.FilePath.join/join_str/parent/file_name/extension/is_absolute/as_string | pure path algebra | n/a | PURE | LIFT | 7 fns |
| 20 | svg.FilePath.get_temp_dir / get_current_dir | std::env | virtual: `/tmp`, `/` in OPFS namespace | SYNC | JS-SYNC | constants; enables AzWriter's `temp_dir()` habit *if* it used the Az API |
| 21 | svg.FilePath.get_home_dir … get_template_dir (18 known-folder fns) | `dirs` crate (`layout/src/file.rs:706+`) | no browser equivalent; map to virtual OPFS subdirs (`/home`, `/downloads`, …) or None | SYNC | JS-SYNC | returning Some(virtual) keeps app logic alive; document that these are origin-private |
| 22 | svg.FilePath.exists/is_file/is_dir (`AzFilePath_exists/…`) | std::fs metadata | OPFS metadata is async on main thread | ASYNC | JS-SYNC via mirror + DEFER | keep a JS-side synchronous *directory index mirror* of OPFS (updated on every mutation) so these stay sync; eventual-consistency caveat |
| 23 | svg.FilePath.metadata / read_dir (`AzFilePath_metadata/_readDir`) | std::fs | OPFS iteration (async) | ASYNC | JS-SYNC via mirror / CB-API | mirror serves size/type; full fidelity needs CB-API |
| 24 | svg.FilePath.create_dir(_all)/remove_file/remove_dir(_all)/rename_to/copy_to | std::fs | OPFS mutations (async) | FIRE | JS-FIRE | mutate mirror synchronously, OPFS asynchronously; Result returns Ok optimistically |
| 25 | svg.FilePath.read_bytes / read_string (`AzFilePath_readBytes/_readString`) | std::fs read | OPFS read (async); picked files pre-read (sync) | ASYNC | CB-API (+ JS-SYNC for picked/preloaded) | `read_bytes_with` variant §5.1; a preload manifest can make chosen assets sync |
| 26 | svg.FilePath.write_bytes / write_string (`AzFilePath_writeBytes/_writeString`) | std::fs write | OPFS write (async) | FIRE | JS-FIRE | THE AzWriter fix, paired with row 27 |
| 27 | *(new)* FileDialog.save_bytes — "give these bytes to the user" | save dialog + write | **Blob + `<a download>` click** (universal) or save-picker handle write | FIRE | JS-FIRE (new API, §5.1) | the PDF-export path: printpdf output → browser download; loader synthesizes the anchor click in page context (no user-gesture requirement for downloads triggered from an input-event callback chain) |
| 28 | svg.FilePath.canonicalize (`AzFilePath_canonicalize`) | std::fs | pure given virtual FS | SYNC | JS-SYNC | |
| 29 | file.FileMetadata / DirEntry / FileType / DirEntryVecSlice; error.FileError(-Kind); option.OptionFile(-Path); error.ResultFilePathFileError + 6 sibling Result types | data types | n/a | PURE | LIFT | shared marshalling vocabulary — keep C-layout stable, they're the CB-API payload types |

### 3.3 HTTP (api.json `http` module; desktop `layout/src/http.rs` = blocking `ureq` 3.3/rustls, `layout/Cargo.toml:84`, feature `http` `:260`)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 30 | http.HttpRequestConfig.create/with_timeout/with_max_size/with_user_agent/with_header | pure builders | n/a | PURE | LIFT | 5 fns; config marshals into the request buffer |
| 31 | http.HttpRequestConfig.http_get(+`_default`) (`AzHttpRequestConfig_httpGet…`) | `ureq` blocking GET (`http.rs:242`) | `fetch()` | ASYNC | CB-API + STUB | sync form → `HttpError::Other("sync http unavailable on web")` (mirrors the existing non-`http`-feature stub `http.rs:248`); new `http_get_with` §5.1; CORS applies |
| 32 | http.HttpRequestConfig.download_bytes(+`_default`) | `ureq` | `fetch()` → bytes | ASYNC | CB-API + STUB | |
| 33 | http.HttpRequestConfig.is_url_reachable | `ureq` HEAD | `fetch(HEAD)` | ASYNC | STUB (Phase 2), CB-API variant optional | sync bool is unimplementable without JSPI |
| 34 | http.HttpResponse.is_success/is_redirect/is_client_error/is_server_error/body_as_string | pure accessors | n/a | PURE | LIFT | 5 fns; response struct is built by the web impl from fetch results |
| 35 | http.HttpHeader, vec.HttpHeaderVec(+Destructor), error.HttpError/HttpStatusError/HttpResponseTooLargeError, ResultHttpResponseHttpError, ResultU8VecHttpError | data types | n/a | PURE | LIFT | |
| 36 | url.Url.parse/from_parts/to_string/is_https/is_http/effective_port/join | pure parsing (azul_core::url) | n/a | PURE | LIFT | 7 fns |

### 3.4 Timers, threads, time (api.json `task`/`time`; desktop pump `layout/src/window.rs:2289` (tick_timers) and `:4230-4330` (thread writebacks); `layout/src/thread.rs` = std::thread + mpsc)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 37 | task.Timer.create/with_delay/with_interval/with_timeout/tick_millis/instant_of_next_run/is_about_to_finish/invoke | pure struct ops | n/a | PURE | LIFT | 8 fns |
| 38 | callbacks.CallbackInfo.add_timer/remove_timer/get_timer/get_timer_ids | queued into window state; OS loop pumps | `setTimeout`-scheduled JS pump calling new `AzStartup_tickTimers` | SYNC | EVENT (runtime gap, Phase 0) | web eventloop has **no timer pump today** (§2.4); the API itself needs no change |
| 39 | time.Instant.now (`AzInstant_now`; also `GetSystemTimeCallback`) | std::time via fn-ptr in GetSystemTimeCallback (force-seeded into lifts, `symbol_table.rs:990-995`) | `performance.now()` | SYNC | JS-SYNC | currently returns garbage/0 through the lifted std path; make it a JS boundary; unblocks timers, animations, `get_current_time` |
| 40 | time.Instant/Duration/SystemTick*/SystemTimeDiff arithmetic (11 fns) | pure | n/a | PURE | LIFT | |
| 41 | task.Thread.create (`AzThread_create`) | `std::thread::spawn` + channels (`layout/src/thread.rs`) | no threads in the lifted runtime | NONE | STUB (Phase 1-3), DEFER (JS-task emulation) | honest failure: thread never reports alive; `PlatformCapability`-style probe must say so. Long-term: "thread" = JS async task servicing the same ThreadSender/ThreadReceiver queues (§5.4) |
| 42 | task.Thread.sleep_ms/us/ns | std::thread::sleep | cannot block the main thread | NONE | STUB (no-op) | JSPI or worker mode could honor it later |
| 43 | task.ThreadSender.send / ThreadReceiver.recv / ThreadWriteBackMsg.create / ThreadReceiveMsg/ThreadSendMsg types | mpsc channels | JS-queue-backed when the "thread" is a JS-owned resource | PUSH | POLL (with §5.4) | the recv-poll shape is web-perfect; only the *owner* changes |
| 44 | callbacks.CallbackInfo.add_thread/remove_thread/get_thread/get_thread_ids | window state + pump | follows row 41 | — | STUB → DEFER | |

### 3.5 Clipboard, drag-drop, screenshots (CallbackInfo + `dom.ClipboardContent`; desktop per-platform `dll/src/desktop/shell2/{windows,macos,linux/x11,linux/wayland}/clipboard.rs`, `native_screenshot.rs`)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 45 | callbacks.CallbackInfo.get_clipboard_content (`AzCallbackInfo_getClipboardContent`) | shell2 clipboard readers | `ClipboardEvent.clipboardData` — **sync inside a paste event**, which is exactly when the API contract says it's available | SYNC | EVENT | loader wires `paste` listener; content travels in the event buffer (like `azDispatchWithText`, `loader_js.rs:1058-1082`) |
| 46 | callbacks.CallbackInfo.set_clipboard_content/set_copy_content/set_cut_content | shell2 clipboard writers | in copy/cut event: `clipboardData.setData` (sync); outside: `navigator.clipboard.writeText` (async, FIRE) | SYNC/FIRE | EVENT + JS-FIRE | |
| 47 | callbacks.CallbackInfo.get_hovered_file/get_dropped_file/get_dragged_file/is_file_drag_active | window-system DnD state | HTML5 dragover/drop events; `File` objects have **no OS path** → registry virtual paths (`/az/picked/<id>`) | SYNC (in-event) | EVENT | payload pre-read or handle-registered at drop time |
| 48 | callbacks.CallbackInfo.accept_drop/set_drag_data/get_drag_types/set_drop_effect/get_drag_state/get_drag_delta* | drag_drop manager | DataTransfer API, sync inside drag events (API is already W3C-modeled per its docs) | SYNC | EVENT | |
| 49 | callbacks.CallbackInfo.take_screenshot(dom_id)/take_screenshot_base64 | CPU renderer → PNG | same CPU render, lifts (heavy) | PURE | LIFT (perf caveat) | |
| 50 | callbacks.CallbackInfo.take_screenshot_to_file | CPU render + fs write | render + Blob download | FIRE | JS-FIRE (reuse row 27 hook) | |
| 51 | callbacks.CallbackInfo.take_native_screenshot(_bytes/_base64) | OS window capture (`dll/src/desktop/native_screenshot.rs`) | no OS window exists; nearest: `getDisplayMedia` (permission, async) or fall back to DOM screenshot | NONE | STUB (alias to row 49 output) | document the semantic difference |

### 3.6 Request/poll backends already shaped for the web (CallbackInfo; desktop backends per-OS)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 52 | callbacks.CallbackInfo.request_biometric_auth + get_biometric_result (+ biometric.* types) | LAContext / Windows Hello / BiometricPrompt | WebAuthn `navigator.credentials.get` (async) | PUSH | POLL | fire-and-forget + later-frame poll **already the API contract** — implement request in JS, push result into guest state, wake via synthetic event |
| 53 | callbacks.CallbackInfo.keyring_store/keyring_get/keyring_delete + get_keyring_result | Keychain/KeyStore/libsecret/CredentialLocker | no OS keyring; IndexedDB + WebCrypto (non-extractable keys) — weaker guarantee | PUSH | POLL (document weaker security) | |
| 54 | callbacks.CallbackInfo.get_location_fix (+ dom.GeolocationProbeConfig) | FusedLocationProvider / CoreLocation | `navigator.geolocation.watchPosition` pushes fixes | PUSH | POLL | textbook fit |
| 55 | callbacks.CallbackInfo.get_sensor_reading (+ sensor.*) | CoreMotion / SensorManager | Generic Sensor API / devicemotion events | PUSH | POLL | permission-gated on iOS Safari |
| 56 | callbacks.CallbackInfo.get_gamepad_state/get_primary_gamepad (+ gamepad.*) | gilrs / GCController | `navigator.getGamepads()` — **sync snapshot** | SYNC | POLL (JS refreshes guest state each frame) | |
| 57 | callbacks.CallbackInfo.get_monitors/get_current_monitor; app.App.get_monitors (`AzApp_getMonitors`) | OS display enumeration | `window.screen` (+ async `getScreenDetails` on Chromium) | SYNC (limited) | JS-SYNC | one synthetic monitor from screen/devicePixelRatio |
| 58 | window.PlatformCapability.udp/camera/screen_capture/microphone/audio_output/sensors/gamepad/geolocation/keyring/biometric/video_codec (11 probes) | per-OS probes | web-truthful answers (`backend: "web"`, reason strings) | SYNC | JS-SYNC | the honesty mechanism for everything STUBbed; add `thread`, `fs`, `dialog` probes (§5.5) |

### 3.7 Data/DB/media/transport handles (`azul_dll::unified::*`; desktop `dll/src/desktop/extra/*`)

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 59 | db.Db.open/is_open/execute/query (+ DbRows/DbValue) | bundled-SQLite `rusqlite` (`dll/Cargo.toml:146,553`) | `:memory:` may lift as-is (no I/O in the hot path); file-backed needs SQLite-VFS→OPFS | PURE(:memory:) / NONE(file) | LIFT (:memory:, verify) + DEFER (OPFS VFS) | worker+SAB mode would enable the sync VFS properly |
| 60 | webtransport.WebTransport.connect/is_connected/stats/send_video/send_audio/send_chat/send_system/request_keyframe/recv/close (10 fns) | quinn-based engine on a bg thread (`desktop/extra/webtransport`; wasm32 stub `unified/webtransport.rs`) | browser WebTransport API (Baseline since 2026-03 per `doc/webtransport-plan.md`), JS-owned engine; `recv` pops a JS-filled guest ring buffer from a timer | PUSH | POLL (JS-owned engine) | the API was *designed* for this (recv "Poll from a timer"); sends are JS-FIRE |
| 61 | audio.AudioSink.open/is_open/play/frames_played/close | ALSA/PipeWire | Web Audio / AudioWorklet, JS-owned | FIRE/PUSH | JS-FIRE + POLL | `play(frame)` = enqueue; frames_played from JS counter |
| 62 | audio.AudioDeviceList.enumerate | `pactl` | `mediaDevices.enumerateDevices()` (async + permission) | ASYNC | STUB → CB-API | sync enumerate can return the cached last answer |
| 63 | screen.ScreenRecorder.start/is_recording/write_frame/frames_written/finish | gstreamer x264 subprocess | MediaRecorder, JS-owned; finish → Blob download | FIRE | JS-FIRE + POLL | path arg becomes suggested download name |
| 64 | widgets.ScreenCaptureWidget / MicrophoneWidget / camera widgets (create/dom/set_on_frame) | bg-thread capture per SUPER_PLAN pattern | `getDisplayMedia` / `getUserMedia` / camera — JS-owned, frames pushed into guest buffers, on_frame via timer/synthetic event | PUSH | POLL (JS-owned resource) | replaces the bg *thread* with a JS resource; same message shapes |
| 65 | video.VideoStartupCheck.run/remediate (+ VideoProvisionOutcome) | Linux kernel/GPU provisioning via pkexec | none | NONE | STUB | returns "nothing to remediate" |
| 66 | widgets.MapWidget.dom_with_fetch (`AzMapWidget_domWithFetch`) | bg tile-fetch worker (`dll/src/desktop/extra/map.rs`; wasm32 fallback = placeholder dom, `unified/map.rs`) | tile fetch via `fetch()`, JS-owned tile cache pushing image updates | PUSH | POLL (JS-owned) | Phase 4 with row 60's machinery |
| 67 | pdf.Pdf.new/from_dom/write_json/read_json | printpdf (wasm-ready), pure compute | lifts as-is | PURE | LIFT | **verified pure**: "no file I/O; save the returned bytes" (api.json doc); pairs with row 27 for export |
| 68 | fluent.IcuLocalizerHandle.from_system_language | OS locale query | `navigator.language` | SYNC | JS-SYNC | remaining 18 Icu/Fluent fns are pure once data blobs are loaded (loadDataBlob = bytes in) |
| 69 | json.Json.* / fmt.* / component.* / gesture.* readers | pure | n/a | PURE | LIFT | listed for completeness — no OS touch |

### 3.8 Widgets & app plumbing that embed OS calls

| # | api.json path (C symbol) | Desktop impl | Browser mapping | Verdict | Disposition | Notes |
|---|---|---|---|---|---|---|
| 70 | widgets.FileInput.create/set_default_text/with_default_text/set_on_path_change/with_on_path_change/dom/swap_with_default (+ FileInputState(-Wrapper), callbacks.FileInputOnPathChangeCallbackType, option.OptionFileInputOnPathChange) | `fileinput_on_click` **blocks on `tfd::FileDialog::open_file`** then invokes `on_path_change` (`layout/src/widgets/file_input.rs:208-240`) | port internals onto `FileDialog::open_file_with` (row 7): click → request; completion → set state + invoke on_path_change | ASYNC | CB-API (internal rewrite, API unchanged!) | **the proof case**: the widget's public API already has the right callback shape; only the widget's *implementation* must stop blocking. Desktop behavior unchanged (§5.1 delivery semantics) |
| 71 | app.App.create/add_window; window.WindowCreateOptions.create; callbacks.CallbackInfo.create_window/close_window/modify_window_state | OS windows | one window = the page; extra windows ≈ none (or window.open, unreliable) | NONE | STUB (multi-window), LIFT (state structs) | web backend is single-window per page today |
| 72 | app.App.run (`AzApp_run`) | OS event loop / web server | never lifted | — | already-handled | `ApiFnClass::ServerEntryPoint → NeverLift` (`classify.rs:146`, `symbol_table.rs:2547`) |
| 73 | menu.Menu.* / CallbackInfo.open_menu* | native menus (Win32/NSMenu) via shell | azul-rendered DOM overlay menus | — | DEFER (renderer-level, not a boundary) | not an OS *service* on web; needs DOM-patch menu rendering |
| 74 | *(rule, not an API)* raw `std::fs`/`std::net`/`std::env`/`std::thread` calls in **user code** — e.g. AzWriter's `std::fs::write` + `std::env::temp_dir` (`examples/azul-writer/src/main.rs:24,117`) | libstd | **cannot be intercepted semantically** — they lift into out-of-image syscall leaves → silent no-op stubs (M10-A1) | — | document + lint | portability rule: OS services must go through the Az API surface. Consider a `azul::fs` prelude alias + an example-lint. AzWriter must switch to row 27's API |
| 75 | *(reserved)* iroh (p2p) — not in workspace yet | (planned crate) | iroh upstream has wasm support; browser transport = relay over WebSocket/WebTransport (no raw UDP/QUIC in page JS); would slot in as a JS-owned engine like row 60, or lift iroh's wasm-native path | PUSH | POLL (future, §6 Phase 5) | design principle: expose iroh through the same handle+send+recv-poll shape as WebTransport so the web impl is mechanical |

**Row count: 75 rows** (rows 18-29, 34-37, 40, 45-51, 58-69 are grouped rows;
individual api.json functions covered: ~230; plus the CallbackInfo OS-facing
family of ~40 functions inventoried across rows 38, 44-58).

---

## 4. The async design question

### 4.1 Option A — callback/continuation APIs (portable redesign)

Shape: every OS request that is async-on-web gains a variant taking
`(data: RefAny, callback: <Result>Callback)`; the sync original stays for
desktop-only code and STUBs honestly on web.

- Desktop implementation: perform the blocking call, then invoke the callback
  **via the deferred-apply queue that `CallbackInfo` already uses** ("applied
  after callback returns" — same contract as `add_timer`/`create_window`), so
  ordering semantics are identical on both targets (callback never runs
  re-entrantly inside the requesting activation).
- Web implementation: the boundary's JS impl starts the browser async op and
  returns immediately. On resolution, JS re-enters wasm through a new
  `AzStartup_completeRequest(state, request_id, status, payload_ptr, payload_len)`
  export, which looks up the pending `{callback fn-ptr, RefAny}` in
  `EventloopState`, performs the guest indirect call (M12.7 dispatcher), runs
  the normal Update→relayout→patch cycle, and returns patches for
  `azApplyPatches` — the exact `azDispatch` shape (`loader_js.rs:802-852`).
- Pros: portable by construction; matches azul's existing idioms (§1.1); no
  browser-support risk; the FileInput widget proves the pattern needs *zero
  public API change* for widget users (row 70); testable on desktop.
- Cons: new API variants to design/codegen (api.json additions §5.1); user code
  using the old sync forms must migrate to be web-portable; continuation style
  is more ceremony than straight-line code.

### 4.2 Option B — JSPI (`WebAssembly.Suspending` / `WebAssembly.promising`)

Mechanically a very good fit: our boundary imports are *wasm imports*, which is
precisely what `WebAssembly.Suspending` wraps; the JS impl could be async and
the lifted caller would suspend until the promise resolves, keeping the sync
desktop signature identical. Entry points (`AzStartup_dispatchEvent`) would be
wrapped with `WebAssembly.promising`, making `azDispatch` async.

- Support status (checked 2026-08): shipped by default in **Chrome 137+**;
  **Firefox 139 behind a flag**; **Safari in development** (objection withdrawn
  late 2025; JSPI is an Interop 2026 focus area — i.e. cross-browser consistency
  is a *goal for this year*, not a fact). Spec standardized by the Wasm CG
  April 2025. Not Baseline.
- Architectural risks specific to azul:
  1. **Re-entrancy.** JSPI suspends the whole activation and returns to the JS
     event loop. A second DOM event then calls `AzStartup_dispatchEvent` again
     on the same instance → two activations mutate the single heap
     `EventloopState` (and the shared bump allocator) concurrently-interleaved.
     Lifted Rust code assumes `&mut` uniqueness; this is silent-corruption
     territory. Mitigation exists (loader-side gate: queue events while a
     suspension is outstanding) but is global-stall-shaped — a slow fetch would
     freeze all input.
  2. Suspension propagates through multi-instance stacks (cb wasm → boundary
     shard → env import); this is within JSPI's model but is exactly the sort
     of engine edge (plus the lifted-code shadow-stack conventions) that needs
     dedicated verification.
  3. The patch-application contract (`azDispatch` reads `out_len` immediately
     after the call) becomes promise-shaped — a loader rework, though a modest
     one.
- Verdict: **excellent later compatibility layer** (make legacy sync
  `FileDialog::open_file` really work on Chromium), wrong foundation for the
  portable API in 2026.

### 4.3 Option C — worker + SharedArrayBuffer + `Atomics.wait` sync bridge

Run all lifted wasm in a Worker over a SAB memory; boundary JS impls post a
request to the main thread and `Atomics.wait` for the reply; main thread does
the async browser work and `Atomics.notify`s.

- Requirements: cross-origin isolation (COOP/COEP). Feasible for us — the azul
  server owns all responses (`server.rs`) and can set both headers; but it
  constrains embedding (no cross-origin iframes/resources without CORP).
- Unique powers: true sync file I/O via OPFS `createSyncAccessHandle`
  (worker-only API); `Thread::sleep` works; a real SQLite-on-OPFS VFS becomes
  possible (row 59); lifted code never needs shape changes.
- Costs: the entire loader/runtime moves off-main-thread — event marshalling
  and TLV patch application become postMessage-proxied (today they are
  synchronous same-thread: dispatch `loader_js.rs:802-852`, patch apply
  `:881-972`, listener wiring `:974-1050`); DOM reads (hit-test
  fallbacks, `azNodeIdxFromTarget`) need rethinking; input latency +1 hop;
  debugging degrades. It also still blocks the UI *worker* (not the page) per
  request — acceptable — but any main-thread jank delays every boundary reply.
- Verdict: **the correct escape hatch** for the few genuinely-sync-required
  cases (sync VFS, legacy C plugins), and a plausible end-state architecture,
  but it is a runtime re-architecture, not an API design — and it does nothing
  for API honesty (permission prompts still async, pickers still user-gesture
  gated). Keep the design worker-compatible; do not build it now.

### 4.4 Recommendation (restated, concrete)

**Hybrid, callback-first:**

1. **Portable surface = Option A** callback variants + the existing
   fire-and-forget/poll idioms (§3 dispositions JS-FIRE / POLL / EVENT cover a
   large majority of the surface without any new API at all — only dialogs,
   file reads, and HTTP need CB-API variants).
2. **Web mechanics** = JS-implemented boundary imports (§5.2) + one new
   generic completion export + a timer pump (Phase 0).
3. **JSPI = Phase 5 progressive enhancement** for legacy sync calls on
   Chromium, behind a loader feature-detect, once the re-entrancy gate is
   designed.
4. **Worker+SAB reserved**; keep all new JS boundary code DOM-independent where
   possible so it can migrate into a worker later.

Delivery guarantee to document for CB-APIs: *the result callback runs on the
main UI activation, never re-entrantly inside the requesting callback; it may
run within the same frame on desktop and always runs on a later task on web.*

---

## 5. Concrete API & mechanism changes

### 5.1 New api.json entries (module `dialog`, `http`, `svg`/file)

New callback typedef classes (module `callbacks`, mirroring
`FileInputOnPathChangeCallbackType`'s `{fn_args, returns}` shape):

```text
FileOpenCallbackType:      fn(RefAny, CallbackInfo, OptionFilePath)            -> Update
FileOpenMultiCallbackType: fn(RefAny, CallbackInfo, FilePathVec)               -> Update
SaveTargetCallbackType:    fn(RefAny, CallbackInfo, OptionSaveTarget)          -> Update
ColorPickCallbackType:     fn(RefAny, CallbackInfo, OptionColorU)              -> Update
FileReadCallbackType:      fn(RefAny, CallbackInfo, ResultU8VecFileError)      -> Update
HttpResponseCallbackType:  fn(RefAny, CallbackInfo, ResultHttpResponseHttpError) -> Update
HttpBytesCallbackType:     fn(RefAny, CallbackInfo, ResultU8VecHttpError)      -> Update
```

New functions (each also gets the wrapper `…Callback` struct class per azul
convention):

```text
dialog.FileDialog.open_file_with(title: String, default_path: OptionString,
    filter_list: OptionFileTypeList, data: RefAny, on_result: FileOpenCallback) -> DialogRequestId
dialog.FileDialog.open_multiple_files_with(...same..., on_result: FileOpenMultiCallback) -> DialogRequestId
dialog.FileDialog.open_directory_with(title, default_path, data, on_result: FileOpenCallback) -> DialogRequestId
dialog.FileDialog.save_file_with(title: String, suggested_name: String,
    data: RefAny, on_result: SaveTargetCallback) -> DialogRequestId
dialog.FileDialog.save_bytes(suggested_name: String, mime: String, bytes: U8Vec) -> bool
    // "hand these bytes to the user": desktop = save dialog + write;
    // web = Blob + <a download> click. FIRE semantics; bool = scheduled.
dialog.ColorPickerDialog.open_with(title: String, default: OptionColorU,
    data: RefAny, on_result: ColorPickCallback) -> DialogRequestId
svg.FilePath.read_bytes_with(self: ref, data: RefAny, on_result: FileReadCallback) -> DialogRequestId
http.HttpRequestConfig.http_get_with(self: ref, url: String,
    data: RefAny, on_result: HttpResponseCallback) -> HttpRequestId
http.HttpRequestConfig.download_bytes_with(self: ref, url: String,
    data: RefAny, on_result: HttpBytesCallback) -> HttpRequestId
```

New data classes: `SaveTarget` (opaque handle: desktop = path; web = FS-Access
handle id or "download" sentinel) with `write_bytes(self, U8Vec) -> bool`
(FIRE); `FilePathVec`; `DialogRequestId`/`HttpRequestId` (u64 newtypes, usable
for cancellation later). Note `ResultU8VecFileError`, `OptionColorU`,
`OptionFileTypeList` etc. already exist (§3 row 29/35) — the payload vocabulary
is complete.

Desktop `fn_body`s call the existing sync impls then enqueue the callback
through the same deferred-apply queue as `add_timer` (documented delivery
semantics §4.4). The old sync functions remain, marked in docs: *"Blocking;
desktop only. On the web backend returns None/Err immediately — use
`…_with`."*

**FileInput widget**: rewrite `fileinput_on_click`
(`layout/src/widgets/file_input.rs:208`) onto `open_file_with` — state update +
`on_path_change` invocation move into the internal `FileOpenCallback`. Public
widget API unchanged.

**api.json web metadata**: add an optional per-function key read by
`classify.rs` (which already parses full api.json at startup):

```json
"open_file": { ..., "web": { "class": "js" } }         // JS-implemented boundary
"run":       { ..., "web": { "class": "server" } }     // existing ServerEntryPoint
"from_dom":  { ..., "web": { "class": "lift" } }       // default, may be omitted
"open_file_with": { ..., "web": { "class": "js", "hook": "az_file_open" } }
```

`classify_api_functions` maps `"js"` to a new `ApiFnClass::WebJsImpl` →
`classify_for_name` returns a new `FnClass::BoundaryJsImport` that behaves like
`BoundaryImport` (declare-only, `used_boundaries` recording) **except** the
orchestrator skips `lift_boundary_to_wasm` for it and instead emits a manifest
entry `{name, body_export, impl: "js", hook, sig}` — loader.js then registers
its generated JS implementation in `azBoundarySymbols` under the same
`sub_<synth_hex>` key. No change needed in the caller-side lift or the import
Proxy (`loader_js.rs:302-304` already prefers `azBoundarySymbols`).

### 5.2 The JS boundary implementation protocol (keep JS dumb)

Two layers, to avoid hand-decoding Win-x64 `State` structs in JS for every API:

1. **Thin per-boundary JS trampoline** (generated into loader.js from api.json
   signatures + the `pcs` tables §2.3): reads scalar/pointer args out of
   `State` (`DataView` at `ARG[i]`/stack disps), turns `AzString`/`U8Vec` args
   into JS strings/arrays (ptr+len reads), calls the hook, writes the return
   scalar to `RET` (2216) or fills the `SRET` destination struct. For Phase 1's
   surface (strings, bytes, options-of-string, bool) this is ~6 marshallers.
2. **For CB-APIs**, keep the pending-request bookkeeping in *lifted Rust*, not
   JS: the web substitute body (see below) stores
   `{request_id, callback fn-ptr, cloned RefAny}` into an `EventloopState`
   pending-table and calls a tiny typed extern hook
   (`__az_web_file_open_begin(request_id, params_ptr, params_len)`) that JS
   implements for real by name in the import object — the same mechanism as
   `__az_resolve_callback`/`fmaxf`/`memset` today (`loader_js.rs:197-228`).
   Params travel as one flat byte buffer (postcard-style TLV), mirroring the
   event-buffer protocol JS already speaks (`loader_js.rs:810-825`).

Where does that "web substitute body" come from, given the same native binary
serves desktop and web? Same trick as `AzStartup_*`: compile *web-specific Rust
functions* into azul-dll (`dll/src/web/boundary_impls.rs`,
`az_web_impl_FileDialog_open_file_with(...)` etc., cfg'd into the binary
whenever the web feature is on), and have the boundary pass lift the substitute
symbol's body as the shard for the corresponding Az name (a
`substitute_addr` on the `BoundaryShard` request). The substitute is ordinary
lifted code — full access to repr-C structs, RefAny cloning, the pending-table
— and only its leaf hooks are JS. This keeps marshalling correctness in Rust
where the type layouts live.

Completion path (new exports in `eventloop.rs`):

```text
AzStartup_completeRequest(state, request_id, status: u32,
                          payload_ptr: u32, payload_len: u32) -> patches_ptr (u32)
    // looks up pending entry; decodes payload per request kind into the
    // repr-C result (OptionFilePath / ResultHttpResponseHttpError / ...);
    // invokes the stored guest callback (indirect dispatcher);
    // Update handling + relayout + TLV patches — shared with dispatchEvent.
AzStartup_tickTimers(state, now_millis_f64_bits...) -> patches_ptr
    // runs due timers (reuses azul_layout tick_timers logic on web state);
    // loader schedules setTimeout(next_due) after each call.
```

JS side: `azCompleteRequest(requestId, status, bytes)` = alloc via
`AzStartup_alloc`, copy payload, call export, apply patches — a clone of
`azDispatch` (`loader_js.rs:802-852`).

### 5.3 Virtual filesystem semantics (web)

- Root namespace = OPFS. Known-folder getters return fixed virtual paths
  (`/home`, `/tmp`, `/downloads`, …), all origin-private.
- Picked files (dialogs, drag-drop): JS registry `Map<id, {File|handle, bytes?}>`;
  guest sees `/az/picked/<id>/<name>` paths. Phase 1 pre-reads picked files at
  pick time (bounded size warning) so `FilePath::read_bytes` on a picked path
  is JS-SYNC; `read_bytes_with` is the general path.
- A JS-side synchronous **index mirror** (name → {kind,size,mtime}) backs
  `exists/is_file/is_dir/metadata/read_dir` sync answers; every mutation
  updates mirror synchronously + OPFS asynchronously (JS-FIRE), giving
  read-your-writes consistency within a session.
- "Real" save to user disk is only via `save_bytes` (download) or
  `save_file_with` (FS Access handle on Chromium; downloads-fallback
  elsewhere).

### 5.4 Threads on web — the JS-owned-resource rule

`Thread::create` with a user `ThreadCallback` cannot run (the callback is a
blocking loop). Policy: (1) Phase 1-3: creation reports dead-on-arrival +
capability probe says `thread: false`; (2) framework features that use the
bg-thread pattern (webtransport / audio / capture / map tiles) are re-imple-
mented per-feature as JS-owned resources feeding the *same*
`ThreadReceiveMsg`/writeback queues, so `ThreadSender/ThreadReceiver/recv`-
polling app code keeps working unchanged; (3) revisit true user threads only
with the worker+SAB mode.

### 5.5 Capability honesty

Add `PlatformCapability::{thread, file_system, dialogs, http}` probes; the
existing 11 (`window.PlatformCapability`, row 58) get JS-SYNC web
implementations returning `{available, backend: "web/<api>", reason}` — this is
the sanctioned way for portable apps to branch, replacing `#[cfg]`.

---

## 6. Phased implementation plan

**Phase 0 — runtime enablers** (prereq: M10-D sharded mode green — currently
gated behind `AZ_ENABLE_SHARDS`, `symbol_table.rs:76-87`):
1. `FnClass::BoundaryJsImport` + api.json `"web"` key + manifest `impl:"js"`
   entries + loader registration into `azBoundarySymbols` (§5.1-5.2).
2. `AzInstant_now`/GetSystemTimeCallback boundary → `performance.now()` (row 39).
3. Timer pump: `AzStartup_tickTimers` + loader setTimeout scheduling (row 38).
4. `AzStartup_completeRequest` + `EventloopState` pending-table + `azCompleteRequest`.
   Acceptance: a lifted timer callback fires and patches the DOM; a synthetic
   pending request completes into a user callback.

**Phase 1 — AzWriter ships** (file-save download + file-open + message boxes):
1. `FileDialog::save_bytes` → Blob download (JS-FIRE, row 27); port AzWriter's
   `on_export` from `std::fs::write` to it (row 74; desktop behavior: save
   dialog or Downloads-dir write).
2. `FileDialog::open_file_with` (+ `open_multiple_files_with`) over
   `<input type=file>`; picked-file registry + pre-read (§5.3).
3. `FilePath::read_bytes/read_string` JS-SYNC for picked paths;
   `read_bytes_with` general form.
4. `MsgBox` ok/info/ok_cancel/yes_no → alert/confirm (JS-SYNC, rows 2-5).
5. Rewrite `fileinput_on_click` onto `open_file_with` (row 70).
6. Old sync dialog/file fns: honest STUBs (None/Err + one console.warn).
   Acceptance: AzWriter on web exports a real PDF the browser downloads; a
   FileInput widget round-trips a picked file's bytes; same source runs on
   desktop unchanged.

**Phase 2 — HTTP + clipboard + drag-drop payloads**:
1. `http_get_with`/`download_bytes_with` over fetch (CORS documented); sync
   forms → immediate `HttpError` STUB on web (row 31-33).
2. Clipboard: paste/copy/cut listeners → event-buffer content (rows 45-46);
   `set_clipboard_content` outside events → `navigator.clipboard` JS-FIRE.
3. Drag-drop files → registry virtual paths in event payloads (row 47-48).
4. `PlatformCapability` web answers + new probes (§5.5).

**Phase 3 — request/poll backends**: geolocation watch → fixes pushed into
guest state (row 54); sensors (55); gamepad per-frame refresh (56); biometric →
WebAuthn (52); keyring → IndexedDB+WebCrypto with documented weaker guarantees
(53); monitors (57); screenshots: `take_screenshot_to_file` → download (50),
native-screenshot alias (51). Wake-on-push: reuse `AzStartup_completeRequest`
with reserved request-ids or a synthetic event kind.

**Phase 4 — media + transport (JS-owned resources, §5.4)**: WebTransport JS
engine over the browser API feeding `recv` ring buffer (row 60 — fulfills the
`webtransport-plan.md` "web-transport-wasm follow-up"); AudioSink →
AudioWorklet (61); mic/screen/camera widgets → getUserMedia/getDisplayMedia
(64); ScreenRecorder → MediaRecorder + download (63); MapWidget tiles → fetch
(66); AudioDeviceList (62).

**Phase 5 — heavy/optional**: iroh integration behind the same
handle+send+recv-poll shape (row 75; browser = relay over
WebSocket/WebTransport); SQLite `:memory:` lift verification, later OPFS VFS
(59); JSPI compatibility layer for legacy sync APIs on Chromium (feature-
detected, event-gated §4.2); evaluate worker+SAB mode (§4.3) if sync VFS /
user threads become must-haves.

Non-goals for now: multi-window on web (row 71), native menus (row 73),
video provisioning (row 65).

---

## 7. Open questions

1. **Shard-mode default.** Everything here rides on M10-D; when does
   `AZ_ENABLE_SHARDS` become the default (the planned polarity flip,
   `symbol_table.rs:81-83`)? The JS-impl mechanism could also work in bundled
   mode by *pre-seeding* `azBoundarySymbols` before instantiation only if the
   bundled lift also stopped at these fns — i.e. JS-impl classification must
   force boundary behavior even in legacy mode.
2. **Callback discovery for CB-APIs.** Result callbacks are guest fn-ptrs
   invoked via the indirect dispatcher — the target must have been *lifted*.
   Node-attached callbacks are discovered by DOM walking; a callback passed
   only to `open_file_with` might never be. Options: (a) transpiler scans
   relocations/vtables for `*CallbackType`-shaped statics; (b) api.json-
   registered callbacks get force-enqueued like `Instant::now`
   (`find_recursable_by_name`, `symbol_table.rs:990-995`); (c) a server-side
   registration pass at first request. Needs a decision in Phase 0.
3. **RefAny lifetime across the async gap.** The pending-table clones the
   RefAny (keeps refcount alive). Cancellation semantics (window close, repeat
   requests, `DialogRequestId`-based cancel) and table GC need a policy.
4. **Result-struct marshalling fidelity.** `ResultHttpResponseHttpError`
   contains nested vecs/strings — the Phase 2 decoder in
   `AzStartup_completeRequest` builds these in lifted Rust from the flat
   payload; confirm bump-allocator pressure (no free) is acceptable for large
   downloads, or add a dedicated arena reset.
5. **Re-entrancy rules for `completeRequest`.** Must it be queued if a
   dispatch is currently on the stack (JS microtask timing can fire a promise
   between two synchronous dispatches)? Proposal: loader-side single-flight
   gate + FIFO, matching the JSPI gate design so the two share code later.
6. **Event-buffer vs boundary for clipboard/DnD.** Rows 45-48 choose the event
   layer (sync in-event data). That couples loader event encoding to
   CallbackInfo state population — is the current 256-byte fixed buffer
   (`loader_js.rs:60`) enough, or does the TLV extension (deferred "Stage A.6"
   per `loader_js.rs:975-983`) become a Phase 2 prerequisite for large paste
   payloads?
7. **Security/permissions UX.** Browser pickers and clipboard/geolocation/
   WebAuthn require user gestures and show permission prompts; boundary impls
   called *outside* an input-event activation will fail. Do we surface a
   "requires user gesture" error kind in `FileError`/`HttpError`, or document
   that request-APIs must be called from input callbacks?
8. **api.json `web` key vs codegen.** api.json is generated/patched by the
   azul-doc pipeline (`autofix → patch → normalize → codegen all`). The new
   optional key must survive normalization and be ignored by all other
   codegens (C/C++/Python bindings). Alternative: a sidecar
   `web-classification.json` — uglier but zero codegen risk. Decide with the
   azul-doc owner.
9. **Which sync APIs deserve JSPI rescue in Phase 5?** Candidates:
   `File::read_to_bytes` on non-picked OPFS paths, `Db` file-backed queries,
   `is_url_reachable`. Define the list before building the gate.
10. **Native wasm32 target parity.** Should `dll/src/unified/*` stubs adopt
    the same `__az_web_*` hook seam so a future emscripten/wasm-bindgen build
    shares the JS implementations? (Cheap if the hooks are plain `extern "C"`
    imports.)

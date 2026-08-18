# Web API Remodel Triage — the Single-Surface Cut

Status: design triage · Date: 2026-08-18 · Builds on `doc/web-boundary-apis-plan.md` §3/§5.1
(inventory + first signature draft) and `doc/webtransport-plan.md`. api.json verified at
`0.2.0` (repo root, 1888 classes / 2961 constructors+functions).

---

## 1. The rule (no hybrid)

Maintainer directive 2026-08-18: **there is one API surface.** Every api.json function has one
signature, identical on desktop and web (and mobile). The old plan's "sync form stays and STUBs
on web" hybrid (`web-boundary-apis-plan.md` §5.1 "the old sync functions remain") is dead.
Breaking changes are wanted, pre-1.0.

Operationalized, the rule has four clauses — these are the triage verdicts used below:

| Verdict | Meaning |
|---|---|
| **KEEP-PURE** | No OS touch. Lifts as-is. Nothing to do. |
| **KEEP-SYNC** | The browser answers synchronously (`alert`/`confirm`, `performance.now`, `navigator.language`, `window.screen`, `getGamepads`, JS-side fs index mirror). Sync signature stays. |
| **KEEP-FIRE / KEEP-POLL** | Non-blocking already: fire-and-forget writes/sends, or request+poll pairs (`request_biometric_auth`→`get_biometric_result`, `WebTransport::recv`). Signature stays; the web impl fills guest-visible state. Not "hybrid" — the same non-blocking contract runs on both targets. |
| **REMODEL** | The browser can only answer async ⇒ the blocking form is **deleted** and replaced by a resumable request/resume pair: `request(args…, data: RefAny, on_result: <T>Callback) -> RequestId` that never blocks; the runtime later re-enters through the single dispatch surface and invokes `on_result(data, CallbackInfo, result)` as a fresh activation. |
| **HONEST-ERR** | No web implementation can exist (native window capture, GPU provisioning). Signature stays; web returns a documented `Err`/`None` immediately **and** a `PlatformCapability` probe says so. Capability honesty is sanctioned; silent lying is not. |
| **DELETE** | Removed with no same-name successor (replacement named per row). |

**Desktop implements every REMODEL form** by running the existing sync impl (tfd/ureq/std::fs —
possibly blocking inside the request call, which is native modal behavior) and delivering the
resume through the **existing deferred-apply queue**: `CallbackChange` +
`CallbackInfo::push_change` (`layout/src/callbacks.rs:167`, `:850`), the exact contract
`add_timer`/`add_thread` already use (`callbacks.rs:927-938`, variants `:204`/`:210`). One new
variant (`CallbackChange::CompleteRequest { request_id, status, payload }`) processed after the
activation returns gives desktop the same ordering guarantee as web: *the result callback never
runs re-entrantly inside the requesting activation.*

**Naming policy (recommended):** since the sync forms are deleted, do **not** keep the §5.1
`…_with` suffixes — reuse the original names with the new signatures (`FileDialog::open_file`
*is* the resumable function). Same-name+new-arity breaks loudly at compile time, which is the
stated goal. Rows below show the original name; where §5.1 drafted a `_with` name it is noted.
One shared `RequestId` (u64 newtype, module `task`) replaces the drafted per-domain
`DialogRequestId`/`HttpRequestId` — one class, future `CallbackInfo::cancel_request(RequestId)`.

**Runtime prerequisites (Phase 0, no api.json churn):** `AzStartup_completeRequest` export +
`EventloopState` pending table, the timer pump (`AzStartup_tickTimers` — the web eventloop has
none today), and the `Instant::now`/`GetSystemTimeCallback` boundary → `performance.now()`.
All per `web-boundary-apis-plan.md` §5.2/§6 Phase 0, unchanged by this triage.

**Consequence worth stating:** with no sync forms left to rescue, the plan's Phase-5 **JSPI
compatibility layer loses its reason to exist**. Drop it. (Worker+SAB stays reserved as an
escape hatch for true-sync needs; keep JS boundary impls DOM-independent.)

---

## 2. EASY — single-shot request→result, obvious browser counterpart

Desktop change size: **S** = wrap existing sync impl + push `CompleteRequest` (mechanical);
**M** = restructure around a handle/registry.

| # | Function (current api.json signature) | Remodeled signature | Result callback type (payload reused) | Web API | Desktop | Snags (honest) |
|---|---|---|---|---|---|---|
| E1 | `dialog.FileDialog.open_file(title, default_path: OptionString, filter_list: OptionFileTypeList) -> OptionString` | `open_file(title, default_path, filter_list, data: RefAny, on_result: FileOpenCallback) -> RequestId` | `FileOpenCallbackType: fn(RefAny, CallbackInfo, OptionFilePath) -> Update` — `OptionFilePath` exists | `<input type=file>` universal; `showOpenFilePicker` Chromium | S — tfd call (`layout/src/desktop/dialogs.rs:297-304`) + deferred resume | web path is a virtual `/az/picked/<id>/…` registry path (plan §5.3); browsers require a user gesture — resolve `None` + console.warn if called outside an input activation |
| E2 | `open_multiple_files(…) -> OptionStringVec` | `…(…, data, on_result: FileOpenMultiCallback) -> RequestId` | `fn(…, FilePathVec) -> Update` — **new class `FilePathVec`** (vec-codegen boilerplate) | `<input type=file multiple>` | S | same as E1 |
| E3 | `dialog.ColorPickerDialog.open(title, default_value: OptionColorU) -> OptionColorU` | `open(title, default_value, data, on_result: ColorPickCallback) -> RequestId` | `fn(…, OptionColorU) -> Update` — exists | `<input type=color>` + change event | S — tfd ColorChooser (`dialogs.rs:235`) | `<input type=color>` has no cancel event in all browsers — resolve on blur-without-change as `None` |
| E4 | *(new)* `dialog.FileDialog.save_bytes(suggested_name: String, mime: String, bytes: U8Vec) -> bool` | fire-and-forget, no callback — bool = "scheduled" | — | Blob + `<a download>` click (universal, works from event-driven callback chains) | S — desktop = save dialog (or Downloads dir) + `std::fs::write` | **the AzWriter fix** — `examples/azul-writer/src/main.rs:34,127` currently uses raw `std::env::temp_dir` + `std::fs::write`, which lift to silent no-op syscall leaves; must port to this API |
| E5 | `svg.FilePath.read_bytes(self) -> ResultU8VecFileError` | `read_bytes(self, data, on_result: FileBytesCallback) -> RequestId` | `fn(…, ResultU8VecFileError) -> Update` — exists | picked files: pre-read `File.arrayBuffer()`; OPFS: async `getFileHandle`+read | S | picked-file pre-read at pick time makes the common read cheap; add `FileError::NotAllowed` kind for permission failures |
| E6 | `svg.FilePath.read_string(self) -> ResultStringFileError` | `read_string(self, data, on_result: FileStringCallback) -> RequestId` | `fn(…, ResultStringFileError) -> Update` — exists | as E5 + TextDecoder | S | |
| E7 | `http.HttpRequestConfig.http_get(self, url) -> ResultHttpResponseHttpError` | `http_get(self, url, data, on_result: HttpResponseCallback) -> RequestId` | `fn(…, ResultHttpResponseHttpError) -> Update` — exists | `fetch()` | S — ureq is blocking by design (`layout/src/http.rs:3`, agent `:405-422`); wrap + defer | CORS applies on web (document); add `HttpError::Cors`? — recommend reuse `HttpError::Other` in v1 |
| E8 | `download_bytes(self, url) -> ResultU8VecHttpError` | `download_bytes(self, url, data, on_result: HttpBytesCallback) -> RequestId` | `fn(…, ResultU8VecHttpError) -> Update` — exists | `fetch()` → `arrayBuffer()` | S | large-payload bump-allocator pressure (plan §7 Q4) — accept for v1 |
| E9 | `is_url_reachable(url) -> bool` | `is_url_reachable(url, data, on_result: BoolResultCallback) -> RequestId` | `fn(…, bool) -> Update` — new small callback type | `fetch(HEAD, no-cors)` | S | opaque no-cors responses make "reachable" approximate on web — document |
| E10 | `image.RawImage.decode_image_bytes_any(bytes: U8VecRef) -> ResultRawImageDecodeImageError` — **KEEP-SYNC** (pure `image`-crate compute, lifts) **+ add** resumable sibling | *(new)* `decode_image_bytes(bytes: U8Vec, data, on_result: ImageDecodeCallback) -> RequestId` | `fn(…, ResultRawImageDecodeImageError) -> Update` — exists | `createImageBitmap()` + OffscreenCanvas readback → RGBA (hardware decoders) | S — desktop calls the sync decoder + defers | the maintainer's "intercept image decoding" item. Sync form legally stays (it works everywhere) but lifted JPEG decode has no SIMD — the resumable form IS the fast path on web; steer docs/examples to it. Readback converts to RGBA8 — premultiplication/ICC nuances vs the `image` crate: document "decoded pixels may differ per backend" |
| E11 | `audio.AudioDeviceList.enumerate() -> AudioDeviceList` | `enumerate(data, on_result: AudioDeviceListCallback) -> RequestId` | `fn(…, AudioDeviceList) -> Update` — exists | `mediaDevices.enumerateDevices()` | S — desktop `pactl`-style enumeration + defer | device labels are blank on web until a getUserMedia permission is granted — document |
| E12 | `callbacks.CallbackInfo.take_screenshot_to_file(self, dom_id, path) -> ResultVoidString` — **DELETE** | compose: `take_screenshot(dom_id)` (KEEP-PURE, CPU render lifts) + `save_bytes` (E4) | — | — | S (deletion) | removing a convenience, gaining one honest path for "give the user an image" |

New classes for §2: callback-type + wrapper pairs (`FileOpenCallback(Type)`,
`FileOpenMultiCallback`, `ColorPickCallback`, `FileBytesCallback`, `FileStringCallback`,
`HttpResponseCallback`, `HttpBytesCallback`, `BoolResultCallback`, `ImageDecodeCallback`,
`AudioDeviceListCallback`), `FilePathVec` (+destructor/slice), `task.RequestId`. The result
payload vocabulary (`OptionFilePath`, `ResultU8VecFileError`, `ResultHttpResponseHttpError`, …)
**already exists** — §3 row 29/35 of the plan was right: no new Result/Option types needed
except `FilePathVec` and (M2) `OptionSaveTarget`.

---

## 3. MEDIUM — handle/subscription/registry shaped, still request/resume

| # | Function(s) | Remodel | Web mapping | Desktop | Snags |
|---|---|---|---|---|---|
| M1 | `dialog.FileDialog.open_directory(title, default_path) -> OptionString` | `open_directory(title, default_path, data, on_result: FileOpenCallback) -> RequestId` (result = virtual dir root path) | `showDirectoryPicker` Chromium; `<input webkitdirectory>` fallback (yields a file *list*, not a handle) | S | needs the picked-dir registry + fs index mirror populated from the handle so subsequent `read_dir`/`exists` answer; Firefox/Safari fallback enumerates eagerly (slow on big trees) |
| M2 | `dialog.FileDialog.save_file(title, default_path) -> OptionString` | `save_file(title, suggested_name, data, on_result: SaveTargetCallback) -> RequestId`; **new class `SaveTarget`** (opaque: desktop=path, web=FS-Access handle or "download" sentinel) with `write_bytes(self, U8Vec) -> bool` (FIRE), `as_path(self) -> OptionFilePath` | `showSaveFilePicker` Chromium; portable fallback = the E4 download (no real path exists on web) | M | the result is a *write target*, not a string path — genuine model change; on the fallback path `as_path` is `None`. Apps that only ever "export bytes" should use E4 instead — document the decision tree |
| M3 | `svg.FilePath.read_dir(self) -> ResultDirEntryVecFileError` | `read_dir(self, data, on_result: DirListCallback)`; `fn(…, ResultDirEntryVecFileError) -> Update` — exists | OPFS/`FileSystemDirectoryHandle` async iteration | S | sync mirror could fake it but would silently omit unindexed entries — remodel is the honest call. `metadata`/`exists`/`is_file`/`is_dir` STAY SYNC off the mirror (eventual-consistency caveat documented) |
| M4 | `svg.FilePath.write_bytes/write_string(self, data) -> ResultEmptyStructFileError`; `create_dir(_all)`, `remove_file`, `remove_dir(_all)`, `rename_to`, `copy_to` | **KEEP-FIRE**: signatures stay; web mutates the JS mirror synchronously (read-your-writes) + OPFS async; `Result` = validation-time errors only, durability not implied. *(Optional later: `flush(data, on_result)` for durability-critical apps.)* | OPFS mutations | none | the Result's meaning narrows on both targets — desktop should keep reporting real errors; web can only report pre-validation ones. Documented semantic, not a signature change |
| M5 | `file.File.*` (7 fns: `open`, `create`, `read_to_string`, `read_to_bytes`, `write_string`, `write_bytes`, `close`) | **DELETE the class** — fold into `FilePath` (E5/E6/M4). A stateful handle whose only powers are read-all/write-all earns nothing over `FilePath`, and OPFS handles are async-only on the main thread | — | S (deletion) | desktop impl `layout/src/desktop/file.rs` retires; grep shows no framework-internal dependency on `File` |
| M6 | Drag-drop files: `CallbackInfo.get_hovered_file/get_dropped_file/get_dragged_file -> OptionString`, `is_file_drag_active` | **KEEP-SYNC (EVENT)** — values populated from HTML5 drag events; dropped `File`s registered in the picked-file registry, returned as `/az/picked/<id>/…` virtual paths, pre-read at drop | `dragover`/`drop` + DataTransfer | none | payload travels in the event buffer — may force the TLV event-buffer extension (plan §7 Q6) for many/large files |
| M7 | Clipboard: `get_clipboard_content -> OptionClipboardContent`, `set_clipboard_content`, `set_copy_content`, `set_cut_content` | **KEEP-SYNC with narrowed contract**: getter guaranteed inside clipboard events (`ClipboardEvent.clipboardData` is sync there), `None` outside on web; setters sync in-event, `navigator.clipboard.writeText` FIRE outside. *(Optional Phase 3: `read_clipboard(data, on_result)` for out-of-event reads.)* | paste/copy/cut listeners → event buffer | none | Windows desktop impl is handle-free (`dll/src/desktop/shell2/windows/clipboard.rs:20` — no HWND needed), so no desktop snag; contract narrowing is the only cost |
| M8 | Biometric: `request_biometric_auth(prompt)` + `get_biometric_result -> OptionBiometricResult` + `get_biometric_kind` | **KEEP-POLL** — the request+poll pair is already non-blocking and portable; web impl = WebAuthn `navigator.credentials.get`, result pushed into guest state + synthetic wake event | WebAuthn | none | user-gesture + HTTPS requirements; `BiometricKind` on web = "platform authenticator" honesty |
| M9 | Keyring: `keyring_store/get/delete` + `get_keyring_result` | **KEEP-POLL**; web = IndexedDB + WebCrypto non-extractable keys | — | none | materially weaker guarantee than Keychain/Hello — must be documented, probe `PlatformCapability::keyring` reports `backend:"web-crypto"` |
| M10 | Geolocation: `get_location_fix -> OptionLocationFix` (+ `dom.GeolocationProbeConfig`) | **KEEP-POLL** — subscription manager already exists (`dll/src/desktop/extra/geolocation/mod.rs`: GeolocationManager Subscribe/Release diff events); web = `watchPosition` pushes fixes into guest state | Geolocation API | none | permission prompt on first subscribe; probe honesty |
| M11 | Sensors: `get_sensor_reading(kind) -> OptionSensorReading` | **KEEP-POLL**; web = Generic Sensor API / devicemotion refreshing guest state | — | none | iOS Safari requires `DeviceMotionEvent.requestPermission()` (a user-gesture async grant) before any data — first read stays `None` until granted |
| M12 | Gamepad: `get_gamepad_state(id)`, `get_primary_gamepad` | **KEEP-SYNC** — `navigator.getGamepads()` is a sync snapshot; loader refreshes guest state per frame | Gamepad API | none | none — textbook fit |
| M13 | Monitors/DPI: `CallbackInfo.get_monitors`, `get_current_monitor`, `app.App.get_monitors -> MonitorVec` | **KEEP-SYNC** — one synthetic monitor from `window.screen` + `devicePixelRatio` | Screen API | none | multi-monitor detail Chromium-only (`getScreenDetails`, async) — synthetic single monitor is the portable answer |
| M14 | Camera/mic/screen widgets: `widgets.CameraWidget/MicrophoneWidget/ScreenCaptureWidget.create/dom/set_on_frame/with_on_frame` | **API UNCHANGED** — replace the desktop bg-capture-thread with a JS-owned `getUserMedia`/`getDisplayMedia` resource pushing frames into guest ring buffers; `on_frame` fires via the timer pump | getUserMedia / getDisplayMedia + VideoFrame readback | none (API) / M (runtime) | permission prompts async → frames simply start later; the permission manager (`dll/src/desktop/extra/permission/mod.rs`) maps 1:1 to browser permission requests |
| M15 | `screen.ScreenRecorder.start(path,…)/write_frame/finish -> bool` | `start(suggested_name, …)` — path arg reinterpreted as download name (doc change); `finish(self, data, on_result: BoolResultCallback) -> RequestId` — MediaRecorder finalization is async (`dataavailable`); desktop gstreamer finalize also blocks today, so the remodel helps desktop too | MediaRecorder → Blob → download | S/M | `write_frame` KEEP-FIRE; `is_recording`/`frames_written` KEEP-POLL |
| M16 | `image.VideoEncoder.encode(frame, force_keyframe) -> U8Vec`, `VideoDecoder.decode(data) -> OptionVideoFrame`, `image.DecodedVideo.decode_mp4_h264(bytes) -> OptionDecodedVideo` | Remodel to **submit + poll**: `encode(frame, force_kf) -> bool` (enqueue) + `recv_packet(self) -> OptionU8Vec`; `decode(data) -> bool` + `recv_frame(self) -> OptionVideoFrame`; `decode_mp4_h264(bytes, data, on_result: VideoDecodeCallback) -> RequestId` (`OptionDecodedVideo` exists) | WebCodecs (output-callback model — inherently async) | M | **desktop is already dishonest here**: the codec backend is VideoToolbox-only — "anything else: none (encode/decode no-op)" (`dll/src/desktop/extra/video_codec/mod.rs:14,51`), i.e. sync `encode` returns empty on Windows/Linux *today*. The remodel unblocks Windows (Media Foundation is async-shaped) as well as web |
| M17 | `widgets.MapWidget.dom_with_fetch(self, cb: ThreadCallback) -> Dom` | **DELETE the `ThreadCallback` arg** → `dom_with_fetch(self) -> Dom` with a framework-owned tile fetcher (desktop: bg thread as today internally; web: `fetch()`-owned tile cache) | fetch | M | the only api.json function that takes a `ThreadCallback` from users besides `Thread::create` — removing it shrinks the HARD surface |
| M18 | `widgets.FileInput.*` (7 fns) | **API UNCHANGED** — internal rewrite only: `fileinput_on_click` currently blocks on tfd inside the click callback (`layout/src/widgets/file_input.rs:208-240`) → re-implement on E1; state update + `on_path_change` invocation move into the internal `FileOpenCallback` | — | M | the proof case: public widget API already has the right callback shape |
| M19 | Timers/threads plumbing: `add_timer/remove_timer/get_timer(_ids)`, `Timer.*`, `TimerId/ThreadId.unique`, `ThreadSender.send`, `ThreadReceiver.recv`, `ThreadWriteBackMsg.create` | **KEEP** — all non-blocking; web needs the Phase-0 timer pump (`tick_timers` desktop pump at `layout/src/window.rs:2289`, thread writebacks `:4230-4330`) | setTimeout-scheduled pump | none | `ThreadSender/ThreadReceiver` survive as the message vocabulary for JS-owned resources (§5.4 of the plan) even while `Thread::create` is capability-gated |
| M20 | `fluent.IcuLocalizerHandle.from_system_language`, locale getters | **KEEP-SYNC** — `navigator.language` | — | none | |
| M21 | Windowing: `create_window/close_window/modify_window_state`, `App.add_window`, `get_current_window_handle -> RawWindowHandle`, window state getters | **KEEP + HONEST** — single window per page; `create_window` no-ops (probe `PlatformCapability::multi_window` = false), `RawWindowHandle` returns its unsupported/none variant on web | — | none | new probe needed; menus (`open_menu*`) stay DEFER — renderer-level DOM overlay work, not an OS boundary |

---

## 4. HARD — long-lived, bidirectional, ownership-inverting

### 4.1 WebTransport (and iroh) — `doc/webtransport-plan.md` fits the style already

The 2026-06 design **pre-conforms** to the single-surface rule: `connect` returns a handle
immediately (browser `new WebTransport(url)` is also sync-handle + async `.ready`),
`is_connected`/`stats` are poll getters, `send_*` are FIRE bools, `recv` is a poll drained from
a Timer ("Poll from a timer" is written into the api.json doc). **Zero api.json churn.**

What changes vs. that doc under web-lift:
- The engine is **JS-owned** on web (browser WebTransport API; Baseline since 2026-03 per the
  plan), not the `web-transport-wasm` crate — the wasm stub (`dll/src/unified/webtransport.rs:3-4`,
  "engine is a follow-up") is superseded by a loader-side engine feeding a guest ring buffer.
- `stats()` sync vs. browser `getStats()` async: serve a JS-cached last snapshot (poll pattern —
  same trick as gamepad). `send_*`: JS queues internally; bool = enqueued.
- **Hard dependency on Phase 0**: `recv` drained from a Timer requires the timer pump.
- iroh (not in workspace yet): expose through the *same* handle+send+recv-poll shape from day 1;
  browser transport = relay over WebSocket/WebTransport (no raw UDP/QUIC in page JS). Sequence
  last; no API invented before the engine exists.

Conflict to resolve: `webtransport-plan.md` §8 item 1 says "Remove the `Udp` class" — done in
api.json, but `PlatformCapability::udp` still exists (`window.PlatformCapability`, 11 probes) and
`dll/src/desktop/extra/udp/` + `dll/src/unified/udp.rs` are still on disk. Rename the probe to
`webtransport` (or add alongside) and delete the dead module.

### 4.2 General threading — `Thread::create(init, writeback, callback: ThreadCallback)`

A user `ThreadCallback` is a blocking loop; it cannot run on the web main thread, and running
lifted wasm in a worker means duplicating the whole runtime there over SAB memory (loader
re-architecture, cross-origin isolation). Verdict: **KEEP + HONEST-ERR** for now — creation
reports dead-on-arrival, probe `PlatformCapability::thread` = false; framework features that
used bg threads are re-implemented per-feature as JS-owned resources feeding the same
`ThreadReceiveMsg`/writeback queues (M14/M15/M17, WebTransport). Revisit true user threads only
with a deliberate worker+SAB mode. `Thread::sleep_ms/us/ns` are DELETED (see §6) — they are
blocking by definition and even desktop callers should use `Timer::with_delay`.

### 4.3 Worker + SAB (reserved)

The only route to true sync OPFS reads (`createSyncAccessHandle`), user threads, and legacy
sync C plugins. Not built now; the rule is: every new JS boundary impl stays DOM-independent so
it can migrate into a worker later (plan §4.3 unchanged).

### 4.4 Absent by design (do not add)

Raw sockets, subprocess/exec, file watches, system tray: **not in api.json** — keep it that way
until a resumable design exists. Notifications: **also not in api.json** (contrary to what one
might assume) — when added, it must be born resumable (`Notification.requestPermission` is
async). Same for any future camera-roll/share-sheet API.

---

## 5. REDESIGN (maintainer direction 2026-08-18) — the `db` module

Current surface: `db.Db.open(path) / is_open / execute(sql, params) -> usize / query(sql,
params) -> DbRows` (+ `DbRows`/`DbValue`), desktop engine = **turso** (pure-Rust SQLite,
`dll/src/desktop/extra/sqlite/mod.rs:1-19` — not rusqlite; the boundary plan's §3.7 row 59 is
stale on this), sync surface via an in-crate `block_on`.

**Directive:** the web target must NOT ship an embedded SQL engine — no turso-in-wasm, no
sqlite-wasm, and (consequence) **no lifting turso for `:memory:` either** (lifted turso is
still a shipped engine — strike the plan's ":memory: may lift as-is, verify" item). The browser
already maintains a database; use it. Local layer: IndexedDB primary (localStorage only as an
internal small-KV fast path, never a separate API). **Remote backup/sync is a first-class open
parameter on BOTH targets** (`backup_sync_url: OptionString` — local-first, remote = backup
endpoint, identical semantics desktop/web).

### 5.1 The web cut happens IN THE LIFTER (early, standalone)

Two classifier changes, both pure classification — cheap, land EARLY, independent of the new
db API design:

1. **db API fns → JS-implemented boundary imports.** Classify the `AzDb_*` names
   `ApiFnClass::WebJsImpl` → `FnClass::BoundaryJsImport` (exactly the plan's §5.1 mechanism):
   callers get `env.sub_<hex>` imports, the orchestrator skips `lift_boundary_to_wasm`, and
   loader.js registers the IndexedDB-backed JS implementation under the same key. The
   transitive lift then **never descends into turso at all**.
2. **Defensive module cut:** classify every `turso::` / `turso_core::` module-path symbol
   `NeverLift` with a **loud trap** body — reaching one on web is a design error, and today it
   would instead lift megabytes of engine that no-ops at the fs syscall leaves.
   **Apply the display_list classifier lesson** (`dll/src/web/symbol_table.rs:2493-2521`,
   fixed 2026-08-17): match the module path WITH the `::` (`"turso::"`, never the bare
   substring — the old `contains("display_list")` rule caught `set_skip_display_list`, the
   setter of its own gate); exempt `alloc::`/`core::`/`std::` generics that only mention turso
   types as type parameters (`<… as core::ops::…>::… <turso_core::T>` must still lift).

### 5.2 Partial DB copying is a requirement, not an option

Local stores are size-constrained (browser quotas), so the portable model is **working-set
replication**, not full-database copies:

- **Scope**: `open`/`subscribe` take a scope — collections, key ranges, index predicates —
  plus a local size budget. The local store holds the scoped working set only.
- **Sync points are explicit**: `sync_now(scope?)` = push dirty ops, then refresh the scoped
  working set; optional auto-sync on idle/timer. **Offline = queued, never an error** — ops
  accumulate in the local oplog and push on the next successful sync.
- **Eviction**: under quota pressure (web: `navigator.storage.estimate()`; desktop: the
  configured budget) **clean** rows are evictable, **dirty** (unpushed) rows are never evicted
  before a successful push.
- **Status callbacks** report: working-set coverage, pending-push count, last-sync time,
  quota usage, and conflict events. Conflict policy is per-collection: LWW default,
  `ServerWins`/`ClientWins`, or a custom merge callback.

**The raw-SQL trade-off, stated honestly:** `execute`/`query` take SQL strings. SQL cannot run
against IndexedDB without shipping an engine — so under this directive the portable surface
**cannot be SQL**. And the "speak turso/libSQL's HTTP sync protocol via fetch" idea has a catch:
that protocol ships **WAL frames**, and applying WAL frames client-side requires… an engine.
What fetch *can* speak engine-free is (a) a **row/op-level sync protocol** (oplog push/pull,
JSON/postcard over HTTP) or (b) **remote SQL over HTTP** (Hrana-style, queries execute
server-side) — but (b) is not local-first. Recommendation:

- **Portable core = KV/collection/index-query API** mapping to IndexedDB object stores + indexes
  on web and to generated SQLite tables (turso) on desktop.
- **Sync = row-level oplog** over HTTP(S) via fetch/ureq — same protocol both targets; evaluate
  whether the server can be a thin adapter in front of a turso/libSQL instance so desktop-to-
  server sync reuses it.
- **Raw SQL is demoted**: `execute`/`query` deleted from api.json (desktop keeps turso as an
  internal engine detail). If a real demand for SQL emerges, it returns later as a
  capability-gated extension (`PlatformCapability::sql`) or via remote-SQL — explicitly not v1.

### 5.3 Proposed surface

All resumable — IndexedDB is async anyway; desktop = turso sync + deferred resume, change size
S/M. Config is builder-style per azul convention (`HttpRequestConfig` precedent):

```text
db.DbConfig.create(local_name: String) -> DbConfig                              // pure builders:
db.DbConfig.with_backup_sync_url(self, url: String) -> DbConfig                 //   first-class, both targets
db.DbConfig.with_auth_token(self, token: String) -> DbConfig
db.DbConfig.with_schema(self, schema: DbSchema) -> DbConfig                     //   declarative stores+indexes+per-collection
db.DbConfig.with_scope(self, scope: DbScope) -> DbConfig                        //     conflict policy (no DDL strings)
db.DbConfig.with_local_budget_bytes(self, bytes: u64) -> DbConfig
db.DbConfig.with_auto_sync(self, auto: DbAutoSync) -> DbConfig                  //   idle and/or interval

db.Db.open(config: DbConfig, data: RefAny, on_open: DbOpenCallback) -> RequestId
    // DbOpenCallbackType: fn(RefAny, CallbackInfo, ResultDbDbError) -> Update
db.Db.is_open(self) -> bool                                                     // KEEP (sync handle state)
db.Db.get(self, store: String, key: DbValue, data, on_result: DbValueCallback) -> RequestId
db.Db.set(self, store: String, key: DbValue, value: DbValue) -> bool            // FIRE: local mirror sync, persist async,
db.Db.delete(self, store: String, key: DbValue) -> bool                         //   marks row dirty in the oplog
db.Db.iterate(self, store: String, range: DbKeyRange, limit: u32, data, on_result: DbRowsCallback) -> RequestId
db.Db.query_index(self, store: String, index: String, range: DbKeyRange, limit: u32, data, on_result: DbRowsCallback) -> RequestId
db.Db.subscribe(self, scope: DbScope, data, on_change: DbChangeCallback) -> RequestId
    // extends the local working set to cover `scope`; on_change fires on working-set updates
db.Db.sync_now(self, scope: OptionDbScope, data, on_result: DbSyncStatusCallback) -> RequestId
    // explicit sync point: push dirty ops, then refresh the (given or whole) scoped working set;
    // offline ⇒ ops stay queued, callback reports Queued — NOT an error
db.Db.sync_status(self) -> DbSyncStatus                                         // sync poll getter
db.Db.set_on_sync_status(self, data, on_status: DbSyncStatusCallback)           // subscription (via completeRequest)
db.Db.set_on_conflict(self, store: String, data, on_merge: DbMergeCallback)     // custom merge (policy = Merge)
db.Db.close(self)
```

New types: `DbConfig`, `DbSchema` (stores + indexes + per-collection `DbConflictPolicy`),
`DbScope { collections: DbCollectionScopeVec }`,
`DbCollectionScope { store: String, range: OptionDbKeyRange, index_predicate: OptionDbIndexPredicate }`,
`DbKeyRange`, `DbIndexPredicate`, `DbAutoSync { interval: OptionDuration, on_idle: bool }`,
`DbConflictPolicy { LastWriteWins /*default*/, ServerWins, ClientWins, Merge }`,
`DbConflict { store, key, local: DbValue, remote: DbValue, local_modified/remote_modified: Instant }`,
`DbMergeCallbackType: fn(RefAny, CallbackInfo, DbConflict) -> DbValue` (synchronous guest
merge, invoked at sync-apply time),
`DbSyncStatus { state: DbSyncState, working_set_coverage_x1000: u32, pending_push_ops: u64,
last_synced: OptionInstant, local_bytes_used: u64, local_bytes_budget: u64,
quota_bytes_available: u64 /* web: navigator.storage.estimate */, error: OptionString }`,
`DbSyncState { Disconnected, Idle, Pushing, Pulling, Queued /*offline*/, Error }`, plus the
callback pairs (`DbOpenCallback`, `DbValueCallback`, `DbRowsCallback`, `DbChangeCallback`,
`DbSyncStatusCallback`, `DbMergeCallback`) and option wrappers. `DbValue`/`DbRows` are reused
as the value vocabulary. Eviction is engine policy, not API: clean rows evictable under quota
pressure, dirty rows never before a successful push (surfaced via `DbSyncStatus`, not knobs).

Delete list for db (see §6): `Db.open(path)` (replaced — no real paths on web), `Db.execute`,
`Db.query`. `is_open` stays. Offline queue, coverage/progress, conflict hooks = the "MORE
extensive, not less" growth area, all resumable.

**Phasing:** the §5.1 lifter cut lands EARLY (with Phase 0 — pure classification, no api.json
churn). The API redesign is its own track (Phase R, runs parallel from Phase 2 onward, ships
with Phase 4). Neither blocks Phase 1 — nothing in Phase 1 depends on `Db`.

---

## 6. Delete / deprecate list (single-surface compliance)

Every row is a **removal** from api.json (breaking, wanted). "→" = replacement.

| # | Deleted function | Replacement |
|---|---|---|
| D1 | `dialog.FileDialog.open_file` (sync form) | same name, resumable (E1) |
| D2 | `dialog.FileDialog.open_multiple_files` (sync) | resumable (E2) |
| D3 | `dialog.FileDialog.open_directory` (sync) | resumable (M1) |
| D4 | `dialog.FileDialog.save_file` (sync) | resumable `save_file` → `SaveTarget` (M2); plain byte export → `save_bytes` (E4) |
| D5 | `dialog.ColorPickerDialog.open` (sync) | resumable (E3) |
| D6-D12 | `file.File.open/create/read_to_string/read_to_bytes/write_string/write_bytes/close` (whole class + `OptionFile`) | `FilePath.read_bytes/read_string` resumable (E5/E6) + `FilePath.write_*` FIRE (M4) |
| D13 | `http.HttpRequestConfig.http_get` (sync) | resumable (E7) |
| D14 | `http.HttpRequestConfig.http_get_default` | compose: `HttpRequestConfig::create()` + E7 (default-config convenience not worth a second entry point) |
| D15 | `http.HttpRequestConfig.download_bytes` (sync) | resumable (E8) |
| D16 | `http.HttpRequestConfig.download_bytes_default` | compose (as D14) |
| D17 | `http.HttpRequestConfig.is_url_reachable` (sync bool) | resumable (E9) |
| D18 | `svg.FilePath.read_bytes` (sync) | resumable (E5) |
| D19 | `svg.FilePath.read_string` (sync) | resumable (E6) |
| D20 | `svg.FilePath.read_dir` (sync) | resumable (M3) |
| D21-D23 | `task.Thread.sleep_ms/sleep_us/sleep_ns` | none — `Timer::with_delay`/`with_interval` (blocking sleeps are wrong on every target's UI thread) |
| D24 | `callbacks.CallbackInfo.take_screenshot_to_file` | `take_screenshot` + `save_bytes` (E12) |
| D25 | `callbacks.CallbackInfo.take_native_screenshot(path)` (the path-writing variant) | keep `take_native_screenshot_bytes/_base64` as HONEST-ERR (probe `screen_capture`); bytes + `save_bytes` composes the file case |
| D26 | `audio.AudioDeviceList.enumerate` (sync) | resumable (E11) |
| D27 | `image.DecodedVideo.decode_mp4_h264` (sync) | resumable (M16) |
| D28 | `image.VideoEncoder.encode` / `VideoDecoder.decode` (sync byte-returning forms) | submit+poll pair (M16) |
| D29 | `screen.ScreenRecorder.finish` (sync bool) | resumable `finish` (M15) |
| D30 | `widgets.MapWidget.dom_with_fetch(cb: ThreadCallback)` | `dom_with_fetch()` framework-owned fetcher (M17) |
| D31 | `db.Db.open(path)` | `Db.open(config: DbConfig, data, on_open)` — local name + first-class `backup_sync_url`, no real paths on web (§5.3) |
| D32-D33 | `db.Db.execute` / `db.Db.query` (raw SQL) | KV/collection/index surface (§5); SQL demoted to desktop-internal |
| D34 | `dll/src/desktop/extra/udp/` + `dll/src/unified/udp.rs` dead modules; `PlatformCapability::udp` probe | delete modules; rename probe → `webtransport` |

**Explicitly NOT deleted** (would violate nothing): `MsgBox.ok/info/ok_cancel/yes_no`
(sync `alert`/`confirm` exist — the only OS dialogs that stay sync; `yes_no` button labels
approximate to OK/Cancel, documented), `Instant.now` (`performance.now()`), all `FilePath` pure
path algebra + the 20 known-folder/`temp`/`current` getters (virtual OPFS dirs, `dirs`-crate
desktop impl `layout/src/file.rs:705+`; `get_executable_dir` honestly `None` on web),
`exists/is_file/is_dir/metadata/canonicalize` (mirror-backed sync), clipboard/DnD event-scoped
getters, biometric/keyring/geolocation/sensor request+poll pairs, gamepad/monitor snapshots,
`WebTransport` (all 10 fns), `AudioSink` (open/play/frames_played FIRE+POLL), all
`PlatformCapability` probes (+ new: `thread`, `file_system`, `dialogs`, `http`, `multi_window`,
`sql`, `sync`), `Pdf.*`, `Url.*`, all Icu/Fluent, `VideoStartupCheck` (HONEST no-op),
`Thread.create` (HONEST-ERR + probe until worker mode), `take_screenshot(_base64)` (pure CPU
render).

---

## 7. Phase order and api.json churn

Maintainer priority: "decoding images, loading files, etc." intercepted first. Phases 0-2
below match `web-boundary-apis-plan.md` §6 with the hybrid removed; the db REDESIGN gets its
own parallel track. Churn = api.json entries added/deleted (class = one class entry).

| Phase | Content | api.json churn |
|---|---|---|
| **0 — runtime enablers** | `completeRequest` export + pending table; timer pump; `Instant::now` boundary; `FnClass::BoundaryJsImport` + manifest `impl:"js"`; callback-discovery decision (plan §7 Q2); **db lifter cut (§5.1)** — `AzDb_*` → `BoundaryJsImport` + `turso::`/`turso_core::` → `NeverLift` loud trap (pure classification, independent of the db API redesign) | +1 class (`task.RequestId`). No fn churn |
| **1 — bytes in, bytes out** (AzWriter ships) | E1, E2, E4 (`save_bytes`), E5, E6, **E10 (image decode intercept)**, E12; M4 semantics note; M5 (delete `File`); M18 (FileInput rewrite); D1/D2/D5/D6-D12/D18/D19/D24; MsgBox stays sync via alert/confirm | **+9 fns, +12 classes** (7 cb-type+wrapper pairs, `FilePathVec` set, `RequestId` done in P0), **−13 fns, −2 classes** (`File`, `OptionFile`) |
| **2 — HTTP + events + honesty** | E7, E8, E9; M6 (DnD registry), M7 (clipboard contract), M13 (monitors); capability probes (+6 constructors on `PlatformCapability`); D13-D17 | **+3 fns, +3 classes, +6 probe ctors, −5 fns** |
| **3 — pickers II + poll backends** | M1, M2 (`SaveTarget`), M3; M8-M12 runtime wiring (no API change); D3/D4/D20/D25 | **+4 fns, +4 classes** (`SaveTarget`, `OptionSaveTarget`, 2 cb pairs), **−4 fns** |
| **4 — media (JS-owned resources)** | M14 (widgets, API unchanged), M15, M16, M17, E11; AudioSink runtime; D26-D30 | **+8 fns, +3 classes, −7 fns** |
| **R — db redesign** (parallel, from P2; ships with P4) | §5.3 surface (scoped working-set replication + backup-sync + conflict machinery); D31-D33 | **+22 fns** (7 DbConfig builders + 13 Db + is_open kept + ctor), **+20 classes** (schema/scope/status/conflict + 6 cb pairs + options), **−3 fns** |
| **5 — transport + escape hatches** | WebTransport JS engine (0 churn); iroh design (new classes when real); worker+SAB evaluation; ~~JSPI~~ dropped | 0 (iroh TBD) |

Net: roughly **+46 functions / +42 classes added, −32 functions / −2 classes deleted** across
the whole program — the surface grows (mostly the db redesign) while becoming 100% portable,
and every deleted symbol has a named replacement in §6.

Sequencing rationale: Phase 1 needs only Phase 0's four runtime pieces; nothing in Phase 1
touches pickers-with-handles (M1/M2), subscriptions, media, or db. The riskiest unresolved
design item remains **callback discovery for request-passed fn-ptrs** (plan §7 Q2) — it gates
every resumable API and must be settled in Phase 0.

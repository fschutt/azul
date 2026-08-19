# WORK ORDER — Resumable-Callback API Rewrite

Status: **implementation work order** (executable spec, not a proposal) · Written 2026-08-19 ·
Branch `weblift/x86-lifter-fixes`, draft PR #431 · repo HEAD when verified: `4d57444d7`

**Who this is for.** One engineer/agent executes this top-to-bottom. Every design question is
already settled; this document says *what to change, where, in what order, and how to prove it
worked*. Do not re-litigate the design — if you think a decision is wrong, finish the phase and
raise it separately.

**Inputs already decided (read once, then stop):**

| Doc | What it settles |
|---|---|
| `doc/web-boundary-apis-plan.md` §4.1 | The RESUMABLE CALLBACK STYLE — normative design |
| `doc/web-boundary-apis-plan.md` §5.1 | First-draft signatures + the RefAny-pair delivery contract |
| `doc/web-boundary-apis-plan.md` §5.6 | `db` module direction (local-first + backup sync, no engine on web) |
| `doc/web-api-remodel-triage.md` | Per-function EASY/MEDIUM/HARD triage, delete list, phase order |
| `doc/web-platform-api-research.md` | Browser support/permission facts constraining every promise |
| `doc/web-json-hydrate-plan.md` | The RefAny reflection bridge these APIs ride on |
| `scripts/web-e2e-harness-plan.md` §6.5 (`:591`) | The e2e mock protocol for deterministic dialog results |

### Corrections to those docs that this work order supersedes

All verified against `api.json` at `0.2.0` (1888 classes / 2961 constructors+functions) and against
source at HEAD `4d57444d7`.

1. `web-api-remodel-triage.md` says **`svg.FilePath`** throughout. Wrong. `FilePath` lives in
   api.json module **`file`**, moved there by commit `4bd0ca520` *"fix(doc/autofix): FilePath
   belongs in the file module, not svg"*. Cite `file.FilePath.*`.
2. The triage §2 proposes **ten per-payload callback typedefs** (`FileOpenCallback`,
   `HttpResponseCallback`, …). Superseded by boundary plan §4.1/§5.1: **ONE universal callback
   typedef, ONE completion export**, per-operation result structs type-erased into a `RefAny`.
   Removes ~20 classes of churn.
3. `audio.AudioDeviceList.enumerate`, `image.DecodedVideo.decode_mp4_h264`,
   `image.RawImage.decode_image_bytes_any` and `task.Thread.sleep_ms/_us/_ns` are api.json
   **constructors**, not functions.
4. `doc/web-json-hydrate-plan.md` describes the JSON hydrate bridge as unbuilt. It has landed:
   `AzStartup_hydrateJson` (`dll/src/web/eventloop.rs:550`),
   `AzStartup_registerStateDeserializer` (`:675`), loader side `azHydrateJsonMarkers`
   (`dll/src/web/loader_js.rs:734`, calling the setter at `:752`), extra-root seeding of the
   deserializer (`dll/src/web/mod.rs:1077-1085`). Treat the reflection bridge as **present**.
5. `web-boundary-apis-plan.md` §5.1 proposes an optional api.json `"web"` key. **A raw JSON key
   cannot work**: `ClassData`/`FunctionData` (`doc/src/api.rs:1064-1125`, `:1268-1298`) have no
   unknown-field capture and every `normalize`/`patch` run re-serializes the file, so an unknown
   key is **silently dropped**. See §4.5 — this becomes a `doc/src/api.rs` code change, not a JSON
   edit.
6. `web-boundary-apis-plan.md` §3 row 59 says the desktop db engine is `rusqlite`. It is **turso**
   (pure-Rust SQLite, `dll/src/desktop/extra/sqlite/`).
7. The plan's §4.1 claim that a resume "runs the normal Update→relayout→patch cycle" overstates
   what `AzStartup_dispatchEvent` does today: it emits a patch and returns; **relayout is
   JS-driven** through the separate `AzStartup_relayout` export (`eventloop.rs:2189`). Copy the
   real shape, not the description (§5.1).

---

## 1. The design (normative — restated so this document stands alone)

### 1.1 One surface

There is exactly **one** public API surface. Every api.json function has one signature, identical
on desktop, web, and mobile. No hybrid mode, no `_with`-suffixed twin, no sync variant left
behind, no `#[cfg]` in user code. Breaking changes are wanted: this is pre-1.0 and the point is to
break loudly at compile time before anyone depends on the old shape.

### 1.2 Request / Resume

Every OS-facing operation the browser can only answer asynchronously splits in two:

* **REQUEST** — called from inside any ordinary callback activation. It **never blocks**. It
  registers `{request_id, callback fn-ptr, cloned user RefAny}` in the runtime's pending table and
  returns a `RequestId` immediately. The calling callback then returns its `Update` and **the
  activation ends**.
* **RESUME** — when the result exists, the *runtime* re-enters the guest and invokes the stored
  result callback as a **fresh, ordinary callback activation**. A resume may issue the next
  request, so a chain of awaits becomes a chain of resumes: user code is a state machine over its
  own `RefAny`.

Guarantee to document on every request function: *the result callback never runs re-entrantly
inside the requesting activation. On desktop it may run within the same frame; on web it always
runs on a later task.*

### 1.3 Delivery contract — a PAIR of RefAnys

The resume callback receives exactly three values:

```text
callbacks.ResumeCallbackType : fn(data: RefAny, info: CallbackInfo, result: RefAny) -> Update
```

* `data` — the user's context `RefAny`, exactly as submitted, returned untouched.
* `info` — a normal `CallbackInfo` for this fresh activation.
* `result` — the per-operation **RESULT STRUCT**, built by the runtime and type-erased into a
  `RefAny`.

**One typedef, one completion export, per-operation result structs.** The pairing (request function
→ result struct type) is part of each request function's documented contract and is listed in §2.

Argument order matches the existing three-argument callback convention
(`callbacks.OnVideoFrameCallbackType : fn(RefAny, CallbackInfo, VideoFrame) -> Update`). Note the
closest *behavioural* precedent, `WriteBackCallbackType` (`layout/src/thread.rs:272-276`), orders
its arguments `(original_data, writeback_data, info)` — **do not copy that order**; copy its
semantics.

**How the guest gets the typed value back out.** In Rust: `result.downcast_ref::<T>()`. api.json
exposes no way to obtain a type's id (`callbacks.RefAny` has `get_type_id(self: ref) -> u64` and
`is_type(self: ref, type_id: u64) -> bool`, but nothing hands one out), so **every result struct
gets exactly one static accessor**:

```text
<module>.<T>.downcast(result: RefAny) -> Option<T>
```

`fn_body`: `result.downcast_ref::<T>().cloned().into()`. That is the binding-portable downcast for
C/C++/Python/etc. — 19 mechanical entries across the whole program (§2.8). It goes under
`"constructors"` (no `self` argument; see §4.4).

### 1.4 Desktop implements the identical signature

Desktop performs the blocking OS call inside the request function (native modal behaviour is
correct on desktop) and delivers the resume through the **existing deferred-apply queue**:

* `pub enum CallbackChange` — `layout/src/callbacks.rs:167` (ends `:553`)
* `CallbackInfo::push_change(&mut self, change: CallbackChange)` — `layout/src/callbacks.rs:850`
  (std) and `:860` (no_std); backing field `changes` at `:782`/`:784`
* precedent: `add_timer` pushes `CallbackChange::AddTimer` at `:928` (variant at `:204`);
  `add_thread` pushes `AddThread` at `:938` (variant at `:210`). Their api.json doc already reads
  *"applied after callback returns"*.
* **the single drain site**: `fn apply_user_change(...)` at
  `dll/src/desktop/shell2/common/event.rs:1383`, documented at `:1379` as *"This is the SINGLE
  place where all `CallbackChange` variants are handled. Adding a new variant causes a compile
  error here — no silent bugs."* Exhaustive match from `:1390`; `AddTimer` arm `:1568`, `AddThread`
  arm `:1586`. It is a provided trait method (declared `event.rs:1061`), reached from 14 drain
  sites across the shells, so a new arm is live on every platform automatically.

**There are already three "deferred invoke a user callback" precedents** — use them, do not invent:

| Precedent | Queue variant | Where the user callback is actually invoked |
|---|---|---|
| **Thread writeback** (closest match) | `AddThread` (`callbacks.rs:210`, arm `event.rs:1586`) | `layout/src/window.rs:4327-4331` inside `run_all_threads` (`:4234`), driven by `invoke_thread_callbacks` (`event.rs:5570`). Type: `WriteBackCallbackType = fn(RefAny, RefAny, CallbackInfo) -> Update` (`layout/src/thread.rs:272-276`). A fresh changes vector (`window.rs:4301`) and fresh `CallbackInfo` (`:4319-4325`) are built per invocation and drained at `:4334-4339`. |
| **Timer** | `AddTimer` (`callbacks.rs:204`, arm `event.rs:1568`) | `LayoutWindow::run_single_timer` (`window.rs:4143`), `timer.invoke(...)` at `:4209` |
| **Virtual view** (cleanest template) | `UpdateVirtualView` (`callbacks.rs:236`, arm `event.rs:1664`) | arm only *queues* (`window.rs:7682`); a later frame-time pump `process_pending_virtual_view_updates` (`window.rs:7724`) invokes the user callback |

**The design.** Follow the virtual-view template: the `CallbackChange` arm *queues*, a pump
*invokes*.

```rust
// layout/src/callbacks.rs, added to `pub enum CallbackChange`
CompleteRequest {
    request_id: RequestId,
    data: RefAny,           // the user's context, cloned at request time
    callback: ResumeCallback,
    result: RefAny,         // the type-erased result struct
},
```

The `apply_user_change` arm pushes it onto a per-window pending-completion queue; a pump alongside
`invoke_expired_timers` / `invoke_thread_callbacks` (`event.rs:5424` / `:5570`) drains it, builds a
fresh `CallbackInfo` exactly as `run_all_threads` does (`window.rs:4301-4325`), invokes
`(callback.cb)(data, info, result)`, and drains the changes that callback itself pushed. Process
FIFO; append nested completions to the **end** of the queue, never recurse.

### 1.5 Web implements the same signature as a JS-backed boundary

* The request function is classified `classify::FnClass::WebJsImpl` →
  `symbol_table::FnClass::BoundaryJsImport`; the transitive lift stops there and the caller gets an
  `env.sub_<hex>` import (`BoundaryImport` documented `dll/src/web/symbol_table.rs:123`;
  loader-side substitution map `azBoundarySymbols` declared `dll/src/web/loader_js.rs:207`,
  preferred over the no-op stub at `:327-328`, populated by `azLoadBoundaryShards()` at
  `:348`/`:381`).
* The JS implementation may `await` a real browser API (`showOpenFilePicker`, `fetch`,
  `createImageBitmap`, permission prompts) because the guest has already returned.
* On resolution JS re-enters through the new export `AzStartup_completeRequest` (§5.1), which looks
  up the pending entry, builds the result struct **in lifted Rust**, calls the stored guest
  fn-pointer, and returns a TLV patch buffer for `azApplyPatches` (`loader_js.rs:997`) — the
  `azDispatch` shape (`loader_js.rs:877`).

### 1.6 What does NOT become resumable

Operations the browser answers synchronously stay synchronous; operations already non-blocking keep
their shape. They are the **KEEP** rows in §2.7. Do not add variants for them.

---

## 2. Scope table

Legend for **Disposition**:

* **REMODEL** — the existing entry's signature changes in place, same name (triage §1 naming
  policy: same-name + new-arity breaks loudly, which is the goal).
* **NEW** — a new api.json entry.
* **DELETE** — removed; the "New signature" column names the replacement.
* **RENAME** — same shape, different name.
* **KEEP** — signature unchanged; implementation work only (§2.7).

"Current signature" cells are verbatim from `api.json` `0.2.0`. `RequestId` is `task.RequestId`;
`ResumeCallback` is `callbacks.ResumeCallback` (§3.1).

### 2.1 Phase 0 — the primitive (5 entries)

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| P0-1 | `task.Thread.sleep_ms(milliseconds: u64) -> EmptyStruct` *(ctor)* | — | — | DELETE → `task.Timer.with_delay(self: value, delay: Duration) -> Timer` |
| P0-2 | `task.Thread.sleep_us(microseconds: u64) -> EmptyStruct` *(ctor)* | — | — | DELETE → as P0-1 |
| P0-3 | `task.Thread.sleep_ns(nanoseconds: u64) -> EmptyStruct` *(ctor)* | — | — | DELETE → as P0-1 |
| P0-4 | — | `task.RequestId.invalid() -> RequestId` *(ctor)* | — | NEW |
| P0-5 | — | `task.RequestId.is_valid(self: ref) -> bool` | — | NEW |

New classes: `task.RequestId`, `callbacks.ResumeCallbackType`, `callbacks.ResumeCallback` (§3.1).

### 2.2 Phase 1 — bytes in, bytes out (21 entries) — *AzWriter ships*

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| P1-1 | `dialog.FileDialog.open_file(title: String, default_path: OptionString, filter_list: OptionFileTypeList) -> OptionString` | `open_file(title: String, default_path: OptionString, filter_list: OptionFileTypeList, data: RefAny, on_result: ResumeCallback) -> RequestId` | `dialog.FileOpenResult` | REMODEL |
| P1-2 | `dialog.FileDialog.open_multiple_files(title: String, default_path: OptionString, filter_list: OptionFileTypeList) -> OptionStringVec` | `open_multiple_files(title: String, default_path: OptionString, filter_list: OptionFileTypeList, data: RefAny, on_result: ResumeCallback) -> RequestId` | `dialog.FileOpenMultiResult` | REMODEL |
| P1-3 | `dialog.ColorPickerDialog.open(title: String, default_value: OptionColorU) -> OptionColorU` | `open(title: String, default_value: OptionColorU, data: RefAny, on_result: ResumeCallback) -> RequestId` | `dialog.ColorPickResult` | REMODEL |
| P1-4 | `file.FilePath.read_bytes(self: ref) -> ResultU8VecFileError` | `read_bytes(self: ref, data: RefAny, on_result: ResumeCallback) -> RequestId` | `file.FileReadBytesResult` | REMODEL |
| P1-5 | `file.FilePath.read_string(self: ref) -> ResultStringFileError` | `read_string(self: ref, data: RefAny, on_result: ResumeCallback) -> RequestId` | `file.FileReadStringResult` | REMODEL |
| P1-6 | — | `dialog.FileDialog.save_bytes(suggested_name: String, mime: String, bytes: U8Vec) -> bool` | — (FIRE; `bool` = *scheduled*) | NEW |
| P1-7 | — | `image.RawImage.decode_image_bytes(bytes: U8Vec, data: RefAny, on_result: ResumeCallback) -> RequestId` | `image.ImageDecodeResult` | NEW |
| P1-8 | `callbacks.CallbackInfo.take_screenshot_to_file(self: ref, dom_id: DomId, path: String) -> ResultVoidString` | — | — | DELETE → `take_screenshot(dom_id)` + `save_bytes` (P1-6) |
| P1-9 | `file.File.open(path: String) -> OptionFile` *(ctor)* | — | — | DELETE (whole class) |
| P1-10 | `file.File.create(path: String) -> OptionFile` *(ctor)* | — | — | DELETE |
| P1-11 | `file.File.read_to_string(self: refmut) -> OptionString` | — | — | DELETE → `file.FilePath.read_string` (P1-5) |
| P1-12 | `file.File.read_to_bytes(self: refmut) -> OptionU8Vec` | — | — | DELETE → `file.FilePath.read_bytes` (P1-4) |
| P1-13 | `file.File.write_string(self: refmut, string: String) -> bool` | — | — | DELETE → `file.FilePath.write_string` (KEEP-FIRE, §2.7) |
| P1-14 | `file.File.write_bytes(self: refmut, bytes: U8VecRef) -> bool` | — | — | DELETE → `file.FilePath.write_bytes` (KEEP-FIRE) |
| P1-15 | `file.File.close(self: value)` | — | — | DELETE |
| P1-16 | — | `dialog.FileOpenResult.downcast(result: RefAny) -> OptionFileOpenResult` | — | NEW (accessor) |
| P1-17 | — | `dialog.FileOpenMultiResult.downcast(result: RefAny) -> OptionFileOpenMultiResult` | — | NEW (accessor) |
| P1-18 | — | `dialog.ColorPickResult.downcast(result: RefAny) -> OptionColorPickResult` | — | NEW (accessor) |
| P1-19 | — | `file.FileReadBytesResult.downcast(result: RefAny) -> OptionFileReadBytesResult` | — | NEW (accessor) |
| P1-20 | — | `file.FileReadStringResult.downcast(result: RefAny) -> OptionFileReadStringResult` | — | NEW (accessor) |
| P1-21 | — | `image.ImageDecodeResult.downcast(result: RefAny) -> OptionImageDecodeResult` | — | NEW (accessor) |

Class churn: **+16** (6 result structs + their 6 `Option…` wrappers + the 4-class `FilePathVec`
set), **−2** (`file.File`, `option.OptionFile`).

`image.RawImage.decode_image_bytes_any(bytes: U8VecRef) -> ResultRawImageDecodeImageError`
**stays** (KEEP-PURE — the `image` crate lifts and works everywhere). P1-7 is the *fast* path on
web (`createImageBitmap` → hardware decoder → RGBA readback). Document in both doc strings that
decoded pixels may differ per backend (premultiplication / ICC).

### 2.3 Phase 2 — HTTP + events + capability honesty (16 entries)

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| P2-1 | `http.HttpRequestConfig.http_get(self: ref, url: String) -> ResultHttpResponseHttpError` | `http_get(self: ref, url: String, data: RefAny, on_result: ResumeCallback) -> RequestId` | `http.HttpGetResult` | REMODEL |
| P2-2 | `http.HttpRequestConfig.download_bytes(self: ref, url: String) -> ResultU8VecHttpError` | `download_bytes(self: ref, url: String, data: RefAny, on_result: ResumeCallback) -> RequestId` | `http.HttpBytesResult` | REMODEL |
| P2-3 | `http.HttpRequestConfig.is_url_reachable(url: String) -> bool` | `is_url_reachable(self: ref, url: String, data: RefAny, on_result: ResumeCallback) -> RequestId` | `http.HttpReachableResult` | REMODEL (gains `self` so it honours the config/timeout) |
| P2-4 | `http.HttpRequestConfig.http_get_default(url: String) -> ResultHttpResponseHttpError` | — | — | DELETE → `HttpRequestConfig::create()` + P2-1 |
| P2-5 | `http.HttpRequestConfig.download_bytes_default(url: String) -> ResultU8VecHttpError` | — | — | DELETE → `HttpRequestConfig::create()` + P2-2 |
| P2-6 | `window.PlatformCapability.udp() -> PlatformCapability` *(ctor)* | `window.PlatformCapability.webtransport() -> PlatformCapability` | — | RENAME (triage D34; the `Udp` class is already gone, the probe is the leftover) |
| P2-7 | — | `window.PlatformCapability.thread()` | — | NEW |
| P2-8 | — | `window.PlatformCapability.file_system()` | — | NEW |
| P2-9 | — | `window.PlatformCapability.dialogs()` | — | NEW |
| P2-10 | — | `window.PlatformCapability.http()` | — | NEW |
| P2-11 | — | `window.PlatformCapability.multi_window()` | — | NEW |
| P2-12 | — | `window.PlatformCapability.sql()` | — | NEW |
| P2-13 | — | `window.PlatformCapability.sync()` | — | NEW |
| P2-14 | — | `http.HttpGetResult.downcast(result: RefAny) -> OptionHttpGetResult` | — | NEW (accessor) |
| P2-15 | — | `http.HttpBytesResult.downcast(result: RefAny) -> OptionHttpBytesResult` | — | NEW (accessor) |
| P2-16 | — | `http.HttpReachableResult.downcast(result: RefAny) -> OptionHttpReachableResult` | — | NEW (accessor) |

Class churn: **+6** (3 result structs + 3 `Option…`).

Honesty constraints to encode in doc strings (research §9): CORS applies on web and cannot be
escaped; `mode:'no-cors'` makes "reachable" approximate; Chrome 142+ prompts for Local Network
Access on loopback/LAN targets. Reuse `HttpError::Other(String)` — do **not** add a `Cors` variant
in v1.

Also Phase 2, no api.json churn: delete the dead `dll/src/desktop/extra/udp/` and
`dll/src/unified/udp.rs` modules (triage D34).

### 2.4 Phase 3 — pickers II + write targets (8 entries)

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| P3-1 | `dialog.FileDialog.open_directory(title: String, default_path: OptionString) -> OptionString` | `open_directory(title: String, default_path: OptionString, data: RefAny, on_result: ResumeCallback) -> RequestId` | `dialog.FileOpenResult` (reused; the path is the virtual dir root) | REMODEL |
| P3-2 | `dialog.FileDialog.save_file(title: String, default_path: OptionString) -> OptionString` | `save_file(title: String, suggested_name: String, data: RefAny, on_result: ResumeCallback) -> RequestId` | `dialog.SaveTargetResult` | REMODEL (arg 2 changes meaning: a *name*, not a path) |
| P3-3 | `file.FilePath.read_dir(self: ref) -> ResultDirEntryVecFileError` | `read_dir(self: ref, data: RefAny, on_result: ResumeCallback) -> RequestId` | `file.FileDirListResult` | REMODEL |
| P3-4 | `callbacks.CallbackInfo.take_native_screenshot(self: ref, path: String) -> ResultVoidString` | — | — | DELETE → keep `take_native_screenshot_bytes`/`_base64` (HONEST-ERR) + `save_bytes` |
| P3-5 | — | `dialog.SaveTarget.write_bytes(self: ref, bytes: U8Vec) -> bool` | — (FIRE) | NEW |
| P3-6 | — | `dialog.SaveTarget.as_path(self: ref) -> OptionFilePath` | — | NEW (`None` on the download fallback) |
| P3-7 | — | `dialog.SaveTargetResult.downcast(result: RefAny) -> OptionSaveTargetResult` | — | NEW (accessor) |
| P3-8 | — | `file.FileDirListResult.downcast(result: RefAny) -> OptionFileDirListResult` | — | NEW (accessor) |

Class churn: **+7** — `SaveTarget`, `SaveTargetKind`, `OptionSaveTarget`, `SaveTargetResult`,
`OptionSaveTargetResult`, `FileDirListResult`, `OptionFileDirListResult`.

Honesty constraints (research §1/§3): `showDirectoryPicker`/`showSaveFilePicker` are **Chromium
only** — Firefox and Safari hold negative standards positions and will not ship them. Portable
fallbacks are `<input webkitdirectory>` (read-only snapshot) and the `<a download>` Blob save.
`SaveTarget::as_path` therefore returns `None` on the fallback path; apps that only export bytes
must be steered to `save_bytes` (P1-6). Put the decision tree in `save_file`'s doc string.

### 2.5 Phase 4 — media as JS-owned resources (11 entries)

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| P4-1 | `audio.AudioDeviceList.enumerate() -> AudioDeviceList` *(ctor)* | `audio.AudioDeviceList.enumerate(data: RefAny, on_result: ResumeCallback) -> RequestId` *(moves to `functions`)* | `audio.AudioDeviceListResult` | REMODEL |
| P4-2 | `image.DecodedVideo.decode_mp4_h264(bytes: U8VecRef) -> OptionDecodedVideo` *(ctor)* | `image.DecodedVideo.decode_mp4_h264(bytes: U8Vec, data: RefAny, on_result: ResumeCallback) -> RequestId` *(moves to `functions`)* | `image.VideoDecodeResult` | REMODEL |
| P4-3 | `image.VideoEncoder.encode(self: ref, frame: VideoFrame, force_keyframe: bool) -> U8Vec` | `encode(self: ref, frame: VideoFrame, force_keyframe: bool) -> bool` (enqueue) | — | REMODEL |
| P4-4 | — | `image.VideoEncoder.recv_packet(self: refmut) -> OptionU8Vec` | — (poll) | NEW |
| P4-5 | `image.VideoDecoder.decode(self: ref, data: U8Vec) -> OptionVideoFrame` | `decode(self: ref, data: U8Vec) -> bool` (enqueue) | — | REMODEL |
| P4-6 | — | `image.VideoDecoder.recv_frame(self: refmut) -> OptionVideoFrame` | — (poll) | NEW |
| P4-7 | `screen.ScreenRecorder.finish(self: refmut) -> bool` | `finish(self: refmut, data: RefAny, on_result: ResumeCallback) -> RequestId` | `screen.ScreenRecordingResult` | REMODEL |
| P4-8 | `widgets.MapWidget.dom_with_fetch(self: value, cb: ThreadCallback) -> Dom` | `dom_with_fetch(self: value) -> Dom` | — | REMODEL (drop the user `ThreadCallback`) |
| P4-9 | — | `audio.AudioDeviceListResult.downcast(result: RefAny) -> OptionAudioDeviceListResult` | — | NEW (accessor) |
| P4-10 | — | `image.VideoDecodeResult.downcast(result: RefAny) -> OptionVideoDecodeResult` | — | NEW (accessor) |
| P4-11 | — | `screen.ScreenRecordingResult.downcast(result: RefAny) -> OptionScreenRecordingResult` | — | NEW (accessor) |

Class churn: **+6** (3 result structs + 3 `Option…`).

P4-3/P4-5: the desktop codec backend is **already dishonest** — VideoToolbox-only, everything else
is a no-op (`dll/src/desktop/extra/video_codec/mod.rs`), so `encode` returns an empty `U8Vec` on
Windows/Linux today. Submit+poll matches WebCodecs' output-callback model *and* unblocks Media
Foundation on Windows. `ScreenRecorder.start/is_recording/write_frame/frames_written` keep their
signatures (FIRE + POLL).

`MapWidget.dom_with_fetch` is the only api.json function besides `task.Thread.create` that takes a
user `ThreadCallback`; removing it shrinks the HARD surface to exactly one function.

### 2.6 Phase R — the `db` redesign (27 entries, parallel track)

Directive (`web-boundary-apis-plan.md` §5.6): the web target **must not ship an SQL engine** — no
turso-in-wasm, no sqlite-wasm, and no lifting turso for `:memory:` either. Local layer is
IndexedDB. Raw SQL therefore cannot be the portable surface, so `execute`/`query` are deleted and
turso stays a hidden desktop-internal backend. Remote backup/sync is a **first-class open parameter
on both targets**.

| # | Current api.json entry | New signature | Result struct | Disposition |
|---|---|---|---|---|
| PR-1 | `db.Db.open(path: String) -> Db` *(ctor)* | — | — | DELETE (no real paths on web) |
| PR-2 | `db.Db.execute(self: ref, sql: String, params: DbValueVec) -> usize` | — | — | DELETE (raw SQL) |
| PR-3 | `db.Db.query(self: ref, sql: String, params: DbValueVec) -> DbRows` | — | — | DELETE (raw SQL) |
| PR-4 | — | `db.DbConfig.create(local_name: String) -> DbConfig` *(ctor)* | — | NEW |
| PR-5 | — | `db.DbConfig.with_backup_sync_url(self: value, url: String) -> DbConfig` | — | NEW |
| PR-6 | — | `db.DbConfig.with_auth_token(self: value, token: String) -> DbConfig` | — | NEW |
| PR-7 | — | `db.DbConfig.with_schema(self: value, schema: DbSchema) -> DbConfig` | — | NEW |
| PR-8 | — | `db.DbConfig.with_scope(self: value, scope: DbScope) -> DbConfig` | — | NEW |
| PR-9 | — | `db.DbConfig.with_local_budget_bytes(self: value, bytes: u64) -> DbConfig` | — | NEW |
| PR-10 | — | `db.DbConfig.with_auto_sync(self: value, auto: DbAutoSync) -> DbConfig` | — | NEW |
| PR-11 | — | `db.Db.open(config: DbConfig, data: RefAny, on_open: ResumeCallback) -> RequestId` | `db.DbOpenResult` | NEW |
| PR-12 | — | `db.Db.get(self: ref, store: String, key: DbValue, data: RefAny, on_result: ResumeCallback) -> RequestId` | `db.DbValueResult` | NEW |
| PR-13 | — | `db.Db.set(self: ref, store: String, key: DbValue, value: DbValue) -> bool` | — (FIRE; marks the row dirty) | NEW |
| PR-14 | — | `db.Db.delete(self: ref, store: String, key: DbValue) -> bool` | — (FIRE) | NEW |
| PR-15 | — | `db.Db.iterate(self: ref, store: String, range: DbKeyRange, limit: u32, data: RefAny, on_result: ResumeCallback) -> RequestId` | `db.DbRowsResult` | NEW |
| PR-16 | — | `db.Db.query_index(self: ref, store: String, index: String, range: DbKeyRange, limit: u32, data: RefAny, on_result: ResumeCallback) -> RequestId` | `db.DbRowsResult` | NEW |
| PR-17 | — | `db.Db.subscribe(self: ref, scope: DbScope, data: RefAny, on_change: ResumeCallback) -> RequestId` | `db.DbChangeResult` (fires repeatedly) | NEW |
| PR-18 | — | `db.Db.sync_now(self: ref, scope: OptionDbScope, data: RefAny, on_result: ResumeCallback) -> RequestId` | `db.DbSyncStatusResult` | NEW |
| PR-19 | — | `db.Db.sync_status(self: ref) -> DbSyncStatus` | — (poll getter) | NEW |
| PR-20 | — | `db.Db.set_on_sync_status(self: refmut, data: RefAny, on_status: ResumeCallback)` | `db.DbSyncStatusResult` (repeats) | NEW |
| PR-21 | — | `db.Db.set_on_conflict(self: refmut, store: String, data: RefAny, on_merge: DbMergeCallback)` | — (synchronous merge) | NEW |
| PR-22 | — | `db.Db.close(self: refmut)` | — | NEW |
| PR-23 | — | `db.DbOpenResult.downcast(result: RefAny) -> OptionDbOpenResult` | — | NEW (accessor) |
| PR-24 | — | `db.DbValueResult.downcast(result: RefAny) -> OptionDbValueResult` | — | NEW (accessor) |
| PR-25 | — | `db.DbRowsResult.downcast(result: RefAny) -> OptionDbRowsResult` | — | NEW (accessor) |
| PR-26 | — | `db.DbChangeResult.downcast(result: RefAny) -> OptionDbChangeResult` | — | NEW (accessor) |
| PR-27 | — | `db.DbSyncStatusResult.downcast(result: RefAny) -> OptionDbSyncStatusResult` | — | NEW (accessor) |

`db.Db.is_open(self: ref) -> bool` **stays** unchanged.

`DbMergeCallback` is the **one deliberate exception** to the single-typedef rule: conflict merge is
a *synchronous* guest computation invoked at sync-apply time, not a resume, and it returns a
`DbValue` rather than an `Update`:
`callbacks.DbMergeCallbackType : fn(RefAny, CallbackInfo, DbConflict) -> DbValue`.

Class churn for Phase R: **~30 new classes**, enumerated in §3.7.

**Subscriptions and repeat delivery.** `subscribe` (PR-17) and `set_on_sync_status` (PR-20) fire
their callback **many times** for one registration, so `AzStartup_completeRequest` must support a
*non-consuming* completion: the pending entry carries `once: bool`. Consuming entries are removed
after delivery; subscription entries persist until the owning object is dropped (§8 R7).

### 2.7 KEEP rows — no signature change, implementation work only

In scope for the named phase, but changing no api.json entry. Counts are exact function entries.

| Phase | Group | Functions | Web contract | Count |
|---|---|---|---|---|
| 0 | `time.Instant.now` | `now()` | `performance.now()` via a JS boundary (currently lifts to garbage) | 1 |
| 0 | Timer plumbing | `callbacks.CallbackInfo.add_timer/remove_timer/get_timer/get_timer_ids` | needs the `AzStartup_tickTimers` pump (§5.2) | 4 |
| 1 | Message boxes | `dialog.MsgBox.ok/info/ok_cancel/yes_no` | KEEP-SYNC — `alert()`/`confirm()`; `yes_no` buttons read OK/Cancel (documented approximation) | 4 |
| 1 | `file.FilePath` FIRE mutations | `write_bytes/write_string/create_dir/create_dir_all/remove_file/remove_dir/remove_dir_all/rename_to/copy_to` | KEEP-FIRE — JS mirror mutated synchronously (read-your-writes) + OPFS async; the `Result` narrows to validation-time errors, durability not implied | 9 |
| 1 | `file.FilePath` sync metadata | `exists/is_file/is_dir/metadata/canonicalize` | KEEP-SYNC off the JS index mirror; eventual-consistency caveat documented | 5 |
| 1 | `file.FilePath` known folders | `get_temp_dir/get_current_dir` + the 19 other `get_*_dir` constructors | KEEP-SYNC — fixed virtual OPFS paths; `get_executable_dir` honestly `None` | 21 |
| 1 | `widgets.FileInput` | `create/set_default_text/with_default_text/set_on_path_change/with_on_path_change/dom/swap_with_default` | **API UNCHANGED** — internal rewrite onto P1-1 (§6.2). The proof case. | 7 |
| 1 | Pure screenshots | `callbacks.CallbackInfo.take_screenshot/take_screenshot_base64` | KEEP-PURE (CPU render lifts; perf caveat) | 2 |
| 2 | Clipboard | `get_clipboard_content/set_clipboard_content/set_copy_content/set_cut_content/inspect_copy_changeset/inspect_cut_changeset` | KEEP-SYNC, narrowed: guaranteed inside clipboard events (`ClipboardEvent.clipboardData`), `None` outside on web; setters FIRE via `navigator.clipboard` outside events | 6 |
| 2 | Drag & drop | `get_hovered_file/get_dropped_file/get_dragged_file/is_file_drag_active/accept_drop/get_drag_types/set_drag_data/set_drop_effect/get_drag_state` | KEEP-SYNC (EVENT) — HTML5 drag events; dropped `File`s registered in the picked-file registry as `/az/picked/<id>/…` and pre-read at drop | 9 |
| 2 | Monitors / DPI | `callbacks.CallbackInfo.get_monitors/get_current_monitor`, `app.App.get_monitors` | KEEP-SYNC — one synthetic monitor from `window.screen` + `devicePixelRatio` (multi-monitor is `getScreenDetails`, Chromium-only) | 3 |
| 3 | Biometric | `request_biometric_auth/get_biometric_result/get_biometric_kind` | KEEP-POLL — WebAuthn `credentials.get` with `userVerification:'required'`; needs a registration ceremony + gesture + HTTPS | 3 |
| 3 | Keyring | `keyring_store/keyring_get/keyring_delete/get_keyring_result` | KEEP-POLL — IndexedDB + WebCrypto non-extractable keys; **materially weaker** than Keychain/Hello, probe reports `backend:"web-crypto"` | 4 |
| 3 | Geolocation | `get_location_fix` | KEEP-POLL — `watchPosition` pushes fixes into guest state | 1 |
| 3 | Sensors | `get_sensor_reading` | KEEP-POLL — Generic Sensor is Chromium-only; portable path is `devicemotion`, and iOS needs `DeviceMotionEvent.requestPermission()` from a gesture, so the first read stays `None` | 1 |
| 3 | Gamepad | `get_gamepad_state/get_primary_gamepad` | KEEP-SYNC — `navigator.getGamepads()` is a sync snapshot; refresh guest state per frame | 2 |
| 3 | Honest screenshots | `take_native_screenshot_bytes/take_native_screenshot_base64` | HONEST-ERR — no OS window exists; `Err` + `PlatformCapability::screen_capture` says so | 2 |
| 4 | Capture widgets | `widgets.CameraWidget/MicrophoneWidget/ScreenCaptureWidget` × `create/dom/with_on_frame/set_on_frame` | **API UNCHANGED** — replace the desktop bg capture thread with a JS-owned `getUserMedia`/`getDisplayMedia` resource pushing frames into guest ring buffers; `on_frame` fires off the timer pump | 12 |
| 4 | Audio out | `audio.AudioSink.open/is_open/play/frames_played/close` | KEEP — Web Audio / AudioWorklet, JS-owned; `play` enqueues | 5 |
| 4 | Recorder FIRE/POLL | `screen.ScreenRecorder.start/is_recording/write_frame/frames_written` | KEEP — MediaRecorder; `start`'s path arg is reinterpreted as a suggested download name (doc change only) | 4 |
| 4 | Locale | `fluent.IcuLocalizerHandle.from_system_language` | KEEP-SYNC — `navigator.language` | 1 |
| 5 | WebTransport | all 10 `webtransport.WebTransport` functions | KEEP — the 2026-06 design already pre-conforms; JS-owned engine (browser WebTransport, Baseline 2026-03), `stats()` serves a cached snapshot, `recv` drains a ring buffer from a Timer | 10 |
| 5 | Threads | `task.Thread.create`, `callbacks.CallbackInfo.add_thread/remove_thread/get_thread/get_thread_ids`, `task.ThreadSender.send`, `task.ThreadReceiver.recv`, `task.ThreadWriteBackMsg.create` | `Thread::create` = HONEST-ERR + `PlatformCapability::thread` false until a worker mode exists; the sender/receiver vocabulary survives as the channel for JS-owned resources | 8 |
| 5 | Windowing | `create_window/close_window/modify_window_state`, `app.App.add_window`, `get_current_window_handle` | KEEP + HONEST — one window per page; `create_window` no-ops, probe `multi_window` false, `RawWindowHandle` returns its none variant | 5 |
| — | Provisioning | `video.VideoStartupCheck.run/remediate` | HONEST no-op — "nothing to remediate" | 2 |

**KEEP total: 131 api.json function entries** (row sum: 1+4+4+9+5+21+7+2+6+9+3+3+4+1+1+2+2+12+5+4+1+10+8+5+2).

### 2.8 The count

| Phase | Signature-changing entries | REMODEL | NEW | DELETE | RENAME | of which `downcast` accessors |
|---|---|---|---|---|---|---|
| 0 — the primitive | 5 | 0 | 2 | 3 | 0 | 0 |
| 1 — bytes in, bytes out | 21 | 5 | 8 | 8 | 0 | 6 |
| 2 — HTTP + honesty | 16 | 3 | 10 | 2 | 1 | 3 |
| 3 — pickers II | 8 | 3 | 4 | 1 | 0 | 2 |
| 4 — media | 11 | 6 | 5 | 0 | 0 | 3 |
| R — db redesign | 27 | 0 | 24 | 3 | 0 | 5 |
| 5 — transport + escape hatches | 0 | 0 | 0 | 0 | 0 | 0 |
| **Total** | **88** | **17** | **53** | **17** | **1** | **19** |

**88 api.json function entries change** — 69 real operations plus 19 mechanical `downcast`
accessors. **131 further functions** are KEEP rows with implementation-only work (§2.7), for
**219 functions in scope overall**. The eleven `FilePathVec` boilerplate functions (§3.3) are on
top of that and not counted, since they are generated the way every other Vec's are.

**Reconciliation with the triage.** The triage's "~75" is the *inventory row count* from
`web-boundary-apis-plan.md` §3 ("Row count: 75 rows"), where a row is often a whole family (row 21
alone is 18 known-folder getters), not a function count. The triage's own arithmetic — "+46
functions / +42 classes added, −32 functions / −2 classes deleted" — double-counts every REMODEL as
one add *and* one delete, contradicting its own §1 naming policy of reusing the original name. This
work order counts a REMODEL as **one edited entry**. Folded together the two agree: 17 remodels
counted twice plus 53 genuine adds lands in its +46/−32 range, and its class figures shrink because
the ten per-payload callback typedefs collapse into one (§1.3).

---

## 3. New api.json entities — exact shapes

Reference shapes verified in `api.json`:

| Kind | Reference | api.json line |
|---|---|---|
| callback typedef | `callbacks.FileInputOnPathChangeCallbackType` | `api.json:7286-7304` |
| callback wrapper struct | `widgets.FileInputOnPathChangeCallback` | `api.json:43083-43114` |
| newtype | `task.TimerId` | `api.json:98318-98347` |
| Vec set | `vec.StringVec` / `…Destructor` / `…DestructorType` / `str.StringVecSlice` | `75880-76084` / `79232-79248` / `79133-79144` / `70130-70155` |
| Option wrapper | `option.OptionFilePath` | `api.json:92119-92136` |
| Result wrapper | `error.ResultU8VecFileError` | `api.json:96555-96572` |

**Rust homes are constrained by the autofixer** (§4.2/§4.3) — a class's `external` path prefix
decides whether autofix leaves it where you put it. Use the homes named per-class below, not a new
catch-all module.

### 3.1 The primitive (Phase 0)

```jsonc
// api.json module: task
"RequestId": {
    "doc": ["Identifies one in-flight request created by a resumable API function.",
            "Returned by every request function; `invalid()` is the never-matching sentinel."],
    "external": "azul_core::task::RequestId",
    "derive": ["PartialOrd", "Ord", "PartialEq", "Hash", "Debug", "Copy", "Clone", "Eq"],
    "struct_fields": [ { "id": { "type": "u64" } } ],
    "constructors": {
        "invalid": {
            "doc": ["A `RequestId` that never matches a real request — use it to initialise state."],
            "fn_args": [],
            "fn_body": "azul_core::task::RequestId::invalid()"
        }
    },
    "functions": {
        "is_valid": {
            "fn_args": [ { "self": "ref" } ],
            "returns": { "type": "bool" },
            "fn_body": "request_id.is_valid()"
        }
    },
    "repr": "C"
}
```

Sibling `task.TimerId` uses `usize`; `RequestId` uses `u64` deliberately — the id crosses to JS as
a BigInt and wasm32 `usize` is 32-bit. Precedent for a `u64` guest handle:
`EventloopState::state_deserializer: u64` (`dll/src/web/eventloop.rs:181`).

```jsonc
// api.json module: callbacks
"ResumeCallbackType": {
    "doc": ["The one callback shape every resumable API resumes into.",
            "`data` is the RefAny you passed to the request, returned untouched.",
            "`result` is the per-operation result struct, type-erased into a RefAny;",
            "downcast it with `<ResultStruct>::downcast(result)`.",
            "Never runs re-entrantly inside the requesting activation."],
    "external": "azul_core::callbacks::ResumeCallbackType",
    "callback_typedef": {
        "fn_args": [ { "type": "RefAny" }, { "type": "CallbackInfo" }, { "type": "RefAny" } ],
        "returns": { "type": "Update" }
    }
}
```

```jsonc
// api.json module: callbacks
"ResumeCallback": {
    "external": "azul_core::callbacks::ResumeCallback",
    "custom_impls": ["Clone", "Debug", "Eq", "Hash", "Ord", "PartialEq", "PartialOrd"],
    "derive": ["Debug", "Clone", "Eq", "Hash", "Ord", "PartialEq", "PartialOrd"],
    "struct_fields": [
        { "cb":  { "type": "ResumeCallbackType" } },
        { "ctx": { "type": "OptionRefAny" } }
    ],
    "repr": "C"
}
```

The `ctx: OptionRefAny` field is not decoration — every callback wrapper in api.json carries it
(`callbacks.TimerCallback` and `callbacks.ThreadCallback` name it `ctx`;
`widgets.FileInputOnPathChangeCallback` and `dom.MapPinTapCallback` name it `callable`). Use `ctx`.
Note callback typedefs carry **only** `external` + `callback_typedef` — no `derive`, no `repr`.

### 3.2 Result structs — the uniform shape

Plain `repr(C)` struct, `Clone`+`Debug` derives, one static `downcast` constructor, one `Option…`
sibling:

```jsonc
// api.json module: dialog
"FileOpenResult": {
    "doc": ["Result of `FileDialog::open_file` / `open_directory`. `path` is `None` if cancelled."],
    "external": "azul_layout::desktop::dialogs::FileOpenResult",
    "derive": ["Debug", "Clone", "PartialEq", "Eq"],
    "struct_fields": [ { "path": { "type": "OptionFilePath" } } ],
    "constructors": {
        "downcast": {
            "doc": ["Downcast the `result` RefAny delivered to a ResumeCallback."],
            "fn_args": [ { "result": "RefAny" } ],
            "returns": { "type": "OptionFileOpenResult" },
            "fn_body": "result.downcast_ref::<azul_layout::desktop::dialogs::FileOpenResult>().cloned().into()"
        }
    },
    "repr": "C"
}
```

```jsonc
// api.json module: option
"OptionFileOpenResult": {
    "external": "azul_layout::desktop::dialogs::OptionFileOpenResult",
    "derive": ["Clone", "Debug", "PartialEq", "Eq"],
    "enum_fields": [ { "None": {}, "Some": { "type": "FileOpenResult" } } ],
    "repr": "C, u8"
}
```

All 19 result structs — fields, api.json module, and required Rust home:

| Result struct | api.json module | Rust `external` prefix | Fields |
|---|---|---|---|
| `FileOpenResult` | dialog | `azul_layout::desktop::dialogs::` | `path: OptionFilePath` |
| `FileOpenMultiResult` | dialog | `azul_layout::desktop::dialogs::` | `paths: FilePathVec` |
| `ColorPickResult` | dialog | `azul_layout::desktop::dialogs::` | `color: OptionColorU` |
| `SaveTargetResult` | dialog | `azul_layout::desktop::dialogs::` | `target: OptionSaveTarget` |
| `FileReadBytesResult` | file | `azul_layout::file::` | `result: ResultU8VecFileError` |
| `FileReadStringResult` | file | `azul_layout::file::` | `result: ResultStringFileError` |
| `FileDirListResult` | file | `azul_layout::file::` | `result: ResultDirEntryVecFileError` |
| `HttpGetResult` | http | `azul_layout::http::` † | `result: ResultHttpResponseHttpError` |
| `HttpBytesResult` | http | `azul_layout::http::` † | `result: ResultU8VecHttpError` |
| `HttpReachableResult` | http | `azul_layout::http::` † | `reachable: bool`, `error: OptionString` |
| `ImageDecodeResult` | image | `azul_layout::image::` | `result: ResultRawImageDecodeImageError` |
| `VideoDecodeResult` | image | `azul_layout::image::` | `video: OptionDecodedVideo` |
| `AudioDeviceListResult` | audio | `azul_core::audio::` | `devices: AudioDeviceList` |
| `ScreenRecordingResult` | screen | `azul_core::screencap::` | `ok: bool`, `error: OptionString` |
| `DbOpenResult` | db | `azul_core::db::` | `result: ResultDbDbError` |
| `DbValueResult` | db | `azul_core::db::` | `value: OptionDbValue`, `error: OptionString` |
| `DbRowsResult` | db | `azul_core::db::` | `rows: DbRows`, `error: OptionString` |
| `DbChangeResult` | db | `azul_core::db::` | `store: String`, `key: DbValue`, `value: OptionDbValue` |
| `DbSyncStatusResult` | db | `azul_core::db::` | `status: DbSyncStatus` |

† `azul_layout::http::` and `azul_layout::image::` have **no arm** in `module_from_external_path`
today — see §4.3 for the two `module_map.rs` edits this requires.

**Every payload type already exists in api.json** except `FilePathVec` (§3.3), `OptionSaveTarget`
(§3.4) and the db types (§3.7). Verified present: `option.OptionFilePath`, `option.OptionColorU`,
`error.ResultU8VecFileError`, `error.ResultStringFileError`, `error.ResultDirEntryVecFileError`,
`error.ResultHttpResponseHttpError`, `error.ResultU8VecHttpError`,
`error.ResultRawImageDecodeImageError`, `option.OptionDecodedVideo`, `audio.AudioDeviceList`,
`option.OptionString`, `db.DbRows`, `db.DbValue`, `option.OptionDbValue`, `callbacks.RefAny`,
`callbacks.CallbackInfo`, `callbacks.Update`, `option.OptionRefAny`.

### 3.3 `FilePathVec` — the four-class Vec set (Phase 1)

Copy `vec.StringVec` verbatim, substituting `String`→`FilePath`. Four classes:
`vec.FilePathVec`, `vec.FilePathVecDestructor`, `vec.FilePathVecDestructorType`,
`file.FilePathVecSlice` — the `…VecSlice` lives in the **element's** module (`str.StringVecSlice`,
`file.DirEntryVecSlice`, `db.DbValueVecSlice`). Plus the eleven boilerplate functions `StringVec`
carries: `create`, `with_capacity`, `len`, `capacity`, `is_empty`, `get`, `from_item`,
`copy_from_ptr`, `c_get`, `as_c_slice`, `as_c_slice_range`. These eleven are pure vec boilerplate
and are **not** counted in §2.8. `doc/src/autofix/diff.rs:1205-1210` (`check_vec_functions`)
actively audits that all eleven exist; `collect_vec_required_types` (`diff.rs:1245`) protects the
Option/Slice companions from removal. `external`: `azul_layout::file::FilePathVec` etc.

`*Vec`, `*VecDestructor`, `*VecDestructorType` are **structurally forced** into module `vec` by
`determine_module` priority 2 (`doc/src/autofix/module_map.rs:510-517`) and the unconditional
structural branch at `:602-608`. You cannot put them anywhere else — do not try.

### 3.4 `SaveTarget` (Phase 3)

```jsonc
// api.json module: dialog
"SaveTarget": {
    "doc": ["An opaque write target obtained from `FileDialog::save_file`.",
            "Desktop: a real path. Web: a File-System-Access handle (Chromium) or a",
            "`Download` sentinel (Firefox/Safari), where `as_path` returns `None`."],
    "external": "azul_layout::desktop::dialogs::SaveTarget",
    "derive": ["Debug", "Clone", "PartialEq", "Eq"],
    "struct_fields": [
        { "kind":      { "type": "SaveTargetKind" } },
        { "path":      { "type": "OptionFilePath" } },
        { "handle_id": { "type": "u64" } }
    ],
    "functions": { "write_bytes": { "..." : "..." }, "as_path": { "..." : "..." } },
    "repr": "C"
}
```

plus `dialog.SaveTargetKind` = `{ Path, WebHandle, Download }` (`repr: "C"`, fieldless) and
`option.OptionSaveTarget` (`repr: "C, u8"`).

### 3.5 Error-vocabulary additions

`error.FileErrorKind` today is
`{NotFound, PermissionDenied, AlreadyExists, InvalidPath, IoError, DirectoryNotEmpty, IsDirectory, IsFile, Other}`
(`repr: "C"`, fieldless). Browser reality (research doc, cross-cutting rules) needs two more,
appended at the end so existing discriminants are stable:

* `NeedsUserGesture` — the picker/permission call happened outside a transient activation.
* `Unsupported` — no browser implementation exists on this engine (distinct from
  `PermissionDenied`, which is a *decision*).

`error.HttpError` gains nothing in v1 — reuse `HttpError::Other(String)` for CORS and
Local-Network-Access failures, with the message text documented.

### 3.6 Capability probes (Phase 2)

`window.PlatformCapability` today has exactly 11 constructors: `udp`, `camera`, `screen_capture`,
`microphone`, `audio_output`, `sensors`, `gamepad`, `geolocation`, `keyring`, `biometric`,
`video_codec`. Rename `udp` → `webtransport`, add `thread`, `file_system`, `dialogs`, `http`,
`multi_window`, `sql`, `sync`. Each is a zero-arg constructor returning `PlatformCapability`; web
implementations answer truthfully with `{available, backend: "web/<api>", reason}`.

### 3.7 Phase R class list

~30 new classes: `db.DbConfig`, `db.DbSchema`, `db.DbStoreSchema`, `db.DbIndexSchema`,
`db.DbScope`, `db.DbCollectionScope`, `db.DbKeyRange`, `db.DbIndexPredicate`, `db.DbAutoSync`,
`db.DbConflictPolicy` (`{LastWriteWins, ServerWins, ClientWins, Merge}`), `db.DbConflict`,
`db.DbSyncStatus`, `db.DbSyncState` (`{Disconnected, Idle, Pushing, Pulling, Queued, Error}`),
`error.DbError`, `error.ResultDbDbError`, `callbacks.DbMergeCallbackType`, `db.DbMergeCallback`,
the five result structs + five `Option…` siblings from §3.2, `option.OptionDbKeyRange`,
`option.OptionDbIndexPredicate`, `option.OptionDbScope`, and the four-class Vec sets for
`DbCollectionScope`, `DbStoreSchema`, `DbIndexSchema`.

`DbSyncStatus` fields (triage §5.3): `state: DbSyncState`, `working_set_coverage_x1000: u32`,
`pending_push_ops: u64`, `last_synced: OptionInstant`, `local_bytes_used: u64`,
`local_bytes_budget: u64`, `quota_bytes_available: u64` (web: `navigator.storage.estimate()`),
`error: OptionString`.

Semantics that are **requirements, not options** (triage §5.2): working-set replication with an
explicit scope + local budget; `sync_now` = push dirty ops then refresh; **offline is `Queued`,
never an error**; clean rows evictable under quota pressure, dirty rows never evicted before a
successful push; per-collection conflict policy, LWW default.

---

## 4. Codegen + autofix mechanics

### 4.1 The pipeline — and the command sequence you must actually run

`api.json` at the repo root is both the source of truth **and** a rewritten artifact. `azul-doc`
(`doc/src/main.rs:33`) parses `argv` with a slice-pattern match at `main.rs:45`.

**`codegen all` (`main.rs:1379-1387`) is read-only on api.json.** It calls
`codegen::v2::generate_all_v2` (`doc/src/codegen/v2/mod.rs:509-532`): build the IR
(`ir_builder.rs:38-86`, 10 phases including a hard-erroring validation phase at `:94-486`), run 35
language targets (`generator.rs:95-531`), then produce `target/codegen/api.json.br`
(`mod.rs:541-573`) and the compressed icon font.

**It does NOT run `autofix`, `patch`, or `normalize`.** Those *do* rewrite api.json, and CI fails if
running them produces any diff (`.github/workflows/rust.yml:97-110`). The full sequence after any
hand-edit — copied from `rust.yml:89-115` — is:

```bash
cargo run -r -p azul-doc autofix
cargo run -r -p azul-doc autofix explain
cargo run -r -p azul-doc patch safe target/autofix/patches
cargo run -r -p azul-doc patch target/autofix/patches
cargo run -r -p azul-doc normalize
cargo run -r -p azul-doc codegen all
cd dll && cargo test          # memtest: size/align of every Az* vs its `external` type
```

(`-r` is `--release`; see §9.) **Inspect the diff after step 5** — those five commands can move,
rename, or delete your class (§4.2). Your edit must be a fixpoint under them.

Generated outputs the web backend depends on:

* `target/codegen/api.json.br` — brotli of minified api.json, `include_bytes!`d by
  `dll/src/web/classify.rs:13-16`. `classify_api_functions()` (`classify.rs:78`) walks
  *version → api → module → classes → {constructors, functions}* (`:99-119`) and derives the C name
  `Az{Class}_{snake_case_to_lower_camel(fn)}` (`:113-114`). **Adding a function to api.json changes
  web classification with no Rust edit — but only after `codegen all` regenerates this file.**
* `target/codegen/dll_api_internal.rs` — the `cabi_export` bodies, `include!`d into `azul-dll` at
  `dll/src/lib.rs:195-209`. C-name synthesis: `ir_builder.rs:1466-1471`; helper
  `doc/src/utils/string.rs:2-15`.

`classify_fn` (`classify.rs:144-151`) is **prefix-based only** (`AzApp_run` → ServerEntryPoint,
`AzDisplayList_*`/`AzGl_*` → ReplaceWithDomPatcher, everything else → `Framework`). Every new
`Az*` function therefore defaults to `Framework` — i.e. it gets pulled into wasm — unless §4.5's
mechanism marks it otherwise.

Never hand-edit anything under `target/codegen/`.

### 4.2 The autofixer will move, rename, or delete your class if you let it

`doc/src/autofix/module_map.rs::determine_module(type_name) -> (String, bool)` (`:500-575`) decides
by **case-insensitive substring matching on the class name**, in this order:

1. `:505` starts_with `option` → `option`
2. `:510` ends_with `vec|vecdestructor|vecdestructortype|vecref|vecrefmut` → `vec`
3. `:520` ends_with `error` or starts_with `result` → `error`
4. `:530-552` collect matches of every module **name** (`MODULES`, `:9-52`, 39 entries, order
   matters) and every module **keyword** (`get_module_keywords`, `:56-462`)
5. `:554` no match → `misc` + warning
6. `:564-572` sort: longest match first; tie → module-NAME beats generic keyword; tie → lower
   `MODULES` index

**The precedent to internalise** — commit `4bd0ca520`, comment now at `module_map.rs:559-563`:

> Sort by: longest keyword first; on equal length a MODULE-NAME match outranks a generic keyword
> (a type containing a module's own name is stronger evidence than a shared word — "FilePath"
> contains the module name "file" AND svg's generic keyword "path", both length 4: `file` must
> win); remaining ties fall to module order (first in MODULES wins).

Regression test at `module_map.rs:831-834`. The fix landed **in `module_map.rs`**, plus a test, plus
the mechanical block move in api.json and a `codegen all` re-run.

**The one escape hatch** is `get_correct_module_with_path` (`module_map.rs:587-651`) line
`:617-623`: if the class's `external` path prefix maps — via `module_from_external_path`
(`:654-709`) — to *the module you already put it in*, autofix returns `None` and leaves it alone. A
*non-confirming* path is explicitly not authoritative (`:610-616`).

**So the rule is: place the class where you want it AND give it an `external` prefix that
`module_from_external_path` maps to that same module.** Existing arms include
`azul_layout::desktop::dialogs::` → `dialog` (`:700`), `azul_layout::file::` and
`azul_layout::desktop::file::` → `file` (`:696-697`), `azul_core::callbacks::` → `callbacks`
(`:662`), `azul_core::db::` → `db` (`:690`), `azul_core::audio::` → `audio`, `azul_core::screencap::`
→ `screen`, `azul_layout::widgets::` → `widgets` (`:678`), `azul_css::*` → `css` (`:656`).

Three further rewrites to know about:

* **`patch::normalize_class_names`** (`doc/src/patch/mod.rs:1822-1889`) renames a class to the last
  `::` segment of its `external` path whenever they differ. Keep them equal.
* **`patch::remove_duplicate_types`** (`patch/mod.rs:551-598`, run by `normalize`) deletes a class
  from every module except the one `determine_module` names, if the same name exists twice.
* **`autofix` removal** (`doc/src/autofix/diff.rs:1263-1266`) emits a *removal* patch for any
  api.json type it cannot resolve in the workspace index. Write the Rust type first.

If a class lands in `misc`, `patch` will **create a `misc` module** (`patch/mod.rs:938-947`).
api.json has no `misc` module today — its appearance in a diff means you got a placement wrong.

### 4.3 Mandatory pre-flight, and the two `module_map.rs` edits this program needs

Before writing a single api.json entry:

1. Decide each class's Rust `external` path and its intended api.json module.
2. Add every new class name to the case list in `module_map.rs`'s `test_get_correct_module` and run
   `cargo test -p azul-doc --release module_map`.
3. Fix `determine_module` / `module_from_external_path` where the test fails — that is a
   deliverable, shipped in the same commit as the api.json addition.
4. **`test_get_correct_module` is already failing on a pristine tree** (a stale `CascadeInfo -> misc`
   expectation, `module_map.rs:871-874`, called out in `4bd0ca520`'s commit message). Fix or update
   that case in Phase 0 so the test becomes a usable gate.

**Predicted verdicts for every new class family** (evaluated against the literal keyword table):

| Class family | `determine_module` verdict | Action |
|---|---|---|
| `RequestId` | `misc` + warning — matches no module name and no keyword, **and `module_from_external_path` has no `azul_core::task::` arm**, so there is no pin either | **EDIT REQUIRED**: add an `azul_core::task::` → `task` arm to `module_from_external_path` (`module_map.rs:654-709`), or add keyword `"requestid"` to `task`'s list (`:359-372`). Prefer the arm. |
| `ResumeCallback` | `dom` — dom's generic keyword `callback` (len 8) matches; the module name `callbacks` (plural) does not substring-match | Pinned by `external: azul_core::callbacks::ResumeCallback` + module `callbacks` (arm `:662`). *(Empirically: 44 `*Callback` classes sit in `dom`; only the 4 with dedicated keywords sit in `callbacks`.)* |
| `ResumeCallbackType` | `callbacks` — keyword `callbacktype` (len 12) wins | OK as-is; pin also applies |
| `*Result` structs, generally | error's keyword `result` (len 6) usually wins → `error` | Pin every one via its `external` prefix (table in §3.2) |
| `FileOpenResult`, `FileOpenMultiResult`, `ColorPickResult`, `SaveTargetResult`, `SaveTarget`, `SaveTargetKind` | `error` / `css` / `misc` | Pinned by `azul_layout::desktop::dialogs::` → `dialog` (`:700`) |
| `FileReadBytesResult`, `FileReadStringResult`, `FileDirListResult`, `FilePathVecSlice` | `error` / `str` | Pinned by `azul_layout::file::` → `file` (`:696`) |
| `HttpGetResult`, `HttpBytesResult`, `HttpReachableResult` | `error` (`result` 6 > `http` 4) | **EDIT REQUIRED**: add an `azul_layout::http::` → `http` arm |
| `ImageDecodeResult`, `VideoDecodeResult` | `image` — image's keyword `decode` (6) ties `result` (6), and `image` precedes `error` in `MODULES` | OK as-is. Optionally add an `azul_layout::image::` arm to make it robust |
| `AudioDeviceListResult` | `error` (`result` 6 > `audio` 5) | Pinned by `azul_core::audio::` → `audio` |
| `ScreenRecordingResult` | `screen` — module-NAME `screen` (6) ties `result` (6) and module-name wins | OK as-is; pin also applies via `azul_core::screencap::` |
| all `Db*Result`, `DbConfig`, `DbScope`, … | `db` for short names; `error` for the `*Result` ones | Pinned by `azul_core::db::` → `db` (`:690`) |
| `FilePathVec`, `…Destructor`, `…DestructorType` | `vec`, **unconditionally** (`:510`, `:602-608`) | Intended. Do not fight it. |
| every `Option…` | `option`, unconditionally (`:505`) | Intended |
| `ResultDbDbError`, `DbError` | `error`, unconditionally (`:520`) | Intended |

**Net: exactly two `module_map.rs` edits** (`azul_core::task::` → `task`, `azul_layout::http::` →
`http`), plus regression-test cases for all ~35 new names, plus the stale-case fix. Land them in
Phase 0.

### 4.4 Mechanics of editing an entry

* An entry lives under `"constructors"` or `"functions"`; both produce the same C name (the
  classifier walks both, `classify.rs:99-119`). Constructors normally omit `"returns"` (implicitly
  `Self`) but may carry one — `image.RawImage.decode_image_bytes_any` is a constructor returning
  `ResultRawImageDecodeImageError`. **P4-1 and P4-2 move from `constructors` to `functions`.**
* **A `"functions"` entry whose body mentions `self.` and whose first `fn_arg` is not `self` is a
  hard error during `codegen all`** (`ir_builder.rs:1580-1593`); without the mention it silently
  becomes a static method. Static factories — including every `downcast` accessor — go under
  `"constructors"`.
* `fn_args` is an ordered array of single-key objects: `[{"self": "ref"}, {"url": "String"}]`.
  `self` takes `"ref" | "refmut" | "value"`.
* `fn_body` is emitted into the generated `cabi_export` body after textual rewrites
  (`doc/src/codegen/v2/transmute_helpers.rs:109+`): `azul_dll::` → `crate::` (`:139-143`), and
  **both `self.` and `object.` → the snake_case class name** (`:147-148`) — which is why existing
  bodies read `object.http_get(url)` *and* `string_vec.len()`. Prefer the snake_case form.
  A **missing** `fn_body` compiles to `unimplemented!()` (`lang_rust.rs:3468-3473`) — a runtime
  panic, not a build error. Always supply one.
* **A `fn_body` naming a function that does not exist fails `cargo build -p azul-dll`, not
  `codegen all`.** Write the `layout/`-crate impl first, then the api.json entry.
* `external` must name a real Rust type whose **last `::` segment equals the class name exactly**.
  A missing `external` silently becomes `crate::{ClassName}` (`lang_rust.rs:3321-3325`). A wrong one
  is transmuted verbatim — best case a compile error, worst case a layout-mismatched transmute
  caught only by `cd dll && cargo test` (memtest asserts `size_of`/`align_of` equality,
  `lang_rust.rs:3884-3928`; types **without** `external` are silently untested).
* Validation phase 0 (`ir_builder.rs:94-486`) **hard-errors** on: array types `[T;N]`, non-FFI-safe
  field types (`Box<`, `Arc<`, `Vec<`, `BTreeMap<`, `NonZeroUsize`, … `:108-149`), wrong `repr`
  (`C` for structs and fieldless enums, `C, u8` for data-carrying enums, `:379-414`), and the
  reserved function names `hash, partialEq, partialCmp, cmp, deepCopy, delete, eq, clone, default,
  debug, display` (`:417-442`). `downcast` is not reserved.
* Class ordering inside a module is irrelevant — phase 9 topologically sorts types by dependency
  (`ir_builder.rs:72`, `:789-792`).

### 4.5 The `web` classification key — a code change, not a JSON key

The boundary plan proposes a per-function `"web": {"class": "js", "hook": "…"}` key so
`classify.rs` can return `WebJsImpl` → `BoundaryJsImport`. **A bare JSON key will not survive.**
`ClassData` (`doc/src/api.rs:1064-1125`) and `FunctionData` (`:1268-1298`) have no
`#[serde(deny_unknown_fields)]` and no flattened catch-all map, and every `normalize`/`patch` run
re-serializes the whole file through `to_json_pretty_4space` (`main.rs:24-31`) — so unknown keys are
**silently dropped and surviving keys are reordered to struct-declaration order**.

**Do this instead, in Phase 0:**

1. Add a field to `FunctionData` in `doc/src/api.rs:1268-1298`:
   ```rust
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub web: Option<WebFnClass>,   // { class: "js"|"server"|"lift", hook: Option<String> }
   ```
2. Confirm no other language target emits it: `codegen all` and diff every file under
   `target/codegen/`. Only `classify.rs` should ever read it (it parses raw JSON, so it needs no
   change beyond looking the key up).
3. Round-trip test: add the key to one existing function, run the full §4.1 sequence, and verify
   `git diff api.json` shows only your addition.

The sidecar `web-classification.json` remains the fallback if step 2 turns up a binding that chokes.
Decide in Phase 0; do not discover in Phase 1.

---

## 5. Runtime pieces — current status and what to build

### 5.1 `AzStartup_completeRequest` — **DOES NOT EXIST**

Verified: `dll/src/web/eventloop.rs` exports 40 `AzStartup_*` functions and none is
`completeRequest`; a repo-wide grep for `completeRequest`/`complete_request`/`tickTimers` inside
`dll/src/web/` returns nothing.

**Signature — keep it to four arguments.** On Windows x64 the 5th argument becomes a guest-stack
argument, and the lift currently mis-handles exactly that: `AzStartup_dispatchEvent`'s
`*out_len_ptr = used` store (its 5th, stack-passed arg) is provably broken — remill spills RBP,
reuses it, and drops the reload before the final store, so `out_len` stays 0. The loader works
around it by recovering the length from the self-describing TLV header
(`dll/src/web/loader_js.rs:905-917`; the hazard is also flagged at
`dll/src/web/transpiler_remill.rs:318`). Do not inherit that bug:

```rust
#[no_mangle]
pub unsafe extern "C" fn AzStartup_completeRequest(
    state: u32,
    request_id: u64,      // status + kind travel in the payload header
    payload_ptr: u32,
    payload_len: u32,
) -> u32                  // returns patches_ptr; JS reads the length from the TLV header
```

Body outline, mirroring `AzStartup_dispatchEvent` (`eventloop.rs:2286-2413`):

1. null-guard `state` (`:2293-2299`);
2. `let s = &mut *(state as usize as *mut EventloopState);` (`:2300`);
3. look up `request_id` in the pending table (§5.2). On miss: log **loudly** and return 0 — never
   silently;
4. decode `payload_ptr[..payload_len]` (flat TLV, the shape JS already speaks —
   `loader_js.rs:929-944`, encoder `AzStartup_buildPatch` at `eventloop.rs:1966`) into the
   operation's `repr(C)` result struct **in lifted Rust**, and wrap it in a `RefAny`;
5. call the stored guest callback (§5.3);
6. fold the returned `Update` and emit a patch buffer the way `dispatchEvent` does at `:2388-2408`
   (lazily allocate `s.patch_buf_ptr` via `AzStartup_alloc`, `:2394-2396`), returning the pointer.
   **Do not call relayout from here** — `dispatchEvent` does not either; relayout is JS-driven
   through the separate `AzStartup_relayout` export (`eventloop.rs:2189`).

**Two things any new `AzStartup_*` export needs — both compile-silent if omitted:**

* an entry in the `eventloop_symbols![...]` macro invocation, `dll/src/web/mod.rs:88-157`
  (the macro at `mod.rs:72-86` generates `EVENTLOOP_SYMBOLS` and `eventloop_symbol_addr`);
* a match arm in `signature_for_eventloop_fn`, `dll/src/web/transpiler_remill.rs:272` (arms
  `:280-451`, `_ => None` at `:452`). Without it the export silently degrades to
  `__az_dep_<addr>` with a generic 3-arg callback signature (`transpiler_remill.rs:5797-5803`) and
  `azMini.AzStartup_completeRequest` is `undefined` in JS.

Loader side (`dll/src/web/loader_js.rs`): add `azCompleteRequest(requestId, status, bytes)` as a
clone of `azDispatch` (`:877`) — `AzStartup_alloc` (`eventloop.rs:421`) for the payload, call the
export, recover the patch length from the TLV header, `azApplyPatches` (`:997`), then
`AzStartup_free`. u64 crosses as BigInt, exactly as the loader already does at `:752`.

**Re-entrancy gate (required, not optional).** A promise can resolve between two synchronous
dispatches. Put a loader-side single-flight FIFO in front of `azCompleteRequest`: if a dispatch is
on the stack, queue the completion and drain after it returns.

### 5.2 The pending-request registry in `EventloopState` — **DOES NOT EXIST**

`EventloopState` is defined at `dll/src/web/eventloop.rs:168-319` and constructed field-by-field in
`AzStartup_init` at `:467-495`. It has 23 fields; **none** is a pending table and **none** is a
timer list. The one stored guest fn-pointer today is `state_deserializer: u64` (`:181`) — your
direct precedent. (`cb_fn_cache: BTreeMap<u32, u64>` at `:185` exists but is never populated,
`:2243-2245`.)

Add:

```rust
struct PendingRequest {
    id: u64,
    kind: u16,             // which operation → which decoder / result struct
    callback_addr: u64,    // the guest fn-ptr taken from ResumeCallback.cb
    data: RefAny,          // cloned at request time; keeps the refcount alive
    once: bool,            // false for subscriptions (PR-17, PR-20)
}
// EventloopState gains: pending: Vec<PendingRequest>, next_request_id: u64
```

Mint ids monotonically from `next_request_id` starting at 1 (0 = `RequestId::invalid`). Extend the
`AzStartup_init` initializer at `:467-495`.

**Architectural constraint** (module doc, `eventloop.rs:35-48`): Rust source-level statics generate
address-materialisation sequences that do not lift. All state must be heap-allocated and threaded
through as the `u32` state pointer. Do not add a static pending table.

Also Phase 0:

* **`AzStartup_tickTimers`** — does not exist. Reuse the desktop tick logic; the loader schedules
  `setTimeout(next_due)` after each call. Required before anything drains from a Timer
  (`WebTransport::recv`, capture-widget `on_frame`). Note the desktop
  `LayoutWindow::tick_timers` (`layout/src/window.rs:2289-2301`) is currently a no-op filter that
  returns **every** timer id and ignores `current_time` — the real deadline math lives in
  `time_until_next_timer_ms` (`window.rs:2311-2330`) and in `Timer::invoke`. Copy the latter two,
  not `tick_timers`.
* **`Instant::now` / `GetSystemTimeCallback` → `performance.now()`** — currently lifts through the
  std path and returns garbage.

### 5.3 Calling the stored callback — **no new bridge is needed**

There are two distinct indirect-call mechanisms; pick the right one.

1. **`__az_call_indirect` (JS table)** — `eventloop.rs:369`, `FnClass::CallIndirect`
   (`symbol_table.rs:153`), lowered to a wasm `call_indirect` over the imported
   `__indirect_function_table` (helper IR at `transpiler_remill.rs:9155-9194`). Requires a JS-owned
   `WebAssembly.Table` index obtained from `__az_resolve_callback` (`eventloop.rs:349`,
   `FnClass::ResolveCallback` at `symbol_table.rs:165`). This is the DOM-attached widget-callback
   path: `AzStartup_dispatchEvent` → `invoke_node_cb` (`eventloop.rs:2268`, called at `:2360`/`:2368`)
   → `__az_resolve_callback` (`:2273`) → `__az_call_indirect` (`:2282`). Signature fixed at
   `(i64, i64, i32) -> i32`, so a 3-argument callback would need a new variant — the precedent
   being `__az_call_indirect_layout4` (`eventloop.rs:397`, `FnClass::CallIndirectLayout4`,
   `symbol_table.rs:162`), added purely to carry one more argument.
2. **`__az_indirect_dispatch` (in-module PC switch)** — the M12.7 dispatcher. Emitter
   `emit_indirect_dispatcher_obj` at `transpiler_remill.rs:2920`, IR at `:2961` (a
   `switch i64 %pcm` over every lifted synth PC, three alias labels per address from
   `dispatcher_csynths` at `:3060`), wired into the link at `:2860-2882` (batched path
   `:3569-3586`), reached from `__remill_function_call` at `:9566` and `__remill_missing_block` at
   `:9588`. This needs **no JS involvement** — it needs the callee's *body* lifted into the same
   bundle.

**Use mechanism 2.** The resume callback is stored in `EventloopState` exactly like
`state_deserializer`, and `AzStartup_hydrateJson` already demonstrates the whole pattern: it reads
the stored fn-pointer at `eventloop.rs:565`, guards zero at `:566-568`, and calls through it via an
ordinary Rust indirect call at `:578`, which the lifter routes through the dispatcher. Its doc at
`:540-543` states the requirement plainly — *"the deserializer's body must be part of this bundle;
the mini lift seeds it as an extra root."*

So `AzStartup_completeRequest` just calls `(callback.cb)(data, info, result)` in Rust. **No new
bridge, no new `FnClass`.** What it *does* require is §5.4.

⚠ The dispatcher's `%unk` default label is a **no-op, not a trap** (`transpiler_remill.rs:2938-2946`)
— an unlifted fn-pointer target is silently dropped. Add a loud trap there as part of Phase 0, or
this class of bug stays invisible.

### 5.4 Fn-pointer-only lift seeding — the gating unknown

A `ResumeCallback` fn-pointer passed **only** to a request function is discovered by neither of the
transpiler's two scans — `scan_guest_branch_targets` (`transpiler_remill.rs:3214`, BL/B byte scan)
nor `scan_guest_code_addr_targets` (`:3222`, code-address materialisation scan) — because the
pointer arrives as a *runtime value*. Its body is then absent from the bundle and the dispatcher's
`%unk` label swallows the call. **This gates every resumable API.**

Two seeding levers exist today:

* **Extra roots into the mini lift.** `fn lift_eventloop_mini_wasm(extra_roots: &[(String, usize, usize)])`
  — `dll/src/web/mod.rs:930` (body `:930-987`; roots merged at `:955-968` with the comment *"Extra
  roots reachable ONLY through a function pointer — the byte-scan walk can never discover them.
  Today: the app's JSON state deserializer."*). The **only** call site is `mod.rs:1077-1085`, which
  passes exactly one root: `app_data.get_deserialize_fn()`.
* **Force-seed by name.** `SymbolTable::find_recursable_by_name(&self, must_contain: &[&str])` —
  `dll/src/web/symbol_table.rs:1023` (doc `:1018-1022`). Consumed at
  `transpiler_remill.rs:3179-3198`, whose hardcoded pattern list (`:3180-3183`) is how
  `GetSystemTimeCallback`'s target gets lifted. Cheap, but name-based — it works for
  framework-owned callbacks (the internal `FileInput` resume) and not for arbitrary user ones.

**Recommendation for Phase 0: extend the extra-roots path.** Scan the app image for
`ResumeCallback`-shaped statics server-side when the lift plan is built and pass them all through
`lift_eventloop_mini_wasm`'s existing parameter. Use the name-based lever as the stopgap for the
framework's own resume callbacks. **Prove this before writing any Phase-1 api.json entry** — if
callbacks cannot be reached, nothing downstream works.

### 5.5 The RefAny reflection bridge — **PRESENT, reuse it**

`AzStartup_hydrateJson` (`eventloop.rs:550-591`), `AzStartup_hydrate` (`:594-662`),
`AzStartup_registerStateDeserializer` (`:675-684`), loader `azHydrateJsonMarkers`
(`loader_js.rs:734`) + setter call (`:752`), extra-root seeding (`mod.rs:1077-1085`). api.json
exposes `callbacks.RefAny.serialize_to_json(self: ref) -> OptionJson`, `set_serialize_fn`,
`set_deserialize_fn`, `can_serialize`, `can_deserialize`, and
`json.Json.deserialize_to_refany(self: value, deserialize_fn: usize) -> ResultRefAnyString`.

Use it for shipping a user `RefAny` into a worker (boundary plan §5.4) and for `unmocked`
diagnostics. **Do not** route ordinary result delivery through JSON — result structs are built in
lifted Rust from a flat TLV payload (§5.1 step 4). JS never hand-builds Rust memory.

### 5.6 The db lifter cut — land it early, independent of everything

Pure classification, no api.json churn, lands with Phase 0:

1. Classify the `AzDb_*` names `classify::FnClass::WebJsImpl` → `BoundaryJsImport` so the
   transitive walk stops at the API boundary and **never descends into turso**.
2. Classify every `turso::` / `turso_core::` module-path symbol `NeverLift` with a **loud trap**
   body — reaching one on web is a design error, and today it silently lifts megabytes of engine
   that no-ops at the fs syscall leaves.

Apply the display-list classifier lesson (fixed 2026-08-17 in `dll/src/web/symbol_table.rs`): match
the module path **with the `::`** (`"turso::"`, never the bare substring — the old
`contains("display_list")` rule caught `set_skip_display_list`, the setter of its own gate), and
exempt `alloc::`/`core::`/`std::` generics that merely mention turso types as type parameters
(`<… as core::ops::…>::… <turso_core::T>` must still lift).

---

## 6. Desktop-side work, per phase

Small by construction: the sync impls already exist and stay; they move behind a request function
that queues `CallbackChange::CompleteRequest`.

### 6.1 Phase 0 — the mechanism

* `layout/src/callbacks.rs`: new `CompleteRequest` variant in the enum at `:167`.
* `dll/src/desktop/shell2/common/event.rs`: new arm in `apply_user_change` (`:1383`), next to
  `AddTimer` (`:1568`). It **queues**, following the `UpdateVirtualView` template (`:1664`).
* A completion pump alongside `invoke_expired_timers` (`event.rs:5424`) and
  `invoke_thread_callbacks` (`:5570`), building a fresh `CallbackInfo` the way
  `LayoutWindow::run_all_threads` does (`layout/src/window.rs:4301-4325`), invoking the callback,
  and draining its own pushed changes (`window.rs:4334-4339`).
* `RequestId`, `ResumeCallback(Type)` Rust types in `azul_core::task` / `azul_core::callbacks`,
  plus the result-struct types in the homes named in §3.2.
* Delete the `Thread::sleep_*` impls (P0-1..3).

### 6.2 Phase 1

| Wrap | Existing sync impl |
|---|---|
| P1-1 | `layout/src/desktop/dialogs.rs:297` — `FileDialog::open_file`, blocking `dialog.open_file()` at `:311` |
| P1-2 | `layout/src/desktop/dialogs.rs:342` — `open_multiple_files`, `dialog.open_files()` at `:357` |
| P1-3 | `layout/src/desktop/dialogs.rs:228` — `ColorInput::open`, `tfd::ColorChooser…run_modal()` at `:235-237` |
| P1-4 | `layout/src/file.rs:921` — `FilePath::read_bytes` |
| P1-5 | `layout/src/file.rs:930` — `FilePath::read_string` (→ `file_read_string`, `file.rs:224`) |
| P1-6 | new: save dialog (or Downloads dir) + `std::fs::write` |
| P1-7 | `image` crate decode + defer (the same code the sync `decode_image_bytes_any` runs) |
| P1-9..15 | **retire** `layout/src/desktop/file.rs` — `impl File` at `:68`, `open` `:77`, `create` `:84`, `read_to_string` `:91`, `read_to_bytes` `:96`, `write_string` `:101`, `write_bytes` `:105`, `close` `:112`. Its own header (`:3-4`) already says `layout/src/file.rs` is the more complete API. Grep for framework-internal users before deleting. |

Note all five dialog entry points are `#[cfg(not(any(target_os = "android", target_os = "ios")))]`
with `None`-returning mobile stubs — preserve that structure in the resumable form (mobile resolves
with an empty result rather than never resolving).

Two more Phase-1 desktop items:

* **`widgets.FileInput` internal rewrite.** `extern "C" fn fileinput_on_click` at
  `layout/src/widgets/file_input.rs:208` **blocks on `tfd::FileDialog::open_file` inside the click
  callback** (`:223`) and then synchronously forwards to the user's `on_path_change` at `:232`
  (defaulting to `Update::RefreshDom` at `:234`). Re-implement on P1-1: the click issues the
  request; the state write (`:226`) and the `on_path_change` invocation move into an internal
  `ResumeCallback`. The widget's seven public api.json functions do not change — this is the proof
  the pattern costs widget users nothing.
* **AzWriter port.** `examples/azul-writer/src/main.rs:34` uses `std::env::temp_dir()` and `:127`
  uses `std::fs::write`. Raw `std::fs`/`std::env` in user code **cannot be intercepted** — it lifts
  into out-of-image syscall leaves that stub to silent no-ops. Port `on_export` onto
  `FileDialog::save_bytes` (P1-6). This is the whole point of Phase 1.

### 6.3 Phase 2

* `layout/src/http.rs` — `ureq` is blocking by design. Wrap `http_get_with_config` (`:431`),
  `download_bytes_with_config` (`:567`) and `is_url_reachable` (`:598`), and defer. Delete the two
  api.json `_default` entry points (P2-4/P2-5); the Rust helpers `http_get` (`:383`) and
  `download_bytes` (`:545`) may stay internal. Every one of these has a
  `#[cfg(not(feature = "http"))]` stub twin — keep both arms in sync.
* Capability probes: extend the desktop probe implementations for the seven new names; rename
  `udp` → `webtransport`; delete `dll/src/desktop/extra/udp/` and `dll/src/unified/udp.rs`.
* Clipboard/DnD are **runtime-only** on desktop — no change.

### 6.4 Phase 3

* `layout/src/desktop/dialogs.rs:323` (`open_directory`, `dialog.select_folder()` at `:330`) and
  `:369` (`save_file`, `dialog.save_file()` at `:376`). Desktop's `save_file` resolves with
  `SaveTarget { kind: Path, path: Some(p), handle_id: 0 }`.
* `layout/src/file.rs:985` — `FilePath::read_dir` (→ `dir_list`, `file.rs:487`).
* Poll backends (`dll/src/desktop/extra/{biometric,keyring,geolocation,...}`) need no desktop
  change; the geolocation subscription manager already has the Subscribe/Release diff-event shape
  the web impl mirrors.

### 6.5 Phase 4

* `dll/src/desktop/extra/video_codec/` — restructure `encode`/`decode` to submit + poll. On Windows
  this is also the opening for a Media Foundation backend (async-shaped), which the current
  VideoToolbox-only code has no room for.
* `screen.ScreenRecorder::finish` — the gstreamer finalize blocks today, so the remodel improves
  desktop too.
* `dll/src/desktop/extra/map.rs` — `dom_with_fetch` keeps its background tile thread, now
  framework-owned instead of user-supplied.

### 6.6 Phase R

Desktop keeps **turso** (pure-Rust SQLite, `dll/src/desktop/extra/sqlite/`) as the hidden local
engine. The KV/collection/index surface maps onto generated SQLite tables; the sync layer speaks the
same row-level oplog protocol as web, over `ureq` instead of `fetch`.

---

## 7. Order of work, with checkpoints

Every checkpoint ends with: workspace `cargo build --release` green, the full §4.1 azul-doc sequence
re-run **and committed alongside the api.json edit**, `cd dll && cargo test` green, and the named
e2e spec passing. Commit semantically at each numbered step.

**Build / serve / test recipe.** Git Bash; node at `C:\Users\felix\tools\node\node.exe` (v20.18.1);
the harness has **zero** npm dependencies (no `package.json`, no `node_modules`; raw-WebSocket CDP
in `scripts/e2e-web/lib/cdp.mjs`, pure-JS PNG diffing in `lib/png.mjs`).

```bash
# 1. build the web-capable dll  (scripts/m9_e2e/cdp_gate.sh:6-14)
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=abort" \
  cargo build -p azul-dll --release --no-default-features \
  --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc
cp -f target/x86_64-pc-windows-msvc/release/azul.{dll,pdb} examples/c/

# 2. serve the app (blocks; wait for "Listening on")
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 \
AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192 ./examples/c/hello-world.exe

# 3. headless browser with CDP
"/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" \
  --headless=new --remote-debugging-port=9222 --disable-gpu --no-first-run \
  --user-data-dir=/tmp/az-edge-headless about:blank &

# 4. run a spec  — NOTE: http://, not web://
AZ_BACKEND=http://127.0.0.1:8800 AZ_E2E=tests/e2e/azwriter_boot.json \
  /c/Users/felix/tools/node/node.exe scripts/e2e-web/run.mjs
```

`scripts/e2e-web/run.mjs` accepts `AZ_BACKEND` **only** if it matches `^https?://` (`run.mjs:92-93`)
— a `web://` value is silently ignored and it falls back to `http://127.0.0.1:8800`. Specs live in
`tests/e2e/` (7 JSON files) and `scripts/e2e-web/specs/smoke.json`. `scripts/m9_e2e/cdp_gate.sh`
automates 1-3; `scripts/m9_e2e/azwriter_web.sh` does the same for AzWriter on port 8801. Any azul
binary can also *be* the runner: `dll/build.rs:868-912` bundles the `.mjs` files into the dll and
`dll/src/e2e_web_runner.rs:101-134` spawns node on them when `AZ_E2E` is set and `AZ_BACKEND` is
`http(s)://` — in that mode pass `--golden-dir`/`--out-dir` explicitly, since `specs/` and `golden/`
are not bundled. No CI job runs the web harness today (`.github/workflows/rust.yml:1554` runs only
the desktop `AZ_BACKEND=headless` lane on `hello_world_counter.json`).

### Checkpoint 0 — the primitive works end to end

Steps: §5.4 seeding decision **and proof** → loud trap on the dispatcher's `%unk` label → pending
table (§5.2) → `AzStartup_completeRequest` + `eventloop_symbols!` entry +
`signature_for_eventloop_fn` arm + `azCompleteRequest` + the single-flight gate (§5.1) →
`AzStartup_tickTimers` + the `Instant::now` boundary → `CallbackChange::CompleteRequest` + the
desktop completion pump (§6.1) → `task.RequestId` + `ResumeCallback(Type)` (§3.1) → the two
`module_map.rs` arms + regression cases + the stale `CascadeInfo` fix (§4.3) → the `web` field on
`FunctionData` and its round-trip proof (§4.5) → the db lifter cut (§5.6).

**Proof:** new spec `tests/e2e/resume_primitive.json` — a button click issues a synthetic request
against a debug-only echo boundary, the resume callback mutates state, and the assertion is
`assert_text` on the resulting DOM change (the shape of `tests/e2e/hello_world_counter.json`, the
proven-passing gate). Plus: a lifted timer callback fires and patches the DOM. Desktop parity: the
same spec under `AZ_BACKEND=headless`.

### Checkpoint 1 — AzWriter ships

Steps: §2.2 api.json entries (after the §4.3 pre-flight) → desktop wrappers (§6.2) → JS boundary
impls for open-file / save-bytes / read-bytes / read-string / decode-image → picked-file registry +
virtual `/az/picked/<id>/…` paths → `FileInput` rewrite → AzWriter port → `MsgBox` on
`alert`/`confirm`.

**Implement the §6.5 mock protocol in this checkpoint, before the specs that depend on it**
(`scripts/web-e2e-harness-plan.md:591`, `:598-622`): a page global `window.__az_e2e_mock` (absent in
production) that the JS boundary impls consult **first** — a mocked `open_file` resolves immediately
with a predefined path, a mocked read serves canned bytes, and an **unmocked** request in an e2e
context fails loudly with `status: "unmocked"` and never falls through to a real picker (a hanging
modal is the worst e2e outcome). The resume path is the normal one. Harness side: a
`{"op": "mock", "set": {…}}` step merging into the global via CDP `Runtime.evaluate`; today that op
falls to `scripts/e2e-web/lib/driver.mjs:378` and is reported as `skip`. Desktop lane: map the same
op onto the existing `AZ_E2E_TEST` deterministic mechanism (which covers biometric
`dll/src/desktop/extra/biometric/mod.rs:56` and keyring `…/keyring/mod.rs:47` only).

**Proof:** `tests/e2e/azwriter_boot.json` still passes; new `tests/e2e/azwriter_export.json`
asserts a real PDF download; new `tests/e2e/fileinput_roundtrip.json` asserts a picked file's bytes
reach the DOM. Also fix the pre-existing bug in `tests/e2e/azwriter_boot.json:15-17`, where the
screenshot step writes `"name": "azwriter_boot"` instead of the required `"reference"` key — the web
harness silently falls back to a step-index golden filename that collides across specs
(`lib/asserts.mjs:196`) and the desktop runner errors
(`dll/src/desktop/shell2/common/debug_server/full.rs:3844-3846`).

### Checkpoint 2 — HTTP + honesty

Steps: §2.3 entries → `layout/src/http.rs` wrappers → fetch boundary impl → clipboard/DnD event
plumbing → the eight capability probes → delete the dead udp modules.

**Proof:** `tests/e2e/http_get_resume.json` (a mocked fetch resolves into a DOM text change; a
second test asserts an unmocked cross-origin URL reports an `HttpError` rather than hanging) and a
capability spec asserting every probe returns a truthful `{available, backend, reason}` on web.

### Checkpoint 3 — pickers II + poll backends

Steps: §2.4 entries → `SaveTarget` + the Chromium/fallback split → `read_dir` → the picked-directory
registry feeding the fs index mirror → geolocation/sensors/gamepad/biometric/keyring runtime wiring.

**Proof:** `tests/e2e/save_target.json` (mocked `showSaveFilePicker` **and** the download fallback
both resolve; `as_path` is `None` on the fallback) and `tests/e2e/read_dir_resume.json`.

### Checkpoint 4 — media

Steps: §2.5 entries → JS-owned `getUserMedia`/`getDisplayMedia`/MediaRecorder/WebCodecs resources →
desktop submit+poll codec restructure → framework-owned map tile fetcher.

**Proof:** `tests/e2e/capture_widget_frames.json` (a mocked media stream drives `on_frame` through
the timer pump) and `tests/e2e/video_submit_poll.json`.

### Checkpoint R — db (parallel; starts after Checkpoint 2, ships with Checkpoint 4)

Steps: §2.6 entries and §3.7 classes → IndexedDB JS backend → desktop turso mapping → the row-level
oplog sync protocol → conflict machinery.

**Proof:** `tests/e2e/db_local_first.json` — set/get round-trip offline, `sync_status` reports
`Queued`, and a mocked sync endpoint drains the oplog and reports `Idle`.

### Checkpoint 5 — transport + escape hatches

WebTransport JS engine (zero api.json churn), iroh design (relay-only in browsers; no API invented
before the engine exists), worker+SAB evaluation. **JSPI is dropped** — with no sync forms left to
rescue it has no user-visible role.

---

## 8. Risks and unknowns — with the cheapest experiment that settles each

| # | Risk | Why it matters | Cheapest experiment |
|---|---|---|---|
| R1 | **Fn-pointer-only callbacks are never lifted** (§5.4). Neither transpiler scan can see a pointer that arrives as a runtime value, the mini lift takes exactly one extra root today (`mod.rs:1077-1085`), and the dispatcher's `%unk` default is a **no-op, not a trap** (`transpiler_remill.rs:2938-2946`) — so the resume silently never fires. | Gates the entire program. | Before any api.json work: a debug export that stores a fn-ptr handed in from a click callback and calls it one tick later through the dispatcher. Land the loud `%unk` trap first so the failure is visible. If it does not resolve, extra-roots seeding is mandatory and lands before Phase 1. One afternoon. |
| R2 | **`repr(C)` result structs cross the boundary wrong.** Nested `Vec`/`String` inside `ResultHttpResponseHttpError` etc. are built in lifted Rust from a flat TLV payload; a layout mismatch is silent corruption, not a crash. `cd dll && cargo test` memtest only checks the top-level size/align. | Every non-trivial result (HTTP, dir listing, decoded image) rides this. | Round-trip **one** struct first: `HttpReachableResult { reachable: bool, error: OptionString }` — one bool, one string. Assert both fields with `assert_text`. Only then attempt `ResultHttpResponseHttpError`. |
| R3 | **The autofixer moves, renames, or deletes new classes** (§4.2). `FilePath` already went to `svg` for exactly this reason; `RequestId` has **no rescue path at all** today and would drag a brand-new `misc` module into api.json. CI fails on any diff the autofix sequence produces (`rust.yml:97-110`). | Costs a full regen cycle per class, and a wrong `external` can produce a layout-mismatched transmute. | Add all ~35 new class names to `test_get_correct_module` **in one commit, before writing any api.json entry**, and run `cargo test -p azul-doc --release module_map`. Land the two `module_from_external_path` arms (`azul_core::task::`, `azul_layout::http::`) in the same commit, plus the stale `CascadeInfo` fix. Minutes. |
| R4 | **`completeRequest` re-entrancy.** A promise can resolve between two synchronous dispatches; two activations then mutate `EventloopState` and the bump allocator interleaved. Lifted Rust assumes `&mut` uniqueness — silent corruption, the same hazard that sank JSPI. | Corruption, not a clean failure. | Loader-side single-flight FIFO from day one (§5.1), plus a spec that fires a click while a mocked slow request is outstanding and asserts both complete in order. |
| R5 | **Bump-allocator pressure.** The web allocator never frees (`FnClass::BumpDealloc`, `symbol_table.rs:149`, is a documented no-op). Large downloads and decoded images allocate per request and leak by construction. | An app that fetches in a loop dies. | A spec that issues 200 mocked 1 MiB downloads while reading the bump pointer via `AzStartup_peekU32` (`eventloop.rs:2008`). Linear growth to exhaustion ⇒ a per-request arena reset is a Phase-2 prerequisite, not a Phase-5 nicety. |
| R6 | **The `web` classification key cannot be a bare JSON key** (§4.5) — `FunctionData` has no unknown-field capture and `normalize`/`patch` re-serialize the file, so it is silently dropped. Without it, `classify_fn` (`classify.rs:144-151`) leaves every new request function as `Framework` and the JS-impl mechanism has no trigger. | Blocks classifying anything as `BoundaryJsImport`. | Not an experiment — a work item: add `web: Option<WebFnClass>` to `doc/src/api.rs:1268-1298`, run `codegen all`, and diff every file under `target/codegen/` to confirm no other binding emits it. Half a day, a Phase-0 gate. Sidecar `web-classification.json` is the fallback. |
| R7 | **Cancellation and RefAny lifetime.** The pending table clones the user `RefAny` and holds it until completion. Window close, repeated requests, and subscriptions (PR-17/PR-20, which never complete) all leak entries. `CallbackInfo::cancel_request(RequestId)` is deliberately **out of scope for v1**. | A long-running app grows a pending table forever. | Decide the GC policy in Phase 0 and encode it in `PendingRequest`: drop everything on window close; subscription entries are owned by the object that created them (`Db`, capture widget) and dropped with it. Assert with a spec that opens and closes 100 subscriptions and checks `pending.len()` via a debug probe. |
| R8 | **The e2e mock protocol does not exist.** §6.5 is committed as documentation only — no `__az_e2e_mock` anywhere in `scripts/e2e-web/`, and `{"op":"mock"}` currently reports `skip` (`driver.mjs:378`), i.e. **a spec that should fail passes**. `scripts/e2e-web/golden/` does not exist either, so the first screenshot run auto-creates baselines and passes. | Every checkpoint from 1 onward claims proof from specs that would be vacuous. | Implement the mock op in Checkpoint 1 *before* writing the specs that depend on it; make an unmocked request fail loudly (`status: "unmocked"`); fix the `"name"` vs `"reference"` bug in `tests/e2e/azwriter_boot.json:15-17`; commit golden baselines explicitly rather than letting a first run mint them. |
| R9 | **File System Access is Chromium-only, permanently.** Firefox and Safari hold negative standards positions on `showOpenFilePicker`/`showDirectoryPicker`/`showSaveFilePicker`; this will not become Baseline. | `SaveTarget`, `open_directory`, and any "re-save to the same file" story are tier-1-Chromium features. | Nothing to experiment on — it is a fact. Design `SaveTarget` with the `<a download>` fallback as the *default* path, not the exception, and make `as_path() -> None` a documented, tested case (Checkpoint 3 spec). |
| R10 | **User gestures.** Pickers, clipboard reads, WebAuthn and iOS motion permissions all require transient activation; a request issued from a timer gets `NotAllowedError`. | A resumable API is *designed* to be callable from anywhere — exactly what the browser forbids. | Settle boundary-plan open question 7 in Phase 1: this work order adds `FileErrorKind::NeedsUserGesture` (§3.5) and requires every gesture-gated request function's doc string to say so. Verify with a spec that calls `open_file` from a timer and asserts the error kind. |
| R11 | **CORS.** `http_get(arbitrary_url)` cannot work in a browser unless the target sends `Access-Control-Allow-Origin`, and there is no permission to escape it. Chrome 142+ additionally prompts for loopback/LAN targets. | Desktop parity for "fetch any URL" is unachievable; a proxy story is required. | No experiment — decide and document. v1: `HttpError::Other` with a message naming CORS, and "server-side proxy" recorded as an explicit non-goal with a named follow-up. |
| R12 | **Turso must not leak into the web lift**, even before the db API is redesigned. Today an app that touches `Db` lifts megabytes of engine that no-ops at the fs leaves. | Silent wrong answers and enormous lift times for any db-touching app. | Land §5.6 in Phase 0 (pure classification, no api.json churn) and verify by lifting an app that calls `Db::open` and grepping the shard manifest for `turso`. Match `"turso::"` **with** the `::`, per the display-list lesson. |
| R13 | **New `AzStartup_*` exports fail silently.** Omitting the `eventloop_symbols!` entry (`mod.rs:88-157`) or the `signature_for_eventloop_fn` arm (`transpiler_remill.rs:272`) degrades the export to `__az_dep_<addr>` with a wrong signature — no compile error, and `azMini.AzStartup_completeRequest` is simply `undefined`. Separately, a 5th (stack-passed) argument inherits the known-broken `out_len_ptr` store (`loader_js.rs:905-917`). | Silent, and easy to misdiagnose as a lift bug. | Keep `completeRequest` to four arguments (§5.1) and add a Checkpoint-0 assertion that `typeof azMini.AzStartup_completeRequest === 'function'` before any resume test runs. |

---

## 9. Do NOT

* **No hybrid sync variants.** Do not keep a sync `open_file` "for desktop", do not add a `_with`
  twin, do not add a `#[cfg]`-selected second entry point. One signature, both targets. If a caller
  breaks, that is the intended outcome.
* **No silent no-op stubs on web.** Every unimplementable path fails **loudly**: a documented
  `Err`/`None` **plus** a `PlatformCapability` probe that says so, plus one `console.warn`. A
  boundary that returns 0 and moves on is the exact bug this program exists to eliminate.
  `AzStartup_completeRequest` on an unknown `request_id` logs an error; it does not return quietly.
  The dispatcher's `%unk` label gets a trap, not a no-op.
* **No name or date attributions in source comments.** No "added by X", no "2026-08-19:", no
  "per maintainer". Comments explain the code. (This work order is a doc; source is not.)
* **Always `--release`.** Every `cargo build`, `cargo test`, and `cargo run` in this workspace takes
  `--release` (`-r` for azul-doc). Debug builds of the lifter are unusably slow.
* **Never push to `master`.** Work on `weblift/x86-lifter-fixes` (or a branch off it); PR #431 is the
  draft target.
* **Commit semantically.** One concern per commit, conventional-commit subject
  (`feat(api): resumable FileDialog::open_file`, `fix(doc/autofix): route SaveTarget to dialog`).
  An api.json edit, the autofix/patch/normalize output it implies, and the `codegen all` output all
  belong in the **same** commit — a tree where api.json and `target/codegen/` disagree is broken,
  and CI fails on any autofix diff (`rust.yml:97-110`).
* **Do not hand-edit `target/codegen/*`.** Regenerate.
* **Do not hand-move a class between api.json modules.** Fix `doc/src/autofix/module_map.rs` and let
  the pipeline move it (§4.2).
* **Do not add an unknown key to api.json** expecting it to survive — it will not (§4.5).
* **Do not add a static to `dll/src/web/eventloop.rs`.** Source-level statics do not lift
  (`eventloop.rs:35-48`); state lives on the heap behind the `u32` state pointer.
* **Do not build JSPI or worker+SAB.** Both are recorded as evaluated-and-rejected for the API
  surface. Keep new JS boundary implementations DOM-independent so a worker mode *could* be added
  later without touching the API — that is the only obligation they impose.
* **Do not invent APIs for engines that do not exist yet** (iroh, notifications, share sheets). When
  they arrive they must be born resumable.

# Web Platform API Research — OS-facing capabilities for a desktop+browser GUI framework

Research snapshot: **2026-08-18**. Current stable browsers at time of writing: Chrome/Edge ~152, Firefox ~153,
Safari 26.6 (WebKit ships point features in 26.x updates: [26.4 blog](https://webkit.org/blog/17862/webkit-features-for-safari-26-4/),
[26.6 blog](https://webkit.org/blog/18178/webkit-features-for-safari-26-6/)).

Scope: every capability the framework's OS-API surface must map to when the app runs lifted-to-wasm in a
browser. All browser calls happen in JS glue; results re-enter wasm asynchronously, so **async APIs are fine**.
What matters per capability: entry points, permission/gesture/secure-context gates, data shapes (for C-ABI
design), support status, footguns.

**Cross-cutting rules that shape the whole API surface:**

- Nearly everything below requires a **secure context** (HTTPS or localhost). Treat "insecure context" as
  `unsupported`, not `denied`.
- Many APIs require **transient activation** (a user gesture: click/keydown, valid for ~a few seconds, some
  APIs consume it). A desktop-style API that fires on a timer will get `NotAllowedError` in the browser.
  The framework's error vocabulary needs at minimum: `Ok / Denied / Unsupported / NeedsUserGesture /
  InsecureContext / NotInstalled(PWA)`.
- **Permissions API**: `navigator.permissions.query({name})` → `PermissionStatus {state: 'granted'|'denied'|'prompt', onchange}`.
  Support: Chrome 43+, Firefox 46+, Safari 16+. Firefox recognizes only a subset of names (geolocation,
  notifications, push, midi, persistent-storage — **not** clipboard/camera/microphone names); Safari 16+
  recognizes geolocation, camera, microphone, notifications, midi, persistent-storage, push, screen-wake-lock,
  storage-access. Querying an unrecognized name **throws TypeError** — wrap every query in try/catch and map
  to `unknown`. ([MDN Permissions.query](https://developer.mozilla.org/en-US/docs/Web/API/Permissions/query),
  BCD `api/Permissions.json`)
- `'denied'` is often permanent until the user flips a site setting — there is no API to re-prompt. Surface
  this distinctly from a dismissible prompt.

---

## 1. Local directory access — File System Access API

The "vscode.dev capability": user picks a real directory once, the app gets a persistent, revocable
capability handle to that subtree (read or read/write), can enumerate/watch/rewrite files inside it. This is
**the** API for a code-editor / asset-pipeline style app.

### Entry points

- `window.showDirectoryPicker({id?, mode: 'read'|'readwrite', startIn?})` → `Promise<FileSystemDirectoryHandle>`.
  Requires transient activation. Throws `AbortError` on cancel, `SecurityError` in iframes without
  permission-policy. `id` (string ≤32 chars) lets the browser remember a per-purpose last directory; `startIn`
  accepts a well-known name (`'documents'`, `'desktop'`, `'downloads'`, `'home'`, `'music'`, `'pictures'`, `'videos'`)
  or another handle.
- `window.showOpenFilePicker({multiple?, types?, excludeAcceptAllOption?, id?, startIn?})` → `Promise<FileSystemFileHandle[]>`.
- `FileSystemDirectoryHandle`: async iteration via `entries()` / `keys()` / `values()` (async iterators of
  `[name, handle]`), `getFileHandle(name, {create?})`, `getDirectoryHandle(name, {create?})`,
  `removeEntry(name, {recursive?})`, `resolve(descendantHandle)` → relative path array or null.
  Recursion = walk `values()` and recurse on `kind === 'directory'`. There is **no watch API** (no inotify
  equivalent — the `FileSystemObserver` proposal is Chromium-experimental, OPFS-focused; poll mtimes instead).
- `FileSystemFileHandle`: `getFile()` → `File` (has `name`, `size`, `lastModified`, `arrayBuffer()`, `stream()`,
  `slice()`), `createWritable({keepExistingData?})` → `FileSystemWritableFileStream` (`write(BufferSource|Blob|string|{type:'write'|'seek'|'truncate',...})`,
  `seek`, `truncate`, `close`). Writes go to a temp file and appear **atomically on close()**.
- Permissions on any handle: `handle.queryPermission({mode:'read'|'readwrite'})` and
  `handle.requestPermission({mode})` → `'granted'|'denied'|'prompt'`. `requestPermission` needs transient
  activation.
- **Persistence across sessions**: handles are structured-cloneable → store them in **IndexedDB**. On next
  visit `queryPermission()` usually returns `'prompt'`; call `requestPermission()` from a user gesture to
  re-arm. Since **Chrome 122** the re-prompt offers "Allow this time / **Allow on every visit**"; with the
  persistent grant (always for installed PWAs, opt-in otherwise) restored handles come back `'granted'` with
  no prompt. ([Chrome blog: persistent permissions](https://developer.chrome.com/blog/persistent-permissions-for-the-file-system-access-api))
- **Drag & drop**: `DataTransferItem.getAsFileSystemHandle()` → `Promise<FileSystemFileHandle|FileSystemDirectoryHandle>`
  (call synchronously inside the `drop` event, before any await, or the DataTransfer is neutered). Gives full
  read/readwrite-requestable handles, including whole dropped directories. Chromium 86+ **only** (BCD:
  Firefox/Safari none).
- Also: `launchQueue` / File Handling (installed PWA opens files from OS shell) — Chromium-only, niche.

### Gates

Secure context; top-level or permission-policy-delegated frame; picker + `requestPermission` need user
gesture; per-handle permission model (`read` default, `readwrite` triggers a second prompt); browser blocks
sensitive directories (system dirs, the whole home dir root, etc. → `AbortError`-like failures).

### Support (the hard truth)

| Browser | Status |
|---|---|
| Chrome/Edge desktop | **86+** full (pickers, handles, DnD handles). Persistent permissions Chrome 122+. |
| Chrome Android | **132+** (Jan 2025) pickers via Android document picker |
| Firefox | **Not supported, negative standards position** — no pickers, no `getAsFileSystemHandle` |
| Safari (mac+iOS) | **Not supported, opposed on security grounds** — no pickers |

Baseline: "limited availability; negative position from at least one vendor — very unlikely to ever become
Baseline in this form" ([web-features explorer: file-system-access](https://web-platform-dx.github.io/web-features-explorer/features/file-system-access/)).
Confirmed unchanged as of Aug 2026 ([MDN File System API](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API),
[Chrome docs](https://developer.chrome.com/docs/capabilities/web-apis/file-system-access),
[spec](https://wicg.github.io/file-system-access/)). **Design the framework API as
Chromium-tier-1 with a mandatory degraded mode.**

### Fallback story where unsupported (Firefox/Safari)

- **Read-only directory import**: `<input type="file" webkitdirectory>` — user picks a directory, you get a
  flat `FileList` of every file within, each `File` carrying `webkitRelativePath` ("dir/sub/file.txt"). One-shot
  snapshot copy: no writes, no rescan, no persistence across sessions, whole tree is read eagerly (slow on
  huge trees; browsers may warn on large uploads). Support: Chrome 30+, Firefox 50+, Safari 11.1+,
  iOS Safari 18.4+, Chrome Android 151+ ([caniuse input-file-directory](https://caniuse.com/input-file-directory)).
- **Read-only DnD of a directory**: `DataTransferItem.webkitGetAsEntry()` → `FileSystemDirectoryEntry` with
  callback-based `createReader().readEntries()` recursion. Works Chrome 13+, Firefox 50+, Safari 11.1+.
  Same limits: snapshot, read-only, no persistence.
- **Writeback fallback**: none. Save = per-file download (§3) or OPFS-side copy (§2). vscode.dev's own
  Firefox/Safari story is exactly this shape.

**C-ABI sketch**: opaque `DirHandleId` (u32 into a JS-side handle table); ops = pick(mode, start_in) →
Result<DirHandleId>; list(dir, cursor) → batches of {name_utf8, kind, size, mtime_ms}; read(fileid) → bytes;
write(fileid, bytes) (unsupported ⇒ `ReadOnlyBackend`); persist(dirid, key_utf8) / restore(key) →
granted|prompt-needed|gone; capability flags per backend {writable, persistent, watchable=false}.

### Footguns

Handles die with the handle table — only IndexedDB-stored handles survive reload. `requestPermission`
outside a gesture rejects. Reading a `File` after on-disk modification throws `NotReadableError`
(stale snapshot) — re-`getFile()` each time. Moving/renaming the underlying dir orphans the handle
(`NotFoundError`). Brave ships Chromium but disables the API by default. Electron lacks the Chrome 122
persistent-permission UI.

---

## 2. OPFS — origin private file system

Private, origin-scoped, user-invisible file tree. The right target for caches, databases, unpacked assets —
**the app's `$APPDATA`**, not the user's documents.

### Entry points

- `navigator.storage.getDirectory()` → `Promise<FileSystemDirectoryHandle>` (root). Same handle/iteration
  API as §1, minus permissions (always granted, no prompts, no gesture).
- Fast path (worker-only): `FileSystemFileHandle.createSyncAccessHandle()` → `FileSystemSyncAccessHandle`
  with **synchronous** `read(buffer, {at})`, `write(buffer, {at})`, `truncate`, `getSize`, `flush`, `close`.
  Exclusive lock per file; not available on the main thread — ideal shape for wasm libc-style FS shims
  (this is how SQLite-wasm works).
- Main-thread path: `getFile()` / `createWritable()` (async, atomic-on-close) — note Safari only allows
  `createWritable` from 26.0-ish; use sync handles in a worker as the portable write path.
- Quota: `navigator.storage.estimate()` → `{usage, quota}` (estimates). Persistence:
  `navigator.storage.persist()` → bool (exempts origin from eviction; Firefox prompts, Chrome auto-decides on
  engagement, Safari auto).

### Quotas & eviction ([MDN quotas page](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria))

- Chrome/Edge: up to **60% of total disk per origin**.
- Firefox: best-effort = min(10% of disk, **10 GiB** per site group); persisted = 50% of disk (cap 8 TiB).
- Safari: ~**60% of disk per origin** in-browser (~15% inside WebViews; home-screen/dock web apps get the
  60% tier). Safari proactively evicts script storage of origins **not interacted with for 7 days**
  (ITP) unless installed — the classic "my OPFS vanished" footgun.
- Eviction is LRU, all-or-nothing per origin, skips `persist()`ed origins.

### Support

**Baseline "widely available" since 2025-09-27**: Chrome/Edge 108+ (basic getDirectory since 86, sync
handles 102+), Firefox 111+, Safari 16.4+ (initial OPFS 15.2 had async-flavored sync-handle methods —
treat 16.4 as the floor). All incl. Android/iOS.
([web-features explorer: origin-private-file-system](https://web-platform-dx.github.io/web-features-explorer/features/origin-private-file-system/),
[web.dev OPFS](https://web.dev/articles/origin-private-file-system), [MDN FileSystemSyncAccessHandle](https://developer.mozilla.org/en-US/docs/Web/API/FileSystemSyncAccessHandle))

**Verdict: portable now.** This can back a full synchronous `read/write/seek/stat` C-ABI as long as the wasm
runs in a worker. Footguns: sync handles are exclusive (second open → `NoModificationAllowedError`); no
cross-directory atomic rename until `move()` is universal (Chromium has `FileSystemHandle.move()` for OPFS,
others spotty — emulate with copy+delete); Safari 7-day eviction; quota exceeded surfaces as
`QuotaExceededError` mid-write.

---

## 3. File save

### Entry points

- **Picker path (Chromium)**: `window.showSaveFilePicker({suggestedName, types:[{description, accept:{mime:[ext]}}], id, startIn})`
  → `FileSystemFileHandle` → `createWritable()` → stream writes → `close()` (atomic). Requires gesture.
  Supports true streaming (write a multi-GB file without materializing it). Re-save silently to the same
  handle later = the desktop "Save" vs "Save As" distinction — the handle is the document identity.
- **Universal fallback**: `<a download="name.ext" href=URL.createObjectURL(blob)>` + synthetic click, then
  `revokeObjectURL`. Works everywhere (Safari 10.1+, all modern). Data shape: full content as one in-memory
  `Blob` — no streaming, no overwrite-in-place, lands in Downloads, browser may mangle names/duplicate.
  Cross-origin `<a download>` is ignored (same-origin/blob/data URLs only). Must be user-gesture-adjacent or
  popup blockers may eat it.
- Streaming fallback trick: a service worker that turns a client-fed stream into a download response
  (the StreamSaver.js pattern) — works cross-browser but is a hack with SW lifetime pitfalls.

### Support

`showSaveFilePicker`: Chrome/Edge 86+, Chrome Android 132+; **Firefox no, Safari no** (same negative
positions as §1; confirmed current — [MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/showSaveFilePicker)).
`<a download>` + Blob: universal.

**Verdict: fallback-needed but the fallback is fine** for whole-file saves. C-ABI: `save_file(name, mime,
bytes | stream_source) → Saved{handle_id?} | Cancelled | Denied`; expose `can_resave(handle_id)` so apps can
show "Save" only on Chromium.

---

## 4. Notifications & badging

### Entry points

- Permission: `Notification.requestPermission()` → `'granted'|'denied'|'default'`. **Firefox (72+) and Safari
  require transient activation** for the request; Chrome allows non-gesture requests but punishes them with
  quiet UI. State query: `Notification.permission` (sync) or permissions.query `{name:'notifications'}`.
- Page-owned: `new Notification(title, {body, icon, tag, silent, data, ...})` + `onclick`.
  **Throws `TypeError` on Chrome Android** (BCD: partial since 42 — Android mandates the SW path);
  iOS Safari throws `ReferenceError` unless installed. So the constructor is desktop-only in practice.
- SW-owned (portable path): `registration.showNotification(title, options)` from a registered service
  worker; click handling via `notificationclick` in the SW; enumerate via `registration.getNotifications({tag})`.
  Baseline across engines since ~March 2023 ([MDN showNotification](https://developer.mozilla.org/en-US/docs/Web/API/ServiceWorkerRegistration/showNotification)).
- **Action buttons** (`options.actions: [{action, title, icon}]`, cap = `Notification.maxActions`, typically 2):
  **only** on SW notifications. Support: Chrome 53+, **Firefox 152+ (new, 2026)**, **Safari: none** (BCD
  `api/Notification.json`). Replying via `action` id in `notificationclick.action`.
- iOS Safari 16.4+: notifications **only for home-screen-installed web apps**, and only via Push API + SW.
- **Badging**: `navigator.setAppBadge(count?)` / `clearAppBadge()` (also on SW registration). No permission
  prompt (iOS ties it to notification permission). Only visible when the app is **installed** (taskbar/dock/
  home-screen icon). Support: Chrome/Edge 81+ desktop (not Android), Safari iOS 16.4+ (home-screen app),
  Safari 17+ macOS (dock web app), **Firefox: no** ([web-features: badging](https://web-platform-dx.github.io/web-features-explorer/features/badging/)).

**Data shapes**: notification = {id/tag utf8, title, body, icon(url or blob), actions[{id,title}], data(json)};
events re-entering wasm: {tag, action_id|null, clicked|closed}. Badge = u32 or clear.

**Verdict**: basic notify = portable now (via SW path + gesture-gated permission). Actions =
Chrome/Firefox-only (Safari: fire plain notification, treat click as default action). Badge =
Chromium desktop + Safari installed; feature-flag it. Footguns: permission request without gesture silently
`'default'` in Firefox; `tag` reuse replaces (dedup) — good for progress patterns; notifications from a
non-installed iOS web page: API absent entirely (`Notification` undefined — feature-detect, don't sniff).

---

## 5. Biometrics / auth — WebAuthn & passkeys

There is **no bare "prompt for fingerprint" API on the web**. The only biometric primitive is WebAuthn
user verification: OS biometric/PIN gate wrapped around a credential ceremony. A `request_biometric_auth()`
framework call must map to *verify-user-with-platform-authenticator against a previously registered
credential*, and needs a server (or at least stored challenge/verification logic) to be meaningful.

### Entry points

- Register: `navigator.credentials.create({publicKey: {challenge: BufferSource, rp:{id,name}, user:{id: BufferSource ≤64B, name, displayName}, pubKeyCredParams:[{type:'public-key', alg:-7|-257}], authenticatorSelection:{authenticatorAttachment:'platform', userVerification:'required', residentKey:'required'}, timeout, extensions}})`
  → `PublicKeyCredential {rawId, response.{clientDataJSON, attestationObject}, getClientExtensionResults()}`.
- Authenticate (the "biometric prompt"): `navigator.credentials.get({publicKey:{challenge, rpId, allowCredentials?, userVerification:'required'}, mediation?})`
  → `{rawId, response.{authenticatorData, signature, clientDataJSON, userHandle}}`. UV flag in
  authenticatorData bit tells you biometric/PIN actually happened.
- Capability probes: `PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()` → bool
  (Chrome 67+, Firefox 60+, Safari 13+); `isConditionalMediationAvailable()` (Chrome 108+, Firefox 119+,
  Safari 16+); **`PublicKeyCredential.getClientCapabilities()`** → map incl. `userVerifyingPlatformAuthenticator`,
  `conditionalGet`, `extension:prf` — Chrome 133+, Firefox 135+, Safari 17.4+, Baseline since Feb 2025
  ([web.dev](https://web.dev/articles/webauthn-client-capabilities), [MDN](https://developer.mozilla.org/en-US/docs/Web/API/PublicKeyCredential/getClientCapabilities_static)).
- Secret derivation (client-side crypto keyed to biometric unlock — closest thing to "biometric-gated local
  secret"): **`prf` extension** (`extensions: {prf: {eval: {first: BufferSource}}}` → 32-byte output).
  Support is authenticator+OS dependent: iCloud Keychain (macOS 15+) via Safari 18+/Chrome 132+/Firefox 139+;
  Windows Hello via Chrome 147+/Firefox 147-148+; Safari 26.4 added CTAP PRF for security keys
  ([corbado PRF matrix](https://www.corbado.com/blog/passkeys-prf-webauthn), [WebKit 26.4](https://webkit.org/blog/17862/webkit-features-for-safari-26-4/)).
  `largeBlob` is spottier — prefer `prf`.

### Gates

Secure context; `rp.id` must be the origin's registrable domain (framework cannot offer cross-site identity);
`get()`/`create()` require transient activation in practice (Chrome enforces for cross-origin iframes;
Safari consumes the activation); one ceremony at a time (`AbortError` via `AbortController` supported).
Cancel/no-credential = `NotAllowedError` after a deliberate delay (indistinguishable from user cancel — by
design, anti-probing).

### Support

WebAuthn core is Baseline-old: Chrome 67+, Firefox 60+, Safari 13+; passkeys (discoverable + sync) mature on
all three since ~2023. **Verdict: portable now**, including presence checks. Footguns: challenges must come
from unpredictable server-side randomness; `userHandle` only returned for discoverable credentials; UV can
silently downgrade to device PIN (that still counts as "user verified"); Windows Hello ≠ always biometric
(PIN counts); no way to enumerate credentials.

**C-ABI**: `bio_available() → {none|platform|platform+prf}`; `bio_register(user_id, user_name) → CredentialId`;
`bio_verify(challenge32) → {ok, cred_id, uv_flag, signature...}` — all byte-buffer based (BufferSource in/out,
base64url at the JSON boundary).

---

## 6. Geolocation

- `navigator.geolocation.getCurrentPosition(ok, err, {enableHighAccuracy, timeout, maximumAge})`;
  `watchPosition(...)` → watchId, `clearWatch(id)`. Callback-style (wrap into promise/stream in glue).
- Data: `GeolocationPosition {coords: {latitude, longitude, accuracy, altitude?, altitudeAccuracy?, heading?, speed?}, timestamp}` —
  all doubles; error = `{code: 1 PERMISSION_DENIED | 2 POSITION_UNAVAILABLE | 3 TIMEOUT, message}`. Maps
  1:1 to a C struct.
- Gates: secure context (hard-blocked on HTTP since Chrome 50). No gesture strictly required, but Safari
  ties prompts to recent interaction and browsers auto-deny background/iframe spam; permission-policy gates
  iframes. Query state portably: `permissions.query({name:'geolocation'})` — works Chrome 43+/Firefox 46+/
  Safari 16+. OS-level location toggle produces `POSITION_UNAVAILABLE` even when browser permission is
  granted (double gate).
- Support: universal (Chrome 5+, Firefox 3.5+, Safari 5+; API shape unchanged). **Portable now.**
  ([MDN Geolocation API](https://developer.mozilla.org/en-US/docs/Web/API/Geolocation_API))
- Footguns: `watchPosition` throttled in background tabs; `timeout` default Infinity (always set one);
  desktop machines without GPS return IP-geolocation-grade accuracy (accuracy field is honest — surface it);
  Firefox "allow once" grants expire per session.

---

## 7. Clipboard

### Entry points

- `navigator.clipboard.writeText(string)` / `readText()` → Promise.
- Rich: `write([new ClipboardItem({'image/png': blobOrPromise, 'text/plain': ...})])`;
  `read()` → `ClipboardItem[]`, each `{types: string[], getType(mime) → Promise<Blob>}`.
  `ClipboardItem.supports(mime)` static (Chrome 121+, Firefox 127+, Safari 18.4+).
- MIME reality: sanitized `text/plain`, `text/html`, `image/png` are the interoperable trio. Chrome 104+
  adds unsanitized custom formats via `"web "` -prefixed types (Chromium-only). SVG spotty. Arbitrary
  formats: no.

### Gates (the messy part — three different models)

- **Chromium**: Permissions API names `clipboard-read` / `clipboard-write`. Write of text: allowed for
  focused document without prompt; read: permission prompt (persistent grant). Extra rule: reading requires
  the document be focused (`NotAllowedError: Document is not focused` — classic devtools-open footgun).
- **Firefox 127+**: no site permission; `read()`/`readText()` triggers an ephemeral **"Paste" confirmation
  popup** each time (user click on it = the grant); `writeText`/`write` need transient activation.
  Clipboard permission names are **not queryable** (permissions.query throws).
- **Safari 13.1+**: everything gated on **transient activation**; reads show a native "Paste" button flow;
  `ClipboardItem` values for async data **must be Promises created synchronously within the gesture**
  (the "write after await" bug: do `new ClipboardItem({'image/png': renderPromise})` immediately, don't await
  first). No Permissions API names.
- Everywhere: secure context; `navigator.clipboard` is absent otherwise. `document.execCommand('copy')`
  remains the legacy sync fallback (deprecated but universal).

### Support

Async clipboard is **Baseline newly-available since 2024-06-11** (the Firefox 127 date): Chrome 76+ (66 for
readText/writeText), Edge 79+, Safari 13.1+, Firefox 127+ (write since 125-126, `ClipboardItem` 127)
([web-features: async-clipboard](https://web-platform-dx.github.io/web-features-explorer/features/async-clipboard/),
BCD `api/ClipboardItem.json`, [MDN Clipboard API](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API)).
Image (PNG) copy/paste works on all three engines.

**Verdict: portable now** with a gesture-shaped API. C-ABI: `clipboard_write(items: [{mime, bytes}]) →
Ok|NeedsGesture|Denied`; `clipboard_read(accept_mimes) → items` where every call may interpose UI. Do not
design a poll-the-clipboard desktop idiom — there is no clipboard-change event on the web.

---

## 8. Image / media decode, fonts

- **`createImageBitmap(source, {resizeWidth, resizeHeight, resizeQuality, imageOrientation, premultiplyAlpha, colorSpaceConversion})`**
  → `ImageBitmap` (GPU-uploadable, transferable, works in workers; sources: Blob, ImageData, canvas, video).
  Universal/Baseline (Chrome 50+, Firefox 42+, Safari 15+ for full option support). First frame only for
  animated formats. ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/createImageBitmap))
- **`ImageDecoder` (WebCodecs)** — real decode control: `new ImageDecoder({data: BufferSource|ReadableStream, type: 'image/webp'})`,
  `decode({frameIndex})` → `{image: VideoFrame, complete}`; `tracks` for animation (frameCount,
  repetitionCount); `ImageDecoder.isTypeSupported(mime)`. Handles animated GIF/WebP/AVIF frame-by-frame.
  Support: Chrome/Edge 94+, **Firefox 133+ (desktop)**, **Safari 26.0+** (Sept 2025 — shipped the full
  WebCodecs set incl. ImageDecoder; 16.4-18.x were video-only). Newly cross-engine; treat as
  progressive-enhancement for ~1 more year, with `<img>`+canvas readback or wasm decoders as fallback.
  ([MDN ImageDecoder](https://developer.mozilla.org/en-US/docs/Web/API/ImageDecoder),
  [WebKit 26.0](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/), [Bugzilla ship](https://bugzilla.mozilla.org/show_bug.cgi?id=1923755))
- **`AudioDecoder` (WebCodecs)**: `configure({codec:'opus'|'mp4a.40.2'|..., sampleRate, numberOfChannels})`,
  `decode(EncodedAudioChunk)` → `AudioData` (planar/interleaved f32/s16 buffers — clean C-ABI shape).
  Support: Chrome 94+, Firefox 130+ (desktop, not Android), Safari 26.0+. Fallback: `AudioContext.decodeAudioData`
  (universal, whole-file, resamples to context rate). ([WebKit 26.0](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/),
  [MDN codec guide](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API/Codec_selection))
- **Fonts — `FontFace` API**: `new FontFace(family, buffer|url, {weight, style, unicodeRange...})`,
  `face.load()`, `document.fonts.add(face)`, `document.fonts.ready`, `check()`. Baseline-universal
  (Chrome 35+, Firefox 41+, Safari 10+). Loading from an `ArrayBuffer` (fonts shipped in the wasm bundle or
  OPFS) works everywhere — the natural framework path. **Enumerating installed system fonts**
  (`window.queryLocalFonts()`, Local Font Access, permission `local-fonts`, gesture-gated) is **Chromium
  103+ only**, Firefox/Safari negative — desktop font-picker parity is Chromium-only; ship bundled fonts
  elsewhere. ([MDN FontFace](https://developer.mozilla.org/en-US/docs/Web/API/FontFace),
  [MDN queryLocalFonts](https://developer.mozilla.org/en-US/docs/Web/API/Window/queryLocalFonts))
- Gates: none beyond secure context for WebCodecs; no permissions/gestures anywhere here (except
  queryLocalFonts). All usable in workers (ImageDecoder/AudioDecoder explicitly worker-exposed).

**Verdict**: createImageBitmap + FontFace = portable now; WebCodecs decode = portable-new (needs fallback on
Firefox Android + Safari <26); system-font enumeration = Chromium-only.

---

## 9. HTTP — fetch

- `fetch(url, {method, headers, body, signal, mode, credentials, cache, redirect, referrer, duplex?})` →
  `Response {status, headers, body: ReadableStream, arrayBuffer(), text(), json()}`.
- **Response streaming (download)**: `response.body.getReader().read()` chunks — universal (Chrome 43+,
  Firefox 65+, Safari 10.1+). Progress = count consumed bytes vs `Content-Length`.
- **Request streaming (upload)**: `body: ReadableStream` + `duplex: 'half'` — **Chromium 105+ only**
  (BCD `api/Request.json`: Firefox open bug 1387483, Safari none). Requires HTTP/2+; triggers CORS preflight.
  Everywhere else: buffer the upload (Blob/ArrayBuffer bodies stream from disk fine via Blob).
- **Abort/timeout**: `AbortController.abort(reason)`, `AbortSignal.timeout(ms)`, `AbortSignal.any([...])` —
  universal (timeout/any: Chrome 103/116+, Firefox 100/124+, Safari 16/17.4+).
- **CORS reality for a desktop-app-shaped framework**: an app that GETs *arbitrary user-supplied URLs*
  (feeds, images, APIs) **cannot** do so from the browser unless the target serves `Access-Control-Allow-Origin`.
  `mode:'no-cors'` yields opaque responses (body unreadable — useless except cache/img). There is no
  permission to escape CORS. **The framework needs a documented server-side proxy story** (or PWA +
  user-configured proxy) for "http_get(any_url)" parity with desktop. Cookies/credentialed cross-site
  requests additionally fight third-party-cookie phase-out.
- **Local network calls** (talking to localhost daemons / LAN devices — common desktop pattern): Chrome 142+
  (Oct 2025) gates fetches from public sites to private/loopback addresses behind a **Local Network Access
  permission prompt**; extended to WebSocket/WebTransport in Chrome 147. Expect `PermissionDeniedError`-shaped
  failures and surface them. ([Chrome LNA blog](https://developer.chrome.com/blog/local-network-access))
- Other gaps vs native HTTP: forbidden headers (`Host`, `Cookie`, `User-Agent`...), no raw sockets, no
  self-signed-cert override, HTTP cache is browser-managed, redirects are followed opaquely (`redirect:'manual'`
  gives you nothing readable).

**Verdict: portable now** for well-behaved/same-origin/CORS-enabled endpoints; **proxy-needed** for
arbitrary-URL fetch; upload streaming Chromium-only. ([MDN fetch](https://developer.mozilla.org/en-US/docs/Web/API/Window/fetch))

---

## 10. Networking beyond HTTP — WebTransport, WebSocket, WebRTC, and iroh

### WebTransport — **newly Baseline (March 2026)**

- `new WebTransport(httpsUrl, {serverCertificateHashes?})`, `await wt.ready`;
  `wt.datagrams.readable/.writable` (unreliable, MTU-ish sized Uint8Array payloads);
  `createBidirectionalStream()` / `createUnidirectionalStream()` + `incomingBidirectionalStreams` (reliable,
  multiplexed, no head-of-line blocking); `close({closeCode, reason})`. Runs over HTTP/3/QUIC.
- Support: Chrome/Edge 97+, Firefox 114+, **Safari 26.4+ (2026-03-24)** — Baseline newly-available; in
  Interop 2026. Firefox quirks: `getStats()` unimplemented, uni-streams return plain `WritableStream`.
  `serverCertificateHashes` (self-signed short-lived certs, the "talk to a local/dev endpoint" escape hatch):
  Chrome 100+, Firefox 125+, Safari 26.4+ — cert validity ≤14 days, ECDSA.
  ([web-features: webtransport](https://web-platform-dx.github.io/web-features-explorer/features/webtransport/),
  [caniuse](https://caniuse.com/webtransport), [WebKit 26.4](https://webkit.org/blog/17862/webkit-features-for-safari-26-4/),
  BCD `api/WebTransport.json`)
- Gates: secure context; no permission/gesture; server must speak HTTP/3 + WebTransport (not a raw QUIC
  socket). Safari <26.4 (still a large installed base through 2026) needs a WebSocket fallback.

### WebSocket

- Universal since ~2011; `new WebSocket(url, protocols)`, `binaryType='arraybuffer'`, `bufferedAmount` for
  backpressure (poll it — no drain event), no custom headers, TCP head-of-line blocking.
  `WebSocketStream` (promise/streams wrapper with real backpressure) is **Chromium 124+ only**.
  **Verdict: portable now; the lowest common denominator.**

### WebRTC DataChannel

- `RTCPeerConnection` + `createDataChannel(label, {ordered:false, maxRetransmits|maxPacketLifeTime})` —
  the only browser primitive with **unreliable/unordered delivery to another peer (or to a server
  impersonating a peer)** that works on every engine today incl. Safari < 26.4. Universal (Chrome 26+,
  Firefox 22+, Safari 11+).
- Cost: needs a signaling channel (SDP offer/answer), ICE/STUN(/TURN) infrastructure, DTLS+SCTP on the
  server side (heavyweight vs WebTransport). Message-size interop: keep chunks ≤16 KiB, negotiate via
  `sctp.maxMessageSize` (256 KiB typical modern).

### iroh over the browser (QUIC p2p)

Current reality, from the iroh team ([iroh 0.32 "Browsers Alpha"](https://www.iroh.computer/blog/iroh-0-32-0-browser-alpha-qad-and-n0-future),
[docs: WebAssembly & browsers](https://docs.iroh.computer/languages/wasm-browser),
[iroh 1.0 announcement, 2026-06-15](https://www.iroh.computer/blog/v1)):

- iroh compiles to `wasm32-unknown-unknown` and runs in browsers; **1.0 (June 2026) includes browser wasm as
  a continuously-tested target** with wire-protocol stability across versions/languages.
- In-browser transport = **WebSocket connections to iroh relay servers only**. The browser cannot send UDP,
  so **no hole-punching, no direct connections** — everything relays ("it won't try to hole-punch … not
  possible in browsers without deeply integrating with WebRTC"). End-to-end encryption is preserved through
  the relay (relay sees ciphertext).
- Build constraints: `default-features = false` (drop `metrics`, discovery/pkarr-dht, local-network
  discovery), `wasm-bindgen` + `wasm-bindgen-futures`, `n0-future` for runtime-agnostic async;
  `iroh-gossip` works in browsers since 0.33.
- Not yet used: WebTransport/WebRTC transports (would enable browser↔server QUIC-grade paths or
  browser↔browser direct); nothing shipped as of 1.0.
- **Framework implication**: an iroh-backed p2p API can present identical Rust surface on desktop and web,
  but the web build is relay-bound (latency/throughput ceiling, relay availability required). Now that
  WebTransport is Baseline, a future iroh WebTransport relay/endpoint path is plausible — track upstream.

**Verdict**: WebSocket portable-now; WebTransport portable-now-with-Safari-26.4-floor (fallback for older
Safari); WebRTC DataChannel portable but operationally expensive; iroh-in-browser = works today via
WS-relay, relay-only.

---

## 11. Sensors

- **Generic Sensor API** (`new Accelerometer({frequency})`, `Gyroscope`, `LinearAccelerationSensor`,
  `GravitySensor`, `AbsoluteOrientationSensor`, `Magnetometer`, `AmbientLightSensor`): **Chromium 67-91+
  only**; Firefox and Safari **negative positions** (privacy/fingerprinting) — "very unlikely to ever become
  Baseline" ([web-features: accelerometer](https://web-platform-dx.github.io/web-features-explorer/features/accelerometer/)).
  Permissions: per-sensor names (`'accelerometer'`, `'gyroscope'`…) auto-granted or prompted in Chrome;
  `SecurityError`/`NotAllowedError` + `onerror` events. Chromium-only bucket.
- **DeviceOrientation/DeviceMotion events** (the portable path): `window.addEventListener('deviceorientation'|'devicemotion')`
  → Euler angles alpha/beta/gamma, accel xyz incl. gravity, rotationRate, `interval`. Universal on
  mobile hardware. Gate: **iOS 13+** requires `DeviceOrientationEvent.requestPermission()` /
  `DeviceMotionEvent.requestPermission()` — gesture-gated, promise `'granted'|'denied'`; Safari 26.4 restricted
  the events to secure contexts. Desktop browsers fire nothing (no sensor). Data is unfused/noisy vs Generic
  Sensor quaternions.
- **Battery**: `navigator.getBattery()` → `{charging, level 0-1, chargingTime, dischargingTime}` + change
  events. **Chromium 38+ only** (secure context since 103); Firefox shipped 43-51 then **removed in 52**
  (privacy); Safari never (BCD `api/BatteryManager.json`). Chromium-only.
- **Network information**: `navigator.connection {effectiveType '4g'.., downlink (≤10Mbps cap), rtt (≤3s cap),
  saveData; type/downlinkMax Android/ChromeOS only}` + `change`. **Chromium only** (desktop 61+); Firefox
  removed (Android removed after 99); Safari none (BCD `api/NetworkInformation.json`). Treat as hint-only.
- **Vibration**: `navigator.vibrate(ms | pattern[])` — Chrome-on-Android 30+ and Firefox-on-Android
  (16+ desktop API exists but no hardware); **Safari/iOS: never** ([caniuse vibration](https://caniuse.com/vibration)).
  Gesture required on Chrome Android (silently no-ops otherwise). No iOS haptics for web at all.

**Verdict**: motion sensing = fallback-needed (DeviceOrientation portable-ish on mobile w/ iOS gesture
dance; Generic Sensor Chromium-only); battery/network-info/vibration = Chromium-only extras — expose as
optional capabilities defaulting `unsupported`.

---

## 12. System integration bits

- **Web Share**: `navigator.share({title, text, url, files?})`, probe `navigator.canShare({files})`.
  Gesture required, secure context, one share at a time; `AbortError` on cancel. Support (caniuse
  `web-share.json`): Chrome desktop 89+ Windows/ChromeOS-only → **full incl. macOS/Linux since 128**;
  Edge 95+; Safari 14+ (12.1 partial); iOS 12.2+; Chrome Android yes; Firefox Android **153+ (2026)**;
  **Firefox desktop: none**. Verdict: portable-except-Firefox-desktop — fall back to clipboard-copy.
  `files` sharing narrower (Chrome 89+/Safari 15+). ([MDN Web Share](https://developer.mozilla.org/en-US/docs/Web/API/Web_Share_API))
- **Page visibility**: `document.visibilityState 'visible'|'hidden'` + `visibilitychange`, plus
  `document.hasFocus()`. Universal. This is the web's minimize/background signal; also the trigger to
  re-acquire wake locks and expect timer throttling. Portable now.
- **Wake lock**: `navigator.wakeLock.request('screen')` → `WakeLockSentinel {release(), onrelease}`.
  **All engines**: Chrome 84+, Safari 16.4+, Firefox 126+ ([web.dev](https://web.dev/blog/screen-wake-lock-supported-in-all-browsers),
  [caniuse wake-lock](https://caniuse.com/wake-lock)). Secure context; visible-document requirement
  (`NotAllowedError` when hidden; auto-released on hide — re-request on visibilitychange). Only `'screen'`
  type (no system/CPU lock). Portable now.
- **Idle detection** (user idle/screen locked): `IdleDetector` + `IdleDetector.requestPermission()`
  (gesture) — **Chromium 94+ only**, Firefox/Safari negative positions
  ([web-features: idle-detection](https://web-platform-dx.github.io/web-features-explorer/features/idle-detection/)).
  Fallback: synthesize idle from visibility+input events inside your own windows.
- **Locale/timezone**: `navigator.languages` + `languagechange`; `Intl.DateTimeFormat().resolvedOptions().timeZone`
  (IANA name); `Intl.Locale`, `NumberFormat`, `RelativeTimeFormat`, `Intl.supportedValuesOf('timeZone'|'currency'|...)`
  (Chrome 99+/Firefox 93+/Safari 15.4+). No timezone-change event — re-check on visibility/focus. Universal.
- **Monitors / window placement**: `window.getScreenDetails()` → `{screens: [{left, top, width, height,
  availWidth/Height, devicePixelRatio, label, isPrimary, isInternal}], currentScreen, onscreenschange}`;
  permission name `'window-management'` (gesture-prompted); enables multi-screen `window.open` placement +
  `element.requestFullscreen({screen})`. **Chromium 100/111+ only**; Firefox/Safari none
  ([web-features: window-management](https://web-platform-dx.github.io/web-features-explorer/features/window-management/),
  [MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/getScreenDetails)). Portable subset:
  single `window.screen` (width/height/availWidth/availHeight, `screen.orientation` + change event,
  `isExtended` hint Chrome-only). Multi-monitor awareness = Chromium-only.
- **DPI**: `window.devicePixelRatio` (universal). Change notification: none dedicated — re-arm
  `matchMedia(`(resolution: ${dpr}dppx)`).addEventListener('change', …)` after each fire (covers zoom +
  monitor moves). Physical-pixel-perfect canvas sizing: `ResizeObserver` `devicePixelContentBoxSize` —
  Chrome 84+/Firefox 108+, **Safari: none** (BCD) — fall back to `getBoundingClientRect()*dpr` rounding.

---

## 13. Timers / threads (one paragraph each)

- **setTimeout/rAF as an event pump**: `setTimeout`/`setInterval` clamp to ≥4 ms after 5 levels of nesting
  and are throttled in hidden tabs to ≥1 Hz (Chromium "intensive throttling": chained timers → 1/min after
  5 min hidden; Safari/Firefox similar in spirit). `requestAnimationFrame(cb(timestampMs))` fires per display
  refresh (incl. 120 Hz) and **stops entirely while hidden**. Consequence for a GUI framework: render loop on
  rAF; background work on workers or `visibilitychange`-aware timers; never busy-wait (blocks the only UI
  thread); for immediate-yet-ordered scheduling use `queueMicrotask`/`MessageChannel` postMessage (no 4 ms
  clamp); `scheduler.postTask(cb, {priority})`/`scheduler.yield()` exist on Chrome 94+/129+ and Firefox 142+
  but **not Safari** (BCD `api/Scheduler.json`) — feature-detect, fall back to MessageChannel.
- **Web Workers / module workers**: `new Worker(url, {type:'module'})` universal (module type: Chrome 80+,
  Safari 15+, Firefox 114+); workers run wasm fine, get OPFS sync handles (§2), fetch, WebSocket/WebTransport,
  createImageBitmap/ImageDecoder — but **no DOM**. Data crosses via `postMessage` structured clone +
  transferables (ArrayBuffer, ImageBitmap, OffscreenCanvas, streams); `OffscreenCanvas` (universal since
  Safari 16.4/Firefox 105) lets a worker own rendering. Worker startup ~ms and each worker is an OS thread —
  pool them; nested workers work everywhere modern.
- **SharedArrayBuffer / wasm threads**: `SharedArrayBuffer` + `Atomics` (incl. `Atomics.wait` in workers only;
  `Atomics.waitAsync` main-thread: Chrome 87+/Safari 16.4+/Firefox 140+) require **cross-origin isolation**:
  top-level response headers `Cross-Origin-Opener-Policy: same-origin` + `Cross-Origin-Embedder-Policy:
  require-corp` (or `credentialless` — Chrome 96+/Firefox 119+, **not Safari**), check
  `self.crossOriginIsolated`. Supported with those headers on Chrome 92+*, Firefox 79+, Safari 15.2+
  (*Android 88+). Isolation breaks casually-embedded cross-origin iframes/images (need CORP/CORS) and
  disables non-isolated popups (`window.open` OAuth flows need `Cross-Origin-Opener-Policy-Report-Only` or
  COOP `same-origin-allow-popups` compromises — a real deployment footgun for wasm-threaded apps).
  ([web.dev COOP/COEP](https://web.dev/articles/coop-coep), [MDN COEP](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy))

---

## Summary table

Verdicts: **P** = portable now (all three engines) · **P-new** = portable but newest engine support is <18
months old (keep fallback) · **C** = Chromium-only · **F** = fallback-needed (portable core + degraded path
elsewhere).

| Capability | Primary API | Chrome/Edge | Firefox | Safari | Gesture / permission gates | Verdict |
|---|---|---|---|---|---|---|
| Local dir trees (read/write) | `showDirectoryPicker` + handles | 86+ (Android 132+) | never (neg.) | never (neg.) | gesture for picker + `requestPermission`; per-handle read/readwrite prompts | **C** — fallback: `webkitdirectory` / `webkitGetAsEntry` read-only snapshot |
| Dir fallback (read-only) | `<input webkitdirectory>` | 30+ | 50+ | 11.1+ (iOS 18.4+) | file-picker gesture | **P** (read-only, no persistence) |
| App-private FS | OPFS `getDirectory` + sync handles | 108+ | 111+ | 16.4+ | none (quota only; `persist()` for eviction-exemption) | **P** (Baseline widely-avail. 2025) |
| Save file | `showSaveFilePicker` | 86+ | never | never | gesture | **C** — fallback `<a download>` Blob (**P**, no re-save/stream) |
| Notifications (basic, via SW) | `registration.showNotification` | ✔ (SW-only on Android) | ✔ (gesture to request) | 16.4+ (iOS: installed PWA only; gesture) | permission prompt; FF+Safari need gesture for request | **P** (iOS: install-gated) |
| Notification actions | `options.actions` | 53+ | **152+ (2026)** | never | same as above | **F** (Safari: default click only) |
| App badge | `setAppBadge` | 81+ desktop | never | iOS 16.4+/mac 17+ | installed-app only; iOS ties to notif permission | **F** |
| Biometric auth | WebAuthn `credentials.get` UV=required (+`prf`) | 67+ (prf 116/147+) | 60+ (prf 139-148) | 13+ (prf 18-26.4) | gesture (in practice); rpID=origin; needs registration ceremony | **P** |
| Auth presence probe | `isUVPAA` / `getClientCapabilities` | 67+ / 133+ | 60+ / 135+ | 13+ / 17.4+ | none | **P** |
| Geolocation | `getCurrentPosition`/`watchPosition` | ✔ | ✔ | ✔ | permission prompt; secure ctx; OS-level second gate | **P** |
| Clipboard text | `writeText`/`readText` | 66+ | 125-127+ | 13.1+ | Chromium: read-permission; FF: per-read paste popup; Safari: gesture-only | **P** |
| Clipboard rich (PNG/HTML) | `read`/`write` + `ClipboardItem` | 76+ | 127+ | 13.1+ (promise-in-gesture rule) | same | **P** (Baseline 2024) |
| Image decode (fast path) | `createImageBitmap` | ✔ | ✔ | ✔ | none | **P** |
| Image decode (frames/animated) | `ImageDecoder` | 94+ | 133+ (not Android) | **26.0+** | none | **P-new** |
| Audio decode | `AudioDecoder` (fallback `decodeAudioData`) | 94+ | 130+ (not Android) | **26.0+** | none | **P-new** |
| Fonts (load own) | `FontFace` + `document.fonts` | ✔ | ✔ | ✔ | none | **P** |
| Fonts (enumerate system) | `queryLocalFonts` | 103+ | never | never | gesture + `local-fonts` permission | **C** |
| HTTP GET/POST | `fetch` + streams + Abort | ✔ | ✔ | ✔ | CORS wall for arbitrary URLs; Chrome 142+ LNA prompt for localhost/LAN | **P** (+ **proxy-needed** for arbitrary URLs) |
| HTTP upload streaming | `duplex:'half'` ReadableStream body | 105+ | never (bug open) | never | HTTP/2 + preflight | **C** |
| WebSocket | `WebSocket` (`WebSocketStream` Chr 124+) | ✔ | ✔ | ✔ | none (LNA for LAN targets, Chr 147+) | **P** |
| WebTransport | `WebTransport` datagrams+streams | 97+ | 114+ (minor gaps) | **26.4+ (2026-03)** | none; server must do HTTP/3; `serverCertificateHashes` Chr 100/FF 125/Saf 26.4 | **P-new** (fallback WS for Safari <26.4) |
| WebRTC DataChannel | `RTCPeerConnection`+`createDataChannel` | ✔ | ✔ | ✔ | none (needs signaling+ICE infra) | **P** (ops-heavy) |
| iroh p2p | iroh wasm (1.0, Jun 2026) over WS-relays | ✔ | ✔ | ✔ | relay-only: no UDP/hole-punch in any browser | **F** (works everywhere, degraded to relay) |
| Motion sensors (high-level) | Generic Sensor (`Accelerometer`…) | 67-91+ | never (neg.) | never (neg.) | per-sensor permissions | **C** |
| Motion sensors (events) | `deviceorientation`/`devicemotion` | ✔ (mobile hw) | ✔ (mobile hw) | ✔ + **iOS `requestPermission()` gesture** | iOS 13+ gesture-gated; secure ctx | **F** (mobile-only hw) |
| Battery | `navigator.getBattery` | 38+ | removed (52) | never | secure ctx | **C** |
| Network info | `navigator.connection` | 61+ | removed | never | — | **C** (hint only) |
| Vibration | `navigator.vibrate` | Android only | Android only | never | gesture (Android) | **C**-ish (mobile; no iOS) |
| Share sheet | `navigator.share`/`canShare` | 89+/full 128+ | **desktop: never**; Android 153+ | 12.1/14+ | gesture; secure ctx | **F** (FF desktop → clipboard fallback) |
| Visibility | `visibilitychange` | ✔ | ✔ | ✔ | none | **P** |
| Keep screen awake | `wakeLock.request('screen')` | 84+ | 126+ | 16.4+ | visible-doc; auto-release on hide | **P** |
| Idle detection | `IdleDetector` | 94+ | never (neg.) | never (neg.) | gesture + permission | **C** |
| Locale/timezone | `Intl` + `navigator.languages` | ✔ | ✔ | ✔ | none | **P** |
| Multi-monitor | `getScreenDetails` | 100/111+ | never | never | gesture + `window-management` permission | **C** (fallback: single `window.screen`) |
| DPI | `devicePixelRatio` + matchMedia re-arm | ✔ | ✔ | ✔ (no `devicePixelContentBoxSize`) | none | **P** |
| GUI event pump | rAF + workers (+`scheduler.postTask` Chr 94+/FF 142+, no Safari) | ✔ | ✔ | ✔ (no postTask) | bg-tab throttling; rAF stops when hidden | **P** (design around throttling) |
| wasm threads | SAB + Atomics + COOP/COEP | 92+ | 79+ | 15.2+ (no COEP `credentialless`) | needs cross-origin-isolated deployment headers | **P** (deployment-gated) |

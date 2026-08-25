# 05 — Asset & permission foundation (fonts, images, OS permissions)

Research brief for SUPER_PLAN_2 §1.1 "Mobile font / image loading + permissions."
Targets iOS, Android, macOS, Linux, Windows; Web/WASM is the shape compat target.

Files inspected: `SUPER_PLAN_2.md`, `scripts/architecture.md`,
`layout/Cargo.toml`, `core/src/resources.rs`,
`dll/src/desktop/shell2/{ios,android}/mod.rs`, `dll/src/desktop/app.rs`,
`layout/src/image.rs`, local override at
`/Users/fschutt/Development/rust-fontconfig/src/{lib,registry,config,multithread}.rs`.

---

## 0. TL;DR — verified gaps

* **`rust-fontconfig` finds zero fonts on iOS or Android.**
  `FcFontCache::build()` has only `cfg(target_os = "{linux,windows,macos}")`
  arms (`rust-fontconfig/src/lib.rs:1835/1856/1882`). `OperatingSystem::current()`
  catch-all returns `Linux` (`lib.rs:134`), so the scout tries `/usr/share/fonts`
  on both mobile targets — empty.
* **Neither mobile backend amends `fc_cache`.**
  `shell2/ios/mod.rs:818` + `shell2/android/mod.rs:94` accept
  `Arc<FcFontCache>` read-only.
* **Text only renders today thanks to `AppConfig.bundled_fonts`
  (`resources.rs:422`) + the embedded `material-icons` TTF (`layout/Cargo.toml:79`).**
  An app without `bundled_fonts.push(...)` and without the `icons`
  feature has zero fonts on iOS/Android. TODO: verify by running
  `scripts/mobile/golden/<feature>.png` with `icons` off.
* **Image decode works** — pure-Rust `image 0.25` via
  `layout/src/image.rs::decode_raw_image_from_any_bytes`.
* **Image byte-acquisition not implemented.** No PhotoKit / MediaStore
  code path.
* **No permission infrastructure** — no `PermissionManager`, no
  `App::request_*`, no plist/manifest generation.

---

## 1. Fonts on mobile

### 1.1 iOS — sandboxed; use CoreText

System fonts live in `/System/Library/Fonts/{,Core,AssetsV2}` and a
CoreText cache under `/var/mobile/Library/Caches/com.apple.CoreText/`,
but the iOS sandbox denies direct `open(2)` on anything outside the
app container even when files are world-readable (Apple, *App Sandbox
Design Guide*, "File System Access"). Headline fonts are `.ttc`
collections (`Helvetica.ttc`, `SFNS.ttc`, `PingFang.ttc`);
`rust-fontconfig` already parses `.ttc` through `allsorts`
(`lib.rs:3953..3967`) — only enumeration / I/O is missing.

CoreText entry points (`<CoreText/CTFontManager.h>`):

* `CTFontManagerCopyAvailableFontURLs()` (iOS 13+) — `CFArrayRef<CFURLRef>`
  of every visible font; URLs are sandbox-mediated and openable with
  `CGDataProviderCreateWithURL` even though `/System/...` is otherwise
  inaccessible. Pre-iOS 13 fallback:
  `CTFontManagerCopyAvailableFontFamilyNames` →
  `CTFontDescriptorCreateWithName` → `kCTFontURLAttribute` per family.
* `CTFontCopyTable(font, tag, ...)` — read individual SFNT tables
  (`cmap`, `head`, `name`) without ever touching the file.
* `CTFontManagerRegisterFontsForURL` / `RegisterGraphicsFont` — register
  an app-bundled font with the system so other APIs find it by family.
* Bundled fonts declared in `Info.plist` `UIAppFonts` are
  auto-registered at launch.

(Apple, *Core Text Programming Guide*; *Adding a Custom Font to Your App*.)

`AppConfig.bundled_fonts: NamedFontVec` (`resources.rs:422`) already
carries `(name, bytes)`. On iOS we additionally
`CTFontManagerRegisterGraphicsFont` so the system shaper (used by the
soft keyboard preview) can find them. TODO: verify dual-registration is
necessary versus parsing bytes ourselves only.

### 1.2 Android — `/system/fonts/` + `fonts.xml`

System fonts live under `/system/fonts/` and are world-readable; apps
can `open(2)` and `mmap(2)` them with no API wrapper. The family map +
fallback chain config is at `/system/etc/fonts.xml` (API 21+; older
devices used `/system/etc/system_fonts.xml` + `fallback_fonts.xml`).
Source: AOSP `frameworks/base/data/fonts/fonts.xml`.

Schema highlights: `<family name="sans-serif">` blocks bind a family
to one or more weight/style font files; `<family lang="und-Arab">`
blocks are the locale-keyed fallback chain; `<alias name="..." to="..."
weight="...">` maps additional family names onto an existing block. A
correct discoverer must (a) walk `/system/fonts/`, (b) parse `fonts.xml`
to build the family map + fallback chain — otherwise CSS
`font-family: sans-serif` won't resolve to Roboto on most devices, and
(c) optionally read app-bundle fonts from `assets/fonts/` (user reads
via `AssetManager.open()` and forwards bytes through
`AppConfig.bundled_fonts`).

NDK side, `AFontMatcher_*` (API 29+) is a thin C wrapper over the same
config (NDK ref `<android/font_matcher.h>`). Downloadable Fonts via
`com.google.android.gms.fonts` is out of scope for v1 — `NamedFont`
already accepts arbitrary bytes if the user wants to fetch them.

### 1.3 Verified `rust-fontconfig` mobile gap

`OperatingSystem::current()` (`lib.rs:121`) returns `Linux` as the
catch-all for unknown targets, so iOS / Android map to `Linux`.
`FcFontCache::build()` (`lib.rs:1833..1903`) has only `target_os` arms
for linux / windows / macos — no body executes on mobile, cache stays
empty. The async scout (`multithread.rs:27`) reads
`config::font_directories(self.os)`, so on mobile it tries
`/usr/share/fonts` and `$HOME/.fonts` — neither path exists.

The only fonts that show up on mobile today: `AppConfig.bundled_fonts`
(`resources.rs:422`) and the embedded `material-icons` TTF, registered
via the `icons` feature at `dll/src/desktop/app.rs:57`.

### 1.4 Cross-platform Rust font-discovery crates

| Crate | iOS / Android | Notes |
|---|---|---|
| `core-text` (servo) | iOS yes / n/a | Already optional in `layout/Cargo.toml:120` (`coretext_tests`); just needs an `ios` `cfg` arm. |
| `font-kit` (servo) | yes / yes (FreeType-backed) | Heavy dep tree (FreeType + DirectWrite). |
| `fontique` (linebender/parley) | yes / yes | Native `fonts.xml` parsing; CoreText on Apple. Cleanest fit for cosmic-text-style fallback. |
| `fontdb` (RazrFalcon) | partial | Filesystem only — misses TTC-only system fonts on iOS. |

`fontique` is the strongest external option. We still recommend patching
`rust-fontconfig` instead (§1.5) — the framework type churn would be
large. TODO: verify fontique MSRV + license vs. Azul's MPL-2.0.

### 1.5 Recommendation — patch `rust-fontconfig`, don't swap

The user owns `rust-fontconfig` as a local override. Patching it is the
cleanest path because `FcFontCache` is already plumbed through
`AppInternal.fc_cache`, both mobile windows, and 15+ test files;
`FcFontRegistry` already gives us the scout/background semantics
fontique would re-implement. Add two new `OperatingSystem` variants
(`IOS`, `Android`) and the two missing arms in `FcFontCache::build`,
`config::font_directories`, and `multithread::scout_thread`:

* **iOS arm** — call `core_text::font_manager::copy_available_font_urls()`
  (or its `msg_send!` equivalent), then feed each URL to the existing
  `FcParseFont` path (`.ttc` collection support already in place at
  `lib.rs:3953..3967`). `CTFontManagerCopyAvailableFontURLs` is iOS 13+;
  for iOS 11/12 the fallback is `CopyAvailableFontFamilyNames` →
  `CTFontDescriptorCreateWithName` → `kCTFontURLAttribute`. TODO: verify
  the iOS min in `dll/build.rs::configure_ios`.

* **Android arm** — walk `/system/fonts/`, `/product/fonts/`,
  `/system_ext/fonts/` (vendor variation), and parse
  `/system/etc/fonts.xml` (fallback `/vendor/etc/fallback_fonts.xml`).
  Parser is ~80 LOC of `xmlparser` (already a dep). Feed family map +
  `<family lang>` chain into `FcFontCache::state.fallback_chains` so
  CSS `font-family: sans-serif` resolves to Roboto.

`bundled_fonts: NamedFontVec` already short-circuits the system path in
the registry's `memory_fonts` lookup — no new wiring needed for the
app-bundle case on either platform.

After the rust-fontconfig change, `shell2/{ios,android}/mod.rs` need no
new code — `App::create()` already calls
`FcFontRegistry::spawn_scout_and_builders()` (`app.rs:155`). One nit:
`CTFontManagerCopyAvailableFontURLs` is documented thread-safe but
`CTFontManagerRegisterFontsForURL` is not — restrict the latter to the
main queue or wrap with `dispatch_sync(main_queue, ...)`.

---

## 2. Images on mobile — Photos library / Camera roll / file system

### 2.1 The decoder path is already cross-platform

`layout/src/image.rs::decode_raw_image_from_any_bytes()` (line 70) takes
arbitrary `&[u8]` and returns a `RawImage`. The `image 0.25` crate is
pure-Rust. JPEG, PNG, WebP, GIF, BMP, TIFF, HEIF (via `image-hdr` feature
elsewhere) are all decodeable. **The question is: how do the bytes
arrive?**

Three flows to design for, mirroring W3C primitives:

| Flow | W3C analogue | iOS | Android |
|---|---|---|---|
| Pick existing photo | `<input type="file" accept="image/*">` | `PHPickerViewController` (iOS 14+) | `ACTION_PICK` / `ActivityResultContracts.PickVisualMedia` (API 33+) |
| Read user file | `File System Access API` | `UIDocumentPickerViewController` | `ACTION_OPEN_DOCUMENT` (Storage Access Framework) |
| Capture live photo | `getUserMedia({video: true})` | `UIImagePickerController(sourceType: .camera)` (deprecated) / `AVCaptureSession` (feature #2) | `ACTION_IMAGE_CAPTURE` (returns URI) / `CameraX` |

The first two return a sandbox-mediated path/URI; the framework reads
the bytes (or maps them as `mmap`) and feeds `decode_raw_image_from_any_bytes`.
File picker is already a planned feature (#8 in SUPER_PLAN_2). Photo
picker overlaps it but is structurally different on iOS — see §2.4.

### 2.2 iOS — Photos.framework + PhotoKit

* **Picker (preferred path)**: `PHPickerViewController` (iOS 14+) is
  system-rendered and **needs no `Info.plist` key, no authorization
  call**. Results carry `NSItemProvider`s; resolve bytes via
  `loadDataRepresentation(forTypeIdentifier:completionHandler:)`.
  (Apple WWDC20 session 10652.)
* **Full-library access**: `PHPhotoLibrary.requestAuthorization(for:.readWrite, handler:)`
  returns one of `.notDetermined | .restricted | .denied | .authorized | .limited`.
  Calling `PHAsset.fetchAssets(...)` without
  `NSPhotoLibraryUsageDescription` in `Info.plist` **crashes the app at
  the call site** (SIGABRT, "This app has crashed because it attempted
  to access privacy-sensitive data without a usage description").
* **Per-asset bytes**:
  `PHImageManager.default().requestImageDataAndOrientation(for:options:resultHandler:)`
  hands back `(Data?, String? UTI, ...)`. Feed `Data` straight to
  `decode_raw_image_from_any_bytes`.

Info.plist (only for full-library access — picker doesn't need these):

```xml
<key>NSPhotoLibraryUsageDescription</key>      <string>Why we need it.</string>
<key>NSPhotoLibraryAddUsageDescription</key>   <string>Why we need write.</string>
```

### 2.3 Android — MediaStore + Storage Access Framework

* **Picker (preferred path)**: `MediaStore.ACTION_PICK_IMAGES` (API 33+)
  or `ActivityResultContracts.PickVisualMedia` / `PickMultipleVisualMedia`
  (the AndroidX wrapper, back-ports to older API levels via Google Play
  services). **Needs no permission**; returns content URIs scoped to the
  caller. Older fallback: `Intent.ACTION_PICK` on
  `MediaStore.Images.Media.EXTERNAL_CONTENT_URI`.
  (developer.android.com/training/data-storage/shared/photopicker.)
* **Bytes**: `ContentResolver.openInputStream(uri)` returns
  `InputStream`. Read on the JNI side (existing bridge pattern in
  `shell2/android/mod.rs:743`) into `Vec<u8>` → decode.
* **Full-library access**: needs `READ_MEDIA_IMAGES` (API 33+) or
  `READ_EXTERNAL_STORAGE` (API ≤32) at runtime.
* **General file picker**: `Intent.ACTION_OPEN_DOCUMENT` (API 19+) →
  same SAF URI pattern.

AndroidManifest.xml (only for full-library access):

```xml
<uses-permission android:name="android.permission.READ_MEDIA_IMAGES"
                 android:maxSdkVersion="33" />
<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE"
                 android:maxSdkVersion="32" />
```

### 2.4 Why pickers come before raw library access

System pickers (`PHPickerViewController`, `PickVisualMedia`) require
**no permission prompt**, no usage-string in plist/manifest, and have
matching sheet-style UX on both platforms. The Azul API
`App::pick_photos(options) -> AzFuture<Vec<ImageRef>>` behaves
identically on iOS, Android, and (via `<input type="file">`) the web
back-end. Full-library enumeration (photo-browser apps) comes second,
behind `request_permission(Permission::PhotoLibrary)`.

`ImageCache.image_id_map: OrderedMap<AzString, ImageRef>`
(`resources.rs:1121`) already supports CSS named images — picker API
returns `Vec<ImageRef>`, user assigns CSS ids. No new caching surface.

---

## 3. Permissions

### 3.1 iOS — Info.plist + first-call prompt

iOS permissions are declarative + runtime. Missing a required
`Info.plist` key = the OS SIGABRTs the app at the API call.

| Feature | Info.plist key | Runtime entry |
|---|---|---|
| Photo library | `NSPhotoLibraryUsageDescription` | `PHPhotoLibrary.requestAuthorization` |
| Photo library write | `NSPhotoLibraryAddUsageDescription` | `PHPhotoLibrary.requestAuthorization(.addOnly)` |
| Camera (#2) | `NSCameraUsageDescription` | `AVCaptureDevice.requestAccess(for:.video)` |
| Microphone | `NSMicrophoneUsageDescription` | `AVAudioSession.requestRecordPermission` |
| Geolocation (#10) | `NSLocationWhenInUseUsageDescription`, `NSLocationAlwaysAndWhenInUseUsageDescription` | `CLLocationManager.requestWhenInUseAuthorization` |
| Biometrics (#4) | `NSFaceIDUsageDescription` (FaceID only) | `LAContext.evaluatePolicy` |
| Motion (#5) | `NSMotionUsageDescription` | `CMMotionManager.start*Updates` |
| Bluetooth | `NSBluetoothAlwaysUsageDescription` | `CBCentralManager` |
| Local network | `NSLocalNetworkUsageDescription` | first multicast |

Full list at developer.apple.com/documentation/bundleresources/information_property_list
("Privacy keys"). Status reads (`authorizationStatus`) are sync class
methods; request entry points are async callback-style.

`App::request_permission(Permission::PhotoLibrary, prompt)` ignores
`prompt` on iOS (the text is the plist value) but logs a warning if
the matching plist key isn't present. TODO: verify
`Bundle.main.object(forInfoDictionaryKey:)` is the right runtime check.

### 3.2 Android — manifest + runtime

Two-tier model:

1. **`<uses-permission>` in `AndroidManifest.xml`** — declared at
   install time. Normal-level permissions (e.g. `INTERNET`,
   `VIBRATE`, `ACCESS_NETWORK_STATE`) are granted automatically;
   dangerous-level permissions need step 2.

2. **Runtime prompt** — `ActivityCompat.requestPermissions(activity, permissions, requestCode)`
   or the modern `ActivityResultContracts.RequestPermission` /
   `RequestMultiplePermissions`. Returns through `onRequestPermissionsResult`
   (legacy) or the registered launcher's callback (modern). Status
   query: `ContextCompat.checkSelfPermission(context, perm) == PERMISSION_GRANTED`.

   Dangerous permissions of interest:

   | Feature | Permission |
   |---|---|
   | Photo library (legacy) | `READ_EXTERNAL_STORAGE` (API ≤32), `READ_MEDIA_IMAGES`/`VIDEO`/`AUDIO` (API 33+) |
   | Camera (#2) | `CAMERA` |
   | Microphone | `RECORD_AUDIO` |
   | Geolocation (#10) | `ACCESS_COARSE_LOCATION`, `ACCESS_FINE_LOCATION`, `ACCESS_BACKGROUND_LOCATION` (API 29+) |
   | Body sensors | `BODY_SENSORS`, `ACTIVITY_RECOGNITION` |
   | Phone state | `READ_PHONE_STATE` |
   | Contacts / Calendar | `READ_CONTACTS`, `READ_CALENDAR` |

   The full taxonomy is at developer.android.com/reference/android/Manifest.permission. The "Dangerous Permissions" subset
   is what needs runtime prompts.

3. **Special permissions** — a third category requiring a settings-app
   intent (`ACTION_MANAGE_OVERLAY_PERMISSION`, `MANAGE_EXTERNAL_STORAGE`,
   `SYSTEM_ALERT_WINDOW`, `POST_NOTIFICATIONS` on API 33+). Not
   `requestPermissions`-able; the framework needs to launch a settings
   intent and listen for the activity result.

### 3.3 The state machine across platforms

iOS and Android (and the W3C `navigator.permissions.query()` API) all
return a similar enum:

```rust
#[repr(C)]
pub enum PermissionStatus {
    NotDetermined,  // iOS .notDetermined  / Android: not asked yet / W3C "prompt"
    Granted,        // iOS .authorized     / Android PERMISSION_GRANTED / W3C "granted"
    Denied,         // iOS .denied         / Android: denied + don't-ask-again / W3C "denied"
    Restricted,     // iOS .restricted (MDM / parental) — Android equivalent is `shouldShowRequestPermissionRationale==false && status==denied`
    Limited,        // iOS .limited (photos only) — Android: per-media-type API 34+ "Selected photos"
}
```

The OS can revoke permission at any time (settings.app on iOS, app-info
on Android), so every call site must re-check before use. TODO: verify
whether Android's "revoke permission on unused app" (API 30+) flips
status back to NotDetermined or to Denied.

### 3.4 W3C compatibility

Web `navigator.permissions.query({name})` returns `'granted' | 'denied'
| 'prompt'` — maps cleanly onto our `PermissionStatus` (`Limited` is a
proposed extension). `<input type="file">` is the picker without
permissions on web. (W3C *Permissions API*, *File API*.)

---

## 4. Proposed Azul integration

### 4.1 New manager: `PermissionManager`

`layout/src/managers/permission.rs`, modelled on FocusManager /
FileDropManager. Holds the latest known status per permission for
sync callback reads, plus a queue of pending requests:

```rust
#[repr(C, u8)]
pub enum Permission {
    PhotoLibrary, PhotoLibraryWrite,
    Camera, Microphone,
    Geolocation, GeolocationBackground,
    Motion, Biometrics, Bluetooth, LocalNetwork,
    Notifications, Contacts, Calendar, Reminders,
}

pub struct PermissionManager {
    pub statuses: BTreeMap<Permission, PermissionStatus>,
    pub pending: Vec<PendingPermission>,  // permission + prompt + callback_id
}
```

CallbackInfo accessor: `get_permission_status(Permission) -> PermissionStatus`.
Platform injection point mirrors `inject_native_gesture` — when the OS
authorization callback fires, the backend calls
`PermissionManager::set_status(...)`, which flips
`frame_needs_regeneration` so the layout callback re-runs.

### 4.2 `App::request_permission` API

```rust
impl App {
    /// Async permission request. The future resolves when the OS
    /// callback fires (iOS) / the user dismisses the system dialog
    /// (Android) / the W3C Permissions API settles (web).
    pub fn request_permission(
        &self,
        permission: Permission,
        prompt: AzString,    // user-visible reason
    ) -> AzFuture<PermissionStatus>;

    /// Synchronous status read (no prompt).
    pub fn permission_status(&self, permission: Permission) -> PermissionStatus;
}
```

`AzFuture` shape is TBD — Rust `Pin<Box<dyn Future>>`, C poll-based
handle. TODO: verify `azul_core::task` has a future-shaped primitive
we can reuse (existing `Thread` manager is close but heavy).

### 4.3 Per-platform plumbing

* **iOS** (`shell2/ios/mod.rs`) — new `permission_objc.rs` submodule
  wrapping PhotoKit / AVCaptureDevice / CLLocationManager / LAContext
  via `objc::msg_send!`. No new framework deps; UIKit + AVFoundation +
  CoreLocation already link. Status reads are sync class methods;
  `request_permission` dispatches the matching `requestAuthorization`
  with a one-shot block that calls back into `PermissionManager`.

* **Android** (`shell2/android/mod.rs`) — `permission_jni.rs` +
  `scripts/android/AzulPermissions.java`. Same `javac + d8` flow as the
  existing `NativeGestureBridge.java`. Wrap
  `ActivityCompat.checkSelfPermission` / `requestPermissions`; activity
  result JNIs back into Rust.

* **macOS** — reuse `permission_objc.rs` cross-target via
  `cfg(any(target_os = "ios", target_os = "macos"))`. Same PhotoKit /
  AVCaptureDevice APIs.

* **Linux / Windows / Web** — most permissions are prompt-less or
  capability-system based. Default Linux to `Granted`; on Windows
  query `Windows.Security.Authorization.AppCapabilityAccess` (MSIX
  apps only). Web → `navigator.permissions.query()` for the few
  that exist.

### 4.4 Photo picker API (not permission-gated)

```rust
#[repr(C)] pub struct PhotoPickerOptions {
    pub allow_multiple: bool, pub max_count: usize,
    pub include_videos: bool, pub allow_camera_capture: bool,
}
impl App { pub fn pick_photos(&self, opts: PhotoPickerOptions) -> AzFuture<Vec<ImageRef>>; }
```

The future resolves on picker dismissal; bytes flow via
`loadDataRepresentation` (iOS) / `ContentResolver.openInputStream`
(Android) → `decode_raw_image_from_any_bytes`. User cancellation maps
to `Ok(vec![])` for parity with web `<input type=file>`.

### 4.5 Font registration API

```rust
impl AppConfig { pub fn register_font_bytes(&mut self, family: AzString, bytes: U8Vec); }
```

Mirrors the static `bundled_fonts: NamedFontVec`. On iOS, additionally
calls `CTFontManagerRegisterGraphicsFont` so system shapers see it.

### 4.6 Codegen / Info.plist / AndroidManifest generation

`dll/build.rs` already configures iOS / Android linking. Natural
follow-up: hand-written `examples/<lang>/Info.plist.in` +
`AndroidManifest.xml.in` templates that the build script merges with
feature-flag-driven additions. TODO: decide whether
`App::request_permission(Permission::Camera, _)` without a manifest
entry should be a build-time fail or a runtime warning.

### 4.7 W3C shape (the integration must remain compatible with)

| Azul | Web back-end |
|---|---|
| `App::request_permission(Camera)` | `navigator.permissions.query({name:'camera'})` + `getUserMedia({video:true})` |
| `App::permission_status(Geolocation)` | `navigator.permissions.query({name:'geolocation'})` |
| `App::pick_photos(...)` | hidden `<input type="file" accept="image/*" multiple>` |
| `AppConfig::register_font_bytes("X", bytes)` | `new FontFace("X", bytes).load()` → `document.fonts.add()` |
| `FontFallbackChain::resolve_char(text)` | CSS `font-family: X, Y, sans-serif` browser cascade |

---

## 5. Risks / gotchas

1. **CoreText TTC handling** — `Helvetica.ttc` carries 14 faces.
   `allsorts::parse_font_faces` already handles `.ttc` collections
   (`lib.rs:3953..3967`). TODO: verify that
   `CTFontManagerCopyAvailableFontURLs` URLs open via `std::fs::File`
   inside the iOS sandbox (Apple docs imply yes).

2. **Scout silent-failure on iOS** — without the iOS arm in
   `multithread::scout_thread`, the scout sees zero dirs and
   `request_fonts` returns empty. We've been masking this with the
   embedded `material-icons` font + `bundled_fonts`. Symptom: blank
   text, no error. TODO: verify by disabling the `icons` feature.

3. **`fonts.xml` schema drift** — AOSP renames `<familyset>`
   versions across Android releases (v23 added `fallbackFor`).
   Heavily-customized ROMs (Samsung One UI, MIUI, EMUI) add
   proprietary attributes; the parser must be permissive (xmlparser
   tolerates unknown attrs by default).

4. **Plist string localization** — `NSPhotoLibraryUsageDescription` is
   user-visible. Framework can't ship one English string; document
   "user must localize" for v1.

5. **App-Review surface** — Apple rejects apps shipping privacy plist
   keys they don't actually invoke. Don't auto-add keys from the
   `Permission` enum presence; require explicit opt-in.

6. **Android "Selected photos" (API 34+)** — analogue of iOS `.limited`.
   `READ_MEDIA_VISUAL_USER_SELECTED`. Expose as `PermissionStatus::Limited`.

7. **Permission revocation mid-run** — OS can yank permissions
   without notifying. Every photo/camera/location callback must
   re-check `permission_status(...)`. CallbackInfo accessor is the
   cross-platform sync point.

8. **`PHPickerViewController` user-cancel** returns an empty
   `[PHPickerResult]`, not an error. Map to `Ok(vec![])` for parity
   with web `<input type=file>` cancellation.

9. **iOS framework linking** — `dll/build.rs::configure_ios` currently
   links `UIKit`, `Foundation`, `CoreGraphics`. Add `-framework CoreText`
   for `CTFontManager*`, and `-framework Photos`, `-framework PhotosUI`
   for PhotoKit / PHPicker.

---

## 6. Implementation sketch & order

Fits inside SUPER_PLAN_2 §4.1. Estimated ~3–4 focused days.

1. **rust-fontconfig mobile arms** (~200 LOC). Add `OperatingSystem::IOS`
   / `Android` variants; iOS arm via `core-text` crate; Android arm
   walks `/system/fonts/` + parses `/system/etc/fonts.xml`. Mirror in
   `multithread.rs::scout_thread` and `config::font_directories`.
   Add a target-cfg'd test asserting non-empty enumeration.

2. **iOS framework linking** in `dll/build.rs`: `-framework CoreText`,
   `-framework Photos`, `-framework PhotosUI` (+ `AVFoundation`,
   `CoreLocation`, `LocalAuthentication`, `CoreMotion` when those
   features land).

3. **`PermissionManager`** (`layout/src/managers/permission.rs`,
   ~150 LOC) — state holder + pending-request queue, wired next to
   existing managers in `LayoutWindow`.

4. **iOS Objective-C bridge** (`shell2/ios/permission_objc.rs`,
   ~250 LOC). One helper per permission (PhotoKit / AVCaptureDevice /
   CLLocationManager / LAContext / CMMotionManager). Block-style
   callbacks update the manager.

5. **Android JNI bridge** (`shell2/android/permission_jni.rs` +
   `scripts/android/AzulPermissions.java`, ~120 LOC each). Same shape
   as the existing `NativeGestureBridge`.

6. **Photo picker** (`shell2/{ios,android}`, ~200 LOC each).
   iOS: `PHPickerViewController` + `loadDataRepresentation`.
   Android: `ActivityResultContracts.PickVisualMedia`.

7. **Codegen** in `doc/api.json`: `Permission`, `PermissionStatus`,
   `PhotoPickerOptions`, `App.requestPermission`,
   `App.permissionStatus`, `App.pickPhotos`,
   `AppConfig.registerFontBytes`. Standard `azul-doc autofix add` +
   `codegen all`.

8. **Smoke test** (`examples/permissions/`): print initial status,
   await `pick_photos({allow_multiple: true})`, print returned count
   and sizes. C + Rust + one binding language.

Biggest two risks (#1 CoreText TTC URLs in sandbox, #2 framework
linking) are smoke-testable in <1 hour via
`scripts/mobile/golden/<feature>.png`.

---

## 7. Open questions

1. **Minimum iOS / Android API levels?** Assumes iOS 14 + Android 13
   for permission-less pickers; older versions need legacy intents +
   permission prompts. TODO: verify `dll/build.rs::configure_ios` min.
2. **Future shape for `request_permission`** — `azul_core::task` future
   primitive vs. callback-based vs. a new
   `Update::WaitForCallback(id)`. Plan §1 says `Future` so this brief
   assumed that.
3. **Long-term: fontique migration?** 1–2 week churn touching
   FontManager + 15 tests. Out of scope here; deferred.
4. **Plist / manifest strategy** — hand-rolled templates per example
   vs. generated from api.json feature flags. Generated is more robust
   but harder to debug.

---

## 8. Citations

* Apple: *App Sandbox Design Guide* (§1.1); *Core Text Programming
  Guide* (§1.1); "Adding a Custom Font to Your App" (§1.1); *PhotoKit
  Reference* + `PHPickerViewController` + WWDC20 session 10652 (§2.2);
  *Information Property List* — Privacy keys (§3.1).
* Android: "Photo picker" / `PickVisualMedia` (§2.3); `Manifest.permission`
  (§3.2); "Downloadable fonts" + `<android/font_matcher.h>` (§1.2).
* AOSP: `frameworks/base/data/fonts/fonts.xml` schema (§1.2).
* W3C: Permissions API, File API (§3.4).
* linebender.org: parley + fontique design (§1.4).
* In-repo:
  `rust-fontconfig/src/{lib,registry,config,multithread}.rs`,
  `dll/src/desktop/shell2/{ios,android}/mod.rs`,
  `core/src/resources.rs`, `layout/src/image.rs`,
  `dll/src/desktop/app.rs`.

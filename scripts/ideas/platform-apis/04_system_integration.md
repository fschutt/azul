# 04 · System integration — file pickers, IME / text input, geolocation

**Scope:** three OS-integration features required to make Azul mobile-shape
complete, inventoried for all five target platforms (iOS, Android, macOS,
Linux, Windows). Sources: `SUPER_PLAN_2.md` §0 / features 8-10, existing
desktop API in `layout/src/desktop/dialogs.rs`, existing macOS
`NSTextInputClient` and Wayland `zwp_text_input_v3` wiring, existing IME
composition surface in `core/src/events.rs` and `layout/src/managers/text_input.rs`.

Citations preserved inline. Items not yet verified marked `TODO: verify`.

---

## 1 · File pickers

### 1.1 What we already have

`/Users/fschutt/Development/azul-mobile/layout/src/desktop/dialogs.rs` exposes
three namespace structs that the codegen lifts into every binding language:

- `MsgBox` — info / ok-cancel / yes-no modal alerts. Backed by `tfd::MessageBox`.
- `FileDialog` — `open_file`, `open_directory`, `open_multiple_files`,
  `save_file`. All return `OptionString` / `OptionStringVec` with a
  `file://`-style path. Backed by `tfd::FileDialog`.
- `ColorPickerDialog` — opens an OS color chooser. Backed by `tfd::ColorChooser`.

Filter type:

```rust
pub struct FileTypeList {
    pub document_types: StringVec,    // ["*.png", "*.jpg"]
    pub document_descriptor: AzString, // "Image files"
}
```

Every method is gated `#[cfg(not(any(target_os = "android", target_os = "ios")))]`;
the mobile arm is a stub that returns `OptionString::None`. The signatures are
identical on every target so consumer code keeps compiling, but the picker is
a no-op on mobile (see lines 142-146, 224-228, 279-283, 296-299, 321-325,
338-342 of `dialogs.rs`). The job is to fill those arms.

`tfd` 0.1 is a thin wrapper around the `tinyfiledialogs` C library; the
crate root explicitly notes `tfd 0.1.0` does not cross-compile for iOS or
Android. `rfd` (rust-file-dialog) does claim mobile support but routes
through the same SAF / `UIDocumentPicker` paths described below — i.e. it is
not free, and pulling it in for desktop would duplicate `tfd`. The cheaper
path is to keep `tfd` for desktop and write the two mobile arms ourselves.

### 1.2 iOS — `UIDocumentPickerViewController`

**Framework:** UIKit, `Foundation` (URL types).
**Class:** `UIDocumentPickerViewController` (replaces deprecated
`UIDocumentMenuViewController` since iOS 8; UTType-based initializer is iOS 14+).
**Headers:** `<UIKit/UIDocumentPickerViewController.h>`,
`<UniformTypeIdentifiers/UTType.h>`.

Open flow:

```objc
// iOS 14+ API (UTType-based)
NSArray<UTType *> *types = @[UTTypeImage, UTTypePDF];
UIDocumentPickerViewController *picker =
    [[UIDocumentPickerViewController alloc]
        initForOpeningContentTypes:types asCopy:YES];
picker.allowsMultipleSelection = YES;
picker.delegate = self;
[viewController presentViewController:picker animated:YES completion:nil];
```

`asCopy:YES` makes iOS copy the picked file into the app's tmp sandbox and
hand you a regular `file://` URL — the caller does not need to hold an iCloud
security-scoped bookmark. Without `asCopy`, the URL is sandbox-scoped and
needs `startAccessingSecurityScopedResource` / `stopAccessing…` brackets, plus
a saved bookmark for re-opens (`URL.bookmarkData(options:.minimalBookmark)`).

Save flow uses the same class with `initForExportingURLs:asCopy:` — you
hand it the file you have already written to tmp, and iOS asks the user where
to put it.

Multi-select returns a `[URL]` via the delegate
`documentPicker(_:didPickDocumentsAt:)`; cancel hits `documentPickerWasCancelled(_:)`.

UTType filter mapping from our `FileTypeList`:
| `document_types` entry | UTType         |
|------------------------|----------------|
| `*.png`                | `UTTypePNG`    |
| `*.jpg`, `*.jpeg`      | `UTTypeJPEG`   |
| `*.pdf`                | `UTTypePDF`    |
| `*.txt`                | `UTTypePlainText` |
| `*.json`               | `UTTypeJSON`   |
| any unknown extension  | `UTType(filenameExtension:)` (iOS 14+) — `TODO: verify` returns nil for unknown UTIs |

Document directory mounted is iCloud Drive + on-device "Files"; Apple does
not let you preselect a default path beyond `directoryURL` on iOS 13+ (and
even that only works for iCloud).

**Permission strings (Info.plist):**
- `NSDocumentsFolderUsageDescription` — required if you read from
  `NSDocumentsDirectory` outside the sandbox; not required for
  `UIDocumentPickerViewController` itself, but good to declare.
- `NSPhotoLibraryUsageDescription` / `NSPhotoLibraryAddUsageDescription` —
  only required if we expose a "pick image from photos" variant (different
  picker — `PHPickerViewController` since iOS 14). Out of scope for the basic
  `FileDialog::open_file`.
- `LSSupportsOpeningDocumentsInPlace = YES` + `UIFileSharingEnabled = YES` —
  optional, allows Files.app to surface files this app owns.

**Risks / gotchas:**
- The picker is sheet-modal and *cannot* be invoked synchronously. The
  current desktop API returns `OptionString` directly; on iOS the call has to
  return a future / completion handler. See §1.7.
- `asCopy:YES` doubles disk usage briefly. Acceptable for image / doc picking,
  but for large media we should use the security-scoped path.
- iCloud documents may not be downloaded — opening a URL can return EAGAIN
  while iOS downloads. `NSFileCoordinator.coordinate(readingItemAt:options:.withoutChanges,...)`
  blocks until ready; the picker handles this for us when `asCopy:YES`.

### 1.3 Android — Storage Access Framework

**API:** `Intent.ACTION_OPEN_DOCUMENT` (`Intent.ACTION_CREATE_DOCUMENT` for
save, `Intent.ACTION_OPEN_DOCUMENT_TREE` for directory). Returns a
`content://com.android.providers.media.documents/document/…` URI, *not* a
`file://` path.

**Min API:** SAF lands in API 19 (KitKat). `ACTION_OPEN_DOCUMENT_TREE` is API 21.
Scoped storage (forced `MediaStore` / SAF for non-app files) starts at API 29
(Android 10) but only enforced fully at API 30 (Android 11). `requestLegacyExternalStorage`
in the manifest is gone at API 30+. The picker is the only safe path going
forward.

Open flow (Java, called over JNI from `android_main`):

```java
Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
i.addCategory(Intent.CATEGORY_OPENABLE);
i.setType("*/*");
i.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{"image/png", "image/jpeg"});
i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
// For persistent access across reboots:
i.addFlags(Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
startActivityForResult(i, REQUEST_PICK);
```

`onActivityResult` returns either `data.getData()` (single) or
`data.getClipData()` (multi). To make the URI usable after process death,
call `contentResolver.takePersistableUriPermission(uri, FLAG_GRANT_READ_URI_PERMISSION)`.

To read the bytes:
```java
InputStream s = contentResolver.openInputStream(uri);  // -> bytes
```

For our `FileDialog::open_file` returning `AzString` (a path), three options:

1. **Copy to app cache** — read the stream into `getCacheDir() + "/" + name`,
   return that `file://` path. Cheap, lossy if the file is huge, but mirrors
   iOS `asCopy:YES`. Recommended default.
2. **Return the `content://` URI as a string** — opaque to the user, but they
   can pass it back to a future `FileDialog::read_bytes(uri) -> Vec<u8>`
   accessor. Lets us avoid copying for large media. Requires that the binding
   languages do not treat the return as a real filesystem path.
3. **Hand back in-memory bytes** — change the return type to
   `Result<Vec<u8>, _>`. Breaks API parity with desktop.

Recommendation: option 1 for the v1 `open_file`, expose `open_file_bytes` as
a separate accessor for callers that want zero-copy.

MIME filter mapping:
| `*.png` | `image/png` |
| `*.jpg` | `image/jpeg` |
| `*.*`   | `*/*` |
| `*.pdf` | `application/pdf` |
| any unknown ext | `application/octet-stream` |

Use `MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext)` for the full
table (Java side), pass the resulting strings to `EXTRA_MIME_TYPES`. Glob-style
filters (`*.tar.gz`) cannot be expressed in SAF — limitation of the framework.

**Permissions (AndroidManifest.xml):**
- *Nothing* for `ACTION_OPEN_DOCUMENT` itself — the picker grants per-URI
  read permission via the intent flags, no manifest entry needed.
- `READ_EXTERNAL_STORAGE` is *deprecated* at API 33+. Replace with
  `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, `READ_MEDIA_AUDIO` (granular media
  permissions, Android 13+). Only needed if we also expose a `MediaStore`-based
  picker for the photo gallery. For `FileDialog::open_file` going through SAF,
  no media permissions required.
- `WRITE_EXTERNAL_STORAGE` is dead at API 30+. Save dialogs use
  `ACTION_CREATE_DOCUMENT`, which grants its own per-URI write permission.

**Risks / gotchas:**
- The "Files" UI on Android is provided by `DocumentsUI.apk`, which varies by
  OEM. Samsung's "My Files" looks different. Behavior is generally consistent.
- `content://` URIs from `MediaStore` can be revoked at any time (e.g., user
  uninstalls the source provider). Persistable permission only survives reboot
  if `FLAG_GRANT_PERSISTABLE_URI_PERMISSION` was passed.
- The picker is sheet-modal and *async-only*. Like iOS — see §1.7.
- `android-activity` (`game-activity` feature) exposes `app.create_waker()` but
  has no built-in `startActivityForResult`. We need a Java shim — same pattern
  as the proposed `NativeInputConnection.java` in `ANDROID_IMPLEMENTATION_PLAN.md`.
  Skeleton: `FilePickerBridge.java` with `pickDocument(long nativePtr, ...)`
  that posts a `Handler` to call `Activity.startActivityForResult` on the UI
  thread, then `onActivityResult` calls back into Rust via
  `Java_com_azul_picker_FilePickerBridge_nativeOnResult`.
- Audio/Video MIME types on Android also need `READ_MEDIA_AUDIO` /
  `READ_MEDIA_VIDEO` at API 33+ *if* the user re-opens via `MediaStore`. SAF
  alone is enough for the first pick.

### 1.4 macOS — already covered by `tfd` (`NSOpenPanel` / `NSSavePanel`)

`tfd` 0.1 on macOS shells out to AppleScript `choose file` /
`choose folder` for `tinyfiledialogs.c::tinyfd_openFileDialog`. This works
but is slow (spawns `osascript`) and has UTF-8 quoting quirks (hence the
`msg.replace('\"', "")` in `dialogs.rs::MsgBox::ok`).

If we want a tighter macOS path:

- Use `objc2`'s `objc2_app_kit::NSOpenPanel` directly. `NSOpenPanel.runModal()`
  returns `NSModalResponseOK`; `URLs` returns `[NSURL]`. `allowedContentTypes`
  takes `[UTType]` (the same UTType type used on iOS, exposed via
  `objc2_uniform_type_identifiers`).
- Hardened-runtime apps must declare the
  `com.apple.security.files.user-selected.read-write` entitlement; without
  it, the sandbox allows the panel to open but refuses to let the app read
  the returned URL.

Recommendation: leave the `tfd` path for now (it works), file an issue to swap
to direct `NSOpenPanel` once we already pull `objc2` into shell2 for the IME
work. The Android / iOS arms are higher priority.

**Entitlements (.entitlements file, for signed builds):**
- `com.apple.security.files.user-selected.read-write` — required in the
  app sandbox to keep the returned URL readable.
- `com.apple.security.files.bookmarks.app-scope` — required to persist
  bookmark URLs across launches.

### 1.5 Linux — XDG portal (Wayland-clean) or GTK / Qt

`tfd` on Linux tries `zenity`, then `kdialog`, then `qarma`, then `Xdialog`
as a fallback chain (see `tinyfiledialogs.c` ~ line 4500). This works on
GNOME / KDE desktops with the matching dialog binary installed. For Wayland
or sandboxed (Flatpak / Snap) shells, the correct path is the
**`org.freedesktop.portal.FileChooser`** D-Bus interface:

```text
service:   org.freedesktop.portal.Desktop
object:    /org/freedesktop/portal/desktop
interface: org.freedesktop.portal.FileChooser
methods:   OpenFile, SaveFile, SaveFiles
```

`OpenFile` takes a parent window handle (`x11:0xWWWWW` or `wayland:HANDLE`
exported via `xdg_foreign`), an a{sv} options dict (multiple, modal, filters,
current_folder…) and returns a `Request` object path; the actual response
arrives as a `Response` signal on that path. The desktop portal then talks to
whichever native dialog the host provides (GTK file chooser on GNOME, Qt on
KDE, etc.) and forwards the user's URI back.

`tfd` keeps working for desktop, so we do not need to do anything new for
Linux at the API level. Optional cleanup: add an XDG-portal backend behind a
feature flag for Flatpak / Snap users where `zenity` is not available.

Existing Rust crate: `ashpd` provides a typed wrapper around all XDG portals;
it pulls in `zbus` (already a transitive dep of several Wayland crates). 
`TODO: verify` whether `ashpd` works without an async runtime — we run a
sync event loop and would prefer not to drag tokio in.

### 1.6 Windows — already covered by `tfd` (`IFileDialog` / Common Item Dialog)

`tfd` calls `GetOpenFileNameW` (legacy `comdlg32.dll`) on Windows. Works on
Vista+ but does not use the modern `IFileDialog` COM API (Windows 7+) which
gives the explorer-style picker. Not a blocker; users see the older "common
dialog" but it functions.

`rfd` (alternative crate) uses `IFileOpenDialog` / `IFileSaveDialog` directly
and gets the modern look. Single-line swap if we want it. `TODO: verify`
that `rfd` does not double our binary size — the COM bindings tend to be
heavyweight.

**Manifest capabilities (UWP / packaged):**
- Win32 desktop: no manifest entry required.
- UWP / WinUI 3 packaged: declare `Pickers` capability if using
  `FileOpenPicker` from `Windows.Storage.Pickers`. We are not (we link
  classical Win32), so n/a.

### 1.7 Integration sketch — sync vs async

The mobile pickers are inherently async (sheet-modal on iOS, intent-result
on Android). The current `FileDialog::open_file(...) -> OptionString` API is
synchronous-blocking. Two options:

**Option A — block on a oneshot channel.**
- Spawn the picker, wait on a `std::sync::mpsc::Receiver<Option<String>>`.
- Works only on a non-UI thread; the desktop API today is called from
  callbacks (UI thread), so this would deadlock on iOS/Android because the
  picker delegate runs *on the same UI thread*.
- Rejected.

**Option B — async future-style return.**
- New API:
  ```rust
  pub struct FilePickerHandle { /* opaque */ }
  impl FileDialog {
      pub fn open_file_async(
          title: AzString,
          default_path: OptionString,
          filter_list: OptionFileTypeList,
      ) -> FilePickerHandle;
  }
  // Polled via:
  impl FilePickerHandle {
      pub fn poll(&mut self) -> FilePickerStatus;
  }
  pub enum FilePickerStatus {
      Pending,
      Cancelled,
      Selected(OptionString),     // file:// path (or copied-to-cache path on Android)
      SelectedMultiple(StringVec),
      Error(AzString),
  }
  ```
- Desktop arms wrap the existing `tfd` sync call into an immediately-resolved
  handle (call `open_file()` on a thread, push the result to a channel).
- Mobile arms hold the OS picker open and write to the handle's shared state
  from the OS callback (`documentPicker(_:didPickDocumentsAt:)` on iOS,
  `onActivityResult` on Android).
- Callbacks check `handle.poll()` on each frame; when `Selected`, fire an
  app-level event (could attach a new `WindowEventFilter::FilePicked` so
  binding languages get a hover-equivalent callback). `TODO: verify` that
  bindings can hold an opaque `FilePickerHandle` across event-loop ticks; the
  `Timer` / `Task` infra already does this so the pattern is precedented.

**Recommendation:** keep the existing sync `FileDialog::open_file(...)` for
desktop *only* (mobile arms remain stubs that immediately return
`OptionString::None`, as today) and ship `open_file_async(...)` as a new
parallel API. Eventually we can deprecate the sync version in favor of
async-on-all-platforms, but that's an API churn we don't need to take this
sprint.

**W3C equivalent:** `<input type="file" multiple accept=".png,.jpg">` plus
the File System Access API (`window.showOpenFilePicker()`) for the modern
async-promise shape. Our async handle maps cleanly onto the
`showOpenFilePicker()` Promise.

---

## 2 · Text input / IME / soft keyboard

### 2.1 What we already have

Composition events are defined in `core/src/events.rs`:

```text
HoverEventFilter::CompositionStart    (line 494)
HoverEventFilter::CompositionUpdate   (line 496)
HoverEventFilter::CompositionEnd      (line 498)
FocusEventFilter::CompositionStart    (line 1618)
FocusEventFilter::CompositionUpdate   (line 1620)
FocusEventFilter::CompositionEnd      (line 1622)
EventType::CompositionStart           (line 1805)
EventType::CompositionUpdate          (line 1807)
EventType::CompositionEnd             (line 1809)
```

`TextInputManager::record_input(...)` (lines 158-174 of
`layout/src/managers/text_input.rs`) accepts a `TextInputSource::Ime` source
so the manager already knows how to ingest an IME-originated edit. The split
between "record" (no mutation) and "apply" (after callbacks observe and may
preventDefault) means an IME commit is a regular `record_input` call with
`source = Ime`.

The platform side has *partial* wiring:

| Platform | IME path                                              | Status |
|----------|-------------------------------------------------------|--------|
| macOS    | `NSTextInputClient` protocol on `GLView`/`CPUView`    | Wired. `setMarkedText:selectedRange:replacementRange:` (`mod.rs:782`), `insertText:replacementRange:` (`mod.rs:853`), `unmarkText` (`mod.rs:825`), `markedRange` (`mod.rs:758`), `hasMarkedText` (`mod.rs:748`). Calls `text_edit_manager.set_preedit(...)` and `text_edit_manager.clear_preedit()`. |
| Linux/X11| XIM (X Input Method)                                  | *Not wired yet.* `XFilterEvent` mentioned in `event.rs:116` but call site `TODO: verify`. |
| Linux/Wayland | `zwp_text_input_v3`                              | Listener bound (`linux/wayland/events.rs:805+`), six event handlers (preedit_string, commit_string, delete_surrounding_text, done, enter, leave) declared as `_text_input: *mut zwp_text_input_v3` with empty bodies — need to push into `TextEditManager`. |
| Windows  | WM_CHAR (basic ASCII input)                          | Wired in win32 events, but no IME (`WM_IME_COMPOSITION` / `WM_IME_STARTCOMPOSITION` / TSF) yet. |
| Android  | none — only physical KeyEvent → VirtualKeyCode map   | `dll/src/desktop/shell2/android/mod.rs:520-528`. `unicode_char` (KeyCharacterMap) marked TODO, no `InputConnection` / JNI bridge yet. |
| iOS      | none                                                  | View has no UIKeyInput / UITextInput conformance yet. |

The TextEditManager (`layout/src/managers/text_edit.rs:108-110, 142, 227-237`)
already owns `preedit_text: Option<String>`, `set_preedit(text, start, end)`,
`clear_preedit()`. So the plumbing on the layout side is done; the platform
side is what is missing for Android, iOS, and to a lesser degree Wayland +
Windows.

### 2.2 iOS — `UIKeyInput` vs `UITextInput`

**Framework:** UIKit.
**Two protocols, ascending in complexity:**

#### 2.2.1 `UIKeyInput` (recommended first pass)

Three methods + `canBecomeFirstResponder`. Sketch from
`scripts/IOS_IMPLEMENTATION_PLAN.md` (lines 440-491) is essentially correct:

```rust
extern "C" fn can_become_first_responder(_: &Object, _: Sel) -> bool { true }
extern "C" fn has_text(_: &Object, _: Sel) -> bool {
    // query text_edit_manager.has_text()
}
extern "C" fn insert_text(_: &Object, _: Sel, text: *mut Object) {
    // NSString → &str → window.handle_text_input(str)
    // which calls text_input_manager.record_input(node, str, old, Keyboard)
}
extern "C" fn delete_backward(_: &Object, _: Sel) {
    // window.handle_key_down(VirtualKeyCode::Back)
}
```

Plus protocol conformance via `decl.add_protocol(Protocol::get("UIKeyInput")?)`.

Show keyboard: `becomeFirstResponder` on the view. Hide: `resignFirstResponder`.

**Pros:**
- ~50 LOC, gets us ASCII / Latin text input today.
- Works with hardware Bluetooth keyboards out of the box.
- Most demo apps only need this.

**Cons:**
- No IME preedit (no `CompositionStart` / `CompositionUpdate` /
  `CompositionEnd`).
- CJK / Korean / Vietnamese / Devanagari users see the keyboard but the
  candidate-strip selection commits to `insertText` with no preedit phase.
  Marginally usable for short input but broken for long composition.

#### 2.2.2 `UITextInput` (full IME)

UITextInput is a 20-method protocol that lets iOS run a real IME against
your view. The methods are:

| Property / method | Purpose |
|-------------------|---------|
| `selectedTextRange: UITextRange?`  | Get/set selection |
| `markedTextRange: UITextRange?`    | Get IME preedit range |
| `markedTextStyle: [NSAttributedString.Key:Any]?` | Visual style for preedit |
| `setMarkedText(_:selectedRange:)`  | Set preedit text (`CompositionStart`/`Update`) |
| `unmarkText()`                     | Commit preedit (`CompositionEnd`) |
| `beginningOfDocument: UITextPosition` | Anchor for offset math |
| `endOfDocument: UITextPosition`    | ditto |
| `textRange(from:to:)`              | Build a range |
| `position(from:offset:)`           | Position math |
| `position(from:in:offset:)`        | Position with direction |
| `compare(_:to:)`                   | Order positions |
| `offset(from:to:)`                 | Distance |
| `tokenizer: UITextInputTokenizer`  | Word / line break (default impl OK) |
| `position(within:furthestIn:)`     | Layout-direction-aware nav |
| `text(in:)`                        | Read text in range |
| `replace(_:withText:)`             | Programmatic edit |
| `firstRect(for:)`                  | Cursor rect for an IME's candidate window |
| `caretRect(for:)`                  | Single-point caret rect |
| `closestPosition(to:)`             | Hit test (point → text position) |
| `closestPosition(to:within:)`      | Hit test bounded |
| `characterRange(at:)`              | Word at point |
| `inputDelegate: UITextInputDelegate?` | iOS sets this; we ping it on edits |

`UITextPosition` and `UITextRange` are *abstract* classes — we have to
subclass them in ObjC. The simplest backing: store a `u32` offset (UTF-16
code unit index, because `UITextInput` is UTF-16-indexed by spec) and let
the subclass be a one-property wrapper.

`TODO: verify` how big the actual code lands. The macOS `NSTextInputClient`
implementation in `dll/src/desktop/shell2/macos/mod.rs:745-950` is ~200 LOC
not counting the `objc2`-related glue. iOS `UITextInput` is roughly twice
the protocol surface, so estimate 300-500 LOC of `extern "C"` shims plus
two `decl_class!` invocations for `AzulTextPosition` and `AzulTextRange`.

**Event mapping:**
| iOS callback                       | Azul action                                          |
|------------------------------------|------------------------------------------------------|
| `setMarkedText(text, range)` (first call) | `text_edit_manager.set_preedit(text, ...)` + emit `EventType::CompositionStart` |
| `setMarkedText(text, range)` (subsequent) | `text_edit_manager.set_preedit(text, ...)` + emit `EventType::CompositionUpdate` |
| `unmarkText()` then `insertText(committed)` | `text_edit_manager.clear_preedit()` + `text_input_manager.record_input(node, committed, old, Ime)` + emit `EventType::CompositionEnd` |
| `replace(range, with: text)`       | `text_input_manager.record_input(node, text, old_in_range, Ime)` |
| `text(in: range)`                  | Query `text_edit_manager.get_text_in_range(range)` |
| `firstRect(for: range)`            | Query text-layout — return the rect of the first char of `range` so iOS can position its candidate window |
| `caretRect(for: position)`         | Cursor rect at offset |

**Show / hide keyboard:** `becomeFirstResponder` / `resignFirstResponder`,
same as UIKeyInput. The IME (system keyboard switcher) decides whether to
show a candidate bar.

**Soft-keyboard-aware layout:** subscribe to
`UIKeyboardWillShowNotification` / `UIKeyboardWillHideNotification` on
`NotificationCenter.default`, read `UIKeyboardFrameEndUserInfoKey` (a
`CGRect`), shrink the view's bottom safe-area inset to that height. Without
this, the keyboard covers the focused input. `TODO: verify` whether
`UIScrollView.adjustForKeyboard` could handle this for us if we host inside
a UIScrollView — currently we render directly into a custom UIView so we
need the manual handling.

**Info.plist:** no IME-specific entries required. Default Apple system IMEs
just work. Third-party IMEs (Gboard, SwiftKey) ask the user for "Allow Full
Access" — this is the user's keyboard-app configuration, not ours.

**Implementation order:**
1. Ship UIKeyInput today (covers ASCII / hardware-keyboard cases). Wire to
   `text_input_manager.record_input(..., TextInputSource::Keyboard)`.
2. Promote to UITextInput in a follow-up sprint when CJK support becomes a
   declared requirement.

### 2.3 Android — `BaseInputConnection` over JNI

The skeleton from `scripts/ANDROID_IMPLEMENTATION_PLAN.md` Phase 5 (lines
636-744) is correct in shape but lacks the IME-preedit story. Filling that
in:

#### 2.3.1 Java shim — `scripts/android/NativeInputConnection.java`

```java
package com.azul.input;

import android.view.View;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

public class NativeInputConnection extends BaseInputConnection {
    private final long nativePtr;

    public NativeInputConnection(View view, boolean fullEditor, long nativePtr) {
        super(view, fullEditor);
        this.nativePtr = nativePtr;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        nativeCommitText(nativePtr, text.toString(), newCursorPosition);
        return true;
    }

    @Override
    public boolean setComposingText(CharSequence text, int newCursorPosition) {
        nativeSetComposingText(nativePtr, text.toString(), newCursorPosition);
        return true;
    }

    @Override
    public boolean finishComposingText() {
        nativeFinishComposingText(nativePtr);
        return true;
    }

    @Override
    public boolean deleteSurroundingText(int beforeLength, int afterLength) {
        nativeDeleteSurrounding(nativePtr, beforeLength, afterLength);
        return true;
    }

    @Override
    public boolean sendKeyEvent(android.view.KeyEvent event) {
        // Hardware key bypass — let the existing KeyEvent path handle it.
        nativeSendKeyEvent(nativePtr, event.getAction(), event.getKeyCode(),
                            event.getUnicodeChar());
        return true;
    }

    private static native void nativeCommitText(long ptr, String text, int cursor);
    private static native void nativeSetComposingText(long ptr, String text, int cursor);
    private static native void nativeFinishComposingText(long ptr);
    private static native void nativeDeleteSurrounding(long ptr, int before, int after);
    private static native void nativeSendKeyEvent(long ptr, int action, int code, int unicode);
}
```

`setComposingText` is the IME preedit hook (CJK candidate strip).
`commitText` fires when the user accepts a candidate.
`finishComposingText` is "I gave up composing without selecting anything".

#### 2.3.2 View shim — `onCreateInputConnection`

`NativeActivity` does not give us an Android `View` directly (the activity
*is* a `View` of sorts but `onCreateInputConnection` isn't called on it by
default). The accepted pattern is to spawn a `GameActivity` (which the
`android-activity` crate exposes via `game-activity` feature) and override
its `onCreateInputConnection(EditorInfo)`:

```java
@Override
public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
    outAttrs.inputType = InputType.TYPE_CLASS_TEXT;
    outAttrs.imeOptions = EditorInfo.IME_ACTION_DONE;
    return new NativeInputConnection(this, true, nativeGetWindowPtr());
}
```

This is part of the `GameActivity.java` that the `android-activity` crate
ships pre-baked, so we either subclass `GameActivity` in our app build or
patch it post-build. Patching is easier (the user does not need to write
Java themselves). `TODO: verify` whether `android-activity`'s
`GameActivity.java` is in fact open to subclassing — if not, we have to
ship our own `AzulGameActivity.java`.

#### 2.3.3 Rust JNI side

```rust
#[no_mangle]
pub extern "system" fn Java_com_azul_input_NativeInputConnection_nativeCommitText(
    env: JNIEnv,
    _class: JClass,
    native_ptr: jlong,
    text: JString,
    _cursor: jint,
) {
    let window = unsafe { &mut *(native_ptr as *mut AndroidWindow) };
    let text_str: String = env.get_string(&text).expect("commitText utf8").into();
    if let Some(ref mut lw) = window.common.layout_window {
        // End any in-progress preedit, then commit
        lw.text_edit_manager.clear_preedit();
        // Push through text_input_manager (will fire Input event)
        let editing = lw.text_edit_manager.get_editing_node_id();
        if let Some(node) = editing {
            let old = lw.text_edit_manager.get_text_for_node(node);
            lw.text_input_manager.record_input(node, text_str, old, TextInputSource::Ime);
        }
    }
    window.process_window_events(0);
}

#[no_mangle]
pub extern "system" fn Java_com_azul_input_NativeInputConnection_nativeSetComposingText(
    env: JNIEnv,
    _class: JClass,
    native_ptr: jlong,
    text: JString,
    _cursor: jint,
) {
    let window = unsafe { &mut *(native_ptr as *mut AndroidWindow) };
    let text_str: String = env.get_string(&text).expect("composing utf8").into();
    if let Some(ref mut lw) = window.common.layout_window {
        let was_composing = lw.text_edit_manager.preedit_text.is_some();
        lw.text_edit_manager.set_preedit(text_str.clone(), 0, text_str.len() as i32);
        // Emit synthetic CompositionStart / CompositionUpdate
        // (handled via TextEditManager event queue; see TextInputManager pattern)
    }
    window.process_window_events(0);
}

#[no_mangle]
pub extern "system" fn Java_com_azul_input_NativeInputConnection_nativeFinishComposingText(
    _env: JNIEnv, _class: JClass, native_ptr: jlong,
) {
    let window = unsafe { &mut *(native_ptr as *mut AndroidWindow) };
    if let Some(ref mut lw) = window.common.layout_window {
        lw.text_edit_manager.clear_preedit();
    }
    window.process_window_events(0);
}
```

#### 2.3.4 Show / hide soft keyboard

`android-activity` (game-activity feature) exposes:
- `app.show_soft_input(implicit: bool)` — show. `implicit=false` → forced.
- `app.hide_soft_input(implicit: bool)` — hide.

These wrap `InputMethodManager.showSoftInput(view, ...)` /
`hideSoftInputFromWindow`. Trigger on focus change:
- Node with `text_edit_manager` registers as editing → call `show_soft_input`.
- Focus leaves text node → call `hide_soft_input`.

`TODO: verify` exact method names on the current `android-activity` crate
version pinned in our `Cargo.toml`.

#### 2.3.5 Hardware-keyboard `KeyCharacterMap` (orthogonal)

The TODO at `dll/src/desktop/shell2/android/mod.rs:524`:

```rust
// unicode_char mapping (KeyCharacterMap) still TODO
```

Resolution: `android-activity`'s `KeyEvent` exposes `get_unicode_char()`. For
each `KeyAction::Down` with a printable char (`> 0` and not combining),
push:
```rust
let ch = k.get_unicode_char(0);  // 0 = no modifier mask, returns u32
if ch != 0 {
    if let Some(c) = char::from_u32(ch) {
        // Fire EventType::TextInput with c.to_string()
        if let Some(node) = lw.text_edit_manager.get_editing_node_id() {
            let old = lw.text_edit_manager.get_text_for_node(node);
            lw.text_input_manager.record_input(
                node, c.to_string(), old, TextInputSource::Keyboard,
            );
        }
    }
}
```

This is *separate* from the `InputConnection` path: it handles hardware
keyboards (Bluetooth, dock keyboards) that bypass the IME. Both paths feed
the same `TextInputManager`.

**Permissions / manifest:** no IME permission needed. Just declare the
activity is `windowSoftInputMode="adjustResize"` so the OS reshrinks the
view bounds when the keyboard appears.

### 2.4 macOS — already wired

Reference implementation in `dll/src/desktop/shell2/macos/mod.rs:745-953`
covers all six relevant `NSTextInputClient` methods. Both `GLView` (OpenGL)
and `CPUView` (CPU rasterizer) declare conformance:
```rust
unsafe impl NSTextInputClient for GLView {}   // line 957
unsafe impl NSTextInputClient for CPUView {}  // line 1802
```

What is missing relative to the proposed cross-platform model: the macOS
code calls `text_edit_manager.set_preedit` / `clear_preedit` directly but
does *not* emit `EventType::CompositionStart` / `CompositionUpdate` /
`CompositionEnd` events. The composition surface in `core/src/events.rs`
exists but no platform emits them yet. To unify with the proposed iOS /
Android flow:

```rust
// In set_marked_text (macos/mod.rs:782):
let was_composing = lw.text_edit_manager.preedit_text.is_some();
lw.text_edit_manager.set_preedit(preedit, ...);
let event = if was_composing { CompositionUpdate } else { CompositionStart };
// fire via the same EventProvider pattern TextInputManager already uses
```

```rust
// In unmark_text (macos/mod.rs:825) and on commit:
lw.text_edit_manager.clear_preedit();
// fire CompositionEnd
```

This is a small follow-up; the underlying hook works today and Japanese /
Chinese composition is already usable on macOS.

### 2.5 Linux — XIM (X11) + `zwp_text_input_v3` (Wayland)

#### 2.5.1 X11 — XIM

Path: open an `XIM` with `XOpenIM(display, db, res_name, res_class)`, create
an `XIC` per window with `XCreateIC(im, …)`, route `KeyPress` events
through `XFilterEvent(event, window)` *before* dispatching them to our
handler. If `XFilterEvent` returns `True`, the event was consumed by the
IME — do not process. If `False`, call `Xutf8LookupString(xic, &event,
buf, ...)` which returns either a string (commit) or an `XIMStatus`
indicating preedit.

Preedit callbacks (`XIMPreeditDrawCallback`, `XIMPreeditCaretCallback`,
`XIMPreeditStartCallback`, `XIMPreeditDoneCallback`) need to be registered
via `XVaCreateNestedList(XNPreeditAttributes, ...)`. The draw callback
gives us the preedit string and its color attributes. From there, push
into `text_edit_manager.set_preedit(...)` as on macOS.

Existing code: `event.rs:116` references `XFilterEvent` as a comment.
`TODO: verify` whether our X11 backend already wires `XFilterEvent` or if
that's a follow-up.

Recommendation: ship the minimal commit-string path first (route key
events through `XFilterEvent`, take `Xutf8LookupString`'s returned bytes as
a `TextInput` event). Preedit callbacks are a separate sprint.

#### 2.5.2 Wayland — `zwp_text_input_v3`

Already bound in `dll/src/desktop/shell2/linux/wayland/events.rs:803-928`.
Six events:

| Event              | Hook                                                 |
|--------------------|------------------------------------------------------|
| `enter`            | Window got keyboard focus — call `enable` request and `commit` |
| `leave`            | Window lost focus — call `disable` and `commit`     |
| `preedit_string`   | `text_edit_manager.set_preedit(text, cursor_begin, cursor_end)` |
| `commit_string`    | `text_input_manager.record_input(node, text, old, TextInputSource::Ime)` |
| `delete_surrounding_text` | Delete N chars before/after cursor (no current accessor — `TODO: verify` how `text_edit_manager` exposes "delete N chars before cursor") |
| `done`             | Flush — call `process_window_events(0)`. This is the IME committing a batch. |

The bodies of all six handlers in `events.rs:805-928` are currently empty
placeholders. Filling them in is a clean sprint of ~80 LOC.

To activate the protocol on focus, we also need to:
- Call `text_input_v3.enable()` when a node with `contenteditable` /
  `text_edit_manager` gets focus.
- Call `text_input_v3.set_cursor_rectangle(x, y, w, h)` so IBus / Fcitx
  positions the candidate window correctly.
- Call `text_input_v3.commit()` after `enable` / `disable` /
  `set_content_type` / `set_cursor_rectangle` — the protocol is
  double-buffered.

`set_content_type` hints (purpose, hint flags) — already enumerated in
`defines.rs:498-525`. Useful for "this is a password field" → no
suggestions / autocorrect, but we can ship without it.

### 2.6 Windows — TSF vs WM_IME_*

Current state: `WM_CHAR` (basic ASCII) is wired in `win32/events.rs`. Full
IME requires *either* TSF (modern, Vista+) or the legacy `WM_IME_*` set
(Windows 95+).

#### 2.6.1 Legacy IMM32 — `WM_IME_COMPOSITION`

Cheaper to implement. Three messages:
- `WM_IME_STARTCOMPOSITION` (`0x010D`) — fire `CompositionStart`.
- `WM_IME_COMPOSITION` (`0x010F`) — `wParam` is the trigger key,
  `lParam` is a bitmask of `GCS_COMPSTR` / `GCS_COMPATTR` / `GCS_RESULTSTR`.
  Use `ImmGetContext(hwnd)` → `ImmGetCompositionStringW(himc, ...)` to
  read either the preedit (`GCS_COMPSTR`) or the committed string
  (`GCS_RESULTSTR`). Release with `ImmReleaseContext`.
- `WM_IME_ENDCOMPOSITION` (`0x010E`) — fire `CompositionEnd`,
  `clear_preedit()`.

This is the path used by Chrome / Firefox on Windows for non-TSF IMEs and
works fine for Pinyin / IME Pad / Japanese input. Recommended first cut.

#### 2.6.2 TSF — `ITextStoreACP2`

Newer (Vista+). Required for some advanced IMEs (Han Unification, voice
typing in Win11). Implementing TSF requires:
- An `ITextStoreACP2` COM object on our window
- Registering with `ITfThreadMgr2` /`ITfDocumentMgr`
- Per-edit `RequestLock` reentry handling

This is ~1000 LOC of COM glue and is *not* worth shipping until users
report broken Korean / advanced-Pinyin input. Defer.

**Manifest / capability:** none. Plain Win32 + IMM32 just works.

### 2.7 Integration sketch — composition event surface

Concrete plan to wire the composition events from §2.1 across all five
platforms, mirroring the desktop-managers pattern (gesture, focus, text-input):

1. Add to `TextEditManager` (`layout/src/managers/text_edit.rs`):
   ```rust
   pub fn set_preedit_with_event(
       &mut self, text: String, start: i32, end: i32,
   ) -> Option<SyntheticEvent>;  // returns CompositionStart on first, CompositionUpdate after
   pub fn clear_preedit_with_event(&mut self) -> Option<SyntheticEvent>;  // CompositionEnd
   ```
2. Have all five platforms call these instead of the bare `set_preedit` /
   `clear_preedit`. macOS gets a one-line change. Wayland / X11 / Windows /
   Android / iOS get fresh wiring.
3. `TextEditManager` implements `EventProvider` (parallel to
   `TextInputManager`'s existing impl) so `get_pending_events()` returns the
   composition events at the right tick.
4. Event filters already exist (HoverEventFilter / FocusEventFilter
   `CompositionStart/Update/End`), so binding-language callbacks already
   have the surface. No api.json change needed for the event side.

**W3C equivalents:**
- HTML: `<input type="text">` + `contenteditable="true"`
- Events: `compositionstart`, `compositionupdate`, `compositionend`
  (CompositionEvent.data carries the preedit string).
- Soft keyboard: `inputmode` attribute (text / numeric / decimal / email /
  url) — we can map this through to `EditorInfo.inputType` on Android and
  `keyboardType` on iOS in a follow-up.

---

## 3 · Geolocation

### 3.1 iOS — `CLLocationManager`

**Framework:** `CoreLocation.framework`.
**Class:** `CLLocationManager`. Delegate: `CLLocationManagerDelegate`.

Two-step UX:

1. Instantiate the manager, set its delegate, call
   `requestWhenInUseAuthorization` (foreground only) or
   `requestAlwaysAuthorization` (background — requires app to be in the
   "Location" background mode).
2. iOS calls back `locationManager(_:didChangeAuthorization:)` with one of
   `notDetermined`, `restricted`, `denied`, `authorizedWhenInUse`,
   `authorizedAlways`.
3. On `authorizedWhenInUse` (or `authorizedAlways`), call
   `startUpdatingLocation()`. Updates arrive via
   `locationManager(_:didUpdateLocations:)` with a `[CLLocation]`.

`CLLocation` fields we want to surface:
- `coordinate.latitude` / `coordinate.longitude` (`CLLocationDegrees` = `f64`)
- `altitude` (`CLLocationDistance` = `f64`, meters)
- `horizontalAccuracy` (radius in meters; `-1` = invalid)
- `verticalAccuracy`
- `timestamp` (`Date`)
- `speed` (m/s; negative = invalid)
- `course` (true-north heading)

Accuracy levels (`CLLocationAccuracy = Double`):
- `kCLLocationAccuracyBestForNavigation`  — driving / running, GPS-heavy
- `kCLLocationAccuracyBest`               — best available
- `kCLLocationAccuracyNearestTenMeters`   — 10 m
- `kCLLocationAccuracyHundredMeters`      — 100 m
- `kCLLocationAccuracyKilometer`          — 1 km
- `kCLLocationAccuracyThreeKilometers`    — 3 km (least battery)
- `kCLLocationAccuracyReduced`            — iOS 14+, user-selectable "precise
  off" mode; returns ~5 km fuzzed coordinates

iOS 14 introduced **precise vs reduced** as a user toggle in Settings → 
Privacy → Location Services → [app]. We must call
`locationManager.accuracyAuthorization` after auth to know which we got.
`requestTemporaryFullAccuracyAuthorization(withPurposeKey:)` can ask for a
one-time precise upgrade.

**Info.plist (required):**
- `NSLocationWhenInUseUsageDescription` — string shown in the auth dialog.
  Mandatory for `requestWhenInUseAuthorization`; missing key = crash on call.
- `NSLocationAlwaysAndWhenInUseUsageDescription` — for `requestAlwaysAuthorization`.
- `NSLocationTemporaryUsageDescriptionDictionary` — dict of purpose keys for
  iOS 14 temporary-precise prompts.

For background updates additionally:
- `UIBackgroundModes` array containing `location`.

**Existing Rust crate:** none that wraps `CLLocationManager` cleanly for
both iOS and macOS. `core-location` on crates.io has 100 downloads and was
last updated 2017. Roll our own via `objc2_core_location` (the `objc2`
crate family already covers it — `TODO: verify` the exact crate name in
the `objc2` ecosystem; might be `objc2-core-location`).

**Risks:**
- Simulator: the simulator can fake a fixed lat/lon via `Features →
  Location → Custom Location`. Useful for tests. Real-device testing
  requires actual GPS lock — first fix can take 30+ seconds outdoors.
- Background updates eat battery — accuracy=Best + always-on is the
  classic "battery drain" complaint. Default to `kCLLocationAccuracyHundredMeters`
  unless user code requests better.

### 3.2 Android — `LocationManager` (system) vs `FusedLocationProviderClient`

**Two implementations, pick one:**

#### 3.2.1 System `LocationManager` (recommended)

**Package:** `android.location.LocationManager`.
**Pros:** No Google Play Services dependency. Works on AOSP-only devices
(Huawei, Amazon Fire, GrapheneOS).
**Cons:** Less polished — switches between GPS / network providers manually,
no battery-aware fusing of sensors.

Flow (Java / JNI):
```java
LocationManager lm = (LocationManager) ctx.getSystemService(Context.LOCATION_SERVICE);
LocationListener listener = new LocationListener() {
    @Override public void onLocationChanged(Location loc) { nativeOnFix(...); }
    @Override public void onProviderEnabled(String p) {}
    @Override public void onProviderDisabled(String p) {}
};
lm.requestLocationUpdates(
    LocationManager.GPS_PROVIDER,    // or NETWORK_PROVIDER, or FUSED_PROVIDER (API 31+)
    1000L,                            // minimum time between updates (ms)
    1.0f,                             // minimum distance (m)
    listener
);
```

API 31 added `LocationManager.FUSED_PROVIDER` which gives Google-style
fusion without pulling Play Services. Use it when available, fall back to
`GPS_PROVIDER` / `NETWORK_PROVIDER` on older devices.

`Location` fields: `latitude`, `longitude`, `altitude`, `accuracy` (m),
`time` (epoch ms), `speed` (m/s), `bearing` (degrees from true north),
`verticalAccuracyMeters` (API 26+), `bearingAccuracyDegrees`.

#### 3.2.2 `FusedLocationProviderClient` (skip)

Lives in `com.google.android.gms.location`, which requires Google Play
Services. On the AOSP-only path we just listed, this crate isn't even
available. Forcing it on users would lock us out of those devices. Skip.

#### 3.2.3 Permissions (AndroidManifest.xml)

```xml
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION"/>
<uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION"/>
<!-- Background updates: -->
<uses-permission android:name="android.permission.ACCESS_BACKGROUND_LOCATION"/>
```

Runtime permission flow (API 23+):
```java
if (ContextCompat.checkSelfPermission(this, ACCESS_FINE_LOCATION) != PERMISSION_GRANTED) {
    ActivityCompat.requestPermissions(this,
        new String[]{ACCESS_FINE_LOCATION, ACCESS_COARSE_LOCATION},
        REQUEST_LOCATION);
}
```

**Android 12+ "approximate vs precise":** the prompt shows two
buttons — "Precise" (fine) and "Approximate" (coarse). User can downgrade
in Settings later. Check `ACCESS_FINE_LOCATION` at runtime to know which.

**Android 10+ background location:** if the app needs updates while
backgrounded, you must *first* request foreground (fine/coarse), then
separately prompt for `ACCESS_BACKGROUND_LOCATION`. The second prompt
opens a Settings page, not a dialog — UX is ugly. Defer.

#### 3.2.4 Show / hide soft keyboard analog

n/a for geolocation — the app subscribes, the OS pushes fixes.

### 3.3 macOS — same `CLLocationManager`

Identical API to iOS. Differences:
- Auth methods: `requestAlwaysAuthorization` only (no "when in use" prompt
  on Catalina+; that pattern is iOS-only). Use this and accept the system
  dialog.
- Info.plist key: `NSLocationUsageDescription` (singular). macOS Big Sur+
  also accepts `NSLocationAlwaysUsageDescription`.
- Entitlement: `com.apple.security.personal-information.location` —
  required for sandboxed apps.

Same `CLLocation` fields, same accuracy enum. Code-sharing with iOS via
the `objc2` crate is trivial.

**Risks:**
- macOS users have a "Location Services" master toggle in System Settings.
  If off, `CLLocationManager` returns `denied` without prompting.

### 3.4 Linux — GeoClue D-Bus

**Service:** `org.freedesktop.GeoClue2` on the session bus.
**Spec:** <https://www.freedesktop.org/software/geoclue/docs/>

Flow:
1. `GetClient` on `org.freedesktop.GeoClue2.Manager` returns a client
   object path.
2. Set the client's `DesktopId` property (required — GeoClue uses it to
   look up `/etc/geoclue/geoclue.conf` for per-app allow-listing).
3. Set `DistanceThreshold` (meters) and `TimeThreshold` (seconds) on the
   client.
4. Set `RequestedAccuracyLevel` — one of:
   - `0` (none), `1` (country), `2` (city), `3` (neighborhood),
     `4` (street), `5` (exact / GPS, requires a GPS device or
     unattended-access permission).
5. `Start` on the client. GeoClue starts emitting `LocationUpdated`
   signals (old `Location` object path, new `Location` object path).
6. Read properties from the new Location object: `Latitude`, `Longitude`,
   `Accuracy` (meters), `Altitude`, `Speed`, `Heading`, `Timestamp`,
   `Description`.

**Backends:** GeoClue uses Mozilla Location Service (WiFi-based geolocation),
WiFi MAC lookup, modem GPS (ofono / ModemManager), or actual GPS hardware
on supported laptops (rare). Accuracy depends on what backends are
installed and licensed — on most desktop Linux installs it falls back to
IP-based geolocation via MLS, which gives city-level accuracy at best.

**Permissions:** GeoClue gates access in `/etc/geoclue/geoclue.conf`:
```ini
[whitelist]
mybinary=true
```
*or* the user clicks "Allow" in the GNOME / KDE indicator. Sandboxed
(Flatpak) apps get prompted via the `org.freedesktop.portal.Location`
portal — same flow as the file-chooser portal.

**Existing Rust crate:** `geoclue2` (low-downloads but reasonable),
`ashpd::desktop::location` for the portal path. `zbus` is the underlying
D-Bus crate; we already touch it transitively via Wayland. `TODO: verify`
whether sync zbus calls block the UI loop — we'd want async-via-channel
like the file picker.

**Risks:**
- GeoClue is *not* installed by default on every distro. Arch / minimal
  installs may lack it. Detect via `GetNameOwner("org.freedesktop.GeoClue2")`
  on the bus; if `org.freedesktop.DBus.Error.NameHasNoOwner`, return
  `Unsupported`.
- IP-based accuracy is ~10 km. For a map widget this is enough for "what
  city am I in?" but not for turn-by-turn nav. Expected.

### 3.5 Windows — `Windows.Devices.Geolocation.Geolocator`

**Namespace:** `Windows.Devices.Geolocation`. WinRT API.
**Class:** `Geolocator`.

```csharp
var geo = new Geolocator { DesiredAccuracy = PositionAccuracy.High };
var perm = await Geolocator.RequestAccessAsync();  // pops permission dialog
if (perm == GeolocationAccessStatus.Allowed) {
    geo.PositionChanged += (s, args) => {
        var pos = args.Position.Coordinate;
        // pos.Point.Position.Latitude, .Longitude, .Altitude
        // pos.Accuracy (m), .Heading, .Speed, .Timestamp
    };
}
```

**`PositionAccuracy`:** `Default` (~500 m), `High` (~10 m, uses GPS if
available). Win10+ also exposes `DesiredAccuracyInMeters: u32` for
finer control.

**Capabilities (Package.appxmanifest, UWP / WinUI 3):**
```xml
<Capabilities>
  <DeviceCapability Name="location"/>
</Capabilities>
```

For Win32 desktop (our case), no capability declaration needed. The
`Geolocator` still prompts at first call:
- Windows 10/11: settings → Privacy → Location → master toggle.
- If master toggle off, `RequestAccessAsync` returns `Denied`.

**Existing Rust crate:** `windows` (the official Microsoft WinRT crate)
exposes `Windows::Devices::Geolocation::Geolocator` directly. Already a
common dep for any Win11 stuff. `TODO: verify` async support — `Geolocator`
returns `IAsyncOperation<GeolocationAccessStatus>`; we need to spin a
single-threaded apartment (`RoInitialize(RO_INIT_SINGLETHREADED)`) to use
it from the UI thread.

`winrt-geolocation` (older crate) wraps the same API but predates the
official `windows` crate; do not use.

**Risks:**
- The "high accuracy" path requires either a GPS chip (most laptops do
  not have one) or a Wi-Fi card with location services enabled. Falls
  back to IP-geolocation otherwise.
- ARM64 Windows (Surface Pro X, etc.) lacks the same WiFi-positioning
  database — accuracy is worse.

### 3.6 Integration sketch — `GeolocationManager` + event surface

Mirroring the gesture / pen / text-input manager pattern:

```rust
// layout/src/managers/geolocation.rs

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LocationFix {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_meters: f64,            // NaN if unknown
    pub horizontal_accuracy_meters: f32, // -1 if unknown
    pub vertical_accuracy_meters: f32,
    pub speed_meters_per_sec: f32,       // -1 if unknown
    pub heading_degrees: f32,            // -1 if unknown
    pub timestamp_unix_ms: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, u8)]
pub enum LocationAccuracy {
    Best,
    NearestTenMeters,
    HundredMeters,
    Kilometer,
    ThreeKilometers,
    Reduced,  // user-selected privacy mode
}

#[derive(Debug, Clone, Copy)]
#[repr(C, u8)]
pub enum LocationAuthStatus {
    NotDetermined,
    Restricted,
    Denied,
    AuthorizedWhenInUse,
    AuthorizedAlways,
}

pub struct GeolocationManager {
    pub auth_status: LocationAuthStatus,
    pub current_fix: Option<LocationFix>,
    pub fix_history: Vec<LocationFix>,    // ring buffer, last N fixes
    pub native_override: Option<LocationFix>,  // for tests / inject_native_fix
    pending_event: Option<LocationFix>,
}

impl GeolocationManager {
    pub fn new() -> Self { /* … */ }
    pub fn inject_native_fix(&mut self, fix: LocationFix) { /* … */ }
    pub fn record_fix(&mut self, fix: LocationFix) { /* … */ }
    pub fn current(&self) -> Option<LocationFix> { /* … */ }
}

impl EventProvider for GeolocationManager {
    fn get_pending_events(&self, ts: Instant) -> Vec<SyntheticEvent> {
        if let Some(fix) = self.pending_event {
            vec![SyntheticEvent::new(
                EventType::LocationUpdate,
                CoreEventSource::Os,
                /* no node — window-level */
                ts,
                EventData::Location(fix),
            )]
        } else {
            vec![]
        }
    }
}
```

Event filter additions (`core/src/events.rs`):
```text
EventType::LocationUpdate           // top-level
EventType::LocationAuthChanged
WindowEventFilter::LocationUpdate   // window-broadcast (no hover propagation)
WindowEventFilter::LocationAuthChanged
```

Geolocation is *not* hover-propagatable (it has no DOM position), so it
lives only in `WindowEventFilter` — same model as `Resized` / `Moved` /
`CloseRequested`. Skip `HoverEventFilter` and `FocusEventFilter` variants.

CallbackInfo accessor:
```rust
impl CallbackInfo<'_> {
    pub fn get_current_location(&self) -> Option<LocationFix> {
        self.layout_window?.geolocation_manager.current()
    }
    pub fn get_location_auth_status(&self) -> LocationAuthStatus { /* … */ }
}
```

App-level methods (`App::request_geolocation(...)`):
```rust
impl App {
    pub fn request_geolocation(
        &mut self,
        prompt: AzString,
        accuracy: LocationAccuracy,
    ) -> Result<(), PermissionError>;
    pub fn stop_geolocation(&mut self);
}
```

The platform impl wires this to:
| Platform | Implementation |
|----------|----------------|
| iOS      | `CLLocationManager.requestWhenInUseAuthorization` + `startUpdatingLocation` |
| Android  | `LocationManager.requestLocationUpdates(GPS_PROVIDER, ..., listener)` via JNI |
| macOS    | Same as iOS |
| Linux    | `org.freedesktop.GeoClue2` `GetClient` + `Start` |
| Windows  | `Geolocator.PositionChanged += handler` + `RequestAccessAsync` |

Each platform's location callback flows into `inject_native_fix` (which
records into pending_event), the next event tick picks it up via
`EventProvider`, and `WindowEventFilter::LocationUpdate` fires on every
node with a matching callback (same brute-force pattern as `Resized`).

**Rate-limiting:** we throttle in `inject_native_fix` — drop duplicate
fixes more frequent than e.g. 1 Hz, configurable per-app via a
`set_geolocation_min_interval(ms)` accessor. iOS / Android both honor
`distance_filter` / `minimumDistanceMeters` at the OS side too; expose
that as a hint:

```rust
impl App {
    pub fn set_geolocation_min_distance_meters(&mut self, m: f32);
    pub fn set_geolocation_min_interval_ms(&mut self, ms: u32);
}
```

**W3C equivalent:**
- `navigator.geolocation.getCurrentPosition(success, error, options)` — one-shot.
- `navigator.geolocation.watchPosition(success, error, options)` — subscription.
- `GeolocationPosition.coords.{latitude, longitude, altitude, accuracy,
  altitudeAccuracy, heading, speed}`.
- `PositionOptions.enableHighAccuracy`, `maximumAge`, `timeout`.

Our `LocationFix` is field-for-field compatible (modulo unit choices).
`request_geolocation` maps onto `watchPosition`; `get_current_location()`
is the cached-fix accessor that matches `getCurrentPosition`'s
`maximumAge: Infinity` shape.

---

## 4 · Cross-cutting integration notes

### 4.1 Permission UX flow

Each of the three features (and most of the §2 features in
`SUPER_PLAN_2.md`) needs a *typed* permission-error return so the user
binding sees `Denied` vs `Unsupported` vs `Pending` vs `Granted`:

```rust
#[repr(C, u8)]
pub enum PermissionError {
    NotRequested,
    Denied,
    Restricted,    // OS-level, e.g. parental controls
    Unsupported,   // platform doesn't have this API
    Pending,       // dialog is up, no answer yet
}
```

The desktop file-picker doesn't need this (it's a one-shot modal that
returns either a path or `None`). Geolocation does. IME doesn't expose
permissions per se but does need a way to say "no IME available, fall
back to ASCII" — which can be the same enum.

### 4.2 Async-result handles

Both `FileDialog::open_file_async` and `App::request_geolocation` return
handles that the event loop polls. The same pattern is already used in
`Timer` / `Task` infrastructure. Recommended: define a single
`AsyncResult<T>` type that wraps a `mpsc::Receiver<T>` with a polling
`take(&mut self) -> Option<T>` method, reuse across all async-result
APIs (file pickers, biometric prompts, location auth dialogs, etc.).

### 4.3 Manager registration

For consistency with the existing managers (`GestureAndDragManager`,
`FocusManager`, `TextInputManager`, `TextEditManager`, `ScrollManager`),
each new manager goes on `LayoutWindow`:

```rust
// layout/src/window.rs
pub struct LayoutWindow {
    // existing
    pub gesture_drag_manager: GestureAndDragManager,
    pub focus_manager: FocusManager,
    pub text_input_manager: TextInputManager,
    pub text_edit_manager: TextEditManager,
    pub scroll_manager: ScrollManager,
    // new
    pub geolocation_manager: GeolocationManager,   // §3 of this doc
    pub file_picker_manager: FilePickerManager,    // §1, holds pending handles
}
```

The file-picker manager is *optional* — we could keep `FileDialog` purely
static-method and dispatch through a thread-local. But putting the
pending-handle state on `LayoutWindow` matches every other manager and
keeps the API consistent.

### 4.4 api.json + codegen

Every new type goes through `azul-doc autofix add <Type>.<method>` so all
35 binding languages pick up the surface. Specifically:

| New api.json entry                                       | What it lifts |
|----------------------------------------------------------|---------------|
| `FilePickerHandle`                                       | Opaque async handle |
| `FilePickerStatus`                                       | tagged union (Pending / Cancelled / Selected / Error) |
| `FileDialog.open_file_async`                             | New static method |
| `LocationFix`                                            | Struct |
| `LocationAccuracy`                                       | Enum |
| `LocationAuthStatus`                                     | Enum |
| `App.request_geolocation`                                | Permission entry point |
| `App.stop_geolocation`                                   | Counterpart |
| `App.set_geolocation_min_interval_ms`                    | Throttle |
| `CallbackInfo.get_current_location`                      | Accessor |
| `CallbackInfo.get_location_auth_status`                  | Accessor |
| `WindowEventFilter.LocationUpdate`                       | New filter variant |
| `WindowEventFilter.LocationAuthChanged`                  | ditto |
| `PermissionError`                                        | Shared typed-error enum |

The composition events (§2) already have api.json entries — no new ones
needed for IME. The platform job is *purely* wiring `set_preedit` /
`clear_preedit` + the new `_with_event` variants.

### 4.5 Build-side artifacts

| Path | Status | Purpose |
|------|--------|---------|
| `scripts/android/NativeInputConnection.java` | **new** | §2.3 — IME bridge over JNI |
| `scripts/android/FilePickerBridge.java`      | **new** | §1.3 — SAF intent dispatch |
| `scripts/android/build-classes.sh`           | **new** | javac + d8 + zip-into-apk |
| `scripts/ios/Info.plist`                     | **patch** | add `NSLocationWhenInUseUsageDescription`, `NSDocumentsFolderUsageDescription` |
| `scripts/android/AndroidManifest.xml.template` | **patch** | add `ACCESS_FINE_LOCATION` + `windowSoftInputMode="adjustResize"` |
| `dll/src/desktop/shell2/ios/text_input.rs`   | **new** | UIKeyInput / UITextInput conformance |
| `dll/src/desktop/shell2/android/text_input.rs` | **new** | JNI handlers |
| `dll/src/desktop/shell2/linux/wayland/events.rs` | **patch** | fill in zwp_text_input_v3 six handlers (lines 805-928) |
| `dll/src/desktop/shell2/win32/ime.rs`        | **new** | WM_IME_* handlers |
| `dll/src/desktop/shell2/{ios,android,macos,linux,win32}/geolocation.rs` | **new** | Platform CL/GeoClue/Geolocator wrappers |
| `layout/src/managers/geolocation.rs`         | **new** | Cross-platform manager |

### 4.6 Test fixtures

- **File picker**: integration-test on each mobile target that opens the
  picker programmatically (impossible to fully automate without UI test
  harness), but the synthetic path — calling
  `FilePickerManager::inject_test_result(Some("/tmp/foo.png".into()))` and
  verifying the callback fires — is feasible. Same model as
  `gesture_drag_manager.inject_native_gesture(...)`.
- **IME**: a synthetic test that calls
  `text_edit_manager.set_preedit("こ".into(), 0, 1)` then
  `set_preedit("こん".into(), 0, 2)` then `clear_preedit()`, observes
  `CompositionStart` → `CompositionUpdate` → `CompositionEnd` events
  fire in order. Hits everything except the actual OS IME hookup.
- **Geolocation**: `geolocation_manager.inject_native_fix(LocationFix { lat: 52.5, lon: 13.4, ... })`
  and verify `WindowEventFilter::LocationUpdate` callbacks see the fix.

All three reuse the existing snapshot harness pattern at
`scripts/mobile/golden/` (see `SUPER_PLAN_2.md` §2 table).

---

## 5 · Open questions / `TODO: verify` list

1. iOS `UTType(filenameExtension:)` (iOS 14+) — does it return `nil` for
   completely unknown extensions, or fall back to `public.data`? Affects
   our extension → UTI mapping (§1.2).
2. `android-activity` (game-activity feature) — exact method names for
   `show_soft_input` / `hide_soft_input` on the version pinned in our
   `Cargo.toml`. The plan in `ANDROID_IMPLEMENTATION_PLAN.md` shows
   `app.show_soft_input(true)` but the API may have shifted.
3. `android-activity` — is `GameActivity.java` open to subclassing, or do
   we ship our own `AzulGameActivity.java`? (§2.3.2)
4. `objc2_core_location` — exact crate name and version. The `objc2`
   ecosystem has reorganized recently. (§3.1)
5. `ashpd` — does it work without an async runtime, or does it pull
   tokio? Affects whether we use it for XDG portals (file chooser §1.5,
   geolocation §3.4) or write raw `zbus` calls. (§1.5)
6. `text_edit_manager` — current API does not expose
   `delete_surrounding_text(before, after)`. Either add it (needed for
   Wayland `zwp_text_input_v3::delete_surrounding_text` event handling)
   or work around by reading the text, deleting client-side, and pushing
   a replacement. (§2.5.2)
7. macOS `tfd` shell-out — confirmed that swapping to direct
   `NSOpenPanel` is a "nice to have, not a priority" given the picker
   works today. (§1.4)
8. Windows TSF — confirmed deferred; legacy `WM_IME_*` path is enough
   for v1. (§2.6)
9. iOS background location — out of scope for v1 since `<MapWidget>`
   doesn't need background updates. Document as a future addition with
   the extra `UIBackgroundModes` requirement.
10. Linux GeoClue accuracy — IP geolocation gives city-level accuracy
    only on most distros. For a map widget that's fine ("show me where
    I am"), but if users expect <100 m precision they'll be confused.
    Document the limitation prominently.

---

## 6 · Reference: existing seam pointers

For the next session's implementation agent, the relevant pre-existing
code locations:

| Concern | File | Lines / symbols |
|---------|------|------------------|
| Desktop dialog API to keep parity with | `/Users/fschutt/Development/azul-mobile/layout/src/desktop/dialogs.rs` | `FileDialog::open_file` 263-284; `MsgBox::ok` 132-146; `ColorPickerDialog::open` 204-229 |
| Composition events surface | `/Users/fschutt/Development/azul-mobile/core/src/events.rs` | `HoverEventFilter::CompositionStart/Update/End` 494-498; `EventType::CompositionStart/Update/End` 1805-1810; routing 2269-2271 |
| TextInputManager (record/apply pipeline) | `/Users/fschutt/Development/azul-mobile/layout/src/managers/text_input.rs` | `record_input` 158-174; `EventProvider` impl 201-238 |
| TextEditManager (preedit state) | `/Users/fschutt/Development/azul-mobile/layout/src/managers/text_edit.rs` | `preedit_text` 108-110; `set_preedit` 227-233; `clear_preedit` 235-237 |
| macOS NSTextInputClient reference impl | `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/macos/mod.rs` | GLView: 745-953; CPUView: 1600-1820; protocol conformance: 957, 1802 |
| Wayland zwp_text_input_v3 listener (placeholders) | `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/linux/wayland/events.rs` | listener struct 105-130; handler stubs 805-928 |
| Wayland text-input protocol opcodes | `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/linux/wayland/defines.rs` | 447-531 |
| Android drain_input (where IME plumbing lands) | `/Users/fschutt/Development/azul-mobile/dll/src/desktop/shell2/android/mod.rs` | `drain_input` 482-602; `map_keycode` 611-632; TODO marker 524-528 |
| iOS implementation skeleton | `/Users/fschutt/Development/azul-mobile/scripts/IOS_IMPLEMENTATION_PLAN.md` | UIKeyInput sketch 440-491; UITextInput notes 494-504 |
| Android implementation skeleton | `/Users/fschutt/Development/azul-mobile/scripts/ANDROID_IMPLEMENTATION_PLAN.md` | NativeInputConnection sketch 646-744 |
| SUPER_PLAN_2 architecture seams | `/Users/fschutt/Development/azul-mobile/SUPER_PLAN_2.md` | §0 11-21; feature 8 47; feature 9 48; feature 10 49 |
| Manager registration pattern | `/Users/fschutt/Development/azul-mobile/layout/src/managers/` | text_input.rs, text_edit.rs (existing), geolocation.rs (new) |

---
slug: system/file-dialogs
title: File Dialogs
language: en
canonical_slug: system/file-dialogs
audience: external
maturity: wip
guide_order: 121
topic_only: false
short_desc: Native open/save dialogs and folder pickers
prerequisites: [events]
tracked_files:
  - layout/src/desktop/dialogs.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - FileDialog
  - MsgBox
  - MsgBoxIcon
  - ColorPickerDialog
  - FileTypeList
  - OkCancel
  - YesNo
  - OptionColorU
  - OptionString
---

# File Dialogs

## Introduction

Azul ships native message boxes, file pickers, folder pickers, and a color
chooser. Each call shows the platform's native dialog chrome.

Every picker is a *request*: it returns a `RequestId` immediately and
resumes a callback you pass in once the user has answered. The same code
runs on desktop (where the dialog is modal and the callback runs right after
the requesting callback returns), on mobile (the OS picker answers through a
delegate) and in the browser (the picker is asynchronous and gated on a user
gesture). Only `MsgBox` stays synchronous, because `alert()` / `confirm()`
are.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_open_clicked(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        "Open a file".into(),
        OptionString::None,
        OptionFileTypeList::None,
        data,            // handed back to `on_file_picked` untouched
        on_file_picked,  // the resume callback
    );
    Update::DoNothing
}

extern "C" fn on_file_picked(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match picked.path.into_option() {
        Some(p) => MsgBox::info(format!("You picked {}", p.as_str()).into()),
        None    => MsgBox::info("Cancelled".into()),
    };
    Update::DoNothing
}
```

The resume callback always has the same shape,
`fn(data: RefAny, info: CallbackInfo, result: RefAny) -> Update`: `data` is
what you passed to the request, `result` is the operation's result struct
type-erased into a `RefAny`, and every result struct has one static
`downcast(result)` accessor to get the typed value back.

## Types

- `MsgBox`: message boxes (`ok`, `ok_cancel`, `yes_no`, `info`).
- `FileDialog`: open, save, multi-select file pickers, and the folder
  picker.
- `ColorPickerDialog`: RGB color picker, returns `OptionColorU`.

Supporting enums:

- `MsgBoxIcon` with variants `Info`, `Warning`, `Error`, `Question`. Used by
  every `MsgBox` call.
- `OkCancel` with variants `Ok`, `Cancel`. Used by `MsgBox::ok_cancel` for the
  default-highlighted button and the return value.
- `YesNo` with variants `Yes`, `No`. Used by `MsgBox::yes_no` the same way.
- `FileTypeList` with `document_types: StringVec` and `document_descriptor`,
  used to pass `FileDialog` filters.

## Chaining requests

A resume callback is an ordinary callback, so it can issue the next request.
Reading a picked file is the common two-step chain: the picker resumes with
a path, `FilePath::read_bytes` resumes with the bytes.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_open_clicked(data: RefAny, _info: CallbackInfo) -> Update {
    let filter = FileTypeList {
        document_types: StringVec::from_vec(vec![
            "png".into(),
            "jpg".into(),
            "jpeg".into(),
        ]),
        document_descriptor: "Image files".into(),
    };
    let _request = FileDialog::open_file(
        "Pick an image".into(),
        OptionString::None,
        OptionFileTypeList::Some(filter),
        data,
        on_image_picked,
    );
    Update::DoNothing
}

extern "C" fn on_image_picked(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let _request = path.read_bytes(data, on_image_bytes);
    Update::DoNothing
}

extern "C" fn on_image_bytes(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(read) = FileReadBytesResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match read.result {
        ResultU8VecFileError::Ok(_bytes) => Update::RefreshDom,
        ResultU8VecFileError::Err(_e)    => Update::DoNothing,
    }
}
```

`OptionString::None` for `default_path` lets the OS pick a sensible starting
directory. Pass `OptionString::Some(...)` to override. In the browser a
picker only opens from a user gesture: a request issued from a timer resolves
with `path: None`.

## File pickers

The picker requests on `FileDialog`, and the result struct each one resumes
with:

```rust,ignore
fn open_file(
    title: AzString,
    default_path: OptionString,
    filter_list: OptionFileTypeList,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // FileOpenResult { path: OptionFilePath, error: OptionString }

fn open_directory(
    title: AzString,
    default_path: OptionString,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // FileOpenResult (path = the directory)

fn open_multiple_files(
    title: AzString,
    default_path: OptionString,
    filter_list: OptionFileTypeList,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // FileOpenMultiResult { paths: FilePathVec, error: OptionString }

fn save_file(
    title: AzString,
    suggested_name: AzString,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // SaveTargetResult { target: OptionSaveTarget, error: OptionString }

fn save_bytes(
    suggested_name: AzString,
    mime: AzString,
    bytes: U8Vec,
) -> bool;                              // fire-and-forget export (a desktop and the browser; NOT a phone)

fn open_folder(
    title: AzString,
    default_path: OptionString,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // FolderPickResult { locator: OptionString, error: OptionString }

fn export_bytes(
    suggested_name: AzString,
    mime: AzString,
    bytes: U8Vec,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // ExportResult { saved_as: OptionString, error: OptionString }
```

Cancelling resolves with `path: None` / an empty `paths` / `target: None` /
`locator: None` / `saved_as: None` and `error: None`. A dialog that FAILED
resolves the same way but with `error: Some(reason)`: an app that wants to tell
the user why nothing happened reads `error`; a cancel is never reported as one.
On a phone, a request the platform cannot serve (`save_file`, and
`open_directory` on Android, where a folder is a permission and not a path)
fails with its reason instead of showing a dialog whose answer would be thrown
away, and `save_bytes` answers `false` (and logs why) instead of writing a
folder the user did not choose and saying `true`.

On a phone, ask for:

* a FILE with `open_file` / `open_multiple_files`: the answer is the path of a
  working copy in the app's own storage, which every decoder and `std::fs`
  open (the same folder a file another app opens yours with lands in; copies
  are capped like those and say why when one could not be made);
* a FOLDER with `open_folder`: the answer is a `locator`, the folder as
  `CallbackInfo::documents()` takes it (see "Files and folders the user owns"
  below). On Android the system's tree picker is shown and the grant is KEPT, so the
  folder is still in `Documents::places()` after a restart; on iOS and on a desktop
  the locator is the folder's path;
* a place to SAVE a rendered document with `export_bytes`: the system's "save as"
  is shown, the bytes are written where the person chose, and `saved_as` names what
  they were saved under only after the write came back. A cancel and a failure
  are different answers, and no empty file is left behind by a write that failed.

Filters are extension-only: pass bare extensions like `"png"`, `"jpg"` (no
leading dot, no glob), plus a human-readable label. `open_directory` and
`save_file` ignore filters.

`save_file` answers with a *write target*, not a path: on desktop it is a
real path (`SaveTarget::as_path` is `Some`), in Chromium a File System
Access handle, and in Firefox / Safari a `Download` sentinel whose `as_path`
is `None`. Use it when the app has to write the same file again later.
An app that only exports a blob of bytes (a PDF, an image) should call
`save_bytes` instead: a native save dialog on desktop, a download in the
browser, and no path ever reaches the app.

## Sharing a file with another app

`CallbackInfo::share` hands files, a text or a link to ANOTHER app through the system's share sheet: Android's chooser, iOS's
activity view. Call it from the callback of the user's tap on a Share button; it answers at once with what happened.

```rust,ignore
extern "C" fn on_share_clicked(data: RefAny, mut info: CallbackInfo) -> Update {
    let request = ShareRequest::file(report_path(&data))   // a path this app can read
        .with_title("Quarterly report".into());
    match info.share(request) {
        ShareStatus::Shown => {}                                   // the sheet is up
        ShareStatus::Unsupported(why) => show_toast(&data, why),   // a desktop: offer "reveal in file manager" instead
        ShareStatus::Failed(why) | ShareStatus::Refused(why) => show_toast(&data, why),
    }
    Update::RefreshDom
}
```

- `ShareRequest::file`, `::files`, `::text`, `::url` start a request, `with_item` adds more, `with_title` is the sheet's title (and
  the subject of a mail made from it), `with_anchor(rect)` is where an iPad's popover points (the Share button's rect in window
  coordinates).
- `with_open_with(true)` on a request of ONE file is "Open with...": only the apps that can open the file are offered.
- The files are COPIED for the receiver under the name the request gives them (`ShareItem::with_name`) and the type of their
  extension (`ShareItem::with_mime` overrides it): the receiver is never shown the app's data folder, and the copies are removed
  after a day. A request is checked against limits (64 items, 256 MiB a file, 512 MiB in all, safe unique names); one over a limit is
  `Refused` with the reason.
- `CallbackInfo::can_share` says whether this platform has a share sheet: ask it before a Share button is shown. A desktop answers
  `Unsupported` (with the reason) and `false`: offer what the platform has (reveal the file, copy the link).
- In a headless or scripted run nothing is shown: the request is checked and recorded, and the scenario reads it with the
  `assert_shared` step (`{"op": "share", ...}` makes the call without an app button).

On a phone, "Export as a file" is a share: write the file into the app's own folder and share it. (`FileDialog::save_bytes` answers
`false` on a phone until a save dialog is behind it: it never writes a folder the user did not choose and says `true`.)

## Being opened with a file

The files and links an app opens are declared in ONE place, its entry in `scripts/demo_apps.toml`, and the phone builds make the
intent filters of Android and the document types of iOS from it:

```toml
[[app.documents]]
name = "Markdown document"            # what the system's "Open with" list shows
role = "editor"                       # "viewer" | "editor"
extensions = ["md", "markdown"]       # matched by name where a provider says no type
mime = ["text/markdown"]
uti = "net.daringfireball.markdown"   # iOS: the type's identifier (uti_conforms_to = [...] only for a type the app defines)

[app.links]
schemes = ["mailto", "geo"]
```

The app installs ONE handler for everything the system asks it to open, usually in its window's create callback:

```rust,ignore
extern "C" fn on_open(data: RefAny, info: CallbackInfo) -> Update {
    let Some(request) = info.get_open_request().into_option() else { return Update::DoNothing };
    for item in request.items.as_ref() {
        match item.kind {
            OpenedKind::File => match item.path.as_ref() {
                Some(path) => open_document(&data, path.as_str(), item.name.as_str()),   // a WORKING COPY: std::fs opens it
                None => show_error(&data, item.error.as_ref()),                          // too large, the provider failed
            },
            OpenedKind::Url => handle_link(&data, item.text.as_str()),
            OpenedKind::Text => insert_text(&data, item.text.as_str()),
        }
    }
    Update::RefreshDom
}
// in the create callback: info.set_open_handler(data.clone(), on_open);
```

- A request that arrives before the handler is installed (the one that STARTED the app) is kept, up to 16, and delivered when the
  handler is set. `request.launched_app` says that it started the app.
- A file is never a `content://` URI for the app: the engine copies it into `<app files>/azul-inbox/<token>/<name>` on a worker
  thread (256 MiB a file, 1 GiB a request) and hands the path of the copy, with the name, type and size the system said and where it
  came from (`item.uri`). An item that could not be copied is still an item: no `path`, and `error` says why.
- `request.action` is `View`, `Edit` or `Send` (the file was SHARED to the app); `item.writable` is set where the sender allows the app
  to write it back.
- Scenarios play the system with `{"op": "simulate_open", "files": [...]}`.

## Files and folders the user owns

A path is not enough on a phone. Android gives an app the folders the person GRANTS (a tree of documents named by a `content://` URI, with a
permission the person can take back), iOS security-scoped folders; neither is something `std::fs` can open. `CallbackInfo::documents()` hands out a
`Documents` handle that works by LOCATOR, an opaque string the platform made (a path on a desktop, an Android `content://` URI) which the app stores,
shows by its entry's `name`, and hands back:

```rust,ignore
let documents = info.documents();            // Clone + Send + Sync: take it here, move it to a worker thread
// on the worker (every call blocks until the platform answers; a cloud provider may take seconds):
documents.places();                          // the app's storage, every folder the person granted, "All files" where given
documents.children(locator);                 // Result<DocumentEntryVec, DocumentError>, folders first
documents.stat(locator);                     // name, MIME type, size, date, what may be done with it
documents.read(locator, offset, len);        // a range (at most 64 MiB a call): a file is never read whole by the engine
documents.write(locator, bytes);             // replaces a file
documents.create_file(parent, name, mime);   // never over a name that is taken (`Exists`)
documents.create_dir(parent, name);
documents.rename(locator, new_name);
documents.remove(locator);                   // a file, or a folder with what is in it
documents.forget(locator);                   // release a granted folder (the folder itself is untouched)
documents.all_files_access();                // Granted / NotGranted / Unavailable (Android's MANAGE_EXTERNAL_STORAGE, a flavour only)
documents.origin(working_copy);              // where a file another app opened this app with came from, and whether it may be saved back
documents.write_back(working_copy);          // the copy's bytes over the original (the same call as `write`)
```

Every failure says what to do next: `Denied` (the person took the grant back, or never gave it: ask for the folder again with `open_folder`),
`NotFound` (it vanished: forget it), `TimedOut` (a slow provider: retry or tell the person; a provider that never answers is refused after eight late
calls instead of collecting a thread per retry), `Exists`, `NotADirectory`, `Unsupported`, `Cancelled`, `Failed(reason)`. `DocumentError::describe()` is
the sentence for the person. A listing is cut at 20 000 entries and the cut is logged; a name is one plain segment (no `/`, `\`, NUL, at most 255 bytes).

A file another app opened this app with is a WORKING COPY (a path `std::fs` opens); the engine remembers where it came from next to the copy, so
that the app can save the person's edits back with `write_back` where the sender allowed writing (`DocumentOrigin::writable`: Android's `ACTION_EDIT`
with a write grant). Where it did not (a read-only share, a copy iOS made in the app's own folder before the app saw it, a file the app made itself)
`origin` says nothing and `write_back` answers `Unsupported` (no origin) or `Denied` (read-only): offer `FileDialog::export_bytes`, the system's
"save as", instead.

What a provider cannot do is said, not hidden: a write through Android's storage framework is in place (there is no atomic replace across providers; keep
the working copy until the write came back), and a provider that renames instead of failing is checked first (`create_file` over a name that is taken
answers `Exists`). The app asks for "All files access" only in a build that declared it and only from the tap of a person; it is never asked at launch.

## Message boxes

```rust,no_run
use azul::prelude::*;

MsgBox::ok("Saved".into(), "File written.".into(), MsgBoxIcon::Info);

let proceed = MsgBox::ok_cancel(
    "Confirm".into(),
    "Overwrite existing file?".into(),
    MsgBoxIcon::Warning,
    OkCancel::Cancel,
);
if proceed == OkCancel::Ok {
    // ...
}

let answer = MsgBox::yes_no(
    "Quit".into(),
    "Discard unsaved changes?".into(),
    MsgBoxIcon::Question,
    YesNo::No,
);

MsgBox::info("All done.".into());
```

## Color picker

```rust,no_run
use azul::prelude::*;

extern "C" fn on_pick_color(data: RefAny, _info: CallbackInfo) -> Update {
    let initial = OptionColorU::Some(ColorU { r: 37, g: 99, b: 235, a: 255 });
    let _request = ColorPickerDialog::open("Pick a color".into(), initial, data, on_color_picked);
    Update::DoNothing
}

extern "C" fn on_color_picked(_data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = ColorPickResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match picked.color.into_option() {
        Some(c) => println!("rgb({}, {}, {})", c.r, c.g, c.b),
        None    => println!("cancelled"),
    }
    Update::DoNothing
}
```

The picker returns RGB only; alpha is forced to opaque. Pass
`OptionColorU::None` to start at black. In the browser `<input type=color>`
has no cancel event everywhere, so closing it without a change resolves as
`None`.

## Limitations

- `Documents` calls block until the platform answers: take the handle in a callback, call from a worker thread.
- All dialog calls block the calling thread until dismissed. Inside a callback
  this stops event delivery to every window in your app. For long-running
  operations triggered after the dialog closes, spawn a background thread.
- Filter format is extension-only.
- Pass one `FileTypeList` per call. If you need multiple labelled groups,
  present them in a custom in-window picker instead.

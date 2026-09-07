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
) -> RequestId;                         // FileOpenResult { path: OptionFilePath }

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
) -> RequestId;                         // FileOpenMultiResult { paths: FilePathVec }

fn save_file(
    title: AzString,
    suggested_name: AzString,
    data: RefAny,
    on_result: ResumeCallbackType,
) -> RequestId;                         // SaveTargetResult { target: OptionSaveTarget }

fn save_bytes(
    suggested_name: AzString,
    mime: AzString,
    bytes: U8Vec,
) -> bool;                              // fire-and-forget export
```

Cancelling resolves with `path: None` / an empty `paths` / `target: None`.
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

- All dialog calls block the calling thread until dismissed. Inside a callback
  this stops event delivery to every window in your app. For long-running
  operations triggered after the dialog closes, spawn a background thread.
- Filter format is extension-only.
- Pass one `FileTypeList` per call. If you need multiple labelled groups,
  present them in a custom in-window picker instead.

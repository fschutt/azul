---
slug: file-dialogs
title: File Dialogs
language: en
canonical_slug: file-dialogs
audience: external
maturity: wip
guide_order: 240
topic_only: false
short_desc: Native open/save dialogs and folder pickers
prerequisites: [events]
tracked_files:
  - layout/src/desktop/dialogs.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# File Dialogs

> **WIP.** The dialog API is functional on all platforms, but every call is
> synchronous and blocks the calling thread. An async variant is planned;
> signatures here are stable.

Azul ships native message boxes, file pickers, folder pickers, and a color
chooser. Each call shows the platform's native dialog chrome.

```rust,no_run
# use azul::prelude::*;
let path = FileDialog::open_file(
    "Open a file".into(),
    OptionString::None,
    OptionFileTypeList::None,
);
match path.into_option() {
    Some(p) => MsgBox::info(format!("You picked {}", p.as_str()).into()),
    None    => MsgBox::info("Cancelled".into()),
};
```

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

## Calling from a callback

Dialog functions block the entire window event loop until the user dismisses
the dialog. That's almost always what you want from inside a callback. The
user just clicked File → Open and doesn't expect to interact with the
underlying window until they pick something.

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_open_clicked(_data: RefAny, _info: CallbackInfo) -> Update {
    let filter = FileTypeList {
        document_types: StringVec::from_vec(vec![
            "png".into(),
            "jpg".into(),
            "jpeg".into(),
        ]),
        document_descriptor: "Image files".into(),
    };

    let picked = FileDialog::open_file(
        "Pick an image".into(),
        OptionString::None,
        OptionFileTypeList::Some(filter),
    );

    match picked.into_option() {
        Some(_path) => Update::RefreshDom,
        None        => Update::DoNothing,
    }
}
```rust

`OptionString::None` for `default_path` lets the OS pick a sensible starting
directory. Pass `OptionString::Some(...)` to override.

## File pickers

The four picker variants on `FileDialog`:

```rust,ignore
fn open_file(
    title: AzString,
    default_path: OptionString,
    filter_list: OptionFileTypeList,
) -> OptionString;

fn open_directory(
    title: AzString,
    default_path: OptionString,
) -> OptionString;

fn open_multiple_files(
    title: AzString,
    default_path: OptionString,
    filter_list: OptionFileTypeList,
) -> OptionStringVec;

fn save_file(
    title: AzString,
    default_path: OptionString,
) -> OptionString;
```

Each returns `None` on cancel. Filters are extension-only: pass bare
extensions like `"png"`, `"jpg"` (no leading dot, no glob), plus a
human-readable label. `open_directory` and `save_file` ignore filters.

## Message boxes

```rust,no_run
# use azul::prelude::*;
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
# use azul::prelude::*;
let initial = OptionColorU::Some(ColorU { r: 37, g: 99, b: 235, a: 255 });
let picked = ColorPickerDialog::open("Pick a color".into(), initial);

match picked.into_option() {
    Some(c) => println!("rgb({}, {}, {})", c.r, c.g, c.b),
    None    => println!("cancelled"),
}
```

The picker returns RGB only; alpha is forced to opaque. Pass
`OptionColorU::None` to start at black.

## Limitations

- All dialog calls block the calling thread until dismissed. Inside a callback
  this stops event delivery to every window in your app. For long-running
  operations triggered after the dialog closes, spawn a background thread.
- Filter format is extension-only.
- Pass one `FileTypeList` per call. If you need multiple labelled groups,
  present them in a custom in-window picker instead.

## Coming Up Next

- [Clipboard](clipboard.md) — Reading and writing the system clipboard
- [Networking](networking.md) — HTTP from a callback
- [Windows, Menus, Decorations](windowing.md) — Windows, menus, decorations, and per-window state

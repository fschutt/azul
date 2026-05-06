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

> **WIP** — the dialog API is functional on all platforms but every call is **synchronous and blocks the calling thread**. An async variant is planned; signatures here are stable.

Native message boxes, file pickers, folder pickers, and color choosers live in `azul_layout::desktop::dialogs`. They are static-method namespaces backed by the `tfd` (tiny-file-dialogs) crate, so each platform shows its native chrome — Win32 `IFileDialog` on Windows, NSOpenPanel on macOS, GTK3 `gtk_file_chooser_dialog_new` on Linux.

```rust,no_run
use azul_layout::desktop::dialogs::{FileDialog, MsgBox, MsgBoxIcon};
use azul_css::{AzString, OptionString};
use azul_css::corety::OptionString as CoreOptionString;

let path = FileDialog::open_file(
    AzString::from("Open a file"),
    OptionString::None,
    azul_layout::desktop::dialogs::OptionFileTypeList::None,
);
match path.into_option() {
    Some(p) => MsgBox::info(AzString::from(format!("You picked {}", p.as_str()))),
    None    => MsgBox::info(AzString::from("Cancelled")),
}
```

## Types

| Namespace | Purpose |
|---|---|
| `MsgBox` | message boxes (`ok`, `ok_cancel`, `yes_no`, `info`) |
| `FileDialog` | open / save / multi-select file pickers and folder picker |
| `ColorPickerDialog` | RGB color picker, returns `OptionColorU` |

Each namespace is a zero-sized `#[repr(C)]` struct with one reserved byte. They expose only static methods; instances carry no state and exist only so the FFI layer (Python, C, C++) can hang the dialog functions off a class.

Supporting enums — all `#[repr(C)]`, all in `layout/src/desktop/dialogs.rs`:

| Type | Variants | Used by |
|---|---|---|
| `MsgBoxIcon` | `Info`, `Warning`, `Error`, `Question` | every `MsgBox` call |
| `OkCancel` | `Ok`, `Cancel` | `MsgBox::ok_cancel` (default + return) |
| `YesNo` | `Yes`, `No` | `MsgBox::yes_no` (default + return) |
| `FileTypeList` | `{ document_types: StringVec, document_descriptor: AzString }` | `FileDialog` filters |

## Calling from a callback

Dialog functions block the entire window event loop until the user dismisses the dialog. That's almost always what you want from inside an `Update` callback — the user just clicked **File → Open**, they don't expect to interact with the underlying window until they pick something.

```rust,no_run
# use azul::callbacks::{CallbackInfo, RefAny, Update};
# use azul_layout::desktop::dialogs::{FileDialog, FileTypeList, OptionFileTypeList};
# use azul_css::{AzString, OptionString, StringVec};
fn on_open_clicked(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let filter = FileTypeList {
        document_types: StringVec::from_vec(vec![
            AzString::from("png"),
            AzString::from("jpg"),
            AzString::from("jpeg"),
        ]),
        document_descriptor: AzString::from("Image files"),
    };

    let picked = FileDialog::open_file(
        AzString::from("Pick an image"),
        OptionString::None,
        OptionFileTypeList::Some(filter),
    );

    match picked.into_option() {
        Some(_path) => Update::RefreshDom,
        None        => Update::DoNothing,
    }
}
```

`OptionString::None` for `default_path` lets the OS pick a sensible starting directory (last-used location on Windows, `$HOME` on Linux, NSOpenPanel default on macOS). Pass `OptionString::Some(AzString::from("/some/path"))` to override.

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

Each returns `None` on cancel. Filters are extension-only — pass bare extensions like `["png", "jpg"]` (no leading `.`, no glob), plus a human-readable label that appears in the platform's filter dropdown. `open_directory` and `save_file` ignore filters because the underlying platform dialogs don't accept them.

## Message boxes

```rust,no_run
use azul_layout::desktop::dialogs::{MsgBox, MsgBoxIcon, OkCancel, YesNo};
use azul_css::AzString;

MsgBox::ok(AzString::from("Saved"), AzString::from("File written."), MsgBoxIcon::Info);

let proceed = MsgBox::ok_cancel(
    AzString::from("Confirm"),
    AzString::from("Overwrite existing file?"),
    MsgBoxIcon::Warning,
    OkCancel::Cancel, // default highlighted button
);
if proceed == OkCancel::Ok {
    // …
}

let answer = MsgBox::yes_no(
    AzString::from("Quit"),
    AzString::from("Discard unsaved changes?"),
    MsgBoxIcon::Question,
    YesNo::No,
);

MsgBox::info(AzString::from("All done.")); // shortcut: title="Info", icon=Info
```

`MsgBox::ok` strips single and double quotes from the message before passing it to `tfd`, because some platform shells (notably the GTK fallback) treat them as metacharacters and produce empty dialogs otherwise. If your message must contain quotes, render them as `&quot;` / `&apos;` in the source string and decode on the user's machine.

## Color picker

```rust,no_run
use azul_layout::desktop::dialogs::ColorPickerDialog;
use azul_css::{AzString, props::basic::color::{ColorU, OptionColorU}};

let initial = OptionColorU::Some(ColorU { r: 37, g: 99, b: 235, a: 255 });
let picked = ColorPickerDialog::open(AzString::from("Pick a color"), initial);

match picked.into_option() {
    Some(c) => println!("rgb({}, {}, {})", c.r, c.g, c.b),
    None    => println!("cancelled"),
}
```

The picker returns RGB only — alpha is forced to `ColorU::ALPHA_OPAQUE` (`255`). Pass `OptionColorU::None` to start at black.

## Limitations

- **Synchronous.** All four namespaces block the calling thread until dismissed. Inside a callback this stops event delivery to every window in your app. For long-running operations triggered *after* the dialog closes (file decode, network upload), spawn a background thread; for the dialog itself there is no async escape hatch yet.
- **Filter format is extension-only.** Platforms that accept full glob patterns (e.g. Windows `*.txt;*.md`) lose pattern fidelity — `tfd` normalizes everything to extension lists.
- **No multi-line file filters.** Pass one `FileTypeList` per call; if you need multiple labelled groups, present them in a custom in-window picker instead.
- **No "do not bother me" or default-button highlighting on Linux/GTK.** The `default` argument to `ok_cancel` and `yes_no` is honoured on Windows and macOS; on Linux it is advisory.
- **`MsgBox::ok` strips quotes** from the message. If you display user-supplied text, escape or sanitize it before passing it in.

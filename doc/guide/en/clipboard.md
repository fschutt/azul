---
slug: clipboard
title: Clipboard
language: en
canonical_slug: clipboard
audience: external
maturity: wip
guide_order: 250
topic_only: false
prerequisites: [events]
tracked_files:
  - layout/src/managers/clipboard.rs
  - layout/src/managers/selection.rs
  - layout/src/callbacks.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T22:00:00Z
---

# Clipboard

> **WIP** — copy/cut/paste of plain text works on every supported platform; styled-text round-trip and HTML clipboard formats are partially implemented (HTML is generated but not yet written to the system clipboard). API is stable.

The clipboard is hooked from inside callbacks via `CallbackInfo` helper methods, not from a dedicated event filter. The `ClipboardManager` (held by `LayoutWindow`) acts as a buffer between the OS clipboard and the application: when the user presses Ctrl+V, the platform shell reads the system clipboard *first*, stages the content in the manager, then runs your paste callback so you can inspect or override what gets inserted. Copy and cut go the other way — the manager stages the proposed content, your callback can rewrite it, and the platform shell commits the final value to the OS clipboard.

```rust,no_run
# use azul::callbacks::{CallbackInfo, RefAny, Update};
# use azul_layout::managers::selection::ClipboardContent;
# use azul_css::AzString;
fn on_paste(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    if let Some(content) = info.get_clipboard_content() {
        println!("about to paste: {:?}", content.plain_text.as_str());
    }
    Update::DoNothing
}
```

## Data model

`ClipboardContent` (`layout/src/managers/selection.rs:460`) is the unit of currency for every clipboard operation. It carries plain text plus an ordered list of styled runs, so a future rich-text round-trip works without changing the API:

```rust,ignore
#[repr(C)]
pub struct ClipboardContent {
    pub plain_text: AzString,            // UTF-8, the lowest-common-denominator format
    pub styled_runs: StyledTextRunVec,   // optional rich-text decomposition
}

#[repr(C)]
pub struct StyledTextRun {
    pub text:        AzString,
    pub font_family: OptionString,
    pub font_size_px: f32,
    pub color:       ColorU,
    pub is_bold:     bool,
    pub is_italic:   bool,
}
```

`ClipboardContent::to_html()` serializes the styled runs into a `<div><span style="…">…</span></div>` blob suitable for the platform's `text/html` clipboard format. Today only `plain_text` is written to the OS clipboard; the HTML branch is wired up but disabled until the per-platform write paths land.

## Reading the clipboard during paste

`CallbackInfo::get_clipboard_content` returns the staged paste content while a paste callback is running. Outside of paste it returns `None`.

```rust,no_run
# use azul::callbacks::{CallbackInfo, RefAny, Update};
# use azul_layout::managers::selection::ClipboardContent;
fn block_long_pastes(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let Some(content) = info.get_clipboard_content() else {
        return Update::DoNothing;
    };

    if content.plain_text.as_str().len() > 10_000 {
        // Returning DoNothing without consuming the staged content
        // suppresses the default text-input insert.
        return Update::DoNothing;
    }

    Update::RefreshDom
}
```

The function returns `Option<&ClipboardContent>` — the manager owns the value, so the borrow lasts for the rest of the callback. Clone the `AzString` if you need to store the text past callback exit.

## Overriding what gets copied

`set_clipboard_content` is the easy path: it queues a `ClipboardContent` for the *current hit node*, so a Copy callback can transform the selected text before the OS clipboard receives it.

```rust,no_run
# use azul::callbacks::{CallbackInfo, RefAny, Update};
# use azul_layout::managers::selection::{ClipboardContent, StyledTextRunVec};
# use azul_css::AzString;
fn rewrite_copy(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from("[copied via my-app]"),
        styled_runs: StyledTextRunVec::default(),
    });
    Update::DoNothing
}
```

For the cut path, use `set_cut_content`. Both helpers also exist in their explicit-target form — `set_copy_content(target, content)` and `set_cut_content(target, content)` (`layout/src/callbacks.rs:3747` and `:3755`) — when you need to override clipboard content for a node other than the one that fired the callback (for example, a toolbar Copy button operating on a separate text editor).

## Inspecting before commit

`inspect_copy_changeset(target)` and `inspect_cut_changeset(target)` peek at whatever has been queued so far for a given node, *before* the platform shell pushes it to the OS. Use them when one callback wants to react to a change another callback already queued — for example, log the final text, or veto the operation by overwriting it with empty content.

```rust,ignore
fn inspect_copy_changeset(&self, target: DomNodeId) -> Option<ClipboardContent>;
fn inspect_cut_changeset (&self, target: DomNodeId) -> Option<ClipboardContent>;
```

These return owned values (the changeset is private to the callback batch), so cloning the data is implicit.

## Platform behaviour

| Platform | Backend | Notes |
|---|---|---|
| Windows | `clipboard-win` (CF_UNICODETEXT) | Plain UTF-16 text. CRLF normalization handled by the crate. |
| macOS | `NSPasteboard` (`NSPasteboardTypeString`) | Plain UTF-8 text. |
| Linux X11 | `x11-clipboard` | Writes to both `CLIPBOARD` and `PRIMARY` selections. |
| Linux Wayland | falls back to X11 via XWayland | Pure-Wayland `wl_data_device` paths are stubbed; build with X11 fallback for now. |

All four backends round-trip plain UTF-8 strings only. Embedded NUL bytes are stripped on read; line endings are not normalized — what you write is what you get back.

## Common pitfalls

- **Calling `get_clipboard_content` outside a paste callback returns `None`.** The manager only stages content while a paste is in flight; the OS clipboard is not polled on demand.
- **Set methods are queued, not synchronous.** `set_copy_content` records a pending change in the callback's changeset; the platform writes to the OS clipboard once the callback batch completes. Do not assume the OS clipboard reflects your change inside the same callback.
- **Styled runs are accepted but not yet committed.** Build them now if you want to be ready for the rich-text path, but assume only `plain_text` reaches another application today.
- **No event filter for clipboard.** Today there is no `On::Copy`/`On::Cut`/`On::Paste` event filter — clipboard interception happens via the system's keyboard-shortcut handler, which calls into the manager regardless of which callbacks are registered. Use a `TextInput` or focus-related event filter and inspect `get_clipboard_content()` from inside it; the planned dedicated filters are tracked in the events page.

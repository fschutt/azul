---
slug: clipboard
title: Clipboard
language: en
canonical_slug: clipboard
audience: external
maturity: wip
guide_order: 250
topic_only: false
short_desc: Reading and writing the system clipboard
prerequisites: [events]
tracked_files:
  - layout/src/managers/clipboard.rs
  - layout/src/managers/selection.rs
  - layout/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Clipboard

> **WIP.** Copy, cut, and paste of plain text work on every supported
> platform. Styled-text round-trip and HTML clipboard formats are partially
> implemented. The API is stable.

Clipboard access happens from inside callbacks via `CallbackInfo` helper
methods. When the user presses paste, the platform reads the system clipboard
first, stages the content, then runs your paste callback so you can inspect or
override what gets inserted. Copy and cut go the other way: your callback can
queue a `ClipboardContent`, and the platform commits it to the OS clipboard
once the callback batch completes.

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_paste(_data: RefAny, info: CallbackInfo) -> Update {
    if let Some(content) = info.get_clipboard_content() {
        println!("about to paste: {:?}", content.plain_text.as_str());
    }
    Update::DoNothing
}
```

## Data model

`ClipboardContent` is the unit of currency for every clipboard operation. It
carries plain text plus an ordered list of styled runs.

```rust,ignore
pub struct ClipboardContent {
    pub plain_text: AzString,
    pub styled_runs: StyledTextRunVec,
}

pub struct StyledTextRun {
    pub text:        AzString,
    pub font_family: OptionString,
    pub font_size_px: f32,
    pub color:       ColorU,
    pub is_bold:     bool,
    pub is_italic:   bool,
}
```

Today only `plain_text` is written to the OS clipboard. Build the styled runs
now if you want to be ready for the rich-text path.

## Reading the clipboard during paste

`CallbackInfo::get_clipboard_content` returns the staged paste content while a
paste callback is running. Outside of paste it returns `None`.

```rust,no_run
# use azul::prelude::*;
extern "C" fn block_long_pastes(_data: RefAny, info: CallbackInfo) -> Update {
    let Some(content) = info.get_clipboard_content() else {
        return Update::DoNothing;
    };

    if content.plain_text.as_str().len() > 10_000 {
        return Update::DoNothing;
    }

    Update::RefreshDom
}
```

## Overriding what gets copied

`CallbackInfo::set_clipboard_content` queues a `ClipboardContent` for the
current hit node, so a Copy callback can transform the selected text before
the OS clipboard receives it.

```rust,no_run
# use azul::prelude::*;
extern "C" fn rewrite_copy(_data: RefAny, info: CallbackInfo) -> Update {
    info.set_clipboard_content(ClipboardContent {
        plain_text: "[copied via my-app]".into(),
        styled_runs: StyledTextRunVec::from_vec(vec![]),
    });
    Update::DoNothing
}
```

For the cut path, `CallbackInfo::set_cut_content` mirrors the copy helper.
Both helpers also exist in their explicit-target form,
`CallbackInfo::set_copy_content(target, content)` and
`CallbackInfo::set_cut_content(target, content)`, when you need to override
clipboard content for a node other than the one that fired the callback.

## Inspecting before commit

`CallbackInfo::inspect_copy_changeset(target)` and
`CallbackInfo::inspect_cut_changeset(target)` peek at whatever has been
queued so far for a given node, before the platform pushes it to the OS. Use
them when one callback wants to react to a change another callback already
queued (for example, to log the final text or to veto the operation by
overwriting it with empty content).

## Common pitfalls

- Calling `get_clipboard_content` outside a paste callback returns `None`. The
  manager only stages content while a paste is in flight.
- Set methods are queued, not synchronous. The platform writes to the OS
  clipboard once the callback batch completes. Don't assume the OS clipboard
  reflects your change inside the same callback.
- Styled runs are accepted but not yet committed. Only `plain_text` reaches
  another application today.

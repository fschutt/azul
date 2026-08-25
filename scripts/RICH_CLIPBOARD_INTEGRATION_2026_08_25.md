# rich-clipboard integration into azul — status, 2026-08-25

Branch `feat/rich-clipboard`, stacked on `feat/multimonitor-and-tray`.

Implements `plan/INTEGRATION.md` from <https://github.com/fschutt/rich-clipboard>:
the OS transport layer (layer 1), which the crate deliberately does not contain.

## What the seam looks like now

`dll/src/desktop/shell2/common/event.rs` used to hold two functions:

```rust
fn get_system_clipboard() -> Option<String>;
fn set_system_clipboard(text: String) -> bool;
```

They now live in a new module, `shell2/common/clipboard.rs`, and carry typed
multi-flavor payloads:

```rust
pub fn get_system_clipboard() -> Option<ClipboardPayload>;
pub fn set_system_clipboard(payload: &ClipboardPayload) -> bool;
```

That module also owns the two conversions between `ClipboardPayload` and
azul's FFI type `ClipboardContent` (`plain_text` + `styled_runs`):
`clipboard_content_to_payload` and `payload_to_clipboard_content`.

Every caller in `event.rs` — `SetCopyContent`, `SetCutContent`,
`CopyToClipboard`, `CutToClipboard`, `PasteFromClipboard`, and the deferred
W3C-clipboard-event path — goes through those two.

## Per-platform state

| Platform | Read | Write | Verified |
|---|---|---|---|
| macOS | every flavor, per pasteboard item | full fan-out | compiles; **not run against a pasteboard** |
| Windows | every format, `GlobalSize`-guarded | full fan-out | cross-compiles only |
| Wayland (native) | advertised mimes only | full fan-out | cross-compiles only |
| X11 | ranked probe list | **plain text only** | cross-compiles only |

### macOS — `shell2/macos/clipboard.rs`, rewritten

Moved off `objc` 0.2 onto `objc2-app-kit` (design decision #2 in the repo:
new backends are objc2-native). The whole file is now `unsafe`-free — the old
one needed a `transmute` of a `&Class` to make `readObjectsForClasses:`
typecheck.

All four documented traps are handled: it walks `-pasteboardItems` rather than
using the pasteboard-level API (which reaches only the first item offering a
type, so a three-file copy read back as one file); it treats a nil
`dataForType:` as a declined promise rather than an error; it dedupes the
legacy UTI twins per item; and it checks `-[NSData length]` *before*
`to_vec()`.

Removing that file's `objc` 0.2 usage left `objc-foundation` and `objc_id`
with no macOS consumer at all, so they are gone from the macOS dependency
section (still present for iOS, which does use them).

### Windows — `shell2/windows/clipboard.rs`, rewritten

Was a 24-line wrapper around `clipboard_win::get_clipboard::<String>`. Now
enumerates with `raw::EnumFormats`, sizes with `raw::size` (`GlobalSize`)
before copying, and writes with one `raw::empty()` followed by
`set_without_clear` per format — `raw::set` empties the clipboard each call,
so a fan-out written with it would leave only the last format.

Predefined formats are named through `WindowsFormat::name()` rather than
`GetClipboardFormatNameW` (which returns nothing for them), which is exactly
what `Flavor::from_windows_name` reads back. `CF_BITMAP`, `CF_ENHMETAFILE`,
`CF_METAFILEPICT`, `CF_PALETTE` and `CF_OWNERDISPLAY` are skipped: they are
handles, and `GlobalSize` on an `HBITMAP` describes nothing.

### Wayland — real fan-out, both directions

`NATIVE_COPY` went from `Option<String>` to `Option<ClipboardPayload>`.
`wl_data_source.offer` is now called once per flavor the payload carries (plus
the pre-MIME `UTF8_STRING` / `text/plain` spellings), and `data_source_send`
serves the representation the peer actually asked for instead of answering
every mime with the same bytes — which would have pasted RTF source into a
plain-text field once more than one flavor was offered.

Reads are driven by the offer's advertised mime list, newly accumulated in
`WaylandDragState::pending_mimes` and promoted at `wl_data_device.selection`
(the same shape as the existing `pending_has_uri_list` promotion). Probing
blind was not an option: `wl_data_offer.receive` with a mime the source never
advertised is answered by a pipe the source need not close, so each wrong
guess costs a full transfer deadline.

## Findings

### 1. Unbounded Wayland pipe read — fixed here (a real bug in azul)

`drain_offer_pipe` in `wayland/events.rs` had a 3-second deadline and **no
byte cap**, so a peer that streams fast enough could push unbounded data into
a `Vec` inside that window. This is precisely the case §4 of INTEGRATION.md
singles out: Wayland is the one platform where the length is unknowable in
advance, and counting while reading is the only defence there is.

Now capped at `MAX_FLAVOR_BYTES` (64 MiB, matching
`rclip_core::Limits::default().max_flavor_bytes`). What arrives past the cap
is **discarded, not truncated** — a truncated flavor is worse than none,
because the decode would succeed on the prefix and paste half a document.

### 2. `x11-clipboard` 0.9.3 cannot enumerate `TARGETS`

`Clipboard::load` rejects any reply whose `type_` differs from the target it
asked for:

```rust
} else if reply.type_ != target {
    return Err(Error::UnexpectedType(reply.type_));
}
```

A `TARGETS` conversion answers with type `ATOM` by definition, so the
enumeration ICCCM prescribes always errors before returning. The X11 read
therefore probes a fixed rank-ordered candidate list and stops at the first
target that times out (a dead owner would otherwise cost the deadline once per
candidate). Flavors azul has no codec for cannot be carried through as
`RichItem::Unknown`, because learning their names requires the `TARGETS` list.

### 3. `x11-clipboard` 0.9.3 cannot publish more than one target

Its owner state is `HashMap<selection, (target, value)>` — one target per
selection — and its `SelectionRequest` handler answers a `TARGETS` query with
exactly that one target. `store()` called twice replaces rather than adds.

So an X11 copy publishes plain text and nothing else. Offering RTF *instead*
would break every plain-text paste target to style one. Lifting this needs a
selection owner that serves several targets, i.e. a rewrite of that crate's
owner loop or a direct `x11rb` connection.

### 4. `SizeHint` is only half-producible

Windows (`GlobalSize`) and macOS (`-[NSData length]`) both give
`SizeHint::Exact` for free before any copy, and both are used. X11's `INCR`
lower bound is **not** reachable through `x11-clipboard`: it reads the value
only to `reserve` the buffer and never reports it, so the X11 guard is a
post-hoc length check instead of a pre-read rejection.

## Copy-side style extraction (the follow-up, now done)

`get_selected_content_for_clipboard` in `layout/src/window.rs` used to build
`styled_runs` empty, so azul read styling but never produced it. It now pulls
per-run formatting off each source `StyledRun`'s `StyleProperties` as the
selection is walked, through a new `ClipboardExtract` accumulator in
`layout/src/managers/selection.rs`.

All three selection shapes — single-run, multi-run and cross-block — go through
that one accumulator, which was half the point: they each built the plain text
separately before, so only one of them would have grown styling by accident.

The mapping is `font_stack.first_selector()` → family, `weight >= 700` → bold
(CSS's own cut), `Italic | Oblique` → italic, plus `font_size_px` and `color`
straight through. Two deliberate choices:

- An embedded `FontRef` publishes **no** family rather than the internal
  `"<embedded-font>"` placeholder, which is a debugging string no receiving
  application could resolve. `None` means inherit, which is the honest answer.
- The cross-block paragraph joiner (`\n`) **inherits** the formatting it
  follows, so it does not split one styled run into three.

Adjacent runs with identical formatting are merged. That is not cosmetic: a
style run is cut at every DOM text node and every styling change, so a
paragraph typed in one font arrives as dozens of runs, and emitting each as its
own `<span>` would produce RTF and HTML several times the size of the text.

The invariant the seam depends on is that `plain_text` is exactly the
concatenation of the runs' text — `content_to_rich_item` drops the rich flavor
outright when they disagree, so a mismatch silently costs all the formatting.
There is a test named for that claim.

## Also removed: the dead `sync_clipboard` chain

`LinuxWindow::sync_clipboard` → `{x11,wayland}/mod.rs::sync_clipboard` →
`clipboard::sync_clipboard` was a four-level chain with no caller, left behind
when the copy/cut/paste shortcuts started writing to the OS clipboard directly.
Both leaf functions carried a TODO asking for exactly this removal. It is gone:
it is not in api.json, so nothing outside the crate could reach it, and keeping
it meant a second, diverging path to the clipboard that still compiled.

## Not done

- **Images, file lists and links as `ClipboardContent`.** `RichItem::Image` /
  `Files` / `Link` decode correctly and are then dropped, because
  `ClipboardContent` has no representation for them. Extending that FFI type is
  an api.json change.
- **XDND / Wayland drag protocols** on top of this seam. Drag-in already works
  in azul through a separate path; unifying the two would let a drop deliver a
  payload.
- **`CFSTR_FILECONTENTS`** (virtual file contents as an `IStream`).
- **Running any of it.** Everything compiles on macOS, Windows and Linux and the
  unit tests pass, but nothing here has been exercised against a live clipboard
  on any platform. That is what the verification prompt handed off with this
  branch is for.

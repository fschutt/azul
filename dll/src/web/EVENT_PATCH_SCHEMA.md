# Azul web backend — event + patch TLV schema (M11 Sprint 7 / 8)

This document is the canonical reference for the wire formats used
between the JS bootstrap loader and the lifted wasm event loop.

Keep in sync with:
  - `dll/src/web/eventloop.rs` (`event_kind` module, `event_offset`
    module, `PATCH_KIND_*` constants, `EVENT_BYTES_LEN`).
  - `dll/src/web/loader_js.rs` (`EVT_*` constants, `azApplyPatches`
    decoder, `azDispatch*` encoders).

## Event payload (JS → wasm)

Fixed-width 256-byte buffer per dispatch. Header is 24 bytes
(20 + per-kind payload length); kind-specific tail starts at
offset 24.

| Offset | Type | Field           | Notes                                    |
|--------|------|-----------------|------------------------------------------|
| 0      | u32  | NODE_IDX        | `0xFFFFFFFF` (SENTINEL_NO_NODE) → wasm-side hit-test runs. Otherwise honored as-is. |
| 4      | u32  | X               | CSS-pixel integer coord (Math.floor of clientX). Scroll events use this for scrollX. |
| 8      | u32  | Y               | CSS-pixel integer coord. Resize events use this for height. |
| 12     | u32  | BUTTON_OR_KEY   | Mouse: `e.button`; Keyboard: `e.keyCode`; Scroll/resize: 0. |
| 16     | u32  | MODIFIERS       | Bitset: bit0=shift, bit1=ctrl, bit2=alt, bit3=meta. |
| 20     | u32  | PAYLOAD_LEN     | Length of per-kind tail bytes starting at offset 24. |
| 24+    | u8[] | PAYLOAD         | Kind-specific. See below. |

**Why integer pixels (not f32 bits)**: `f32::from_bits` proved
unreliable through remill's lift — integer coords sidestep that
conversion entirely. The JS encoder uses
`Math.floor(domEvent.clientX || 0)`.

### Event kinds (`event_kind` module)

| Kind | JS const     | Meaning                                |
|------|--------------|----------------------------------------|
| 0    | EVT_CLICK    | mouse click                            |
| 1    | EVT_MOUSEDOWN| mouse down                             |
| 2    | EVT_MOUSEUP  | mouse up                               |
| 3    | EVT_MOUSEMOVE| mouse move                             |
| 4    | EVT_DBLCLICK | double click                           |
| 5    | EVT_WHEEL    | wheel scroll (deferred, no extra payload yet) |
| 6    | EVT_KEYDOWN  | key press — extends with text payload  |
| 7    | EVT_KEYUP    | key release                            |
| 8    | EVT_FOCUSIN  | focus gained                           |
| 9    | EVT_FOCUSOUT | focus lost                             |
| 10   | EVT_RESIZE   | window resize (x=w, y=h)               |
| 11   | EVT_SCROLL   | scroll (x=scrollX, y=scrollY)          |

Deferred (Stage A.6 in the M11 plan): touch (TouchList encoding),
drag (DataTransfer encoding), composition (IME data).

## Patch payload (wasm → JS)

Each patch is a TLV record:

| Offset | Type | Field        | Notes                                |
|--------|------|--------------|--------------------------------------|
| 0      | u8   | KIND         | See `PATCH_KIND_*` table below.       |
| 1      | u32  | NODE_IDX     | Target DOM node (matches `id="az_N"`).|
| 5      | u32  | PAYLOAD_LEN  | Length of payload bytes.              |
| 9+     | u8[] | PAYLOAD      | Kind-specific. See below.             |

The wasm side emits patches by calling `AzStartup_buildPatch(
out_buf, out_buf_cap, kind, node_idx, payload_ptr, payload_len)`.
JS decodes via `azApplyPatches(ptr, len)` in `loader_js.rs`.

### Patch kinds (`PATCH_KIND_*`)

| Kind | Name              | Payload format                  | JS action                       |
|------|-------------------|---------------------------------|---------------------------------|
| 1    | SetText           | UTF-8 text bytes                | `el.textContent = text`         |
| 2    | SetAttr           | `name\0value\0`                 | `el.setAttribute(name, value)`  |
| 3    | RemoveAttr        | `name\0`                        | `el.removeAttribute(name)`      |
| 4    | SetInlineStyle    | CSS text bytes                  | `el.setAttribute('style', css)` |
| 5    | RemoveNode        | (empty)                         | `el.parentNode.removeChild(el)` |
| 6    | InsertNode        | `parent_idx:u32 \| html_bytes`  | `parent.appendChild(parsedHtml)`|
| 7    | MoveNode          | `new_parent_idx:u32 \| new_sibling_idx:u32` | (TODO: not implemented in JS decoder yet) |
| 8    | ReplaceSubtree    | `new_subtree_html`              | (TODO: not implemented in JS decoder yet) |
| 9    | Focus             | (empty)                         | `el.focus()`                    |
| 10   | ScrollTo          | `x:i32 \| y:i32`                | `el.scrollTo(x, y)`             |
| 11   | AddClass          | class name bytes                | `el.classList.add(name)`        |
| 12   | RemoveClass       | class name bytes                | `el.classList.remove(name)`     |

## CallbackChange → patch-kind mapping (Sprint 7)

The `azul_layout::callbacks::CallbackChange` enum produced by user
callbacks maps to TLV patch kinds as follows. Variants the bench
needs (Sprint 7 scope) are marked **bench**.

| `CallbackChange` variant         | Patch kind   | Notes                              |
|----------------------------------|--------------|------------------------------------|
| `ChangeNodeText` **bench**       | 1 SetText    | direct                             |
| `SetFocusTarget` **bench**       | 9 Focus      | node_idx points at target          |
| `ScrollTo` **bench**             | 10 ScrollTo  | x/y in payload                     |
| `ChangeNodeCssProperties` / `OverrideNodeCssProperties` **bench** | 4 SetInlineStyle | serialized via `format_css` (deferred) |
| `StopPropagation` / `PreventDefault` | (return flag) | dispatch return value, not a TLV patch |
| `AddTimer` / `RemoveTimer`       | (deferred)   | needs JS `setInterval` + `AzStartup_fireTimer` |
| `ScrollIntoView`                 | 15 (deferred)| (analog to ScrollTo)               |
| `AddImageToCache`                | 16 (deferred)| `URL.createObjectURL(Blob([png]))` |
| `OpenMenu`                       | 17 (deferred)| `<div class="az-menu">` overlay    |
| `ShowTooltip` / `HideTooltip`    | 18/19 (deferred) | cursor-positioned `<div>`      |
| `SetCopyContent` / `SetCutContent` | 21 (deferred) | `navigator.clipboard.writeText` |
| `InsertChildNode` / `DeleteNode` | 6 / 5 (via RefreshDom diff loop) | indirect — RefreshDom relayouts + diff produces kind=5/6 |
| `SwitchRoute`                    | 22 (deferred)| `azNavigate(path)`                 |
| `ModifyWindowState`              | (deferred)   | `document.title = ...` etc.        |
| `AddThread` / `RemoveThread`     | (stub)       | no web threading yet (warn)        |

## Bootstrap sequence

```
1. azBootstrap()                          (loader.js)
2.   fetch + instantiate mini.wasm
3.   AzStartup_init(0, 0)                 → state pointer
4.   azLoadBoundaryShards()               (sharded mode)
5.   azHydrate()                          → AzRefAny pointer
6.   for each [data-az-cb][data-az-wasm]:
        fetch + instantiate cb wasm
        table.set(node_idx, cb.exports.callback)
        AzStartup_registerCbNode(state, node_idx)
7.   fetch + instantiate /az/layout/*.wasm
8.   AzStartup_setLayoutCbTableIdx + setRefAny
9.   AzStartup_initLayoutCache(state, viewportW, viewportH, 0)
10.  AzStartup_hydrateStyledDom(state)    (S1.B)
11.  AzStartup_solveLayout(state, w, h)   (S1.C / Sprint 2)
12.  azWireListeners()                    — every kind in the
                                            event-kinds table above
```

## Dispatch sequence (per event)

```
1. azDispatch(kind, domEvent)             (loader.js)
2.   alloc 256-byte evtPtr + 4-byte outLenPtr
3.   encode event header (NODE_IDX=SENTINEL, x, y, button, mods)
4.   AzStartup_dispatchEvent(state, kind, evtPtr, 256, outLenPtr)
       a. wasm hit-tests via positioned_rects cache
       b. wasm resolves cb fn-addr → table_idx
       c. wasm call_indirect(cb)
       d. cb mutates state via hydrated RefAny
       e. wasm reads cb's Update return
       f. if RefreshDom: relayout + diff + emit patches
5.   patchesLen = outLenPtr u32 read
6.   if patchesLen > 0:
        azApplyPatches(patchesPtr, patchesLen)
7.   free evtPtr + outLenPtr
```

## What's NOT yet wired (intentional)

  - **Real CallbackInfo wasm-side**: today the cb receives
    `event_bytes_ptr` as its info arg. Sprint 7 *narrow* status:
    patch infrastructure is in place; real `CallbackInfo` blob
    with `Arc<Mutex<Vec<CallbackChange>>>` drain via
    `take_changes` deferred. User cbs that call
    `CallbackInfo::*_change_*` setters today no-op on web.
  - **`MoveNode` / `ReplaceSubtree` decoder**: TLV format is
    documented; JS decoder logs and skips.
  - **`AddTimer` / `RemoveTimer`**: JS `setInterval` +
    `AzStartup_fireTimer` deferred (Sprint 7 C.3).
  - **`AddImageToCache` / `OpenMenu` / `ShowTooltip` /
    `SetCopyContent` / `SetCutContent`**: deferred per M11 plan
    Stage C.5.
  - **`AddThread` / `RemoveThread`**: no web threading; warn on
    encounter.

# NodeData Optimization Plan

**Goal:** Reduce `NodeData` from **320 bytes** to **~136 bytes** (–57%).

## Current Layout (320 B)

```
Offset  Field               Type                            Size  Align
  0     node_type           NodeType                         72B    8     ← largest: IFrame(IFrameNode) = 64B payload
 72     dataset             OptionRefAny                     32B    8     ← rarely used
104     ids_and_classes     IdOrClassVec                     48B    8     ← C-Vec (ptr+len+cap+destructor+run_destructor)
152     attributes          AttributeTypeVec                 48B    8     ← rarely used on most nodes
200     callbacks           CoreCallbackDataVec              48B    8
248     css_props           CssPropertyWithConditionsVec     48B    8
296     tab_index           OptionTabIndex                   12B    4     ← repr(C, u8) enum: tag + pad + u32
308     contenteditable     bool                              1B    1
309     (padding)                                             3B         → alignment for next 8-align field
312     extra               Option<Box<NodeDataExt>>          8B    8     ← niche-optimized
                                                      TOTAL: 320B
```

## Optimization Steps

### Step 1: Merge `run_destructor` into Destructor enum (saves 8 B per Vec)

**Current C-Vec layout (48 B):**
```rust
#[repr(C)]
pub struct XxxVec {
    ptr: *const T,            //  8B
    len: usize,               //  8B
    cap: usize,               //  8B
    destructor: XxxDestructor, // 16B (repr(C, u8): tag 1B + 7B pad + 8B fn ptr)
    run_destructor: bool,     //  1B + 7B padding
}                             // = 48B
```

**New C-Vec layout (40 B):**
```rust
#[repr(C)]
pub struct XxxVec {
    ptr: *const T,            //  8B
    len: usize,               //  8B
    cap: usize,               //  8B
    destructor: XxxDestructor, // 16B (repr(C, u8): tag 1B + 7B pad + 8B fn ptr)
}                             // = 40B

#[repr(C, u8)]
pub enum XxxDestructor {
    DefaultRust,               // = run_destructor: true, use Rust Vec dealloc
    NoDestructor,              // = run_destructor: false (static slice or already destroyed)
    AlreadyDestroyed,          // = run_destructor was false (post-drop state)
    External(XxxDestructorType), // = run_destructor: true, use external fn
}
```

The `run_destructor: bool` field exists to prevent double-free. After drop runs,
the current code sets `self.run_destructor = false`. Instead, we set
`self.destructor = XxxDestructor::AlreadyDestroyed` (or `NoDestructor`).

This is semantically cleaner: the destructor enum fully describes the lifecycle
state. The `Drop` impl becomes:

```rust
impl Drop for XxxVec {
    fn drop(&mut self) {
        match self.destructor {
            XxxDestructor::DefaultRust => {
                let _ = unsafe {
                    Vec::from_raw_parts(self.ptr as *mut T, self.len, self.cap)
                };
                self.destructor = XxxDestructor::AlreadyDestroyed;
            }
            XxxDestructor::External(f) => {
                f(self);
                self.destructor = XxxDestructor::AlreadyDestroyed;
            }
            XxxDestructor::NoDestructor | XxxDestructor::AlreadyDestroyed => {}
        }
    }
}
```

**Impact:**
- All `impl_vec!` types shrink by 8 bytes: **48 → 40 B**
- `AzString` (= `U8Vec`) shrinks: **48 → 40 B**
- `RefCount` also has `run_destructor: bool` — same pattern applies (16 → 8 B), 
  but that is a separate optimization and NOT part of this plan.

**Files to change:**
- `css/src/macros.rs`: `impl_vec!` macro — remove `run_destructor` field, add
  `AlreadyDestroyed` variant, update `Drop` impl, update `from_vec()` / `from_const_slice()`
- `css/src/macros.rs`: `impl_vec_clone!` macro — clone must not copy `AlreadyDestroyed`;
  if source is `AlreadyDestroyed` or `NoDestructor`, clone gets `NoDestructor`
- `doc/src/autofix/mod.rs` / `doc/src/autofix/diff.rs` / `doc/src/patch/index.rs`:
  Update the `impl_vec!` detection to generate api.json entries without `run_destructor`
  field and with the new `AlreadyDestroyed` destructor variant
- `api.json`: All Vec types lose the `run_destructor` struct field; all Destructor
  enums gain `AlreadyDestroyed` variant. Generate patches via `cargo run --package azul-doc -- autofix`.
- All codegen outputs regenerated via `cargo run --package azul-doc -- codegen all`.

**Verification:**
- `cargo test --workspace` — all tests pass
- `cargo run --package azul-doc -- autofix` — 0 critical warnings
- `cargo build --package azul-dll --features build-dll --release` — 0 transmute errors

---

### Step 2: Move IFrame payload to NodeDataExt (shrinks NodeType 72 → 48 B)

IFrame nodes are extremely rare (~0.1% of all nodes), yet `IFrameNode` (64 B)
forces every `NodeType` to pay 72 B (tag + pad + 64B payload).

**Change:**
- Keep `NodeType::IFrame` as a **unit variant** (no payload).
- Move `IFrameNode` into `NodeDataExt`:
  ```rust
  pub struct NodeDataExt {
      // ... existing fields ...
      pub iframe: Option<Box<IFrameNode>>,   // 8B (niche-optimized)
  }
  ```
- When constructing an iframe node: `node_type = NodeType::IFrame`,
  `extra = Some(Box::new(NodeDataExt { iframe: Some(Box::new(iframe_node)), .. }))`.

**New `NodeType` layout:**
```
tag(1B) + 7B pad + max(AzString=40B after Step 1) = 48B
```

After Step 1 (AzString = 40B), largest remaining payload variants are:
- `Text(AzString)` = 40B
- `Image(ImageRef)` = 24B (ptr + copies + run_destructor = 8+8+1 + 7pad = 24B)
- `Icon(AzString)` = 40B

So `NodeType` = **48 B** (down from 72 B = –24 B).

**Files to change:**
- `core/src/dom.rs`: `NodeType::IFrame` becomes unit variant
- `core/src/dom.rs`: `NodeDataExt` gains `iframe: Option<Box<IFrameNode>>`
- `core/src/dom.rs`: `NodeData` constructors / `set_iframe()` / `get_iframe()`
  methods adjusted
- All layout code that pattern-matches `NodeType::IFrame(node)` → reads from
  `NodeDataExt` instead
- `api.json`: `NodeType` enum_fields updated, `NodeDataExt` struct_fields updated

---

### Step 3: Remove `IdOrClassVec`, merge into `AttributeTypeVec`

`AttributeType` already has `Id(AzString)` and `Class(AzString)` variants.
The separate `IdOrClassVec` is redundant.

**Change:**
- Remove `ids_and_classes: IdOrClassVec` from `NodeData`
- All IDs/classes go into the `attributes: AttributeTypeVec`
- `AttributeType::Id` and `AttributeType::Class` already exist, with `AzString`
  payload — identical functionality to `IdOrClass::Id(AzString)` / `IdOrClass::Class(AzString)`

**This is a larger refactoring** because many parts of the codebase iterate
`ids_and_classes` for CSS matching. We need to:
1. Remove `IdOrClassVec` field from `NodeData`
2. Remove `IdOrClass` enum and `IdOrClassVec` type (or deprecate)
3. Update all `ids_and_classes.as_ref().iter()` → filter `attributes` for Id/Class
4. Update CSS selector matching (layout engine) to look in `attributes`
5. Update all `NodeData::with_id()` / `with_class()` helpers to push into `attributes`
6. Update `NodeData::Hash` impl
7. Move `attributes` from `NodeDataExt` consideration (it's now always needed)
   back into `NodeData` as the primary attribute store

Note: after this change, `attributes` is still in `NodeData` (not moved to ext),
since it now holds IDs and classes too.

**Savings:** –40 B (one `XxxVec` field removed after Step 1, each Vec = 40 B).

---

### Step 4: Move `dataset` to NodeDataExt (saves 32 B)

`dataset: OptionRefAny` (32 B) is rarely used — most nodes don't carry
application data.

**Change:**
- Remove `dataset: OptionRefAny` from `NodeData`
- Add `dataset: Option<RefAny>` to `NodeDataExt` (uses Rust `Option` niche
  optimization — `RefAny` has a pointer, so `Option<RefAny>` = 24 B when `Some`,
  0 cost in `NodeDataExt` when absent since it's already heap-allocated)
- Update `NodeData::set_dataset()` / `get_dataset()` to read/write through `extra`
- Since `NodeDataExt` is behind `Option<Box<...>>`, allocation only happens when
  dataset (or any other ext field) is actually set

**Savings:** –32 B

---

### Step 5: Merge `OptionTabIndex` + `contenteditable` into packed `u32`

**Current:** `OptionTabIndex` (12 B) + `contenteditable` (1 B) + 3 B padding = **16 B**

`TabIndex` has three variants:
- `Auto` (tag only)
- `OverrideInParent(u32)` (tag + u32)
- `NoKeyboardFocus` (tag only)

Realistically, `OverrideInParent` values are small (< 2^30). We can pack
everything into a single `u32`:

```rust
/// Packed representation of tab index + contenteditable flag.
///
/// Bit layout (32 bits):
///   [31]     contenteditable flag (1 = true)
///   [30:29]  tab_index variant:
///              00 = None (no tab index set)
///              01 = Auto
///              10 = OverrideInParent (value in bits [28:0])
///              11 = NoKeyboardFocus
///   [28:0]   OverrideInParent value (max ~536 million, more than enough)
///
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeFlags(pub u32);

impl NodeFlags {
    pub const CONTENTEDITABLE_BIT: u32  = 1 << 31;
    pub const TAB_INDEX_SHIFT: u32      = 29;
    pub const TAB_INDEX_MASK: u32       = 0b11 << 29;
    pub const TAB_VALUE_MASK: u32       = (1 << 29) - 1;

    pub const TAB_NONE: u32             = 0b00 << 29;
    pub const TAB_AUTO: u32             = 0b01 << 29;
    pub const TAB_OVERRIDE: u32         = 0b10 << 29;
    pub const TAB_NO_KEYBOARD: u32      = 0b11 << 29;
}
```

**Change:**
- Replace `tab_index: OptionTabIndex` + `contenteditable: bool` with
  `flags: NodeFlags` (4 B)
- Provide getter/setter methods: `get_tab_index() -> Option<TabIndex>`,
  `set_tab_index(Option<TabIndex>)`, `is_contenteditable() -> bool`,
  `set_contenteditable(bool)`
- Update all call sites

**Savings:** 16 B → 4 B = **–12 B** (but with alignment, actual gain depends on
position; placed after `css_props` which is 8-aligned, the 4B `u32` will have
4B padding before `extra`. Net: 16 B → 8 B = **–8 B**)

To get the full 12 B saving, we can place `flags` as the very last field before
`extra`, or even embed it differently. With the struct layout after all changes,
alignment should work out.

---

## Resulting Layout

After all 5 steps:

```
Offset  Field          Type                            Size  Align
  0     node_type      NodeType                         48B    8   (Step 1+2: IFrame→ext, Vec→40B)
 48     callbacks      CoreCallbackDataVec              40B    8   (Step 1: 48→40)
 88     css_props      CssPropertyWithConditionsVec     40B    8   (Step 1: 48→40)
128     flags          NodeFlags (u32)                   4B    4   (Step 5: tab+contenteditable packed)
132     (padding)                                        4B        → align to 8 for extra
136     extra          Option<Box<NodeDataExt>>          8B    8
                                                  TOTAL: 144B
```

Wait — we also need `attributes`:

```
Offset  Field          Type                            Size  Align
  0     node_type      NodeType                         48B    8   (Step 1+2)
 48     attributes     AttributeTypeVec                 40B    8   (Step 1+3: now holds ids/classes too)
 88     callbacks      CoreCallbackDataVec              40B    8   (Step 1)
128     css_props      CssPropertyWithConditionsVec     40B    8   (Step 1)
168     flags          NodeFlags (u32)                   4B    4   (Step 5)
172     (padding)                                        4B        → align to 8
176     extra          Option<Box<NodeDataExt>>          8B    8
                                                  TOTAL: 184B
```

**184 B** = **–42.5%** reduction from 320 B.

### Alternative: Move `attributes` to NodeDataExt too

If we also move `attributes` to `NodeDataExt` (as the attached analysis suggests),
most leaf nodes (plain `Div`, `Span`, etc.) would only need the fixed fields:

```
Offset  Field          Type                            Size  Align
  0     node_type      NodeType                         48B    8
 48     callbacks      CoreCallbackDataVec              40B    8
 88     css_props      CssPropertyWithConditionsVec     40B    8
128     flags          NodeFlags (u32)                   4B    4
132     (padding)                                        4B
136     extra          Option<Box<NodeDataExt>>          8B    8
                                                  TOTAL: 144B
```

In this variant, `NodeDataExt` holds ids, classes, and other HTML attributes via
a single `AttributeTypeVec`. Nodes that need IDs/classes get an `extra` allocation.

**Trade-off:** Most styled nodes DO have at least one class, so this may cause
many `NodeDataExt` allocations. The question is whether the memory saving per node
(40 B) outweighs the overhead of a small heap allocation (~16 B allocator overhead + 
`NodeDataExt` struct size). For DOMs with thousands of nodes, the net saving is
still significant.

**Recommendation:** Keep `attributes` (with merged ids+classes) in `NodeData` 
for now → **184 B**. If profiling shows further need, move it to ext later.

---

## Summary

| Step | Change | Savings | New Total |
|------|--------|---------|-----------|
| 0 | Baseline | — | **320 B** |
| 1 | `run_destructor` → `AlreadyDestroyed` in Destructor | –8 B × 4 Vecs – 8 B (AzString in NodeType) = –40 B | **280 B** |
| 2 | `IFrame` payload → `NodeDataExt` | –24 B | **256 B** |
| 3 | Remove `IdOrClassVec`, merge into `AttributeTypeVec` | –40 B | **216 B** |
| 4 | `dataset` → `NodeDataExt` | –32 B | **184 B** |
| 5 | `OptionTabIndex` + `contenteditable` → `NodeFlags(u32)` | –8 B | **176 B**¹ |

¹ Actual measured size depends on final field ordering and alignment; likely **176–184 B**.

### Implementation Order

1. **Step 1** first — it's the broadest change (all Vec types + AzString) and
   requires autofix/codegen updates. All other steps depend on the new Vec size.
2. **Step 2** next — straightforward, self-contained.
3. **Step 5** next — small, self-contained bit-packing.
4. **Step 4** next — simple move to ext.
5. **Step 3** last — largest refactoring surface area (CSS matching, DOM construction,
   many call sites).

### Autofix / Codegen Impact

- **Step 1** requires updating `impl_vec!` macro detection in `doc/src/patch/index.rs`
  and `doc/src/autofix/` to remove `run_destructor` from Vec struct_fields and add
  `AlreadyDestroyed` to destructor enum_fields. All Vec types in `api.json` change.
- **Steps 2–5** change `NodeData`, `NodeType`, `NodeDataExt` in `api.json`.
  These are standard struct/enum field changes handled by the existing autofix diff.

### NodeDataExt After All Steps

```rust
#[repr(C)]
pub struct NodeDataExt {
    pub clip_mask: Option<ImageMask>,
    pub accessibility: Option<Box<AccessibilityInfo>>,
    pub menu_bar: Option<Box<Menu>>,
    pub context_menu: Option<Box<Menu>>,
    pub iframe: Option<Box<IFrameNode>>,         // NEW (Step 2)
    pub dataset: Option<RefAny>,                  // NEW (Step 4), was OptionRefAny on NodeData
    pub dataset_merge_callback: Option<DatasetMergeCallback>,
    pub component_origin: Option<ComponentOrigin>,
    pub is_anonymous: bool,
    pub key: Option<u64>,
}
```

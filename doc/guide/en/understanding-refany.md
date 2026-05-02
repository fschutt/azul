---
slug: understanding-refany
title: Understanding RefAny
language: en
canonical_slug: understanding-refany
audience: external
maturity: mature
guide_order: 25
topic_only: false
prerequisites: [architecture]
tracked_files:
  - core/src/refany.rs
  - core/src/callbacks.rs
  - core/src/dom.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

# Understanding RefAny

`RefAny` is a type-erased, atomically reference-counted smart pointer with
runtime borrow checking. Conceptually `Arc<RefCell<dyn Any>>`, but
`#[repr(C)]` so the same value moves across the C ABI into Python, C++,
and other bindings. It is the only piece of state plumbing the framework
gives you — every callback, dataset, timer, thread, and component
backreference is a `RefAny`.

```rust
# extern crate azul_core;
use azul_core::refany::RefAny;

struct Counter { value: i64 }

let mut data = RefAny::new(Counter { value: 0 });
let mut clone = data.clone(); // cheap — bumps a refcount

if let Some(mut c) = clone.downcast_mut::<Counter>() {
    c.value += 1;
}

assert_eq!(data.downcast_ref::<Counter>().unwrap().value, 1);
```

## What RefAny stores

`RefAny` heap-allocates the value once and stores a pointer to it
alongside metadata (type id, type name, destructor, layout, atomic
counters) in a separate heap allocation, the `RefCountInner`. Every clone
of a `RefAny` shares both allocations.

| Field on `RefCountInner` | Purpose |
|---|---|
| `_internal_ptr`         | Pointer to the heap-allocated value |
| `num_copies`            | Number of `RefAny` instances sharing the data |
| `num_refs`              | Active shared borrows (`Ref<T>`) |
| `num_mutable_refs`      | Active mutable borrows (`RefMut<T>`) |
| `type_id` / `type_name` | Runtime type identity for safe downcasts |
| `custom_destructor`     | `extern "C" fn(*mut c_void)` — runs `T::drop` |
| `_internal_layout_*`    | Size and alignment captured for deallocation |

Source: `core/src/refany.rs:69`.

## Constructing a RefAny

```rust
# extern crate azul_core;
# use azul_core::refany::RefAny;
struct AppData { user: String, count: u32 }

let data = RefAny::new(AppData {
    user: "alice".to_string(),
    count: 0,
});
```

`RefAny::new<T>` (`core/src/refany.rs:590`) records `TypeId::of::<T>()`,
allocates with `Layout::from_size_align(size_of::<T>(), align_of::<T>())`,
copies the value onto the heap, and `mem::forget`s the original to skip
its destructor — the destructor stored in `RefCountInner` will run later
when the last reference drops.

`T` must be `'static`. Borrowed references cannot live inside a `RefAny`
because the runtime cannot enforce a static lifetime through the C ABI.

## Cloning

```rust
# extern crate azul_core;
# use azul_core::refany::RefAny;
# let data = RefAny::new(0u32);
let a = data.clone();
let b = data.clone();
assert_eq!(a.get_ref_count(), 3); // original + 2 clones
```

`Clone` (`core/src/refany.rs:1231`) atomically increments `num_copies`
with `SeqCst` ordering and assigns the clone a unique `instance_id`. No
data is copied. A clone is a few atomic instructions — pass `RefAny`s
freely.

## Borrowing the inner value

```rust
# extern crate azul_core;
# use azul_core::refany::RefAny;
let mut data = RefAny::new(42i32);

{
    let r = data.downcast_ref::<i32>().unwrap(); // shared borrow
    assert_eq!(*r, 42);
} // borrow released here

if let Some(mut m) = data.downcast_mut::<i32>() { // mutable borrow
    *m = 100;
}
```

| Method | Returns | Fails when |
|---|---|---|
| `downcast_ref<U>(&mut self)`  | `Option<Ref<'_, U>>`    | wrong type, or a mutable borrow is live |
| `downcast_mut<U>(&mut self)`  | `Option<RefMut<'_, U>>` | wrong type, or any borrow is live |
| `is_type(type_id)`            | `bool`                  | — |
| `get_type_name()`             | `AzString`              | — |
| `replace_contents(new_value)` | `bool`                  | borrow is live (returns `false`) |

Both downcasts take `&mut self` so the compiler prevents two borrows from
the *same* `RefAny` value. Borrows from *different clones* of the same
data are guarded by the runtime counters and the `Ref` / `RefMut` RAII
guards (`core/src/refany.rs:405`, `core/src/refany.rs:440`).

Returning `Option` instead of panicking is deliberate. A failed downcast
reports a mismatch a callback can react to (return `Update::DoNothing`)
instead of aborting the process.

## Wiring a RefAny into a Dom

```rust,ignore
use azul_core::dom::{Dom, EventFilter, HoverEventFilter};
use azul_core::callbacks::{CallbackInfo, Update};
use azul_core::refany::RefAny;

struct Counter { value: i64 }

extern "C" fn on_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut c) = data.downcast_mut::<Counter>() {
        c.value += 1;
    }
    Update::RefreshDom
}

fn build() -> Dom {
    let state = RefAny::new(Counter { value: 0 });
    Dom::div().with_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        state,
        on_click,
    )
}
```

`Dom::with_callback` (`core/src/dom.rs:4940`) stores the `RefAny`
alongside the callback. When the event fires, the framework clones the
`RefAny` (cheap), invokes the `extern "C"` callback with that clone, and
drops the clone after the callback returns. The original `RefAny`
remains attached to the node for the next event.

`Dom::with_dataset` (`core/src/dom.rs:4992`) attaches a `RefAny` without
a callback — useful when child callbacks need to navigate to data
attached to an ancestor.

## The backreference pattern

The architecture page introduces backreferences in the abstract: a
component holds a `RefAny` to *its* parent, so events can flow up the
state graph without touching intermediate components. The mechanic is
direct — store the parent's `RefAny` inside the child's data struct.

```rust,ignore
use azul_core::dom::{Dom, EventFilter, FocusEventFilter};
use azul_core::callbacks::{CallbackInfo, Update};
use azul_core::refany::{RefAny, OptionRefAny};

// Lowest level: a text input with a focus-lost hook.
struct TextInput {
    text: String,
    on_focus_lost: OptionRefAny,        // backreference to parent
    on_focus_lost_cb: Option<extern "C" fn(RefAny, CallbackInfo, &str) -> Update>,
}

impl TextInput {
    fn new(text: String) -> Self {
        Self { text, on_focus_lost: OptionRefAny::None, on_focus_lost_cb: None }
    }

    fn set_on_focus_lost(
        &mut self,
        parent: RefAny,
        cb: extern "C" fn(RefAny, CallbackInfo, &str) -> Update,
    ) {
        self.on_focus_lost = OptionRefAny::Some(parent);
        self.on_focus_lost_cb = Some(cb);
    }

    fn dom(self) -> Dom {
        let state = RefAny::new(self);
        Dom::div().with_callback(
            EventFilter::Focus(FocusEventFilter::FocusLost),
            state,
            text_input_focus_lost,
        )
    }
}

extern "C" fn text_input_focus_lost(mut data: RefAny, info: CallbackInfo) -> Update {
    let mut me = match data.downcast_mut::<TextInput>() {
        Some(m) => m,
        None => return Update::DoNothing,
    };
    let cb = match me.on_focus_lost_cb { Some(cb) => cb, None => return Update::DoNothing };
    let parent = match me.on_focus_lost.as_option() {
        Some(p) => p.clone(),
        None => return Update::DoNothing,
    };
    let text = me.text.clone();
    drop(me); // release the mutable borrow before re-entering user code
    cb(parent, info, &text)
}
```

A higher-level component (`NumberInput`) wraps `TextInput`. It stores
*its* parent's `RefAny` in the same way and passes a `RefAny` to itself
into the `TextInput`:

```rust,ignore
struct NumberInput {
    on_number: OptionRefAny,            // backreference to AgeInput
    on_number_cb: Option<extern "C" fn(RefAny, CallbackInfo, i64) -> Update>,
}

impl NumberInput {
    fn dom(self, initial: i64) -> Dom {
        let me = RefAny::new(self);
        let mut text = TextInput::new(initial.to_string());
        text.set_on_focus_lost(me, validate_number);
        text.dom()
    }
}

extern "C" fn validate_number(mut data: RefAny, info: CallbackInfo, s: &str) -> Update {
    let mut me = match data.downcast_mut::<NumberInput>() {
        Some(m) => m,
        None => return Update::DoNothing,
    };
    let n: i64 = match s.parse() { Ok(n) => n, Err(_) => return Update::DoNothing };
    let cb = match me.on_number_cb { Some(cb) => cb, None => return Update::DoNothing };
    let parent = match me.on_number.as_option() {
        Some(p) => p.clone(),
        None => return Update::DoNothing,
    };
    drop(me);
    cb(parent, info, n)
}
```

The chain at runtime:

1. `text_input_focus_lost` runs with `RefAny<TextInput>`.
2. It calls `validate_number(parent: RefAny<NumberInput>, ..., "32")`.
3. `validate_number` parses the string and calls
   `app_callback(parent: RefAny<AgeInput>, ..., 32_i64)`.

`TextInput` never sees `AgeInput`. `AgeInput` never sees `TextInput`.
The state graph is `AgeInput → NumberInput → TextInput`, set up at
construction time and traversed by following clones of the `RefAny`s
stored inside each level's data.

## Always drop the borrow before re-entering user code

`downcast_mut` holds a runtime mutable borrow until its `RefMut` guard
drops. If you call a user-supplied callback while still holding it and
the callback re-enters your component (touches the same `RefAny`), the
inner downcast returns `None`. The pattern in the example above —
extracting whatever you need into local variables, calling `drop(me)`
explicitly, then dispatching — keeps the borrow window minimal.

```rust,ignore
let mut me = data.downcast_mut::<NumberInput>().unwrap();
let parent = me.on_number.as_option().unwrap().clone(); // clone the RefAny
let cb = me.on_number_cb.unwrap();
drop(me);                                                 // release the borrow
cb(parent, info, value)                                   // safe to re-enter
```

## Memory and threading

`RefAny` is `Send` and `Sync` (`core/src/refany.rs:536`). The data is
heap-allocated and the counters are `AtomicUsize` with `SeqCst` ordering,
so a `RefAny` can be moved or shared into a `Task` or `Thread`. The
runtime borrow checker is per-`RefAny` value, not per-data — concurrent
mutable downcasts on *clones* of the same `RefAny` race because the
check-then-increment in `downcast_mut` is not atomic. For exclusive
mutation across clones, use `replace_contents`
(`core/src/refany.rs:1094`), which uses `compare_exchange` on
`num_mutable_refs`.

Deallocation is automatic. When the last `RefAny` clone drops:

1. `RefCount::drop` (`core/src/refany.rs:185`) sees `num_copies == 1`.
2. It reclaims `RefCountInner` via `Box::from_raw`.
3. It runs `custom_destructor` on the data pointer (executes `T::drop`).
4. It calls `dealloc` with the saved layout to release the bytes.

Borrow guards (`Ref`, `RefMut`) clone the `RefCount`, so they keep the
data alive even if the original `RefAny` is dropped. A guard outliving
its parent is rare but legal.

## Common pitfalls

- **`'static` only.** A struct that borrows from another value cannot
  go through `RefAny::new`. Either own the data or wrap the borrowed
  source in a `RefAny` and clone that into the child.
- **Wrong type id.** `downcast_ref::<Foo>()` on a `RefAny` constructed
  from `Bar` returns `None`, not a panic. Check the result.
- **Holding borrows across callbacks.** A callback that calls another
  callback while still holding a `RefMut` to the same data will see
  `None` on re-entry. Drop guards before dispatching.
- **Borrow leaks across threads.** A `Ref<T>` sent to another thread
  keeps the read borrow alive there; mutations on any clone block until
  it returns. Prefer cloning the `RefAny` itself across threads and
  taking borrows locally.
- **Cycles.** Two structs holding `RefAny` clones of each other will
  never drop. Use one direction of backreference only — children point
  at parents, never the reverse.

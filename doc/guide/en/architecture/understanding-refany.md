---
slug: architecture/understanding-refany
title: Understanding RefAny
language: en
canonical_slug: architecture/understanding-refany
audience: external
maturity: mature
guide_order: 25
topic_only: false
short_desc: RefAny helps you to share and store data type-erased data between callbacks and threads
prerequisites: [architecture]
tracked_files:
  - core/src/refany.rs
  - core/src/callbacks.rs
  - core/src/dom.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:48:36Z
---

# Understanding RefAny

`RefAny` is a type-erased, atomically reference-counted smart pointer with
runtime borrow checking. Conceptually `Arc<RefCell<dyn Any>>`, but
`#[repr(C)]` so the same value moves across the C ABI into Python, C++,
and other bindings. It is the only piece of state plumbing the framework
imposes on you - every piece of callback data, dataset data, timer data, thread data, 
and component backreference is a `RefAny`.

```rust
extern crate azul_core;
use azul_core::refany::RefAny;

// plain Rust struct - doesn't necessarily have to be repr(C)
struct Counter { 
    value: i64 
}

// internally uses compiler internals to store the "RefAny" TypeId,
// to uniquely identify the type. Additionally stores metadata such as
// name, JSON serializing callbacks (if applicable), memory size, etc. 
// for introspection / debugging
let mut data = RefAny::new(Counter { 
    value: 0 
});
let mut clone = data.clone(); // cheap — bumps a refcount

// refmut "c" object created - bumps refcount
// call only succeeds if no borrow exists
if let Some(mut c) = clone.downcast_mut::<Counter>() {
    c.value += 1;
    // refmut "c" object dropped - refcount decreased
}

assert_eq!(data.downcast_ref::<Counter>().unwrap().value, 1);
```

## What RefAny stores

`RefAny` heap-allocates the value *once* and stores a pointer to it
alongside metadata (type id, type name, destructor, layout, atomic
counters) in a separate heap allocation, the `RefCountInner`. Every clone
of a `RefAny` shares both allocations.

Fields on `RefCountInner`:

- `_internal_ptr`: Pointer to the users heap-allocated value
- `num_copies`: Number of `RefAny` instances sharing the data
- `num_refs`: Active shared borrows (`Ref<T>`)
- `num_mutable_refs`: Active mutable borrows (`RefMut<T>`)
- `type_id` / `type_name`: Runtime type identity for safe downcasts
- `custom_destructor`: `extern "C" fn(*mut c_void)` — runs `T::drop`
- `_internal_layout_*`: Size and alignment captured for deallocation

See `core/src/refany.rs` for explicit documentation around safety / soundness.

## Constructing RefAny

```rust
extern crate azul_core;
use azul_core::refany::RefAny;

struct AppData { 
    user: String, 
    click_count: u32 
}

let data = RefAny::new(AppData {
    user: "alice".to_string(),
    click_count: 0,
});
```

`RefAny::new<T>` records `TypeId::of::<T>()`, allocates with 
`Layout::from_size_align(size_of::<T>(), align_of::<T>())`, copies the 
value onto the heap, and `mem::forget`s the original to skip its destructor: 
the destructor stored in `RefCountInner` will run later when the last reference 
drops. `T` must be `'static` (cannot store references with lifetimes in a `RefAny` 
because the runtime cannot enforce a static lifetime through the C ABI.

## Cloning

```rust
extern crate azul_core;
use azul_core::refany::RefAny;

let data = RefAny::new(0u32);
let a = data.clone();
let b = data.clone();
assert_eq!(a.get_ref_count(), 3); // original + 2 clones
```

`Clone` atomically increments `num_copies` with `SeqCst` ordering and 
assigns the clone a unique `instance_id`. No data is copied. A clone 
is a few atomic instructions — you can pass `RefAny`s freely.

## Borrowing the inner value

```rust
extern crate azul_core;
use azul_core::refany::RefAny;

let mut data = RefAny::new(42i32);

{
    let r = data.downcast_ref::<i32>().unwrap(); // shared borrow
    assert_eq!(*r, 42);
} // borrow released here

if let Some(mut m) = data.downcast_mut::<i32>() { // mutable borrow
    *m = 100;
}
```

- `downcast_ref<U>(&mut self)` returns `Option<Ref<'_, U>>`: fails on wrong type, or a mutable borrow is live
- `downcast_mut<U>(&mut self)` returns `Option<RefMut<'_, U>>`: fails on wrong type, or **any** borrow is live
- `replace_contents(new_value)` returns `bool` (success): returns false borrow is live

For debugging, `RefAny` contains other functions such as `get_type_name()` and
`get_ref_count()` to debug why a downcast is failing.

Both `downcast_*` functions take `&mut self` so the compiler prevents two 
borrows from the *same* `RefAny` value. Borrows from *different clones* of the 
same data are guarded by the runtime counters and the `Ref` / `RefMut` RAII
guards.

Returning `Option` instead of panicking is deliberate. A failed downcast
reports a mismatch a callback can react to (return `Update::DoNothing`)
instead of aborting the process.

## Dataset vs Callback RefAny

```rust
use azul_core::dom::{Dom, EventFilter, HoverEventFilter};
use azul_core::callbacks::{CallbackInfo, Update};
use azul_core::refany::RefAny;

struct Counter { value: i64 }

struct MyCustomStruct { foo: u32 }

extern "C" fn on_click(mut data: RefAny, info: CallbackInfo) -> Update {
    match data.downcast_mut::<Counter>() {
        Some(c) => c.value += 1,
        None => return Update::DoNothing,
    }

    let storage = info.get_dataset(info.get_hit_node_id())
        .and_then(data.downcast_mut::<MyCustomStruct>());

    match storage {
        Some(MyCustomStruct { foo }) => println!("{foo}"),
        None => return Update::DoNothing,
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
    .with_dataset(RefAny::new(MyCustomStruct {
        foo: 0,
    }))
}
```

`Dom::with_callback` stores the `RefAny` alongside the callback. 
When the event fires, the framework clones the `RefAny` (cheap), 
invokes the `extern "C"` callback with that clone, and drops the 
clone after the callback returns. The original `RefAny` remains 
attached to the node for the next event.

`Dom::with_dataset` attaches a `RefAny` without
a callback — useful when child callbacks need to navigate to data
attached to an ancestor.

## Memory and threading

`RefAny` is `Send` and `Sync`. The data is heap-allocated and the counters 
are `AtomicUsize` with `SeqCst` ordering, so a `RefAny` can be moved or shared 
into a `Task` or `Thread`. The runtime borrow checker is per-`RefAny` value, 
not per-data — concurrent mutable downcasts on clones of the same `RefAny` 
race because the check-then-increment in `downcast_mut` is not atomic. 

For exclusive mutation across clones, use `replace_contents` instead, which 
uses `compare_exchange` on `num_mutable_refs`.

Deallocation is automatic. When the last `RefAny` clone drops:

1. `RefCount::drop` (`core/src/refany.rs:185`) sees `num_copies == 1`.
2. It reclaims `RefCountInner` via `Box::from_raw`.
3. It runs `custom_destructor` on the data pointer (executes `T::drop`).
4. It calls `dealloc` with the saved layout to release the bytes.

Borrow guards (`Ref`, `RefMut`) clone the `RefCount`, so they keep the
data alive even if the original `RefAny` is dropped. A guard outliving
its parent is rare but legal.

## Common pitfalls

### RefMut still holding a reference

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

### Can't store references

A struct that borrows from another value cannot go through `RefAny::new`. 
Either own the data or wrap the borrowed source in a `RefAny` and clone 
that into the child.

### Wrong type id

`downcast_ref::<Foo>()` on a `RefAny` constructed from `Bar` returns `None`, 
not a panic. Check the result.

### Borrow leaks across threads

A `Ref<T>` sent to another thread keeps the read borrow alive there; 
mutations on any clone block until it returns. Prefer cloning the `RefAny` 
itself across threads and taking borrows locally.

### Cycles

Two structs holding `RefAny` clones of each other will never drop. 
Use one direction of backreference only — children point at parents, 
never the reverse.


## Coming Up Next

- [Document Object Model](../dom.md) — The Dom tree - node types, hierarchy, and CSS
- [Datasets](../dom/datasets.md) — Attaching state to a node for navigation and per-instance state
- [Events](../events.md) — Callbacks, event filters, and how state triggers relayout

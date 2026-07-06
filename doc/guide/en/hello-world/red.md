---
slug: hello-world/red
title: Hello World [Red]
language: en
canonical_slug: hello-world/red
audience: external
maturity: alpha
guide_order: 29
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/red/hello-world.red
  - doc/src/codegen/v2/lang_red/mod.rs
last_generated_rev: HEAD
generated_at: 2026-07-06T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Red]

> **Status: ALPHA (unverified).** These bindings are constructed from the
> published Red/System specification and have **not** been compiled with a Red
> toolchain (none was available at generation time). The FFI mechanism is sound
> and cited in `scripts/RED_FFI_FINDINGS.md`, but expect to fix small syntax
> issues on first build, and see the *Honest limitations* section below.

## Introduction

[Red](https://www.red-lang.org/) is a full-stack language: a high-level,
Rebol-inspired dialect (**Red**) sits on top of a low-level, statically typed,
C-like dialect (**Red/System**). Both compile with the same ~1 MB toolchain into
a single, dependency-free native executable.

Azul's binding targets **Red/System**, because that is the dialect with a
general external-library FFI: the `#import` directive dlopen-loads `libazul` at
startup and maps every `Az*` C symbol into scope. High-level interpreted Red (the
console) cannot load an arbitrary shared library on its own — it reaches C only
*through* Red/System, via the `routine!` type and `#system-global` embedded
blocks. So the honest framing is "Azul from Red/System," which is still ordinary
Red code compiled by the ordinary Red toolchain.

Everything lives in a single generated file, `azul.reds`, with two layers:

- **`Az*`** — raw `#import`ed C-ABI functions mirroring `azul.h` one-to-one
  (`AzDom_createBody`, `AzButton_create`, …). Structs are passed and returned
  **by value** using Red/System's `value` keyword, which the spec documents as
  ABI-compatible with mainstream C compilers.
- **`azul-*`** — a small host-invoker convenience layer:
  `azul-register-<kind>` turns a Red/System `[callback]` function into an
  `Az<Kind>Callback` value, and `azul-refany-create` / `azul-refany-get` wrap and
  recover your data-model pointer.

Callbacks dispatch through libazul's host-invoker plumbing: each registered
function gets a handle id, and when the framework fires the callback it calls a
per-kind invoker inside `azul.reds` that looks the function up and runs it. Wire
those up once at startup with `azul-host-invoker-init`.

## Installation

You need the **Red toolchain** (a single ~1 MB binary from
[red-lang.org](https://www.red-lang.org/) — the `redc` compiler front-end can
build Red/System programs). No package manager or system libraries are required;
Red produces a static, dependency-free executable.

The download set is: the native library, the generated `azul.reds` bindings, and
the counter example source.

**Linux:**
```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.reds
curl -O https://azul.rs/ui/release/$VERSION/hello-world.red
```

**macOS:**
```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.reds
curl -O https://azul.rs/ui/release/$VERSION/hello-world.red
```

**Windows:**
```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.reds
curl -O https://azul.rs/ui/release/$VERSION/hello-world.red
```

## Build & run

`hello-world.red` `#include`s `azul.reds`, so you compile a single file:

```sh
redc -r hello-world.red     # -r = release build
./hello-world               # libazul must be on the loader path (e.g. LD_LIBRARY_PATH=.)
```

`#import` loads `libazul` at executable startup, so the library must be findable
by the OS loader at run time (same directory, or `LD_LIBRARY_PATH` /
`DYLD_LIBRARY_PATH` / `PATH`).

## The counter, explained

The full source is in
[`examples/red/hello-world.red`](../../../../examples/red/hello-world.red). The
shape mirrors every other Azul binding:

1. A `model!` struct holds the counter (starting at 5).
2. `on-click` is a `[cdecl]` callback: it reads the model back out of the
   `AzRefAny*` via `azul-refany-get`, bumps the counter, and writes
   `AzUpdate_RefreshDom` through the `out` pointer.
3. `on-layout` builds `body > [ div{font-size:32px} > text(counter), button ]`,
   registering `on-click` with `azul-register-button-on-click-callback`.
4. `main` calls `azul-host-invoker-init`, registers the layout callback, builds
   the window options, and calls `AzApp_run`.

Every callback dispatcher takes **pointer arguments only** (plus one out-pointer
for the return) — no aggregate crosses the callback boundary by value. libazul's
static thunk does the by-value plumbing on the C side, so the Red/System code
only ever sees `byte-ptr!`s.

## High-level Red

To drive Azul from a high-level Red (`Red [...]` header) program instead, wrap
the pieces you need in `routine!`s and inject the `#import` with
`#system-global`:

```red
Red [Title: "azul from high-level Red"]

#system-global [ #include %azul.reds ]

make-app: routine [ /local ... ][ ... AzApp_create ... ]
```

`routine!` bodies are Red/System, compiled to native code; they are the only
place high-level Red values cross into C. This is more ceremony than using
Red/System directly, which is why the shipped example is Red/System.

## Honest limitations

- **Unverified.** No Red toolchain was available to compile-check the output.
- **64-bit integers.** Red/System's `integer!` is 32-bit and it lacks a portable
  int64. Host-handle ids stay small (they start at 1) and 64-bit-valued API
  fields need an int64 shim. See `scripts/RED_FFI_FINDINGS.md`.
- **Tagged unions.** `AzOption*` / `AzResult*` / union types are emitted as
  opaque blobs pending exact-size wiring of the shared layout pass; construct and
  inspect them through C-API helpers, never by field access. The counter demo
  only round-trips `AzUpdate` (a unit enum), so it is unaffected.
- **arm64 by-value aggregates.** Red/System claims mainstream-C-ABI
  compatibility, but the AArch64 rules for large (>16 B) by-value structs are
  subtle; first verification should be on x86-64.

## Common errors

- **`redc: cannot open %azul.reds`** — run the compile from the directory that
  holds both `hello-world.red` and `azul.reds`.
- **Loader can't find `libazul`** — set `LD_LIBRARY_PATH=.` (Linux),
  `DYLD_LIBRARY_PATH=.` (macOS), or put the DLL beside the exe (Windows).
- **Window opens but the button does nothing** — you forgot
  `azul-host-invoker-init` before `AzApp_run`, so libazul cannot dispatch into
  Red.
- **Counter renders but never updates** — the click callback did not write
  `AzUpdate_RefreshDom` through the `out` pointer on every code path.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS

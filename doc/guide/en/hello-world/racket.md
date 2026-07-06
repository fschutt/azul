---
slug: hello-world/racket
title: Hello World [Racket]
language: en
canonical_slug: hello-world/racket
audience: external
maturity: wip
guide_order: 29
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/racket/hello-world.rkt
last_generated_rev: 0000000000000000000000000000000000000000
generated_at: 2026-07-06T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Racket]

## Introduction

The Racket binding calls the prebuilt `libazul` native library through Racket's
built-in `ffi/unsafe` + `ffi/unsafe/define` libraries. Because `ffi/unsafe` is
libffi-backed, a Racket closure passed for a function-pointer argument becomes a
**real C function pointer** — Racket is C-ABI-direct for callbacks (archetype A).
You pass plain Racket procedures straight to `button-set-on-click`; the generated
`azul.rkt` routes them through libazul's host-invoker plumbing (a pointer-arg
invoker plus an out-pointer for aggregate returns like the layout callback's
`AzDom`), so the ctx/RefAny lifetime story matches every other binding.

`azul.rkt` gives you an idiomatic, non-prefixed surface: `(dom-add-child dom
child)`, `(button-create label)`, `(make-window-create-options)`, plus the raw
`Az*` bindings and cstruct accessors (`make-AzDom`, `AzDom-...`,
`set-AzFullWindowState-title!`, ...) for when you need them.

## The GC-retention gotcha (read this first)

Racket is garbage-collected. The `ffi_closure` behind a `_fun` callback is only
kept alive **while the Racket procedure it wraps stays reachable**. If you build a
callback in a `let` that then goes out of scope, the GC frees the closure and the
next C call into it crashes.

`azul.rkt` defends against this by storing every user callback in a module-level
`azul-handles` hash (keyed by host-handle id) and pinning every per-kind invoker
closure in a module-level `live-pins` list — both are process-lifetime roots. On
**your** side the rule is simple: **bind your callbacks with a top-level
`(define ...)`** (or otherwise keep a reference), never an anonymous lambda that
escapes. The example keeps `on-click` and `layout` as top-level defines for
exactly this reason.

## Installation

You need **Racket 8.x** (any recent CS or BC build) and the native `libazul`
library. There is no `raco pkg` catalogue entry yet — install manually:

### Linux

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.rkt
AZ_LIB_DIR=. racket hello-world.rkt
```

### macOS

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.rkt
AZ_LIB_DIR=. racket hello-world.rkt
```

### Windows

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.rkt
racket hello-world.rkt
```

`azul.rkt` reads `AZ_LIB_DIR` and tries `<AZ_LIB_DIR>/libazul` first, then falls
back to the bare name on the dynamic loader's default search path. It is also
produced by `cargo run --bin azul-doc -- codegen all` into `target/codegen/azul.rkt`
if you build from a checkout.

## Simple "Counter" Example

```racket
#lang racket/base
(require "azul.rkt")

;; Mutable model.
(define model (box 5))

(define (az-str s)
  (define b (string->bytes/utf-8 s))
  (AzString_copyFromBytes b 0 (bytes-length b)))

;; Click callback: return an Update enum. Top-level define => GC-retained.
(define (on-click data-ptr info-ptr)
  (set-box! model (add1 (unbox model)))
  AzUpdate_RefreshDom)

;; Layout callback: f(data, info) -> AzDom. Runs on startup and each RefreshDom.
(define (layout data-ptr info-ptr)
  (define label (dom-create-text (az-str (number->string (unbox model)))))
  (define wrap (dom-create-div))
  (dom-add-css-property
   wrap
   (css-property-with-conditions-simple
    (css-property-font-size (style-font-size-px 32.0))))
  (dom-add-child wrap label)
  (define btn (button-create (az-str "Increase counter")))
  (button-set-button-type btn AzButtonType_Primary)
  (button-set-on-click btn (refany-create model) on-click)  ; plain procedure
  (define body (dom-create-body))
  (dom-add-child body wrap)
  (dom-add-child body (button-dom btn))
  body)

(define (run-app)
  (define app (app-create (refany-create model) (app-config-create)))
  (define wco (make-window-create-options))
  (define ws (AzWindowCreateOptions-window-state wco))
  (set-AzFullWindowState-title! ws (az-str "Hello World"))
  ;; The raw AzWindowCreateOptions_create takes a bare LayoutCallbackType and
  ;; discards the ctx, so splice the registered wrapper (which carries it) into
  ;; window_state.layout_callback with the generated cstruct setter.
  (set-AzFullWindowState-layout-callback! ws (register-callback "LayoutCallback" layout))
  (app-run app wco))

(run-app)
```

### What is happening

- `(refany-create value)` wraps any Racket value in an `AzRefAny`; `(refany-get
  ptr)` recovers it inside a callback. Both share libazul's host-handle table, so
  the value stays alive exactly as long as a `RefAny` clone does.
- `(button-set-on-click btn data on-click)` accepts a **plain procedure**. The
  wrapper calls `(register-callback "ButtonOnClickCallback" on-click)` for you,
  which stores the procedure (GC-retention) and returns the callback wrapper
  struct the framework stores.
- The layout callback returns an `AzDom` (a 240-byte aggregate). The per-kind
  invoker writes it back through an out-pointer using `ctype-sizeof`, so the
  aggregate return crosses the C boundary intact.
- Non-prefixed wrappers (`dom-add-child`, `button-create`, ...) drop the
  `Az<Class>_` prefix; the raw `Az*` bindings and cstruct setters remain available
  for the small amount of nested-field plumbing the window options need.

## Callbacks in depth

`register-callback` allocates a monotonic host-handle id, stores your procedure
under it, and returns the matching `Az<Kind>` wrapper struct built by
`Az<Kind>_createFromHostHandle`. When the framework fires the callback, libazul's
static thunk calls back into `azul.rkt`'s per-kind invoker with that id; the
invoker looks your procedure up in `azul-handles` and calls it. When the last
`RefAny` clone tied to a handle drops, libazul calls the releaser and the entry is
removed — so callbacks are cleaned up without you tracking lifetimes manually.

The one rule you own: **don't let your callback procedures become unreachable**.
Top-level `(define ...)` is the idiomatic way to guarantee that.

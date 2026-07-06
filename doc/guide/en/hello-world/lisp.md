---
slug: hello-world/lisp
title: Hello World [Common Lisp]
language: en
canonical_slug: hello-world/lisp
audience: external
maturity: wip
guide_order: 28
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/lisp/hello-world.lisp
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Common Lisp]

## Introduction

The Common Lisp binding calls the prebuilt `libazul` native library through
[CFFI](https://cffi.common-lisp.dev/) with the `cffi-libffi` extension — no C
compile step. The generated bindings live in `azul.lisp` (the `:azul` ASDF
system); callbacks route through libazul's host-invoker plumbing, so CFFI
never has to synthesize a struct-by-value trampoline. `cffi-libffi` is what
lets the by-value struct calls the host-invoker thunks expect go through.

## Installation

You need a Common Lisp implementation (**SBCL** recommended),
[Quicklisp](https://www.quicklisp.org/), and the `cffi` + `cffi-libffi`
systems (Quicklisp pulls these in). `cffi-libffi` needs the system `libffi`
headers:

```sh
# libffi dev headers (macOS: brew install libffi ; Debian/Ubuntu:)
sudo apt-get install libffi-dev
# copy the native library next to the example so the loader finds it
cp /path/to/target/release/libazul.dylib examples/lisp/
```

The two ASDF systems are already wired: `azul.asd` defines `:azul` (the
generated `azul.lisp`), and `azul-example.asd` defines `:azul-example`, which
depends on `:azul`, `:cffi`, and `:cffi-libffi`.

## Running

Point ASDF at the example directory and quickload the system:

```sh
cd examples/lisp
AZ_LIB_DIR=. sbcl --non-interactive \
  --eval '(push #p"./" asdf:*central-registry*)' \
  --eval '(ql:quickload :azul-example)' \
  --eval '(azul-hello:run-app)'
```

## The program

`examples/lisp/hello-world.lisp` builds a counter: a `32px` label showing the
count and an "Increase counter" button that bumps it. The model is a mutable
`cons` cell so the same Lisp object is recovered inside every callback through
the `RefAny` host-handle table.

```lisp
(defpackage #:azul-hello (:use #:cl) (:export #:run-app))
(in-package #:azul-hello)

;; A cons cell so the counter is mutable in place through the host-handle table.
(defun make-model () (cons :counter 5))

(defun on-click (data info)
  (declare (ignore info))
  (let ((m (refany-get data)))                 ; recover the same cons cell
    (if m
        (progn (incf (cdr m)) (az-update-refresh-dom))
        (az-update-do-nothing))))

(defun layout (data info)
  (declare (ignore info))
  (let ((m (refany-get data)))
    ;; build div{font-size:32px} > text(counter) + Button, return a raw AzDom
    ...))
```

### How callbacks work

* **`refany-create`** wraps a Lisp value in an `AzRefAny` (an opaque host
  handle); **`refany-get`** recovers it inside a callback. Pass the same model
  to the app and to the button so both see the same counter.
* **`register-callback`** returns the matching `Az<Kind>` record. The generated
  invoker fires your function and writes its return value back through the
  callback out-pointer — a layout callback returns a `Dom`, an on-click
  callback returns an `AzUpdate`.
* The `LayoutCallback` is installed into `window_state.layout_callback` before
  `App.run`.

Build the DOM with the raw `AzDom_*` / `AzButton_*` FFI calls (as the example
does): these are the exact by-value struct calls the host-invoker thunks
expect, and the raw records carry no destructor, so the moved-out structs are
never double-freed.

## Status

The counter demo passes the headless E2E
(`scripts/e2e_language_matrix.sh lisp` → counter 5 → 6 → 8, "test result: ok").
On macOS a *real windowed* run is limited: `App.run` can't co-host with SBCL's
runtime ownership of `NSApplication`, so the windowed path is a smoke; the
headless E2E is the supported verification. See `examples/lisp/README.md` for
the ASDF/fasl-cache details.

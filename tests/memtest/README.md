# Per-language memory tests

Every azul language binding wraps the same C ABI, but each has its own
create/clone/consume/**drop** and finalizer logic — so each can have a *distinct*
double-free / use-after-free / leak bug (see the clone/drop double-free that hit
only the python binding). These tests verify, per language, that the bindings
**do not segfault and do not leak**.

## What each test does

`mem_test.<lang>` (one per shipped binding) is deliberately tiny and does NOT run
the event loop (`App.run` needs a display and hangs headless):

1. **Consume-by-value drop path** — `App.create(model, AppConfig.create())` then
   destroy it. This is the exact path that double-freed before the codegen fix
   (App consumes AppConfig → nested `SystemStyle`, one of the 7 types that
   bitwise-cloned + double-freed).
2. **Leak loop** — create/destroy an `AppConfig` (and/or `Dom`) `AZ_MEMTEST_N`
   times, freeing each object every iteration via the binding's real
   delete/dispose/close (NOT relying on GC timing).
3. Print `memtest <lang> OK` and exit 0.

`AZ_MEMTEST_N` (default 200000) sets the loop count; the harness varies it.

## The harness — `scripts/run_memtest.sh <label> <run-cmd...>`

Two language-agnostic checks (the caller sets up env + the run command):

1. **Segfault** — runs the memtest under `gdb` with a tiny N; any
   `SIGSEGV`/`SIGABRT` fails.
2. **Leak** — runs with a small and a large N and compares **peak RSS**
   (`/usr/bin/time -v`). A real per-iteration leak scales with N; a correct
   binding stays flat (python: +0.5 MB over 250k iters). Threshold: 12 MB.

Example (python, after building the extension into `examples/python/azul.so`):

```sh
AZ_BACKEND=headless LD_LIBRARY_PATH=examples/python PYTHONPATH=examples/python \
  scripts/run_memtest.sh python python3 tests/memtest/mem_test.py
```

## Status

- **Validated locally**: python, c, cpp (compile/run + harness PASS).
- **Written, pending CI validation**: node, ruby, lua, csharp, java, kotlin,
  scala, go, zig, ocaml, haskell, pascal, fortran (matched against the generated
  `target/codegen/` bindings; no local toolchains).

## CI wiring (TODO — next step)

Add a `memtest` matrix job to `.github/workflows/rust.yml` that reuses the
per-language compile/run recipes already in `scripts/e2e_language_matrix.sh`
(same toolchains, same `libazul.so`/binding build), but runs `mem_test.<lang>`
through `scripts/run_memtest.sh` instead of the counter. Start `continue-on-error`
until every language is green, then make it a hard gate (like the AZ_E2E board).

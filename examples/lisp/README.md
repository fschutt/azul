# Azul — Common Lisp (SBCL)

⊘ **macOS NSApp threading conflict.** Codegen is correct (commit
`744f1e90c` fixed the tagged-union tag width); `hello-world.lisp`
runs through the host-invoker smoke layer but `App.Run` can't
co-host with SBCL's runtime ownership of `NSApplication` on the
main thread.

## Status

- Smoke test (refany round-trip) verified.
- Full GUI blocked on libazul-side NSApp-aware event loop (the
  same Phase C item that blocks Pascal and macOS PowerShell).

## Files

- `hello-world.lisp` — smoke test.
- `azul.lisp` — generated CFFI bindings.
- `libazul.dylib` — prebuilt native library.

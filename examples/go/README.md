# Azul — Go

Go bindings for the [Azul](https://azul.rs) GUI framework via cgo.

## Status

✅ **Full GUI E2E** — counter probe 5→8 verified.

## Requirements

- Go 1.21+, with `CGO_ENABLED=1` (the default on native builds)
- C compiler reachable to cgo (`clang` on macOS, `gcc` on Linux, MinGW on Windows)
- `libazul.{so,dylib}` / `azul.dll` in the working directory

## Build + Run

cgo's own preamble only says `-lazul`, so the include path, library path,
and platform link flags come from the environment (see the
[guide](../../doc/guide/en/hello-world/go.md) for the full per-OS command):

```sh
# macos
CGO_CFLAGS="-I." \
CGO_LDFLAGS="-L. -lazul -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText -framework CoreFoundation" \
  go build -o hello-world .
DYLD_LIBRARY_PATH=. ./hello-world

# linux
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -lpthread -lm -ldl" go build -o hello-world .
LD_LIBRARY_PATH=. ./hello-world
```

## What's idiomatic

cgo's `//export` lets Go functions be passed as C function pointers
(`AzCallbackType` / `AzLayoutCallbackType`) directly — no
host-invoker thunk needed. The codegen emits typed `C.AzString` /
`C.AzDom` etc. and Go calls C-ABI functions through `C.<fn>`.

```go
//export myLayoutCallback
func myLayoutCallback(data C.AzRefAny, info C.AzLayoutCallbackInfo) C.AzDom {
    // ... build the DOM and return
}
```

No AzulHostInvoker, no FinalizationRegistry, no JNA Structure —
Go's cgo handles the C interop natively.

## Files

- `main.go` — 165-line reference implementation.
- `libazul.dylib` — prebuilt native library.

## Notes

165 lines is verbose because Go has no class-method sugar — each
Az* type's methods become free `Az<Type>_<method>(self, ...)`
function calls. This is the cgo idiom; Go users expect it.

## Recent updates (2026-05-15/16)

- **R9 consume mechanism** (commit `dbc7d82b9`):
  `runtime.SetFinalizer(self, nil)` in the codegen-emitted consume
  helper disarms the Go finalizer for by-value C calls. Mirrors
  the Ruby `_consume` / Lua `azul._consume` pattern.

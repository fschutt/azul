# Azul — Go

Go bindings for the [Azul](https://azul.rs) GUI framework via cgo.

## Status

✅ **Full GUI E2E** — counter probe 5→8 verified.

## Requirements

- Go 1.20+
- C compiler reachable to cgo (`clang` on macOS, `gcc` on Linux)
- `libazul.dylib` in the working directory or via `CGO_LDFLAGS`

## Build + Run

```sh
DYLD_LIBRARY_PATH=. go run main.go
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

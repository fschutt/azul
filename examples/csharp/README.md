# Azul — C# / .NET

C# bindings for the [Azul](https://azul.rs) GUI framework via P/Invoke.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- .NET 8+ (`dotnet`)
- `libazul.dylib` (macOS) / `libazul.so` (Linux) / `azul.dll` (Windows) on `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` / `PATH`

## Build + Run

```sh
DYLD_LIBRARY_PATH=. dotnet run
```

`Azul.cs` is large (~120 K LOC, 11,696 P/Invoke `static extern`
declarations) but compiles fast — P/Invoke declarations have no
method bodies, just metadata.

## What's idiomatic

- `WindowCreateOptions.Create(Func<IntPtr, IntPtr, AzDom>)` smart
  factory. The codegen accepts any `Delegate`, so users can pass
  the typed `Func<...>` shape OR `HostInvoker.LayoutCallbackInvokerDelegate`.
- `Button.Create(label).WithButtonType(...).OnClick(data, fn)`.
- `AzString.ToString()`, `AzOption<T>.AsNullable()`,
  `AzVec<T>.ToArray()`, `AzResult<T,E>.Unwrap()`.
- `using var wco = WindowCreateOptions.Create(...)` — disposable
  pattern; `Dispose()` calls the C-side delete.
- Typed `Data<T>` delegates: `<Wrapper>WithData<T>` lets you write
  `(MyDataModel data, LayoutCallbackInfo info) => Dom` instead of
  unpacking `IntPtr dataPtr` yourself. Register via
  `HostInvoker.Register<Wrapper><MyDataModel>(fn)` (uses `as T`
  silent skip on mismatch). CC-1, 17 of 19 callback kinds.
- Primitive Vec sibling arrays: `U8Vec.ToByteArray()`,
  `U32Vec.ToIntArray()`, etc.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed** (commits `62094b885` / `75a1fbcd2`
  / `4edb65d7c`).
- **Primitive Vec sibling arrays** (commit `8f09b714d`).
- **CC-1 typed Data<T>** (commit `ccb59cb60`): see "What's idiomatic"
  above. Mirrors the Java emit; uses `as T` for the runtime cast
  (null-silent on mismatch).

## Gotchas

- C# `bool` is the 4-byte Win32 `BOOL` by default; the codegen
  applies `[MarshalAs(UnmanagedType.U1)]` to every bool struct
  field for the 1-byte C `_Bool`.
- Tagged-union tags emit as `: byte` (was `: uint` pre-fix; same
  family as the JNA/Pascal tag-width recurring bug).

## Files

- `hello-world.cs` — 84-line Python-quality port.
- `Azul.cs` — generated bindings (5.9 MB).
- `*.csproj` — dotnet project config.
- `libazul.dylib` — prebuilt native library.

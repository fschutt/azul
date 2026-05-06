---
slug: bindings/c
title: C Bindings
language: en
canonical_slug: bindings/c
audience: external
maturity: wip
guide_order: 320
topic_only: false
short_desc: C ABI surface, headers, and memory ownership
prerequisites: [hello-world, code-generation]
tracked_files:
  - api.json
  - dll/build.rs
  - doc/src/dllgen/build.rs
  - doc/src/dllgen/deploy.rs
  - doc/src/dllgen/license.rs
  - doc/src/dllgen/mod.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:50:43Z
---

# C Bindings

> **WIP** — `azul.h` is regenerated on every release. Symbol names are stable for a given `apiversion`, but new entries land between releases. Pin to a specific commit until your project no longer changes shape.

The C binding is two files: `azul.h` and a shared library (`libazul.so`, `libazul.dylib`, or `azul.dll`). For the program itself, see [Hello, World — C](../hello-world/c.md).

## Get the artifacts

Download the platform tarball from `azul.rs/release/<version>/`. It contains the header, the dylib, the matching static archive, and a `LICENSE-<PLATFORM>.txt` listing the bundled third-party licenses.

The license file must accompany binary distributions.

## Linux — GCC

```sh
gcc -I. hello-world.c -L. -lazul -Wl,-rpath,'$ORIGIN' -o hello-world
./hello-world
```

`-Wl,-rpath,'$ORIGIN'` makes the binary look for `libazul.so` in its own directory. The single-quoted `$ORIGIN` is consumed by the linker, not the shell.

## macOS — Clang

```sh
clang -I. hello-world.c -L. -lazul -o hello-world
./hello-world
```

The release dylib has its install name set to `@executable_path/libazul.dylib`, so a sibling `libazul.dylib` is found at run time.

If you built the dylib yourself and skipped that step:

```sh
install_name_tool -id @executable_path/libazul.dylib libazul.dylib
install_name_tool -change /full/path/libazul.dylib @executable_path/libazul.dylib hello-world
```

## Windows — MinGW

```sh
gcc -I. hello-world.c -L. -lazul -o hello-world.exe
hello-world.exe
```

Place `azul.dll` next to `hello-world.exe`. Windows resolves DLLs from the executable directory by default.

## Windows — MSVC

Link against the import library `azul.dll.lib`, not the static archive `azul.lib`:

```bat
cl /I. hello-world.c azul.dll.lib /Fehello-world.exe
hello-world.exe
```

## Static linking

For a single-binary build, link against `libazul.a` (Linux/macOS) or `azul.lib` (Windows). Expect a binary in the 30 to 60 MB range.

```sh
gcc -I. hello-world.c -L. -lazul -lpthread -ldl -lm -o hello-world
```

The static archive does not pull in the C++ standard library or platform GUI libraries. You must link them yourself:

| platform | extra link flags |
|---|---|
| Linux x86_64 | `-lpthread -ldl -lm -lfontconfig -lX11 -lxkbcommon -lEGL -lGL` |
| macOS | `-framework AppKit -framework CoreFoundation -framework OpenGL` |
| Windows | `gdi32.lib user32.lib opengl32.lib comdlg32.lib uxtheme.lib dwmapi.lib` |

## Header conventions

`azul.h` is single-file and self-contained. It declares only the C ABI; the C++ wrapper lives in a separate header.

- Every type is prefixed with `Az`: `AzApp`, `AzDom`, `AzString`.
- Every constructor is `Az<Type>_<methodName>`: `AzString_copyFromBytes`, `AzApp_create`.
- Enums have an `Az<Type>Tag` companion type and `Az<Type>_<Variant>` constructors. For tagged unions, the payload is read from `value.<Variant>.payload`.
- Heap-owned types provide `Az<Type>_delete(&value)`. Stack types do not.
- Reference-counted types (e.g. `AzRefAny`) have `Az<Type>_deepCopy(&value)` and `Az<Type>_delete(&value)`.

## Memory ownership

- Functions that return a heap type transfer ownership to the caller. Call the matching `_delete` when done.
- Functions that take a heap type by value consume it. Do not call `_delete` afterwards.
- Functions that take a `*const` or `*mut` pointer borrow. The caller still owns the value.
- `AzRefAny` is reference counted. `AzRefAny_deepCopy` bumps the count; `AzRefAny_delete` drops it.

## Example layout

```
my-app/
├── azul.h
├── libazul.so
└── hello-world.c
```

Compile, run, ship the binary alongside the dylib and the license file.

## Next

- [Hello, World — C](../hello-world/c.md) — full program walkthrough.
- [C++ Bindings](cpp.md) — same dylib, type-safer header.

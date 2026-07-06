# Azul — Swift binding example

`hello-world.swift` is the counter example. Swift talks to Azul through the
generated C header `azul.h`, exposed as a Clang module (`CAzul`) by
`module.modulemap`. Swift's C interop imports every `AzFoo` struct / enum /
tagged union with its authoritative C layout, and a plain Swift func with a
C-compatible signature converts to a real `@convention(c)` C function
pointer — so callbacks are passed C-direct, with no host-invoker.

```
examples/swift/
├── hello-world.swift    # the driver (import CAzul)
├── azul.swift           # generated idiomatic layer (@_exported import CAzul)
├── module.modulemap     # exposes azul.h as the CAzul Clang module
└── azul.h               # generated C header (the ABI source of truth)
```

`azul.swift` and `azul.h` are build artifacts — generate them with
`cargo run -p azul-doc codegen all` (they land in `target/codegen/`) and copy
`azul.swift` + `azul.h` here, or download them from a release. `module.modulemap`
is static scaffolding shipped in this directory.

## Build

With `libazul.{so,dylib}` / `azul.dll`, `azul.h`, `module.modulemap` and the
generated `azul.swift` in this directory:

```sh
# linux
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world

# macos (Swift toolchain ships with Xcode / CommandLineTools)
swiftc -I. hello-world.swift azul.swift -L. -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText \
  -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# windows
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world.exe
hello-world.exe
```

`-I.` lets Swift discover `module.modulemap` (and thus `import CAzul`);
`-L. -lazul` links the native library. The binary embeds no rpath, so keep the
`LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix at run time.

You should see a 400×300 window rendering `5`. Click **Increase counter**:
the counter increments, `layout` re-runs, and the new value renders.

Callbacks are C-direct: `onClick` / `layout` are plain Swift funcs passed
straight to the C-ABI setters — no host-invoker, exactly like the C, Zig and
Odin bindings.

# Azul — Java

Java bindings for the [Azul](https://azul.rs) GUI framework via JNA.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- JDK 17+
- Maven (for the included `pom.xml`)
- JNA 5.14+ (`net.java.dev.jna:jna:5.14.0`, declared in `pom.xml`)
- `libazul.dylib` (macOS) / `libazul.so` (Linux) / `azul.dll` (Windows) in the working directory or on `-Djna.library.path`

## Build + Run

```sh
mvn package
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp target/hello-world-1.0.0.jar:$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar \
    com.azul.HelloWorld
```

macOS requires `-XstartOnFirstThread` so libazul's NSApplication
loop pumps on the JVM main thread.

## What's idiomatic

- `WindowCreateOptions.create(layout)` smart factory hides the
  manual host-invoker register + bytes-splice.
- `Button.create(...).withButtonType(...).onClick(data, fn)` for
  clicks — `data` is any object, `fn` is a `CallbackInvokerCallback`
  SAM (lambda).
- `AzString` decodes to a `java.lang.String` via `.toString()`.
- `AzOption<T>.toNullable()`, `AzResult<T,E>.unwrap()`,
  `AzVec<T>.toList()` accessors mirror Java collection idioms.

## Gotchas

- Inside `package com.azul`, unqualified `String` resolves to the
  `com.azul.String` wrapper, not `java.lang.String`. Qualify
  everywhere you need the JVM string.
- JNA nested-struct field assignment is a Java reference swap, not
  a byte copy. The smart `WindowCreateOptions.create` factory
  already handles the splice; direct callers need
  `Pointer.write(0, byteArray, 0, length)`.

## Files

- `HelloWorld.java` — 86-line Python-quality port.
- `com/azul/*.java` — 1000+ generated wrapper files (regen via `cargo run -p azul-doc -- codegen`).
- `pom.xml` — Maven build config.
- `libazul.dylib` — prebuilt native library.

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
java -XstartOnFirstThread -Djna.library.path=. -jar target/hello-world-1.0.0.jar
```

macOS requires `-XstartOnFirstThread` so libazul's NSApplication
loop pumps on the JVM main thread; drop it on Linux/Windows.
The pom's `maven-shade-plugin` bundles JNA into the jar and the
manifest sets `Main-Class: com.azul.HelloWorld`, so no explicit
classpath is needed. Point `-Djna.library.path` at the directory
holding the native library (`.` assumes it sits next to the pom).

## What's idiomatic

- `WindowCreateOptions.create(layout)` smart factory hides the
  manual host-invoker register + bytes-splice.
- `Button.create(...).withButtonType(...).onClick(data, fn)` for
  clicks — `data` is any object, `fn` is the event's typed SAM
  (`AzulNativeManaged.ButtonOnClickCallbackInvokerCallback` for
  `Button.onClick`), written as a lambda.
- `AzulString` decodes to a `java.lang.String` via `.toString()`.
- `toNullable()` / `unwrap()` / `toList()` accessors mirror Java
  idioms, but live on the raw `Az*` JNA structs (e.g.
  `AzOptionString.toNullable()`, `AzStringVec.toList()` →
  `List<AzString>`), not on the high-level wrapper classes.
- Typed `Data<T>` SAMs: `AzulHostInvoker.<Wrapper>WithData<T>` lets
  you write `(MyDataModel data, LayoutCallbackInfo info) -> Dom`
  instead of unpacking `Pointer dataPtr` yourself; register via
  `AzulHostInvoker.register<Wrapper>(MyDataModel.class, fn)`. CC-1,
  17 of 19 callback kinds.
- Primitive Vec sibling arrays: `U8Vec.toByteArray()`,
  `U32Vec.toIntArray()`, etc. — bulk copy without the per-element
  iteration cost.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed** (commits `62094b885` consume,
  `75a1fbcd2` Option/Result delete+clone, `4edb65d7c` Vec iter
  per-elem clone).
- **AzulString rename** (commit `af6855e4e`): wrapper formerly named
  `String` (which shadowed `java.lang.String` inside `package com.azul`)
  is now `AzulString`. Drops the `java.lang.String.valueOf(...)`
  qualifier from user code.
- **CC-1 typed Data<T>** (commit `533df7ab5`): see "What's idiomatic"
  above. Follow-up: smart-factory integration
  (`button.onClick<MyDataModel>(fn)`) still TODO.

## Gotchas

- JNA nested-struct field assignment is a Java reference swap, not
  a byte copy. The smart `WindowCreateOptions.create` factory
  already handles the splice; direct callers need
  `Pointer.write(0, byteArray, 0, length)`.

## Files

- `HelloWorld.java` — 51-line Python-quality port.
- `pom.xml` — Maven build config. It pulls the generated bindings
  (~6,800 flat `com.azul` `*.java` files) straight from
  `../../target/codegen/java/` as an extra source root
  (build-helper-maven-plugin) — they are NOT copied into this
  directory. Regenerate via `cargo run -r -p azul-doc codegen all`;
  override the location with `-Dazul.codegen.dir=...`.
- `libazul.dylib` — prebuilt native library (git-ignored local
  artifact; build via `cargo build -r -p azul-dll` or download from
  the release page).

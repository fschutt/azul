# Azul — Kotlin

Kotlin bindings for the [Azul](https://azul.rs) GUI framework via JNA.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- Kotlin 1.9+ (compiler `kotlinc`), or Gradle 9.x — recommended;
  `build.gradle.kts` pins Kotlin Gradle plugin 2.3.21
- JDK 17+
- JNA 5.14+ (fetched automatically by the Gradle build)

## Build + Run

Recommended: the Gradle project in this directory. `gradle run`
copies the generated `Azul.kt` from `../../target/codegen/kotlin/`,
compiles with daemon caching (4 GB compiler heap preset in
`gradle.properties`), pulls JNA from Maven Central, and wires
`jna.library.path` (default `../../target/release`) onto the run
task:

```sh
gradle run
```

Manual `kotlinc` path (`$JNA_JAR` = your `jna-5.14.0.jar` from
Maven Central; `Azul.kt` copied from `../../target/codegen/kotlin/`
or downloaded from the release page):

```sh
kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt \
    -include-runtime -d hello-world.jar
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
```

macOS requires `-XstartOnFirstThread` for windowed runs (Cocoa
main-thread rule); drop it on Linux/Windows.

## What's idiomatic

- `WindowCreateOptions.create(layout)` (companion object) hides the
  host-invoker register + JNA bytes-splice.
- `Button.create(...).withButtonType(...).onClick(data, fn)` — `fn`
  is a SAM-converted Kotlin lambda.
- `AzulString.toString()` decodes UTF-8 bytes into `kotlin.String`.
- `Option<T>.toNullable()`, `Result<T,E>.unwrap()`, `Vec<T>.toList()`
  / `toByteArray()` etc.
- Typed `Data<T>` SAMs: `AzulHostInvoker.<Wrapper>WithData<T> { m, info -> ... }`
  lets you write the natural data shape instead of unpacking `Pointer`
  args; register via
  `AzulHostInvoker.register<Wrapper>(MyDataModel::class.java, fn)`.
  CC-1, 17 of 19 callback kinds.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed** (commits `62094b885` / `75a1fbcd2`
  / `4edb65d7c` — rides on the Java JNA emit).
- **AzulString rename** (commit `af6855e4e`): wrapper formerly named
  `String` (which shadowed `kotlin.String` inside `package com.azul`)
  is now `AzulString`.
- **CC-1 typed Data<T>** (commit `aadcf3a01`): see "What's idiomatic"
  above. Implementation lifts from the Java emit (commit `533df7ab5`).
  Two Kotlin-specific differences: labelled lambda for early-return
  (`val raw = <Sam> inv@{ ... return@inv ... }`) and `!!` non-null
  unwrap on the `Pointer?` SAM args before passing to wrapper-class
  constructors.

## Per-api.json-module JNA interfaces

Kotlin's JNA Proxy `<clinit>` overflows the JVM's 64KB-per-method
limit if every C-ABI function lives in one interface (~1700 methods).
The codegen splits them per api.json module:

- `AzulNativeApp` — App, AppConfig
- `AzulNativeDom` — Dom, Callback, NodeData, …
- `AzulNativeWindow` — WindowCreateOptions, FullWindowState, …
- `AzulNativeWidgets` — Button, CheckBox, TextInput, …
- `AzulNativeStr` — String
- `AzulNativeCallbacks` — RefAny, LayoutCallback, …
- `AzulNativeManaged` — host-invoker plumbing
- (and ~20 more — see api.json)

Each interface stays well under cap (largest is `vec` at ~888 methods, ~45 KB).

## Files

- `HelloWorld.kt` — 39-line Python-quality port.
- `build.gradle.kts` / `settings.gradle.kts` / `gradle.properties` —
  Gradle project; `gradle run` is the recommended workflow (see
  Build + Run above).
- `Azul.kt` — generated Kotlin bindings (single ~6.3 MB / ~136k-line file). NOT
  checked in here: it is emitted to `../../target/codegen/kotlin/Azul.kt`
  by `cargo run -r -p azul-doc codegen all` (or downloaded from the
  release page). The Gradle build picks it up from there
  automatically; the manual `kotlinc` path expects a copy in this
  directory.
- `libazul.dylib` — prebuilt native library (git-ignored local
  artifact; build via `cargo build -r -p azul-dll` or download from
  the release page).

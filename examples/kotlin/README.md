# Azul — Kotlin

Kotlin bindings for the [Azul](https://azul.rs) GUI framework via JNA.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- Kotlin 1.9+ (compiler `kotlinc`)
- JDK 17+
- JNA 5.14+

## Build + Run

```sh
kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt \
    -include-runtime -d hello-world.jar
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
```

macOS requires `-XstartOnFirstThread`.

## What's idiomatic

- `WindowCreateOptions.create(layout)` (companion object) hides the
  host-invoker register + JNA bytes-splice.
- `Button.create(...).withButtonType(...).onClick(data, fn)` — `fn`
  is a SAM-converted Kotlin lambda.
- `String.toString()` decodes UTF-8 bytes into `kotlin.String`.
- `Option<T>.toNullable()`, `Result<T,E>.unwrap()`, `Vec<T>.toList()`
  / `toByteArray()` etc.

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

- `HelloWorld.kt` — 67-line Python-quality port.
- `Azul.kt` — generated Kotlin bindings.
- `libazul.dylib` — prebuilt native library.

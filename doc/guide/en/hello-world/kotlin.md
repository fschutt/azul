---
slug: hello-world/kotlin
title: Hello World [Kotlin]
language: en
canonical_slug: hello-world/kotlin
audience: external
maturity: wip
guide_order: 17
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/kotlin/HelloWorld.kt
last_generated_rev: 39416ebc681c6423bfdefa94dc996f613184ea0b
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Kotlin]

## Introduction

The Kotlin binding rides on the same [JNA](https://github.com/java-native-access/jna)
layer as Java, so it loads the prebuilt `libazul` native library directly. You write
idiomatic Kotlin — a data class, a `LayoutCallback` SAM that returns a `Dom`, and the
companion-object `App` factory — and the generated wrappers handle the FFI.

## Installation

You need **Kotlin 1.9+**, **JDK 17+**, **JNA 5.14+**, and the native `libazul` library.

### Recommended: Gradle dependency

```kotlin
repositories {
    mavenCentral()
    maven { url = uri("https://azul.rs/maven") } // azul.rs-hosted artifacts
}
dependencies {
    implementation("rs.azul:azul:0.2.0")
    implementation("net.java.dev.jna:jna:5.14.0")
}
```

### Manual

1. Download the native library from the [/releases](/releases) page.
2. Add the generated `Azul.kt` bindings (from the
   [examples archive](/release/0.2.0/examples.zip) under `kotlin/`) to your sources.

The native library must be discoverable via `-Djna.library.path` /
`DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` / `PATH`.

## Simple "Counter" Example

```kotlin
package com.azul

import com.sun.jna.Pointer

// Plain data class - the "single source of truth" for app state.
class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

// Click callback: write the Update int through the out-pointer.
private val onClick = AzulNativeManaged.CallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; AzUpdate.RefreshDom.value }
                 else AzUpdate.DoNothing.value
    outPtr!!.setInt(0, result)
}

// Typed layout callback: returns a Dom directly; the bridge splices the bytes
// into the native out-pointer internally.
private val layout = AzulHostInvoker.LayoutCallback { _, dataPtr, _ ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        Dom.createBody()
    } else {
        val label = Dom.createDiv()
            .withCss("font-size: 32px;")
            .withChild(Dom.createText(m.counter.toString()))
        val buttonDom = Button.create("Increase counter")
            .withButtonType(AzButtonType.Primary.value)
            .onClick(m, onClick)
            .dom()
        Dom.createBody()
            .withChild(label)
            .withChild(buttonDom)
    }
}

fun main() {
    // `use { }` disposes the App (C-side delete) when the block exits.
    App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create()).use { app ->
        app.run(WindowCreateOptions.create(layout))
    }
}
```

Three things to notice.

- **`refanyWrap` / `refanyGet` with `is` smart-casts** — the same object instance is
  handed back to every callback; `if (m is MyDataModel)` both guards and smart-casts.
  On mismatch return `Dom.createBody()` / `AzUpdate.DoNothing.value`.
- **`LayoutCallback` SAM returns `Dom`** — the companion `WindowCreateOptions.create`
  factory hides the host-invoker register + JNA byte-splice. Note the `!!` on the
  nullable `Pointer?` out-pointer before `setInt`.
- **Fluent wrapper API** — `Dom.createBody().withChild(...)` and
  `Button.create(...).withButtonType(...).onClick(data, fn).dom()`. `AzString.toString()`
  decodes UTF-8 into `kotlin.String`.

## Build and run

```sh
kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt \
    -include-runtime -d hello-world.jar
# macOS requires -XstartOnFirstThread (Cocoa main-thread rule).
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
```

`$JNA_JAR` points at your `jna-5.14.0.jar`. On Linux/Windows drop
`-XstartOnFirstThread` and use `LD_LIBRARY_PATH` / `PATH`.

You should see the window pictured on the [hello-world landing page](../hello-world.md).

## Common errors

- **`UnsatisfiedLinkError`** — native library not on the JNA library path.
- **No window on macOS** — `-XstartOnFirstThread` missing.
- **Counter does not advance** — the click handler wrote `AzUpdate.DoNothing.value`.
- **`NullPointerException` on `outPtr`** — the `!!` unwrap on the SAM's nullable
  `Pointer?` arg is required; keep it.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Java]](java.md)

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
last_generated_rev: dab922c5e869ab3c1ff69a2d7f4af1af19a5c27c
generated_at: 2026-07-04T00:00:00Z
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

> **Windows: not shipped yet.** The Kotlin hello-world builds and starts on
> Windows, but after the headless layout the JVM never terminates — a native
> thread or an undrained native event queue keeps it alive (a known
> JNA-on-Windows class of problem; the same-JVM **Java** binding exits
> cleanly on Windows, and this binding passes the full e2e on Linux and
> macOS). Until a Windows-host thread dump pins the thread, use the Java
> binding on Windows or Kotlin on Linux/macOS. Tracked in
> `scripts/e2e_language_matrix.sh` (`lang_kotlin`).

You need **JDK 17+** and **Maven** (or Kotlin 1.9+ and JNA 5.14+ for the
manual route below).

The binding is published as `rs.azul:azul-kotlin` on the self-hosted Maven
repository at `https://azul.rs/ui/maven`: the compiled `Azul.kt` with
`libazul` for Linux, macOS and Windows bundled as JNA resources, so nothing
native has to be downloaded. The example project is a `pom.xml` that depends
on it and shades everything into one runnable jar:

```sh
curl -o pom.xml https://azul.rs/ui/release/$VERSION/pom-kotlin.xml
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.kt
mvn -q package
java -jar target/hello-world-1.0.0.jar
# macOS: java -XstartOnFirstThread -jar target/hello-world-1.0.0.jar
```

Without Maven, the binding is one generated file, `Azul.kt` (package
`com.azul`), compiled together with your program. Download it, the counter
example and the native library, then compile with `kotlinc` against JNA:

```sh
curl -O https://azul.rs/ui/release/$VERSION/Azul.kt
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.kt
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib     # or libazul.so / azul.dll
curl -L -o jna.jar https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar

kotlinc -J-Xmx4g -cp jna.jar Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar
java -Djna.library.path=. -cp hello-world.jar:jna.jar com.azul.HelloWorldKt
# macOS: java -XstartOnFirstThread -Djna.library.path=. ...
```

`Azul.kt` is a ~120k-line file; the `-J-Xmx4g` heap is required. The native
library must be discoverable via `-Djna.library.path` / `DYLD_LIBRARY_PATH`
/ `LD_LIBRARY_PATH` / `PATH`.

Gradle users: `examples/kotlin/build.gradle.kts` in the repository is a
complete project that compiles `Azul.kt` from a directory of your choice
(`-Pazul.codegen.dir=...`) and wires `jna.library.path` onto `gradle run`.

The self-hosted Maven repository at `https://azul.rs/ui/maven` also serves
`rs.azul:azul` — that is the *Java* binding (with `libazul` for Linux, macOS
and Windows bundled as JNA resources), which Kotlin can call directly like
any Java library; its API differs from `Azul.kt` (Java wrapper classes
rather than the Kotlin-idiomatic surface used below).

## Simple "Counter" Example

```kotlin
package com.azul

import com.sun.jna.Pointer

// Plain data class - the "single source of truth" for app state.
class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

// Click callback: write the Update int through the out-pointer.
private val onClick = AzulNativeManaged.ButtonOnClickCallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; Update.RefreshDom.value }
                 else Update.DoNothing.value
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
            .withChild(Dom.createSpanWithText(m.counter.toString()))
        val buttonDom = Button.create("Increase counter")
            .withButtonType(ButtonType.Primary.value)
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
  On mismatch return `Dom.createBody()` / `Update.DoNothing.value`.
- **`LayoutCallback` SAM returns `Dom`** — the companion `WindowCreateOptions.create`
  factory hides the host-invoker register + JNA byte-splice. Note the `!!` on the
  nullable `Pointer?` out-pointer before `setInt`.
- **Fluent wrapper API** — `Dom.createBody().withChild(...)` and
  `Button.create(...).withButtonType(...).onClick(data, fn).dom()`. The click
  handler is the event's typed SAM (`ButtonOnClickCallbackInvokerCallback` for
  `Button.onClick`). `AzulString.toString()` decodes UTF-8 into `kotlin.String`.

## Build and run

```sh
kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt \
    -include-runtime -d hello-world.jar
# macOS requires -XstartOnFirstThread (Cocoa main-thread rule).
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
```

`$JNA_JAR` points at your `jna-5.14.0.jar` (from Maven Central,
`net.java.dev.jna:jna:5.14.0`). On Linux/Windows drop
`-XstartOnFirstThread` and use `LD_LIBRARY_PATH` / `PATH`.

Alternatively — and recommended — use the Gradle project from the
repository's `examples/kotlin/` directory
([`build.gradle.kts`](https://github.com/fschutt/azul/blob/master/examples/kotlin/build.gradle.kts)):
`gradle run` pulls JNA from Maven Central, compiles `Azul.kt` +
`HelloWorld.kt` with daemon caching (the 4 GB compiler heap is preset in
`gradle.properties`), and wires `jna.library.path` onto the run task for
you.

You should see the window pictured on the [hello-world landing page](..md).

## Common errors

- **`UnsatisfiedLinkError`** — native library not on the JNA library path.
- **No window on macOS** — `-XstartOnFirstThread` missing.
- **Counter does not advance** — the click handler wrote `Update.DoNothing.value`.
- **`NullPointerException` on `outPtr`** — the `!!` unwrap on the SAM's nullable
  `Pointer?` arg is required; keep it.
- **Process hangs at exit on Windows** — the example builds and runs the whole
  headless layout, but the JVM may not terminate afterwards. This is a known
  JNA-on-Windows behaviour: the JVM exits only once **all non-daemon threads
  end** and the native event queue is drained, so a native (libazul) thread or
  window left on the JVM thread keeps it alive. The binding itself is fine — it
  passes the full run on macOS, and the Java binding (same JVM) runs on Windows
  — so a fix needs a Windows-host thread dump of the hung JVM. The E2E board
  reports Kotlin `⊘ SKIP` on Windows for this reason.

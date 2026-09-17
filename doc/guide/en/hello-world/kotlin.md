---
slug: hello-world/kotlin
title: Hello World [Kotlin]
language: en
canonical_slug: hello-world/kotlin
audience: external
maturity: mature
guide_order: 17
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/kotlin/HelloWorld.kt
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
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

The Kotlin binding relies on the exact same JNA underpinnings as Java. It provides
a fully idiomatic Kotlin experience out of the box — the `App` and callback registry 
have been completely refactored to use standard Kotlin generics, removing all `Pointer` 
arithmetic and manual type erasure from your code.

## Installation

Kotlin uses the identical maven repository and jar as Java. You can use Gradle:

```kotlin
repositories {
    mavenCentral()
    maven {
        url = uri("https://azul.rs/ui/maven")
    }
}

dependencies {
    implementation("rs.azul:azul:$VERSION")
    // JNA is a transitive dependency, but you can pin it
    implementation("net.java.dev.jna:jna:5.14.0")
}
```

Or you can use the pre-made `HelloWorld.kt` and `build.gradle.kts` from
the release page:

```sh
curl -O https://azul.rs/ui/release/$VERSION/build.gradle.kts
curl -O https://azul.rs/ui/release/$VERSION/settings.gradle.kts
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.kt
gradle build
java -jar build/libs/hello-world-1.0.0.jar      # macOS: java -XstartOnFirstThread -jar ...
```

Alternatively, to compile from the generated `Azul.kt`:

1. Download the native library from the
   [release page](https://azul.rs/ui/release/$VERSION) (`libazul.dylib`
   / `libazul.so` / `azul.dll`) and keep it in your working directory.
2. Download the Kotlin bundle — `azul-kotlin-$VERSION.tar.gz` — and add
   the generated Kotlin files to your source set.
3. Build and run.

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL with the now-generated .rs C-API bindings
cargo build -p azul-dll --release --features build-dll
```

Notice the required `--features build-dll`. The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The Kotlin bindings are generated at `target/codegen/kotlin/`.

## Simple "Counter" Example

```kotlin
package com.azul

class Counter {
    var count: Int = 0
}

// Top-level functions don't need @JvmStatic or objects!
fun layout(data: Counter, info: LayoutCallbackInfo): Dom {
    val countStr = "Count: ${data.count}"
    val btn = Button.create(countStr)
        .withOnClick(data, ::onClick)
    
    return Dom.createBody()
        .withChild(btn.dom())
}

fun onClick(data: Counter, info: CallbackInfo): Update {
    data.count++
    return Update.RefreshDom
}

fun main(args: Array<String>) {
    App.create(Counter(), ::layout).use { app ->
        val options = WindowCreateOptions.create()
        app.run(options)
    }
}
```

This works identically to Java: the `App.create` generic parameters properly type your layout callback, and `withOnClick` expects a callback receiving your specific typed `data` model. The `AzulHostInvoker` abstraction handles registering the class and hiding the JNA `Pointer` conversion logic behind the scenes.

Using `app.use { ... }` ensures that `.close()` is called deterministically when the app exits, properly disposing of the native C memory.

## Build and run

```sh
gradle build
# macOS — -XstartOnFirstThread is REQUIRED so libazul's NSApplication loop
# pumps on the JVM main thread.
java -XstartOnFirstThread -Djna.library.path=. -jar build/libs/hello-world-1.0.0.jar
```

On Linux/Windows drop `-XstartOnFirstThread`. `-Djna.library.path=.` points
JNA at the directory holding `libazul.dylib` / `libazul.so` / `azul.dll`.

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app.run(...)` opened a native window and ran the layout callback once with your data model.
2. The returned DOM was styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the DOM. On click, the framework borrows your data model mutably, runs the click callback, observes the refresh return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!

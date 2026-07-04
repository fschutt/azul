---
slug: hello-world/scala
title: Hello World [Scala]
language: en
canonical_slug: hello-world/scala
audience: external
maturity: wip
guide_order: 25
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/scala/HelloWorld.scala
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

# Hello World [Scala]

## Introduction

There is no separate Scala code generator: Scala rides the
[Java (JNA) binding](java.md) **directly**. You compile the generated
`com.azul.*` Java sources once with `javac`, put the resulting classes
on your classpath, and write ordinary Scala 3 against them — the same
`Dom` / `Button` wrapper classes, the same
`AzulNativeManaged.*CallbackInvokerCallback` SAM interfaces, the same
`AzulHostInvoker.refanyGet` data round-trip. Everything the Java
binding can do, Scala can do, plus pattern matching and lambda-SAM
conversion on top.

The flow is: **javac** (compile the generated bindings once) → **scala
run** (Scala CLI compiles and runs your program against those classes).

## Installation

You need a **JDK 17+**, **Scala 3 / Scala CLI** (the `scala` runner),
**JNA 5.14+**, and the native `libazul` library.

Linux:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul-java.zip
unzip -o azul-java.zip -d azul-java
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.scala
curl -L -o jna.jar https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar
javac -cp jna.jar -d classes azul-java/*.java
scala run HelloWorld.scala --class-path classes:jna.jar --java-opt -Djna.library.path=.
```

macOS (note the extra `-XstartOnFirstThread`):

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul-java.zip
unzip -o azul-java.zip -d azul-java
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.scala
curl -L -o jna.jar https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar
javac -cp jna.jar -d classes azul-java/*.java
scala run HelloWorld.scala --class-path classes:jna.jar --java-opt -Djna.library.path=. --java-opt -XstartOnFirstThread
```

Windows:

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul-java.zip
unzip -o azul-java.zip -d azul-java
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.scala
curl -L -o jna.jar https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar
javac -cp jna.jar -d classes azul-java\*.java
scala run HelloWorld.scala --class-path classes;jna.jar --java-opt -Djna.library.path=.
```

`azul-java.zip` is the same generated-bindings archive the Java guide
uses — Scala consumes the compiled `.class` files, so the one `javac`
invocation is the only Java-side step.

## Simple "Counter" Example

This is the exact program shipped as `examples/scala/HelloWorld.scala`:

```scala
package com.azul

import com.sun.jna.{Pointer, Structure}

object HelloWorld {

  class MyDataModel(var counter: Int)
  private val MODEL = new MyDataModel(5)

  private val ON_CLICK: AzulNativeManaged.ButtonOnClickCallbackInvokerCallback =
    new AzulNativeManaged.ButtonOnClickCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            m.counter += 1
            outPtr.setInt(0, Update.RefreshDom.value)
          case _ =>
            outPtr.setInt(0, Update.DoNothing.value)
        }
    }

  private def writeDom(outPtr: Pointer, dom: Dom): Unit = {
    val raw = Structure.newInstance(classOf[AzDom.ByValue], dom.rawPointer())
    raw.read()
    outPtr.write(0, raw.getPointer().getByteArray(0, raw.size()), 0, raw.size())
  }

  private val LAYOUT: AzulNativeManaged.LayoutCallbackInvokerCallback =
    new AzulNativeManaged.LayoutCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            val label = Dom.createDiv()
              .withCss("font-size: 32px;")
              .withChild(Dom.createText(String.valueOf(m.counter)))
            val buttonDom = new Dom(
              Button.create("Increase counter")
                .withButtonType(ButtonType.Primary.value)
                .onClick(m, ON_CLICK)
                .dom()
                .rawPointer())
            writeDom(outPtr, Dom.createBody().withChild(label).withChild(buttonDom))
          case _ =>
            writeDom(outPtr, Dom.createBody())
        }
    }

  def main(args: Array[String]): Unit = {
    // Smart factory: hides the host-invoker register + bytes-splice
    // (compare with the pre-rewrite version's ~6 lines of boilerplate).
    val wco = WindowCreateOptions.create(LAYOUT)
    val rawWco = Structure.newInstance(classOf[AzWindowCreateOptions.ByValue], wco.rawPointer())
    rawWco.read()
    val app = AzulNativeApp.AzApp_create(AzulHostInvoker.refanyCreate(MODEL), AzulNativeApp.AzAppConfig_create())
    app.write()
    AzulNativeApp.AzApp_run(app.getPointer(), rawWco)
  }
}
```

Five things to notice.

- **It is the Java binding, verbatim** — `Dom`, `Button`,
  `WindowCreateOptions`, `Update` are the generated `com.azul` Java
  classes; JNA loads `libazul` underneath. The program lives in
  `package com.azul` because the raw-pointer plumbing it uses (the
  `Dom(Pointer)` constructor, for instance) is package-private in the
  generated sources. That also means *all* internals are reachable —
  stick to the shown patterns.
- **`AzulHostInvoker.refanyGet` + pattern match** — your `MyDataModel`
  is wrapped once (`refanyCreate(MODEL)` produces the raw
  `AzRefAny.ByValue` struct that `AzApp_create` takes) and every
  callback recovers the *same instance*. `match { case m: MyDataModel
  => ...; case _ => ... }` is the Scala-natural version of Java's
  `instanceof` guard, with the mismatch arm writing
  `Update.DoNothing` / an empty body.
- **`ButtonOnClickCallbackInvokerCallback` SAM** — click handlers use
  the *typed per-widget* SAM from `AzulNativeManaged` (a generic
  `CallbackInvokerCallback` no longer matches `Button.onClick`).
  Mutate the model, then write the `Update` int through the
  out-pointer: `outPtr.setInt(0, Update.RefreshDom.value)`. Scala 3
  lambda-SAM conversion works too — the anonymous-class form above is
  just explicit about which interface is implemented.
- **The layout callback splices bytes** — this example implements the
  raw `LayoutCallbackInvokerCallback` and copies the built `AzDom`
  struct into libazul's out-pointer itself (`writeDom`:
  `Structure.newInstance` + `getByteArray` + `outPtr.write`). The
  typed alternative from the Java guide —
  `AzulHostInvoker.LayoutCallback`, which returns a `Dom` directly and
  does the splice (plus ownership bookkeeping) for you — is the same
  bytecode and works from Scala unchanged; prefer it for real
  applications.
- **`WindowCreateOptions.create(LAYOUT)`** — the smart factory hides
  the host-invoker registration for the layout callback. `main` then
  drops to the raw `AzulNativeApp.AzApp_create` / `AzApp_run` calls
  with by-value structs; `AzApp_run` blocks until the last window
  closes.

## Build and run

Two steps, matching the Installation block: compile the generated
bindings once, then let Scala CLI do the rest.

```sh
javac -cp jna.jar -d classes azul-java/*.java

# Linux / Windows
scala run HelloWorld.scala --class-path classes:jna.jar --java-opt -Djna.library.path=.

# macOS — Cocoa requires the event loop on thread 0; scala-cli forwards
# the option to the JVM it launches:
scala run HelloWorld.scala --class-path classes:jna.jar \
  --java-opt -Djna.library.path=. --java-opt -XstartOnFirstThread
```

`--java-opt -Djna.library.path=.` points JNA at the directory holding
`libazul.dylib` / `libazul.so` / `azul.dll`. You should see the window
pictured on the [hello-world landing page](../hello-world.md). Click
the button: the counter increments and the layout callback re-runs.

## Common errors

- **Window never appears / instant crash on macOS** — you omitted
  `--java-opt -XstartOnFirstThread`. Plain `-XstartOnFirstThread` on
  the `scala` command line does not reach the JVM; it must go through
  `--java-opt`.
- **`UnsatisfiedLinkError` / library not found** — the native library
  is not on `-Djna.library.path` (or `DYLD_LIBRARY_PATH` /
  `LD_LIBRARY_PATH`). Keep it in the working directory and pass
  `--java-opt -Djna.library.path=.`.
- **`duplicate class` errors** — the generated `.java` sources are
  both compiled into `classes/` *and* sitting inside a source tree the
  compiler picks up. Compile them exactly once with `javac -d classes`
  and reference only the `classes/` directory afterwards.
- **Counter does not advance** — the mismatch arm ran and wrote
  `Update.DoNothing.value`; verify `refanyGet` returns your model
  type and that you write `Update.RefreshDom.value` *after*
  mutating.
- **Sporadic crashes under GC pressure** — a hazard of the raw
  byte-splice style shown here: once a struct's bytes are handed to
  libazul (as in `writeDom`, or the `rawWco` passed to `AzApp_run`),
  the Java wrapper object still has a finalizer that will eventually
  `delete` the same memory. For anything beyond hello-world, use the
  typed `AzulHostInvoker.LayoutCallback` and the `App` wrapper
  (`App.create(...)` + try-with-resources in Java, `Using` in Scala),
  which mark wrappers consumed at the splice point.
- **Unresolved `com.azul` symbols when compiling** — `classes` is
  missing from `--class-path`, or the `javac` step failed silently;
  re-run it and check that `classes/com/azul/Dom.class` exists.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Java]](java.md) — the binding Scala rides on

# Azul — Scala

Scala bindings for the [Azul](https://azul.rs) GUI framework — rides
on Java's `com.azul.*` compiled bytecode. No separate Scala-side
codegen.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- Scala 3.8+ (compiler `scalac`)
- JDK 17+
- JNA 5.14+
- Java's `target/classes` from `examples/java/` (run `mvn package` there first)

## Build + Run

Compile the generated Java bindings into `classes/`, then let the Scala 3
runner build and launch `HelloWorld.scala` (this is the release-download
flow the install steps use — no repo clone required):

```sh
# `azul-java.zip` is the same generated-bindings archive the Java guide ships.
javac -cp jna.jar -d classes azul-java/*.java
scala run HelloWorld.scala --class-path classes:jna.jar \
    --java-opt -Djna.library.path=. --java-opt -XstartOnFirstThread   # macOS
```

Drop `--java-opt -XstartOnFirstThread` on Linux/Windows. Inside the repo you
can instead run `./build.sh`, which reuses `../java/target/classes` and wires
the JDK/Scala classpath for you.

## What's idiomatic

Scala uses Java's compiled bytecode directly, so every smart factory
Java has is available in Scala too:

- `WindowCreateOptions.create(layout)` — pass a Scala lambda or
  anonymous SAM instance.
- `Button.create(...).withButtonType(...).onClick(data, fn)`.
- AzString.toString, AzOption.toNullable, AzResult.unwrap, etc.

## Gotchas

Inside `package com.azul`, unqualified `String` resolves to
`com.azul.String` (the AzString wrapper) and **shadows
`java.lang.String`**. The `str` helper and `main(args)` qualify to
`java.lang.String` explicitly. If you write Scala code in
`package com.azul`, expect to type `java.lang.String` a lot.

## Files

- `HelloWorld.scala` — 77-line port.
- `build.sh` — compile + run script.
- `libazul.dylib` — symlink to `../java/libazul.dylib`.

## Recent updates (2026-05-15/16)

Scala rides on the Java bytecode emit, so all Java changes (the
17-binding memory-safety arc, `String -> AzulString` rename, typed
Data<T> SAMs in `AzulHostInvoker`) apply automatically. The
`HelloWorld.scala` example was refreshed to drop the now-redundant
`java.lang.String.valueOf(...)` qualifier (commit `af6855e4e`).

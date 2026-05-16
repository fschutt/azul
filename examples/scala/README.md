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

```sh
./build.sh
```

The script handles the JDK/Scala library classpath. Direct invocation:

```sh
JNA_JAR=$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar
SCALA_LIB=/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala-library/3.8.3/scala-library-3.8.3.jar
SCALA3_LIB=/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala3-library_3/3.8.3/scala3-library_3-3.8.3.jar

scalac -cp ../java/target/classes:$JNA_JAR HelloWorld.scala -d HelloWorld.jar
DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
    -cp HelloWorld.jar:../java/target/classes:$JNA_JAR:$SCALA_LIB:$SCALA3_LIB \
    com.azul.HelloWorld
```

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

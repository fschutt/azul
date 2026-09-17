---
slug: hello-world
title: Hello World
language: en
canonical_slug: hello-world
audience: external
maturity: mature
guide_order: 10
topic_only: false
short_desc: Window setup, state management, DOM creation, handling mouse clicks for a "counter" app
prerequisites: []
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - Dom
  - Css
---

# Hello World

## Introduction

Welcome to the Azul framework. In this guide you will learn how to write a simple 50-line program that produces a window with a counter and a button to increase said counter - showcasing how data models, click callbacks, and the installation and running of Azul applications work.

```azul-render screenshot=hello-world width=400 height=240 subtitle="Azul Window"
<body style="background-color: #efefef; margin: 0;">
  <p style="font-size: 50px; margin: 0;">5</p>
  <button>Increase counter</button>
</body>
```

Because each programming language is different, there's no such thing as "one hello world guide" as every language has differences in setup, installation methods, and code style. 

Each guide is self-contained, you do not need to read the others. Each page walks you through the same five-step path:

1. Installation or linking the Azul library.
2. Defining a data model.
3. Writing a `layout` callback that returns a `Dom`.
4. Attaching a click callback that mutates the model.
5. Building, running, notes for common pitfalls.

## Supported languages

Pick the "Hello World" / Setup guide for your language:

- [C (99+)](hello-world/c.md)
- [C++ (03 - 23)](hello-world/cpp.md)
- [C#](hello-world/csharp.md)
- [Fortran](hello-world/fortran.md)
- [Go](hello-world/go.md)
- [Haskell](hello-world/haskell.md)
- [Java](hello-world/java.md)
- [Kotlin](hello-world/kotlin.md)
- [Lua](hello-world/lua.md)
- [Node.js](hello-world/node.md)
- [OCaml](hello-world/ocaml.md)
- [Pascal](hello-world/pascal.md)
- [Python (3.10+)](hello-world/python.md)
- [Ruby](hello-world/ruby.md)
- [Rust](hello-world/rust.md)
- [Scala](hello-world/scala.md)
- [Zig](hello-world/zig.md)

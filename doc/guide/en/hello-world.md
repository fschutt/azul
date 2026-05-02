---
slug: hello-world
title: Hello World
language: en
canonical_slug: hello-world
audience: external
maturity: mature
guide_order: 10
topic_only: false
short_desc: Introduction - simple counter app showing window setup, state management, DOM creation, and handling mouse clicks
prerequisites: []
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Hello World

Welcome to the Azul framework. In this guide you will learn how to write a simple 50-line program that produces a window with a counter and a button to increase said counter - showcasing how data models, click callbacks and installation / running Azul applications works.

```azul-render screenshot=hello-world width=400 height=240 subtitle="The minimum viable Azul window — counter label plus a button."
<body style="background-color: #efefef;">
  <p style="font-size: 50px;">5</p>
  <button>Increase counter</button>
</body>
```

Because each programming language is different, there's no such thing as "one hello world guide" as every language has differences in setup, installation methods and code style. 

Each guide is self-contained, you do not need to read the others. Each page walks you through the same five-step path:

1. Installation or linking the Azul library.
2. Defining a data model.
3. Writing a `layout` callback that returns a `Dom`.
4. Attaching a click callback that mutates the model.
5. Building, running, notes for common pitfalls.

## Supported languages

Pick the "Hello World" / Setup guide for your language:

- [Rust](hello-world/rust.md)
- [C (99+)](hello-world/c.md)
- [C++ (03 - 23)](hello-world/cpp.md)
- [Python (3.10+)](hello-world/python.md)

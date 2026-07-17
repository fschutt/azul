# AZUL - Desktop GUI framework

<!-- [START badges] -->
[![CI](https://github.com/fschutt/azul/actions/workflows/rust.yml/badge.svg)](https://github.com/fschutt/azul/actions/workflows/rust.yml)
[![Coverage](https://img.shields.io/badge/coverage-report-blue.svg)](https://github.com/fschutt/azul/actions/workflows/rust.yml)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.88%20stable-blue.svg)]()
[![dependency status](https://deps.rs/repo/github/fschutt/azul/status.svg)](https://deps.rs/repo/github/fschutt/azul)
<!-- [END badges] -->

> Azul is a free, functional, reactive GUI framework for Rust, C and C++,
built using the WebRender rendering engine and a CSS / HTML-like document
object model for rapid development of beautiful, native desktop applications

###### [Website](https://azul.rs/) | [Releases](https://azul.rs/ui/releases) | [User guide](https://azul.rs/ui/guide) | [API documentation](https://azul.rs/ui/api)

## Current Status

> [!WARNING]
> **This repository is currently under heavy development. Azul is NOT usable yet.**
> 
> APIs may change frequently and features may be incomplete or unstable.
>
> If you are looking for the old README, see [README-OLD.md](/README-OLD.md)
> 
> The current release is from 2+ years ago, see the [releases page](https://github.com/fschutt/azul/releases).
> 
> Visit https://azul.rs/reftest in order to see the current testing and development
> of the core rendering / HTML layouting engine.

## Building

```bash
cargo build -r -p azul-doc
./target/release/azul-doc codegen all
```

azul-doc is a multitool that generates all the code *necessary* for having a stable public API that works across multiple languages. 

```bash
# link dynamic (fast Rust rebuilds)
export AZ_LINK_PATH=/path/to/azul/target/release
cargo add azul --features link_dynamic
cargo run --release my-project
```

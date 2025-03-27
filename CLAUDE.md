# Azul Development Guidelines

## Build/Test Commands
- Build project: `python3 ./build.py && cargo build --release`
- Run tests: `cargo test --no-fail-fast && cd test && cargo test --no-fail-fast && cd ..`
- Run single test: `cargo test test_name`
- Run reftests: `cargo run --manifest-path reftest/Cargo.toml --release` (outputs reftest/reftest_output/results.json)
- Check specific package: `cargo check --manifest-path path/to/Cargo.toml`

## Code Style
- Use `rustfmt` with project config: Rust 2021 edition
- Imports: Group by std/external/crate, prefer field init shorthand
- Type definitions: Use clear type aliases for complex types
- Error handling: Return `Update::DoNothing` or empty elements on errors
- Naming: Snake case for functions, CamelCase for types
- Comments: Wrap at 100 chars, format doc comments
- DOM testing: Use `Dom::assert_eq` for UI tests

## Project Structure
- Core modules: core/, css/, layout/, dll/
- Examples: examples/rust/, examples/c/, examples/cpp/, examples/python/
- Cross-language API in api/
- License: MPL-2.0
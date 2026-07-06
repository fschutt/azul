> **⚠️ This is `allsorts-azul` — a fork of [allsorts](https://github.com/yeslogic/allsorts) by [YesLogic](https://yeslogic.com/).**
>
> 🐛 **File issues and pull requests at
> [github.com/fschutt/allsorts-azul](https://github.com/fschutt/allsorts-azul) —
> NOT on the upstream YesLogic repo.** Reports sent upstream are about *this*
> fork and only create confusion for the YesLogic maintainers.
>
> This fork adds pixel-snap TrueType hinting, slimmer direct dependencies
> (7 fewer than upstream), and assorted bug fixes. All credit for the original
> engine goes to YesLogic. Intended to be upstreamed — use the official
> [`allsorts`](https://crates.io/crates/allsorts) crate if you can. For a
> `no_std` build, see [`allsorts_no_std`](https://crates.io/crates/allsorts_no_std).

---

<h1 align="center">
  <img src="https://github.com/yeslogic/allsorts/raw/master/allsorts.svg?sanitize=1" alt=""><br>
  Allsorts
</h1>

<div align="center">
  <strong>Font parser, shaping engine, and subsetter for OpenType, WOFF, and WOFF2 implemented in Rust</strong>
</div>

<br>

<div align="center">
  <a href="https://github.com/yeslogic/allsorts/actions/workflows/ci.yml">
    <img src="https://github.com/yeslogic/allsorts/actions/workflows/ci.yml/badge.svg" alt="Build Status"></a>
  <a href="https://docs.rs/allsorts">
    <img src="https://docs.rs/allsorts/badge.svg" alt="Documentation">
  </a>
  <a href="https://crates.io/crates/allsorts">
    <img src="https://img.shields.io/crates/v/allsorts.svg" alt="Version">
  </a>
  <a href="https://github.com/yeslogic/allsorts/blob/master/LICENSE">
    <img src="https://img.shields.io/crates/l/allsorts.svg" alt="License">
  </a>
</div>

<br>

Allsorts is a font parser, shaping engine, and subsetter for OpenType, WOFF, and WOFF2
written entirely in Rust. It was extracted from
[Prince](https://www.princexml.com/), a tool that typesets and lays out HTML
and CSS documents into PDF.

The Allsorts shaping engine was developed in conjunction with [a specification
for OpenType shaping](https://github.com/n8willis/opentype-shaping-documents/),
which aims to specify OpenType font shaping behaviour.

## Features

* **Parse** TrueType/OpenType (`glyf`, `CFF`, `CFF2`), WOFF, and WOFF2 files.
* **Shape** Arabic, Cyrillic, Greek, Hebrew, [Indic
  scripts](https://en.wikipedia.org/wiki/Languages_of_India) (Bengali,
  Devanagari, Gujarati, Gurmukhi, Kannada, Malayalam, Oriya, Sinhala, Tamil,
  Telugu), Khmer, Lao, Latin, Myanmar, Syriac, Thai, and other scripts.
* **Subset** from TrueType, OpenType, WOFF, and WOFF2 into fonts suitable
  for PDF embedding.
* **Instance** variable fonts into non-variable fonts.

## What is font shaping?

Font shaping is the process of taking text in the form of Unicode codepoints
and a font, and laying out glyphs from the font according to the text. This
involves honouring kerning, ligatures, and substitutions specified by the font.
For some languages this is relatively straightforward. For others, such as
Indic scripts it is quite complex. After shaping, another library such as
[Pathfinder](https://github.com/servo/pathfinder) or
[FreeType](https://www.freetype.org/) is responsible for rendering the glyphs.
To learn more about text rendering, Andrea Cognolato has a good [overview of
modern font rending on
Linux](https://mrandri19.github.io/2019/07/24/modern-text-rendering-linux-overview.html).
The concepts remain similar on other platforms.

## Examples

Refer to the [Allsorts Tools repository](https://github.com/yeslogic/allsorts-tools) for
a set of tools that exercise Allsorts font parsing, shaping, and subsetting.

## Unimplemented Features / Known Issues

We don't currently support:

* Unicode normalisation.

Known limitations:

* Allsorts does not do font lookup/matching. For this something like
  [font-kit](https://github.com/pcwalton/font-kit) is recommended.

## Development Status

Allsorts is still under active development but reached its first release
milestone with its inclusion in Prince 13 in 2019. In Prince it is responsible
for all font loading, and font shaping.

Currently, the font parsing code is handwritten. It is planned for this to
eventually be replaced by machine generated code via our [declarative data
definition language project](https://github.com/yeslogic/fathom).

## Platform Support

Allsorts CI runs tests on Linux, macOS, and Windows. Via Prince it is also
built for FreeBSD.

## Building and Testing

**Minimum Supported Rust Version:** 1.83.0

To build the crate ensure you have [Rust 1.83.0 or newer installed](https://www.rust-lang.org/tools/install).

Build with `cargo build` and run the tests with `cargo test`.

### Cargo Features

| Feature          | Description                                      | Default Enabled | Extra Dependencies       |
|------------------|--------------------------------------------------|:---------------:|--------------------------|
| `flate2_zlib`    | Use the zlib backend for flate2                  | ✅              | `zlib`                   |
| `flate2_rust`    | Use the Rust backend for flate2                  | ❌              | `miniz_oxide`            |
| `flate2_zlib-rs` | Use the zlib-rs backend for flate2               | ❌              | `zlib-rs`                |
| `prince`         | Enable Prince-specific tests and code            | ❌              |                          |
| `specimen`       | Enable module for generating HTML font specimens | ❌              | `upon`, `unicode-blocks` |

**Note:** In our testing the `zlib` `flate2` backend was faster but you may
prefer the Rust backend for a pure Rust solution when compiling to WASM or
similar.

## Contributing

Contributions are welcome, please refer to the
[contributing document](https://github.com/yeslogic/allsorts/blob/master/CONTRIBUTING.md)
for more details.

## Code of Conduct

We aim to uphold the Rust community standards:

> We are committed to providing a friendly, safe and welcoming environment for
> all, regardless of gender, sexual orientation, disability, ethnicity,
> religion, or similar personal characteristic.

We follow the [Rust code of conduct](https://www.rust-lang.org/policies/code-of-conduct).

## Acknowledgements

* [OpenType shaping documents](https://github.com/n8willis/opentype-shaping-documents/)
  forms the specification from which the shaping engine is implemented.
* [HarfBuzz](https://github.com/harfbuzz/harfbuzz) the widely used open source
  font shaping engine was used as reference for test output.
* The [Adobe Annotated OpenType Specification](https://github.com/adobe-type-tools/aots)
  test suite is used as part of the Allsorts test suite.
* [ttf-parser](https://github.com/RazrFalcon/ttf-parser) for CFF CharString parsing code.

## License

Allsorts is distributed under the terms of the Apache License (Version 2.0).

See [LICENSE](https://github.com/yeslogic/allsorts/blob/master/LICENSE) for details.

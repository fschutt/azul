# rust-fontconfig

Pure-Rust rewrite of the Linux fontconfig library (no system dependencies). Enable the `parsing` feature to parse `.woff`, `.woff2`, `.ttc`, `.otf` and `.ttf` with allsorts.

**NOTE**: Also works on Windows, macOS and WASM - without external dependencies!

## Motivation

There are a number of reasons why I want to have a pure-Rust version of fontconfig:

- fontconfig with all dependencies (expat and freetype) is ~190.000 lines of C (extremely bloated for what it does)
- fontconfig, freetype, expat and basically any kind of parsing in C is a common attack vector (via maliciously crafted fonts). The Rust version (allsorts) checks the boundaries before accessing memory, so attacks via font files should be less common.
- it gets rid of the cmake / cc dependencies necessary to build [azul](https://azul.rs) on Linux
- fontconfig isn't really a "hard" library to rewrite, it just parses fonts and selects fonts by name
- Rust has existing xml parsers and font parsers, just use those
- It allows fontconfig libraries to be purely statically linked
- Font parsing / loading can be easily multithreaded (parsing font files in parallel)
- It reduces the number of necessary non-Rust dependencies on Linux for azul to 0
- fontconfig (or at least the Rust bindings) do not allow you to store an in-memory cache, only an on-disk cache, requiring disk access on every query (= slow)
- in-memory ("bring your own font files") font loading for WASM and sandboxed environments
 
Now for the more practical reasons:

- libfontconfig 0.12.x sometimes hangs and crashes ([see issue](https://github.com/maps4print/azul/issues/110))
- libfontconfig introduces build issues with cmake / cc ([see issue](https://github.com/maps4print/azul/issues/206))
- To support font fallback in CSS selectors and text runs based on Unicode ranges, you have to do several calls into C, since fontconfig doesn't handle that
- The rust rewrite uses multithreading and memory mapping, since that is faster than reading each file individually
- The rust rewrite only parses the font tables necessary to select the name, not the entire font
- The rust rewrite uses very few allocations (some are necessary because of UTF-16 / UTF-8 conversions and multithreading lifetime issues)

## Installation

```toml
[dependencies]
rust-fontconfig = { version = "4.4", features = ["parsing"] }
```

The default build (`std` only) discovers system fonts via **filename
heuristics**. Enable `parsing` to read the actual font tables with allsorts for
accurate family names, weights, Unicode coverage and
`.woff`/`.woff2`/`.ttc`/`.otf`/`.ttf` support.

### Cargo features

| Feature | Default | Description |
|---------|:-------:|-------------|
| `std` | ✅ | Filesystem scanning + mmap-backed font loading. Currently required — the crate is std-only as of v4.1. |
| `parsing` | | Parse font tables via allsorts (accurate metadata; WOFF/WOFF2/TTC/OTF/TTF). Implies `std`. |
| `multithreading` | | Parallel font scanning/parsing via rayon. |
| `cache` | | Persist the parsed cache to disk (serde + bincode + dirs). |
| `async-registry` | | `FcFontRegistry` for incremental/background font discovery. Implies `parsing`. |
| `ffi` | | C API bindings. Implies `parsing` + `async-registry`. |

> **WASM:** `wasm32-*` targets build out of the box — `mmapio` and `rayon` are
> excluded automatically via `cfg`. Build with `--features parsing`.

## Usage

### Basic Font Query

```rust
use rust_fontconfig::{FcFontCache, FcPattern};

fn main() {
    // Build the font cache (scans system fonts)
    let cache = FcFontCache::build();
    
    // Query a font by name
    let mut trace = Vec::new();
    let results = cache.query(
        &FcPattern {
            name: Some(String::from("Arial")),
            ..Default::default()
        },
        &mut trace
    );
    
    if let Some(font_match) = results {
        println!("Font match ID: {:?}", font_match.id);
        println!("Font unicode ranges: {:?}", font_match.unicode_ranges);
        
        // Get font metadata
        if let Some(meta) = cache.get_metadata_by_id(&font_match.id) {
            println!("Family: {:?}", meta.family);
        }
        
        // Get font file path
        if let Some(source) = cache.get_font_by_id(&font_match.id) {
            match source {
                rust_fontconfig::OwnedFontSource::Disk(path) => {
                    println!("Path: {}", path.path);
                }
                rust_fontconfig::OwnedFontSource::Memory(font) => {
                    println!("Memory font: {}", font.id);
                }
            }
        }
    } else {
        println!("No matching font found");
    }
}
```

### Font Fallback Chain for CSS font-family

The new API separates font chain resolution from text querying:

1. **`resolve_font_chain()`** - Create a fallback chain from CSS font-family (without text)
2. **`chain.resolve_text()`** - Query which fonts to use for specific text

```rust
use rust_fontconfig::{FcFontCache, FcWeight, PatternMatch};

fn main() {
    let cache = FcFontCache::build();
    
    // Step 1: Build font fallback chain (without text parameter)
    let mut trace = Vec::new();
    let font_chain = cache.resolve_font_chain(
        &["Arial".to_string(), "sans-serif".to_string()],
        FcWeight::Normal,
        PatternMatch::DontCare,  // italic
        PatternMatch::DontCare,  // oblique
        &mut trace,
    );
    
    println!("CSS fallback groups: {}", font_chain.css_fallbacks.len());
    for group in &font_chain.css_fallbacks {
        println!("  CSS '{}' resolved to {} fonts", group.css_name, group.fonts.len());
    }
    
    // Step 2: Query which fonts to use for specific text
    let text = "Hello 你好 Здравствуйте";
    let font_runs = font_chain.query_for_text(&cache, text);
    
    println!("\nText '{}' split into {} font runs:", text, font_runs.len());
    for run in &font_runs {
        println!("  '{}' -> font {:?}", run.text, run.font_id);
    }
}
```

### Character-by-Character Font Resolution

For fine-grained control, use `resolve_text()` to get per-character font assignments:

```rust
use rust_fontconfig::{FcFontCache, FcWeight, PatternMatch};

fn main() {
    let cache = FcFontCache::build();
    
    let chain = cache.resolve_font_chain(
        &["sans-serif".to_string()],
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        &mut Vec::new(),
    );
    
    // Get font assignment for each character
    let text = "Hello 世界";
    let resolved = chain.resolve_text(&cache, text);
    
    for (ch, font_info) in resolved {
        match font_info {
            Some((font_id, css_source)) => {
                let font_name = cache.get_metadata_by_id(&font_id)
                    .and_then(|m| m.name.clone().or(m.family.clone()))
                    .unwrap_or_default();
                println!("'{}' -> {} (from CSS '{}')", ch, font_name, css_source);
            }
            None => println!("'{}' -> NO FONT FOUND", ch),
        }
    }
}
```

### List All Fonts Matching a Pattern

```rust
use rust_fontconfig::{FcFontCache, FcWeight};

fn main() {
    let cache = FcFontCache::build();
    
    // List all fonts - filter by properties
    let bold_fonts: Vec<_> = cache.list().into_iter()
        .filter(|(pattern, _id)| {
            matches!(pattern.weight, FcWeight::Bold | FcWeight::ExtraBold)
        })
        .collect();

    println!("Found {} bold fonts:", bold_fonts.len());
    for (pattern, id) in bold_fonts.iter().take(5) {
        println!("  {:?}: {:?}", id, pattern.name.as_ref().or(pattern.family.as_ref()));
    }
}
```

## Using from C

### Linking with the C API

The rust-fontconfig library provides C-compatible bindings that can be used from C/C++ applications.

#### Binary Downloads

You can download pre-built binary files from the [latest GitHub release](https://github.com/fschutt/rust-fontconfig/releases/latest):
- Windows: `rust_fontconfig.dll` and `rust_fontconfig.lib`
- macOS: `librust_fontconfig.dylib` and `librust_fontconfig.a`
- Linux: `librust_fontconfig.so` and `librust_fontconfig.a`

#### Building from Source

Alternatively, you can build the library from source:

```bash
# Clone the repository
git clone https://github.com/fschutt/rust-fontconfig.git
cd rust-fontconfig

# Build with FFI support
cargo build --release --features ffi

# The generated libraries will be in target/release
```

#### Including in Your C Project

1. Copy the header file from `ffi/rust_fontconfig.h` to your include directory
2. Link against the static or dynamic library
3. Include the header file in your C code:

```c
#include "rust_fontconfig.h"
```

### Minimal C Example

```c
#include <stdio.h>
#include "rust_fontconfig.h"

int main() {
    // Build the font cache
    FcFontCache cache = fc_cache_build();
    if (!cache) {
        fprintf(stderr, "Failed to build font cache\n");
        return 1;
    }
    
    // Create a pattern to search for Arial
    FcPattern* pattern = fc_pattern_new();
    fc_pattern_set_name(pattern, "Arial");
    
    // Search for the font
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    FcFontMatch* match = fc_cache_query(cache, pattern, &trace, &trace_count);
    
    if (match) {
        char id_str[40];
        fc_font_id_to_string(&match->id, id_str, sizeof(id_str));
        printf("Found font! ID: %s\n", id_str);
        
        // Get the font path
        FcFontPath* font_path = fc_cache_get_font_path(cache, &match->id);
        if (font_path) {
            printf("Font path: %s (index: %zu)\n", font_path->path, font_path->font_index);
            fc_font_path_free(font_path);
        }
        
        fc_font_match_free(match);
    } else {
        printf("Font not found\n");
    }
    
    // Clean up
    fc_pattern_free(pattern);
    if (trace) fc_trace_free(trace, trace_count);
    fc_cache_free(cache);
    
    return 0;
}
```

For a more comprehensive example, see the [example.c](ffi/example.c) file included in the repository.

#### Compiling the C Example

On Linux:
```bash
gcc -I./include -L. -o font_example example.c -lrust_fontconfig
```

On macOS:
```bash
clang -I./include -L. -o font_example example.c -lrust_fontconfig
```

On Windows:
```bash
cl.exe /I./include /Fe:font_example.exe example.c rust_fontconfig.lib
```

## Performance

- cache building: ~90ms for ~530 fonts
- cache query: ~4µs

## Features

- **Font matching** by name, family, style properties, or Unicode ranges
- **CSS font-family resolution** with `resolve_font_chain()` for proper fallback handling
- **Per-character font resolution** with `chain.resolve_text()` for multilingual text
- **Font run grouping** with `chain.query_for_text()` for text shaping pipelines
- Support for font weights (thin, light, normal, bold, etc.)
- Support for font stretches (condensed, normal, expanded, etc.)
- In-memory font loading and caching
- WASM support (`wasm32-*` targets; `mmapio`/`rayon` auto-excluded via `cfg`)
- C API for integration with non-Rust languages

## License

MIT
//! Emits the `package.json` manifest that ships alongside the
//! generated `azul.js`.
//!
//! The npm package only contains the JS glue file. The native shared
//! library (`libazul.{so,dylib}` / `azul.dll`) is bundled separately;
//! `azul.js` resolves it from `$AZ_LIB`, its own directory,
//! `$AZ_LIB_DIR`, the working directory, or the dynamic-loader search
//! path (see `mod.rs` `_resolveDllPath`). We pin a single runtime
//! dependency: `koffi`.
//! Bun and Deno consumers don't need any npm install at all because
//! their FFI primitives are runtime-built-ins.
//!
//! Module format: `commonjs`. ESM consumers receive the same default
//! export through Node's CJS-interop shim. We chose CJS over ESM
//! because:
//!
//! - `koffi` itself is a CJS package; mixing CJS-required deps inside
//!   an ESM file forces consumers onto the unstable Node `--experimental-*`
//!   flags.
//! - Every JS runtime since 2014 supports CJS. ESM-only support is a
//!   regression for older Node LTS versions still in production use.
//!
//! koffi minimum: `^2.7` (the version that introduced the
//! `koffi.proto(...)` API used by our callback wrappers).

/// Generate the `package.json` body as a String.
pub fn generate_package_json(version: &str) -> String {
    format!(
        r#"{{
    "name": "azul",
    "version": "{version}",
    "description": "JavaScript bindings for the Azul GUI framework (Node.js / Bun / Deno).",
    "main": "azul.js",
    "type": "commonjs",
    "engines": {{
        "node": ">=16"
    }},
    "dependencies": {{
        "koffi": "^2.7.0"
    }},
    "license": "MPL-2.0 OR MIT OR Apache-2.0",
    "homepage": "https://azul.rs",
    "repository": {{
        "type": "git",
        "url": "https://github.com/maps4print/azul.git"
    }},
    "keywords": [
        "azul",
        "gui",
        "ffi",
        "koffi",
        "bun",
        "deno",
        "native",
        "ui"
    ],
    "files": [
        "azul.js",
        "README.md"
    ]
}}
"#,
    )
}

# Review: layout/src/http.rs

## Summary
- Lines: 590
- Public functions: 21 (including methods)
- Public structs/enums: 5 (HttpStatusError, HttpResponseTooLargeError, HttpError, HttpHeader, HttpRequestConfig, HttpResponse) + 1 type alias
- Findings: 0 high, 1 medium, 2 low

## Findings

### [MEDIUM] Dead Code — free function `http_get` likely unused
- **Location**: `http.rs:354`
- **Details**: The free function `pub fn http_get(url: &str)` has no callers outside this file. The C API uses `HttpRequestConfig::http_get_default` (line 224) instead. The free function is a Rust convenience wrapper but appears unused.
- **Evidence**: `Grep pattern="fn http_get\b" glob="!layout/src/http.rs"` — zero matches. The example `http_zip_demo.rs` imports `http_get_with_config` directly, not `http_get`.
- **Recommendation**: Consider removing or marking `pub(crate)` if it has no downstream consumers.

### [LOW] Documentation Verbosity — boilerplate doc comments on simple wrappers
- **Location**: `http.rs:216-275`, `http.rs:346-352`, `http.rs:501-519`
- **Details**: Several thin wrapper functions have verbose `# Arguments` / `# Returns` doc sections that add little value (e.g., `http_get_default`, `download_bytes_default`, `download_bytes`). The function signatures are self-documenting.
- **Recommendation**: Reduce to one-line doc comments for trivial wrappers.

### [LOW] Duplication — parallel free-function and method APIs
- **Location**: `http.rs:224-275` (methods), `http.rs:354-553` (free functions)
- **Details**: The module exposes two parallel APIs: free functions (`http_get`, `download_bytes`, `is_url_reachable`) and methods on `HttpRequestConfig` (`http_get_default`, `http_get`, `download_bytes_default`, `download_bytes`, `is_url_reachable`). The methods simply delegate to the free functions. This is intentional for C API exposure but worth noting — if the free functions have no Rust callers, they could be made `pub(crate)` or private.
- **Recommendation**: Consider making the free functions private/`pub(crate)` since the public API is through `HttpRequestConfig` methods and the C FFI.

## System Documentation
- System identified: HTTP client / networking system
- Existing doc: none (no `doc/guide/` file for HTTP/networking)
- Doc needed: A brief guide covering the HTTP client API, its C-compatible types, TLS configuration, and how it integrates with the rest of Azul (e.g., language pack downloads).

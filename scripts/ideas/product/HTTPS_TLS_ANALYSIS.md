# HTTPS / TLS Analysis: `rustls-rustcrypto` without `ring`

## Root Cause (RESOLVED)

The `example.com` HTTPS failure is **NOT** a bug in `rustls-rustcrypto` or the
TLS stack. The pure-Rust crypto provider works correctly.

**Actual cause**: `example.com` is served by Cloudflare with a cross-signed
certificate chain rooted at "AAA Certificate Services" (old Comodo root), which
was **removed from the Mozilla trust store** in `webpki-roots 1.0.6`. This
results in `InvalidCertificate(UnknownIssuer)` regardless of crypto provider.

**Evidence**:
- `https://www.google.com` — works (HTTP 200, 78KB body)
- `https://github.com` — works (HTTP 200, 565KB body)
- `https://crates.io` — works (HTTP 404 but TLS succeeds)
- `https://example.com` — fails (`UnknownIssuer`) — server chain issue

## Original Problem Statement

The browser demo (`examples/c/browser.c`) fails when fetching `https://example.com`:

## Architecture: Full Call Chain

```
browser.c
  └─ AzHttpRequestConfig_httpGetDefault(url)     [C FFI]
       └─ HttpRequestConfig::http_get_default()   [dll/ → layout/src/http.rs:218]
            └─ http_get(url)                      [layout/src/http.rs:347]
                 └─ http_get_with_config(url, &default_config)  [layout/src/http.rs:380]
                      ├─ make_agent(30)            [layout/src/http.rs:360]
                      │    ├─ TlsConfig::builder()
                      │    │    .provider(Rustls)
                      │    │    .unversioned_rustls_crypto_provider(
                      │    │        Arc::new(rustls_rustcrypto::provider())  ← PURE RUST CRYPTO
                      │    │    )
                      │    │    .root_certs(RootCerts::WebPki)               ← MOZILLA CA BUNDLE
                      │    │    .build()
                      │    └─ Agent::config_builder().tls_config(tls).build().new_agent()
                      │
                      └─ agent.get(url).call()     [ureq HTTP request]
                           └─ RustlsConnector::connect()  [ureq/src/tls/rustls.rs:36]
                                ├─ build_config(tls_config)  [ureq/src/tls/rustls.rs:136]
                                │    ├─ provider = tls_config.rustls_crypto_provider  ← rustls-rustcrypto
                                │    ├─ ClientConfig::builder_with_provider(provider)
                                │    │    .with_protocol_versions(ALL_VERSIONS)  ← TLS 1.2 + 1.3
                                │    └─ [RootCerts::WebPki branch]
                                │         └─ RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS }
                                │              builder.with_root_certificates(root_store)
                                └─ ClientConnection::new(config, server_name)
                                     └─ TLS HANDSHAKE ← FAILURE OCCURS HERE
```

## Dependency Configuration

### Cargo.toml (layout/Cargo.toml)

```toml
ureq = { version = "3.3", default-features = false,
         features = ["rustls-no-provider", "rustls-webpki-roots"] }
rustls = { version = "0.23", default-features = false,
           features = ["std", "tls12", "logging"] }
rustls-rustcrypto = { version = "0.0.2-alpha" }
webpki-roots = { version = "1.0" }
```

### Feature Resolution

```
build-dll ──→ http ──→ ureq(rustls-no-provider, rustls-webpki-roots)
                   ──→ rustls(std, tls12, logging)
                   ──→ rustls-rustcrypto (default = std, tls12, zeroize)
                   ──→ webpki-roots
```

**Key point**: `rustls-no-provider` enables ureq's `_rustls` feature (needed for
the `rustls_crypto_provider` field on `TlsConfig`) but does NOT enable `_ring`.
Without `_ring`, ureq's fallback panics — the provider MUST be set explicitly
via `unversioned_rustls_crypto_provider()`, which our `make_agent()` does.

### Resolved Dependency Versions (from `cargo tree`)

```
rustls                v0.23.37  ← all three consumers unify to this
├── rustls-rustcrypto  v0.0.2-alpha (requires rustls >=0.23.0)
├── ureq               v3.3.0    (requires rustls >=0.23.22)
└── azul-layout         v0.0.7   (requires rustls >=0.23)

webpki-roots v1.0.6 ← shared between ureq and azul-layout
```

No version conflicts — Cargo unifies all `rustls` references to a single
`0.23.37` build, and all `rustls-pki-types` to a single version.
No `ring` or `aws-lc-rs` anywhere in the tree (confirmed via `cargo tree`).

## What `rustls-rustcrypto 0.0.2-alpha` Provides

### Cipher Suites

| Suite | TLS Version | Status |
|-------|-------------|--------|
| TLS_AES_128_GCM_SHA256 | 1.3 | Provided |
| TLS_AES_256_GCM_SHA384 | 1.3 | Provided |
| TLS_CHACHA20_POLY1305_SHA256 | 1.3 | Provided |
| TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 | 1.2 | Provided (feature `tls12`) |
| TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384 | 1.2 | Provided |
| TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 | 1.2 | Provided |
| TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 | 1.2 | Provided |
| TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 | 1.2 | Provided |
| TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 | 1.2 | Provided |

### Key Exchange Groups

| Group | Implementation |
|-------|----------------|
| X25519 | x25519-dalek 2.x |
| secp256r1 (P-256) | p256 0.13.x |
| secp384r1 (P-384) | p384 0.13.x |

### Signature Verification Algorithms

| Algorithm | Implementation |
|-----------|----------------|
| ECDSA P-256 + SHA-256 | p256::ecdsa (prehash verify) |
| ECDSA P-256 + SHA-384 | p256::ecdsa (prehash verify) |
| ECDSA P-384 + SHA-256 | p384::ecdsa (prehash verify) |
| ECDSA P-384 + SHA-384 | p384::ecdsa (prehash verify) |
| Ed25519 | ed25519-dalek 2.x |
| RSA PKCS#1 v1.5 SHA-256 | rsa 0.9.x (pkcs1v15) |
| RSA PKCS#1 v1.5 SHA-384 | rsa 0.9.x |
| RSA PKCS#1 v1.5 SHA-512 | rsa 0.9.x |
| RSA-PSS SHA-256 | rsa 0.9.x (pss) |
| RSA-PSS SHA-384 | rsa 0.9.x |
| RSA-PSS SHA-512 | rsa 0.9.x |

### Random Number Generation

- `SecureRandom`: `rand_core::OsRng` (via `rand_core 0.6` → `getrandom 0.2`)
- Key exchange: `EphemeralSecret::random_from_rng(OsRng)` for X25519
- Key exchange: `EphemeralSecret::random(&mut OsRng)` for P-256/P-384

### Notable Absences vs ring

| Capability | ring | rustls-rustcrypto 0.0.2-alpha |
|------------|------|-------------------------------|
| FIPS compliance | No | No |
| Assembly-optimized AES | Yes (AES-NI) | No (pure Rust aes-gcm) |
| P-521 (secp521r1) key exchange | Yes | **Missing** |
| Ed448 signing | Yes | **Missing** (TODO in source) |
| Battle-tested in production | Yes (years) | **No (alpha)** |

## Root Cause Analysis

### Most Likely: `rustls-rustcrypto` crypto primitive bug or incompatibility

`rustls-rustcrypto 0.0.2-alpha` is pre-release software. Its core crypto
dependencies are individually well-tested (RustCrypto crates), but the
**glue code** that adapts them to rustls's `CryptoProvider` trait is alpha.

Specific risk areas:

1. **RSA signature verification** (`verify/rsa.rs`): Uses
   `RsaPublicKey::from_pkcs1_der(public_key)`. The `public_key` bytes come from
   `rustls-webpki`'s certificate parsing (raw PKCS#1 RSAPublicKey DER from the
   SubjectPublicKeyInfo BIT STRING). If there's a mismatch in how `webpki`
   extracts these bytes vs what `rsa::RsaPublicKey::from_pkcs1_der` expects,
   every certificate verification with RSA keys would fail.

2. **ECDSA prehash verification** (`verify/ecdsa.rs`): Uses
   `verify_prehash(digest, &signature)` rather than `verify(message, &signature)`.
   This means the code manually hashes the message with SHA-256/384 and then
   verifies. If the DER signature decoding (`DerSignature::from_der`) has edge
   cases, verification could fail.

3. **AES-GCM AEAD** (`aead/gcm.rs`): The TLS 1.2 encrypt/decrypt paths use
   `encrypt_in_place_detached` / `decrypt_in_place_detached` with explicit nonce
   handling. The nonce construction for TLS 1.2 differs from TLS 1.3. A bug here
   would cause decryption failures after a successful handshake.

### Possible: `rand_core` / `getrandom` version interaction

`rustls-rustcrypto` depends on `rand_core 0.6.4` → `getrandom 0.2`.
Other crates in the tree may use `rand_core 0.9` → `getrandom 0.3`.
Cargo keeps both versions, and each works independently on macOS. However:

- If `x25519-dalek 2.x` or `p256 0.13.x` internally depend on a **different**
  `rand_core` version than what `rustls-rustcrypto` passes to their APIs,
  trait bounds might resolve to the wrong version's `RngCore` trait. This would
  be caught at compile time, though — so this is unlikely given the build succeeds.

### Unlikely but worth checking: Feature gating of `tls12`

`rustls-rustcrypto`'s default features include `tls12`. If `tls12` were somehow
disabled (unlikely since defaults are on), the `ALL_CIPHER_SUITES` array would
contain only TLS 1.3 suites. If the server only supports TLS 1.2 (rare but
possible), the handshake would fail with "no common cipher suite".

## Diagnostic Steps

### Step 1: Get the actual error message

The old `browser.c` printed only "HTTP error occurred". The current version
(lines 415-427) prints the debug representation of `HttpError`:

```c
AzString err_dbg = AzHttpError_toDbgString(&http_result.Err.payload);
printf("[BROWSER] HTTP error: %s\n", ...);
```

**Rebuild and rerun** with the current `browser.c` to see the exact error variant
(ConnectionFailed, TlsError, Timeout, etc.).

### Step 2: Write a minimal Rust reproduction

Add this to `layout/src/http.rs` tests:

```rust
#[test]
#[cfg(feature = "http")]
fn test_https_with_rustcrypto() {
    let result = http_get("https://example.com");
    match &result {
        Ok(resp) => println!("SUCCESS: status={}, body_len={}",
            resp.status_code, resp.body.as_slice().len()),
        Err(e) => println!("FAILED: {}", e),
    }
    assert!(result.is_ok(), "HTTPS request failed: {:?}", result.err());
}
```

Run with: `cargo test -p azul-layout --features http test_https_with_rustcrypto -- --nocapture`

### Step 3: Isolate TLS from HTTP

Test rustls + rustcrypto directly without ureq:

```rust
#[test]
#[cfg(feature = "http")]
fn test_tls_handshake_direct() {
    use std::sync::Arc;
    use rustls::{ClientConfig, ClientConnection, RootCertStore, ALL_VERSIONS};
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let provider = Arc::new(rustls_rustcrypto::provider());
    let root_store = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(ALL_VERSIONS)
        .unwrap()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let mut conn = ClientConnection::new(
        Arc::new(config),
        "example.com".try_into().unwrap(),
    ).expect("ClientConnection::new failed");

    let mut sock = TcpStream::connect("example.com:443").unwrap();
    let mut tls = rustls::StreamOwned::new(conn, sock);

    tls.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    tls.read_to_end(&mut buf).unwrap();
    let response = String::from_utf8_lossy(&buf);
    assert!(response.contains("200 OK") || response.contains("HTTP/1.1"),
            "Unexpected response: {}", &response[..200.min(response.len())]);
}
```

This bypasses ureq entirely and tests the rustls + rustcrypto + webpki-roots
stack in isolation.

## Fix Options

### Option A: Fix `rustls-rustcrypto` integration (preferred)

If the diagnostic tests reveal a specific crypto failure:

1. Fork `rustls-rustcrypto` to a local path dependency
2. Fix the specific failing algorithm (likely RSA verification or ECDHE)
3. Upstream the fix if possible

### Option B: Use native platform TLS on macOS, rustls-rustcrypto on Linux

```toml
# macOS/Windows: use Security.framework / SChannel via native-tls
[target.'cfg(not(target_os = "linux"))'.dependencies]
ureq = { version = "3.3", features = ["native-tls"] }

# Linux: pure Rust TLS (cross-compilable)
[target.'cfg(target_os = "linux")'.dependencies]
ureq = { version = "3.3", features = ["rustls-no-provider", "rustls-webpki-roots"] }
rustls-rustcrypto = { version = "0.0.2-alpha" }
```

Pro: Uses battle-tested platform TLS on macOS. Con: Different TLS stacks per
platform, more complex feature gating in `http.rs`.

### Option C: Upgrade to a newer `rustls-rustcrypto` or alternative pure-Rust provider

Check if a newer version of `rustls-rustcrypto` is available (post-0.0.2-alpha)
that fixes the issue. Alternatively, consider:

- `rustls-post-quantum` (if available)
- `boring-rustls-provider` (uses BoringSSL, but has C code)
- Manually installing the crypto provider using individual RustCrypto crates
  without the `rustls-rustcrypto` glue (more work but full control)

### Option D: Vendor and patch `rustls-rustcrypto`

Since this is alpha software:

1. Copy `rustls-rustcrypto` source into the repo (e.g., `vendor/rustls-rustcrypto/`)
2. Use as path dependency: `rustls-rustcrypto = { path = "vendor/rustls-rustcrypto" }`
3. Fix bugs directly, upgrade individual RustCrypto crate versions as needed
4. Maintain the fork until upstream stabilizes

## Key Files

| File | Purpose |
|------|---------|
| `layout/src/http.rs:360-377` | `make_agent()` — TLS config with rustls-rustcrypto |
| `layout/src/http.rs:380-478` | `http_get_with_config()` — request execution |
| `layout/Cargo.toml:77-80` | ureq/rustls/rustls-rustcrypto deps |
| `dll/Cargo.toml:139-184` | `build-dll` feature list (includes `http`) |
| `examples/c/browser.c:393-551` | `load_page()` — C-side HTTP call + error handling |
| `ureq/src/tls/rustls.rs:136-239` | ureq's `build_config()` — rustls ClientConfig construction |
| `ureq/src/tls/mod.rs:70-78` | `TlsConfig` struct with `rustls_crypto_provider` field |
| `rustls-rustcrypto/src/lib.rs:55-63` | `provider()` — CryptoProvider construction |
| `rustls-rustcrypto/src/verify/rsa.rs` | RSA signature verification (PKCS#1, PSS) |
| `rustls-rustcrypto/src/verify/ecdsa.rs` | ECDSA prehash verification |
| `rustls-rustcrypto/src/kx.rs` | Key exchange (X25519, P-256, P-384) |
| `rustls-rustcrypto/src/aead/gcm.rs` | AES-GCM AEAD for TLS 1.2 and 1.3 |

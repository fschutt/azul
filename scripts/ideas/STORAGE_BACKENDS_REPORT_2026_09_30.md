# Storage backends for AzDrive's "S3 Drive": options report

Date: 2026-09-30. Research only; nothing here is implemented.
Scope: the AzDrive `Drive` trait (Home plus added "S3 Drives", browse without syncing, download
one file on demand, RBAC through access links in the DB). The build designs S3 first. This
report covers the alternatives and how to keep them open.

All versions, dates and counts below were checked on 2026-09-30 against crates.io, PyPI,
GitHub or the vendor docs linked inline. Dependency counts come from `cargo tree` resolution
in scratch crates. Nothing was compiled; the method is in the appendix. **UNVERIFIED** marks a
claim that was not tested.

---

## TL;DR

1. **fsspec is an API shape, not a backend.** "fsspec vs S3" is a category error. fsspec is
   Python's client-side filesystem interface. S3 is a wire protocol and the first backend.
   Keep the name "S3 Drive". Copy fsspec's *structure*: a few primitives plus derived helpers,
   range reads with a block cache, a listing cache, a URL registry and a prefix view.
2. **There is no real Rust fsspec.** The only crate is `fsspec_rs` 0.1.5 (3 stars, about 1.9k
   downloads). It is a Python-first project whose Rust crate wraps `object_store` and tokio. In
   practice, Rust's fsspec equivalents are **`object_store`** (Apache Arrow) and **OpenDAL**.
3. **OpenDAL is the right "later" option, not the right "now" option.** It has 66 services and
   24 layers, is an ASF top-level project, and has native presign and conditional writes. Against
   it today:
   - its default build pulls reqwest, rustls, **aws-lc-sys** (C, cmake) and tokio (158 crates);
   - it is async-first, and `blocking::Operator` needs a tokio runtime;
   - it ships a breaking 0.x release every one to two months;
   - 0.59.3 is younger than azul's 14-day cooldown.

   Since 0.59 it has a pluggable `HttpTransport` and a reqwest-free core. An OpenDAL-backed
   `Drive` over azul's own HTTP client is therefore plausible later (untested).
4. **Take no S3 SDK dependency now.** Hand-roll SigV4 (about 300 to 500 lines) on crates
   already in azul's tree (`hmac`, `sha2`, `percent-encoding`, `url`), and parse XML with
   `roxmltree` (already behind azul-layout's `xml` feature). The fallback is `rusty-s3` with
   minimal features: sans-IO, BSD-2-Clause, 3 new crates, query-signing only.
   - `aws-sdk-s3` is ruled out: 195 crates, aws-lc-sys, and MSRV 1.94.1, above the 1.91.0 pin.
5. **Design the trait around five things the other backends also need:**
   - opaque version tokens (ETag);
   - conditional put (`If-None-Match: *` and `If-Match`);
   - opaque list cursors;
   - a capabilities struct;
   - a credential *source* that can refresh.

   Git-on-S3 (Cursor's Continuity, awslabs `git-remote-s3`) and offline conflict handling both
   depend on conditional put.
6. **RBAC: recommend a credential-vending Worker.** The Worker checks the Turso `access_link`
   table, then mints **R2 temporary credentials** (local JWT signing, no subrequest) scoped to
   one bucket, a prefix and an action set, with a short TTL. AzDrive then talks to R2 directly.
   - External shares are Worker-verified links that 302 to a short presigned GET.
   - Streaming through the Worker is the fallback for custom domains and browsers.
   - R2's per-bucket tokens cannot scope to a prefix, per the docs.
   - AWS's equivalent is `AssumeRole` with a session policy.
7. **Test servers:**
   - **moto_server** is the Python conformance server. It verifies header-signed SigV4 only in
     auth mode, never presigned URLs, and ignores STS session policies.
   - Add a small **stdlib strict mini-S3**: it verifies SigV4 (header and query, with expiry)
     and emulates R2's prefix-scoped session tokens for the access-link tests.
   - **MinIO is archived and unmaintained** (README, repo archived 2026-04).
   - Real R2 is the only fully faithful target; use it in a manual smoke leg.

---

## 0. Constraints from this repo that decide the choice

| Constraint | Where | Consequence |
|---|---|---|
| azul's HTTP client is **blocking `ureq` 3.3 + rustls + `rustls-rustcrypto`** (no ring, no aws-lc C code). On wasm the public API routes to error stubs | `layout/Cargo.toml:545-548`, `layout/src/http.rs` | An S3 client should sit on this transport. Crates that bring reqwest, hyper or aws-lc-sys duplicate the stack. wasm needs a fetch transport either way |
| The async-task design says **"No external async deps (no tokio, no async-std, no mio)"**; work runs on azul `Thread`s with main-thread write-back | `scripts/ideas/ASYNC_TASK_API_DESIGN.md` §1 | The `Drive` trait should be **blocking** and called from worker threads. An async-first crate (OpenDAL, object_store) needs an adapter plus a runtime |
| Toolchain is pinned to **1.91.0** | `rust-toolchain.toml` | Blocks `aws-sdk-s3` 1.150 and `aws-sigv4` 1.6 (MSRV 1.94.1), and `s3s` 0.17 (MSRV 1.96.0). `opendal` 0.59 needs exactly 1.91 |
| Every crate in the tree needs a written justification | `scripts/dependency-justifications.toml` (CI gate) | Each transitive crate costs a reviewed line |
| **14-day publish-age cooldown** on locked versions | `scripts/supply-chain/lockfile_guard.py --min-age-days 14`, `cooldown-exempt.txt` | Today it rejects `opendal` 0.59.3 (8 days), `reqsign-*` 3.3.x (5 days) and `aws-sdk-s3` 1.150 (5 days). Fast-releasing SDKs keep hitting it |
| Build scripts need a policy entry | `scripts/supply-chain/build-script-policy.toml` | `aws-lc-sys` (cmake, C) is a high-cost entry |
| cargo-vet imports (google, mozilla, bytecode-alliance, isrg, zcash, embark) | `supply-chain/config.toml`, `imports.lock` | None of `opendal*`, `object_store`, `rusty-s3`, `aws-sigv4`, `reqsign*` or `jiff` has an imported audit. `hmac` (1) and `sha2` (4) do |
| Durable data are files in S3 (per user, per meeting folder); the DB holds only minting, invites, transient state and **access links** | memory `azlin_cloud_storage_split.md`; `../azul-apps/planning/cloud-platform-notes.md` §2.5 and §8.2.2 | RBAC must be "DB row decides, storage enforces" |

---

## 1. fsspec (Python `filesystem_spec`)

`fsspec` 2026.9.0 (PyPI, 2026-09-18), BSD-3-Clause,
[github.com/fsspec/filesystem_spec](https://github.com/fsspec/filesystem_spec) (1.36k stars,
pushed 2026-09-29). Docs: [API](https://filesystem-spec.readthedocs.io/en/latest/api.html),
[Features](https://filesystem-spec.readthedocs.io/en/latest/features.html).

### 1.1 The API model

**`AbstractFileSystem`** (`fsspec/spec.py`) uses the template-method pattern. A backend
implements a few primitives and inherits everything else:

- **Listing and metadata:**
  - `ls(path, detail=True)` lists one level and returns dicts with at least `name`, `size` and
    `type` (`"file"` or `"directory"`).
  - `info(path)` returns one such dict.
  - `find`, `walk`, `glob`, `du`, `exists`, `isdir` and `isfile` are **derived** from `ls` and
    `info`.
  - s3fs implements `ls` with `ListObjectsV2(Delimiter="/")`: common prefixes become
    directories (`s3fs/core.py` `_lsdir`, line ~915).
- **Bytes:**
  - `cat_file(path, start=None, end=None)` does range reads.
  - `cat_ranges(paths, starts, ends, max_gap=None)` batches many ranges and merges nearby ones.
  - `pipe_file(path, value)` writes a whole object.
- **File handles:** `open(path, mode='rb', block_size=None, cache_options=None, ...)` returns an
  `AbstractBufferedFile`.
  - A backend supplies only `_fetch_range`, `_initiate_upload` and `_upload_chunk`. Reads go
    through a pluggable read cache; writes buffer into chunks, which maps onto multipart.
  - `DEFAULT_BLOCK_SIZE = 5 * 2**20` and `cache_type="readahead"` are the defaults
    (`spec.py:1945-1955`).
- **Read caches** (`fsspec/caching.py`, registered by name):

  | Name | Behaviour |
  |---|---|
  | `none` | no buffering |
  | `mmap` | sparse local file |
  | `readahead` | sequential readahead |
  | `adaptive` | readahead with async background prefetch |
  | `first` | caches the first block only (headers) |
  | `blockcache` | fixed blocks with **LRU**, `maxblocks=32` |
  | `bytes` | in-memory byte ranges |
  | `all` | the whole file |
  | `parts` | known parts of a file |
  | `background` | block LRU plus prefetch of the next block on a thread |

- **Caching filesystems, chained by URL:**
  - `blockcache::s3://…` keeps only the accessed parts on local disk;
  - `filecache::` keeps whole files, with `expiry_time`;
  - `simplecache::` keeps whole files with no freshness checks. Opened with `wb`, it writes
    locally and uploads on close.
- **Listing cache:** `DirCache(use_listings_cache=True, listings_expiry_time=None,
  max_paths=None)`, with TTL or LRU expiry and `invalidate_cache(path)`.
- **Registry:** `known_implementations` maps a protocol to a lazily imported class, with 54
  entries on master. They include `s3`, `gcs`, `abfs`, `webdav`, `sftp`, `dropbox`, `gdrive`,
  `box`, `git`, `github`, `hf`, `memory`, `zip`, `dir` and the cache protocols. The helpers are
  `register_implementation`, `filesystem(protocol, **opts)`, `url_to_fs(url)` and
  `available_protocols()`. Instances are cached per constructor arguments.
- **Async:** `AsyncFileSystem` exposes `_`-prefixed coroutines. Sync wrappers are generated by
  `mirror_sync_methods` and run on a dedicated event loop in a thread named `fsspecIO`
  (`fsspec/asyn.py:239`).
- **Other:**
  - `sign(path, expiration=100)` raises `NotImplementedError` by default; s3fs implements it
    with `generate_presigned_url`.
  - The `transaction` context defers write commits until the block exits.
  - `ukey` and `checksum` detect changes.
  - **`DirFileSystem` (`dir` protocol)** is a prefix-scoped view over another filesystem.
  - `Callback` classes report progress.

### 1.2 What is worth copying into the Rust `Drive` trait

| fsspec | AzDrive equivalent | Priority |
|---|---|---|
| Small primitive set with derived helpers | `Drive` has about 6 required methods. `walk`, `find`, `glob`, `exists`, `download_to` and `upload_from` are free functions or an extension trait over `&dyn Drive` | **Now** |
| `ls(path, detail=True)` one level via delimiter; `info` dict | `list(dir, cursor) -> ListPage` (delimiter `/`), `head(path) -> Entry`, where `Entry { path, kind, size, modified, version, content_type }` | **Now** |
| `cat_file(start, end)` | `get(path, Option<Range<u64>>)` | **Now** |
| `blockcache` (LRU of fixed blocks) plus `_fetch_range` | A `BlockReader: Read + Seek` over `get(range)`, with blocksize × maxblocks as fsspec does. Useful for previews (image headers, zip central directory, the first KB of a `.eml`) without downloading | **Now**, small |
| `DirCache` with TTL and `invalidate_cache` | A `CachedDrive<D>` wrapper: listing TTL, invalidated by its own writes | Next |
| `filecache::` / `simplecache::` | "Download ONE file on demand": a whole-file cache in the app cache dir keyed by `(drive, path, version)` | Next |
| `cat_ranges(max_gap)` | Coalesce nearby ranges in `BlockReader` | Later |
| Registry and `url_to_fs` | `open_drive(spec: &DriveSpec)`, where the spec is a URL: `file:///…`, `s3://bucket/prefix?endpoint=…&region=auto`, later `webdav+https://…`, `sftp://…`, `opendal+<scheme>://…` | **Now** (a match statement is enough) |
| `DirFileSystem` (`dir::`) | A `PrefixDrive` view. This is exactly what an access link grants: "this user sees `users/<uid>/mail/` as a drive root" | **Now** |
| `sign()` optional capability | `presign(op, path, ttl)` with a default of `Unsupported` | Next |
| `Callback` | A progress callback on `download_to` and `upload_from` | Next |
| `transaction` | Replace with conditional put (`If-Match`) plus conflict copies. Deferred commits add little on S3 | Skip |

**Not worth copying:**

- instance caching keyed on constructor arguments (hidden global state);
- a generated sync-over-async mirror (azul already has threads);
- `mode="a"` append emulation on object stores;
- fsspec's inconsistent pseudo-directory semantics (`isdir` guesses on S3). Make "directory"
  explicit in `Entry.kind` and in a capability flag.

### 1.3 Is there a Rust fsspec?

**No canonical one.** Searches for `fsspec`, `filesystem_spec`, "virtual filesystem" and `vfs`
on crates.io, and `fsspec language:Rust` on GitHub (6 repos in total), found:

| Crate / repo | Facts | Verdict |
|---|---|---|
| [`fsspec_rs`](https://crates.io/crates/fsspec_rs) 0.1.5 ([1kbgz/fsspec-rs](https://github.com/1kbgz/fsspec-rs)) | Created 2026-03-17, last release 2026-07-18, 1,854 downloads, 3 stars, Apache-2.0. Python-first: Python classes subclass `fsspec.AbstractFileSystem` and delegate to Rust. Rust deps: `object_store ^0.12` (`aws`) + `tokio` (`rt-multi-thread`). `FileSystem` trait with 7 primitives (`ls`, `rm_file`, `cp_file`, `open`, `info`, `mkdir`, `rmdir`). The docs say `AsyncFileSystem` is "Experimental … no shipped backend implements it yet" | Too young and small to depend on. Worth a look for trait naming only |
| `fsspec_rs_bridge`, `fsspec_data`, `fsspec_db` (same org) | Up to 830 downloads each | Same org, same verdict |
| [`vfs`](https://crates.io/crates/vfs) 0.13.0 (2026-03-21, 3.17M downloads) | Physical, Memory, Altroot, Overlay and Embedded filesystems. **No network backends**. The async variant is being sunset | Not an object-store abstraction |
| `vmingchen/vnfs`, `lucyge2022/fsspec-pyo3` | 2 and 0 stars | No |

The de facto Rust equivalents are **`object_store`** (§3.1; its Python binding `obstore` 0.11.1
ships an fsspec integration) and **OpenDAL** (§2; the fsspec org hosts `opendalfs` 0.2.0, an
fsspec filesystem backed by OpenDAL, PyPI 2026-08-28).

---

## 2. Apache OpenDAL (`opendal` crate)

**Snapshot:**

- `opendal` 0.59.3, 2026-09-22, Apache-2.0, MSRV 1.91.
- [github.com/apache/opendal](https://github.com/apache/opendal): 5.4k stars, pushed
  2026-09-29, 156 reverse dependencies on crates.io.
- It graduated to an **ASF top-level project on 2024-01-18**
  ([blog](https://opendal.apache.org/blog/apache-opendal-graduated/)).
- Release cadence: 0.55.0 (2025-11-20), 0.56 (2026-05-01), 0.57 (06-01), 0.58 (07-10), 0.59
  (09-04). That is **five breaking 0.x releases in about ten months**; the repo keeps an
  `upgrade.md`.
- 0.59 split the crate into a facade over `opendal-core`, one crate per service and one crate
  per layer.

### 2.1 Services

The 0.59.3 feature list has 68 `services-*` flags, about 66 distinct services:

- **Object stores:** `s3` (with R2, MinIO and other compatibles), `gcs`, `gcs-grpc`, `azblob`,
  `azdls`, `azfile`, `b2`, `cos`, `obs`, `oss`, `tos`, `swift`, `upyun`, `lakefs`,
  `vercel-blob`.
- **File protocols:** `fs`, `webdav`, `sftp`, `ftp`, `http`, `webhdfs`, `hdfs`, `hdfs-native`,
  `ipfs`, `ipmfs`, `alluxio`, `dbfs`, `compfs`, `monoiofs`, `opfs` (wasm32 only).
- **Consumer drives:** `gdrive`, `onedrive`, `dropbox`, `pcloud`, `koofr`, `seafile`,
  `yandex-disk`, `aliyun-drive`.
- **Key-value and databases:** redis, memcached, sqlite, postgresql, mysql, mongodb, gridfs,
  rocksdb, sled, redb, tikv, etcd, foundationdb, surrealdb, persy, moka, mini-moka, dashmap,
  cacache, foyer, cloudflare-kv, d1, memory.
- **Other:** `github`, `hf`/`huggingface`, `ghac`, `vercel-artifacts`.

Notes relevant to AzDrive, from the service sources in the repo at `53610f8`, 2026-09-24:

- **`gdrive`, `onedrive` and `dropbox` take `access_token` or `refresh_token` + `client_id` +
  `client_secret`.** OpenDAL does *not* run the interactive OAuth consent flow. The app still
  owns OAuth (§4.3).
- **`sftp` is built on the `openssh` crate**, which drives the system `ssh` binary through
  ControlMaster and fails with `compile_error!("This crate can only be used on unix")`. Its
  docs say it does not support password login. So: no Windows, no iOS or Android.
- The S3 service advertises `write_with_if_match`, `write_with_if_not_exists`, `presign_stat`,
  `presign_read`, `presign_write` and `presign_delete`. A non-recursive `list` sends
  `delimiter=/`, and `start_after` is supported.

### 2.2 Layers

The 0.59.3 feature list has 24 `layers-*` flags:

- **On by default:** `retry` (backon), `logging`, `timeout`, `concurrent-limit`.
- **Also available:** `throttle`, `immutable-index`, `mime-guess`, `route`,
  `capability-check`, `chaos` (fault injection), `tail-cut`, and observability (`tracing`,
  `fastrace`, `metrics`, `fastmetrics`, `prometheus`, `prometheus-client`, `otel-trace`,
  `otel-metrics`, `dtrace`, `await-tree`, `async-backtrace`, `hotpath`).
- **Cache: `foyer`**, a hybrid memory and disk cache ([foyer](https://github.com/foyer-rs/foyer)).
  Per `core/layers/foyer/src/lib.rs`:
  - `read` checks the cache and fills it on a miss;
  - `write` caches after the service write completes;
  - `delete` invalidates;
  - "`list`, `copy`, and `rename` pass through **without caching**", so there is no listing
    cache;
  - keys are versioned when the read carries a version.

  AzDrive would still write its own `DirCache`.

### 2.3 Blocking vs async

OpenDAL is **async-first** (`Operator`).

- `opendal::blocking::Operator` wraps an async `Operator` plus a
  **`tokio::runtime::Handle`**. `new()` fails without a current tokio runtime
  (`opendal-core/src/blocking/operator.rs`).
- The `blocking` feature enables `internal-tokio-rt`, which enables `tokio/rt-multi-thread`.
- Without `executors-tokio`, "the default executor will always return error if users try to
  perform concurrent tasks" (`types/execute/executor.rs`). A custom `Execute` impl can be
  supplied instead.

### 2.4 Dependency weight, HTTP and TLS stack, wasm

- **0.59 made HTTP pluggable**, via RFC 7749 "http_transporter" (2026-06-16,
  `opendal-core/src/docs/rfcs/7749_http_transporter.md`).
  - `opendal-core` defines `trait HttpTransport { fn fetch(&self, Request<Buffer>) -> impl
    Future<Output = Result<Response<HttpBody>>> }`, with a non-`Send` variant on wasm32.
  - reqwest moved to `opendal-http-transport-reqwest`.
  - The RFC's stated reason is that core users "inherit reqwest, hyper, and rustls even when
    they provide their own HTTP stack."
  - A transport can be set per operator (`Operator::http_transport`) or process-wide
    (`HttpTransporter::install_default`).
- **Default features:** `auto-register-services`, `http-transport-reqwest` (default `rustls`),
  `executors-tokio`, and the layers `concurrent-limit`, `logging`, `retry` and `timeout`.
- `opendal-core`'s tokio dependency is **non-optional** but minimal: `macros` and `io-util`.
  A runtime (`rt`) comes only from `executors-tokio` or `blocking`.
- The S3 service needs `reqsign-aws-v4`, `reqsign-core`, `reqsign-file-read-tokio`,
  `quick-xml`, `crc-fast` and `md-5`.

Measured transitive crates (normal plus build edges, target `aarch64-apple-darwin`):

| Config | Crates | tokio | reqwest / hyper / rustls | aws-lc-sys (C) |
|---|---:|---|---|---|
| `opendal` default + `services-s3` | 158 | yes (rt) | yes | **yes** |
| `opendal` `default-features=false` + `services-s3` | 104 | macros only | no (you supply `HttpTransport`) | no |
| `opendal-core` + `opendal-service-s3` | 103 | macros only | no | no |
| `opendal` default + s3, webdav, fs, sftp, gdrive, onedrive, dropbox | 202 | yes | yes | yes |

**wasm:**

- The repo has an edge test `core/edge/s3_read_on_wasm` built with `http-transport-reqwest` +
  `services-s3` (reqwest uses fetch on wasm).
- `services-opfs` is wasm32-only.
- `blocking` cannot work on wasm (tokio `rt-multi-thread`).
- A browser build still needs bucket CORS.

### 2.5 Presigned URLs

- `Operator::presign_stat`, `presign_read`, `presign_write` and `presign_delete` each take
  `(path, Duration)` and return a `PresignedRequest { method, uri, headers }`. `_options`
  variants carry extra arguments.
- Presigning is a service capability (`presign_*`). S3 has it; sftp does not.
- Signing is done by `reqsign` (Apache,
  [apache/opendal-reqsign](https://github.com/apache/opendal-reqsign)).

### 2.6 Could AzDrive's `Drive` be backed by OpenDAL later without changing the apps?

**Yes**, if the apps only ever see `dyn Drive` plus the helper layer, and the trait keeps the
conventions in §7.1. The mapping:

| `Drive` | OpenDAL |
|---|---|
| `list(dir, cursor)` | `op.list_with(dir)`, non-recursive (S3 sends `delimiter=/`). Cursor = last key, sent as `.start_after(..)`. OpenDAL's `Lister` is a stream, so an opaque cursor maps onto `start_after` |
| `head(path)` | `op.stat(path)`: `Metadata` has content-length, last-modified, etag, content-type and `EntryMode::{FILE, DIR, Unknown}` |
| `get(path, range)` | `op.read_with(path).range(a..b)` |
| `put(path, bytes, PutMode::CreateNew)` / `IfVersion(etag)` | `op.write_with(path, bytes).if_not_exists(true)` / `.if_match(etag)`, gated on capability `write_with_if_not_exists` / `write_with_if_match` |
| `delete` | `op.delete(path)` |
| `presign` | `op.presign_read` / `presign_write` / … |
| `capabilities()` | `op.info().full_capability()`, then map the fields |
| error kinds | OpenDAL `ErrorKind`: `NotFound`, `PermissionDenied`, `AlreadyExists`, `ConditionNotMatch`, `RateLimited`, `Unsupported`, `IsADirectory`, `NotADirectory`, `RangeNotSatisfied`, `Conflict`, `ConfigInvalid`, `Unexpected`, `IsSameFile` |

**Two routes, both behind an optional `opendal` cargo feature of the shared storage crate:**

1. **Easy route:** `opendal` with defaults and a private tokio runtime inside the adapter, used
   through `blocking::Operator`. This brings in tokio, reqwest, rustls and aws-lc-sys, which is
   acceptable only as an opt-in "more backends" build.
2. **azul-native route (plausible, UNTESTED):**
   - `opendal-core` (`default-features=false`) plus the chosen `opendal-service-*` crates;
   - an `HttpTransport` impl that runs azul's blocking `ureq` client on the calling worker
     thread (or a pool) and returns a ready future;
   - futures driven with `futures::executor::block_on` inside the `Drive` method;
   - no concurrent tasks, or a custom `Execute`.

   This keeps rustls-rustcrypto and adds no runtime. Risks:
   - `reqsign-file-read-tokio` would need tokio if credentials are loaded from files. Supply
     static or vended credentials instead;
   - a service may assume tokio somewhere. Test per service.

---

## 3. Other Rust S3 crates, and SigV4 options

Totals are transitive crates for the target `aarch64-apple-darwin` (normal plus build edges).
The "new" column counts crate *names* absent from azul's whole-workspace `Cargo.lock`. That
lock is a union over all features, so "new" understates the cost for a default azul build.
"Heavy" flags are present in the resolved tree.

| Crate (version, date) | License | MSRV vs 1.91 pin | Crates | New | Heavy deps | wasm | Presign | Conditional put | Maintainers |
|---|---|---|---:|---:|---|---|---|---|---|
| **hand-rolled SigV4** (`hmac` 0.13 + `sha2` 0.11) | MIT/Apache | ok | 13 | 0 | none | yes (pass `now` in) | yes | yes (just headers) | us |
| [`rusty-s3`](https://crates.io/crates/rusty-s3) 0.10.2 (2026-08-01), `default-features=false, features=["rustcrypto"]` | BSD-2-Clause | 1.85 ok | 49 | 3 (`rusty-s3`, `jiff`, `jiff-core`) | none | yes (`wasm_bindgen` feature → `jiff/js`) | yes (all requests are query-signed) | add headers yourself | paolobarbolini, guerinoni; 157 stars; 23 rev-deps |
| `rusty-s3` 0.10.2 default (`full`: XML parsing) | BSD-2-Clause | ok | 64 | 5 (+`instant-xml`, `-macros`) | none | yes | yes | " | " |
| [`object_store`](https://crates.io/crates/object_store) 0.14.2 (2026-09-15), `aws-base` + `ring` | MIT/Apache-2.0 | 1.85 ok | 104 | 4 | tokio (rt), hyper (types), ring | wasm32-unknown-unknown: **cloud features unsupported** (README); wasip1 with `*-base` | `Signer::signed_url` | `PutMode::{Overwrite, Create, Update(UpdateVersion{e_tag, version})}` | Apache Arrow (alamb, tustvold…); 575 rev-deps |
| `object_store` 0.14.2, `aws` | " | ok | 141 | 9 | tokio, reqwest, hyper, rustls, **aws-lc-sys** | no | yes | yes | " |
| [`aws-sdk-s3`](https://crates.io/crates/aws-sdk-s3) 1.150.0 (2026-09-25) | Apache-2.0 | **1.94.1: blocked** | 195 (204 with `aws-config`) | 30 | tokio, hyper, rustls, ring, **aws-lc-sys** | not a primary target (not checked) | `PresigningConfig` | yes | AWS; weekly releases (1.144 → 1.150 in one month); crate is 1.8 MB compressed |
| [`rust-s3`](https://crates.io/crates/rust-s3) 0.37.2 (2026-05-04), default | MIT | none declared | 141 | 21 | tokio, reqwest, hyper, **native-tls** | no | `presign_get/put/delete` | not checked | single owner (durch); 679 stars |
| `rust-s3` `sync-rustls-tls` (attohttpc) | MIT | " | 100 | 16 | rustls, **aws-lc-sys** | no | yes | " | " |
| [`aws-sigv4`](https://crates.io/crates/aws-sigv4) 1.6.0 (2026-09-22) | Apache-2.0 | **1.94.1: blocked** | 57 | 12 | tokio (via `aws-smithy-async`) | not checked | yes | n/a | AWS (smithy-rs) |
| [`reqsign-aws-v4`](https://crates.io/crates/reqsign-aws-v4) 3.3.1 (2026-09-25) | Apache-2.0 | 1.86 ok | (in the OpenDAL tree) | | `reqsign-core` (hmac, sha1, sha2, jiff, asyncband) | yes | yes | n/a | Apache OpenDAL; **inside cooldown today** |

### 3.1 `object_store`

`object_store` is the most mature general abstraction in Rust (arrow-rs, now
[apache/arrow-rs-object-store](https://github.com/apache/arrow-rs-object-store)).

- **Trait:** `ObjectStore` offers `put_opts`, `put_multipart_opts`, `get_opts` (range and
  `if_match` / `if_none_match` / `if_modified_since`), `get_ranges`, `list` (a stream),
  `list_with_offset`, `list_with_delimiter` (→ `ListResult { common_prefixes, objects }`),
  `copy_opts` and `rename_opts`.
- **Metadata:** `ObjectMeta { location, last_modified, size, e_tag, version }`.
- **Errors:** `NotFound`, `AlreadyExists`, `Precondition`, `NotModified`, `NotImplemented`,
  `PermissionDenied`, `Unauthenticated`, …
- **Since 0.13/0.14, reqwest is optional.** Using `aws-base` plus your own
  `client::HttpConnector` and a `CryptoProvider` (`ring`, `aws-lc-rs` or custom) is documented
  "to keep `reqwest` out of your dependency tree".
- It is async, and `cloud-base` still pulls tokio (`rt`, `sync`, `time`) and hyper.
- It has no WebDAV-proper, SFTP or consumer-drive backends. Its `http` store does
  "HTTP/WebDAV" GET, PUT and PROPFIND for simple servers.
- **Verdict:** a solid "S3/GCS/Azure only" option. For AzDrive, OpenDAL dominates it (more
  backends, same async cost).

### 3.2 `rusty-s3`

`rusty-s3` is a pure **sans-IO** S3 client. You build an action, get a signed URL, send it with
any HTTP client, and parse the response.

- **Actions:** Create, Delete and HeadBucket; Head, Get, Put and DeleteObject; DeleteObjects;
  ListObjectsV2 (`delimiter`, `start_after`, continuation token, `max_keys`); the multipart set.
- **Signing:** every request is **query-signed** (presigned-URL style) with `UNSIGNED-PAYLOAD`.
  About 350 non-test lines in `src/signing/`.
- Session tokens are supported (`Credentials::new_with_token`).
- **Gaps:** no CopyObject action and no header-auth mode.
- **Testing:** "Minio compatibility tested on every commit." Its unit tests embed the AWS
  documentation vectors (`AKIAIOSFODNN7EXAMPLE`, 20130524), which are handy as oracle vectors
  for a hand-rolled signer.
- **Verdict:** the best *library* fit if we do not hand-roll. It matches "SigV4 over azul's own
  HTTP client" exactly.

### 3.3 `aws-sdk-s3`

This is the full AWS SDK: everything (checksums, SigV4a, S3 Express, retries), at a high cost:

- 195 crates, both ring and aws-lc-sys, and a tokio/hyper runtime;
- MSRV 1.94.1, above the pin;
- weekly releases, which fight the 14-day cooldown.

**Not suitable for a GUI toolkit's app layer.**

### 3.4 `rust-s3`

`rust-s3` is a convenience client with async (tokio, async-std) and sync (attohttpc) modes.

- The default is native-tls.
- The rustls sync build still brings aws-lc-sys, through rustls's default provider.
- It has a single owner.
- **Verdict:** no advantage over `rusty-s3` plus our transport.

### 3.5 `s3s` (server side)

[`s3s`](https://crates.io/crates/s3s) 0.17.0 (2026-09-24), Apache-2.0, **MSRV 1.96.0**,
[s3s-project/s3s](https://github.com/s3s-project/s3s).

- It implements the S3 REST API as a generic hyper service, and **verifies SigV4** for both
  header and presigned requests; presigned expiry gives "Request has expired"
  (`crates/s3s/src/ops/signature.rs`).
- `s3s-fs` is "an experimental S3 server based on file system … **designed for integration
  testing**".
- RustFS (34k stars, Apache-2.0) builds on `s3s` 0.17.
- **Useful as a local test server binary**, installed with a separate newer toolchain (`cargo
  +stable install s3s-fs --features binary`). **Not** useful as an in-tree dependency because of
  the MSRV and hyper/tokio.

### 3.6 Plain-Rust SigV4 options

1. **Hand-rolled** on `hmac` + `sha2` + `percent-encoding` + `url` (all already in azul's tree,
   with imported cargo-vet audits for hmac and sha2).
   - About 300 to 500 lines covering canonical request, string-to-sign, signing-key
     derivation, header auth and query auth.
   - Takes `now` as a parameter, so it works on wasm and tests are deterministic.
   - Cross-check against the AWS documentation vectors
     ([header auth](https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html),
     [query auth](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html))
     and against botocore through moto's auth mode (§6).
2. **`rusty-s3`'s signer** (above), with 3 new crates.
3. **`reqsign-aws-v4`**, OpenDAL's signer. It is small, but brings `reqsign-core`, jiff and
   asyncband, and is inside the cooldown today.
4. **`aws-sigv4`**, the official one. It is blocked by its MSRV and pulls the smithy runtime.
5. **Verification-only:** [`scratchstack-aws-signature`](https://crates.io/crates/scratchstack-aws-signature)
   0.11.4 (MIT), for a Rust test server or a Rust Worker.

---

## 4. Non-S3 backends the user may want later

### 4.1 WebDAV (Nextcloud)

- **Base path:** `/remote.php/dav/files/{user}/`.
- **Listing:** `PROPFIND` with `Depth: 1`.
- **Properties:** `getetag`, `getcontentlength`, `getlastmodified`, `resourcetype`,
  `oc:fileid`, `oc:permissions`, `oc:size`, and more
  ([Nextcloud WebDAV basics](https://docs.nextcloud.com/server/latest/developer_manual/client_apis/WebDAV/basic.html)).
- **Uploads:** large files use Nextcloud's separate chunked-upload protocol.
- **Auth:** use **Login Flow v2**. `POST /index.php/login/v2`, open the returned URL in the
  browser, then poll. The poll returns `{server, loginName, appPassword}`, once only; the token
  is valid for 20 minutes. Store the app password, never the real one
  ([Login Flow](https://docs.nextcloud.com/server/latest/developer_manual/client_apis/LoginFlow/index.html)).
- **Fit:** good. `getetag` is the version token, and `PROPFIND` depth 1 is the delimiter list.
  WebDAV has **real directories**, so `mkdir` exists; set `Capabilities.real_dirs`.
- **UNVERIFIED:** whether Range GET and `If-Match` on PUT work on stock Nextcloud (SabreDAV).
  Write the WebDAV conformance test before relying on them.
- **Implementation:** hand-roll it on azul's HTTP client and `roxmltree` (a PROPFIND
  multistatus parser is small). Alternatively use OpenDAL's `webdav` service (deps: `quick-xml`,
  `serde`, `http`; no extra transport). `reqwest_dav` 0.3.3 ties you to reqwest; `rustydav` is
  GPL-3.0 and stale since 2021.
- **Prior art:** R2 can also be *exposed* as WebDAV from a Worker (`fanchenggang/Davflare`,
  MIT, is the only properly licensed maintained one; see `cloud-platform-notes.md` §8.2.2). The
  reverse direction does not matter for AzDrive.

### 4.2 SFTP

| Option | Facts | Fit |
|---|---|---|
| [`russh`](https://crates.io/crates/russh) 0.63.3 + [`russh-sftp`](https://crates.io/crates/russh-sftp) 3.0.1 | Pure Rust, Apache-2.0, tokio-based | Cross-platform, including mobile, but brings tokio and its own crypto stack |
| [`ssh2`](https://crates.io/crates/ssh2) 0.9.6 | Binds libssh2 (C) and OpenSSL-ish crypto | Blocking API fits azul threads, but it is C code and cross-compiling to mobile is painful |
| [`openssh-sftp-client`](https://crates.io/crates/openssh-sftp-client) 0.15.9 (used by OpenDAL `sftp`) | Spawns the system `ssh`; **Unix-only** (`compile_error!` otherwise); key auth only | Desktop Linux and macOS only |

- **Semantics:** SFTP has real directories, `stat`, ranges (seek + read), and atomic rename.
  There is no ETag, so use `(mtime, size)` as the version token. There is no presign.
- **Recommendation:** defer. When needed, `russh-sftp` behind a feature is the only choice that
  works everywhere.

### 4.3 Google Drive / OneDrive / Dropbox: the OAuth burden

All three need an OAuth client registration, a browser consent flow (PKCE for native apps), a
token refresh store, and per-provider API quirks. None is path-addressed like S3:

- Google Drive is ID-based;
- OneDrive/Graph supports path addressing;
- Dropbox is path-based.

| Provider | Burden (checked 2026-09-30) |
|---|---|
| Google Drive | `drive.file` is **non-sensitive**, but only covers files the app created or the user opened with it. `drive`, `drive.readonly`, `drive.metadata` and `drive.metadata.readonly` are **restricted**: they need "restricted scope OAuth App Verification", and "if you store or transmit restricted scope data on servers, you must go through a **security assessment**" ([scopes page](https://developers.google.com/workspace/drive/api/guides/api-specific-auth), updated 2026-09-03). A general "browse my Drive" feature needs a restricted scope |
| OneDrive (Microsoft Graph) | Entra app registration. Since November 2020, end users cannot consent to *newly registered multitenant apps without a verified publisher* (tenant policy dependent); unverified apps show a warning ([publisher verification](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview)). Publisher verification needs a Microsoft partner (MPN) ID |
| Dropbox | Apps start in "development" status. Once an app links **50 users** it has two weeks to obtain production approval, or linking new users freezes ([developer guide](https://www.dropbox.com/developers/reference/developer-guide)) |

OpenDAL's `gdrive`, `onedrive` and `dropbox` services accept tokens but do not run consent
(§2.1), so the OAuth work stays with the app either way. **Recommendation:** later, and only
through OpenDAL, once `Drive` has a `CredentialSource` that can refresh OAuth tokens.

### 4.4 Git on object storage

The user's reference is [Cursor, "Git at any scale"](https://cursor.com/blog/git-at-any-scale)
(Vicent Martí, 2026-08-18). The system is called **Continuity**.

- "The core primitive behind it is a **write-ahead log, which we store in S3-compatible object
  storage**."
- Each push is a WAL entry (a packfile uploaded to S3). A push "is only visible once we
  successfully prepare its reference transaction on a local copy of the repository and record a
  pointer to the WAL entry in the WAL index file".
- "All updates to the write-ahead log are synchronized with an **atomic compare-and-swap (CAS)
  operation on S3**."
- Replicas keep a normal git repo on NVMe as a warm cache and verify freshness with "a
  **conditional GET to S3 with the ETag** we expect" (304 means current, 200 means catch up).
- Clients use stock git. The post does not say the code is open source.

Open-source prior art: [awslabs/git-remote-s3](https://github.com/awslabs/git-remote-s3)
(Apache-2.0, 840 stars, pushed 2026-08-20). It is a git remote helper plus a git-lfs custom
transfer.

- **Layout:** `<prefix>/<ref>/<sha>.bundle`.
- **Pushes take a per-ref lock** by creating `<prefix>/<ref>/LOCK#.lock` with **`IfNoneMatch="*"`**,
  re-read the ref, upload, delete the old bundle, and unlock. A lock is stale after 60 s.
- **LFS objects** live at `<prefix>/lfs/<oid>`.

**What this demands of `Drive`:**

- `put` with `CreateNew` (`If-None-Match: *`) and `IfVersion(etag)` (`If-Match`);
- conditional `get` (`If-None-Match` → 304 "not modified");
- a stable ETag in `Entry.version`.

AWS S3 documents both conditional-write headers, with **412 Precondition Failed** on conflict
([conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html)).
R2 supports `If-Match`, `If-None-Match`, `If-Modified-Since` and `If-Unmodified-Since` on
PutObject ([R2 S3 compatibility](https://developers.cloudflare.com/r2/api/s3/api/), updated
2026-07-31). moto implements both on PUT (§6).

A git feature can then be built as a remote helper on top of `Drive`. The simplest is
git-remote-s3's bundle-per-ref scheme; `gix` 0.88 is available for the pack side. **Put
conditional put in the trait from day one.**

### 4.5 Local caching, offline mode, conflicts

Prior art: rclone's VFS ([rclone mount](https://rclone.org/commands/rclone_mount/)).

- `--vfs-cache-mode off | minimal | writes | full`. In `full`, "the files in the cache will be
  **sparse files** and rclone will keep track of which bits of the files it has downloaded".
- `--dir-cache-time` defaults to 5 minutes; `--vfs-read-chunk-size` defaults to 128Mi.

This matches the "desktop drive onto a dataset" power-user ask.

Proposal, in order:

1. **Listing cache.** `CachedDrive` keeps `list` pages with a TTL (start at 60 s, since
   fsspec's default is none and rclone's is 5 min) and invalidates them on its own writes.
   A refresh button bypasses it.
2. **On-demand file cache.** Store downloaded files as `cache/<drive-id>/<path>` with a sidecar
   holding `{version, size, fetched_at}`. Revalidate with conditional GET (`If-None-Match`,
   304). This is "download ONE file".
3. **Block cache for previews.** An in-memory LRU of 1 to 8 MiB blocks via `BlockReader`. A
   later step could use a sparse on-disk cache (rclone full mode, fsspec `blockcache::`).
4. **Offline edits.**
   - An outbox of `(path, local_file, base_version)` entries.
   - On reconnect, `put(IfVersion(base_version))`.
   - On a 412, never overwrite. Write `name (conflicted copy <device> <date>).ext` next to the
     original and surface both in the UI. This is the Dropbox and Nextcloud convention.
   - New files use `CreateNew`.
   - Deletes use a conditional DELETE when the store supports it. R2 and S3 DeleteObject
     conditional support differs: **UNVERIFIED**, check before relying on it.
5. **Mail is special.** AzMail owns `/mail/` sync. Treat those objects as write-once, where a
   new message is a new key, so conflicts cannot happen by construction. Keep flags and state
   as separate small objects, never as rewrites of the `.eml`. This keeps them out of the DB,
   per the storage-split ruling.

---

## 5. RBAC and access links

### 5.1 How S3-compatible stores grant per-prefix access

| Mechanism | Scope | Lifetime and revocation | Round trips | Notes |
|---|---|---|---|---|
| **Presigned URL** (SigV4 query auth) | One operation on one key; a *list* URL is fixed to its query, including the continuation token | AWS: up to **7 days** with IAM-user credentials, and "if you created a presigned URL by using a temporary token, then the URL expires when the token expires" ([AWS](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html)). R2: 1 s to 7 days, **reusable until expiry, a bearer token**, GET/HEAD/PUT/DELETE only, and **custom domains do not work** ([R2 presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/), 2026-08-22) | One Worker round trip **per operation and per list page** | Great for "send someone one file"; chatty for browsing |
| **AWS STS session policy** | Anything IAM can express: `arn:aws:s3:::bucket/users/u1/*`, plus `s3:ListBucket` with an `s3:prefix` condition | `AssumeRole` `DurationSeconds` runs from 900 s to the role maximum (1 to 12 h). "The resulting session's permissions are the **intersection** of the role's identity-based policy and the session policies"; inline plus up to 10 managed ARNs, at most 2,048 characters ([AssumeRole](https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html)) | One STS call per mint | AWS only (MinIO has an STS too). Needs an IAM role |
| **R2 API token** | **Per bucket.** Object Read & Write / Object Read only / Admin. The docs mention **no prefix scoping**. Access key = token id; secret = SHA-256 of the token value ([R2 tokens](https://developers.cloudflare.com/r2/api/tokens/), 2026-09-28) | Long-lived until deleted | A Cloudflare API call per token | Fits "one bucket per user"; does not fit sharing `/mail/` or one meeting folder with someone |
| **R2 temporary credentials** | **One bucket + `scope` preset or an `actions` list + `prefixPaths` / `objectPaths`** | "If you revoke the parent API token, all temporary credentials derived from it stop working immediately." TTL via `ttlSeconds`; no documented minimum or maximum | **Zero** with local signing: HS256 JWT signed with the parent secret; temp secret = SHA-256 hex of the signed JWT; session token = `base64("jwt/" + jwt)`. "`actions` is currently supported via local signing only" ([R2 temporary credentials](https://developers.cloudflare.com/r2/api/s3/temporary-credentials/), 2026-04-24) | Standard SigV4 + `X-Amz-Security-Token`, so the normal S3 client uses it unchanged |
| **Worker proxy** (stream through an R2 binding) | Anything the Worker's code checks | Instant (the DB row decides per request) | Every byte flows through the Worker | Works on custom domains and needs no CORS. You implement Range and listing yourself. Workers connection limits apply (`cloud-platform-notes.md` §1) |

### 5.2 Recommendation: a credential-vending Worker (R2 temporary credentials)

This is consistent with "the DB only holds access links". A row decides, storage enforces, and
no content goes into the DB.

**The access-link table (Turso), sketch:**

```sql
CREATE TABLE access_link (
  id          TEXT PRIMARY KEY,             -- uuid
  owner       TEXT NOT NULL,                -- user id who granted it
  bucket      TEXT NOT NULL,
  prefix      TEXT NOT NULL,                -- 'users/<uid>/mail/' (always ends in '/') or one exact key
  grantee     TEXT,                         -- user id; NULL = "anyone holding the link token"
  token_hash  BLOB,                         -- SHA-256 of the link secret when grantee IS NULL
  mode        TEXT NOT NULL CHECK (mode IN ('read','write')),
  expires_at  INTEGER,                      -- NULL = until revoked
  revoked_at  INTEGER,
  created_at  INTEGER NOT NULL
);
-- Implicit rows (not stored): owner -> 'users/<owner>/' read+write; admins -> whole bucket.
```

**Flow for AzDrive browsing:**

1. `POST /drive/credentials {link_id | drive: "users/<uid>/"}` with the user's session.
2. The Worker loads the matching live `access_link` row and checks that the grantee or token
   matches, that it has not expired and that it is not revoked.
3. The Worker locally signs R2 temporary credentials: `bucket`, `paths.prefixPaths: [prefix]`,
   `actions` and `ttlSeconds: 900`.
   - read = `HeadObject`, `GetObject`, `ListObjectsV2`;
   - write adds `PutObject`, `DeleteObject`, `CopyObject` and the multipart set.
4. The Worker returns
   `{endpoint, bucket, prefix, access_key_id, secret_access_key, session_token, expires_at}`.
5. AzDrive's `S3Drive` talks **directly to R2**: ranged GETs, paginated lists, and no Worker in
   the data path.
6. On `ExpiredToken` or 403, or 60 s before `expires_at`, the `CredentialSource` calls the
   Worker again. A revoked link fails at the next refresh; revoking the parent token kills
   everything at once.

**For external or one-off shares** (a recipient without AzDrive): the link carries a secret
whose SHA-256 is `token_hash`. The Worker verifies it, then **302-redirects to a presigned GET**
with a TTL of 60 to 300 s on `<account>.r2.cloudflarestorage.com`. On the user's own custom
domain it **streams through the R2 binding** instead, because presigned URLs do not work on
custom domains. The one-time-use and replay-table patterns are in `cloud-platform-notes.md`
§8.2.2 (nodewarden's `jti` table).

**Non-R2 backends:**

- AWS: the same Worker shape mints via `AssumeRole` with a session policy (one subrequest,
  cacheable per `(user, prefix)` for the TTL).
- Any S3 store without an STS: fall back to presign-per-operation or the streaming proxy.

**Client consequence (design now):** `S3Drive` must accept
`S3Credentials { access_key_id, secret_access_key, session_token: Option<String>, expires_at:
Option<i64> }` from a `CredentialSource` that can refresh. It must send
`X-Amz-Security-Token` and treat `ExpiredToken` as "refresh and retry once".

**Open questions, test before building on them:**

1. Does R2 enforce `prefixPaths` on **ListObjectsV2**? For example, is a list with
   `prefix=users/other/`, or with no prefix, rejected, or filtered? The temp-credentials page
   does not say. **UNVERIFIED.**
2. What are the `ttlSeconds` bounds? None are documented. **UNVERIFIED.**
3. Local-signing JWT claim names beyond `paths.prefixPaths` / `paths.objectPaths`: copy them
   from the page's example when implementing; this report did not transcribe the full payload.

---

## 6. Local test servers (Python preferred)

| Server | Language, license | API shape fidelity | SigV4 verification | RBAC emulation | Notes |
|---|---|---|---|---|---|
| **moto_server** ([getmoto/moto](https://github.com/getmoto/moto), 5.2.3 on PyPI 2026-08-22; master `1bf1674`, 2026-09-29) | Python, Apache-2.0 | **High**: the de facto S3 mock for boto3 users (8.7k stars). ListObjectsV2 `delimiter` / `continuation-token`, Range (`_handle_range_header`), multipart, and **`If-Match` / `If-None-Match: *` on PUT** (`moto/s3/responses.py:2271-2289`) | **Off by default** ("defaults to infinity, thus moto will never perform any authentication"). With `INITIAL_NO_AUTH_ACTION_COUNT=0` it recomputes **header** signatures with botocore and evaluates IAM policies (`moto/iam/access_control.py`; the docs call it "very basic"). **Presigned URLs are not signature- or expiry-checked**: only header metadata consistency (`_invalid_headers`) | Role policies are evaluated for assumed roles, but **session policies are ignored** (`AssumedRoleAccessKey.collect_policies` only reads role policies). No R2 temp credentials | `pip install "moto[server]"`; `moto_server -p 5000`, or `ThreadedMotoServer(port=0)` in pytest ([server mode docs](https://github.com/getmoto/moto/blob/master/docs/docs/server_mode.rst)) |
| **MinIO** | Go, AGPL-3.0 | Was the de facto self-hosted S3 | Real | Real STS | **Repo archived; README: "THIS REPOSITORY IS NO LONGER MAINTAINED"**, community edition "distributed as source code only" (last push 2026-04-24). Do not build new test infrastructure on it |
| **s3s-fs** (§3.5) | Rust, Apache-2.0 | Medium to high | **Real, header and presigned, with expiry** | No | "designed for integration testing"; needs toolchain ≥ 1.96 to install |
| RustFS / SeaweedFS / versitygw / Garage | Rust Apache-2.0 / Go Apache-2.0 / Go Apache-2.0 / Rust AGPL-3.0 | Real servers | RustFS is built on `s3s` (so real); others not checked | Varies | Heavier; fine as an optional CI leg |
| LocalStack | Python (moto-derived) | High | — | — | **Since 2026.03.0 it needs an auth token or account** ([LocalStack blog](https://blog.localstack.cloud/localstack-for-aws-release-2026-03-0/)). Skip |
| **stdlib mini-S3** (to write) | Python stdlib only | Low to medium: only what we implement | **Strict, if we write it** (`hmac`, `hashlib`): header and query auth, `X-Amz-Expires`, clock skew | **Yes: emulate R2 temp credentials** (session token → `{bucket, prefixPaths, actions, exp}`) | Also does deterministic fault injection: 503 SlowDown, 412, `max-keys=2` to force pagination, truncated bodies |

**Which is most faithful?** It depends on the dimension:

- **API semantics:** moto, among Python options by a wide margin.
- **Signatures:** moto in auth mode covers header-signed requests with botocore as the oracle,
  which is excellent for catching SigV4 encoding bugs. Nothing in Python checks presigned URLs
  unless we write it.
- **RBAC:** nothing local emulates R2 temporary credentials, so the access-link tests need the
  mini-S3.
- **Real R2 quirks:** only R2 has them (no versioning, no object tagging, `x-amz-checksum-*`
  differences, custom-domain behaviour).

**Recommended setup:**

1. **moto_server** fixture (pytest, `ThreadedMotoServer(port=0)`) for the `S3Drive`
   conformance suite: list/get/range/put/delete/head, pagination, conditional put, unicode and
   space keys.
2. The **same suite rerun with `INITIAL_NO_AUTH_ACTION_COUNT=0`** and an IAM user created
   through moto's IAM API. Every header-signed request is then verified by botocore.
3. A **~300-400 line stdlib `mini_s3.py`** for three things: presigned-URL and expiry
   verification, the R2-style scoped session tokens behind the access-link / credential-vending
   tests, and fault injection. Pair it with a mock of the credential-vending Worker, in the same
   style as the existing `meet` Worker's local mock.
4. A **manual or nightly smoke leg against a real R2 dev bucket**, including the §5.2 open
   question about List prefix enforcement.

---

## 7. Recommendation and migration path

### 7.1 The `Drive` trait (S3-first, OpenDAL-ready)

These conventions are chosen so that S3, the local filesystem, WebDAV, SFTP and OpenDAL all
map without changes to the apps:

- **Paths** are drive-relative, `/`-separated strings without a leading `/`; a directory path
  ends in `/`; `""` is the root. This matches S3 keys and OpenDAL. `LocalDrive` translates to
  OS paths and rejects `..`.
- **Versions** are opaque strings: S3 ETag, WebDAV `getetag`, and `(mtime, size)` for local and
  SFTP.
- **Cursors** are opaque strings: an S3 continuation token, an OpenDAL `start_after` key, or an
  offset for local.
- **Blocking**, called from azul worker threads (the async-task design). Signing and parsing
  are **sans-IO pure functions** behind a `Transport` trait, so wasm can drive the same code
  from `fetch`.

```rust
pub enum EntryKind { File, Dir }

pub struct Entry {
    pub path: String,                 // "mail/INBOX/0001.eml" or "mail/INBOX/"
    pub kind: EntryKind,
    pub size: Option<u64>,            // None for S3 common prefixes
    pub modified_unix_ms: Option<i64>,
    pub version: Option<String>,      // opaque change token
    pub content_type: Option<String>,
}

pub struct ListPage { pub entries: Vec<Entry>, pub cursor: Option<String> }

pub enum PutMode { Overwrite, CreateNew /* If-None-Match: * */, IfVersion(String) /* If-Match */ }

pub struct Written { pub version: Option<String> }

#[non_exhaustive]
pub enum DriveErrorKind {
    NotFound, AlreadyExists, VersionMismatch /* 412 */, NotModified /* 304 */,
    PermissionDenied, CredentialsExpired /* refresh + retry once */, RateLimited,
    RangeNotSatisfiable, Unsupported, InvalidPath, Transport, Protocol,
}
pub struct DriveError { pub kind: DriveErrorKind, pub message: String, pub retry_after_ms: Option<u64> }

pub struct Capabilities {
    pub range_read: bool,
    pub conditional_put: bool,       // CreateNew + IfVersion
    pub conditional_get: bool,       // If-None-Match -> NotModified
    pub list_recursive: bool,        // flat listing without delimiter
    pub real_dirs: bool,             // false on S3: "folders" are prefixes, empty ones need a marker
    pub server_side_copy: bool,
    pub presign: bool,
    pub max_single_put: Option<u64>, // above this, the helper uses multipart (S3) / chunked (Nextcloud)
}

pub trait Drive: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn list(&self, dir: &str, cursor: Option<&str>) -> Result<ListPage, DriveError>; // one level
    fn head(&self, path: &str) -> Result<Entry, DriveError>;
    fn get(&self, path: &str, range: Option<core::ops::Range<u64>>, if_none_match: Option<&str>)
        -> Result<(Entry, Vec<u8>), DriveError>;
    fn put(&self, path: &str, data: &[u8], mode: PutMode) -> Result<Written, DriveError>;
    fn delete(&self, path: &str) -> Result<(), DriveError>;

    // Optional; defaults return Unsupported, so adding them later is not breaking.
    fn list_recursive(&self, _prefix: &str, _cursor: Option<&str>) -> Result<ListPage, DriveError> { unsupported() }
    fn copy(&self, _from: &str, _to: &str) -> Result<Written, DriveError> { unsupported() }
    fn presign(&self, _op: PresignOp, _path: &str, _ttl_secs: u32) -> Result<PresignedRequest, DriveError> { unsupported() }
    fn get_to(&self, path: &str, range: Option<core::ops::Range<u64>>, sink: &mut dyn std::io::Write)
        -> Result<Entry, DriveError> { /* default: get() then write_all */ }
}

pub struct S3Credentials { pub access_key_id: String, pub secret_access_key: String,
                           pub session_token: Option<String>, pub expires_at_unix: Option<i64> }
pub trait CredentialSource: Send + Sync {
    fn current(&self) -> Result<S3Credentials, DriveError>;  // cached
    fn refresh(&self) -> Result<S3Credentials, DriveError>;  // after CredentialsExpired / near expiry
}
// Implementations: StaticCredentials (user's own keys), VendedCredentials (POST /drive/credentials).
```

**Layers on top**, fsspec-style, generic over `&dyn Drive`, so every backend gets them:

- `PrefixDrive`: the access-link root, like `dir::`;
- `CachedDrive`: listing TTL plus a whole-file cache revalidated with `If-None-Match`;
- `BlockReader: Read + Seek`: LRU blocks over ranged `get`;
- helpers: `walk`, `find`, `glob`, `exists`, `download_to(progress)`, `upload_from(progress)`,
  the last one doing multipart above `max_single_put`;
- `open_drive(&DriveSpec)`: a URL-scheme registry, as a plain `match`.

Deliberately *not* in v1: mkdir, rename and move (on S3, rename is copy plus delete, which is not
atomic). Add them later as optional methods with `real_dirs` and `server_side_copy` gating the
UI.

### 7.2 Which dependency to take now

**None of the SDKs.** Hand-roll:

- SigV4 (header auth for `S3Drive` requests, query auth for `presign`);
- the four S3 XML shapes (`ListBucketResult` v2, `Error`, and the multipart
  Initiate/Complete).

Use crates already in azul's tree: `hmac`, `sha2`, `percent-encoding`, `url`, and `roxmltree`
(azul-layout's `xml` feature). The cost is 0 new crates, 0 build scripts, and no conflict with
the tokio rule, the MSRV pin or the cooldown.

- **Fallback, if hand-rolling looks risky:** `rusty-s3` 0.10.2 with
  `default-features = false, features = ["rustcrypto"]`. That is 3 new crates and 60 days old,
  so it passes the cooldown. Parse its responses with `roxmltree` rather than enabling
  `full`/`instant-xml`. Accept that it is query-signing only.
- **Either way:** the signer takes `now` as input. Test vectors come from the AWS docs and
  rusty-s3's tests, and moto auth mode acts as the botocore oracle.

### 7.3 Migration path

| Phase | Deliverable | Dependencies |
|---|---|---|
| **0 (now)** | Shared storage crate: `Drive`, `LocalDrive`, `S3Drive` (hand-rolled SigV4 over azul's `HttpClient`), `CredentialSource` (static), `PrefixDrive`, `BlockReader`, `open_drive`. AzMail export writes through it. Tests: moto_server suite plus moto auth-mode rerun | none new |
| 1 | Credential-vending Worker (R2 temp credentials) + `VendedCredentials`; `access_link` table; `mini_s3.py` with scoped tokens; share links (Worker verify → 302 presign / stream); `CachedDrive` | none new (Worker side: WebCrypto only) |
| 2 | `WebDavDrive` (Nextcloud, Login Flow v2) hand-rolled on the same HTTP client and `roxmltree`; offline outbox with `IfVersion` and conflict copies | none new |
| 3 (opt-in) | `OpendalDrive` behind a cargo feature `opendal`, preferably `opendal-core` + a custom `HttpTransport` over azul's client (§2.6, route 2). This unlocks gdrive, onedrive, dropbox, sftp (Unix), GCS, Azure and B2 without touching apps | `opendal-core` + chosen services (about 103 crates for s3 alone), each version older than 14 days |
| 4 | Git on S3: a remote helper over `Drive` using the conditional-put lock scheme (git-remote-s3 style) or a WAL + CAS (Continuity style) | possibly `gix` |

### 7.4 Risks

1. **Hand-rolled SigV4 bugs.**
   - Classic traps: per-segment URI encoding with `/` kept (S3 does *not* double-encode, unlike
     other AWS services); `+`, space and unicode in keys; empty query values; sorting of
     repeated query keys; header value whitespace folding; the `x-amz-content-sha256` header
     (use `UNSIGNED-PAYLOAD` over TLS); `region=auto` for R2; path-style vs virtual-host
     addressing; clock skew (`RequestTimeTooSkewed`).
   - *Mitigation:* the vector tests, moto auth mode (botocore oracle), a key-name fuzz list, and
     the R2 smoke leg.
2. **S3-compatible drift.** R2 has no versioning and no tagging. Its `x-amz-checksum-algorithm`
   support is partial, and CopyObject does not support the source-conditional headers
   ([R2 compat](https://developers.cloudflare.com/r2/api/s3/api/)). Multipart ETags are not MD5
   (never compare ETags to content hashes).
3. **RBAC unknowns** (§5.2): whether List is prefix-enforced, the TTL bounds, bearer-token
   reuse of presigned URLs, and the fact that parent-token revocation is all-or-nothing.
4. **Directory semantics.** S3 has no folders. Empty folders need `dir/` marker objects, and
   rename is non-atomic copy plus delete. The UI must read `Capabilities.real_dirs`.
5. **Scale.** Pages are at most 1,000 keys, and `/mail/` may hold 10^5 objects. The explorer
   must virtualize and paginate lazily; never `list_recursive` to compute folder sizes.
6. **wasm.** Blocking HTTP is impossible in the browser, so the async transport must drive the
   sans-IO core. Direct browser access to R2 needs bucket **CORS** (expose `ETag`, allow
   `Range`). Credentials live in page memory; keep the TTL short or proxy through the Worker.
7. **Dependency churn if OpenDAL arrives.** Breaking 0.x releases every 1 to 2 months and
   same-week `reqsign` bumps collide with the 14-day cooldown and the justification gate. Pin
   minor versions and upgrade in batches.
8. **MSRV creep.** `aws-sdk-s3`/`aws-sigv4` (1.94.1) and `s3s` (1.96.0) already exceed the pin,
   and OpenDAL sits exactly on it. An OpenDAL upgrade may force a toolchain bump.
9. **Test infrastructure rot.** MinIO is archived and LocalStack now needs an account. moto is
   the stable Python choice, but it does not verify presigned URLs or session policies, so the
   strict mini-S3 is not optional for the RBAC tests.
10. **OAuth backends** (Google restricted scopes and security assessment, Microsoft publisher
    verification, Dropbox production review) are a product and legal cost, not a code cost. Do
    not promise them early.

---

## Appendix A: how the dependency numbers were produced

- **Setup:** scratch crates (edition 2021, empty `[workspace]`), one per configuration, each
  with only the listed dependency. `cargo generate-lockfile`, then
  `cargo tree -e normal,build --prefix none --target aarch64-apple-darwin`, deduplicated by
  `name version`. Run with cargo 1.91.0 on 2026-09-30.
- **Resolution:** resolver 2 ignores `rust-version`, so the latest versions were resolved even
  when their MSRV exceeds 1.91.
- **"New" column:** names absent from `/Users/fschutt/Development/azul/Cargo.lock` at branch
  `wt/x-mail-explore` (1,056 name-version pairs).
- **What these numbers are:** resolution counts, not compile tests. A config that resolves for
  wasm may still fail to compile.

## Appendix B: sources

- fsspec:
  - [API](https://filesystem-spec.readthedocs.io/en/latest/api.html),
    [Features](https://filesystem-spec.readthedocs.io/en/latest/features.html);
  - source `fsspec/spec.py`, `caching.py`, `asyn.py`, `registry.py` and
    `implementations/dirfs.py` on master, 2026-09-29;
  - s3fs `core.py`; PyPI `fsspec` 2026.9.0.
- fsspec_rs:
  - [crates.io](https://crates.io/crates/fsspec_rs), [GitHub](https://github.com/1kbgz/fsspec-rs),
    `docs/src/rust.md`, `rust/Cargo.toml`.
- OpenDAL:
  - [crates.io 0.59.3](https://crates.io/crates/opendal), [GitHub](https://github.com/apache/opendal)
    (commit `53610f8`, 2026-09-24), and the `opendal-core` 0.59.3 crate source (`blocking/`,
    `types/http_transport/`, `types/execute/`, `docs/rfcs/7749_http_transporter.md`);
  - [graduation](https://opendal.apache.org/blog/apache-opendal-graduated/);
  - [openssh crate](https://github.com/openssh-rust/openssh) ("only works on unix").
- object_store:
  - [crates.io 0.14.2](https://crates.io/crates/object_store), with crate source `README.md`
    and `src/lib.rs` (feature, crypto and wasm docs).
- Other S3 crates:
  - [rusty-s3](https://github.com/paolobarbolini/rusty-s3) (0.10.2 source),
    [aws-sdk-s3](https://crates.io/crates/aws-sdk-s3), [rust-s3](https://github.com/durch/rust-s3),
    [s3s](https://github.com/s3s-project/s3s) (commit `c2187da`, 2026-09-28),
    [aws-sigv4](https://crates.io/crates/aws-sigv4),
    [reqsign](https://github.com/apache/opendal-reqsign),
    [scratchstack-aws-signature](https://crates.io/crates/scratchstack-aws-signature).
- Cloudflare R2:
  - [presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/),
    [temporary credentials](https://developers.cloudflare.com/r2/api/s3/temporary-credentials/),
    [API tokens](https://developers.cloudflare.com/r2/api/tokens/),
    [S3 compatibility](https://developers.cloudflare.com/r2/api/s3/api/);
  - see also `../azul-apps/planning/cloud-platform-notes.md` §2.5 and §8.2.2.
- AWS:
  - [AssumeRole](https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html),
    [presigned URLs](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html),
    [conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html),
    [SigV4 header auth](https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html),
    [SigV4 query auth](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html),
    [ListObjectsV2](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html).
- Git on object storage:
  - [Cursor, Git at any scale](https://cursor.com/blog/git-at-any-scale) (2026-08-18),
    [awslabs/git-remote-s3](https://github.com/awslabs/git-remote-s3).
- Other backends:
  - [Nextcloud WebDAV](https://docs.nextcloud.com/server/latest/developer_manual/client_apis/WebDAV/basic.html),
    [Nextcloud Login Flow](https://docs.nextcloud.com/server/latest/developer_manual/client_apis/LoginFlow/index.html);
  - [Google Drive scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth),
    [Microsoft publisher verification](https://learn.microsoft.com/en-us/entra/identity-platform/publisher-verification-overview),
    [Dropbox developer guide](https://www.dropbox.com/developers/reference/developer-guide);
  - [rclone mount VFS](https://rclone.org/commands/rclone_mount/).
- Test servers:
  - [moto](https://github.com/getmoto/moto) (commit `1bf1674`; `docs/docs/iam.rst`,
    `server_mode.rst`, `moto/s3/responses.py`, `moto/iam/access_control.py`);
  - [MinIO README](https://github.com/minio/minio) (archived);
  - [LocalStack 2026.03.0](https://blog.localstack.cloud/localstack-for-aws-release-2026-03-0/).

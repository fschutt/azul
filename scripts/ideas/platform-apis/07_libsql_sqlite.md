# 07 — libsql / SQLite persistence layer for azul

**Status:** research brief — superseded by next-session implementation.
**Scope:** Feature #13 in [SUPER_PLAN_2 §1](../../SUPER_PLAN_2.md). Adds a built-in persistence layer reachable from the `f(State) -> Dom` callback model so a user can write a full mobile app without bolting on an ORM. Connection string drives mode selection: `:memory:` (transient), `file:/path/to/db.sqlite` (local), `libsql://host[:port]?authToken=...` (remote Turso-style).
**Last updated:** 2026-05-19.

Citations are inline and bare-URL; the user's network can render. `TODO: verify` markers flag claims that need an offline second pass.

---

## 1. Crate landscape — moving target, pick carefully

The biggest finding: the SQLite-on-Rust story split in two during 2025–2026 and the framework needs to choose a side, or build a thin abstraction over both.

### 1.1 `libsql` (turso/libsql) — what to ship with today

* **Crate:** `libsql` on crates.io. Current: **0.9.30, released 2026-03-19** (per lib.rs metrics).
  Source: <https://crates.io/crates/libsql>, <https://github.com/tursodatabase/libsql>.
* **Status:** "Production-ready" per Turso's own docs (<https://github.com/tursodatabase/turso> — see §1.2). 54 published releases, ~200k downloads/month, used by 127 crates.
* **API surface** (<https://docs.rs/libsql/latest/libsql/>):
  * `Builder` — constructs `Database`. Five constructors map to five modes:
    * `Builder::new_local(path)` → local file or `:memory:` (path string `":memory:"` is recognised verbatim).
    * `Builder::new_remote(url, auth_token)` → pure HTTP/WS, no local storage. The remote mode.
    * `Builder::new_remote_replica(path, url, auth_token)` → embedded replica with local cache, writes delegated to primary.
    * `Builder::new_local_replica(path)` → embedded replica requiring manual `Database::sync_frames()`.
    * `Builder::new_synced_database(path, url, auth_token)` → offline writes with later sync.
  * `Database::connect() -> Result<Connection>` — yields a `Connection`.
  * `Connection`: `execute(sql, params).await`, `query(sql, params).await -> Rows`, `prepare(sql).await -> Statement`, `transaction().await`, `last_insert_rowid()` (sync), `total_changes()` (sync), `is_autocommit()` (sync), `add_update_hook(callback)` (NB: spelled `add_update_hook`, not `set_update_hook` — different from rusqlite; verify in source).
  * `Connection: Clone + Send + Sync` — safe to share a clone across async tasks without external locking.
* **Async / sync:** **async only, hard dependency on `tokio ^1.29.1`** (non-optional). No blocking wrapper exists in the crate. Source: <https://docs.rs/libsql/0.9.30/libsql/>.
* **Wire protocol** (remote mode): Hrana 3. WebSocket variant multiplexes multiple SQL streams over a single connection; HTTP variant is stateless with "baton" continuation tokens. Both JSON and Protobuf encodings; servers must support both, clients choose. JWT auth in a hello message. Source: <https://github.com/tursodatabase/libsql/blob/main/docs/HRANA_3_SPEC.md>.
* **URL schemes the server / client speak** (Turso TS docs at <https://docs.turso.tech/sdk/ts/reference>):
  * `libsql:` / `libsql+ws:` / `libsql+wss:` → Hrana over WebSocket (tls / no-tls variants).
  * `ws:` / `wss:` → raw WebSocket Hrana.
  * `http:` / `https:` → Hrana over HTTP.
  * `file:` → local SQLite file via `libsql-sys`.
  * `:memory:` → bare special-cased path string.
  `TODO: verify` — confirm Rust `Builder::new_remote()` accepts all of `libsql://`, `libsql+wss://`, `https://`, and whether it rewrites `libsql://` → `https://` internally (the JS client does).
* **Encryption (native libsql, not SQLCipher)** — `EncryptionConfig { cipher: Cipher, encryption_key: Bytes }`. As of 0.9.30 the only `Cipher` variant is **`Aes256Cbc`** (page-level AES-256-CBC). Source: <https://docs.rs/libsql/0.9.30/libsql/enum.Cipher.html>. Not file-format-compatible with SQLCipher.
* **Migrations:** none built-in. Users bring their own (`refinery`, `sqlx-migrate`, `sqlite_loadable`, or a bespoke "run all `.sql` in `migrations/`" loop). The framework should ship a tiny `App::run_migrations(&[(version, sql)])` helper rather than a dependency on refinery. `TODO: verify` — check whether refinery 0.8+ supports a custom driver trait that accepts `libsql::Connection`.

### 1.2 `turso` — the new direction, not yet ready

* **Crate:** `turso` on crates.io. Current: **0.6.0**; main branch is `0.7.0-pre.1`. Source: <https://docs.rs/turso/latest/turso/>, <https://github.com/tursodatabase/turso/blob/main/Cargo.toml>.
* **What it is:** A *complete rewrite* of SQLite in Rust (formerly the "limbo" project). Turso the company has declared: "Rewriting SQLite in Rust started as an unassuming experiment, and due to its incredible success, replaces libSQL as our intended direction." Source: <https://github.com/tursodatabase/turso>.
* **Status:** **NOT production-ready** per Turso's own README: "evolving rapidly". Documentation coverage 51% (lib.rs). Use libsql until turso reaches 1.0.
* **API parity:** Builder / Database / Connection / Statement — same shape as libsql, also async-first on tokio.
* **Official guidance** (<https://docs.turso.tech/sdk/rust/quickstart>):
  * For local + cloud sync: **`turso` crate with `sync` feature**.
  * For remote-only access: **`libsql` crate with `remote` feature** (smaller dep footprint).
* **Recommendation for azul:** start with `libsql`, design the framework wrapper to be swappable to `turso` later. The abstraction surface is tiny (Connection, Statement, Row) and both crates share it.

### 1.3 `rusqlite` — the synchronous incumbent

* **Crate:** `rusqlite` on crates.io. Current: **0.39.0** (per docs.rs). `TODO: verify` release date — likely late 2025 / early 2026.
* **API:** synchronous; idiomatic `Connection::open()`, `Connection::open_in_memory()`, `conn.execute(...)`, `conn.prepare(...).query_map(...)`. No async. Source: <https://docs.rs/rusqlite/latest/rusqlite/>.
* **Reach:** local file + `:memory:` only. **No remote mode.** Pairing it with a separate Hrana client would re-implement what libsql gives for free.
* **Key features** (<https://docs.rs/crate/rusqlite/0.39.0/features>):
  * `bundled` — vendors SQLite source, no system dependency.
  * `bundled-sqlcipher` / `bundled-sqlcipher-vendored-openssl` — vendors SQLCipher (SQLite + encryption, file-format-compatible with SQLCipher elsewhere).
  * `sqlcipher` — links system SQLCipher.
  * `hooks` — enables `Connection::update_hook(cb)` / `commit_hook` / `rollback_hook`. Required for live-query design (§5.7).
  * `modern-full` — composite of 25+ features.
  * `load_extension`, `blob`, `backup`, `vtab`, `session`, `serialize`.
* **Why we still care:** if a user wants `:memory:` only (unit-test fixtures, demo apps), depending on libsql pulls in tokio + tonic + hyper-rustls (megabytes of binary, hard async tax). A `rusqlite`-only backend behind an `az-db-local` feature would save ~5–10 MB stripped on mobile. `TODO: verify` size delta.

### 1.4 `sqlx`

* **Crate:** `sqlx` 0.8.x. Async, multi-driver (Postgres / MySQL / SQLite).
* **Verdict for azul:** **no.** Three problems:
  1. **Does not speak libsql/Hrana.** SQLite driver talks to local `libsqlite3` only. A user typing `libsql://host` would get a parse error.
  2. Pulls a tokio / async-std selector dep (`runtime-tokio` or `runtime-async-std` Cargo feature must be set by the *consumer*, not the framework).
  3. Compile-time query validation is a feature azul users won't benefit from inside callbacks — they're constructing SQL dynamically based on `RefAny` state.
* When users *want* sqlx, they can `cargo add sqlx` next to azul. The framework shouldn't pre-empt that.

### 1.5 `sea-orm` / `diesel`

Out of scope. Both are higher-level ORMs that wrap one of the above drivers. Choosing the driver azul *ships* doesn't preclude users from putting `sea-orm` on top of it (sea-orm has a libsql driver feature; diesel does not).

### 1.6 SQLCipher standalone crate — does not exist

Standalone `sqlcipher` crate on crates.io: not found (the `sqlcipher` URL on docs.rs returns 404). SQLCipher in Rust is always accessed *through* `rusqlite` with the `bundled-sqlcipher` Cargo feature, or by linking system SQLCipher and using the `sqlcipher` feature. libsql's native page-level AES-256-CBC is the alternative.

### 1.7 Decision matrix

| Driver | Local file | `:memory:` | Remote | Async | Encryption | Binary cost | Verdict |
|---|---|---|---|---|---|---|---|
| `libsql` 0.9.30 | yes | yes | **yes (Hrana)** | tokio req'd | AES-256-CBC | high (~10MB) | **Primary, default** |
| `turso` 0.6.0 | yes | yes | yes (Hrana, via sync) | tokio req'd | `TODO: verify` | medium | Watch; swap when ≥1.0 |
| `rusqlite` 0.39 + `bundled` | yes | yes | no | sync | optional SQLCipher | low (~2MB) | Optional `local-only` feature |
| `sqlx` 0.8 | yes | yes | no | tokio/async-std | via sqlite | medium | Not shipped |

Recommendation: **ship `libsql` as default driver, gate `rusqlite-only` mode behind `default-features = false, features = ["db-local"]`** for users who genuinely don't want tokio in their binary.

---

## 2. Three-mode connection string

The user-facing API is one entry point:

```rust
pub fn open_database(url: &str) -> Result<DbHandle, DbError>
```

Parsing rules (azul-side, before handing to libsql):

| Input | Mode | libsql call |
|---|---|---|
| `":memory:"` | transient | `Builder::new_local(":memory:")` |
| `"file:..."` or absolute / relative path | local | `Builder::new_local(path)` (strip `file:` prefix per RFC 8089) |
| `"libsql://..."` / `"libsql+wss://..."` / `"https://..."` | remote | `Builder::new_remote(url, auth_token)` |
| `"libsql://host?embed=path/to.db"` | embedded replica | `Builder::new_remote_replica(path, url, auth_token)` |

Auth-token extraction: parse the URL's query string for `authToken=...` (matches `@libsql/client` JS conventions). Strip it from the URL **before logging or attaching the URL to any error** — see §6 risk.

The framework also resolves *implicit* paths via a new `App::data_dir() -> PathBuf` (§3.4). `open_database("app.db")` with no scheme means `<data_dir()>/app.db` after path resolution.

---

## 3. Mobile platform considerations

### 3.1 iOS sandbox — Application Support, not Documents

The original SUPER_PLAN_2 brief suggested `NSDocumentDirectory`. That's actually **wrong by Apple's own guidance** for SQLite. Apple's File System Programming Guide says:

> SQLite databases should be stored in `Library/Application Support/`.

Reasons (<https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html>):

| Directory | Backed up by iCloud | User-visible | DB suitable? |
|---|---|---|---|
| `Documents/` | yes | yes (file sharing) | Only if the DB *is* user-visible (e.g., notes app exporting `.db`). |
| `Library/Application Support/` | yes | no | **YES — default for app DBs.** |
| `Library/Caches/` | no | no | Only for re-creatable caches. |
| `tmp/` | no | no | Throwaway. |

* **Resolution path on iOS:** Foundation API — `FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first` plus `appendingPathComponent(<bundle-id>)` (Application Support isn't auto-created per app; the docs are explicit about creating a bundle-id subdirectory).
* **iCloud backup opt-out:** mark the file with `URLResourceKey.isExcludedFromBackupKey = true` after creation (or use Caches/). Useful for large local replicas / cached datasets where a redownload is cheaper than the iCloud quota cost.
* **File protection class:** Apple's at-rest encryption is set per-file via `FileManager.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath:)`. `.complete` makes the file inaccessible while the device is locked — *good for privacy, bad if your app needs to read the DB from a background notification handler.* `.completeUntilFirstUserAuthentication` is the usual compromise (encrypted at rest, readable once the user has unlocked once after boot). This is *independent* of libsql/SQLCipher encryption — it's the OS layer.

### 3.2 Android scoped storage — Context.getDatabasePath, not external

`Context.getDatabasePath(name)` returns `/data/data/<package>/databases/<name>`. This is **internal storage**, **per-app sandboxed**, **unaffected by scoped storage** (which only restricts shared media). No `READ_EXTERNAL_STORAGE` / `MANAGE_EXTERNAL_STORAGE` permission needed. Source: <https://developer.android.com/reference/android/content/Context#getDatabasePath(java.lang.String)>.

* **JNI bridge:** the platform backend's `data_dir()` calls into the existing Android JNI scaffolding (the same surface that already proxies `InputConnection` in feature #9). One static method on a `DataDirJni` companion class: `getDatabasesDir(Context) -> String`.
* **Auto-backup considerations:** Android's default auto-backup *includes* `/data/data/<package>/databases/` unless the manifest opts out. If the DB contains secrets, set `android:allowBackup="false"` or supply an `backup_rules.xml` excluding the `databases/` subfolder.
* **Note on encryption:** Android does not provide a per-file protection-class equivalent. The disk is encrypted at rest (FDE / FBE since Android 7), but if the device is unlocked the DB is readable. SQLCipher / libsql native encryption is the right answer if the threat model includes "attacker has unlocked device but isn't root."

### 3.3 macOS / Linux / Windows

* **macOS:** `~/Library/Application Support/<bundle-id>/<name>.db`. `dirs::config_dir()` already returns this — re-use `FilePath::get_config_dir()` from `layout/src/file.rs` (line 515).
* **Linux:** `XDG_DATA_HOME` or `$HOME/.local/share/<app>/<name>.db`. `dirs::data_dir()` already returns this — re-use `FilePath::get_data_dir()` (line 527).
* **Windows:** `%APPDATA%\<app>\<name>.db` (Roaming) or `%LOCALAPPDATA%\<app>\<name>.db` (Local). Use `dirs::config_dir()` for Roaming, `dirs::config_local_dir()` for Local. Decision: **Roaming** for first-class app data (carries across machines), **Local** for caches.

### 3.4 New API: `App::data_dir()` and `App::cache_dir()`

```rust
impl App {
    /// Returns the platform-appropriate per-app data directory.
    /// iOS: Library/Application Support/<bundle-id>/
    /// Android: /data/data/<package>/files/  (NOT databases/ — see open_database)
    /// macOS: ~/Library/Application Support/<bundle-id>/
    /// Linux: ~/.local/share/<app-name>/
    /// Windows: %APPDATA%\<app-name>\
    pub fn data_dir(&self) -> PathBuf { ... }

    /// Returns the platform-appropriate per-app cache directory (eligible for OS eviction).
    pub fn cache_dir(&self) -> PathBuf { ... }

    /// Opens a libsql database. See §2 for URL syntax.
    /// Implicit paths (no scheme) resolve under data_dir().
    /// On Android, paths matching a bare filename resolve via Context.getDatabasePath.
    pub fn open_database(&self, url: &str) -> Result<DbHandle, DbError> { ... }
}
```

`app-name` is taken from `AppConfig::name: AzString` (already exists). On mobile, the per-platform backend overrides the path lookup via a new injection point `inject_platform_data_dir(PathBuf)`, symmetric to `inject_native_gesture` (matches the pattern called out in SUPER_PLAN_2 §0 bullet 5).

---

## 4. SQLCipher / encryption-at-rest

Two layers, often confused:

1. **OS-level disk encryption** (FDE on macOS/Linux/Windows; FBE on Android; full FDE on iOS) — *transparent* when device is unlocked. Doesn't help against an attacker with the unlocked device.
2. **Database-level encryption** (SQLCipher format / libsql AES-256-CBC) — page-level cipher, key derived from a passphrase via PBKDF2. Independent of OS layer.

### 4.1 SQLCipher via `rusqlite` `bundled-sqlcipher`

Standard pattern:

```toml
rusqlite = { version = "0.39", features = ["bundled-sqlcipher", "hooks"] }
```

User code:

```rust
conn.execute("PRAGMA key = ?1", [key])?;
conn.execute("PRAGMA cipher_compatibility = 4", [])?;
```

* Pros: SQLCipher *file format* is widely supported — readable by other SQLCipher clients (iOS, Android via the SQLCipher community-ed SDK). Good for sync between Rust app and a non-Rust mobile companion.
* Cons: ~15% perf hit on writes; PBKDF2 cost on `PRAGMA key` (~300 ms on a mobile CPU, 256k iterations).
* `TODO: verify` — does `bundled-sqlcipher-vendored-openssl` build on iOS Simulator (x86_64) + iOS device (aarch64) cleanly? OpenSSL build scripts often miss one of these targets.

### 4.2 libsql native encryption (AES-256-CBC)

```rust
let db = Builder::new_local("app.db")
    .encryption_config(EncryptionConfig::new(Cipher::Aes256Cbc, key_bytes.into()))
    .build().await?;
```

* Pros: no extra dependency; integrates with the existing libsql code path; works on remote replicas (encrypted local cache).
* Cons: **not file-format compatible with SQLCipher.** A `.db` file produced by libsql encryption is not readable by SQLCipher clients and vice versa.
* `TODO: verify` — confirm libsql's KDF (likely PBKDF2 too, but the source needs a read). Confirm whether `encryption_config` is supported on `Builder::new_remote_replica` paths (the local cache of a remote replica), not just `new_local`.

### 4.3 Key derivation + platform keychain

* **iOS:** store the DB key in Keychain (`kSecClassGenericPassword`), retrieve via Security framework. Existing iOS backend doesn't have a Keychain bridge yet — separate sprint, but the libsql integration should accept a `key: Box<dyn Fn() -> Vec<u8>>` so the keychain fetch can be deferred to the platform layer.
* **Android:** Android Keystore + `MasterKey.Builder(context).setKeyScheme(KeyScheme.AES256_GCM).build()`. Same closure-callback approach.
* **macOS / Linux / Windows:** `keyring-rs` already integrates with macOS Keychain / Secret Service / Windows Credential Vault. Already a candidate dep if needed.

### 4.4 Recommendation

Default: **no encryption**, document the option clearly. The user-typed `App::open_database("app.db")` opens a plaintext DB by default, just like SQLite always has. Encryption is opt-in via:

```rust
App::open_database_encrypted("app.db", DbKey::from_keychain("MyAppKey"))
```

Choice of cipher (libsql native vs SQLCipher) is a Cargo feature select at compile time, not runtime, so the framework doesn't drag in both.

---

## 5. Integration sketch — fitting libsql into azul's architecture

### 5.1 `DbHandle` and `RefAny`

```rust
#[repr(C)]
pub struct DbHandle {
    inner: Arc<DbHandleInner>,  // Arc so cheap to clone into RefAny
}

struct DbHandleInner {
    conn: libsql::Connection,                // Clone + Send + Sync
    rt:   Arc<tokio::runtime::Runtime>,      // owned by the App; one per process
    url:  String,                            // for diagnostics — NEVER includes auth_token
    update_listeners: Mutex<Vec<UpdateHookCallback>>,
}
```

Users hold `DbHandle` inside their `State` struct, which is put behind `RefAny` — same shape as how `ImageRef` and `FontRef` are stored today (see `core/src/refany.rs` line 401 and the `RefCountInner` walkthrough). `DbHandle::clone()` is `Arc::clone`, so propagating it through `f(State) -> Dom` is free.

### 5.2 The tokio runtime — one per `App`, not per query

libsql is async. The framework today is sync at the callback boundary. Two options:

**Option A — single multi-thread tokio runtime owned by `App`** (recommended):

```rust
impl App {
    pub fn new(...) -> Self {
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)             // small, since most callbacks aren't I/O-bound
                .thread_name("azul-db")
                .enable_all()
                .build().unwrap()
        );
        ...
    }
}
```

All libsql calls go through this runtime. The runtime is dropped when `App` drops.

**Option B — current-thread runtime per query** — high overhead, rejected.

The runtime lives next to the other long-lived per-`App` fields (it's not a "manager" in the layout sense — managers are per-window). It's accessible from any callback through `CallbackInfo::get_app_runtime()` (new accessor on `layout/src/callbacks.rs`).

### 5.3 Query patterns — sync, async-via-thread, declarative live query

Three callback-driven patterns, each maps cleanly to existing infrastructure:

#### 5.3.1 Synchronous-looking blocking query (small, fast lookups)

```rust
fn click_callback(info: &mut CallbackInfo, state: &mut RefAny) -> Update {
    let s = state.downcast_mut::<State>()?;
    // Block on the runtime — fine for queries that complete in <16 ms.
    let user = info.get_app_runtime().block_on(async {
        s.db.conn.query_one("SELECT name FROM users WHERE id = ?", [s.user_id]).await
    })?;
    s.username = user.get(0)?;
    Update::RefreshDom
}
```

`block_on` from within a callback running on the main UI thread is *only* acceptable if the query is local and indexed. Document the rule of thumb: ~16 ms is the budget for a single frame at 60 Hz.

#### 5.3.2 Async background query via existing `Thread` manager

The existing `azul-layout` `Thread` infrastructure (`layout/src/thread.rs`, with the `ThreadSendMsg` + `ThreadReceiveMsg` + `WriteBackCallback` pattern from `core/src/task.rs` lines 706+) is already the answer. The pattern:

```rust
fn click_callback(info: &mut CallbackInfo, state: &mut RefAny) -> Update {
    let s = state.downcast_mut::<State>()?;
    let conn = s.db.conn.clone();      // Clone + Send + Sync
    info.spawn_thread(move |sender| {
        // Runs on a background OS thread.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let rows = rt.block_on(async {
            conn.query("SELECT * FROM tasks WHERE completed = 0", ()).await
        });
        // Send the result back to the main thread via WriteBackCallback —
        // the result type is RefAny, the callback mutates user State.
        sender.send_writeback(refany_for(rows), apply_tasks_to_state);
    });
    Update::DoNothing
}

fn apply_tasks_to_state(state: &mut RefAny, rows: &mut RefAny) -> Update {
    let s = state.downcast_mut::<State>()?;
    let r = rows.downcast_mut::<Vec<Task>>()?;
    s.tasks = r.clone();
    Update::RefreshDom
}
```

Per the existing pattern, `WriteBackCallback` runs *on the main thread* between frames, then `RefreshDom` re-runs `layout()`, picks up the new state, and renders. The reactive pipeline handles everything else.

Note: spawning a fresh tokio runtime per thread is wasteful; the framework should expose `info.get_app_runtime().spawn(future)` plus a writeback mechanism. New API (`layout/src/callbacks.rs`):

```rust
impl<'a> CallbackInfo<'a> {
    /// Spawn an async query on the App's tokio runtime; on completion,
    /// the WriteBackCallback is enqueued for the next main-thread tick.
    pub fn spawn_db_query<F, T>(
        &mut self,
        future: F,
        writeback: WriteBackCallback,
    ) where
        F: Future<Output = T> + Send + 'static,
        T: Into<RefAny>,
    { ... }
}
```

This is the right idiom for "click → query → re-render". Document it as the *default* path.

#### 5.3.3 Live query via SQLite update hook (advanced)

SQLite (and libsql) expose `sqlite3_update_hook` — a callback invoked on insert/update/delete of any rowid table on a connection. libsql wraps it as `Connection::add_update_hook(callback)` (`TODO: verify` exact method name — docs.rs hint says `add_update_hook`).

The hook fires *synchronously inside the SQL operation*, with the following constraints (per <https://www.sqlite.org/c3ref/update_hook.html>):

* "The update hook is not invoked when internal system tables are modified (e.g., `sqlite_sequence`)."
* "The update hook implementation must not do anything that will modify the database connection that invoked it." → can't query from the hook.
* Timing (before/after the change) is unspecified.

In azul terms, the update hook lives on a background thread (libsql's tokio runtime), and the callback needs to *post* a `RefreshDom` signal to the main thread without doing any libsql work itself. Proposed mechanism:

```rust
db.subscribe_table_changes("tasks", |change| {
    // Runs on the libsql worker thread. Must not query the connection.
    // Just post a message to the main loop.
    change.post_to_main(MainThreadMsg::TableChanged("tasks"));
});
```

On the main thread, the `App::run` event loop drains pending `MainThreadMsg`s once per frame and emits a synthetic `EventFilter::DatabaseTableChanged("tasks")`. Callbacks attached to that filter then run a fresh query and update state.

This is feasible but real work — implementation is a follow-up sprint, not the first sprint.

### 5.4 `NodeType::Database(DbHandle)` — declarative auto-connect

SUPER_PLAN_2 §1.5 mentions an invisible `NodeType::Database` node. The idea: a component like

```rust
Database::with_url("libsql://my-app.turso.io?authToken=...")
    .on_ready(load_cached_state)
    .on_error(show_offline_banner)
    .dom()
```

opens the connection at the moment the node first appears in the styled DOM, closes it when the node leaves. The lifecycle is exactly what `IFrameManager` already does for lazy DOM subtrees (`layout/src/managers/iframe.rs`).

**Pro:** composes with the reactive pipeline. The mere presence of a `<Database>` node in the tree, derived from `f(State)`, *is* the lifetime. No imperative open/close needed.

**Con:** harder to share one connection across many subtrees. Most apps want exactly one DB connection for their lifetime, opened once at startup, available as `state.db`. The declarative form is overkill — and worse, the user has to manage how to *pass* the `DbHandle` to descendants (probably via the `DbReady` callback parameter writing it into `RefAny`).

**Recommendation:** ship **both**, mark the declarative form as advanced.

* Default path (covers ≥95% of apps): `state.db = App::open_database("app.db")?` at startup.
* Declarative `Database` node: useful for "the user opens a settings dialog that briefly connects to a sync DB". Use sparingly.

### 5.5 New EventFilter variants

Under `HoverEventFilter` (so they fire on the bearing node, consistent with the permission-aware DOM pattern in §1.5 of the parent plan):

```rust
// On core/src/events.rs, append:
DatabaseReady,                 // emitted once after open_database succeeds
DatabaseError(DbErrorVariant), // open / migration / connectivity failure
DatabaseQueryComplete(QueryId), // for spawn_db_query — the WriteBack landed
DatabaseTableChanged(AzString),// from §5.3.3 update-hook
DatabaseReplicationSynced,     // remote-replica mode only — primary caught up
```

These ride alongside the gesture / permission / sensor variants. The doc/api.json + codegen step (35 binding languages) picks them up automatically (parent SUPER_PLAN_2 §2 bullet 7).

### 5.6 Errors and never logging auth tokens

```rust
#[repr(C, u8)]
pub enum DbError {
    InvalidUrl(AzString),         // URL parse error — NEVER includes the original URL
    ConnectionRefused(AzString),  // host unreachable — host name only, no token
    AuthFailed,                   // 401 from remote — no further info
    SqliteError(i32, AzString),   // (code, message) — message scrubbed
    IoError(AzString),            // local file IO — path safe to include
    Unsupported(AzString),        // e.g., encryption when feature is off
    Timeout,
}
```

Two safety rails:

1. `DbHandle::url` stores the URL *with the auth_token stripped* — anything that includes the URL in an error / log / debug print is safe by construction.
2. The `Debug` impl on `DbHandle` redacts both the connection string and any in-memory copy of the token (replace with `<redacted>`).

### 5.7 A "live query" pattern — feasible but careful

Summary of §5.3.3:

* libsql supports `add_update_hook` (verify name).
* Update hook fires per-row on insert/update/delete; can't query from inside.
* The hook → main-thread bridge is a single MPSC channel drained per frame.
* The signal is *coarse* — "table X changed" — not "row Y changed". Users still have to run a query in their callback.
* For real-time push (e.g., a chat list updating from a sync server), libsql also fires hooks on rows updated by replication, not just local writes. That's the killer feature.

`TODO: verify` the channel-once-per-frame model is fast enough for a chat app with ~10 incoming msgs/sec; the SQLite hook fires synchronously inside `step()`, so backpressure on the channel would block writes.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| **libsql API churn** — 0.x series, 54 releases. | Wrap libsql behind `azul::db::Connection` trait so the impl is swappable. Pin to `libsql = "=0.9.30"` in `Cargo.toml`, not `^`, until 1.0. |
| **Auth-token leakage in logs / panics** | Strip token from URL before storage; redact in `Debug`. Test: `panic!("{:?}", db_handle)` must not contain the token. |
| **Long-running query blocks the main loop** | Document "16 ms rule." Default to async via `spawn_db_query`. Provide a debug-build `App::set_query_warning_threshold(Duration)` that emits a `LayoutDebugMessage` if any blocking query exceeds the threshold. |
| **SQLite write lock contention** | A second writer waiting on the WAL lock can stall callbacks indefinitely. Default to one writer connection; allow N reader connections via `Builder::new_local(path).with_pool(readers: 4, writers: 1)`. `TODO: verify` libsql exposes a connection-pool primitive — if not, build it. |
| **Tokio runtime added to mobile binary size** | Cargo feature `db-libsql` (default) vs `db-rusqlite` (local-only, no tokio). Document the size delta. |
| **iOS Application Support directory not auto-created** | Detect ENOENT, create the bundle-id subdir with permissions 0700 on first open. |
| **Android `allowBackup=true` exposes DB to user backups** | Document; ship a sample `backup_rules.xml` in the Android backend templates. |
| **SQLCipher OpenSSL build on iOS Simulator** | Use `bundled-sqlcipher-vendored-openssl` carefully; verify universal binary builds. May need a custom build script in `build-ios.sh`. |
| **Update-hook → frame coupling** | Coalesce table-change notifications per-frame: if 50 inserts fire in one tick, the main loop emits *one* `DatabaseTableChanged("tasks")`, not 50. |
| **Migration ordering across embedded replicas** | A user runs migration v3 on the local replica before the primary has v3 — replica diverges. Cap embedded-replica writes to apps where the user runs migrations server-side first; document. |

---

## 7. Web/W3C-compatible primitive (future web backend)

When azul gets a WebAssembly backend, the natural mapping is:

* **`:memory:`** → `sql.js` (SQLite compiled to wasm). No file system.
* **`file:`** → Origin-Private File System (OPFS) + `sql.js`. The browser provides per-origin sandboxed storage; OPFS file handles map cleanly to SQLite VFS.
* **`libsql://`** → fetch the Hrana HTTP endpoint directly. The `libsql-client-ts` npm package (`@libsql/client`) already does this; same wire protocol. No native deps.

The `App::open_database(url)` API maps unchanged. The web backend ships a different `DbHandle` implementation that delegates to `@libsql/client` over WASM-JS interop, or uses `sql.js` for local modes. This is consistent with the SUPER_PLAN_2 §0 web-shape-compatible rule.

---

## 8. Concrete artifacts for the implementation sprint

Following the SUPER_PLAN_2 §2 artifact table:

| Artifact | Where |
|---|---|
| Manager — connection pool, runtime, update-hook fanout | `layout/src/managers/database.rs` (new) |
| `DbHandle` + `DbError` + `EncryptionConfig` types | `core/src/database.rs` (new) |
| EventFilter variants (§5.5) | `core/src/events.rs` |
| `NodeType::Database(DbHandle)` (optional declarative) | `core/src/dom.rs` |
| CallbackInfo accessors (`get_app_runtime`, `spawn_db_query`, `subscribe_table_changes`) | `layout/src/callbacks.rs` |
| Platform `data_dir()` overrides | `dll/src/desktop/shell2/<platform>/mod.rs::inject_platform_data_dir` |
| `App::open_database`, `App::data_dir`, `App::cache_dir` | `dll/src/desktop/app.rs` |
| `Cargo.toml` features `db-libsql` (default), `db-rusqlite`, `db-sqlcipher` | `dll/Cargo.toml` |
| api.json entries + codegen | `azul-doc autofix add App.open_database` + `codegen all` |
| Sample app | `examples/rust/todo-app/` showing `open_database`, `spawn_db_query`, `DatabaseTableChanged` |

Estimated effort: **2 sprints**. Sprint 1 covers modes #1–#2 (in-memory + local file), sync block-on + async `spawn_db_query`, no encryption, no update hooks. Sprint 2 adds remote mode, encryption (one cipher only), and update-hook → table-changed events.

---

## 9. Citations summary

* libsql crate docs: <https://docs.rs/libsql/latest/libsql/>, <https://docs.rs/libsql/0.9.30/libsql/struct.Builder.html>, <https://docs.rs/libsql/0.9.30/libsql/struct.Connection.html>, <https://docs.rs/libsql/0.9.30/libsql/enum.Cipher.html>
* libsql repo: <https://github.com/tursodatabase/libsql>
* turso crate (new direction): <https://docs.rs/turso/latest/turso/>, <https://github.com/tursodatabase/turso>
* Hrana protocol spec: <https://github.com/tursodatabase/libsql/blob/main/docs/HRANA_3_SPEC.md>
* Turso Rust quickstart: <https://docs.turso.tech/sdk/rust/quickstart>
* libsql TS client URL schemes: <https://docs.turso.tech/sdk/ts/reference>
* rusqlite 0.39: <https://docs.rs/rusqlite/latest/rusqlite/>, features at <https://docs.rs/crate/rusqlite/0.39.0/features>
* sqlx: <https://github.com/launchbadge/sqlx>
* sqlite3_update_hook: <https://www.sqlite.org/c3ref/update_hook.html>
* iOS sandbox: <https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html>
* Android Context.getDatabasePath: <https://developer.android.com/reference/android/content/Context#getDatabasePath(java.lang.String)>
* In-repo: `core/src/refany.rs` (RefAny model), `core/src/task.rs` (Thread / Timer / ThreadSendMsg), `layout/src/thread.rs` (ThreadReceiveMsg / WriteBackCallback), `layout/src/file.rs` line 527 (dirs::data_dir already wired), `dll/src/desktop/shell2/headless/mod.rs` lines 998–1049 (Timer / Thread management pattern), `SUPER_PLAN_2.md` §1 feature 13 and §1.5 permission-aware DOM nodes.

`TODO: verify` markers in this document — exact method name `Connection::add_update_hook` vs `set_update_hook` (docs hint says `add_`, but the rusqlite-trained intuition wants `set_`); libsql encryption KDF specifics; libsql connection-pool primitive existence; `bundled-sqlcipher-vendored-openssl` building cleanly on iOS Simulator x86_64 + device aarch64; rusqlite 0.39 exact release date; refinery driver-trait support for libsql; default `allowBackup` flag for Android databases dir in current API levels.

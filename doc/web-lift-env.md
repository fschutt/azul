# Web-lift environment variables

Every knob the x86→wasm lift pipeline reads is parsed once, in one place —
`dll/src/web/lift_env.rs`. This document is the human-facing companion to that
struct. If a variable is not a field of `LiftEnv`, it does not exist.

Almost everything here has a working default. In practice you set **`AZ_BACKEND`**
and, for the base image, **`AZ_LIFT_MODE=full`**. The rest is override territory.

---

## The one knob: `AZ_LIFT_MODE`

Selects a profile. Everything in the "fine-grained overrides" section below
refines whatever the mode chose.

| value | what it does | who uses it |
|---|---|---|
| `app` *(default)* | Serve normally. Lift only the functions **this app** reaches. Trust the cache — no re-verification. | every deployed app |
| `full` | Lift the **whole api.json export surface** into the cache, then exit before serving. This is what warms the base image so a derived app finds the library pre-lifted. Equivalent to the older `web-prelift://` URL scheme. | the GHCR base image build |
| `verify` | App closure **plus** fresh-lift every cache hit and byte-compare it. The correctness gate. Slow; for CI and debugging a suspected cache bug. | CI correctness job |

Verify is **off by default**: the relocation cache's correctness is established,
so a normal run reuses cached lifts instead of re-lifting every hit. Only
`AZ_LIFT_MODE=verify` (or the raw `AZ_RELOC_VERIFY=1` override) turns it back on.

---

## Operational (set these deliberately)

| var | default | meaning |
|---|---|---|
| `AZ_BACKEND` | — | Backend URL, e.g. `web://0.0.0.0:8080?allow_public=1`. The `web-prelift://` scheme is a legacy alias for `AZ_LIFT_MODE=full`. |
| `AZ_LIFT_CACHE_DIR` | temp dir | Where the on-disk lift cache lives. Point it at a baked path (`/cache`) for the image. |
| `AZ_LIFT_CACHE` | on in image builds | Enable the cross-run cache. |
| `AZ_NO_LIFT_CACHE` | unset | Force the cache off. |
| `AZ_LIFT_JOBS` | cores − 2, clamped 1..10 | Object-production worker-pool size. |
| `AZ_LIFT_BATCH` | 64 | Functions per remill `--batch_manifest` wave. `1` disables batching. |
| `AZ_LIFT_STRICT` | **on** | Refuse to serve a bundle whose lift audit found fatal problems. `0` downgrades to warnings — do not do this in production. |
| `AZ_UNK_TRAP` | off | Turn an unmatched indirect dispatch into a trap so the stack names the caller, instead of the call silently vanishing. |
| `AZ_MINI_MAX_DEPTH`, `AZ_CB_MAX_DEPTH` | build-set | Transitive-lift depth caps for the mini / callback closures. |
| `AZ_SPAWN_WATCHDOG_SECS` | 300 | Abort with a diagnosis if a subprocess spawn wedges (a full-disk AV stall can do this). `0` disables. |
| `AZ_TOOL_TIMEOUT_SECS` | 900 | Deadline for a single tool invocation, including its pipe reads. |

## Toolchain overrides (paths)

`REMILL_LIFT_BIN`, `LLC`, `LLVM_OPT`, `LLVM_LINK`, `WASM_LD`, `WASM_OPT` — point
each at a specific binary. Unset, the pipeline discovers them (on Windows, from
the remill superbuild install dir). The **content** of remill-lift, llc and opt
is hashed into every cache key, so upgrading any of them invalidates the cache
correctly — and a byte-identical rebuild (e.g. fresh CI mtimes) does **not**.

## Correctness / transform toggles (rarely touched)

Each disables or forces a default-on transform, folded into the object cache
key so a flipped switch never serves a stale object. Reach for these only to
A/B a suspected miscompile against the transform:
`AZ_RELOC_VERIFY`, `AZ_NO_FIX_SP`, `AZ_NO_TRAP_SELFLOOP`, `AZ_NO_INDIRECT_DISPATCH`,
`AZ_FULL_CS_RESTORE`, `AZ_KEEP_ALIAS_SCOPE`, `AZ_NO_HOST_SCOPE`.

## Diagnostics (off unless you are hunting a specific bug)

In-wasm recorders and tracers — never on in a shipped bundle, all excluded from
the cache key so an instrumented object is never reused for a clean build:
`AZ_WRITE_TRACE`, `AZ_READ_TRACE`, `AZ_REG_TRACE`, `AZ_REG_TRACE_NOWRAP`,
`AZ_SP_TRACE`, `AZ_LOG_STORES`, `AZ_LOG_SELFLOOP_VAL`, `AZ_LSWIN_LO`,
`AZ_LSWIN_HI`, `AZ_LSID_LO`, `AZ_FUEL`, `AZ_FUEL_LIMIT`, `AZ_TAG_UNREACHABLE`,
`AZ_TRACE_STALE_PTR`, `AZ_WASM_MIRROR_TRACE`, `AZ_PREFLIGHT`.

The opt-bisect rig (pins which LLVM pass miscompiles one function):
`AZ_OPT_LEVEL`, `AZ_LOWOPT_FNS`, `AZ_BISECT_FN`, `AZ_BISECT_LIMIT`,
`AZ_LTO_LEVEL`, `AZ_WASM_LD_MLLVM`.

Post-mortem / scratch: `AZ_REMILL_KEEP_SCRATCH`, `AZ_REMILL_DEBUG`,
`AZ_WASM_DEBUG`, `AZ_REMILL_SKIP_WASM_OPT`.

## Experimental / niche

`AZ_NATIVE_REMILL` (in-process lifting instead of subprocesses — not yet
feature-equivalent; see the pipeline notes), `AZ_ENABLE_SHARDS` (per-fn boundary
wasm shards), `AZ_REMILL_MERGED_COMPILE` / `AZ_REMILL_DISABLE_AUTO_MERGE`
(merged-module opt — miscompiles at large dep counts, kept small on purpose).

---

**Startup prints the non-default settings** as one line
(`[azul-web] lift env: mode=full AZ_LIFT_CACHE ...`) plus the engine
fingerprint, so a CI log records exactly what produced a bundle.

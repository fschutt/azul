# Supply-chain gates

Four gates, three questions. Each answers something the others structurally cannot.

| gate | question | keyed by |
|---|---|---|
| `scripts/dependency-justifications.toml` (pre-existing) | why is this crate here at all? | crate **name** |
| `scan_build_scripts.py` + `build-script-policy.toml` | may this crate run code at build time, and is it the same code we read? | **content digest** |
| `env_guard.py` | which environment variables may build-time code read? | variable **name** |
| `lockfile_guard.py` | may this **version** enter the tree? | name + **version** |
| `cargo vet` (`supply-chain/`) | has this exact version been audited? | name + **version** |

## Why the name-keyed gate is not enough

`dependency-justifications.toml` requires a written reason for every crate in the
tree, which stops a dependency being added quietly. It is keyed by crate *name*,
so a crate that is already justified stays justified when a compromised
maintainer account publishes a new version of it.

That is not hypothetical. On 2026-08-20 `arrayref`, `internment` and
`append-only-vec` — all long-established, all already in thousands of dependency
trees — had malicious releases published from a compromised account inside a
23-minute window. The only change was one line of `Cargo.toml` adding
`proc-macro1`, a typosquat of `proc-macro2`, whose `build.rs` downloaded and ran
a remote payload. No code in the compromised crates had to be called. Exposure
was 86–107 minutes, and the attacker yanked the good releases to force the
resolver onto the bad ones.

A name-keyed allowlist says yes to every one of those releases. The three gates
here are the ones that say no.

## The gates

### `scan_build_scripts.py` — build-time code execution

`build.rs` runs during `cargo build` with the full authority of whoever typed
the command; proc macros run inside rustc with the same authority and less
visibility. This gate finds every crate that executes code at build time and
requires a policy entry carrying `allow`, an acknowledged `risk`, a written
`reason`, and `reviewed` — a list of `<version>:<sha256>` pins over the actual
build-time files.

The digest is the part the justifications file cannot do. It covers the build
script, a sibling `build/` directory, and the transitive closure of everything
pulled in with `mod` / `#[path]` / `include!` — 16 of azul's 141 build scripts
span more than one file, so hashing `build.rs` alone would let a payload move one
file sideways and keep the pin green. A version bump changes the digest, fails
the build, and forces someone to read the diff.

It also scans build-time code against a table of behaviours that separate a real
build script from an exfiltration stub (`--print-rules`). The table is applied at
full strength to build scripts and proc macros, and reduced to the exfiltration
subset for build *dependencies* — ordinary libraries linked into build scripts,
where rules written for a 200-line `build.rs` produce nothing but noise against
50k lines of FFI declarations.

### `env_guard.py` — environment access

Credential theft through the build is the most productive thing a poisoned
dependency can do, because cargo hands every build script the runner's full
environment. This gate scans build-time code for `env!`, `option_env!`,
`env::var`, `env::var_os`, `env::vars` (whole-environment enumeration), `getenv`
in C sources, and `GetEnvironmentVariable`.

The forbidden-name set does not come from `os.environ` — a secret only exists in
the environment of the job it is wired into, and this gate deliberately runs in a
job that has none. It is parsed out of `.github/workflows/*.yml`: every
`secrets.NAME` the repository can inject, checked for whether or not this run has
it.

### `lockfile_guard.py` — version integrity

Four checks: every vendored file matches the registry checksum cargo recorded
(offline); no locked version is yanked; lockfile checksums match the index; and
no version is younger than `--min-age-days` (default 14).

The cooldown is RFC 3923 `min-publish-age`, implemented here because the
client-side half had not shipped when the arrayref attack happened — two days
after the stabilisation PR entered its final comment period. A 14-day floor turns
a 107-minute exposure window into a non-event. First-party crates are exempt; see
`cooldown-exempt.txt` for why that exemption is necessary rather than convenient.

### `cargo vet` — per-version audits

`supply-chain/config.toml` records an audit or an exemption for every version in
the tree, and imports the shared audit sets published by Mozilla, Google, the
Bytecode Alliance, Embark, ISRG and Zcash. A dependency bump lands as an
unaudited version and fails until someone reviews the delta.

## Running them

```sh
scripts/supply-chain/run_all.sh                  # everything CI blocks on
scripts/supply-chain/run_all.sh --with-cooldown  # + publish-age (slow: one API call per crate)
scripts/supply-chain/run_all.sh --keep-vendor    # leave vendor/ for inspection
```

Individually — each takes `--vendor DIR` and most take `--report-only`:

```sh
cargo vendor --locked --versioned-dirs vendor
python3 scripts/supply-chain/scan_build_scripts.py --vendor vendor
python3 scripts/supply-chain/env_guard.py          --vendor vendor
python3 scripts/supply-chain/lockfile_guard.py     --vendor vendor --check all
python3 scripts/supply-chain/scan_build_scripts.py --print-rules   # the risk table
python3 scripts/supply-chain/env_guard.py          --print-classes # the name classes
```

## When a gate fails

**"UNREVIEWED — no entry in build-script-policy.toml"** — a new crate runs code
at build time. Read its build script, then `--update` to write the skeleton and
fill in the `reason` by hand.

**"DIGEST — version X is not reviewed"** — a crate you already allow shipped new
build-time code. **Read the diff before re-pinning.** This is the account-takeover
signal; `--update` makes re-pinning cheap precisely so that the review, not the
mechanics, is the work.

**"forbidden read"** — build-time code reads a credential-shaped variable or one
of this repository's secrets. There is no correct way to allow this; the crate is
either compromised or must be dropped.

**cargo-vet failure** — `cargo vet diff <crate> <old> <new>`, then
`cargo vet certify` if it is clean, or `cargo vet add-exemption` to accept it
without review.

**COOLDOWN** — a dependency is younger than the floor. Wait, or, if it is genuinely
urgent, pass `--min-age-days` on that one run and say why in the commit message
so the exception is visible instead of permanent.

## In CI

`supply_chain_preflight` (in `.github/workflows/rust.yml`) has no `needs:` and
every job that compiles anything is downstream of it — including `clippy` and
`check_crates`, because `cargo check` runs build scripts too. `cargo vendor`
downloads sources and executes nothing, which is what makes a gate that runs
*before* the build possible at all. `supply_chain_versions` runs cargo-vet and
the cooldown; `supply_chain` runs cargo-deny and cargo-audit.

## Notes

The policy files are written in a small TOML subset parsed by `sc_common.py`
rather than `tomllib`, which is Python 3.11+ — the ubuntu-22.04 runner ships 3.10
and stock macOS ships 3.9, so a `tomllib` import in a CI gate is a gate that does
not run. The files are still valid TOML; the subset constrains what these scripts
*write*, not what is legal.

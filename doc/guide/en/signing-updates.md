---
slug: signing-updates
title: Shipping Signed Updates
language: en
canonical_slug: signing-updates
audience: external
maturity: wip
guide_order: 450
topic_only: false
short_desc: The update manifest, the signing chain, and the release script
prerequisites: [hello-world, security]
tracked_files:
  - layout/src/updater.rs
  - layout/src/dialogs/update_version.rs
  - layout/examples/verify_update_manifest.rs
  - scripts/sign-release.sh
last_generated_rev: 87d8e2882b7e7f84645a106a79d2fd5beacf256e
generated_at: 2026-08-18T22:28:46Z
default-search-keys:
  - AppConfig
  - UpdateSettings
  - UpdateMode
  - CallbackInfo
  - SysDialogType
---

# Shipping Signed Updates

Azul apps can check for, download and install their own updates. This page
covers the release side: the manifest you publish, the signature chain that
makes it trustworthy, and the script that produces both.

If you only want the in-app side — `CallbackInfo::check_for_updates`, the
`SysDialogType::UpdateVersion` dialog, staged rollouts — those read
`AppConfig.updates` and need no release tooling beyond a manifest URL.

## The shape of it

Your app is built with an `UpdateSettings`:

```rust
config.updates.manifest_url =
    Some("https://downloads.example.com/updates.json".into()).into();
config.updates.current_version = env!("CARGO_PKG_VERSION").into();
config.updates.app_name = "myapp".into();
config.updates.root_public_key = "RWQ…".into();   // see below
```

and you publish a manifest at that URL:

```json
{
  "latest": {
    "version": "2.0.0",
    "download_url": "https://downloads.example.com/myapp-2.0.0.bin",
    "changelog_md": "https://downloads.example.com/CHANGELOG.md",
    "digest": "sha256:49dba348…",
    "signature": "untrusted comment: …\nRUThI3d5…\ntrusted comment: …\n…\n",
    "signing_key_statement": "azul-signing-key-v1|pubkey=RWThI3d5…|expires=1818627834|generation=1",
    "signing_key_statement_sig": "untrusted comment: …\n…\n",
    "release_date": "2026-08-18T12:00:00Z",
    "slow": { "10": "…", "50": "…" }
  }
}
```

`release_date` and `slow` drive [staged rollouts](#staged-rollout) and are
optional. Everything else about delivery — where you host it, whether it is
S3 or a static file — is up to you; the client only needs to be able to GET
those two URLs.

## Why there are two keys

A single signing key is a bad trade: it has to live wherever your CI signs
builds, and if it leaks your only remedy is shipping a new binary to every
user, because the key they trust is compiled into the app they already have.

So the client trusts a **root key** that signs nothing but *statements*
about which signing key is currently valid:

```
root key ──signs──> signing-key statement ──names──> signing key ──signs──> artifact
```

The statement is a single line with no trailing newline:

```
azul-signing-key-v1|pubkey=<base64>|expires=<unix>|generation=<n>
```

* The **root secret key** lives offline and signs a statement about once a
  year. Its *public* half is compiled into your app.
* The **signing key** lives on the build machine and signs artifacts.
* `generation` is a rotation counter. Clients remember the highest one they
  have accepted and refuse anything lower, so a retired key stays retired
  even if someone replays an old statement. Rotating is: mint a new signing
  key, publish a statement with `generation` one higher. No new binary.
* `expires` bounds the damage of a leak you never noticed.

A manifest cannot name its own signing key — only a root-signed statement
can. That is the whole point: an attacker who controls your download server
and your manifest still cannot make a client install anything, because they
cannot produce a statement the root key signed.

Leaving `root_public_key` empty disables the chain and leaves only the
`digest` pin, which protects against corruption and a swapped file but not
against someone who can rewrite the manifest.

## Signing a release

```bash
# First run: mint the two key pairs, then STOP so you can put the root
# secret key somewhere safe before it has ever signed anything.
scripts/sign-release.sh --keys ./release-keys

# Then, per release:
scripts/sign-release.sh \
    --keys ./release-keys \
    --artifact ./target/release/myapp \
    --version 2.0.0 \
    --url https://downloads.example.com/myapp-2.0.0.bin \
    --changelog https://downloads.example.com/CHANGELOG.md \
    --out manifest.json
```

The script needs a minisign implementation. Either works:

```bash
cargo install rsign2          # recommended
apt install minisign          # or: brew install minisign
```

Before printing anything the script **verifies its own output with azul's
real client-side code**. Do not remove that step. A signature can be
perfectly valid to the tool that made it and still be refused by the app:

* **Prehashing.** The client accepts only prehashed signatures (`ED`), not
  the legacy form (`Ed`). `rsign2` always prehashes; some `minisign` builds
  need `-H`, which the script passes.
* **The statement is signed as exact bytes.** `echo` appends a newline and
  the signature then covers a string that is not the one in the manifest.
  The script uses `printf '%s'`.

Neither mistake is visible by inspection, and both produce a release that
looks fine until every client rejects it.

## Verifying before you publish

The same check runs standalone:

```bash
cargo run -p azul-layout --features updater --example verify_update_manifest -- \
    manifest.json ./myapp-2.0.0.bin RWQ…rootpubkey
```

It prints the resolved signing key, the generation, when the statement
expires and whether the artifact matches — and exits non-zero if a client
would refuse the release.

`--selftest` mints a throwaway hierarchy and walks the whole chain, which is
a quick way to see the byte formats without touching real keys:

```bash
cargo run -p azul-layout --features updater --example verify_update_manifest -- --selftest
```

## Testing the whole path locally

Serve the manifest and the artifact from a directory and point a drill at
them. This exercises exactly what a client does — check, changelog fetch,
resumable download, digest, signature chain:

```bash
cd release-dir && python3 -m http.server 8731 &

cargo run -p azul-layout --features updater,telemetry \
    --example telemetry_grafana -- \
    --update-manifest http://127.0.0.1:8731/manifest.json \
    --version 1.0.0 \
    --update-root-key RWQ…rootpubkey
```

```text
update: install=UserWritable effective_mode=SelfUpdate
update: 1.0.0 -> 2.0.0 available (manual mode)
update: staged AND VERIFIED …/staging/myapp-2.0.0.bin (4096 bytes, cached=false)
        — signature chain OK, key generation now 1
```

Then break it on purpose, which is the half worth running:

* Replace the artifact on the server after signing → refused, `digest
  mismatch`.
* Replace it *and* recompute the digest in the manifest → refused, `artifact
  signature invalid`, and the staged file is deleted rather than left behind.

## Install kinds and what the client will actually do

Verification is necessary but not sufficient: azul also refuses to
self-update where self-updating is wrong. `InstallKind::detect()` recognises
package-managed installs (dpkg/rpm, Homebrew, Flatpak/Snap, the Windows
Store, macOS `/Applications` bundles installed by a manager) and clamps the
mode to notify-only, so the app tells the user to update through the
mechanism that owns the files instead of overwriting them behind its back.

The machine-wide config can clamp further: `updates.autoupdate: false` in
`{config_dir}/azul/config.json` turns self-updating off for every azul app
on the machine, and `maintenance_window` (an RRULE subset) confines
unattended staging to a time window the machine's owner chose.

## Staged rollout

`release_date` plus an optional `slow` map spread a release over days
instead of shipping it to everyone at once:

```json
"release_date": "2026-08-18T12:00:00Z",
"slow": { "10": "2026-08-19T12:00:00Z", "50": "2026-08-20T12:00:00Z" }
```

Each client draws a persistent cohort bucket (0-99) once and keeps it
forever, so it stays in the same cohort for every release. Auto-updaters
open stage by stage; notify-only installs stay silent until the rollout
reaches 100 %. Clients still inside the gate report
`app_update_check_total{result="staggered"}`, so the reach of a rollout is a
dashboard query rather than a guess.

Without a `slow` map a default ladder applies (1 day → 10 %, 2 → 30 %,
3 → 50 %, 4 → 100 %). `"slow": "off"` ships to everyone immediately.

## Key hygiene, briefly

* The root secret key never touches CI. If it leaks, an attacker can appoint
  their own signing key and there is no in-band recovery — that is why it
  signs once a year and lives offline.
* The signing key on the build machine is the one you expect to rotate.
  Practise the rotation *before* you need it: bump `--generation`, publish,
  confirm clients accept the new statement.
* Keep the statement's `expires` shorter than the interval at which you are
  confident you would notice a compromise.

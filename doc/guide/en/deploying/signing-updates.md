---
slug: deploying/signing-updates
title: Signed Updates
language: en
canonical_slug: deploying/signing-updates
audience: external
maturity: wip
guide_order: 232
topic_only: false
short_desc: The update manifest, the signing chain, and the release script
prerequisites: [hello-world]
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

# Signed Updates

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
    "release_statement": "azul-release-v1|app=myapp|version=2.0.0|platform=linux-x86_64|sha256=49dba348…|released=1787054400|min_generation=1",
    "release_statement_sig": "untrusted comment: …\n…\n",
    "release_date": "2026-08-18T12:00:00Z",
    "slow": { "10": "…", "50": "…" }
  }
}
```

`release_date` and `slow` drive [staged rollouts](#staged-rollout) and are
optional. Everything else about delivery — where you host it, whether it is
S3 or a static file — is up to you; the client only needs to be able to GET
those two URLs.

`version`, `download_url` and `release_date` are the manifest's own words and
nobody signs them; what a client trusts is the **release statement**: a line
the signing key signed that says which app, which version, which platform and
which bytes (the SHA-256) this release is. A client installs a release only
when the statement names the app it is, the platform it runs on, the version
the manifest says and the hash of what it downloaded. So an attacker who can
rewrite the manifest still cannot serve a genuine old release under a new
version label, or one platform's build to another.

## You do not have to publish that manifest

`manifest_url` is one URL, but the updater does not insist on one format.
It fetches whatever is there and works out what it got, so the smallest
useful deployment is a text file and the largest is the manifest above.

| What is at the URL | What you get |
| --- | --- |
| The manifest above | Everything: rollout, changelog, signature chain |
| A flat object `{"version": "2.0.0", "url": "…"}` | Version + download; same optional fields, no `latest` wrapper |
| A GitHub release | Version, asset, changelog, digest, signatures — see below |
| An OCI registry (`oci://…`) | Version, layer blob, and a digest pin for free |
| A bare version number, e.g. a `VERSION` file | Notification only: "there is a 2.0.0", with no download |

The lenience is about SHAPE, never about verification. Whatever the source,
the digest and the signature chain are checked identically, and a body that
matches none of these shapes is an error — an HTML error page from a
misconfigured host must never look like "you are up to date".

### GitHub releases

Point `manifest_url` at the repository. All of these mean the same thing:

```text
github://owner/repo
https://github.com/owner/repo
https://github.com/owner/repo/releases/latest
https://api.github.com/repos/owner/repo/releases/latest
```

The updater then maps a release onto a `ReleaseInfo`:

* `tag_name` is the version, with a leading `v` stripped.
* The release **body** is the changelog, inline — no second request, and it
  still works if the release page is unreachable.
* `published_at` seeds the [rollout ladder](#staged-rollout), so staged
  rollout works with no extra fields at all.
* The **asset** is chosen by matching this build's OS and architecture
  against the asset names (`…-x86_64-unknown-linux-musl.tar.gz`,
  `…_windows_amd64.zip`, `…-aarch64-apple-darwin.tar.gz` all work). Pin it
  explicitly with `github://owner/repo?asset=myapp-{version}-linux.bin` —
  `*` globs are allowed. If nothing matches this platform the updater
  reports the new version but **refuses to nominate a download**: handing
  someone an arbitrary binary is worse than telling them to fetch it.
  The updater installs the artifact exactly as it is and unpacks nothing, so
  an app that updates itself needs the bare executable as its asset: an
  archive (`.tar.gz`, `.zip`, ...) is recognised by its first bytes and
  refused at install time instead of replacing the program with the archive.
* The **digest** comes from the asset's own `digest` field where GitHub
  provides one, and otherwise from a sibling `myapp.bin.sha256` or
  `SHA256SUMS` asset, matched by filename.
* The **signature chain** rides as sibling assets:
  `myapp.bin.minisig`, `signing-key-statement.txt`,
  `signing-key-statement.txt.minisig`, and the release statement of that asset,
  `myapp.bin.release-statement.txt` with its `.minisig` (a release with a
  single artifact may use the shared names `release-statement.txt[.minisig]`;
  a release with an asset per platform needs one statement per asset, because
  a statement names one platform and one hash). `scripts/sign-release.sh
  release … --sidecars DIR` writes exactly these files: upload them next to
  the artifact and the chain verifies as it does from a manifest.

Draft and pre-release entries are skipped. An unauthenticated client makes
one or two API calls per check, well inside GitHub's rate limit for a
per-user application.

A GitHub release with no signature assets is *unsigned*, and an unsigned
release is refused: an app installs only what its root key vouches for, and
should not quietly accept a release that has none.

### OCI registries

If you already push artifacts to a container registry, that is an update
source:

```text
oci://ghcr.io/owner/app:2.0.0
oci://registry.example.com:5000/team/app        # a port is not a tag
```

* The registry's **token dance** is handled: an unauthenticated manifest
  request gets a `401` with a `WWW-Authenticate` challenge, the client
  fetches a (usually anonymous) pull token and retries. The same token is
  carried into the artifact download, since a blob request needs it too.
* A **multi-arch index** is followed to this platform's manifest. OCI names
  platforms `darwin/amd64` where Rust says `macos/x86_64`; that mapping is
  done for you. An index with nothing for your platform selects nothing
  rather than something arbitrary.
* The version comes from the `org.opencontainers.image.version` annotation,
  or from the tag when the reference names one. A `:latest` with no version
  annotation is an **error** — there is no honest answer to "which version
  is this", and reporting "up to date" would be a lie.
  `org.opencontainers.image.created` seeds the rollout ladder.
* Select the artifact layer with `?asset=` matched against the layer's media
  type or its `org.opencontainers.image.title` annotation; the first layer
  is the default.

The nice property here is that **the layer digest is the pin**. A registry
already content-addresses its blobs, so an OCI release is digest-verified by
construction — there is no checksum file to publish, forget, or trust. The
minisign chain still applies on top if you pin a root key; put the `.minisig`
and the statement in the manifest's annotations or ship them as extra layers.

## Why there are two keys

A single signing key is a bad trade: it has to live wherever your CI signs
builds, and if it leaks your only remedy is shipping a new binary to every
user, because the key they trust is compiled into the app they already have.

So the client trusts a **root key** that signs nothing but *statements*
about which signing key is currently valid:

```
root key ──signs──> signing-key statement ──names──> signing key ──signs──> artifact
                                                                  └─signs──> release statement
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

The release statement is a second single line, signed by the signing key
the first one names, also with no trailing newline:

```
azul-release-v1|app=<id>|version=<semver>|platform=<os-arch>|sha256=<hex>|released=<unix>|min_generation=<n>
```

* `app` is `AppConfig.updates.app_name` (for the Azlin apps, the app's key such
  as `azwriter`) and `platform` is `<os>-<arch>` in Rust's words
  (`linux-x86_64`, `macos-aarch64`, `windows-x86_64`).
* `released` may not lie in the future (a day of clock skew is allowed).
* `min_generation` raises the client's key-generation mark when the release
  verifies: sign releases with `min_generation` = the new generation after a
  rotation and every client that installs one refuses the retired key from
  then on, whether or not it ever saw the new key statement. It can never be
  above the generation of the key statement that vouches for the key.
* The client's marks (the highest version it has seen, the lowest key
  generation it accepts) move only after a release has verified in full. A
  release that fails halfway - say a forged `version` of `99.0.0` - moves
  nothing, so it cannot lock the client out of the genuine updates.

A manifest cannot name its own signing key — only a root-signed statement
can. That is the whole point: an attacker who controls your download server
and your manifest still cannot make a client install anything, because they
cannot produce a statement the root key signed.

An empty `root_public_key` does **not** disable the chain. An app that
carries no root key has no way to prove an update: it may check and notify,
but it stages and installs nothing and says "updates are not available in this
build". The `digest` pin alone protects against corruption and a swapped file
but not against someone who can rewrite the manifest, so it is never what an
update falls back to when a key is missing or unreadable (a key that does not
parse is an error too). The key text may list the current and the next root
key, separated by commas, so a root can be replaced without a flag day.

The only way to install without the chain is to say so: an app that sets
`config.updates.allow_unsigned = true` (and has no root key) accepts a release
pinned by its `digest` alone. That is a decision, written in the app's source or
in a test configuration; it is never what an empty or broken key turns into, and
a key, when there is one, always wins. The Azlin apps compile their root keys in
(`UPDATE_ROOT_KEYS` in `examples/azul-appkit/src/anchors.rs`) and hold no switch
for it in a shipped build; the key list is empty until the owner puts the real
root key there.

## Signing a release

Four commands, so that no run of one of them can do what only another may. Keys
live in `--keys DIR` (default `$AZUL_RELEASE_KEYS`, else `~/.azul-release-keys`);
the script refuses a folder inside a git work tree that git does not ignore.

```bash
# Once, on an OFFLINE machine: mint the ROOT key pair. The secret key is
# passphrase-encrypted and the command never overwrites a key.
scripts/sign-release.sh init-root

# Once, on the build machine: the SIGNING key pair (encrypted too; add
# --unencrypted only for a CI runner that cannot answer a prompt).
scripts/sign-release.sh init-signing

# On the offline machine, about once a year or when the signing key changes:
# the ROOT key signs "this signing key, until <date>, generation <n>".
scripts/sign-release.sh statement --generation 1

# Per release, on the build machine. The root secret key is not needed and
# not read; the signing key signs the artifact and its release statement.
scripts/sign-release.sh release \
    --app myapp \
    --artifact ./target/release/myapp \
    --version 2.0.0 \
    --platform linux-x86_64 \
    --url https://downloads.example.com/myapp-2.0.0.bin \
    --changelog https://downloads.example.com/CHANGELOG.md \
    --out manifest.json
```

Nothing here generates a key for you behind your back, and nothing in the source
tree holds one. Put the root key's PUBLIC half into the app (the `init-root`
output prints it) before the first release: until you do, every build says
"updates are not available in this build".

The script needs a minisign implementation. Either works:

```bash
cargo install rsign2          # recommended
apt install minisign          # or: brew install minisign
```

Before printing anything the script **verifies its own output with azul's
real client-side code**, for this `--app` and `--platform`, and writes the
manifest to `--out` only if that passed. Do not remove that step. A signature
can be perfectly valid to the tool that made it and still be refused by the app:

* **Prehashing.** The client accepts only prehashed signatures (`ED`), not
  the legacy form (`Ed`). `rsign2` always prehashes; some `minisign` builds
  need `-H`, which the script passes.
* **The statements are signed as exact bytes.** `echo` appends a newline and
  the signature then covers a string that is not the one in the manifest.
  The script uses `printf '%s'`.

Neither mistake is visible by inspection, and both produce a release that
looks fine until every client rejects it.

To retire a leaked signing key: `init-signing` with a fresh `--keys` folder,
`statement --generation 2` (offline), and `release … --min-generation 2`.

## Verifying before you publish

The same check runs standalone:

```bash
cargo run -p azul-layout --features updater --example verify_update_manifest -- \
    manifest.json ./myapp-2.0.0.bin RWQ…rootpubkey --app myapp --platform linux-x86_64
```

It prints the resolved signing key, the generation, when the statement
expires and whether the artifact matches the release statement — and exits
non-zero if a client would refuse the release. `--app` is required (the
release statement has to name it); `--platform` defaults to the machine it
runs on.

`--selftest` mints a throwaway hierarchy and walks the whole chain, which is
a quick way to see the byte formats without touching real keys:

```bash
cargo run -p azul-layout --features updater --example verify_update_manifest -- --selftest
```

## What a client remembers

Each program keeps `update-state.json` in its state folder: the highest version
it has seen **verified**, the highest signing-key generation it has accepted, and
the cooldown and cohort of the checks. The two marks are what refuse an
older release or a retired key, so they are handled with care: the file is
replaced atomically, a writer never lowers a mark another one raised (the app
and the install manager both write them), and a file that cannot be read is
**not** treated as a fresh start. Staging and installing stop with an error
that names the file; delete it to start over, knowing that an older release is
accepted again from then on. The install manager keeps the marks of an app it
uninstalls for the same reason.

Download limits: a release is taken in 1 MiB windows and a `206` answer is
believed only when its `Content-Range` starts where the request did and the
size it names does not change; nothing larger than 2 GiB is taken.

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

(The drill is the app `telemetry-demo`: sign its test release with
`--app telemetry-demo`.)

```text
update: install=UserWritable effective_mode=SelfUpdate
update: 1.0.0 -> 2.0.0 available (manual mode)
update: staged AND VERIFIED …/staging/myapp-2.0.0.bin (4096 bytes, cached=false)
        — signature chain OK, key generation now 1
```

The same drill takes a GitHub repository, which is the quickest way to see
source resolution working against something real:

```bash
cargo run -p azul-layout --features updater,telemetry \
    --example telemetry_grafana -- \
    --update-manifest github://BurntSushi/ripgrep --version 0.1.0
```

```text
update: 0.1.0 -> 15.2.0 available (manual mode)
update: downloaded …/ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz (2265718 bytes)
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

## Updating other programs

An install manager (the Azlin apps' AzInstallManager) checks and installs
programs other than itself. The updater has the calls it needs; they are
blocking, so a manager runs them on a `Thread`:

* `ReleaseInfo::check_program(url, current_version, channel, state_dir,
  bucket, audience)` checks one program with its own state folder (the
  high-water mark, the accepted key generation) under the cohort the
  manager passes in: one computer sits in one cohort for every program it
  runs. `ReleaseInfo::draw_rollout_bucket()` draws that cohort once.
* `release.stage(app, staging_dir, root_public_key, allow_unsigned, state_dir)`
  downloads and verifies the digest and the signature chain for the program
  named `app` (its release statement has to say so); nothing refused stays.
  It returns the verified file with the SHA-256 of the bytes that passed
  (`StagedUpdate`), so the manager can install exactly those bytes.
* `release.verify_staged(app, path, root_public_key, allow_unsigned,
  state_dir)` checks a staged file once more right before it is installed: a
  file on disk is not trusted across runs.
* `UpdateAudience::Tester` is the gate of a beta tester: every release the
  moment it is published, whatever the rollout says.
* `ReleaseInfo::within_maintenance_window(rule, now)` evaluates a window
  the manager's own config names.

`InstallKind::detect_for(path)` tells the manager whether a program is a
package manager's (it then shows the update command instead of installing).

## Key hygiene, briefly

* The root secret key never touches CI. If it leaks, an attacker can appoint
  their own signing key and there is no in-band recovery — that is why it
  signs once a year and lives offline.
* The signing key on the build machine is the one you expect to rotate.
  Practise the rotation *before* you need it: bump `--generation`, publish
  with `--min-generation`, confirm clients accept the new statement.
* Keep key folders out of the repository. `release-keys/`, `root.key` and
  `signing.key` are in `.gitignore`, and the script refuses a folder in a work
  tree that git does not ignore - but the right place is outside the tree.
* Keep the statement's `expires` shorter than the interval at which you are
  confident you would notice a compromise.

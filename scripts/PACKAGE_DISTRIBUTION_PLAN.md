# Package distribution & multi-product channel plan

Status: PLAN (review me; do not implement wholesale yet). Author pass: 2026-05-30.

This plan defines how Azul (→ future **Azlin**) ships native packages so that:

1. **Artifacts always live on GitHub Releases** (no GitHub Pages file-size /
   1 GB-site limits; Releases allow ≤2 GB/asset).
2. **Every channel users configure is on our own domain** — `azul.rs` today,
   `azlin.io` later — so users, Docker `ADD` URLs, and scripts never hardcode
   `github.com`. We stay free to add a CDN/DNS passthrough later.
3. **Updates flow** — a user on Linux Mint / macOS / Windows who configured the
   channel once keeps getting new versions via the native updater
   (`apt upgrade`, `brew upgrade`, `choco upgrade`, `dnf upgrade`, …).
4. **Multiple products** share the scheme — today just the GUI lib, later a
   `ws` workspace metapackage and `os/*` OS components.

It also stages the **azul → azlin rename** (kept testable on azul.rs first).

--------------------------------------------------------------------------------
## 0. Vocabulary: channel vs. artifact (the one idea that makes this work)

Every package manager separates two things:

- **Channel** = the stable, version-FREE URL the user configures ONCE
  (`azul.rs/apt`, the brew tap git URL, the choco/nuget feed). The updater
  re-reads its *metadata* periodically; that's how "an update is available" is
  discovered. **The version must never appear in the channel URL.**
- **Artifact** = the actual versioned file (`.deb`, `.dylib`, `azul.dll`, …),
  discovered *through* the channel's metadata, which carries a per-version
  download location + checksum.

Multi-product = more *packages inside the same channel* (exactly how Debian and
Homebrew host thousands of packages under one repo). No new infra, no per-product
domain needed.

--------------------------------------------------------------------------------
## 1. Where artifacts live: GitHub Releases, fronted by our domain

**Storage of record = GitHub Releases**, version-pinned and permanent:
`github.com/<org>/<repo>/releases/download/<version>/<file>`
(this is what CI already does for the big `.a` / demos via `gh release upload`).

**Public URL = our domain**, via a stable rewrite path:
`https://azul.rs/dl/<product>/<version>/<file>`  (later `azlin.io/dl/...`).

Two ways to make `azul.rs/dl/...` resolve to the GitHub Release, in increasing
order of independence (the metadata generator is identical for all three — it
only ever emits the `azul.rs/dl/...` form):

  - **(a) now, zero-infra:** GitHub Pages is static and cannot 301-redirect, so
    for the FIRST cut emit the GitHub Releases URL directly in metadata that
    allows an absolute URL (brew, choco install script — see §3), and host the
    small repo-relative artifacts (.deb/.rpm/.apk/pacman pkg) on Pages as today.
  - **(b) soon:** put Cloudflare (free) in front of azul.rs/azlin.io and add a
    redirect rule `azul.rs/dl/*  ->  github.com/<org>/<repo>/releases/download/*`.
    Then ALL metadata uses `azul.rs/dl/...` and nothing references github.com.
  - **(c) later:** self-host or CDN-cache the bytes under `/dl/` if we ever want
    to drop GitHub entirely. Metadata never changes.

DECISION captured: target is **(b)** — metadata emits `azul.rs/dl/<product>/<version>/<file>`,
backed by a Cloudflare redirect to GitHub Releases. Until the redirect exists,
ship **(a)** (brew/choco point at the GH release URL; apt/dnf/etc. serve their
small packages from Pages). The generator takes a `--dl-base` flag so flipping
(a)→(b) is a one-line CI change.

### Per-manager: can the metadata point at an absolute (off-repo) artifact URL?

| Manager   | Metadata file                | Absolute artifact URL allowed? | First-cut artifact home |
|-----------|------------------------------|--------------------------------|-------------------------|
| Homebrew  | `Formula/*.rb` `url "..."`   | YES (any URL)                  | GH Releases / `/dl` ✅ |
| Chocolatey| `tools/chocolateyInstall.ps1` `Get-ChocolateyWebFile -Url64bit` | YES | GH Releases / `/dl` ✅ |
| apt/dpkg  | `Packages` `Filename:`       | NO — relative to `deb <base>`  | Pages `azul.rs/apt/pool` (apt FOLLOWS 30x → `/dl` once (b) lands) |
| dnf/yum   | `repodata` `<location href>` | via `xml:base` — YES-ish       | Pages `azul.rs/rpm`; can set `xml:base` to `/dl` |
| zypper    | (same repodata as dnf)       | same as dnf                    | shares the rpm repo |
| pacman    | `<db>.db` + `Server =`       | NO — relative to Server base   | Pages `azul.rs/arch` (or Server=/dl after (b)) |
| Alpine apk| `APKINDEX.tar.gz`            | NO — relative to repo base     | Pages `azul.rs/alpine` |
| Maven     | already self-contained (jar+pom) | YES (repo layout)          | Pages `azul.rs/maven` (jar can also be a /dl redirect) |
| PyPI      | PEP503 `<a href>`            | YES (any URL)                  | GH Releases / `/dl` ✅ |
| npm       | metadata `dist.tarball`      | YES (any URL)                  | GH Releases / `/dl` ✅ |

Key realization: the genuinely-huge artifacts (`.a` static libs 200 MB+, demo
binaries) are **not** shipped by any system package manager — they're manual
downloads. So "artifacts must be on GitHub Releases for size" and "package repos
need small repo-relative files" do NOT actually conflict: the package files
(.deb/.rpm/.apk ≈ a few MB, dylib/dll/so ≈ 16 MB) are Pages-servable today, and
move behind `/dl` when the redirect lands. Only the never-package-managed big
blobs are GH-Releases-only.

--------------------------------------------------------------------------------
## 2. Channel + product namespacing (today azul.rs, future azlin.io)

Target end state (azlin.io), products: `ui` (the GUI lib, = today's libazul),
`ws` (workspace metapackage), `os/*` (OS components — OUT OF SCOPE here).

Stable channel URLs (version-free), each hosts MANY product packages:

```
azlin.io/apt                      apt repo            (packages: azlin-ui, azlin-ws, …)
azlin.io/rpm                      dnf/yum/zypper      (azlin-ui, azlin-ws, …)
azlin.io/arch                     pacman repo
azlin.io/alpine                   apk repo
azlin.io/nuget/index.json         choco + nuget       (azlin-ui, azlin-ws, …)
azlin.io/brew.git                 ONE homebrew tap    (Formula/ui.rb, ws.rb, …)
azlin.io/maven, /pypi, /npm       language bindings (ui only)
azlin.io/dl/<product>/<ver>/<f>   artifact redirect → GitHub Releases
```

Homebrew UX note: `brew install azlin.io/ui` is not literal brew syntax — taps
are `user/repo`. The closest clean mapping is a SINGLE tap with a formula per
product:
```
brew tap azlin https://azlin.io/brew.git     # once
brew install azlin/brew/ui                    # the GUI lib
brew install azlin/brew/ws                     # workspace metapackage
```
(If we prefer the `azlin/ui` short form, use a tap-per-product:
`brew tap azlin/ui https://azlin.io/ui.git`. Tradeoff: N repos vs N formulas in
1 repo. RECOMMEND: one `brew.git` tap, formula-per-product — one repo to deploy.)

Linux package NAMING: `azlin-ui`, `azlin-ws` (so `apt install azlin-ui`,
`dnf install azlin-ui`, `pacman -S azlin-ui`). A meta-package `azlin-ws` simply
`Depends: azlin-ui, …` so `apt install azlin-ws` pulls the set — and `apt
upgrade` keeps every Mint/Ubuntu/Debian box on the latest, which is the
"everyone gets the same updates" requirement.

--------------------------------------------------------------------------------
## 3. How "updates" work per manager (all PULL-based; no push)

The deploy regenerates each channel's metadata every release; the version only
ever changes INSIDE the metadata + artifact path, never in the configured
channel. Native discovery:

- apt:    `apt update && apt upgrade`     (auto: `unattended-upgrades`)
- dnf:    `dnf upgrade`                    (auto: `dnf-automatic.timer`)
- zypper: `zypper update`
- pacman: `pacman -Syu`
- apk:    `apk upgrade`
- brew:   `brew update && brew upgrade`    (`brew outdated` to list)
- choco:  `choco upgrade all`              (auto: scheduled task)

No in-app update-check endpoint (per decision). If ever wanted, a single
`azlin.io/dl/<product>/latest.json` would suffice — left out now.

--------------------------------------------------------------------------------
## 4. azul → azlin rename (PLAN ONLY — test on azul.rs, migrate later)

Goal: rename the PRODUCT/brand "Azul" → "Azlin" (avoid the Azul Systems / Azul
Zulu JVM trademark) while KEEPING the `Az` code prefix (AzString, AzDom, …).

Scope of the rename (do NOT execute yet — stage + test on azul.rs first):

- **Brand / domain:** azul.rs → azlin.io (keep azul.rs as a redirect for a
  release or two).
- **Package ids:** `libazul` → `azlin-ui` (Linux/choco/brew formula `ui`);
  the GUI lib product is "Azlin UI".
- **Native lib filenames:** KEEP `libazul.so/.dylib` + `azul.dll` for now (ABI
  filename churn breaks every existing linker/script); rename in a major bump,
  shipping symlinks `libazlin.so -> libazul.so` during transition.
- **API symbols:** UNCHANGED — keep the `Az` prefix (`AzString`, `Az*`), so
  api.json + every binding stays put. This is the whole point of "keep Az".
- **Crate names:** internal (`azul-core`, `azul-dll`, …) can stay until a later
  cleanup; crates.io rename is a separate, disruptive step — defer.
- **ghcr image:** `ghcr.io/fschutt/azul` → add `ghcr.io/fschutt/azlin` tag
  alias; keep both during transition.

Testability: build the WHOLE azlin.io layout under azul.rs first — e.g. generate
`azul.rs/brew.git` with formula `ui`, apt package `azlin-ui`, etc. — verify the
install/upgrade flows end-to-end, THEN flip the base domain. The mirror generator
already takes the base URL as an argument, so this is a config flip, not a
rewrite.

--------------------------------------------------------------------------------
## 5. Implementation steps (incremental; each independently shippable)

P0 (refit what already shipped — commit dfb2bb4d6 used version-pinned
   `azul.rs/release/0.2.0/...` artifact URLs, which vanish on the next deploy):
  - Add a `--dl-base` arg to `scripts/build_registry_mirrors.sh` (default
    `https://azul.rs/dl/libazul/<version>`). Emit that in the brew formula `url`,
    the choco install script `-Url64bit`, the pypi/npm download links.
  - CI: after `gh release upload`, the artifacts are at
    `github.com/<org>/<repo>/releases/download/<version>/<file>`. Either (a) emit
    that URL directly now, or (b) add the Cloudflare `/dl/*` redirect and emit
    `/dl/...`. Pick (a) for the next deploy, (b) when DNS is ready.
  - Ensure the package-manager-shipped binaries (dylib/dll/so, .deb/.rpm) are
    uploaded to the GH Release too (today only `.a`/demos/.deb/.rpm are).

P1 (new Linux managers — all static, mirror the apt/rpm pattern in
   build_registry_mirrors.sh):
  - pacman: `repo-add azul.rs/arch/azul.db.tar.gz <pkg>.pkg.tar.zst` (needs the
    `.pkg.tar.zst` built — add a makepkg/PKGBUILD step or convert).
  - apk: build `APKINDEX.tar.gz` (`apk index`), sign with an abuild key.
  - zypper: NONE — it consumes the existing `azul.rs/rpm` repodata as-is; just
    document the `zypper ar https://azul.rs/rpm azlin` line.

P2 (multi-product structure):
  - Parameterize the generator by PRODUCT (`ui`, `ws`): `build_registry_mirrors.sh
    <site> <version> <artifacts> <product>`; package id `azlin-<product>`.
  - `ws` metapackage: a tiny .deb/.rpm/formula with only Depends/dependencies.
  - One brew tap repo `brew.git`, formula per product.

P3 (rename staging): introduce `azlin-*` package ids + `libazlin` symlinks +
   `ghcr.io/.../azlin` alias on azul.rs; verify; then flip domain to azlin.io
   with azul.rs → azlin.io redirects.

--------------------------------------------------------------------------------
## 6. Open questions for the maintainer

1. Homebrew: one `brew.git` tap w/ formula-per-product (recommended) vs
   tap-per-product (`azlin/ui`)? Affects how many repos/dirs we generate.
2. DNS/CDN: is Cloudflare (or similar) available in front of azul.rs/azlin.io so
   we can do the `/dl/*` → GitHub Releases redirect (enables full domain
   independence)? If not, brew/choco use github.com URLs in the interim.
3. Rename timing: cut `azlin-*` package ids now (alongside `libazul`, as
   aliases) so early adopters land on the final names, or wait for the domain?
4. crates.io: keep `azul`/`azul-*` crate names indefinitely, or plan an
   `azlin-*` crates rename (disruptive — separate project)?

--------------------------------------------------------------------------------
## 7. Files this touches (when implemented)

- `scripts/build_registry_mirrors.sh` — add `--dl-base`, `<product>` arg,
  pacman/apk builders.
- `.github/workflows/rust.yml` deploy_pages — upload dylib/dll/so to the GH
  Release; pass `--dl-base`; (later) the Cloudflare redirect is infra, not CI.
- `doc/guide/en/hello-world/*.md` — channel + `brew tap`/`choco --source` lines
  (already done for apt/dnf/brew/choco; update artifact URLs when /dl lands).
- `doc/src/dllgen/deploy.rs` — release-page binding-download block URLs.

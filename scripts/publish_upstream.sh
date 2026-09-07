#!/usr/bin/env bash
# Publish the self-hosted mirrors' contents to the "real" registries — when,
# and only when, the credential for that registry is present in the
# environment. Without it each step prints what it WOULD do and skips; the
# self-hosted channel on azul.rs stays the baseline the docs point at.
#
#   scripts/publish_upstream.sh <website_dir> <version>
#
# Credentials (GitHub Actions secrets, mapped to env by the deploy job):
#   CARGO_REGISTRY_TOKEN   crates.io: the pre-rendered `azul` crate
#                          (website/ui/release/<V>/azul-<V>.crate)
#   AZUL_TAP_PUSH_TOKEN    GitHub: mirror-push ui/brew.git -> fschutt/homebrew-azul
#                          and ui/scoop.git -> fschutt/scoop-azul, so the short
#                          `brew tap fschutt/azul` / `scoop bucket add azul
#                          fschutt/scoop-azul` forms work too
#   (Chocolatey's community push needs the choco CLI = Windows; see the
#   `publish_choco` step in rust.yml. PyPI / npm / RubyGems / NuGet / Maven
#   Central / AUR / LuaRocks have their own gated steps in their packaging
#   jobs; apt/rpm/pacman/apk SIGNING is AZUL_{APT,RPM,PACMAN,APK}_* in
#   build_linux_packages + build_registry_mirrors.sh.)
#
# Every step is idempotent for a version already published (crates.io refuses
# a re-publish; a mirror push of identical history is a no-op).
set -uo pipefail
SITE="${1:?website dir}"
V="${2:?version}"
RELDIR="$SITE/ui/release/$V"
FAILED=""

skip() { echo "  [$1] $2 not set - skipping (self-hosted mirror stays the channel)"; }

# --------------------------------------------------------------------------
# crates.io — the same crate the azul.rs sparse registry serves.
# --------------------------------------------------------------------------
publish_crates_io() {
  [ -n "${CARGO_REGISTRY_TOKEN:-}" ] || { skip crates.io CARGO_REGISTRY_TOKEN; return; }
  local krate="$RELDIR/azul-$V.crate"
  [ -s "$krate" ] || { echo "::error::[crates.io] $krate missing"; return 1; }
  command -v cargo >/dev/null 2>&1 || { echo "::error::[crates.io] cargo not installed"; return 1; }
  local w; w="$(mktemp -d)"
  tar xzf "$krate" -C "$w" || { echo "::error::[crates.io] $krate does not unpack"; return 1; }
  # Already there? Then this version is published and re-publishing is an error
  # crates.io would raise anyway.
  if curl -fsSL "https://crates.io/api/v1/crates/azul/$V" -o /dev/null 2>/dev/null; then
    echo "  [crates.io] azul $V is already published"; return 0
  fi
  ( cd "$w/azul-$V" && cargo publish --allow-dirty --token "$CARGO_REGISTRY_TOKEN" ) \
    || { echo "::error::[crates.io] cargo publish failed for azul $V"; return 1; }
  echo "  [crates.io] published azul $V"
}

# --------------------------------------------------------------------------
# GitHub-hosted tap + bucket — mirror pushes of the bare repos on the site.
# --------------------------------------------------------------------------
mirror_push() { # $1 local bare repo, $2 github repo (owner/name), $3 label
  local src="$1" repo="$2" label="$3"
  [ -d "$src" ] || { echo "  [$label] $src not built - nothing to push"; return; }
  git -C "$src" push --mirror --quiet "https://x-access-token:${AZUL_TAP_PUSH_TOKEN}@github.com/$repo.git" \
    || { echo "::error::[$label] mirror push to github.com/$repo failed"; return 1; }
  echo "  [$label] mirrored $src -> github.com/$repo"
}
publish_github_taps() {
  [ -n "${AZUL_TAP_PUSH_TOKEN:-}" ] || { skip "tap/bucket" AZUL_TAP_PUSH_TOKEN; return; }
  local rc=0
  mirror_push "$SITE/ui/brew.git"  fschutt/homebrew-azul brew  || rc=1
  mirror_push "$SITE/ui/scoop.git" fschutt/scoop-azul    scoop || rc=1
  return $rc
}

echo "==> Upstream publishing for azul $V (secret-gated)"
publish_crates_io   || FAILED="$FAILED crates.io"
publish_github_taps || FAILED="$FAILED tap/bucket"
if [ -n "$FAILED" ]; then
  echo "::error::upstream publishing FAILED for:$FAILED"
  exit 1
fi
echo "==> Upstream publishing done."

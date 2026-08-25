#!/usr/bin/env bash
# publish-crates.sh — publish azul-css / azul-core / azul-layout to crates.io.
#
# Lives here rather than inline in .github/workflows/rust.yml for two reasons:
# that file is within a few KB of GitHub's 512,000-byte workflow limit (over
# which runs are silently not started), and a publish loop is easier to reason
# about — and to dry-run locally — as a script than as embedded YAML.
#
# AUTHENTICATION. Reads CARGO_REGISTRY_TOKEN from the environment and does not
# care where it came from. In CI it is a ~30-minute Trusted Publishing token
# minted from GitHub's OIDC identity by rust-lang/crates-io-auth-action and
# revoked when the job ends; the long-lived API token is only a transitional
# fallback. That distinction matters here specifically: `cargo publish` runs a
# VERIFICATION BUILD, which compiles the crate and its dependencies and
# therefore executes every build script in the tree — with whatever is in this
# environment. A permanent publish credential sitting there is the same
# exposure class as the Apple signing cert that used to sit in the mobile job's
# environment. A short-lived, claim-scoped token is worth far less if read.
#
# IDEMPOTENT. A version already on crates.io is skipped rather than failing with
# "already uploaded". Publishing is irreversible, so a re-run after ANY partial
# outcome — a cancelled run that got one crate out, a manual publish done to
# unblock a release (0.0.14 went out that way on 2026-08-17) — must converge on
# "everything published" rather than go red on the crates that already made it.
#
# Usage:  CARGO_REGISTRY_TOKEN=… scripts/publish-crates.sh [--dry-run]
set -euo pipefail

CRATES=(azul-css azul-core azul-layout)
UA="azul-ci (github.com/fschutt/azul)"
DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1

if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo "::notice::No crates.io credential available — packaged the crates but skipping publish."
  echo "  Trusted Publishing mints one automatically once the publisher is configured at"
  echo "  https://crates.io/crates/<crate>/settings under Trusted Publishing."
  exit 0
fi

version_of() {
  cargo metadata --format-version 1 --no-deps \
    | python3 -c "import sys,json;d=json.load(sys.stdin);print(next(p['version'] for p in d['packages'] if p['name']=='$1'))"
}

# Publish in dependency order; sleep between crates so the index has time to
# expose each new version to the next crate's resolver.
for crate in "${CRATES[@]}"; do
  version="$(version_of "$crate")"
  status="$(curl -s -o /dev/null -w '%{http_code}' \
    "https://crates.io/api/v1/crates/$crate/$version" -H "User-Agent: $UA")"
  if [ "$status" = "200" ]; then
    echo "::notice::$crate $version is already on crates.io — skipping (idempotent re-run)."
    continue
  fi
  if [ "$DRY" = "1" ]; then
    echo "would publish $crate $version"
    continue
  fi
  echo "::group::cargo publish -p $crate ($version)"
  # --token on the command line rather than the env var is deliberate: cargo
  # reads either, and passing it explicitly keeps the intent visible in the log
  # (the value itself is masked by Actions).
  cargo publish -p "$crate" --token "$CARGO_REGISTRY_TOKEN"
  echo "::endgroup::"
  sleep 20
done

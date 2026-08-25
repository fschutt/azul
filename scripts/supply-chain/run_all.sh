#!/usr/bin/env bash
# run_all.sh — run every supply-chain gate locally, exactly as CI runs them.
#
# CI runs these in two jobs (supply_chain_preflight and supply_chain_versions in
# .github/workflows/rust.yml). This script is the single local entry point, so
# "it passed on my machine" and "it passed in CI" mean the same thing.
#
#   scripts/supply-chain/run_all.sh                  # the blocking gates
#   scripts/supply-chain/run_all.sh --with-cooldown  # + publish-age (slow: one
#                                                    #   crates.io request per crate)
#   scripts/supply-chain/run_all.sh --keep-vendor    # leave vendor/ in place
#
# The vendor tree is ~1.1 GB and is removed on exit unless --keep-vendor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VENDOR="${AZ_SC_VENDOR:-vendor}"
META="${AZ_SC_META:-target/sc-metadata.json}"
KEEP=0
COOLDOWN=0
for arg in "$@"; do
  case "$arg" in
    --keep-vendor) KEEP=1 ;;
    --with-cooldown) COOLDOWN=1 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

created_vendor=0
cleanup() {
  if [ "$KEEP" = "0" ] && [ "$created_vendor" = "1" ]; then
    rm -rf "$VENDOR"
  fi
}
trap cleanup EXIT

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

if [ ! -d "$VENDOR" ]; then
  step "cargo vendor (downloads sources; runs nothing)"
  created_vendor=1
  cargo vendor --locked --versioned-dirs "$VENDOR" > /dev/null
  echo "vendored $(find "$VENDOR" -maxdepth 1 -mindepth 1 -type d | wc -l | tr -d ' ') crates"
else
  echo "reusing existing $VENDOR"
fi

mkdir -p "$(dirname "$META")"
cargo metadata --format-version=1 --locked --all-features > "$META"

rc=0
step "1/4  Lockfile integrity (vendor checksums, yanked versions, index checksums)"
python3 scripts/supply-chain/lockfile_guard.py \
  --check vendor --check yanked --check cksum --vendor "$VENDOR" || rc=1

step "2/4  Environment access from build-time code"
python3 scripts/supply-chain/env_guard.py \
  --vendor "$VENDOR" --metadata "$META" || rc=1

step "3/4  Build-time code execution policy (build.rs / proc-macro)"
python3 scripts/supply-chain/scan_build_scripts.py \
  --vendor "$VENDOR" --metadata "$META" || rc=1

step "4/4  cargo-vet (per-version audit state)"
if command -v cargo-vet > /dev/null; then
  cargo vet --locked || rc=1
else
  echo "cargo-vet not installed — skipping (cargo install cargo-vet --locked)"
fi

if [ "$COOLDOWN" = "1" ]; then
  step "extra  Publish-age cooldown (one crates.io request per crate — slow)"
  python3 scripts/supply-chain/lockfile_guard.py --check cooldown --min-age-days 14 || rc=1
fi

printf '\n'
if [ "$rc" = "0" ]; then
  echo "all supply-chain gates passed"
else
  echo "one or more supply-chain gates FAILED (see above)"
fi
exit "$rc"

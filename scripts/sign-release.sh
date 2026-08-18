#!/usr/bin/env bash
#
# sign-release.sh — produce the signed manifest fields for an azul update.
#
# The client (azul-layout's `updater`) verifies a two-link chain:
#
#   root key ──signs──> signing-key STATEMENT ──names──> signing key ──signs──> artifact
#
# The root key is the one compiled into your app
# (`AppConfig.updates.root_public_key`); it should live offline and sign
# nothing but statements. The signing key does the day-to-day work and can be
# rotated by publishing a new statement with a higher `generation` — clients
# remember the highest generation they have accepted and refuse anything
# lower, so a leaked retired key stays dead.
#
# Usage:
#   scripts/sign-release.sh --artifact ./azul-app-2.0.0.bin --version 2.0.0 \
#       --url https://downloads.example.com/azul-app-2.0.0.bin \
#       [--changelog https://example.com/CHANGELOG.md] \
#       [--keys ./release-keys] [--generation 1] [--expires-days 365] \
#       [--out manifest.json]
#
# On the first run it MINTS a key pair for each role into --keys and stops, so
# you can put the root secret key somewhere safe before it ever signs
# anything. Re-run the same command afterwards to sign.
#
# The script verifies its own output with azul's real client-side code before
# printing it. Do not skip that: a minisign signature can be perfectly valid
# and still be rejected by the client (see PREHASHING below), and a release
# checked only by the tool that made it is not checked at all.
#
# PREHASHING: the client accepts only prehashed signatures (`ED`), never the
# legacy form (`Ed`). Some minisign versions produce legacy signatures unless
# `-H` is passed. This script always passes `-H`, and the self-check refuses
# the release if a legacy signature slipped through anyway.

set -euo pipefail

die() { printf 'sign-release: %s\n' "$*" >&2; exit 1; }
note() { printf '  %s\n' "$*" >&2; }

ARTIFACT=""
VERSION=""
URL=""
CHANGELOG=""
KEYS_DIR="./release-keys"
GENERATION=1
EXPIRES_DAYS=365
OUT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --artifact)     ARTIFACT="${2:-}"; shift 2 ;;
    --version)      VERSION="${2:-}"; shift 2 ;;
    --url)          URL="${2:-}"; shift 2 ;;
    --changelog)    CHANGELOG="${2:-}"; shift 2 ;;
    --keys)         KEYS_DIR="${2:-}"; shift 2 ;;
    --generation)   GENERATION="${2:-}"; shift 2 ;;
    --expires-days) EXPIRES_DAYS="${2:-}"; shift 2 ;;
    --out)          OUT="${2:-}"; shift 2 ;;
    -h|--help)      sed -n '2,40p' "$0"; exit 0 ;;
    *)              die "unknown argument: $1" ;;
  esac
done

# Either CLI works. rsign2 is preferred where both exist: it ALWAYS prehashes
# (its -H flag is a no-op kept for compatibility), which is exactly what the
# client requires, whereas some minisign builds need -H to be passed.
if command -v rsign >/dev/null 2>&1; then
  SIGNER=rsign
elif command -v minisign >/dev/null 2>&1; then
  SIGNER=minisign
else
  die "no signing tool found. Either: cargo install rsign2   (recommended)
                            or: apt install minisign / brew install minisign"
fi

# gen_key <pubfile> <secfile>
gen_key() {
  case "$SIGNER" in
    rsign)    rsign generate -p "$1" -s "$2" -W --unencrypted -c "azul release key" >/dev/null ;;
    minisign) minisign -G -W -p "$1" -s "$2" >/dev/null ;;
  esac
}

# sign_file <secfile> <file> <sigfile> <untrusted-comment> <trusted-comment>
sign_file() {
  case "$SIGNER" in
    rsign)    rsign sign -s "$1" -x "$3" -c "$4" -t "$5" -W "$2" >/dev/null ;;
    # -H forces PREHASHED signatures; without it some minisign versions emit
    # the legacy form, which the client refuses. The self-check below is the
    # backstop if a build ignores the flag.
    minisign) minisign -S -H -s "$1" -m "$2" -x "$3" -c "$4" -t "$5" >/dev/null ;;
  esac
}

ROOT_SEC="$KEYS_DIR/root.key"
ROOT_PUB="$KEYS_DIR/root.pub"
SIGN_SEC="$KEYS_DIR/signing.key"
SIGN_PUB="$KEYS_DIR/signing.pub"

# ---------------------------------------------------------------- key minting
if [ ! -f "$ROOT_SEC" ] || [ ! -f "$SIGN_SEC" ]; then
  mkdir -p "$KEYS_DIR"
  [ -f "$ROOT_SEC" ] || gen_key "$ROOT_PUB" "$ROOT_SEC"
  [ -f "$SIGN_SEC" ] || gen_key "$SIGN_PUB" "$SIGN_SEC"
  cat >&2 <<EOF

Key pairs written to $KEYS_DIR.

  BEFORE YOU SIGN ANYTHING:
    * Move $ROOT_SEC offline (hardware token, or a machine that is not your
      build server). It signs statements only, roughly once a year.
      Losing it means shipping a new binary to every user; leaking it means
      an attacker can appoint their own signing key.
    * Compile the ROOT PUBLIC key into your app:

        config.updates.root_public_key =
            "$(tail -n +2 "$ROOT_PUB" | tr -d '\n')".into();

    * $SIGN_SEC may live on the build machine. If it leaks, publish a new
      statement with --generation $((GENERATION + 1)); clients refuse the old
      one from then on.

Re-run this command to sign the release.
EOF
  exit 0
fi

[ -n "$ARTIFACT" ] || die "--artifact is required"
[ -f "$ARTIFACT" ] || die "artifact not found: $ARTIFACT"
[ -n "$VERSION" ]  || die "--version is required"
[ -n "$URL" ]      || die "--url is required (where clients download the artifact)"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ------------------------------------------------------------- the statement
SIGN_PUB_B64="$(tail -n +2 "$SIGN_PUB" | tr -d '\n')"
ROOT_PUB_B64="$(tail -n +2 "$ROOT_PUB" | tr -d '\n')"
NOW="$(date -u +%s)"
EXPIRES=$(( NOW + EXPIRES_DAYS * 86400 ))
STATEMENT="azul-signing-key-v1|pubkey=${SIGN_PUB_B64}|expires=${EXPIRES}|generation=${GENERATION}"

# EXACT BYTES, no trailing newline: the client verifies the statement string
# as it appears in the manifest. `printf '%s'`, never `echo`.
printf '%s' "$STATEMENT" > "$WORK/statement.txt"

note "signing the statement with the ROOT key ($SIGNER)…"
sign_file "$ROOT_SEC" "$WORK/statement.txt" "$WORK/statement.minisig" \
    "azul signing-key statement" "generation $GENERATION, expires $EXPIRES"

note "signing the artifact with the SIGNING key ($SIGNER)…"
sign_file "$SIGN_SEC" "$ARTIFACT" "$WORK/artifact.minisig" \
    "azul release $VERSION" "azul release $VERSION"

# ------------------------------------------------------------------- digest
if command -v sha256sum >/dev/null 2>&1; then
  DIGEST="$(sha256sum "$ARTIFACT" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
  DIGEST="$(shasum -a 256 "$ARTIFACT" | cut -d' ' -f1)"
else
  die "neither sha256sum nor shasum found"
fi

# ------------------------------------------------------------------ manifest
# python3 does the JSON so multi-line signature blocks are escaped correctly.
MANIFEST="${OUT:-$WORK/manifest.json}"
STATEMENT="$STATEMENT" \
VERSION="$VERSION" URL="$URL" CHANGELOG="$CHANGELOG" DIGEST="$DIGEST" \
ART_SIG_FILE="$WORK/artifact.minisig" STMT_SIG_FILE="$WORK/statement.minisig" \
python3 - "$MANIFEST" <<'PY'
import json, os, sys
read = lambda p: open(p, encoding="utf-8").read()
manifest = {
    "latest": {
        "version": os.environ["VERSION"],
        "download_url": os.environ["URL"],
        "changelog_md": os.environ["CHANGELOG"],
        "digest": "sha256:" + os.environ["DIGEST"],
        "signature": read(os.environ["ART_SIG_FILE"]),
        "signing_key_statement": os.environ["STATEMENT"],
        "signing_key_statement_sig": read(os.environ["STMT_SIG_FILE"]),
    }
}
with open(sys.argv[1], "w", encoding="utf-8") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY

# --------------------------------------------------------------- self-check
note "verifying the result the way a CLIENT will…"
if ! cargo run -q --release --manifest-path "$REPO_ROOT/Cargo.toml" \
      -p azul-layout --features updater --example verify_update_manifest -- \
      "$MANIFEST" "$ARTIFACT" "$ROOT_PUB_B64"; then
  cat >&2 <<'EOF'

The release did NOT verify. Do not publish it.

If the failure names the artifact or statement signature, the most likely
cause is a LEGACY (non-prehashed) minisign signature: check that your
minisign build honours -H, or use `cargo install rsign2`, which always
prehashes.
EOF
  exit 1
fi

if [ -n "$OUT" ]; then
  note "manifest written to $OUT"
else
  echo
  cat "$MANIFEST"
fi

cat >&2 <<EOF

Publish the manifest at your AppConfig.updates.manifest_url, and the artifact
at:
  $URL

This app's clients must be built with:
  config.updates.root_public_key = "$ROOT_PUB_B64".into();
EOF

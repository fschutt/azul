#!/usr/bin/env bash
#
# sign-release.sh — produce the signed manifest fields for an azul update.
#
# The client (azul-layout's `updater`) verifies a signature chain:
#
#   root key ──signs──> signing-key STATEMENT ──names──> signing key ──signs──> artifact
#                                                                     └─signs──> RELEASE STATEMENT
#
# The release statement is what binds the artifact to its app, version and platform:
#
#   azul-release-v1|app=<id>|version=<semver>|platform=<os-arch>|sha256=<hex>|released=<unix>|min_generation=<n>
#
# A manifest's own `version`, `download_url` and rollout are unsigned words; the client refuses a
# release whose statement does not name the app it is, the platform it runs on, the version the
# manifest says and the hash of the bytes it downloaded - so a genuine OLD release cannot be served
# under a new version label - and only a release that verified moves the client's marks (the
# highest version seen, the lowest signing-key generation it accepts).
#
# THE ROOT KEY SIGNS NOTHING BUT KEY STATEMENTS, and only on a machine that is not the build
# server. Its public half is compiled into the app (examples/azul-appkit/src/anchors.rs,
# UPDATE_ROOT_KEYS for the Azlin apps; `AppConfig.updates.root_public_key` for any other).
# The signing key does the day-to-day work and can be rotated by a new statement with a higher
# `generation`. Four commands, so that no run of one of them can do what only another may:
#
#   init-root      mint the ROOT key pair. The secret key is ENCRYPTED (you are asked for a
#                  passphrase) and the command refuses to overwrite a key. Run it once, offline.
#   init-signing   mint the SIGNING key pair, encrypted too unless --unencrypted (a CI runner that
#                  cannot answer a prompt: the key is then only as safe as that runner).
#   statement      the ROOT key signs "this signing key, until <date>, generation <n>". Offline,
#                  about once a year or when the signing key is replaced. Writes
#                  signing-key-statement.txt and .minisig into --out (default: the keys folder).
#   release        the SIGNING key signs an artifact and its release statement; the stored key
#                  statement goes into the manifest as it is. The root key is not needed and not
#                  read. The result is checked with azul's real client-side code before it is
#                  printed.
#
# Usage:
#   scripts/sign-release.sh init-root     [--keys DIR]
#   scripts/sign-release.sh init-signing  [--keys DIR] [--unencrypted]
#   scripts/sign-release.sh statement     [--keys DIR] --generation N [--expires-days 365] [--out DIR]
#   scripts/sign-release.sh release       [--keys DIR] --app azwriter --artifact ./AzWriter \
#       --version 2.0.0 --url https://downloads.example.com/AzWriter-2.0.0 \
#       [--platform macos-aarch64] [--changelog URL] [--min-generation N] \
#       [--statement-dir DIR] [--root-pub FILE] [--out manifest.json] [--sidecars DIR]
#
# --sidecars DIR also writes the files of a GitHub release (upload them next to the artifact):
#   <artifact>.minisig, signing-key-statement.txt[.minisig], <artifact>.release-statement.txt[.minisig]
#
# --keys defaults to $AZUL_RELEASE_KEYS, else ~/.azul-release-keys. A keys folder inside a git
# work tree must be ignored by git: the script refuses it otherwise.
#
# PREHASHING: the client accepts only prehashed signatures (`ED`), never the legacy form (`Ed`).
# Some minisign versions produce legacy signatures unless `-H` is passed. This script always
# passes `-H` (rsign2 always prehashes), and the self-check refuses the release if a legacy
# signature slipped through anyway. SIGN_RELEASE_SIGNER=rsign|minisign picks the tool when both
# are installed.
#
# THE KEY CEREMONY (do it once, before the first release; nothing in the tree generates a key):
#   1. On an offline machine: `init-root`. Choose a long passphrase. Make the backups the owner's
#      key policy asks for (RELEASES.md: two of three shares) - losing the root key means a new
#      binary for every user; leaking it means an attacker appoints their own signing key.
#   2. Put the public half (`root.pub`'s second line) into UPDATE_ROOT_KEYS in
#      examples/azul-appkit/src/anchors.rs - the current key and, while one is being replaced,
#      the next - and rebuild. Until then every build says "updates are not available in this
#      build", on purpose.
#   3. On the build machine: `init-signing`. Carry its signing.pub to the offline machine; there:
#      `statement --generation 1`; carry the two statement files back.
#   4. Per release: `release`. To retire a leaked signing key: a new signing key, `statement
#      --generation N+1`, and releases with `--min-generation N+1` (every client that installs one
#      refuses the retired key from then on, whether or not it ever saw the statement).

set -euo pipefail

die() { printf 'sign-release: %s\n' "$*" >&2; exit 1; }
note() { printf '  %s\n' "$*" >&2; }
usage() { sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; }

COMMAND="${1:-}"
case "$COMMAND" in
  init-root|init-signing|statement|release) shift ;;
  -h|--help|help) usage; exit 0 ;;
  "") usage >&2; exit 2 ;;
  -*) die "this script takes a command first now (init-root, init-signing, statement, release; --help). It no longer mints keys by itself: a key is minted on purpose, by init-root and init-signing" ;;
  *) die "unknown command: $COMMAND (init-root, init-signing, statement, release; --help)" ;;
esac

KEYS_DIR="${AZUL_RELEASE_KEYS:-$HOME/.azul-release-keys}"
UNENCRYPTED=0
GENERATION=""
MIN_GENERATION=""
EXPIRES_DAYS=365
OUT=""
STATEMENT_DIR=""
ROOT_PUB_FILE=""
APP=""
ARTIFACT=""
VERSION=""
URL=""
CHANGELOG=""
PLATFORM=""
SIDECARS=""

while [ $# -gt 0 ]; do
  case "$1" in
    --keys)           KEYS_DIR="${2:-}"; shift 2 ;;
    --unencrypted)    UNENCRYPTED=1; shift ;;
    --generation)     GENERATION="${2:-}"; shift 2 ;;
    --min-generation) MIN_GENERATION="${2:-}"; shift 2 ;;
    --expires-days)   EXPIRES_DAYS="${2:-}"; shift 2 ;;
    --out)            OUT="${2:-}"; shift 2 ;;
    --statement-dir)  STATEMENT_DIR="${2:-}"; shift 2 ;;
    --root-pub)       ROOT_PUB_FILE="${2:-}"; shift 2 ;;
    --app)            APP="${2:-}"; shift 2 ;;
    --artifact)       ARTIFACT="${2:-}"; shift 2 ;;
    --version)        VERSION="${2:-}"; shift 2 ;;
    --url)            URL="${2:-}"; shift 2 ;;
    --changelog)      CHANGELOG="${2:-}"; shift 2 ;;
    --platform)       PLATFORM="${2:-}"; shift 2 ;;
    --sidecars)       SIDECARS="${2:-}"; shift 2 ;;
    -h|--help)        usage; exit 0 ;;
    *)                die "unknown argument: $1" ;;
  esac
done

# Either CLI works. rsign2 is preferred where both exist: it ALWAYS prehashes (its -H flag is a
# no-op kept for compatibility), which is exactly what the client requires, whereas some minisign
# builds need -H to be passed. SIGN_RELEASE_SIGNER=rsign|minisign chooses one.
case "${SIGN_RELEASE_SIGNER:-}" in
  rsign|minisign)
    SIGNER="$SIGN_RELEASE_SIGNER"
    command -v "$SIGNER" >/dev/null 2>&1 || die "$SIGNER (SIGN_RELEASE_SIGNER) is not installed" ;;
  "")
    if command -v rsign >/dev/null 2>&1; then
      SIGNER=rsign
    elif command -v minisign >/dev/null 2>&1; then
      SIGNER=minisign
    else
      die "no signing tool found. Either: cargo install rsign2   (recommended)
                            or: apt install minisign / brew install minisign"
    fi ;;
  *) die "SIGN_RELEASE_SIGNER is rsign or minisign" ;;
esac

# The file names inside the keys folder. Set again when the folder's real path is known.
set_key_paths() {
  ROOT_SEC="$KEYS_DIR/root.key"
  ROOT_PUB="${ROOT_PUB_FILE:-$KEYS_DIR/root.pub}"
  SIGN_SEC="$KEYS_DIR/signing.key"
  SIGN_PUB="$KEYS_DIR/signing.pub"
  NO_PASSWORD_MARK="$KEYS_DIR/signing.unencrypted"
}
set_key_paths

# A keys folder in a git work tree must be ignored: a secret key that `git add -A` can take is a
# secret key that will be committed (REVIEW Y12). Checked before a key is minted AND before one is
# used, so a folder that was fine yesterday and is a tracked checkout today is noticed.
#   keys_dir_is_safe create   - make the folder (0700), then check
#   keys_dir_is_safe          - check only
keys_dir_is_safe() {
  if [ "${1:-}" = create ]; then
    mkdir -p "$KEYS_DIR"
    chmod 700 "$KEYS_DIR" 2>/dev/null || true
  fi
  [ -d "$KEYS_DIR" ] || return 0
  KEYS_DIR="$(cd "$KEYS_DIR" && pwd -P)"
  set_key_paths
  # Not inside a work tree (or no git on this machine): nothing to commit it to.
  git -C "$KEYS_DIR" rev-parse --show-toplevel >/dev/null 2>&1 || return 0
  # Ignored as a folder, or both secret key files ignored by name.
  if git -C "$KEYS_DIR" check-ignore -q -- "$KEYS_DIR" 2>/dev/null; then
    return 0
  fi
  if git -C "$KEYS_DIR" check-ignore -q -- "$ROOT_SEC" 2>/dev/null \
     && git -C "$KEYS_DIR" check-ignore -q -- "$SIGN_SEC" 2>/dev/null; then
    return 0
  fi
  die "$KEYS_DIR is inside a git work tree and git does not ignore it: keep the keys outside the tree (--keys DIR) or add the folder to .gitignore"
}

# gen_key <pubfile> <secfile> <encrypted:1|0> <comment>
gen_key() {
  local pub="$1" sec="$2" encrypted="$3" comment="$4"
  case "$SIGNER" in
    rsign)
      if [ "$encrypted" = 1 ]; then
        rsign generate -p "$pub" -s "$sec" -c "$comment"
      else
        rsign generate -p "$pub" -s "$sec" -W -c "$comment"
      fi ;;
    minisign)
      if [ "$encrypted" = 1 ]; then
        minisign -G -p "$pub" -s "$sec"
      else
        minisign -G -W -p "$pub" -s "$sec"
      fi ;;
  esac
}

# sign_file <secfile> <file> <sigfile> <untrusted-comment> <trusted-comment> <encrypted:1|0>
sign_file() {
  local sec="$1" file="$2" sig="$3" untrusted="$4" trusted="$5" encrypted="$6"
  case "$SIGNER" in
    rsign)
      if [ "$encrypted" = 1 ]; then
        rsign sign -s "$sec" -x "$sig" -c "$untrusted" -t "$trusted" "$file" >/dev/null
      else
        rsign sign -s "$sec" -x "$sig" -c "$untrusted" -t "$trusted" -W "$file" >/dev/null
      fi ;;
    # -H forces PREHASHED signatures; without it some minisign versions emit the legacy form,
    # which the client refuses. The self-check below is the backstop if a build ignores the flag.
    minisign) minisign -S -H -s "$sec" -m "$file" -x "$sig" -c "$untrusted" -t "$trusted" >/dev/null ;;
  esac
}

pub_b64() { tail -n +2 "$1" | tr -d '\n'; }

# The platform of this machine as the client names it: <os>-<arch> in Rust's words.
platform_of_this_machine() {
  local os arch
  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW*|MSYS*|CYGWIN*) os=windows ;;
    *) return 1 ;;
  esac
  case "$(uname -m)" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    i386|i686) arch=x86 ;;
    *) return 1 ;;
  esac
  printf '%s-%s' "$os" "$arch"
}

# ---------------------------------------------------------------------------- init-root
if [ "$COMMAND" = init-root ]; then
  keys_dir_is_safe create
  ROOT_PUB="$KEYS_DIR/root.pub"   # --root-pub names the key a release is checked against, not where one is minted
  [ ! -e "$ROOT_SEC" ] && [ ! -e "$ROOT_PUB" ] || die "a root key is already in $KEYS_DIR: refusing to overwrite it"
  note "minting the ROOT key pair (the secret key is encrypted: choose a passphrase)…"
  gen_key "$ROOT_PUB" "$ROOT_SEC" 1 "azul update ROOT key"
  chmod 600 "$ROOT_SEC" 2>/dev/null || true
  cat >&2 <<EOF

ROOT key pair written to $KEYS_DIR.

  * Move $ROOT_SEC to the offline machine and make the backups of your key policy NOW. It signs
    key statements only (\`statement\`), about once a year. Losing it means a new binary for
    every user; leaking it lets an attacker appoint their own signing key.
  * Put the PUBLIC key into the app. The Azlin apps: UPDATE_ROOT_KEYS in
    examples/azul-appkit/src/anchors.rs. Any other app: AppConfig.updates.root_public_key.

        $(pub_b64 "$ROOT_PUB")

EOF
  exit 0
fi

# ------------------------------------------------------------------------- init-signing
if [ "$COMMAND" = init-signing ]; then
  keys_dir_is_safe create
  [ ! -e "$SIGN_SEC" ] && [ ! -e "$SIGN_PUB" ] || die "a signing key is already in $KEYS_DIR: refusing to overwrite it"
  if [ "$UNENCRYPTED" = 1 ]; then
    note "minting the SIGNING key pair, UNENCRYPTED: it is only as safe as the machine that holds it"
    gen_key "$SIGN_PUB" "$SIGN_SEC" 0 "azul release signing key"
    : > "$NO_PASSWORD_MARK"
  else
    note "minting the SIGNING key pair (the secret key is encrypted: choose a passphrase)…"
    gen_key "$SIGN_PUB" "$SIGN_SEC" 1 "azul release signing key"
  fi
  chmod 600 "$SIGN_SEC" 2>/dev/null || true
  cat >&2 <<EOF

SIGNING key pair written to $KEYS_DIR.

  Carry $SIGN_PUB to the offline machine and run there:
      scripts/sign-release.sh statement --generation 1
  then bring signing-key-statement.txt and signing-key-statement.txt.minisig back.
  If this key ever leaks: mint a new one, \`statement --generation N+1\`, and release with
  --min-generation N+1.
EOF
  exit 0
fi

# ---------------------------------------------------------------------------- statement
if [ "$COMMAND" = statement ]; then
  keys_dir_is_safe
  [ -f "$ROOT_SEC" ] || die "no root key at $ROOT_SEC: run init-root on the offline machine"
  [ -f "$SIGN_PUB" ] || die "no signing key at $SIGN_PUB: copy the signing key's .pub here, or pass --keys"
  [ -n "$GENERATION" ] || die "--generation N is required (1 for the first signing key; one more for each replacement)"
  case "$GENERATION$EXPIRES_DAYS" in *[!0-9]*|"") die "--generation and --expires-days are whole numbers" ;; esac
  DEST="${OUT:-$KEYS_DIR}"
  mkdir -p "$DEST"
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  NOW="$(date -u +%s)"
  EXPIRES=$(( NOW + EXPIRES_DAYS * 86400 ))
  STATEMENT="azul-signing-key-v1|pubkey=$(pub_b64 "$SIGN_PUB")|expires=${EXPIRES}|generation=${GENERATION}"
  # EXACT BYTES, no trailing newline: the client verifies the statement string as it appears in
  # the manifest. `printf '%s'`, never `echo`.
  printf '%s' "$STATEMENT" > "$WORK/statement.txt"
  note "signing the key statement with the ROOT key ($SIGNER)…"
  sign_file "$ROOT_SEC" "$WORK/statement.txt" "$WORK/statement.minisig" \
      "azul signing-key statement" "generation $GENERATION, expires $EXPIRES" 1
  cp "$WORK/statement.txt" "$DEST/signing-key-statement.txt"
  cp "$WORK/statement.minisig" "$DEST/signing-key-statement.txt.minisig"
  note "written: $DEST/signing-key-statement.txt and .minisig (generation $GENERATION, expires in $EXPIRES_DAYS days)"
  exit 0
fi

# ----------------------------------------------------------------------------- release
keys_dir_is_safe
[ -f "$SIGN_SEC" ] || die "no signing key at $SIGN_SEC: run init-signing"
[ -n "$APP" ]      || die "--app is required (the app's key, e.g. azwriter: the release statement names it)"
[ -n "$ARTIFACT" ] || die "--artifact is required"
[ -f "$ARTIFACT" ] || die "artifact not found: $ARTIFACT"
# A build made with the cargo feature `test-hooks` takes its update anchor, its endpoints and the token
# server a drive token goes to from files, variables and switches: nothing signed and shipped may be
# one (cargo unifies features across a workspace build, so a binary can be one by accident). Such a
# build holds this marker (azul-appkit's anchors::TEST_HOOKS_MARKER; scripts/check_no_test_hooks.py).
if grep -a -q -F 'AZLIN-BUILD-MARKER:test-hooks:' "$ARTIFACT"; then
  die "$ARTIFACT is a test build (it holds the test-hooks marker): build it without the feature test-hooks"
fi
[ -n "$VERSION" ]  || die "--version is required"
[ -n "$URL" ]      || die "--url is required (where clients download the artifact)"
if [ -z "$PLATFORM" ]; then
  PLATFORM="$(platform_of_this_machine)" || die "cannot tell this machine's platform: pass --platform <os-arch> (linux-x86_64, macos-aarch64, windows-x86_64)"
fi
case "$APP$VERSION$PLATFORM" in *'|'*|*[[:space:]]*) die "--app, --version and --platform must not contain '|' or white space" ;; esac
STATEMENT_DIR="${STATEMENT_DIR:-$KEYS_DIR}"
KEY_STATEMENT_FILE="$STATEMENT_DIR/signing-key-statement.txt"
KEY_STATEMENT_SIG_FILE="$STATEMENT_DIR/signing-key-statement.txt.minisig"
[ -f "$KEY_STATEMENT_FILE" ] && [ -f "$KEY_STATEMENT_SIG_FILE" ] \
  || die "no signed key statement in $STATEMENT_DIR (signing-key-statement.txt and .minisig): the ROOT key makes them with \`statement\`, offline"
[ -f "$ROOT_PUB" ] || die "no root public key at $ROOT_PUB (--root-pub FILE): the self-check needs it"

KEY_STATEMENT="$(cat "$KEY_STATEMENT_FILE")"
KEY_GENERATION="$(printf '%s' "$KEY_STATEMENT" | tr '|' '\n' | sed -n 's/^generation=//p')"
case "$KEY_GENERATION" in ""|*[!0-9]*) die "the key statement has no generation" ;; esac
MIN_GENERATION="${MIN_GENERATION:-$KEY_GENERATION}"
case "$MIN_GENERATION" in ""|*[!0-9]*) die "--min-generation is a whole number" ;; esac
[ "$MIN_GENERATION" -le "$KEY_GENERATION" ] \
  || die "--min-generation $MIN_GENERATION is above the key statement's generation $KEY_GENERATION: no client would accept the release"

SIGN_ENCRYPTED=1
[ -f "$NO_PASSWORD_MARK" ] && SIGN_ENCRYPTED=0

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ------------------------------------------------------------------- digest and statement
if command -v sha256sum >/dev/null 2>&1; then
  DIGEST="$(sha256sum "$ARTIFACT" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
  DIGEST="$(shasum -a 256 "$ARTIFACT" | cut -d' ' -f1)"
else
  die "neither sha256sum nor shasum found"
fi
NOW="$(date -u +%s)"
# EXACT BYTES again: printf '%s', no newline.
RELEASE_STATEMENT="azul-release-v1|app=${APP}|version=${VERSION}|platform=${PLATFORM}|sha256=${DIGEST}|released=${NOW}|min_generation=${MIN_GENERATION}"
printf '%s' "$RELEASE_STATEMENT" > "$WORK/release-statement.txt"

note "signing the artifact with the SIGNING key ($SIGNER)…"
sign_file "$SIGN_SEC" "$ARTIFACT" "$WORK/artifact.minisig" \
    "azul release $VERSION" "azul release $VERSION" "$SIGN_ENCRYPTED"
note "signing the release statement with the SIGNING key…"
sign_file "$SIGN_SEC" "$WORK/release-statement.txt" "$WORK/release-statement.minisig" \
    "azul release statement" "app $APP, version $VERSION, platform $PLATFORM" "$SIGN_ENCRYPTED"

# ------------------------------------------------------------------------------ manifest
# python3 does the JSON so multi-line signature blocks are escaped correctly. The manifest is
# written to the work folder and moved to --out only after it verified.
MANIFEST="$WORK/manifest.json"
STATEMENT="$KEY_STATEMENT" RELEASE_STATEMENT="$RELEASE_STATEMENT" \
VERSION="$VERSION" URL="$URL" CHANGELOG="$CHANGELOG" DIGEST="$DIGEST" \
ART_SIG_FILE="$WORK/artifact.minisig" STMT_SIG_FILE="$KEY_STATEMENT_SIG_FILE" \
REL_SIG_FILE="$WORK/release-statement.minisig" \
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
        "release_statement": os.environ["RELEASE_STATEMENT"],
        "release_statement_sig": read(os.environ["REL_SIG_FILE"]),
    }
}
with open(sys.argv[1], "w", encoding="utf-8") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY

# ------------------------------------------------------------------------- self-check
# The client's own code decides. SIGN_RELEASE_VERIFY names another command (the tests of this
# script use it); a real release never does.
note "verifying the result the way a CLIENT will…"
ROOT_PUB_B64="$(pub_b64 "$ROOT_PUB")"
if [ -n "${SIGN_RELEASE_VERIFY:-}" ]; then
  VERIFY=("$SIGN_RELEASE_VERIFY")
else
  VERIFY=(cargo run -q --release --manifest-path "$REPO_ROOT/Cargo.toml"
          -p azul-layout --features updater --example verify_update_manifest --)
fi
if ! "${VERIFY[@]}" "$MANIFEST" "$ARTIFACT" "$ROOT_PUB_B64" --app "$APP" --platform "$PLATFORM"; then
  cat >&2 <<'EOF'

The release did NOT verify. Do not publish it.

If the failure names the artifact or a statement signature, the most likely cause is a LEGACY
(non-prehashed) minisign signature: check that your minisign build honours -H, or use
`cargo install rsign2`, which always prehashes.
EOF
  exit 1
fi

# The files of a GitHub release (the updater reads them as sibling assets of the artifact). Only a
# release that verified leaves them; the release statement is per asset because it names one
# platform and one hash.
if [ -n "$SIDECARS" ]; then
  mkdir -p "$SIDECARS"
  ASSET="$(basename "$ARTIFACT")"
  cp "$WORK/artifact.minisig" "$SIDECARS/$ASSET.minisig"
  cp "$KEY_STATEMENT_FILE" "$SIDECARS/signing-key-statement.txt"
  cp "$KEY_STATEMENT_SIG_FILE" "$SIDECARS/signing-key-statement.txt.minisig"
  cp "$WORK/release-statement.txt" "$SIDECARS/$ASSET.release-statement.txt"
  cp "$WORK/release-statement.minisig" "$SIDECARS/$ASSET.release-statement.txt.minisig"
  note "GitHub release assets written to $SIDECARS (upload them next to $ASSET)"
fi

if [ -n "$OUT" ]; then
  cp "$MANIFEST" "$OUT"
  note "manifest written to $OUT"
else
  echo
  cat "$MANIFEST"
fi

cat >&2 <<EOF

Publish the manifest at the app's update source, and the artifact at:
  $URL

This release is for app "$APP" on platform "$PLATFORM"; publish one manifest (and one artifact)
per platform. Clients whose compiled-in root key is
  $ROOT_PUB_B64
will accept it. (The Azlin apps: UPDATE_ROOT_KEYS in examples/azul-appkit/src/anchors.rs.)
EOF

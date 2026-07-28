#!/usr/bin/env bash
# Run the install command azul.rs DOCUMENTS, as a user would, against a live
# site — and fail if it does not install azul.
#
# WHY THIS EXISTS
# ---------------
# The deploy already asserts that every channel's index FILE exists and that the
# links on it resolve to files we host (the `need_file`/`need_glob` block in
# .github/workflows/rust.yml). That is necessary and not sufficient: a perfectly
# well-formed PEP 503 page can still be uninstallable, and this repo has shipped
# exactly that, repeatedly, behind a green deploy:
#
#   * a python wheel that 404'd from the index the docs point at
#   * /ui/gems serving a bare .gem with no spec index for two months
#   * a NuGet flat container keyed on `azul` while the package's own
#     <PackageId> is `Azul.Net`, so every file resolved and `dotnet add
#     package azul` died on "Expected package azul 0.2.0, but got package
#     Azul.Net 0.2.0" — found by writing this script and running it
#   * demo binaries published without an execute bit
#
# Every one of those passes a file-exists check. None of them passes
# `pip install azul --index-url https://azul.rs/ui`. So run the command.
#
# Usage:
#   verify_install_commands.sh <channel> [base_url] [version] [digest_file]
#   verify_install_commands.sh --emit-digests <site_dir> <version>
#   verify_install_commands.sh --check-nuget-layout <site_dir>
#
#   channel      pypi | npm | gems | nuget | brew | apt
#   base_url     default https://azul.rs (override to point at a staging tree,
#                e.g. http://127.0.0.1:8099 in front of a local `website/`)
#   version      default 0.2.0
#   digest_file  TSV of `channel <TAB> site-relative-path <TAB> sha256`, as
#                produced by --emit-digests over the staged tree. When given,
#                the channel's entry point on the live site must serve EXACTLY
#                those bytes before the install command runs — otherwise a run
#                can pass against a previous deploy's copy still in the CDN and
#                report "installable" about bytes it never published. Pass `-`
#                to skip that (standalone/manual use); CI always passes a file.
#
# Each channel is meant to run in a FRESH container that has that channel's
# client and nothing of azul — see the verify_install_* jobs in rust.yml. The
# script installs nothing but the client's own documented prerequisites.
#
# Exit status is the whole point: 0 = the documented command installed azul at
# the expected version, non-zero = it did not, and the release is not a release.

set -uo pipefail

# Poll budget for "the bytes we just published are the bytes the CDN serves".
# GitHub Pages is normally consistent by the time actions/deploy-pages reports
# success; this is slack for the edge cases, not an expectation.
FRESHNESS_TIMEOUT="${AZUL_VERIFY_TIMEOUT:-600}"
FRESHNESS_INTERVAL="${AZUL_VERIFY_INTERVAL:-10}"

# The NuGet package id is `Azul.Net` (<PackageId> in
# doc/src/codegen/v2/lang_csharp/csproj.rs), NOT `azul`; NuGet rejects a .nupkg
# whose embedded id differs from the requested one, so the two must agree.
# Only used when no digest file is supplied — with one, the id is read back out
# of the flat-container path that the deploy actually laid down, so the tree is
# the source of truth and this default cannot drift silently in CI.
DEFAULT_NUGET_ID="azul.net"

log()  { printf '\n=== %s\n' "$*"; }
note() { printf '    %s\n' "$*"; }
fail() { printf '::error::%s\n' "$*" >&2; exit 1; }

# Run a command, echoing it first — the log should show the reader the exact
# command line a user would type.
run() {
  printf '\n$ %s\n' "$*"
  "$@"
}

# macOS runners (the brew channel) ship `shasum`, not GNU `sha256sum`.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

lc() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# --------------------------------------------------------------------------
# Entry point = the first file the channel's client fetches, and the one file
# whose bytes change on every deploy. Used both to emit digests from the staged
# tree and (fallback) to poll the live site.
# --------------------------------------------------------------------------
entry_path() { # $1 channel, $2 version
  local v="$2" vlc; vlc="$(lc "$2")"
  case "$1" in
    pypi)  printf 'ui/azul/index.html' ;;
    npm)   printf 'ui/npm/azul-%s.tgz' "$v" ;;
    gems)  printf 'ui/gems/latest_specs.4.8.gz' ;;
    nuget) printf 'ui/nuget/flatcontainer/%s/%s/%s.%s.nupkg' \
                  "$DEFAULT_NUGET_ID" "$vlc" "$DEFAULT_NUGET_ID" "$vlc" ;;
    brew)  printf 'ui/homebrew-azul.git/info/refs' ;;
    apt)   printf 'ui/apt/dists/stable/Release' ;;
    *)     return 1 ;;
  esac
}

# --------------------------------------------------------------------------
# --emit-digests <site_dir> <version>
# Print `channel <TAB> path <TAB> sha256` for every channel present in the
# staged tree. Absent channels print nothing; the caller decides whether a
# missing channel is acceptable (in CI it is not — the site advertises all six).
# --------------------------------------------------------------------------
emit_digests() {
  local site="${1:?site dir}" v="${2:?version}"
  [ -d "$site" ] || fail "--emit-digests: $site is not a directory"
  local ch p
  for ch in pypi npm gems brew apt; do
    p="$(entry_path "$ch" "$v")"
    [ -s "$site/$p" ] || continue
    printf '%s\t%s\t%s\n' "$ch" "$p" "$(sha256_of "$site/$p")"
  done
  # NuGet: read the id + version straight off the flat container the deploy
  # actually built rather than assuming it, so a package-id change shows up
  # here instead of turning into a 30-minute-later mystery. `libazul` is the
  # Chocolatey package sharing this feed — not the .NET binding.
  local nupkg
  for nupkg in "$site"/ui/nuget/flatcontainer/*/*/*.nupkg; do
    [ -s "$nupkg" ] || continue
    case "$(basename "$nupkg")" in libazul.*) continue;; esac
    p="${nupkg#"$site"/}"
    printf 'nuget\t%s\t%s\n' "$p" "$(sha256_of "$nupkg")"
    break
  done
}

# --------------------------------------------------------------------------
# --check-nuget-layout <site_dir>
# NuGet re-reads the <id>/<version> out of every .nupkg it downloads and refuses
# one whose embedded id differs from the flat-container path it came from:
#
#   The nupkg at .../flatcontainer/azul/0.2.0/azul.0.2.0.nupkg is not valid.
#     Expected package azul 0.2.0, but got package Azul.Net 0.2.0
#
# The mirror published the binding under `azul` while <PackageId> in
# doc/src/codegen/v2/lang_csharp/csproj.rs is `Azul.Net`. Every file resolved,
# the service index was well-formed, and `dotnet add package` could not work for
# anybody. Assert the layout rule against each package's OWN metadata, so this
# cannot come back under a different name — and assert it BEFORE the deploy, so
# it costs a red build instead of a published release.
# --------------------------------------------------------------------------
check_nuget_layout() {
  local site="${1:?site dir}"
  need_cmd python3
  python3 - "$site" <<'PY'
import glob, os, re, sys, zipfile

site = sys.argv[1]
root = os.path.join(site, "ui", "nuget", "flatcontainer")
pkgs = sorted(glob.glob(os.path.join(root, "*", "*", "*.nupkg")))
if not pkgs:
    print("::error::nuget: no .nupkg under %s" % root)
    sys.exit(1)

bad = []
for p in pkgs:
    dir_id, dir_ver, base = p.split(os.sep)[-3:]
    with zipfile.ZipFile(p) as z:
        names = [n for n in z.namelist() if n.endswith(".nuspec") and "/" not in n]
        if not names:
            bad.append("%s: no .nuspec at the package root" % p)
            continue
        xml = z.read(names[0]).decode("utf-8", "replace")

    def tag(t, xml=xml):
        # <id>/<version> are ELEMENTS only inside <metadata>; a <dependency>
        # carries them as attributes, so an element match is unambiguous.
        m = re.search(r"<%s>\s*([^<]+?)\s*</%s>" % (t, t), xml)
        return m.group(1) if m else ""

    pid, pver = tag("id").lower(), tag("version").lower()
    if not pid or not pver:
        bad.append("%s: .nuspec has no <id>/<version> element" % p)
        continue
    want = "%s/%s/%s.%s.nupkg" % (pid, pver, pid, pver)
    got = "%s/%s/%s" % (dir_id, dir_ver, base)
    if want != got:
        bad.append("the package is %s %s, so NuGet fetches it from "
                   "flatcontainer/%s - it is published at flatcontainer/%s"
                   % (pid, pver, want, got))
    else:
        print("    [nuget] %s %s serves from flatcontainer/%s" % (pid, pver, got))

for b in bad:
    print("::error::nuget: %s" % b)
sys.exit(1 if bad else 0)
PY
}

# --------------------------------------------------------------------------
# Freshness: block until <base>/<path> serves the sha256 we published.
# --------------------------------------------------------------------------
resolve_from_digests() { # $1 channel, $2 digest file -> sets ENTRY_PATH/ENTRY_SHA
  local ch="$1" file="$2" line
  line="$(awk -F'\t' -v c="$ch" '$1 == c { print; exit }' "$file")"
  [ -n "$line" ] || fail "$ch: this deploy published no $ch mirror, but the site's install docs tell users to run the $ch command — that command has nothing behind it"
  ENTRY_PATH="$(printf '%s' "$line" | cut -f2)"
  ENTRY_SHA="$(printf '%s' "$line" | cut -f3)"
  [ -n "$ENTRY_PATH" ] && [ -n "$ENTRY_SHA" ] || fail "$ch: malformed digest line: $line"
}

await_published_bytes() { # $1 channel
  local ch="$1" url="$BASE/$ENTRY_PATH" tmp got started deadline
  tmp="$(mktemp)"
  started="$(date +%s)"
  deadline=$(( started + FRESHNESS_TIMEOUT ))
  log "$ch: waiting for $url to serve the bytes this deploy published"
  note "expected sha256 $ENTRY_SHA"
  while :; do
    if curl -fsSL --max-time 300 -o "$tmp" "$url"; then
      got="$(sha256_of "$tmp")"
      if [ "$got" = "$ENTRY_SHA" ]; then
        note "match after $(( $(date +%s) - started ))s"
        rm -f "$tmp"
        return 0
      fi
      note "serving sha256 $got ($(wc -c < "$tmp") bytes) — not this deploy's copy yet"
    else
      note "not fetchable yet"
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      rm -f "$tmp"
      fail "$ch: $url never served this deploy's bytes within ${FRESHNESS_TIMEOUT}s — the published $ch index is missing or stale, so the documented command would install the wrong thing (or nothing)"
    fi
    sleep "$FRESHNESS_INTERVAL"
  done
}

need_cmd() { command -v "$1" >/dev/null 2>&1 || fail "${CHANNEL:-verify}: '$1' is not installed in this environment — the check cannot run the documented command and must not report success"; }

# --------------------------------------------------------------------------
# pypi —  pip install azul --index-url https://azul.rs/ui
#
# Identity comes for free: the PEP 503 page carries `#sha256=` per distribution
# and pip verifies the download against it. Once the page is byte-matched to
# what we published, the wheel pip installs is our wheel by construction.
# --------------------------------------------------------------------------
verify_pypi() {
  need_cmd pip
  need_cmd python
  run pip install azul --index-url "$BASE/ui" \
    || fail "pypi: 'pip install azul --index-url $BASE/ui' FAILED — the site tells every Python user to run exactly this"
  local got
  got="$(pip show azul 2>/dev/null | awk '/^Version:/ { print $2 }')"
  [ "$got" = "$VERSION" ] \
    || fail "pypi: installed azul $got, the release is $VERSION — the index resolved to something other than this release"
  # The wheel has to do more than unpack. `import azul` is where this binding
  # has actually broken before (abi3 Py* stub SIGILL, pyclass dealloc SIGSEGV),
  # and a create/del round-trip is where the codegen double-drop showed up.
  run env AZ_BACKEND=headless python -c \
    "import azul; c = azul.ColorU.create(1, 2, 3, 4); del c; a = azul.AppConfig.create(); del a; print('import + pyclass teardown OK')" \
    || fail "pypi: azul $VERSION installs from the documented index but does not import"
  log "pypi: 'pip install azul --index-url $BASE/ui' installs and imports azul $VERSION"
}

# --------------------------------------------------------------------------
# npm —  npm install https://azul.rs/ui/npm/azul-<V>.tgz
#
# The docs give the flat tarball URL (doc/guide/en/hello-world/node.md), which
# is also the entry point we byte-match, so identity is proven directly.
# --------------------------------------------------------------------------
verify_npm() {
  need_cmd npm
  need_cmd node
  local w; w="$(mktemp -d)"
  cd "$w" || fail "npm: cannot enter $w"
  npm init -y >/dev/null 2>&1 || fail "npm: 'npm init' failed in a clean directory"
  run npm install "$BASE/ui/npm/azul-$VERSION.tgz" \
    || fail "npm: 'npm install $BASE/ui/npm/azul-$VERSION.tgz' FAILED — the site tells every Node user to run exactly this"
  local got
  got="$(node -p "require('$w/node_modules/azul/package.json').version" 2>/dev/null)"
  [ "$got" = "$VERSION" ] \
    || fail "npm: installed azul $got, the release is $VERSION"
  run env AZ_BACKEND=headless node -e "require('azul'); console.log('require OK')" \
    || fail "npm: azul $VERSION installs from the documented tarball URL but does not require()"
  log "npm: 'npm install $BASE/ui/npm/azul-$VERSION.tgz' installs and loads azul $VERSION"
}

# --------------------------------------------------------------------------
# gems —  gem install azul --clear-sources --source https://azul.rs/ui/gems
#
# `ffi` is a stated prerequisite of the Ruby binding ("You need Ruby 2.6+, the
# ffi gem, and the native libazul" — doc/guide/en/hello-world/ruby.md) and the
# gem declares it as a runtime dependency. --clear-sources means our mirror is
# the ONLY source, and the mirror hosts azul alone, so ffi has to already be
# present for the documented command to resolve. Install it first, from
# rubygems.org, exactly as the guide says to.
#
# --clear-sources is also what makes the identity check unnecessary here: with
# rubygems.org dropped, anything that installs came from our mirror. Without it
# RubyGems would happily install the unrelated `azul` gem that exists upstream
# and this check would pass while the mirror was dead.
# --------------------------------------------------------------------------
verify_gems() {
  need_cmd gem
  need_cmd ruby
  run gem install ffi --no-document \
    || fail "gems: could not install the 'ffi' prerequisite from rubygems.org"
  run gem install azul --clear-sources --source "$BASE/ui/gems" --no-document \
    || fail "gems: 'gem install azul --clear-sources --source $BASE/ui/gems' FAILED — the site tells every Ruby user to run exactly this"
  gem list -i azul -v "$VERSION" >/dev/null 2>&1 \
    || fail "gems: after a reportedly successful install, 'gem list -i azul -v $VERSION' says azul $VERSION is not installed"
  run env AZ_BACKEND=headless ruby -e "require 'azul'; puts 'require OK'" \
    || fail "gems: azul $VERSION installs from the documented source but 'require \"azul\"' fails"
  log "gems: 'gem install azul --clear-sources --source $BASE/ui/gems' installs and loads azul $VERSION"
}

# --------------------------------------------------------------------------
# nuget —  dotnet nuget add source https://azul.rs/ui/nuget/index.json --name azul
#          dotnet add package <id> --version <V>
#
# `dotnet add package` keeps nuget.org in the source list (the documented
# command does not clear it), so a same-named package upstream could satisfy
# the restore and make this pass while our feed was dead. Close that by
# comparing the .nupkg that landed in the global packages folder against the
# sha256 we published.
# --------------------------------------------------------------------------
verify_nuget() {
  need_cmd dotnet
  export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
  # The id and version live in the flat-container path the deploy laid down:
  # ui/nuget/flatcontainer/<id>/<version>/<id>.<version>.nupkg — taking them
  # from the published tree rather than from a constant means a package-id
  # change is caught here instead of turning into a "not valid" download error
  # for users. The protocol lowercases the path; NuGet ids are case-insensitive,
  # so `azul.net` here is the same package the docs spell `Azul.Net`.
  local rest id ver
  rest="${ENTRY_PATH#ui/nuget/flatcontainer/}"
  id="${rest%%/*}"; rest="${rest#*/}"; ver="${rest%%/*}"
  [ -n "$id" ] && [ -n "$ver" ] \
    || fail "nuget: cannot read the package id/version out of '$ENTRY_PATH'"
  [ "$ver" = "$(lc "$VERSION")" ] \
    || fail "nuget: the feed hosts $id $ver but the release is $VERSION"

  local w; w="$(mktemp -d)"
  cd "$w" || fail "nuget: cannot enter $w"
  # A project to add the package to; the package targets net10.0
  # (<TargetFramework> in doc/src/codegen/v2/lang_csharp/csproj.rs), so an
  # older SDK's default template would fail restore for a framework reason
  # rather than a channel reason.
  run dotnet new console -o app --force >/dev/null \
    || fail "nuget: 'dotnet new console' failed — the .NET SDK in this environment is unusable"
  cd "$w/app" || fail "nuget: cannot enter $w/app"
  run dotnet nuget add source "$BASE/ui/nuget/index.json" --name azul \
    || fail "nuget: 'dotnet nuget add source $BASE/ui/nuget/index.json --name azul' FAILED"
  run dotnet add package "$id" --version "$VERSION" \
    || fail "nuget: 'dotnet add package $id --version $VERSION' FAILED — the site tells every C# user to run exactly this against $BASE/ui/nuget/index.json"

  local cached="$HOME/.nuget/packages/$(lc "$id")/$ver/$(lc "$id").$ver.nupkg"
  [ -s "$cached" ] || fail "nuget: restore reported success but $cached is not there"
  if [ -n "${ENTRY_SHA:-}" ]; then
    local got; got="$(sha256_of "$cached")"
    [ "$got" = "$ENTRY_SHA" ] \
      || fail "nuget: restored a $id $VERSION whose sha256 is $got, not the $ENTRY_SHA we published — it came from another feed, so this proves nothing about $BASE/ui/nuget"
  fi
  [ -s "$HOME/.nuget/packages/$(lc "$id")/$ver/runtimes/linux-x64/native/libazul.so" ] \
    || fail "nuget: the restored package carries no runtimes/linux-x64/native/libazul.so — the binding cannot P/Invoke anything"
  log "nuget: 'dotnet add package $id --version $VERSION' restores our $id $VERSION from $BASE/ui/nuget"
}

# --------------------------------------------------------------------------
# brew —  brew tap fschutt/azul https://azul.rs/ui/homebrew-azul.git
#         brew install fschutt/azul/azul
#
# The tap is a bare git repo served over dumb HTTP from Pages; `brew tap` with
# an explicit URL clones it. The formula then downloads libazul.dylib + azul.h
# from their azul.rs URLs and checks the sha256 baked in at deploy time, so a
# successful install also proves those release files match the formula.
# macOS only: the formula body is `on_macos`, so on Linux `brew install` would
# install nothing and "succeed".
# --------------------------------------------------------------------------
verify_brew() {
  need_cmd brew
  [ "$(uname -s)" = "Darwin" ] \
    || fail "brew: the azul formula is macOS-only (on_macos); running it elsewhere would install nothing and report success"
  # A leftover tap from a previous run would hide a broken publish.
  brew untap fschutt/azul >/dev/null 2>&1 || true
  brew uninstall --force azul >/dev/null 2>&1 || true
  run brew tap fschutt/azul "$BASE/ui/homebrew-azul.git" \
    || fail "brew: 'brew tap fschutt/azul $BASE/ui/homebrew-azul.git' FAILED — the published tap is not clonable"
  run brew install fschutt/azul/azul \
    || fail "brew: 'brew install fschutt/azul/azul' FAILED — the site tells every macOS user to run exactly this"
  local prefix; prefix="$(brew --prefix azul 2>/dev/null)"
  [ -n "$prefix" ] || fail "brew: 'brew --prefix azul' is empty after a reportedly successful install"
  [ -s "$prefix/lib/libazul.dylib" ] \
    || fail "brew: the formula installed but $prefix/lib/libazul.dylib is missing — nothing to link against"
  [ -s "$prefix/include/azul.h" ] \
    || fail "brew: the formula installed but $prefix/include/azul.h is missing — nothing to compile against"
  local vers; vers="$(brew list --versions azul 2>/dev/null)"
  case " $vers " in
    *" $VERSION "*) ;;
    *) fail "brew: 'brew list --versions azul' reports '$vers', not $VERSION" ;;
  esac
  log "brew: the tap installs libazul.dylib + azul.h for azul $VERSION into $prefix"
}

# --------------------------------------------------------------------------
# apt —  echo 'deb [trusted=yes] https://azul.rs/ui/apt stable main' | \
#          sudo tee /etc/apt/sources.list.d/azul.list
#        sudo apt update && sudo apt install azul
#
# Run as root in the container, so no `sudo` (installing sudo just to drop it
# again would test the container, not the repository).
# --------------------------------------------------------------------------
verify_apt() {
  need_cmd apt-get
  [ "$(id -u)" = "0" ] || fail "apt: this check must run as root (it writes /etc/apt/sources.list.d)"
  export DEBIAN_FRONTEND=noninteractive
  printf 'deb [trusted=yes] %s/ui/apt stable main\n' "$BASE" > /etc/apt/sources.list.d/azul.list
  run apt-get update \
    || fail "apt: 'apt update' FAILED with $BASE/ui/apt in sources.list.d — the published repository metadata is unreadable"
  run apt-get install -y azul \
    || fail "apt: 'apt install azul' FAILED — the site tells every Debian/Ubuntu user to run exactly this"
  local got
  got="$(dpkg-query -W -f='${Version}' azul 2>/dev/null)"
  [ "$got" = "$VERSION" ] \
    || fail "apt: installed azul $got, the release is $VERSION"
  # `[trusted=yes]` means apt will take this package from anywhere it is
  # offered; prove it actually came from the azul.rs repository.
  apt-cache policy azul 2>/dev/null | grep -q "$(printf '%s' "$BASE" | sed 's#^https\?://##')" \
    || fail "apt: azul $VERSION installed, but apt-cache policy does not show $BASE as its origin"
  [ -s /usr/lib/libazul.so ] \
    || fail "apt: the package installed but /usr/lib/libazul.so is missing — nothing to link against"
  [ -s /usr/include/azul.h ] \
    || fail "apt: the package installed but /usr/include/azul.h is missing — nothing to compile against"
  log "apt: the documented sources.list line installs azul $VERSION (libazul.so + azul.h)"
}

# --------------------------------------------------------------------------
main() {
  if [ "${1:-}" = "--emit-digests" ]; then
    shift
    emit_digests "$@"
    exit 0
  fi
  if [ "${1:-}" = "--check-nuget-layout" ]; then
    shift
    check_nuget_layout "$@"
    exit $?
  fi

  CHANNEL="${1:?channel: pypi|npm|gems|nuget|brew|apt}"
  BASE="${2:-https://azul.rs}"
  VERSION="${3:-0.2.0}"
  DIGESTS="${4:--}"
  BASE="${BASE%/}"

  need_cmd curl
  command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 \
    || fail "$CHANNEL: neither sha256sum nor shasum is available — this check cannot tell one build's bytes from another's"

  case "$CHANNEL" in
    pypi|npm|gems|nuget|brew|apt) ;;
    *) fail "unknown channel '$CHANNEL' (pypi|npm|gems|nuget|brew|apt)" ;;
  esac

  if [ "$DIGESTS" != "-" ]; then
    [ -s "$DIGESTS" ] || fail "$CHANNEL: digest file '$DIGESTS' is missing or empty — without it this check cannot tell this deploy's bytes from a previous one's"
    resolve_from_digests "$CHANNEL" "$DIGESTS"
    await_published_bytes "$CHANNEL"
  else
    ENTRY_PATH="$(entry_path "$CHANNEL" "$VERSION")"
    ENTRY_SHA=""
    printf '\n::notice::%s: no digest file given — running the documented command against whatever %s currently serves, without proving those bytes are this deploy'"'"'s\n' \
      "$CHANNEL" "$BASE"
  fi

  log "$CHANNEL: running the documented install command against $BASE"
  "verify_$CHANNEL"
}

main "$@"

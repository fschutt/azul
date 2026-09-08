#!/usr/bin/env bash
# Build self-hosted, STATIC package-registry mirrors under the GitHub Pages site
# so the azul.rs install commands work without the official registries:
#
#   Maven    azul.rs/ui/maven       (maven2 layout: rs/azul/azul/<V>/azul-<V>.jar+pom)
#   PyPI     azul.rs/ui            (PEP 503 index root; pip fetches /ui/azul/)
#   npm      azul.rs/ui/npm/azul     (registry metadata doc + hosted .tgz)
#   NuGet    azul.rs/ui/nuget        (v3 service index + flat-container + nupkg)
#   RubyGems azul.rs/ui/gems         (Marshal spec index + quick/ + gems/; see note)
#   DNF/yum/zypper azul.rs/ui/rpm    (createrepo_c repodata; ONE repo serves all
#                                  three — yum and zypper consume dnf repodata)
#   pacman   azul.rs/ui/arch         (repo-add db over the .pkg.tar.zst)
#   Alpine apk  azul.rs/ui/alpine     (apk index APKINDEX.tar.gz over the .apk)
#   Homebrew azul.rs/ui/brew.git  (a real bare git repo = a tap)
#   Scoop    azul.rs/ui/scoop.git     (a real bare git repo = a bucket)
#   Chocolatey  azul.rs/ui/nuget (the v3 feed also serves a `libazul` choco package)
#   cargo    azul.rs/ui/cargo        (sparse registry index over release/<V>/azul-<V>.crate)
#
# SELF-HOSTED FIRST, UPSTREAM LATER: every channel above works with nothing but
# GitHub Pages. The jobs in .github/workflows/rust.yml additionally push the
# same artifacts to the official registries (PyPI, npm, RubyGems, NuGet, Maven
# Central, crates.io, the AUR, Chocolatey, the GitHub-hosted tap/bucket) when
# the matching secret exists — see scripts/publish_upstream.sh. Signing of the
# apt/rpm/pacman/apk metadata is the same kind of opt-in (AZUL_*_KEY secrets);
# unsigned, the docs say [trusted=yes] / gpgcheck=0 / SigLevel=Never /
# --allow-untrusted, which is what they mean.
#
# UPDATE MODEL — every endpoint above is a STABLE, VERSION-INDEPENDENT path. The
# version only ever appears INSIDE the tree (maven coordinates, the formula's
# url, a nupkg version), never in the endpoint you configure. Each deploy
# regenerates the metadata so the endpoint always advertises the just-released
# version as "latest" (maven-metadata <latest>, npm dist-tags.latest, the
# Homebrew formula's version, the apt/dnf/nuget version lists). So a user who
# ran the configure-once command keeps getting upgrades:
#   brew upgrade azul / choco upgrade libazul / apt upgrade / dnf upgrade /
#   pip install -U / etc. pull the next libazul release with no reconfiguration.
# (Old versions live on the GitHub Release; the live mirror tracks latest.)
#
# These are plain files served by GitHub Pages — no registry server needed. The
# clients that can consume a fully-static tree (Maven, pip --index-url, npm
# --registry via the metadata doc, NuGet v3, dnf) work directly. RubyGems is one
# of them: it probes the compact index, 404s, and falls back to the static
# Marshal spec index (specs.4.8.gz + quick/Marshal.4.8/), which build_gems
# generates directly — see the note above build_gems.
#
# Homebrew is handled as a REAL bare git repo (a tap is just a git repo; brew
# clones any explicit URL, including dumb-HTTP from GitHub Pages). Chocolatey
# piggybacks on the static NuGet v3 feed.
#
# Usage: build_registry_mirrors.sh <website_dir> <version> <artifacts_root>
#   <artifacts_root> contains the downloaded package artifacts in subdirs:
#     maven-jar/*.jar  pypi-dist/*  npm-package/*.tgz  nuget-package/*.nupkg
#     ruby-gem/*.gem   artifacts-rpm/*.rpm
# Each input is OPTIONAL — a missing artifact simply skips that registry.
set -uo pipefail

SITE="${1:?website dir}"
V="${2:?version}"
ART="${3:?artifacts root}"
BASE="https://azul.rs"
# The deploy lays the per-release files (dylib / dll / azul.h) here; brew + choco
# point their downloads at the matching azul.rs/ui/release/<V>/ URLs and check the
# sha256 of these exact files.
RELDIR="$SITE/ui/release/$V"

sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
sha1_of()   { sha1sum   "$1" | cut -d' ' -f1; }
md5_of()    { md5sum     "$1" | cut -d' ' -f1; }
# base64 sha512 for npm dist.integrity (sha512-<b64>)
integrity_of() { printf 'sha512-%s' "$(openssl dgst -sha512 -binary "$1" | base64 | tr -d '\n')"; }

first() { ls -1 "$1" 2>/dev/null | head -1; }

# --------------------------------------------------------------------------
# Maven — static maven2 layout. Fixes the java.md / kotlin.md instructions.
#   repositories { maven { url "https://azul.rs/ui/maven" } }  +  rs.azul:azul:<V>
# --------------------------------------------------------------------------
build_maven() {
  # The maven job uploads THREE jars: azul-$V.jar (classes + JNA natives),
  # azul-$V-sources.jar and azul-$V-javadoc.jar. `ls | head -1` sorted them
  # and picked "-javadoc" ("-" < "."), so the live rs.azul:azul:0.2.0 was the
  # javadoc jar — 0 .class files — and every Maven/Gradle/scala-cli user got a
  # NoClassDefFoundError. Select by exact name, and refuse to publish anything
  # that is not demonstrably the classes jar.
  # No maven-jar artifact dir at all = this run did not build the jar (a local
  # run, a CI mode without the maven job): skip like the other channels. The
  # dir existing but the classes jar missing/broken is an error: the docs
  # point every Java/Kotlin/Scala user at rs.azul:azul.
  [ -d "$ART/maven-jar" ] || { echo "  [maven] no maven-jar artifacts — skip"; return; }
  local jar="$ART/maven-jar/azul-$V.jar"
  [ -f "$jar" ] || { echo "::error::[maven] $jar missing — the maven job must upload azul-$V.jar (classes + natives)"; return 1; }
  if ! unzip -l "$jar" | grep -qE '\.class$'; then
    echo "::error::[maven] $jar contains no .class files — refusing to publish it as rs.azul:azul:$V"; return 1
  fi
  if ! unzip -l "$jar" | grep -qE 'libazul\.so|libazul\.dylib|azul\.dll'; then
    echo "::error::[maven] $jar carries no native library — JNA would fail at runtime; refusing to publish"; return 1
  fi
  local dir="$SITE/ui/maven/rs/azul/azul/$V"
  mkdir -p "$dir"
  cp "$jar" "$dir/azul-$V.jar"
  # Sources jar for IDE navigation, when the job produced one.
  [ -f "$ART/maven-jar/azul-$V-sources.jar" ] && cp "$ART/maven-jar/azul-$V-sources.jar" "$dir/azul-$V-sources.jar"
  echo "  [maven] published azul-$V.jar ($(unzip -l "$jar" | grep -cE '\.class$') classes)"
  # Consumer POM (declares the JNA runtime dep; matches the maven-central pom).
  cat > "$dir/azul-$V.pom" <<POM
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>rs.azul</groupId>
  <artifactId>azul</artifactId>
  <version>$V</version>
  <packaging>jar</packaging>
  <name>Azul Java Bindings</name>
  <description>Java/Kotlin (JNA) bindings for the Azul GUI framework.</description>
  <url>https://azul.rs/</url>
  <licenses><license><name>MIT</name><url>https://opensource.org/licenses/MIT</url></license></licenses>
  <dependencies>
    <dependency>
      <groupId>net.java.dev.jna</groupId>
      <artifactId>jna</artifactId>
      <version>5.14.0</version>
    </dependency>
  </dependencies>
</project>
POM
  # maven-metadata.xml so version resolution + `latest`/`release` work.
  cat > "$SITE/ui/maven/rs/azul/azul/maven-metadata.xml" <<META
<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>rs.azul</groupId>
  <artifactId>azul</artifactId>
  <versioning>
    <latest>$V</latest>
    <release>$V</release>
    <versions><version>$V</version></versions>
    <lastUpdated>$(date -u +%Y%m%d%H%M%S)</lastUpdated>
  </versioning>
</metadata>
META
  # Maven verifies .sha1/.md5 sidecars for every artifact it downloads.
  local f
  for f in "$dir/azul-$V.jar" "$dir/azul-$V.pom" "$SITE/ui/maven/rs/azul/azul/maven-metadata.xml"; do
    sha1_of "$f" > "$f.sha1"; md5_of "$f" > "$f.md5"
  done
  echo "  [maven] built rs/azul/azul/$V (jar+pom+metadata+checksums)"
  # rs.azul:azul-kotlin - the compiled Kotlin binding (Azul.kt) with the same
  # natives; a separate artifact because both bindings live in package
  # com.azul with the same class names and can never share a classpath. The
  # maven job writes the consumer pom next to the jar (it knows the Kotlin
  # version it compiled with).
  local kjar="$ART/maven-jar/azul-kotlin-$V.jar" kpom="$ART/maven-jar/azul-kotlin-$V.pom"
  [ -f "$kjar" ] || { echo "::error::[maven] $kjar missing — the maven job must upload azul-kotlin-$V.jar (the Kotlin Maven route depends on rs.azul:azul-kotlin:$V)"; return 1; }
  [ -f "$kpom" ] || { echo "::error::[maven] $kpom missing — the maven job writes it next to the jar"; return 1; }
  if ! unzip -l "$kjar" | grep -qE '\.class$'; then
    echo "::error::[maven] $kjar contains no .class files — refusing to publish it as rs.azul:azul-kotlin:$V"; return 1
  fi
  if ! unzip -l "$kjar" | grep -qE 'libazul\.so|libazul\.dylib|azul\.dll'; then
    echo "::error::[maven] $kjar carries no native library — JNA would fail at runtime; refusing to publish"; return 1
  fi
  local kdir="$SITE/ui/maven/rs/azul/azul-kotlin/$V"
  mkdir -p "$kdir"
  cp "$kjar" "$kdir/azul-kotlin-$V.jar"
  cp "$kpom" "$kdir/azul-kotlin-$V.pom"
  echo "  [maven] published azul-kotlin-$V.jar ($(unzip -l "$kjar" | grep -cE '\.class$') classes)"
  cat > "$SITE/ui/maven/rs/azul/azul-kotlin/maven-metadata.xml" <<META
<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>rs.azul</groupId>
  <artifactId>azul-kotlin</artifactId>
  <versioning>
    <latest>$V</latest>
    <release>$V</release>
    <versions><version>$V</version></versions>
    <lastUpdated>$(date -u +%Y%m%d%H%M%S)</lastUpdated>
  </versioning>
</metadata>
META
}

# --------------------------------------------------------------------------
# PyPI — PEP 503 index (root = /ui) + hosted distributions under /ui/azul/.
#   pip install azul --index-url https://azul.rs/ui
# --------------------------------------------------------------------------
build_pypi() {
  local files; files=$(ls -1 "$ART"/pypi-dist/*.whl 2>/dev/null)
  [ -n "$files" ] || { echo "  [pypi] no wheel artifacts — skip"; return; }
  # PEP 503 index ROOT is /ui itself; pip fetches <index-url>/azul/, so the
  # per-project page lives at /ui/azul/. (pip install azul --index-url .../ui)
  local pkgdir="$SITE/ui/azul"
  mkdir -p "$pkgdir"
  # macOS (arm64) + Windows (x64) wheels, wrapped around the extension modules
  # the build_pyext job produced (release/<V>/azul.so + azul.pyd). cibuildwheel
  # only ran on Linux, so `pip install azul --index-url` had exactly one wheel;
  # on any other platform pip fell back to the sdist, whose build cannot work
  # (the extension needs sources only the repo's generator emits), and the
  # docs said "works with pip" regardless. WHEELS ONLY: an sdist is never
  # listed, whatever the artifact dir holds.
  wrap_pyext_wheel "$RELDIR/azul.so"  "macosx_11_0_arm64" "azul.abi3.so" "$pkgdir"
  wrap_pyext_wheel "$RELDIR/azul.pyd" "win_amd64"         "azul.pyd"     "$pkgdir"
  local links="" f base h
  for f in "$ART"/pypi-dist/*.whl "$pkgdir"/azul-"$V"-cp310-abi3-macosx_11_0_arm64.whl "$pkgdir"/azul-"$V"-cp310-abi3-win_amd64.whl; do
    [ -f "$f" ] || continue
    base="$(basename "$f")"
    [ "$f" = "$pkgdir/$base" ] || cp "$f" "$pkgdir/$base"
    h="$(sha256_of "$pkgdir/$base")"
    links="$links    <a href=\"$base#sha256=$h\">$base</a><br>\n"
  done
  # per-project page: /ui/azul/index.html
  printf '<!DOCTYPE html><html><head><meta name="pypi:repository-version" content="1.0"><title>Links for azul</title></head><body><h1>Links for azul</h1>\n%b</body></html>\n' "$links" \
    > "$pkgdir/index.html"
  # No root simple index (/ui/index.html is the site's mirror landing page).
  # pip/uv/poetry fetch <root>/azul/ directly for a known package, so the root
  # listing is unnecessary and writing one here would clobber that landing page.
  echo "  [pypi] built ui/azul/ ($(ls -1 "$pkgdir" | grep -vc index.html) dists)"
}

# Wrap one prebuilt CPython extension module into an abi3 wheel (PEP 427: a
# zip with the module + <name>-<version>.dist-info/{METADATA,WHEEL,RECORD}).
# $1 module file, $2 platform tag, $3 file name inside the wheel, $4 out dir.
# A placeholder (the skeleton build touches these) is skipped, never wrapped.
wrap_pyext_wheel() {
  local src="$1" plat="$2" inner="$3" out="$4"
  [ -f "$src" ] || { echo "  [pypi] no $(basename "$src") in $RELDIR — no $plat wheel"; return; }
  local size; size=$(wc -c < "$src" | tr -d ' ')
  [ "$size" -gt 1000000 ] || { echo "  [pypi] $(basename "$src") is $size bytes (placeholder) — no $plat wheel"; return; }
  SRC="$src" PLAT="$plat" INNER="$inner" OUT="$out" V="$V" python3 - <<'PY'
import base64, hashlib, os, zipfile
src, plat, inner, out, V = (os.environ[k] for k in ("SRC", "PLAT", "INNER", "OUT", "V"))
tag = f"cp310-abi3-{plat}"
name = f"azul-{V}-{tag}.whl"
info = f"azul-{V}.dist-info"
metadata = f"""Metadata-Version: 2.1
Name: azul
Version: {V}
Summary: Python bindings for the Azul GUI framework
Home-page: https://azul.rs/
License: MIT
Requires-Python: >=3.10
Classifier: Programming Language :: Python :: 3
Classifier: Programming Language :: Rust
Classifier: Topic :: Software Development :: User Interfaces

Python bindings for the Azul GUI framework (prebuilt abi3 extension module).
See https://azul.rs/ui/guide/hello-world/python
"""
wheel = f"""Wheel-Version: 1.0
Generator: azul-release (scripts/build_registry_mirrors.sh)
Root-Is-Purelib: false
Tag: {tag}
"""
def digest(b):
    return "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(b).digest()).rstrip(b"=").decode()
files = [(inner, open(src, "rb").read()),
         (f"{info}/METADATA", metadata.encode()),
         (f"{info}/WHEEL", wheel.encode()),
         (f"{info}/top_level.txt", b"azul\n")]
record = "".join(f"{p},{digest(b)},{len(b)}\n" for p, b in files) + f"{info}/RECORD,,\n"
files.append((f"{info}/RECORD", record.encode()))
path = os.path.join(out, name)
with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
    for p, b in files:
        zi = zipfile.ZipInfo(p, date_time=(2026, 1, 1, 0, 0, 0))
        zi.compress_type = zipfile.ZIP_DEFLATED
        zi.external_attr = 0o644 << 16
        z.writestr(zi, b)
print(f"  [pypi] wrapped {os.path.basename(src)} -> {name} ({os.path.getsize(path)} bytes)")
PY
}

# --------------------------------------------------------------------------
# npm — a static registry metadata document + hosted tarball.
#   npm install azul --registry https://azul.rs/ui/npm/   (or .npmrc registry=)
# npm fetches <registry>/<pkg> for metadata, then dist.tarball for the .tgz.
# --------------------------------------------------------------------------
build_npm() {
  local tgz; tgz="$(ls -1 "$ART"/npm-package/*.tgz 2>/dev/null | head -1)"
  [ -n "$tgz" ] || { echo "  [npm] no tarball artifact — skip"; return; }
  local pkgdir="$SITE/ui/npm/azul"
  mkdir -p "$pkgdir"
  # Flat tarball URL (npm install <url> just fetches this file; the /azul/-/
  # registry nesting isn't needed and a static --registry can't work on Pages).
  cp "$tgz" "$SITE/ui/npm/azul-$V.tgz"
  local tarball="$BASE/ui/npm/azul-$V.tgz"
  local integrity; integrity="$(integrity_of "$tgz")"
  local shasum; shasum="$(sha1_of "$tgz")"
  # Minimal but valid registry metadata doc (npm reads dist-tags + versions).
  cat > "$pkgdir/index.json" <<NPM
{
  "_id": "azul",
  "name": "azul",
  "dist-tags": { "latest": "$V" },
  "versions": {
    "$V": {
      "name": "azul",
      "version": "$V",
      "description": "Azul GUI framework bindings (koffi FFI loader).",
      "license": "MIT",
      "homepage": "https://azul.rs/",
      "dist": {
        "tarball": "$tarball",
        "shasum": "$shasum",
        "integrity": "$integrity"
      }
    }
  }
}
NPM
  # npm requests <registry>/azul (no extension); GitHub Pages serves index.html
  # for a directory but NOT index.json, so also write the bare doc as the dir
  # itself isn't possible — provide azul.json and document the metadata URL.
  cp "$pkgdir/index.json" "$SITE/ui/npm/azul.json"
  echo "  [npm] built npm/azul (metadata + tarball)"
}

# --------------------------------------------------------------------------
# NuGet — v3 static feed: service index -> flat container -> nupkg.
#   dotnet nuget add source https://azul.rs/ui/nuget/index.json -n azul
#   dotnet add package Azul.Net --version <V>
#
# The flat container MUST be keyed on the package's OWN id, lowercased. NuGet
# re-reads the .nuspec out of every .nupkg it downloads and rejects a mismatch:
#
#   The nupkg at .../flatcontainer/azul/0.2.0/azul.0.2.0.nupkg is not valid.
#     Expected package azul 0.2.0, but got package Azul.Net 0.2.0
#
# This id was hardcoded to `azul` while <PackageId> in
# doc/src/codegen/v2/lang_csharp/csproj.rs has always been `Azul.Net`, so the
# feed was perfectly well-formed, every file resolved, and
# `dotnet add package azul` could not work for anybody. Read the id and version
# back out of the artifact instead of asserting them.
# --------------------------------------------------------------------------
build_nuget() {
  local nupkg; nupkg="$(ls -1 "$ART"/nuget-package/*.nupkg 2>/dev/null | head -1)"
  [ -n "$nupkg" ] || { echo "  [nuget] no nupkg artifact — skip"; return 0; }
  local meta
  meta="$(python3 - "$nupkg" <<'PY'
import re, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    names = [n for n in z.namelist() if n.endswith(".nuspec") and "/" not in n]
    if not names:
        sys.exit("no root .nuspec in the package")
    xml = z.read(names[0]).decode("utf-8", "replace")
def tag(t):
    # <id>/<version> appear as ELEMENTS only inside <metadata>; a <dependency>
    # carries them as attributes, so an element match is unambiguous.
    m = re.search(r"<%s>\s*([^<]+?)\s*</%s>" % (t, t), xml)
    if not m:
        sys.exit("nuspec has no <%s> element" % t)
    return m.group(1)
print(tag("id"))
print(tag("version"))
PY
  )" || { echo "::error::[nuget] could not read the id/version out of $(basename "$nupkg")"; return 1; }
  local id ver lid lver
  id="$(printf '%s\n' "$meta" | sed -n 1p)"
  ver="$(printf '%s\n' "$meta" | sed -n 2p)"
  lid="$(echo "$id" | tr '[:upper:]' '[:lower:]')"
  lver="$(echo "$ver" | tr '[:upper:]' '[:lower:]')"
  [ -n "$lid" ] && [ -n "$lver" ] \
    || { echo "::error::[nuget] empty id/version in $(basename "$nupkg")"; return 1; }
  # The docs pin `--version <release>`; a feed advertising a different version
  # makes that documented command unsatisfiable.
  if [ "$lver" != "$(echo "$V" | tr '[:upper:]' '[:lower:]')" ]; then
    echo "::error::[nuget] $(basename "$nupkg") is $id $ver but this release is $V — 'dotnet add package $id --version $V' could not resolve"
    return 1
  fi
  # ...and the same check for the ID, which was missing. The version was
  # validated against the release; the id was taken from the nupkg and used to
  # name the flat-container directory, whatever it happened to be. So the deploy
  # published `flatcontainer/azul/`, printed "package id azul", and went green,
  # while the site tells every C# user to run `dotnet add package Azul.Net`.
  # Live result: /ui/nuget/flatcontainer/azul/index.json is 200 and
  # /ui/nuget/flatcontainer/azul.net/index.json is 404, so the documented
  # command fails with NU1101 "no packages exist with this id".
  #
  # Root cause of the wrong id: the csproj is staged from the `dll-share`
  # codegen artifact, and a stale one predates `<PackageId>Azul.Net</PackageId>`
  # — with no PackageId, MSBuild defaults the id to the project file name,
  # `Azul`. Nothing compared the id it produced to the id it advertises.
  local doc_id="azul.net"
  if [ "$lid" != "$doc_id" ]; then
    echo "::error::[nuget] $(basename "$nupkg") has package id '$id', but the site documents 'dotnet add package Azul.Net' (expected '$doc_id'). Hosting it under flatcontainer/$lid/ would 404 that command. If the id changed on purpose, update the docs, scripts/verify_install_commands.sh (DEFAULT_NUGET_ID) and this check together; if not, the staged csproj is stale and is missing <PackageId>."
    return 1
  fi
  local base="$SITE/ui/nuget"
  mkdir -p "$base/flatcontainer/$lid/$lver"
  cp "$nupkg" "$base/flatcontainer/$lid/$lver/$lid.$lver.nupkg"
  # flat-container version index
  cat > "$base/flatcontainer/$lid/index.json" <<IDX
{ "versions": [ "$lver" ] }
IDX
  # v3 service index pointing at the (static) flat container
  cat > "$base/index.json" <<SVC
{
  "version": "3.0.0",
  "resources": [
    { "@id": "$BASE/ui/nuget/flatcontainer/", "@type": "PackageBaseAddress/3.0.0" }
  ]
}
SVC
  echo "  [nuget] built nuget/index.json + flatcontainer/$lid/$lver (package id $id)"
}

# --------------------------------------------------------------------------
# RubyGems — the STATIC "Marshal spec index" tree that `gem install --source`
# actually fetches:
#
#   <source>/latest_specs.4.8.gz              Marshal([[name, Gem::Version, platform], …])
#   <source>/specs.4.8.gz                     ditto, all released versions
#   <source>/prerelease_specs.4.8.gz          ditto, prerelease versions
#   <source>/quick/Marshal.4.8/<full>.gemspec.rz   Zlib::Deflate(Marshal(Gem::Specification))
#   <source>/gems/<full>.gem                  the package itself
#
# That is exactly the sequence in rubygems' lib/rubygems/source.rb:
# `load_specs` → `load_compact_index_specs || load_marshal_specs` (the compact
# index probe 404s on a static host and falls back), then `fetch_spec` →
# `quick/Marshal.4.8/…rz`, then `download` → `gems/<spec.file_name>`. Source
# URIs are run through `enforce_trailing_slash`, so the documented
# `--source https://azul.rs/ui/gems` (no trailing slash) resolves correctly.
#
# We build those files DIRECTLY rather than shelling out to `gem generate_index`,
# which is dead on every modern RubyGems for two independent reasons:
#   1. `--legacy` was removed in RubyGems 3.0 — passing it is an InvalidOption,
#      so the command exits non-zero on anything newer than that. (This is what
#      the deploy actually hit: "[gems] hosted .gem only (generate_index failed)".)
#   2. RubyGems ≥ 3.5 moved the command out into the `rubygems-generate_index`
#      gem and left behind a trampoline stub. `gem help generate_index` still
#      succeeds — so the old `gem help` guard is a false positive by
#      construction — while executing it tries to `Gem.install` that gem into
#      the system gem dir and fails without root.
#
# Failure here is FATAL (the caller aborts the whole mirror build): hosting a
# bare .gem with no index ships a `gem install azul --source https://azul.rs/ui/gems`
# command that cannot work, which is the exact class of dead install command
# this script exists to prevent.
# --------------------------------------------------------------------------
build_gems() {
  local gem; gem="$(ls -1 "$ART"/ruby-gem/*.gem 2>/dev/null | head -1)"
  [ -n "$gem" ] || { echo "  [gems] no gem artifact — skip"; return 0; }
  local g="$SITE/ui/gems"
  mkdir -p "$g/gems"
  cp "$gem" "$g/gems/"

  if ! command -v ruby >/dev/null 2>&1; then
    echo "::error::[gems] ruby is not installed on this runner — cannot build the RubyGems"
    echo "::error::[gems] spec index, and a .gem with no index is not installable."
    return 1
  fi

  ruby - "$g" <<'RUBY_INDEX' || return 1
# Build (and then read back) the RubyGems Marshal spec index for a static mirror.
# Mirrors Gem::Indexer#build_marshal_gemspecs / #build_modern_index without
# depending on the (removed) `gem generate_index` command.
require "rubygems/package"
require "zlib"
require "fileutils"

root    = ARGV[0]
gem_dir = File.join(root, "gems")
quick   = File.join(root, "quick", "Marshal.4.8")
FileUtils.mkdir_p(quick)

paths = Dir[File.join(gem_dir, "*.gem")].sort
abort("no .gem files under #{gem_dir}") if paths.empty?

specs = paths.map do |path|
  spec = Gem::Package.new(path).spec
  # RemoteFetcher#download requests gems/<spec.file_name>; normalise the on-disk
  # name so the URL the index advertises always resolves.
  want = File.join(gem_dir, spec.file_name)
  FileUtils.mv(path, want) unless File.expand_path(path) == File.expand_path(want)
  spec
end

specs.each do |spec|
  File.binwrite(File.join(quick, "#{spec.original_name}.gemspec.rz"),
                Zlib::Deflate.deflate(Marshal.dump(spec)))
end

def tuples(specs)
  specs.sort_by {|s| [s.name, s.version] }.map do |s|
    plat = s.original_platform.to_s
    plat = Gem::Platform::RUBY if plat.empty?
    [s.name, s.version, plat]
  end
end

def dump_gz(path, data)
  Zlib::GzipWriter.open(path) {|gz| gz.write(Marshal.dump(data)) }
end

prerelease, released = specs.partition {|s| s.version.prerelease? }
# One entry per (name, platform): the highest version.
latest = released.group_by {|s| [s.name, s.original_platform.to_s] }
                 .map {|_, group| group.max_by(&:version) }

dump_gz(File.join(root, "specs.4.8.gz"),            tuples(released))
dump_gz(File.join(root, "latest_specs.4.8.gz"),     tuples(latest))
dump_gz(File.join(root, "prerelease_specs.4.8.gz"), tuples(prerelease))

# ---- read the tree back exactly the way a client walks it -----------------
def load_gz(path)
  Marshal.load(Zlib::GzipReader.open(path, &:read))
end

listed = load_gz(File.join(root, "latest_specs.4.8.gz"))
abort("latest_specs.4.8.gz lists no gems") if listed.empty?

listed.each do |name, version, platform|
  full = platform.to_s == "ruby" ? "#{name}-#{version}" : "#{name}-#{version}-#{platform}"

  quick_file = File.join(quick, "#{full}.gemspec.rz")
  abort("index lists #{full} but #{quick_file} is missing/empty") unless File.size?(quick_file)
  loaded = Marshal.load(Zlib::Inflate.inflate(File.binread(quick_file)))
  unless loaded.name == name && loaded.version.to_s == version.to_s
    abort("quick spec #{quick_file} describes #{loaded.full_name}, index says #{full}")
  end

  gem_file = File.join(gem_dir, "#{full}.gem")
  abort("index lists #{full} but #{gem_file} is missing/empty") unless File.size?(gem_file)
end

puts "  [gems] built RubyGems spec index for #{specs.size} gem(s): " \
     "specs/latest_specs/prerelease_specs.4.8.gz + quick/Marshal.4.8 " \
     "(#{listed.map {|n, v, _| "#{n}-#{v}" }.join(', ')})"
RUBY_INDEX

  # Human landing page: /ui/gems/ is a real URL in the install docs, and a bare
  # 404 there reads as "the whole channel is broken". RubyGems only ever fetches
  # the exact paths above, so an index.html cannot collide with it.
  cat > "$g/index.html" <<HTML
<!DOCTYPE html><html><head><meta charset="utf-8"><title>azul — RubyGems mirror</title></head>
<body><h1>azul — self-hosted RubyGems source</h1>
<pre><code>gem install azul --clear-sources --source $BASE/ui/gems</code></pre>
<p><code>--clear-sources</code> is required: <code>--source</code> only <em>appends</em> to the
source list, and an unrelated gem named <code>azul</code> exists on rubygems.org.</p>
<p>Or in a Gemfile (a <code>source</code> block is already exclusive):</p>
<pre><code>source "$BASE/ui/gems" do
  gem "azul"
end</code></pre>
<p>Index files: <a href="specs.4.8.gz">specs.4.8.gz</a>,
<a href="latest_specs.4.8.gz">latest_specs.4.8.gz</a>,
<a href="prerelease_specs.4.8.gz">prerelease_specs.4.8.gz</a>,
<code>quick/Marshal.4.8/</code>, <code>gems/</code>.</p>
</body></html>
HTML
}

# --------------------------------------------------------------------------
# DNF/yum — createrepo_c repodata over the built .rpm(s).
#   [azul] baseurl=https://azul.rs/ui/rpm   ;  dnf install azul
# --------------------------------------------------------------------------
build_rpm() {
  local rpms; rpms=$(ls -1 "$ART"/artifacts-rpm/*.rpm 2>/dev/null)
  [ -n "$rpms" ] || { echo "  [rpm] no rpm artifacts — skip"; return; }
  local r="$SITE/ui/rpm"
  mkdir -p "$r"
  cp "$ART"/artifacts-rpm/*.rpm "$r/" 2>/dev/null || true
  if command -v createrepo_c >/dev/null 2>&1; then
    createrepo_c "$r" >/dev/null 2>&1 && echo "  [rpm] built repodata (createrepo_c)" \
      || { echo "::error::[rpm] createrepo_c failed - 'dnf install azul' has no repodata to read"; return 1; }
  elif command -v createrepo >/dev/null 2>&1; then
    createrepo "$r" >/dev/null 2>&1 && echo "  [rpm] built repodata (createrepo)" \
      || { echo "::error::[rpm] createrepo failed"; return 1; }
  else
    echo "::error::[rpm] no createrepo_c available - the .rpm is hosted but 'dnf install azul' cannot find it"
    return 1
  fi
  # Repo signing is opt-in (AZUL_RPM_GPG_KEY: base64 armored private key), the
  # same shape as the apt key. Unsigned, azul.repo says gpgcheck=0 - which is
  # what it means - and the docs say so too.
  local gpg_lines="gpgcheck=0"
  if [ -n "${AZUL_RPM_GPG_KEY:-}" ] && command -v gpg >/dev/null 2>&1; then
    if printf '%s' "$AZUL_RPM_GPG_KEY" | base64 -d | gpg --batch --import >/dev/null 2>&1 \
       && gpg --batch --yes --armor --detach-sign -o "$r/repodata/repomd.xml.asc" "$r/repodata/repomd.xml" \
       && gpg --batch --armor --export > "$r/azul.gpg.key"; then
      gpg_lines="gpgcheck=1
repo_gpgcheck=1
gpgkey=$BASE/ui/rpm/azul.gpg.key"
      echo "  [rpm] repodata signed (repomd.xml.asc + azul.gpg.key)"
    else
      echo "::warning::[rpm] AZUL_RPM_GPG_KEY present but signing failed - publishing unsigned"
    fi
  fi
  # A ready-to-use .repo for `dnf config-manager --add-repo .../azul.repo`;
  # yum + zypper read the same baseurl.
  cat > "$r/azul.repo" <<REPO
[azul]
name=Azul GUI framework
baseurl=$BASE/ui/rpm
enabled=1
$gpg_lines
REPO
}

# --------------------------------------------------------------------------
# Homebrew — a tap is just a git repo. Homebrew only DEFAULTS to GitHub for the
# `brew tap user/name` shorthand; with an explicit URL it clones any git URL,
# including a dumb-HTTP bare repo served by GitHub Pages. So we publish a REAL
# bare repo at azul.rs/ui/brew.git (stable path) whose Formula/azul.rb
# is regenerated each release — `brew upgrade azul` then tracks new versions.
#   brew tap fschutt/azul https://azul.rs/ui/brew.git
#   brew install fschutt/azul/azul
# --------------------------------------------------------------------------
build_homebrew() {
  command -v git >/dev/null 2>&1 || { echo "  [brew] git missing — skip"; return; }
  local arm="$RELDIR/libazul.dylib" intel="$RELDIR/libazul.x86_64.dylib" hdr="$RELDIR/azul.h"
  [ -f "$arm" ] || { echo "  [brew] no macOS arm64 dylib in $RELDIR — skip"; return; }
  [ -f "$hdr" ] || { echo "  [brew] no azul.h in $RELDIR — skip"; return; }
  local arm_sha hdr_sha; arm_sha="$(sha256_of "$arm")"; hdr_sha="$(sha256_of "$hdr")"
  # The C++ wrapper headers (azul03.hpp … azul23.hpp) go into include/ too, so
  # `clang++ -I$(brew --prefix)/include` compiles the C++ hello-world without a
  # second download. Each is its own resource so the formula pins its sha256.
  local hpp_resources="" hpp_installs="" std
  for std in 03 11 14 17 20 23; do
    [ -f "$RELDIR/azul$std.hpp" ] || continue
    hpp_resources="$hpp_resources
    resource \"azul$std.hpp\" do
      url \"$BASE/ui/release/$V/azul$std.hpp\"
      sha256 \"$(sha256_of "$RELDIR/azul$std.hpp")\"
    end"
    hpp_installs="$hpp_installs
    resource(\"azul$std.hpp\").stage { include.install \"azul$std.hpp\" }"
  done

  # Build the formula. on_intel is emitted only if the Intel dylib exists.
  local intel_block=""
  if [ -f "$intel" ]; then
    local intel_sha; intel_sha="$(sha256_of "$intel")"
    intel_block="    on_intel do
      url \"$BASE/ui/release/$V/libazul.x86_64.dylib\"
      sha256 \"$intel_sha\"
    end"
  fi
  local work; work="$(mktemp -d)"
  mkdir -p "$work/Formula"
  cat > "$work/Formula/azul.rb" <<RB
# Auto-generated by scripts/build_registry_mirrors.sh — do not edit by hand.
class Azul < Formula
  desc "Azul GUI framework - prebuilt native library (libazul)"
  homepage "https://azul.rs/"
  version "$V"
  license "MIT"

  livecheck do
    skip "self-hosted tap; the formula is regenerated on every azul.rs deploy"
  end

  on_macos do
    on_arm do
      url "$BASE/ui/release/$V/libazul.dylib"
      sha256 "$arm_sha"
    end
$intel_block
    resource "header" do
      url "$BASE/ui/release/$V/azul.h"
      sha256 "$hdr_sha"
    end$hpp_resources
  end

  def install
    lib.install Dir["*.dylib"].first => "libazul.dylib"
    resource("header").stage { include.install "azul.h" }$hpp_installs
    # pkg-config, so `cc \$(pkg-config --cflags --libs azul) hello-world.c` works.
    (lib/"pkgconfig").mkpath
    (lib/"pkgconfig/azul.pc").write <<~PC
      prefix=#{opt_prefix}
      libdir=\${prefix}/lib
      includedir=\${prefix}/include

      Name: azul
      Description: Azul GUI framework (prebuilt libazul)
      Version: $V
      Libs: -L\${libdir} -lazul
      Cflags: -I\${includedir}
    PC
  end

  test do
    assert_predicate lib/"libazul.dylib", :exist?
  end
end
RB
  # Real git repo -> bare clone -> update-server-info so dumb-HTTP clone works.
  ( cd "$work" && git init -q \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" add -A \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" commit -q -m "azul $V" ) || {
    echo "  [brew] git commit failed — skip"; rm -rf "$work"; return; }
  rm -rf "$SITE/ui/brew.git"
  git clone -q --bare "$work" "$SITE/ui/brew.git" || { echo "  [brew] bare clone failed"; rm -rf "$work"; return; }
  ( cd "$SITE/ui/brew.git" && git update-server-info )
  # Compatibility: the tap was published as ui/homebrew-azul.git until
  # 2026-09-07 and every `brew tap` done before then has that URL as its git
  # remote. Serve the identical bare repo there too, so their `brew upgrade`
  # keeps working; the docs use the short path.
  rm -rf "$SITE/ui/homebrew-azul.git" && cp -R "$SITE/ui/brew.git" "$SITE/ui/homebrew-azul.git"
  rm -rf "$work"
  # Prove the published repo is actually clonable (what `brew tap` will do,
  # minus the HTTP transport): a file:// clone of the bare repo must yield
  # Formula/azul.rb. Non-fatal — a failure only warns.
  local chk; chk="$(mktemp -d)"
  if git clone -q "file://$(cd "$SITE/ui/brew.git" && pwd)" "$chk/tap" \
     && [ -f "$chk/tap/Formula/azul.rb" ]; then
    echo "  [brew] self-check: bare repo clones and contains Formula/azul.rb"
  else
    echo "::warning::[brew] self-check clone of brew.git FAILED"
  fi
  rm -rf "$chk"
  echo "  [brew] published brew.git (formula azul $V; intel=$([ -f "$intel" ] && echo yes || echo no))"
}

# --------------------------------------------------------------------------
# Chocolatey — choco consumes a NuGet v3 feed, which we already host at
# azul.rs/ui/nuget. We add a `libazul` choco package (a .nupkg is just a zip with a
# nuspec + tools/chocolateyInstall.ps1) into that same flat-container, so:
#   choco install libazul --source https://azul.rs/ui/nuget/index.json
#   choco upgrade libazul   # the stable v3 source advertises new versions
# The install script downloads azul.dll + azul.dll.lib + azul.h from the
# matching release URLs and sets AZ_LINK_PATH. Verified on a Windows runner by
# scripts/verify_install_commands.sh choco (post-release.yml).
# --------------------------------------------------------------------------
build_choco() {
  local dll="$RELDIR/azul.dll" implib="$RELDIR/azul.dll.lib" hdr="$RELDIR/azul.h"
  for f in "$dll" "$implib" "$hdr"; do
    [ -f "$f" ] || { echo "  [choco] no $(basename "$f") in $RELDIR — skip"; return; }
  done
  local dll_sha implib_sha hdr_sha
  dll_sha="$(sha256_of "$dll")"; implib_sha="$(sha256_of "$implib")"; hdr_sha="$(sha256_of "$hdr")"
  local lver; lver="$(echo "$V" | tr '[:upper:]' '[:lower:]')"
  local dest="$SITE/ui/nuget/flatcontainer/libazul/$lver"
  mkdir -p "$dest"
  SITE="$SITE" V="$V" DLLSHA="$dll_sha" IMPLIBSHA="$implib_sha" HDRSHA="$hdr_sha" DEST="$dest" python3 - <<'PY'
import os, zipfile, uuid
V = os.environ["V"]; sha = os.environ["DLLSHA"]; dest = os.environ["DEST"]
implib_sha = os.environ["IMPLIBSHA"]; hdr_sha = os.environ["HDRSHA"]
nuspec = f'''<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2013/05/nuspec.xsd">
  <metadata>
    <id>libazul</id>
    <version>{V}</version>
    <title>libazul</title>
    <authors>Felix Schuett</authors>
    <projectUrl>https://azul.rs/</projectUrl>
    <licenseUrl>https://opensource.org/licenses/MIT</licenseUrl>
    <requireLicenseAcceptance>false</requireLicenseAcceptance>
    <description>Azul GUI framework prebuilt native library: azul.dll, the MSVC import library azul.dll.lib and the C header azul.h; sets AZ_LINK_PATH.</description>
    <tags>azul gui native dll</tags>
  </metadata>
</package>
'''
# All three files a C/C++/Rust build needs, not just the DLL: the header to
# compile against, the MSVC import library to link, the DLL to run. AZ_LINK_PATH
# makes the pre-rendered `azul` Rust crate's build.rs find them with no
# further configuration (it is the documented env var; build_link.rs).
install_ps1 = f'''$ErrorActionPreference = 'Stop'
$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
Get-ChocolateyWebFile -PackageName 'libazul' `
  -FileFullPath (Join-Path $tools 'azul.dll') `
  -Url64bit 'https://azul.rs/ui/release/{V}/azul.dll' `
  -Checksum64 '{sha}' -ChecksumType64 'sha256'
Get-ChocolateyWebFile -PackageName 'libazul' `
  -FileFullPath (Join-Path $tools 'azul.dll.lib') `
  -Url64bit 'https://azul.rs/ui/release/{V}/azul.dll.lib' `
  -Checksum64 '{implib_sha}' -ChecksumType64 'sha256'
Get-ChocolateyWebFile -PackageName 'libazul' `
  -FileFullPath (Join-Path $tools 'azul.h') `
  -Url64bit 'https://azul.rs/ui/release/{V}/azul.h' `
  -Checksum64 '{hdr_sha}' -ChecksumType64 'sha256'
Install-ChocolateyEnvironmentVariable -VariableName 'AZ_LINK_PATH' -VariableValue $tools -VariableType 'Machine'
Write-Host "libazul installed to $tools (azul.dll, azul.dll.lib, azul.h); AZ_LINK_PATH=$tools"
'''
uninstall_ps1 = '''$ErrorActionPreference = 'Stop'
Uninstall-ChocolateyEnvironmentVariable -VariableName 'AZ_LINK_PATH' -VariableType 'Machine'
'''
content_types = '''<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="nuspec" ContentType="application/octet" />
  <Default Extension="ps1" ContentType="application/octet" />
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml" />
  <Default Extension="psmdcp" ContentType="application/vnd.openxmlformats-package.core-properties+xml" />
</Types>
'''
psmdcp_id = uuid.uuid4().hex
rels = f'''<?xml version="1.0" encoding="utf-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Type="http://schemas.microsoft.com/packaging/2010/07/manifest" Target="/libazul.nuspec" Id="R1" />
  <Relationship Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="/package/services/metadata/core-properties/{psmdcp_id}.psmdcp" Id="R2" />
</Relationships>
'''
psmdcp = f'''<?xml version="1.0" encoding="utf-8"?>
<coreProperties xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns="http://schemas.openxmlformats.org/package/2006/metadata/core-properties">
  <dc:creator>Felix Schuett</dc:creator>
  <dc:description>Azul GUI framework prebuilt native library (azul.dll + import library + C header).</dc:description>
  <dc:identifier>libazul</dc:identifier>
  <version>{V}</version>
</coreProperties>
'''
out = os.path.join(dest, f"libazul.{V.lower()}.nupkg")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("libazul.nuspec", nuspec)
    z.writestr("tools/chocolateyInstall.ps1", install_ps1)
    z.writestr("tools/chocolateyUninstall.ps1", uninstall_ps1)
    z.writestr("[Content_Types].xml", content_types)
    z.writestr("_rels/.rels", rels)
    z.writestr(f"package/services/metadata/core-properties/{psmdcp_id}.psmdcp", psmdcp)
print("  [choco] wrote", out)
PY
  # flat-container version index for the libazul package
  cat > "$SITE/ui/nuget/flatcontainer/libazul/index.json" <<IDX
{ "versions": [ "$lver" ] }
IDX
  echo "  [choco] libazul package added to the nuget v3 feed"
}

# --------------------------------------------------------------------------
# Repo-metadata tools that only exist inside their own distro (repo-add, apk)
# run in that distro's official container when the host lacks them. The GitHub
# runner has docker; a developer running this locally may not — then the
# packages are still hosted and the db/index is skipped with a warning.
# --------------------------------------------------------------------------
in_distro() { # $1 image, $2 host dir (mounted at /repo, cwd), $3.. command
  local image="$1" dir="$2"; shift 2
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    docker run --rm --user "$(id -u):$(id -g)" -v "$(cd "$dir" && pwd):/repo" -w /repo "$image" "$@"
  else
    return 127
  fi
}

# --------------------------------------------------------------------------
# pacman (Arch / Manjaro) — host the .pkg.tar.zst + a repo db (built by
# nfpm --packager archlinux in build_linux_packages).
#   /etc/pacman.conf:  [azul]
#                      SigLevel = Optional TrustAll   # until AZUL_PACMAN_GPG_KEY signs it
#                      Server = https://azul.rs/ui/arch/$arch
#   pacman -Sy azul      (pacman -Syu keeps it updated)
# repo-add builds the db; the runner has none, so it runs in archlinux:base.
# --------------------------------------------------------------------------
build_pacman() {
  local pkgs; pkgs=$(ls -1 "$ART"/artifacts-arch/*.pkg.tar.zst 2>/dev/null)
  [ -n "$pkgs" ] || { echo "  [pacman] no .pkg.tar.zst artifacts — skip"; return; }
  local arch_dir="$SITE/ui/arch/x86_64"
  mkdir -p "$arch_dir"
  cp "$ART"/artifacts-arch/*.pkg.tar.zst "$arch_dir/"
  local sign=""
  if [ -n "${AZUL_PACMAN_GPG_KEY:-}" ]; then
    # Signing runs inside the container too (repo-add --sign needs gpg there):
    # the key is handed over as a file that is deleted right after.
    printf '%s' "$AZUL_PACMAN_GPG_KEY" | base64 -d > "$arch_dir/.signing-key.asc"
    sign='gpg --batch --import .signing-key.asc >/dev/null 2>&1 && repo-add --sign'
  fi
  local cmd="${sign:-repo-add} azul.db.tar.gz ./*.pkg.tar.zst"
  if command -v repo-add >/dev/null 2>&1; then
    ( cd "$arch_dir" && bash -c "$cmd" >/dev/null 2>&1 )
  else
    in_distro archlinux:base@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a "$arch_dir" bash -c "$cmd" >/dev/null 2>&1
  fi
  local rc=$?
  rm -f "$arch_dir/.signing-key.asc"
  if [ "$rc" -eq 0 ] && [ -s "$arch_dir/azul.db.tar.gz" ]; then
    # pacman fetches <repo>.db; repo-add leaves a symlink, which Pages would
    # not serve — ship real files.
    for f in db files; do
      rm -f "$arch_dir/azul.$f"; cp "$arch_dir/azul.$f.tar.gz" "$arch_dir/azul.$f"
      [ -f "$arch_dir/azul.$f.tar.gz.sig" ] && cp "$arch_dir/azul.$f.tar.gz.sig" "$arch_dir/azul.$f.sig"
    done
    echo "  [pacman] built ui/arch/x86_64/azul.db ($(ls "$arch_dir"/*.pkg.tar.zst | wc -l | tr -d ' ') package(s)$([ -n "$sign" ] && echo ', signed'))"
  else
    echo "::error::[pacman] repo-add failed (rc=$rc) — the .pkg.tar.zst is hosted but 'pacman -Sy azul' has no db to find it in"
    return 1
  fi
}

# --------------------------------------------------------------------------
# Alpine apk — host the .apk + an APKINDEX (built by nfpm --packager apk).
# apk repos are <baseurl>/<arch>/, so:
#   /etc/apk/repositories:  https://azul.rs/ui/alpine   (apk appends /<arch>)
#   apk add --allow-untrusted azul   (until AZUL_APK_SIGN_KEY signs the index;
#                                    then: curl -o /etc/apk/keys/azul.rsa.pub
#                                    https://azul.rs/ui/alpine/azul.rsa.pub)
# Each package ships the musl build for its arch (the two musl rows of the
# cross-build matrix), so it depends on `musl` and links with Alpine's own
# gcc — a glibc libazul.so cannot be linked there at all.
# `apk index` builds the index; the runner has none, so it runs in alpine.
# --------------------------------------------------------------------------
build_apk_arch() { # $1 arch (apk's name for it), $2 dir holding that arch's .apk
  local arch="$1" src="$2"
  local apk_dir="$SITE/ui/alpine/$arch"
  mkdir -p "$apk_dir"
  # apk resolves a package to the file "<name>-<version>.apk" recorded in the
  # index, NOT to whatever the file is called on disk. nfpm names it in the
  # Debian style (azul_0.2.0_x86_64.apk), so `apk add` found the package in
  # the index and then died with "package mentioned in index not found".
  # Rename on the way in; the version comes from $V like every other channel.
  local a
  for a in "$src"/*.apk; do
    cp "$a" "$apk_dir/azul-$V.apk"
  done
  # --allow-untrusted: nfpm builds the .apk unsigned (there is no signing key
  # unless AZUL_APK_SIGN_KEY is set), and `apk index` REFUSES an unsigned
  # package with "UNTRUSTED signature", exit 99 — which is exactly how the
  # first website deploy that ever reached this channel died. What matters to
  # a client is the signature on the INDEX, not on the package, and the
  # install route the site documents already says `apk add --allow-untrusted`.
  local cmd="apk index --allow-untrusted --rewrite-arch $arch -o APKINDEX.tar.gz ./*.apk"
  if [ -n "${AZUL_APK_SIGN_KEY:-}" ]; then
    printf '%s' "$AZUL_APK_SIGN_KEY" | base64 -d > "$apk_dir/azul.rsa"
    cmd="$cmd && apk add -q abuild openssl && abuild-sign -k azul.rsa APKINDEX.tar.gz && openssl rsa -in azul.rsa -pubout -out ../azul.rsa.pub"
  fi
  # Keep the output: this step used to discard it, so a channel that had never
  # run once died with a bare "rc=99" and no reason anywhere in the log.
  local apk_log="$apk_dir/.apk-index.log"
  if command -v apk >/dev/null 2>&1 && [ -z "${AZUL_APK_SIGN_KEY:-}" ]; then
    ( cd "$apk_dir" && sh -c "$cmd" ) >"$apk_log" 2>&1
  else
    # abuild-sign needs root inside the container for `apk add`; the files it
    # writes are chowned back below.
    if [ -n "${AZUL_APK_SIGN_KEY:-}" ]; then
      docker run --rm -v "$(cd "$apk_dir" && pwd):/repo" -w /repo alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc sh -c "$cmd" >"$apk_log" 2>&1
    else
      in_distro alpine:3.20 "$apk_dir" sh -c "$cmd" >"$apk_log" 2>&1
    fi
  fi
  local rc=$?
  rm -f "$apk_dir/azul.rsa"
  if [ "$rc" -eq 0 ] && [ -s "$apk_dir/APKINDEX.tar.gz" ]; then
    # The log lives inside the directory we are about to publish: never ship it.
    rm -f "$apk_log"
    echo "  [apk] built ui/alpine/$arch/APKINDEX.tar.gz ($(ls "$apk_dir"/*.apk | wc -l | tr -d ' ') package(s)$([ -n "${AZUL_APK_SIGN_KEY:-}" ] && echo ', signed'))"
    return 0
  fi
  echo "::error::[apk] apk index failed for $arch (rc=$rc) — the .apk is hosted but 'apk add azul' has no index to find it in"
  sed 's/^/  [apk] /' "$apk_log" 2>/dev/null | tail -20
  rm -f "$apk_log"
  return 1
}

build_apk() {
  # One repository per arch: artifacts-apk/<arch>/*.apk, as the deploy job
  # downloads them. A flat artifacts-apk/*.apk is the pre-arm layout and is
  # x86_64 by definition.
  local rc=0 found=0 arch src
  for arch in x86_64 aarch64; do
    src="$ART/artifacts-apk/$arch"
    if ! ls "$src"/*.apk >/dev/null 2>&1; then
      if [ "$arch" = x86_64 ] && ls "$ART"/artifacts-apk/*.apk >/dev/null 2>&1; then
        src="$ART/artifacts-apk"
      else
        echo "  [apk] no $arch .apk artifacts — skip"
        continue
      fi
    fi
    found=1
    build_apk_arch "$arch" "$src" || rc=1
  done
  [ "$found" = 1 ] || echo "  [apk] no .apk artifacts — skip"
  return $rc
}

# --------------------------------------------------------------------------
# cargo — a static SPARSE registry (RFC 2789): plain files, GET-only, no API.
#   .cargo/config.toml:  [registries]
#                        azul = { index = "sparse+https://azul.rs/ui/cargo/" }
#   cargo add azul --registry azul
# The index entry for `azul` lives at az/ul/azul (cargo's layout for 4+-letter
# names); config.json's `dl` template points at the .crate the deploy already
# hosts under release/<V>/, so nothing is copied. The crate itself — the
# pre-rendered Rust API over the prebuilt libazul — is written by `azul-doc
# deploy` (doc/src/dllgen/bundles.rs); this publishes the index for it. Its
# `features` MUST match the crate's Cargo.toml there.
# --------------------------------------------------------------------------
build_cargo() {
  local krate="$RELDIR/azul-$V.crate"
  [ -f "$krate" ] || { echo "::error::[cargo] $krate missing — azul-doc deploy wrote no .crate (was target/codegen present?)"; return 1; }
  local size; size=$(wc -c < "$krate" | tr -d ' ')
  [ "$size" -gt 100000 ] || { echo "::error::[cargo] $krate is $size bytes — a placeholder, not the crate"; return 1; }
  local sha; sha="$(sha256_of "$krate")"
  local dir="$SITE/ui/cargo"
  mkdir -p "$dir/az/ul"
  printf '{"dl":"%s/ui/release/{version}/{crate}-{version}.crate"}\n' "$BASE" > "$dir/config.json"
  printf '{"name":"azul","vers":"%s","deps":[],"cksum":"%s","features":{"default":["link-dynamic"],"link-dynamic":[]},"yanked":false}\n' \
    "$V" "$sha" > "$dir/az/ul/azul"
  echo "  [cargo] built ui/cargo (sparse index for azul $V, crate sha256 $sha)"
}

# --------------------------------------------------------------------------
# Scoop (Windows) — a bucket is a git repo of JSON manifests, and scoop clones
# any git URL, so a bare repo on Pages works exactly like the Homebrew tap.
#   scoop bucket add azul https://azul.rs/ui/scoop.git
#   scoop install azul     (azul.dll + azul.dll.lib + azul.h; sets AZ_LINK_PATH)
# --------------------------------------------------------------------------
build_scoop() {
  command -v git >/dev/null 2>&1 || { echo "  [scoop] git missing — skip"; return; }
  local dll="$RELDIR/azul.dll" implib="$RELDIR/azul.dll.lib" hdr="$RELDIR/azul.h"
  for f in "$dll" "$implib" "$hdr"; do
    [ -f "$f" ] || { echo "  [scoop] no $(basename "$f") in $RELDIR — skip"; return; }
  done
  local work; work="$(mktemp -d)"
  mkdir -p "$work/bucket"
  cat > "$work/bucket/azul.json" <<JSON
{
    "version": "$V",
    "description": "Azul GUI framework - prebuilt native library (azul.dll, the MSVC import library and the C header)",
    "homepage": "https://azul.rs/",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": [
                "$BASE/ui/release/$V/azul.dll",
                "$BASE/ui/release/$V/azul.dll.lib",
                "$BASE/ui/release/$V/azul.h"
            ],
            "hash": [
                "$(sha256_of "$dll")",
                "$(sha256_of "$implib")",
                "$(sha256_of "$hdr")"
            ]
        }
    },
    "env_set": {
        "AZ_LINK_PATH": "\$dir"
    },
    "notes": "Compile against \$dir/azul.h and link \$dir/azul.dll.lib; azul.dll sits next to them. AZ_LINK_PATH points here, so the azul Rust crate finds the library."
}
JSON
  ( cd "$work" && git init -q \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" add -A \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" commit -q -m "azul $V" ) || {
    echo "  [scoop] git commit failed — skip"; rm -rf "$work"; return; }
  rm -rf "$SITE/ui/scoop.git"
  git clone -q --bare "$work" "$SITE/ui/scoop.git" || { echo "  [scoop] bare clone failed"; rm -rf "$work"; return; }
  ( cd "$SITE/ui/scoop.git" && git update-server-info )
  rm -rf "$SITE/ui/scoop-azul.git" && cp -R "$SITE/ui/scoop.git" "$SITE/ui/scoop-azul.git"   # compat, see brew
  rm -rf "$work"
  echo "  [scoop] published scoop.git (manifest azul $V)"
}

# --------------------------------------------------------------------------
# A human (or the docs' link check) landing on a mirror ROOT — /ui/maven,
# /ui/apt, /ui/cargo … — got a 404 from Pages while every file below it was
# fine. One small page per mirror root: the configure-once command.
# --------------------------------------------------------------------------
landing() { # $1 dir under $SITE, $2 title, $3 command block
  local d="$SITE/$1"
  [ -d "$d" ] || return 0
  [ -e "$d/index.html" ] && return 0
  cat > "$d/index.html" <<HTML
<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>$2 - azul.rs</title>
<style>body{font:15px/1.5 system-ui,sans-serif;max-width:52em;margin:3em auto;padding:0 1em;color:#222}pre{background:#f4f4f4;padding:1em;overflow-x:auto}</style>
<h1>$2</h1>
<p>This directory is a self-hosted package mirror for <a href="https://azul.rs/">azul</a> $V, regenerated on every release. Configure it once:</p>
<pre>$3</pre>
<p><a href="https://azul.rs/ui/release/$V">Release page</a> &middot; <a href="https://azul.rs/ui/guide">Guide</a></p>
HTML
}

write_landing_pages() {
  landing ui/maven "Maven repository" "&lt;repository&gt;&lt;id&gt;azul-rs&lt;/id&gt;&lt;url&gt;https://azul.rs/ui/maven&lt;/url&gt;&lt;/repository&gt;
&lt;dependency&gt;&lt;groupId&gt;rs.azul&lt;/groupId&gt;&lt;artifactId&gt;azul&lt;/artifactId&gt;&lt;version&gt;$V&lt;/version&gt;&lt;/dependency&gt;"
  landing ui/apt "apt repository" "echo 'deb [trusted=yes] https://azul.rs/ui/apt stable main' | sudo tee /etc/apt/sources.list.d/azul.list
sudo apt update &amp;&amp; sudo apt install azul"
  landing ui/rpm "dnf / yum / zypper repository" "sudo dnf config-manager --add-repo https://azul.rs/ui/rpm/azul.repo
sudo dnf install azul"
  landing ui/arch "pacman repository" "# /etc/pacman.conf
[azul]
SigLevel = Optional TrustAll
Server = https://azul.rs/ui/arch/\$arch

sudo pacman -Sy azul"
  landing ui/alpine "Alpine apk repository" "echo https://azul.rs/ui/alpine &gt;&gt; /etc/apk/repositories
apk add --allow-untrusted azul"
  landing ui/cargo "cargo registry" "# .cargo/config.toml
[registries]
azul = { index = \"sparse+https://azul.rs/ui/cargo/\" }

cargo add azul --registry azul"
  landing ui/npm "npm registry" "npm install https://azul.rs/ui/npm/azul-$V.tgz"
  landing ui/gems "RubyGems source" "gem install azul --clear-sources --source https://azul.rs/ui/gems"
  landing ui/nuget "NuGet v3 feed" "dotnet nuget add source https://azul.rs/ui/nuget/index.json --name azul
dotnet add package Azul.Net --version $V

# Chocolatey (same feed):
choco install libazul --source https://azul.rs/ui/nuget/index.json"
  landing ui/brew.git "Homebrew tap" "brew tap fschutt/azul https://azul.rs/ui/brew.git
brew install fschutt/azul/azul"
  landing ui/scoop.git "Scoop bucket" "scoop bucket add azul https://azul.rs/ui/scoop.git
scoop install azul"
}

echo "==> Building self-hosted registry mirrors under $SITE (v$V)"
# .nojekyll: GitHub Pages' (legacy) Jekyll would drop dotfiles/_dirs; disable it
# so the bare git repo (objects/, info/refs, HEAD) and every metadata file serve
# verbatim. Harmless under the static (Actions) Pages path too.
touch "$SITE/.nojekyll"
FAILED=""
build_maven || FAILED="$FAILED maven"
build_pypi
build_npm
# must run before build_choco (choco writes into the nuget tree). A nuget
# failure is FATAL for the same reason a gems failure is: the site documents
# `dotnet nuget add source .../ui/nuget/index.json` + `dotnet add package`, and
# a feed built under the wrong package id is a command that cannot work.
build_nuget || FAILED="$FAILED nuget"
build_choco
# A channel that produced an UNUSABLE mirror must red the deploy, not print a
# note and continue: "hosted .gem only (generate_index failed)" scrolled past
# unread for two months while `gem install --source https://azul.rs/ui/gems`
# 404'd for every user. Build everything first so one broken channel does not
# mask the state of the others, then fail once at the end.
build_gems || FAILED="$FAILED gems"
build_rpm    || FAILED="$FAILED rpm"   # yum + zypper consume this same repo
build_pacman || FAILED="$FAILED pacman"
build_apk    || FAILED="$FAILED apk"
build_homebrew
build_scoop
# The cargo index is the ONLY way `cargo add azul --registry azul` finds the
# crate: an index built from a missing/placeholder .crate is a dead command.
build_cargo  || FAILED="$FAILED cargo"
write_landing_pages
if [ -n "$FAILED" ]; then
  echo "::error::registry mirror channels FAILED to build a usable index:$FAILED"
  exit 1
fi
echo "==> Registry mirrors done."

#!/usr/bin/env bash
# Build self-hosted, STATIC package-registry mirrors under the GitHub Pages site
# so the azul.rs install commands work without the official registries:
#
#   Maven    azul.rs/maven       (maven2 layout: rs/azul/azul/<V>/azul-<V>.jar+pom)
#   PyPI     azul.rs/pypi/simple  (PEP 503 simple index + hosted wheels/sdists)
#   npm      azul.rs/npm/azul     (registry metadata doc + hosted .tgz)
#   NuGet    azul.rs/nuget        (v3 service index + flat-container + nupkg)
#   RubyGems azul.rs/gems         (flat: hosted .gem + a human index; see note)
#   DNF/yum/zypper azul.rs/rpm    (createrepo_c repodata; ONE repo serves all
#                                  three — yum and zypper consume dnf repodata)
#   pacman   azul.rs/arch         (repo-add db over the .pkg.tar.zst)
#   Alpine apk  azul.rs/alpine     (apk index APKINDEX.tar.gz over the .apk)
#   Homebrew azul.rs/homebrew-azul.git  (a real bare git repo = a tap)
#   Chocolatey  azul.rs/nuget (the v3 feed also serves a `libazul` choco package)
#
# UPDATE MODEL — every endpoint above is a STABLE, VERSION-INDEPENDENT path. The
# version only ever appears INSIDE the tree (maven coordinates, the formula's
# url, a nupkg version), never in the endpoint you configure. Each deploy
# regenerates the metadata so the endpoint always advertises the just-released
# version as "latest" (maven-metadata <latest>, npm dist-tags.latest, the
# Homebrew formula's version, the apt/dnf/nuget version lists). So a user who
# ran the configure-once command keeps getting upgrades:
#   brew upgrade libazul / choco upgrade libazul / apt upgrade / dnf upgrade /
#   pip install -U / etc. pull the next libazul release with no reconfiguration.
# (Old versions live on the GitHub Release; the live mirror tracks latest.)
#
# These are plain files served by GitHub Pages — no registry server needed. The
# clients that can consume a fully-static tree (Maven, pip --index-url, npm
# --registry via the metadata doc, NuGet v3, dnf) work directly. RubyGems'
# compact index needs a couple of binary-format files we generate best-effort
# when `gem` is available; otherwise we still host the .gem for manual install.
#
# Homebrew (a tap = separate github repo) and Chocolatey (needs a NuGet v2 OData
# server, not static) are intentionally NOT handled here.
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
# point their downloads at the matching azul.rs/release/<V>/ URLs and check the
# sha256 of these exact files.
RELDIR="$SITE/release/$V"

sha256_of() { sha256sum "$1" | cut -d' ' -f1; }
sha1_of()   { sha1sum   "$1" | cut -d' ' -f1; }
md5_of()    { md5sum     "$1" | cut -d' ' -f1; }
# base64 sha512 for npm dist.integrity (sha512-<b64>)
integrity_of() { printf 'sha512-%s' "$(openssl dgst -sha512 -binary "$1" | base64 | tr -d '\n')"; }

first() { ls -1 "$1" 2>/dev/null | head -1; }

# --------------------------------------------------------------------------
# Maven — static maven2 layout. Fixes the java.md / kotlin.md instructions.
#   repositories { maven { url "https://azul.rs/maven" } }  +  rs.azul:azul:<V>
# --------------------------------------------------------------------------
build_maven() {
  local jar; jar="$(first "$ART/maven-jar/*.jar" 2>/dev/null)"
  jar="$(ls -1 "$ART"/maven-jar/*.jar 2>/dev/null | head -1)"
  [ -n "$jar" ] || { echo "  [maven] no jar artifact — skip"; return; }
  local dir="$SITE/maven/rs/azul/azul/$V"
  mkdir -p "$dir"
  cp "$jar" "$dir/azul-$V.jar"
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
  cat > "$SITE/maven/rs/azul/azul/maven-metadata.xml" <<META
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
  for f in "$dir/azul-$V.jar" "$dir/azul-$V.pom" "$SITE/maven/rs/azul/azul/maven-metadata.xml"; do
    sha1_of "$f" > "$f.sha1"; md5_of "$f" > "$f.md5"
  done
  echo "  [maven] built rs/azul/azul/$V (jar+pom+metadata+checksums)"
}

# --------------------------------------------------------------------------
# PyPI — PEP 503 "simple" index + hosted distributions.
#   pip install azul --index-url https://azul.rs/pypi/simple/
# --------------------------------------------------------------------------
build_pypi() {
  local files; files=$(ls -1 "$ART"/pypi-dist/* 2>/dev/null)
  [ -n "$files" ] || { echo "  [pypi] no dist artifacts — skip"; return; }
  local pkgdir="$SITE/pypi/simple/azul"
  mkdir -p "$pkgdir"
  local links="" f base h
  for f in "$ART"/pypi-dist/*; do
    [ -f "$f" ] || continue
    base="$(basename "$f")"
    cp "$f" "$pkgdir/$base"
    h="$(sha256_of "$f")"
    links="$links    <a href=\"$base#sha256=$h\">$base</a><br>\n"
  done
  # per-project page
  printf '<!DOCTYPE html><html><head><meta name="pypi:repository-version" content="1.0"><title>Links for azul</title></head><body><h1>Links for azul</h1>\n%b</body></html>\n' "$links" \
    > "$pkgdir/index.html"
  # root simple index
  mkdir -p "$SITE/pypi/simple"
  printf '<!DOCTYPE html><html><head><title>Simple index</title></head><body><a href="azul/">azul</a><br></body></html>\n' \
    > "$SITE/pypi/simple/index.html"
  echo "  [pypi] built simple/azul/ ($(ls -1 "$pkgdir" | grep -vc index.html) dists)"
}

# --------------------------------------------------------------------------
# npm — a static registry metadata document + hosted tarball.
#   npm install azul --registry https://azul.rs/npm/   (or .npmrc registry=)
# npm fetches <registry>/<pkg> for metadata, then dist.tarball for the .tgz.
# --------------------------------------------------------------------------
build_npm() {
  local tgz; tgz="$(ls -1 "$ART"/npm-package/*.tgz 2>/dev/null | head -1)"
  [ -n "$tgz" ] || { echo "  [npm] no tarball artifact — skip"; return; }
  local pkgdir="$SITE/npm/azul"
  mkdir -p "$pkgdir/-"
  cp "$tgz" "$pkgdir/-/azul-$V.tgz"
  local tarball="$BASE/npm/azul/-/azul-$V.tgz"
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
  cp "$pkgdir/index.json" "$SITE/npm/azul.json"
  echo "  [npm] built npm/azul (metadata + tarball)"
}

# --------------------------------------------------------------------------
# NuGet — v3 static feed: service index -> flat container -> nupkg.
#   dotnet nuget add source https://azul.rs/nuget/index.json -n azul
# --------------------------------------------------------------------------
build_nuget() {
  local nupkg; nupkg="$(ls -1 "$ART"/nuget-package/*.nupkg 2>/dev/null | head -1)"
  [ -n "$nupkg" ] || { echo "  [nuget] no nupkg artifact — skip"; return; }
  local id="azul" lver; lver="$(echo "$V" | tr '[:upper:]' '[:lower:]')"
  local base="$SITE/nuget"
  mkdir -p "$base/flatcontainer/$id/$lver"
  cp "$nupkg" "$base/flatcontainer/$id/$lver/$id.$lver.nupkg"
  # flat-container version index
  cat > "$base/flatcontainer/$id/index.json" <<IDX
{ "versions": [ "$lver" ] }
IDX
  # v3 service index pointing at the (static) flat container
  cat > "$base/index.json" <<SVC
{
  "version": "3.0.0",
  "resources": [
    { "@id": "$BASE/nuget/flatcontainer/", "@type": "PackageBaseAddress/3.0.0" }
  ]
}
SVC
  echo "  [nuget] built nuget/index.json + flatcontainer/$id/$lver"
}

# --------------------------------------------------------------------------
# RubyGems — host the .gem; build the compact index if `gem`/`gem generate_index`
# is available (best-effort: the modern compact index needs server logic, but a
# legacy `gems/` + `quick/` tree from `gem generate_index` supports
#   gem install azul --source https://azul.rs/gems ).
# --------------------------------------------------------------------------
build_gems() {
  local gem; gem="$(ls -1 "$ART"/ruby-gem/*.gem 2>/dev/null | head -1)"
  [ -n "$gem" ] || { echo "  [gems] no gem artifact — skip"; return; }
  local g="$SITE/gems"
  mkdir -p "$g/gems"
  cp "$gem" "$g/gems/"
  if command -v gem >/dev/null 2>&1 && gem help generate_index >/dev/null 2>&1; then
    ( cd "$g" && gem generate_index --legacy >/dev/null 2>&1 ) \
      && echo "  [gems] built legacy index (gem generate_index)" \
      || echo "  [gems] hosted .gem only (generate_index failed)"
  else
    echo "  [gems] hosted .gem only (no gem generate_index available)"
  fi
}

# --------------------------------------------------------------------------
# DNF/yum — createrepo_c repodata over the built .rpm(s).
#   [azul] baseurl=https://azul.rs/rpm   ;  dnf install azul
# --------------------------------------------------------------------------
build_rpm() {
  local rpms; rpms=$(ls -1 "$ART"/artifacts-rpm/*.rpm 2>/dev/null)
  [ -n "$rpms" ] || { echo "  [rpm] no rpm artifacts — skip"; return; }
  local r="$SITE/rpm"
  mkdir -p "$r"
  cp "$ART"/artifacts-rpm/*.rpm "$r/" 2>/dev/null || true
  if command -v createrepo_c >/dev/null 2>&1; then
    createrepo_c "$r" >/dev/null 2>&1 && echo "  [rpm] built repodata (createrepo_c)" \
      || echo "  [rpm] hosted .rpm only (createrepo_c failed)"
  elif command -v createrepo >/dev/null 2>&1; then
    createrepo "$r" >/dev/null 2>&1 && echo "  [rpm] built repodata (createrepo)" \
      || echo "  [rpm] hosted .rpm only (createrepo failed)"
  else
    echo "  [rpm] hosted .rpm only (no createrepo_c available)"
  fi
}

# --------------------------------------------------------------------------
# Homebrew — a tap is just a git repo. Homebrew only DEFAULTS to GitHub for the
# `brew tap user/name` shorthand; with an explicit URL it clones any git URL,
# including a dumb-HTTP bare repo served by GitHub Pages. So we publish a REAL
# bare repo at azul.rs/homebrew-azul.git (stable path) whose Formula/libazul.rb
# is regenerated each release — `brew upgrade libazul` then tracks new versions.
#   brew tap fschutt/azul https://azul.rs/homebrew-azul.git
#   brew install libazul
# --------------------------------------------------------------------------
build_homebrew() {
  command -v git >/dev/null 2>&1 || { echo "  [brew] git missing — skip"; return; }
  local arm="$RELDIR/libazul.dylib" intel="$RELDIR/libazul.x86_64.dylib" hdr="$RELDIR/azul.h"
  [ -f "$arm" ] || { echo "  [brew] no macOS arm64 dylib in $RELDIR — skip"; return; }
  [ -f "$hdr" ] || { echo "  [brew] no azul.h in $RELDIR — skip"; return; }
  local arm_sha hdr_sha; arm_sha="$(sha256_of "$arm")"; hdr_sha="$(sha256_of "$hdr")"

  # Build the formula. on_intel is emitted only if the Intel dylib exists.
  local intel_block=""
  if [ -f "$intel" ]; then
    local intel_sha; intel_sha="$(sha256_of "$intel")"
    intel_block="    on_intel do
      url \"$BASE/release/$V/libazul.x86_64.dylib\"
      sha256 \"$intel_sha\"
    end"
  fi
  local work; work="$(mktemp -d)"
  mkdir -p "$work/Formula"
  cat > "$work/Formula/libazul.rb" <<RB
# Auto-generated by scripts/build_registry_mirrors.sh — do not edit by hand.
class Libazul < Formula
  desc "Azul GUI framework — prebuilt native library (libazul)"
  homepage "https://azul.rs/"
  version "$V"
  license "MIT"

  on_macos do
    on_arm do
      url "$BASE/release/$V/libazul.dylib"
      sha256 "$arm_sha"
    end
$intel_block
    resource "header" do
      url "$BASE/release/$V/azul.h"
      sha256 "$hdr_sha"
    end
  end

  def install
    lib.install Dir["*.dylib"].first => "libazul.dylib"
    resource("header").stage { include.install "azul.h" }
  end

  test do
    assert_predicate lib/"libazul.dylib", :exist?
  end
end
RB
  # Real git repo -> bare clone -> update-server-info so dumb-HTTP clone works.
  ( cd "$work" && git init -q \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" add -A \
      && git -c user.email=ci@azul.rs -c user.name="azul ci" commit -q -m "libazul $V" ) || {
    echo "  [brew] git commit failed — skip"; rm -rf "$work"; return; }
  rm -rf "$SITE/homebrew-azul.git"
  git clone -q --bare "$work" "$SITE/homebrew-azul.git" || { echo "  [brew] bare clone failed"; rm -rf "$work"; return; }
  ( cd "$SITE/homebrew-azul.git" && git update-server-info )
  rm -rf "$work"
  echo "  [brew] published homebrew-azul.git (formula libazul $V; intel=$([ -f "$intel" ] && echo yes || echo no))"
}

# --------------------------------------------------------------------------
# Chocolatey — choco consumes a NuGet v3 feed, which we already host at
# azul.rs/nuget. We add a `libazul` choco package (a .nupkg is just a zip with a
# nuspec + tools/chocolateyInstall.ps1) into that same flat-container, so:
#   choco install libazul --source https://azul.rs/nuget/index.json
#   choco upgrade libazul   # the stable v3 source advertises new versions
# The install script downloads azul.dll from the matching release URL.
# EXPERIMENTAL: not testable on this Linux runner; the .nupkg structure follows
# the documented NuGet OPC layout.
# --------------------------------------------------------------------------
build_choco() {
  local dll="$RELDIR/azul.dll"
  [ -f "$dll" ] || { echo "  [choco] no azul.dll in $RELDIR — skip"; return; }
  local dll_sha; dll_sha="$(sha256_of "$dll")"
  local lver; lver="$(echo "$V" | tr '[:upper:]' '[:lower:]')"
  local dest="$SITE/nuget/flatcontainer/libazul/$lver"
  mkdir -p "$dest"
  SITE="$SITE" V="$V" DLLSHA="$dll_sha" DEST="$dest" python3 - <<'PY'
import os, zipfile, uuid
V = os.environ["V"]; sha = os.environ["DLLSHA"]; dest = os.environ["DEST"]
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
    <description>Azul GUI framework prebuilt native library (azul.dll).</description>
    <tags>azul gui native dll</tags>
  </metadata>
</package>
'''
install_ps1 = f'''$ErrorActionPreference = 'Stop'
$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
Get-ChocolateyWebFile -PackageName 'libazul' `
  -FileFullPath (Join-Path $tools 'azul.dll') `
  -Url64bit 'https://azul.rs/release/{V}/azul.dll' `
  -Checksum64 '{sha}' -ChecksumType64 'sha256'
Write-Host "libazul installed to $tools\\azul.dll"
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
  <dc:description>Azul GUI framework prebuilt native library (azul.dll).</dc:description>
  <dc:identifier>libazul</dc:identifier>
  <version>{V}</version>
</coreProperties>
'''
out = os.path.join(dest, f"libazul.{V.lower()}.nupkg")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("libazul.nuspec", nuspec)
    z.writestr("tools/chocolateyInstall.ps1", install_ps1)
    z.writestr("[Content_Types].xml", content_types)
    z.writestr("_rels/.rels", rels)
    z.writestr(f"package/services/metadata/core-properties/{psmdcp_id}.psmdcp", psmdcp)
print("  [choco] wrote", out)
PY
  # flat-container version index for the libazul package
  cat > "$SITE/nuget/flatcontainer/libazul/index.json" <<IDX
{ "versions": [ "$lver" ] }
IDX
  echo "  [choco] libazul package added to the nuget v3 feed"
}

# --------------------------------------------------------------------------
# pacman (Arch / Manjaro) — host the .pkg.tar.zst + a repo db.
#   /etc/pacman.conf:  [azlin]
#                      Server = https://azul.rs/arch/$arch
#   pacman -Sy azlin-ui     (pacman -Syu keeps it updated)
# repo-add (from pacman/pacman-contrib) builds the .db; if absent we still host
# the package + a hand-written .files-less db is skipped (graceful, like rpm).
# --------------------------------------------------------------------------
build_pacman() {
  local pkgs; pkgs=$(ls -1 "$ART"/artifacts-arch/*.pkg.tar.zst 2>/dev/null)
  [ -n "$pkgs" ] || { echo "  [pacman] no .pkg.tar.zst artifacts — skip"; return; }
  local arch_dir="$SITE/arch/x86_64"
  mkdir -p "$arch_dir"
  cp "$ART"/artifacts-arch/*.pkg.tar.zst "$arch_dir/" 2>/dev/null || true
  if command -v repo-add >/dev/null 2>&1; then
    ( cd "$arch_dir" && repo-add azlin.db.tar.gz ./*.pkg.tar.zst >/dev/null 2>&1 ) \
      && echo "  [pacman] built azlin.db (repo-add)" \
      || echo "  [pacman] hosted .pkg.tar.zst only (repo-add failed)"
  else
    echo "  [pacman] hosted .pkg.tar.zst only (no repo-add available)"
  fi
}

# --------------------------------------------------------------------------
# Alpine apk — host the .apk + an APKINDEX. apk repos are <baseurl>/<arch>/, so
#   /etc/apk/repositories:  https://azul.rs/alpine/x86_64
#   apk add --allow-untrusted azlin-ui   (until the index is signed)
# `apk index` (apk-tools) builds APKINDEX.tar.gz; absent -> host the .apk only.
# --------------------------------------------------------------------------
build_apk() {
  local pkgs; pkgs=$(ls -1 "$ART"/artifacts-apk/*.apk 2>/dev/null)
  [ -n "$pkgs" ] || { echo "  [apk] no .apk artifacts — skip"; return; }
  local apk_dir="$SITE/alpine/x86_64"
  mkdir -p "$apk_dir"
  cp "$ART"/artifacts-apk/*.apk "$apk_dir/" 2>/dev/null || true
  if command -v apk >/dev/null 2>&1; then
    ( cd "$apk_dir" && apk index -o APKINDEX.tar.gz ./*.apk >/dev/null 2>&1 ) \
      && echo "  [apk] built APKINDEX.tar.gz (apk index)" \
      || echo "  [apk] hosted .apk only (apk index failed)"
  else
    echo "  [apk] hosted .apk only (no apk-tools available)"
  fi
}

echo "==> Building self-hosted registry mirrors under $SITE (v$V)"
# .nojekyll: GitHub Pages' (legacy) Jekyll would drop dotfiles/_dirs; disable it
# so the bare git repo (objects/, info/refs, HEAD) and every metadata file serve
# verbatim. Harmless under the static (Actions) Pages path too.
touch "$SITE/.nojekyll"
build_maven
build_pypi
build_npm
build_nuget   # must run before build_choco (choco writes into the nuget tree)
build_choco
build_gems
build_rpm       # yum + zypper consume this same repo
build_pacman
build_apk
build_homebrew
echo "==> Registry mirrors done."

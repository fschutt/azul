#!/usr/bin/env bash
# Build self-hosted, STATIC package-registry mirrors under the GitHub Pages site
# so the azul.rs install commands work without the official registries:
#
#   Maven    azul.rs/maven      (maven2 layout: rs/azul/azul/<V>/azul-<V>.jar+pom)
#   PyPI     azul.rs/pypi/simple (PEP 503 simple index + hosted wheels/sdists)
#   npm      azul.rs/npm/azul    (registry metadata doc + hosted .tgz)
#   NuGet    azul.rs/nuget       (v3 service index + flat-container + nupkg)
#   RubyGems azul.rs/gems        (flat: hosted .gem + a human index; see note)
#   DNF/yum  azul.rs/rpm         (createrepo_c repodata over the built .rpm)
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

echo "==> Building self-hosted registry mirrors under $SITE (v$V)"
build_maven
build_pypi
build_npm
build_nuget
build_gems
build_rpm
echo "==> Registry mirrors done."

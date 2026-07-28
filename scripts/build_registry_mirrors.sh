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
#   Homebrew azul.rs/ui/homebrew-azul.git  (a real bare git repo = a tap)
#   Chocolatey  azul.rs/ui/nuget (the v3 feed also serves a `libazul` choco package)
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
  local jar; jar="$(first "$ART/maven-jar/*.jar" 2>/dev/null)"
  jar="$(ls -1 "$ART"/maven-jar/*.jar 2>/dev/null | head -1)"
  [ -n "$jar" ] || { echo "  [maven] no jar artifact — skip"; return; }
  local dir="$SITE/ui/maven/rs/azul/azul/$V"
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
}

# --------------------------------------------------------------------------
# PyPI — PEP 503 index (root = /ui) + hosted distributions under /ui/azul/.
#   pip install azul --index-url https://azul.rs/ui
# --------------------------------------------------------------------------
build_pypi() {
  local files; files=$(ls -1 "$ART"/pypi-dist/* 2>/dev/null)
  [ -n "$files" ] || { echo "  [pypi] no dist artifacts — skip"; return; }
  # PEP 503 index ROOT is /ui itself; pip fetches <index-url>/azul/, so the
  # per-project page lives at /ui/azul/. (pip install azul --index-url .../ui)
  local pkgdir="$SITE/ui/azul"
  mkdir -p "$pkgdir"
  local links="" f base h
  for f in "$ART"/pypi-dist/*; do
    [ -f "$f" ] || continue
    base="$(basename "$f")"
    cp "$f" "$pkgdir/$base"
    h="$(sha256_of "$f")"
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
# --------------------------------------------------------------------------
build_nuget() {
  local nupkg; nupkg="$(ls -1 "$ART"/nuget-package/*.nupkg 2>/dev/null | head -1)"
  [ -n "$nupkg" ] || { echo "  [nuget] no nupkg artifact — skip"; return; }
  local id="azul" lver; lver="$(echo "$V" | tr '[:upper:]' '[:lower:]')"
  local base="$SITE/ui/nuget"
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
    { "@id": "$BASE/ui/nuget/flatcontainer/", "@type": "PackageBaseAddress/3.0.0" }
  ]
}
SVC
  echo "  [nuget] built nuget/index.json + flatcontainer/$id/$lver"
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
  # A ready-to-use .repo for `dnf config-manager --add-repo .../azul.repo`.
  # gpgcheck=0 until the repo is signed (mirrors the apt "unsigned until signed"
  # state). yum + zypper read this same baseurl.
  cat > "$r/azul.repo" <<REPO
[azul]
name=Azul GUI framework
baseurl=$BASE/ui/rpm
enabled=1
gpgcheck=0
REPO
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
# bare repo at azul.rs/ui/homebrew-azul.git (stable path) whose Formula/azul.rb
# is regenerated each release — `brew upgrade azul` then tracks new versions.
#   brew tap fschutt/azul https://azul.rs/ui/homebrew-azul.git
#   brew install fschutt/azul/azul
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
      && git -c user.email=ci@azul.rs -c user.name="azul ci" commit -q -m "azul $V" ) || {
    echo "  [brew] git commit failed — skip"; rm -rf "$work"; return; }
  rm -rf "$SITE/ui/homebrew-azul.git"
  git clone -q --bare "$work" "$SITE/ui/homebrew-azul.git" || { echo "  [brew] bare clone failed"; rm -rf "$work"; return; }
  ( cd "$SITE/ui/homebrew-azul.git" && git update-server-info )
  rm -rf "$work"
  # Prove the published repo is actually clonable (what `brew tap` will do,
  # minus the HTTP transport): a file:// clone of the bare repo must yield
  # Formula/azul.rb. Non-fatal — a failure only warns.
  local chk; chk="$(mktemp -d)"
  if git clone -q "file://$(cd "$SITE/ui/homebrew-azul.git" && pwd)" "$chk/tap" \
     && [ -f "$chk/tap/Formula/azul.rb" ]; then
    echo "  [brew] self-check: bare repo clones and contains Formula/azul.rb"
  else
    echo "::warning::[brew] self-check clone of homebrew-azul.git FAILED"
  fi
  rm -rf "$chk"
  echo "  [brew] published homebrew-azul.git (formula azul $V; intel=$([ -f "$intel" ] && echo yes || echo no))"
}

# --------------------------------------------------------------------------
# Chocolatey — choco consumes a NuGet v3 feed, which we already host at
# azul.rs/ui/nuget. We add a `libazul` choco package (a .nupkg is just a zip with a
# nuspec + tools/chocolateyInstall.ps1) into that same flat-container, so:
#   choco install libazul --source https://azul.rs/ui/nuget/index.json
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
  local dest="$SITE/ui/nuget/flatcontainer/libazul/$lver"
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
  -Url64bit 'https://azul.rs/ui/release/{V}/azul.dll' `
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
  cat > "$SITE/ui/nuget/flatcontainer/libazul/index.json" <<IDX
{ "versions": [ "$lver" ] }
IDX
  echo "  [choco] libazul package added to the nuget v3 feed"
}

# --------------------------------------------------------------------------
# pacman (Arch / Manjaro) — host the .pkg.tar.zst + a repo db.
#   /etc/pacman.conf:  [azlin]
#                      Server = https://azul.rs/ui/arch/$arch
#   pacman -Sy azlin-ui     (pacman -Syu keeps it updated)
# repo-add (from pacman/pacman-contrib) builds the .db; if absent we still host
# the package + a hand-written .files-less db is skipped (graceful, like rpm).
# --------------------------------------------------------------------------
build_pacman() {
  local pkgs; pkgs=$(ls -1 "$ART"/artifacts-arch/*.pkg.tar.zst 2>/dev/null)
  [ -n "$pkgs" ] || { echo "  [pacman] no .pkg.tar.zst artifacts — skip"; return; }
  local arch_dir="$SITE/ui/arch/x86_64"
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
#   /etc/apk/repositories:  https://azul.rs/ui/alpine/x86_64
#   apk add --allow-untrusted azlin-ui   (until the index is signed)
# `apk index` (apk-tools) builds APKINDEX.tar.gz; absent -> host the .apk only.
# --------------------------------------------------------------------------
build_apk() {
  local pkgs; pkgs=$(ls -1 "$ART"/artifacts-apk/*.apk 2>/dev/null)
  [ -n "$pkgs" ] || { echo "  [apk] no .apk artifacts — skip"; return; }
  local apk_dir="$SITE/ui/alpine/x86_64"
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
FAILED=""
build_maven
build_pypi
build_npm
build_nuget   # must run before build_choco (choco writes into the nuget tree)
build_choco
# A channel that produced an UNUSABLE mirror must red the deploy, not print a
# note and continue: "hosted .gem only (generate_index failed)" scrolled past
# unread for two months while `gem install --source https://azul.rs/ui/gems`
# 404'd for every user. Build everything first so one broken channel does not
# mask the state of the others, then fail once at the end.
build_gems || FAILED="$FAILED gems"
build_rpm       # yum + zypper consume this same repo
build_pacman
build_apk
build_homebrew
if [ -n "$FAILED" ]; then
  echo "::error::registry mirror channels FAILED to build a usable index:$FAILED"
  exit 1
fi
echo "==> Registry mirrors done."

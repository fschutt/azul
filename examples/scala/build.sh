#!/usr/bin/env bash
# Build + run the Scala hello-world. Rides on Java's compiled classes
# (../java/target/classes) so `mvn package` must have run in the
# Java example first.
#
# AZ_DEBUG counter probe (with the GUI window open):
#   curl -s -X POST localhost:8080/ -d '{"op":"get_html_string"}'           # counter=5
#   for _ in 1 2 3; do
#     curl -s -X POST localhost:8080/ \
#       -d '{"op":"click","selector":".__azul-native-button"}'
#     sleep 0.3
#   done
#   curl -s -X POST localhost:8080/ -d '{"op":"get_html_string"}'           # counter=8

set -euo pipefail

cd "$(dirname "$0")"

# Only fall back to a hardcoded JDK when one is not already usable. A
# Homebrew Cellar path is a macOS-developer convenience, not a location any
# other machine has, so it must never take priority over the JDK the
# environment already provides (CI installs one via setup-java).
if [ -z "${JAVA_HOME:-}" ] && ! command -v javac >/dev/null 2>&1; then
    for guess in /opt/homebrew/Cellar/openjdk@17/*/libexec/openjdk.jdk/Contents/Home \
                 /usr/lib/jvm/java-17-openjdk*; do
        if [ -d "$guess" ]; then
            export JAVA_HOME="$guess"
            export PATH="$JAVA_HOME/bin:$PATH"
            break
        fi
    done
fi

JNA_JAR="${JNA_JAR:-$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar}"
# Locate the Scala jars relative to the `scalac` that is ACTUALLY on PATH,
# rather than assuming a macOS Homebrew Cellar prefix and a pinned 3.8.3.
# The old hardcoded defaults meant this script could only ever work on the
# one machine it was written on: on Windows and Linux CI, scalac exists and
# works but both jars resolved to paths that do not exist, so the build died
# on a missing classpath entry rather than on anything about the example.
# git-bash's `command -v` does NOT apply PATHEXT, so a coursier-installed
# `scalac.bat` is invisible to a bare lookup even though it is on PATH and
# runnable. On windows-2022 coursier logged "Wrote scala / Wrote scalac", the
# e2e matrix's own `have()` helper (which DOES try the extensions) agreed the
# toolchain was present, and then this script looked for a bare `scalac`,
# found nothing, and reported "<not on PATH>". Probe and invocation disagreed,
# so scala — a REQUIRED shipped binding — reported FAILS and blocked the
# release on a lookup convention rather than on a missing tool or a real bug.
resolve_cmd() {
    local c="$1" p ext
    if p=$(command -v "$c" 2>/dev/null); then printf '%s\n' "$p"; return 0; fi
    for ext in exe bat cmd com; do
        if p=$(command -v "$c.$ext" 2>/dev/null); then printf '%s\n' "$p"; return 0; fi
    done
    return 1
}

SCALAC="$(resolve_cmd scalac || true)"
# Populated by find_scala_jar so the failure diagnostic can show the search path.
declare -a SEARCHED=()

find_scala_jar() {
    # $1: artifact directory name, $2: jar basename prefix
    local scalac_bin root
    scalac_bin="$SCALAC"
    [ -n "$scalac_bin" ] || return 1
    # Resolve symlinks (Homebrew, coursier and asdf all shim scalac).
    while [ -L "$scalac_bin" ]; do
        scalac_bin=$(cd "$(dirname "$scalac_bin")" && \
                     readlink "$scalac_bin" | sed "s|^\([^/]\)|$(pwd)/\1|")
    done
    root=$(dirname "$(dirname "$scalac_bin")")
    # Coursier's cache is NOT $HOME/.cache/coursier on every platform, and those
    # two Linux/XDG paths were the whole search list besides $root. On
    # windows-2022 that made this the SECOND failure in a row for the same
    # example: once `scalac` resolved (see resolve_cmd above, it lands on
    # /c/Users/runneradmin/cs/bin/scalac.bat), the jars still came back empty
    # because coursier put them under %LOCALAPPDATA%\Coursier\cache, which
    # nothing here looked at. Search every documented location; missing ones
    # cost nothing because `find` is already silenced.
    #
    #   Linux/XDG   $HOME/.cache/coursier
    #   macOS       $HOME/Library/Caches/Coursier
    #   Windows     $LOCALAPPDATA/Coursier/cache  (git-bash sees it as a path)
    #   COURSIER_CACHE overrides all of the above when set.
    local -a search=("$root")
    [ -n "${COURSIER_CACHE:-}" ] && search+=("$COURSIER_CACHE")
    search+=("$HOME/.cache/coursier" "$HOME/.ivy2" "$HOME/.m2/repository")
    search+=("$HOME/Library/Caches/Coursier")
    [ -n "${LOCALAPPDATA:-}" ] && search+=("$LOCALAPPDATA/Coursier/cache")
    search+=("$HOME/AppData/Local/Coursier/cache")
    # `cs` also keeps an "artifacts" tree next to its launcher on Windows.
    search+=("$(dirname "$scalac_bin")/../artifacts")

    SEARCHED=("${search[@]}")
    local -a existing=()
    local d
    for d in "${search[@]}"; do
        [ -d "$d" ] && existing+=("$d")
    done
    [ ${#existing[@]} -gt 0 ] || return 1
    find "${existing[@]}" -name "$2*.jar" \
        -not -name "*-sources.jar" -not -name "*-javadoc.jar" \
        2>/dev/null | sort -V | tail -1
}

# `|| true`: under `set -e` a failing command substitution inside an
# assignment aborts the script INSTANTLY and silently, swallowing the
# diagnostic below — which is the exact confusing failure this rewrite exists
# to remove. Let the emptiness flow through to the check instead.
SCALA_LIB="${SCALA_LIB:-$(find_scala_jar scala-library scala-library || true)}"
SCALA3_LIB="${SCALA3_LIB:-$(find_scala_jar scala3-library scala3-library_3 || true)}"

for var in SCALA_LIB SCALA3_LIB; do
    eval "val=\${$var}"
    if [ -z "$val" ] || [ ! -f "$val" ]; then
        echo "$var could not be located (got: '${val:-<empty>}')." >&2
        echo "scalac: ${SCALAC:-<not on PATH>}" >&2
        # Print WHERE we looked. Twice now this example has failed on CI with a
        # message that named the missing variable but not the search path, which
        # is the one fact needed to tell "toolchain absent" from "jars are
        # somewhere else" — and on Windows it was the latter both times.
        echo "searched:" >&2
        for d in "${SEARCHED[@]:-}"; do
            [ -n "$d" ] && echo "  $d $([ -d "$d" ] && echo '(exists)' || echo '(absent)')" >&2
        done
        echo "Set $var explicitly to the jar path, or COURSIER_CACHE to the cache root." >&2
        exit 1
    fi
done
JAVA_CLASSES="${JAVA_CLASSES:-../java/target/classes}"

if [ ! -d "$JAVA_CLASSES" ]; then
    echo "missing $JAVA_CLASSES — run 'mvn package' in ../java first" >&2
    exit 1
fi
for jar in "$JNA_JAR" "$SCALA_LIB" "$SCALA3_LIB"; do
    [ -f "$jar" ] || { echo "missing $jar" >&2; exit 1; }
done

echo "[scala] compiling HelloWorld.scala"
"$SCALAC" -cp "$JAVA_CLASSES:$JNA_JAR" HelloWorld.scala -d HelloWorld.jar

# -XstartOnFirstThread is a macOS-only JVM flag (needed there for libazul's
# NSApplication loop). HotSpot on Linux/Windows rejects it outright with
# "Unrecognized option ... Could not create the Java Virtual Machine", so gate
# it on the OS. Mirrors lang_java() in scripts/e2e_language_matrix.sh.
FIRST_THREAD=()
if [ "$(uname -s)" = "Darwin" ]; then
    FIRST_THREAD=(-XstartOnFirstThread)
fi
echo "[scala] running (DYLD_LIBRARY_PATH=. ${FIRST_THREAD[*]})"
exec java "${FIRST_THREAD[@]}" -Djna.library.path=. \
    -cp "HelloWorld.jar:$JAVA_CLASSES:$JNA_JAR:$SCALA_LIB:$SCALA3_LIB" \
    com.azul.HelloWorld

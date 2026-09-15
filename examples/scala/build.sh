#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")"

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
resolve_cmd() {
    local c="$1" p ext
    if p=$(command -v "$c" 2>/dev/null); then printf '%s\n' "$p"; return 0; fi
    for ext in exe bat cmd com; do
        if p=$(command -v "$c.$ext" 2>/dev/null); then printf '%s\n' "$p"; return 0; fi
    done
    return 1
}

SCALAC="$(resolve_cmd scalac || true)"
declare -a SEARCHED=()

CPSEP=":"
case "$(uname -s 2>/dev/null)" in
    MINGW*|MSYS*|CYGWIN*) CPSEP=";" ;;
esac

native_path() {
    if [ "$CPSEP" = ";" ]; then
        cygpath -m "$1" 2>/dev/null || printf '%s\n' "$1"
    else
        printf '%s\n' "$1"
    fi
}

join_cp() {
    local out="" p
    for p in "$@"; do
        [ -n "$p" ] || continue
        p="$(native_path "$p")"
        if [ -z "$out" ]; then out="$p"; else out="${out}${CPSEP}${p}"; fi
    done
    printf '%s\n' "$out"
}

find_scala_jar() {
    local scalac_bin root
    scalac_bin="$SCALAC"
    [ -n "$scalac_bin" ] || return 1
    while [ -L "$scalac_bin" ]; do
        scalac_bin=$(cd "$(dirname "$scalac_bin")" && \
                     readlink "$scalac_bin" | sed "s|^\([^/]\)|$(pwd)/\1|")
    done
    root=$(dirname "$(dirname "$scalac_bin")")
    local -a search=("$root")
    [ -n "${COURSIER_CACHE:-}" ] && search+=("$COURSIER_CACHE")
    search+=("$HOME/.cache/coursier" "$HOME/.ivy2" "$HOME/.m2/repository")
    search+=("$HOME/Library/Caches/Coursier")
    [ -n "${LOCALAPPDATA:-}" ] && search+=("$LOCALAPPDATA/Coursier/cache")
    search+=("$HOME/AppData/Local/Coursier/cache")
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

SCALA_LIB="${SCALA_LIB:-$(find_scala_jar scala-library scala-library || true)}"
SCALA3_LIB="${SCALA3_LIB:-$(find_scala_jar scala3-library scala3-library_3 || true)}"

for var in SCALA_LIB SCALA3_LIB; do
    eval "val=\${$var}"
    if [ -z "$val" ] || [ ! -f "$val" ]; then
        echo "$var could not be located (got: '${val:-<empty>}')." >&2
        echo "scalac: ${SCALAC:-<not on PATH>}" >&2
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
"$SCALAC" -cp "$(join_cp "$JAVA_CLASSES" "$JNA_JAR")" HelloWorld.scala -d HelloWorld.jar

FIRST_THREAD=()
if [ "$(uname -s)" = "Darwin" ]; then
    FIRST_THREAD=(-XstartOnFirstThread)
fi
echo "[scala] running (DYLD_LIBRARY_PATH=. ${FIRST_THREAD[*]})"
exec java "${FIRST_THREAD[@]}" -Djna.library.path=. \
    -cp "$(join_cp HelloWorld.jar "$JAVA_CLASSES" "$JNA_JAR" "$SCALA_LIB" "$SCALA3_LIB")" \
    com.azul.HelloWorld

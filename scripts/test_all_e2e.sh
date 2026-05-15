#!/usr/bin/env bash
# test_all_e2e.sh — drive every binding's AZ_DEBUG counter probe.
#
# For each lang with an E2E example, start the hello-world, run the
# probe (5 → 8 after three clicks), tear it down, report pass/fail.
# Exits 0 if every active lang passes, 1 if any fail.
#
# Bindings marked `[⊘]` (libazul blocker, runtime conflict, etc.) or
# `[—]` (toolchain unavailable on macOS) are listed under SKIPPED.
#
# Required env: JAVA_HOME (for Java/Kotlin/Scala), SCALA_LIB / SCALA3_LIB
# (for Scala). The script auto-detects the common macOS Homebrew
# locations and falls back to whatever is on PATH.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROBE="$ROOT/scripts/probe_az_debug.sh"
JNA_JAR="${JNA_JAR:-$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar}"

# Auto-detect JDK 17 (used by Java/Kotlin/Scala). If JAVA_HOME is set
# we honour it; otherwise pick the brew openjdk@17 install.
if [ -z "${JAVA_HOME:-}" ] && [ -d "/opt/homebrew/Cellar/openjdk@17/17.0.19/libexec/openjdk.jdk/Contents/Home" ]; then
    export JAVA_HOME="/opt/homebrew/Cellar/openjdk@17/17.0.19/libexec/openjdk.jdk/Contents/Home"
    export PATH="$JAVA_HOME/bin:$PATH"
fi
SCALA_LIB="${SCALA_LIB:-/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala-library/3.8.3/scala-library-3.8.3.jar}"
SCALA3_LIB="${SCALA3_LIB:-/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala3-library_3/3.8.3/scala3-library_3-3.8.3.jar}"

PASS_LIST=()
FAIL_LIST=()
SKIP_LIST=()

run_with_probe() {
    local lang="$1" port="$2"
    shift 2
    # Start the binary in the background; redirect output so we don't
    # see noise unless something fails.
    local log_file="/tmp/test_e2e_${lang}.log"
    rm -f "$log_file"
    ( "$@" ) > "$log_file" 2>&1 &
    local pid=$!
    # Give the GUI a couple of seconds to bind the AZ_DEBUG port.
    sleep 4
    if bash "$PROBE" "$port" 5 8 >/dev/null 2>&1; then
        echo "[$lang] PASS"
        PASS_LIST+=("$lang")
    else
        echo "[$lang] FAIL — see $log_file"
        FAIL_LIST+=("$lang")
    fi
    kill -9 "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
}

# Lua — luajit hello-world.lua
if command -v luajit >/dev/null 2>&1; then
    pushd "$ROOT/examples/lua" >/dev/null
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18001 run_with_probe lua 18001 luajit hello-world.lua
    popd >/dev/null
else
    SKIP_LIST+=("lua: luajit not in PATH")
fi

# Node — node hello-world.js
if command -v node >/dev/null 2>&1; then
    pushd "$ROOT/examples/node" >/dev/null
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18002 run_with_probe node 18002 node hello-world.js
    popd >/dev/null
else
    SKIP_LIST+=("node: node not in PATH")
fi

# Ruby — ruby -I. hello-world.rb
if command -v ruby >/dev/null 2>&1; then
    pushd "$ROOT/examples/ruby" >/dev/null
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18003 run_with_probe ruby 18003 ruby -I. hello-world.rb
    popd >/dev/null
else
    SKIP_LIST+=("ruby: ruby not in PATH")
fi

# Scala — pre-compile + run
if command -v scalac >/dev/null 2>&1 && command -v java >/dev/null 2>&1; then
    pushd "$ROOT/examples/scala" >/dev/null
    if [ ! -d "../java/target/classes" ]; then
        SKIP_LIST+=("scala: ../java/target/classes missing — run mvn package in examples/java first")
    elif ! scalac -cp "../java/target/classes:$JNA_JAR" HelloWorld.scala -d HelloWorld.jar > /tmp/scala_compile.log 2>&1; then
        echo "[scala] compile FAIL — see /tmp/scala_compile.log"
        FAIL_LIST+=("scala")
    else
        DYLD_LIBRARY_PATH=. AZ_DEBUG=18004 run_with_probe scala 18004 \
            java -XstartOnFirstThread -Djna.library.path=. \
                 -cp "HelloWorld.jar:../java/target/classes:$JNA_JAR:$SCALA_LIB:$SCALA3_LIB" \
                 com.azul.HelloWorld
        rm -f HelloWorld.jar
    fi
    popd >/dev/null
else
    SKIP_LIST+=("scala: scalac/java not in PATH")
fi

# Java — mvn package, then java with JNA.
# Compile step is gated by mvn availability; runtime is gated by the jar
# existing (so a previously-built CI artefact still runs without mvn).
run_java() {
    pushd "$ROOT/examples/java" >/dev/null
    if [ ! -f "target/hello-world-1.0.0.jar" ]; then
        if ! command -v mvn >/dev/null 2>&1; then
            SKIP_LIST+=("java: target/hello-world-1.0.0.jar missing and mvn not in PATH")
            popd >/dev/null; return
        fi
        if ! mvn -q package > /tmp/java_compile.log 2>&1; then
            echo "[java] mvn package FAIL — see /tmp/java_compile.log"
            FAIL_LIST+=("java")
            popd >/dev/null; return
        fi
    fi
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18005 run_with_probe java 18005 \
        java -XstartOnFirstThread -Djna.library.path=. \
             -cp "target/hello-world-1.0.0.jar:$JNA_JAR" \
             com.azul.HelloWorld
    popd >/dev/null
}
if command -v java >/dev/null 2>&1 && [ -f "$JNA_JAR" ]; then
    run_java
else
    SKIP_LIST+=("java: java not in PATH or JNA_JAR missing ($JNA_JAR)")
fi

# Kotlin — kotlinc -> hello-world.jar, then java with JNA.
run_kotlin() {
    pushd "$ROOT/examples/kotlin" >/dev/null
    if [ ! -f "hello-world.jar" ]; then
        if ! command -v kotlinc >/dev/null 2>&1; then
            SKIP_LIST+=("kotlin: hello-world.jar missing and kotlinc not in PATH")
            popd >/dev/null; return
        fi
        if ! kotlinc -J-Xmx4g -cp "$JNA_JAR" Azul.kt HelloWorld.kt -include-runtime \
                -d hello-world.jar > /tmp/kotlin_compile.log 2>&1; then
            echo "[kotlin] kotlinc FAIL — see /tmp/kotlin_compile.log"
            FAIL_LIST+=("kotlin")
            popd >/dev/null; return
        fi
    fi
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18006 run_with_probe kotlin 18006 \
        java -XstartOnFirstThread -Djna.library.path=. \
             -cp "hello-world.jar:$JNA_JAR" \
             com.azul.HelloWorldKt
    popd >/dev/null
}
if command -v java >/dev/null 2>&1 && [ -f "$JNA_JAR" ]; then
    run_kotlin
else
    SKIP_LIST+=("kotlin: java not in PATH or JNA_JAR missing ($JNA_JAR)")
fi

# C# — dotnet run. dotnet builds in-place; no separate compile guard.
if command -v dotnet >/dev/null 2>&1; then
    pushd "$ROOT/examples/csharp" >/dev/null
    DYLD_LIBRARY_PATH=. AZ_DEBUG=18007 run_with_probe csharp 18007 \
        dotnet run -c Release
    popd >/dev/null
else
    SKIP_LIST+=("csharp: dotnet not in PATH")
fi

# Go / Zig / OCaml — placeholder hooks; uncomment when a runnable
# binary is built in CI for each. The probe script itself is binding-
# agnostic — only the launch invocation differs.

echo
echo "============================================================"
echo "  E2E counter probe results"
echo "============================================================"
echo "PASS (${#PASS_LIST[@]}): ${PASS_LIST[*]:-<none>}"
echo "FAIL (${#FAIL_LIST[@]}): ${FAIL_LIST[*]:-<none>}"
echo "SKIP (${#SKIP_LIST[@]}):"
for s in "${SKIP_LIST[@]:-}"; do
    [ -n "$s" ] && echo "  - $s"
done

if [ "${#FAIL_LIST[@]}" -gt 0 ]; then
    exit 1
fi
exit 0

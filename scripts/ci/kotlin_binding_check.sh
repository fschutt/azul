#!/usr/bin/env bash
# Compiles the generated Kotlin binding (target/codegen/kotlin/Azul.kt) the way the Maven Central job of
# .github/workflows/rust.yml does (kotlinc + JNA 5.14.0), so that a change of the Kotlin generator is checked in
# minutes on a runner and not in the hours of a release run.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/kotlin_binding_check.sh' --codegen
#
# The bindings must be generated first (the --codegen flag of remote.sh does it). Exit 0: kotlinc compiled the file
# and the jar holds more than 1000 classes (the same floor as the workflow's).
set -u

KOTLIN_VERSION="${KOTLIN_VERSION:-2.3.21}"
JNA_VERSION="5.14.0"

src="target/codegen/kotlin/Azul.kt"
if [ ! -f "$src" ]; then
  echo "KOTLIN CHECK: $src does not exist: generate the bindings first (remote.sh --codegen)"
  exit 2
fi

work="$(mktemp -d)"
echo "=== kotlin binding check: kotlinc $KOTLIN_VERSION, JNA $JNA_VERSION, work dir $work"

curl -fsSL --retry 5 -o "$work/jna.jar" \
  "https://repo1.maven.org/maven2/net/java/dev/jna/jna/$JNA_VERSION/jna-$JNA_VERSION.jar" || {
  echo "KOTLIN CHECK: could not download JNA"
  exit 3
}
curl -fsSL --retry 5 -o "$work/kotlin.zip" \
  "https://github.com/JetBrains/kotlin/releases/download/v$KOTLIN_VERSION/kotlin-compiler-$KOTLIN_VERSION.zip" || {
  echo "KOTLIN CHECK: could not download kotlinc $KOTLIN_VERSION"
  exit 3
}
unzip -q "$work/kotlin.zip" -d "$work" || {
  echo "KOTLIN CHECK: could not unpack kotlinc"
  exit 3
}

if [ -n "${JAVA_HOME_17_X64:-}" ]; then
  export JAVA_HOME="$JAVA_HOME_17_X64"
  export PATH="$JAVA_HOME/bin:$PATH"
fi
java -version 2>&1 | head -1

out="$work/azul-kotlin.jar"
"$work/kotlinc/bin/kotlinc" -J-Xmx8g -cp "$work/jna.jar" "$src" -d "$out" > "$work/kotlinc.log" 2>&1
status=$?
echo "kotlinc exit $status"
grep -E "error:" "$work/kotlinc.log" | head -40
if [ "$status" -ne 0 ]; then
  tail -n 30 "$work/kotlinc.log"
  echo "KOTLIN CHECK EXIT: $status"
  exit "$status"
fi

classes="$(unzip -l "$out" | grep -cE '\.class$')"
echo "classes in the jar: $classes"
if [ "$classes" -le 1000 ]; then
  echo "KOTLIN CHECK EXIT: 1 (the jar has $classes classes)"
  exit 1
fi
echo "KOTLIN CHECK EXIT: 0"
exit 0

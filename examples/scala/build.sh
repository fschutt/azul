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

JAVA_HOME_GUESS="/opt/homebrew/Cellar/openjdk@17/17.0.19/libexec/openjdk.jdk/Contents/Home"
if [ -d "$JAVA_HOME_GUESS" ]; then
    export JAVA_HOME="$JAVA_HOME_GUESS"
    export PATH="$JAVA_HOME/bin:$PATH"
fi

JNA_JAR="${JNA_JAR:-$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar}"
SCALA_LIB="${SCALA_LIB:-/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala-library/3.8.3/scala-library-3.8.3.jar}"
SCALA3_LIB="${SCALA3_LIB:-/opt/homebrew/Cellar/scala/3.8.3/libexec/maven2/org/scala-lang/scala3-library_3/3.8.3/scala3-library_3-3.8.3.jar}"
JAVA_CLASSES="${JAVA_CLASSES:-../java/target/classes}"

if [ ! -d "$JAVA_CLASSES" ]; then
    echo "missing $JAVA_CLASSES — run 'mvn package' in ../java first" >&2
    exit 1
fi
for jar in "$JNA_JAR" "$SCALA_LIB" "$SCALA3_LIB"; do
    [ -f "$jar" ] || { echo "missing $jar" >&2; exit 1; }
done

echo "[scala] compiling HelloWorld.scala"
scalac -cp "$JAVA_CLASSES:$JNA_JAR" HelloWorld.scala -d HelloWorld.jar

echo "[scala] running (DYLD_LIBRARY_PATH=. -XstartOnFirstThread)"
exec java -XstartOnFirstThread -Djna.library.path=. \
    -cp "HelloWorld.jar:$JAVA_CLASSES:$JNA_JAR:$SCALA_LIB:$SCALA3_LIB" \
    com.azul.HelloWorld

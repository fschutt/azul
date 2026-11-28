#!/usr/bin/env bash
# The font-miss A/B on ONE machine (the runner pool mixes CPUs: two runs cannot be subtracted): libazul and an app built at
# the job's own commit (B) and at another commit (A), then blocks of starts of both, alternating: starts WITHOUT a font cache
# (a fresh cache directory for each start: a MISS) and starts WITH one (HIT: a warm-up start makes it).
#
#   scripts/remote.sh run <sha of B> ubuntu-22.04 'bash scripts/ci/startup_ab_linux.sh <sha of A> [package]' --codegen --artifacts 'out/ab-*.json
#   out/ab-table.txt'
#
# B must be A plus changes that leave api.json alone (one codegen serves both builds). The app defaults to AzDrive (Calmet UI +
# EB Garamond, all registered: the positive case of the first layout's font wait); AzWidgets names system families (the control).
# Rounds: 2, each: A miss x5, B miss x5, A hit (warm-up + 5), B hit (warm-up + 5): 10 miss and 10 hit starts per build.
set +e
A="${1:?the commit of A}"
app="${2:-AzDrive}"
B="$(git rev-parse HEAD)"
echo "=== runner: $(uname -a), cores $(nproc), $(grep -m1 'model name' /proc/cpuinfo)"
echo "=== A = $A, B = $B, app = $app"
sudo apt-get install -y -qq xvfb xauth libegl1 libegl-mesa0 libgl1 libgl1-mesa-dri libglx-mesa0 libgles2 libxkbcommon0 libxkbcommon-x11-0 \
    libx11-6 libx11-xcb1 libxext6 libxrender1 libxrandr2 libxi6 libxcursor1 > /dev/null 2>&1
mkdir -p out/A out/B "$RUNNER_TEMP/tools"
# the harness of B, kept: the checkout of A has its own
cp scripts/ci/startup_measure.py scripts/ci/startup_ab_table.py "$RUNNER_TEMP/tools/"
git fetch -q --depth=1 origin "$A" 2>&1 | tail -2

# build <commit> <A|B>: the library and the app at that commit (the second build only compiles what differs)
build() {
    git checkout -q "$1" || { echo "BUILD $2: cannot check out $1"; return 1; }
    echo "=== build $2 at $(git rev-parse --short HEAD)"
    cargo build --release -p azul-dll --features build-dll 2>&1 | tail -3
    mkdir -p "$RUNNER_TEMP/cabi-$2"
    cp target/release/libazul.so "$RUNNER_TEMP/cabi-$2/" && cp target/release/libazul.so "out/$2/"
    AZ_LINK_PATH="$RUNNER_TEMP/cabi-$2" cargo build --release -p "$app" 2>&1 | tail -3
    cp "target/release/$app" "out/$2/"
}
build "$B" B
build "$A" A
ls -la out/A out/B

Xvfb :99 -screen 0 1280x800x24 > /dev/null 2>&1 &
sleep 2
export DISPLAY=:99

# measure <A|B> <miss|hit> <n>: one harness call; a miss start has a cache directory of its own, a hit file starts a warm-up first
measure() {
    local which="$1" kind="$2" n="$3" runs="$4" cache
    cache="$(mktemp -d)"
    python3 "$RUNNER_TEMP/tools/startup_measure.py" --exe "$PWD/out/$which/$app" --runs "$runs" --label "$app $which $kind $n" \
        --env "LD_LIBRARY_PATH=$PWD/out/$which" --env "XDG_CACHE_HOME=$cache" --json "out/ab-$which-$kind-$n.json" > "out/ab-$which-$kind-$n.txt" 2>&1
    grep -E "^(first run|median of the others|layout passes)" "out/ab-$which-$kind-$n.txt" | tr '\n' ' '
    echo
}
for round in 1 2; do
    for which in A B; do
        for i in 1 2 3 4 5; do
            echo "--- $which miss (round $round, start $i)"
            measure "$which" miss "$round$i" 1
        done
    done
    for which in A B; do
        echo "--- $which hit (round $round): a warm-up start, then 5"
        measure "$which" hit "$round" 6
    done
done
echo "=== the table"
python3 "$RUNNER_TEMP/tools/startup_ab_table.py" out --json out/ab-summary.json | tee out/ab-table.txt
test -s out/ab-table.txt

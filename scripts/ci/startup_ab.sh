#!/usr/bin/env bash
# The font-miss A/B on ONE machine (the runner pool mixes CPUs: two runs cannot be subtracted): libazul and an app built at
# the job's own commit (B) and at another commit (A), then blocks of starts of both, alternating: starts WITHOUT a font cache
# (a fresh cache directory for each start: a MISS) and starts WITH one (HIT: a warm-up start makes it).
#
#   scripts/remote.sh run <sha of B> ubuntu-22.04 'bash scripts/ci/startup_ab.sh <sha of A> [apps]' --codegen --artifacts 'out/ab-*.json
#   out/ab-table.txt'
#   scripts/remote.sh run <sha of B> macos-15     'bash scripts/ci/startup_ab.sh <sha of A> [apps]' --codegen --artifacts 'out/ab-*.json
#   out/ab-table.txt'
#
# B must be A plus changes that leave api.json alone (one codegen serves both builds). `apps` is a comma list (default AzDrive:
# Calmet UI + EB Garamond, all registered: the positive case of the first layout's font wait; AzWidgets names system families:
# the control). AB_FONT_PACKAGES (Linux, apt packages) makes the machine a desktop with many fonts before anything is built:
# a runner has a few hundred font files and its scan is over before the first layout begins, a desktop with `fonts-noto` has
# thousands and the scan outlasts it.
# Rounds: 2, each: A miss x5, B miss x5, A hit (cache filled by a start that stays up, then 6 starts, the first dropped), B hit
# (the same): 10 miss and 10 hit starts per build.
# A miss start has a cache directory of its own: XDG_CACHE_HOME on Linux, HOME on macOS (the library's `dirs::cache_dir()`).
set +e
A="${1:?the commit of A}"
apps="$(echo "${2:-AzDrive}" | tr ',' ' ')"
B="$(git rev-parse HEAD)"
case "$(uname -s)" in
    Linux)
        lib=libazul.so
        libpath=LD_LIBRARY_PATH
        cache_var=XDG_CACHE_HOME
        echo "=== runner: $(uname -a), cores $(nproc), $(grep -m1 'model name' /proc/cpuinfo)"
        sudo apt-get install -y -qq xvfb xauth libegl1 libegl-mesa0 libgl1 libgl1-mesa-dri libglx-mesa0 libgles2 libxkbcommon0 libxkbcommon-x11-0 \
            libx11-6 libx11-xcb1 libxext6 libxrender1 libxrandr2 libxi6 libxcursor1 > /dev/null 2>&1
        if [ -n "${AB_FONT_PACKAGES:-}" ]; then
            # shellcheck disable=SC2086
            sudo apt-get install -y -qq $AB_FONT_PACKAGES > /dev/null 2>&1
            echo "=== font files on this machine: $(find /usr/share/fonts /usr/local/share/fonts -type f 2>/dev/null | wc -l) under /usr/share/fonts"
        fi
        ;;
    Darwin)
        lib=libazul.dylib
        libpath=DYLD_LIBRARY_PATH
        cache_var=HOME
        echo "=== runner: $(uname -a), $(sw_vers -productVersion), cores $(sysctl -n hw.ncpu), $(sysctl -n machdep.cpu.brand_string), vmm $(sysctl -n kern.hv_vmm_present 2>&1)"
        ;;
    *) echo "this script runs on Linux and macOS"; exit 2 ;;
esac
echo "=== A = $A, B = $B, apps = $apps, extra font packages = ${AB_FONT_PACKAGES:-none}"
mkdir -p out/A out/B "$RUNNER_TEMP/tools"
# the harness of B, kept: the checkout of A has its own
cp scripts/ci/startup_measure.py scripts/ci/startup_ab_table.py "$RUNNER_TEMP/tools/"
git fetch -q --depth=1 origin "$A" 2>&1 | tail -2

# build <commit> <A|B>: the library and the app at that commit (the second build only compiles what differs)
build() {
    git checkout -q "$1" || { echo "BUILD $2: cannot check out $1"; return 1; }
    echo "=== build $2 at $(git rev-parse --short HEAD)"
    # A build that fails stops the job: the library of the OTHER build would be copied for it (a first version of this
    # script measured one build twice, A's library failing to compile against the bindings generated at B)
    cargo build --release -p azul-dll --features build-dll > "out/build-$2-lib.log" 2>&1 \
        || { tail -40 "out/build-$2-lib.log"; echo "BUILD $2: the library did not build (A and B must have the same api.json)"; exit 3; }
    tail -2 "out/build-$2-lib.log"
    mkdir -p "$RUNNER_TEMP/cabi-$2"
    cp "target/release/$lib" "$RUNNER_TEMP/cabi-$2/" && cp "target/release/$lib" "out/$2/"
    for app in $apps; do
        AZ_LINK_PATH="$RUNNER_TEMP/cabi-$2" cargo build --release -p "$app" > "out/build-$2-$app.log" 2>&1 \
            || { tail -40 "out/build-$2-$app.log"; echo "BUILD $2: $app did not build"; exit 3; }
        tail -1 "out/build-$2-$app.log"
        cp "target/release/$app" "out/$2/"
    done
    cksum "out/$2/$lib" | sed "s/^/    library of $2 (checksum, size): /"
}
build "$B" B
build "$A" A
ls -la out/A out/B

if [ "$(uname -s)" = Linux ]; then
    Xvfb :99 -screen 0 1280x800x24 > /dev/null 2>&1 &
    sleep 2
    export DISPLAY=:99
fi

# fill_cache <A|B> <app> <cache dir>: one start that is NOT ended after its first frame and stays until the manifest and its key file
# are written (at most 40 s): a start that skips the wait for the scan has its first frame long before the scan is done, and
# the harness's own exit after the first frame would cut the scan and leave no cache for the starts that are to be hits
fill_cache() {
    local which="$1" app="$2" cache="$3" pid
    env "$libpath=$PWD/out/$which" "$cache_var=$cache" "$PWD/out/$which/$app" > /dev/null 2>&1 &
    pid=$!
    for _ in $(seq 1 200); do
        [ -n "$(find "$cache" -name manifest.key 2>/dev/null | head -1)" ] && break
        sleep 0.2
    done
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    echo "    cache filled: $(find "$cache" -name 'manifest.*' 2>/dev/null | wc -l | tr -d ' ') files"
}

# measure <A|B> <app> <miss|hit> <n> <runs>: one harness call; a miss start has a cache directory of its own, a hit file's cache is
# filled first (its first start of the harness is dropped by the table, as before: the harness's own first start is a hit too)
measure() {
    local which="$1" app="$2" kind="$3" n="$4" runs="$5" cache
    cache="$(mktemp -d)"
    [ "$kind" = hit ] && fill_cache "$which" "$app" "$cache"
    python3 "$RUNNER_TEMP/tools/startup_measure.py" --exe "$PWD/out/$which/$app" --runs "$runs" --label "$app $which $kind $n" \
        --env "$libpath=$PWD/out/$which" --env "$cache_var=$cache" --json "out/ab-$app-$which-$kind-$n.json" > "out/ab-$app-$which-$kind-$n.txt" 2>&1
    grep -E "^(first run|median of the others|layout passes)" "out/ab-$app-$which-$kind-$n.txt" | tr '\n' ' '
    echo
}
for app in $apps; do
    for round in 1 2; do
        for which in A B; do
            for i in 1 2 3 4 5; do
                echo "--- $app $which miss (round $round, start $i)"
                measure "$which" "$app" miss "$round$i" 1
            done
        done
        for which in A B; do
            echo "--- $app $which hit (round $round): the cache is filled by a start that stays up, then 6 starts (the first is dropped)"
            measure "$which" "$app" hit "$round" 6
        done
    done
done
echo "=== the table"
python3 "$RUNNER_TEMP/tools/startup_ab_table.py" out --json out/ab-summary.json | tee out/ab-table.txt
test -s out/ab-table.txt

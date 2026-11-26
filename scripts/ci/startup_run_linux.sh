#!/usr/bin/env bash
# The startup timeline on an Ubuntu runner: a release libazul (build-dll) + AzWidgets, started under Xvfb with
# the CPU renderer and with the GL one (llvmpipe: not a GPU), with and without a GNOME-like session (a D-Bus
# session and XDG_CURRENT_DESKTOP=GNOME, where every gsettings query is a real process), an strace of one start
# (every process it spawned, every directory it read), a perf profile of one start, the tests of the timeline.
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_run_linux.sh' --codegen --artifacts 'out/*.json
#   out/strace*.txt
#   out/strace*.log'
set +e
echo "=== runner: $(uname -a)"; . /etc/os-release; echo "$PRETTY_NAME"; grep -m1 'model name' /proc/cpuinfo; echo "cores: $(nproc), mem: $(free -m | awk '/Mem:/ {print $2}') MB"; uptime
sudo apt-get install -y -qq xvfb xauth strace linux-tools-common linux-tools-generic "linux-tools-$(uname -r)" gsettings-desktop-schemas dconf-gsettings-backend dbus-x11 libglib2.0-bin >/dev/null 2>&1
# What a window needs on a desktop and a bare runner image lacks: EGL (the X11 shell requires it even for the CPU renderer), Mesa's
# software GL (llvmpipe) for the GL series, xkbcommon, the X extension libraries
sudo apt-get install -y -qq libegl1 libegl-mesa0 libgl1 libgl1-mesa-dri libglx-mesa0 libgles2 libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxext6 libxrender1 libxrandr2 libxi6 libxcursor1 >/dev/null 2>&1
echo "gtk3 installed: $(ldconfig -p | grep -c 'libgtk-3.so.0'), libEGL: $(ldconfig -p | grep -c 'libEGL.so.1'), gsettings: $(command -v gsettings)"
echo "=== release libazul (build-dll)"
cargo build --release -p azul-dll --features build-dll 2>&1 | tail -30
mkdir -p "$RUNNER_TEMP/azul-cabi" out
cp target/release/libazul.so "$RUNNER_TEMP/azul-cabi/" && cp target/release/libazul.so out/
echo "=== AzWidgets (release, linked dynamically)"
AZ_LINK_PATH="$RUNNER_TEMP/azul-cabi" cargo build --release -p AzWidgets 2>&1 | tail -30
cp target/release/AzWidgets out/
ls -la out
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 &
sleep 2
export DISPLAY=:99
export LD_LIBRARY_PATH="$PWD/out"
echo "=== what loading the libraries of a window costs in a bare process, in the order the X11 shell takes them (the first run: libraries not yet in the page cache; the next two: warm); libgtk-3 and the call of its IM context were loaded by every window at creation"
cc -O1 -o out/dlopen_cost scripts/ci/dlopen_cost.c -ldl
for n in 1 2 3; do echo "--- run $n"; ./out/dlopen_cost libX11.so.6 libxkbcommon.so.0 libEGL.so.1 libgtk-3.so.0 -- gtk_im_context_simple_new; done
echo "=== 9 starts under Xvfb, CPU renderer, no desktop session; the first start has no font cache (cold), the others have it (warm)"
export XDG_CACHE_HOME="$(mktemp -d)"
python3 scripts/ci/startup_measure.py --exe "$PWD/out/AzWidgets" --runs 9 --label "AzWidgets linux release, Xvfb, CPU renderer, no desktop session" --json out/startup-linux-plain.json
echo "=== 9 starts as on a GNOME desktop (a D-Bus session, XDG_CURRENT_DESKTOP=GNOME: every gsettings query is a real process)"
dbus-run-session -- python3 scripts/ci/startup_measure.py --exe "$PWD/out/AzWidgets" --runs 9 --label "AzWidgets linux release, Xvfb, CPU renderer, GNOME-like session" --env XDG_CURRENT_DESKTOP=GNOME --env DESKTOP_SESSION=gnome --json out/startup-linux-gnome.json
echo "=== 5 starts with the GL renderer (llvmpipe: not a GPU)"
python3 scripts/ci/startup_measure.py --exe "$PWD/out/AzWidgets" --runs 5 --label "AzWidgets linux release, Xvfb, AZ_BACKEND=gpu (llvmpipe)" --env AZ_BACKEND=gpu --json out/startup-linux-gl.json
echo "=== the phases of each layout pass: one start with AZ_PROFILE=cpu (the profiler adds a little)"
AZ_STARTUP=1 AZ_PROFILE=cpu AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 ./out/AzWidgets 2>&1 | grep -E "^\[CPU\]|^\[startup\]" | head -220
echo "=== strace -f of one start (GNOME-like session): the processes it spawned and how long each lived"
export XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION=gnome AZ_STARTUP=1 AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1
dbus-run-session -- strace -f -tt -T -e trace=execve,exit_group,openat,connect -o out/strace-gnome.log ./out/AzWidgets 2> out/strace-gnome-stderr.txt
unset XDG_CURRENT_DESKTOP DESKTOP_SESSION AZ_STARTUP AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER
python3 scripts/ci/strace_summary.py out/strace-gnome.log
echo "=== the timeline table of that run"
grep "^\[startup\]" out/strace-gnome-stderr.txt | head -150
echo "=== perf record -F 4000 -g of one start (no desktop session)"
sudo sysctl -w kernel.perf_event_paranoid=-1 kernel.kptr_restrict=0 >/dev/null 2>&1
AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 perf record -F 4000 -g -o out/perf.data ./out/AzWidgets 2>&1 | tail -3
echo "--- by dso and symbol (self time), top 70"
perf report -i out/perf.data --no-children --sort dso,symbol --stdio -g none 2>/dev/null | grep -v '^#' | grep -v '^$' | head -70
echo "--- by thread, top 15"
perf report -i out/perf.data --no-children --sort comm,tid --stdio -g none 2>/dev/null | grep -v '^#' | grep -v '^$' | head -15
echo "--- inclusive (children), top 60 symbols"
perf report -i out/perf.data --children --sort symbol --stdio -g none 2>/dev/null | grep -v '^#' | grep -v '^$' | head -60
echo "=== the tests of the timeline, the settings and the app's shared state"
cargo test --release -p azul-dll --lib --features build-dll -- startup_timeline shell_settings app_shared first_frame gtk_im style_discovery system_style env:: font_instance_store x11_libs 2>&1 | grep -E "^test |^test result|FAILED|panicked|^error|could not compile" | head -120
echo "=== the test of the noise's floor, the transient windows' docking tests and the variable-font tests of azul-layout (the instances of a document's stacks are made together)"
cargo test --release -p azul-layout --lib -- the_noise_floor transient 2>&1 | grep -E "^test |^test result|FAILED|panicked|^error|could not compile" | head -60
cargo test --release -p azul-layout --test all -- variable_font the_instances_of_all_stacks 2>&1 | grep -E "^test |^test result|FAILED|panicked|^error|could not compile" | head -40

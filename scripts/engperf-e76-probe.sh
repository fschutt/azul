#!/usr/bin/env bash
# E76 probe: who asks for the layout that runs after the first picture? AzWidgets (debug build) under Xvfb, X11 shell, CPU renderer: the
# startup timeline of starts nobody touches. The timeline names the producer of every pass (`layout_begin #N reason=... by=file:line`).
set +e
echo "=== runner: $(uname -a)"
sudo apt-get install -y -qq xvfb xauth libegl1 libegl-mesa0 libgl1 libgl1-mesa-dri libglx-mesa0 libgles2 libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxext6 libxrender1 libxrandr2 libxi6 libxcursor1 fonts-dejavu-core fonts-liberation libv4l-dev >/dev/null 2>&1
mkdir -p out
echo "=== libazul (debug, build-dll)"
cargo build -p azul-dll --features build-dll 2>&1 | tail -4
mkdir -p "$RUNNER_TEMP/azul-cabi"
cp target/debug/libazul.so "$RUNNER_TEMP/azul-cabi/" || { echo "NO libazul.so"; ls target/debug | head -30; }
echo "=== AzWidgets (debug, linked dynamically)"
AZ_LINK_PATH="$RUNNER_TEMP/azul-cabi" cargo build -p AzWidgets 2>&1 | tail -4
ls -la target/debug/AzWidgets
Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 &
sleep 2
export DISPLAY=:99
export LD_LIBRARY_PATH="$RUNNER_TEMP/azul-cabi"
export XDG_CACHE_HOME="$(mktemp -d)"
for run in 1 2 3; do
  echo "=== X11 start $run (AZ_STARTUP=1, 30 s, nobody touches it)"
  AZ_STARTUP=1 timeout 30 target/debug/AzWidgets > "out/x11-$run.txt" 2>&1
  grep -aE "^\[startup\] (late|first_present at|layout passes)|layout_begin|layout_end|layout_extra_pass|first_event" "out/x11-$run.txt" | cut -c1-300 | head -40
done
echo "=== X11 start 4, GNOME-like session variables (the focus path of a real desktop session)"
XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION=gnome AZ_STARTUP=1 timeout 30 target/debug/AzWidgets > out/x11-gnome.txt 2>&1
grep -aE "^\[startup\] (late|first_present at|layout passes)|layout_begin|layout_end|layout_extra_pass|first_event" out/x11-gnome.txt | cut -c1-300 | head -40
echo "=== the lines around the first layout after the first present (any log level the engine prints)"
awk '/first_present at/{p=1} p{print} /late: layout_end/{n++} n>=1{exit}' out/x11-1.txt | cut -c1-260 | head -60
echo "=== headless start (AZ_BACKEND=headless)"
AZ_BACKEND=headless AZ_STARTUP=1 timeout 30 target/debug/AzWidgets > out/headless-1.txt 2>&1
grep -aE "^\[startup\] (late|first_present at|layout passes)|layout_begin|layout_end|layout_extra_pass|first_event" out/headless-1.txt | cut -c1-300 | head -40
echo "=== done"

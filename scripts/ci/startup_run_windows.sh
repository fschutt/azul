#!/usr/bin/env bash
# The startup timeline on a Windows runner: a release azul.dll (build-dll) + AzWidgets, the binaries as artifacts
# (out/), 9 starts with the timeline (the first is the cold one), the same under PowerShell's Measure-Command, and the
# tests of the timeline. Run through scripts/remote.sh with --codegen:
#   scripts/remote.sh run <sha> windows-2022 'bash scripts/ci/startup_run_windows.sh' --codegen --artifacts 'out/*'
set +e
echo "=== runner: $(uname -a)"
powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).Caption; (Get-CimInstance Win32_Processor).Name; [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1MB)"
echo "=== release azul.dll (build-dll)"
cargo build --release -p azul-dll --features build-dll 2>&1 | tail -40
mkdir -p "$RUNNER_TEMP/azul-cabi" out
cp target/release/azul.dll target/release/azul.dll.lib "$RUNNER_TEMP/azul-cabi/"
cp target/release/azul.dll out/
echo "=== AzWidgets (release, linked dynamically)"
AZ_LINK_PATH="$RUNNER_TEMP/azul-cabi" cargo build --release -p AzWidgets 2>&1 | tail -40
cp target/release/AzWidgets.exe out/
cp scripts/ci/startup_measure.py out/
ls -la out
HERE="$(pwd -W)"
echo "=== AzWidgets: 9 starts with the timeline, exit at the first frame"
python scripts/ci/startup_measure.py --exe "$HERE/out/AzWidgets.exe" --runs 9 --label "AzWidgets windows release (runner VM)" --json out/startup-windows.json
echo "=== the phases of each layout pass: one start with AZ_PROFILE=cpu (the profiler adds a little)"
AZ_STARTUP=1 AZ_PROFILE=cpu AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 ./out/AzWidgets.exe 2>&1 | grep -E "^\[CPU\]|^\[startup\]" | head -220
echo "=== the same under Measure-Command (what a user's stopwatch sees: spawn to exit)"
export AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1
powershell -NoProfile -Command "1..5 | ForEach-Object { (Measure-Command { & '$HERE/out/AzWidgets.exe' | Out-Null }).TotalMilliseconds }"
echo "=== what the style discovery cost BEFORE it read the registry itself: the five reg queries and the PowerShell call it ran, one after the other, 5 rounds (the app now reads the same values with RegGetValueW)"
powershell -NoProfile -Command '
foreach ($i in 1..5) {
  $r = Measure-Command {
    reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" /v AppsUseLightTheme | Out-Null
    reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Accent" | Out-Null
    reg query "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion" /v CurrentBuildNumber | Out-Null
    reg query "HKCU\Control Panel\Desktop\WindowMetrics" /v MinAnimate | Out-Null
    reg query "HKCU\Control Panel\Accessibility\HighContrast" /v Flags | Out-Null
  }
  $p = Measure-Command { powershell -NoProfile -Command "(Get-Culture).Name" | Out-Null }
  "round $i : 5 x reg query {0:N1} ms, powershell Get-Culture {1:N1} ms, together {2:N1} ms" -f $r.TotalMilliseconds, $p.TotalMilliseconds, ($r.TotalMilliseconds + $p.TotalMilliseconds)
}'
echo "=== the tests of the timeline, the settings and the app's shared state"
cargo test --release -p azul-dll --lib --features build-dll -- startup_timeline shell_settings app_shared first_frame style_discovery the_windows_style 2>&1 | grep -E "^test |^test result|FAILED|panicked|^error|could not compile" | head -80

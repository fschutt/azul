#!/usr/bin/env bash
# Does what the startup work changed COMPILE on the targets that no runner of this work builds natively: Windows (the
# Win32 shell and the registry reads that replaced the reg.exe / PowerShell processes), Android (the bionic property
# reader, the app's font cache path, the shell's marks) and iOS (the shell's call of the layout pass)? `cargo check`
# links nothing, so one Ubuntu runner does all three, each in a target directory of its own so that they run side by
# side (cargo locks a target directory).
#   scripts/remote.sh run <sha> ubuntu-22.04 'bash scripts/ci/startup_check_targets.sh' --codegen
set +e
# Which checks run: the names given (windows, ios, android), all three by default.
wanted="${*:-windows ios android}"
mkdir -p out
rustup target add x86_64-pc-windows-gnu aarch64-apple-ios >/dev/null 2>&1
# ring's build script compiles C for the Windows target: the MinGW cross compiler (the runner image does not ship it)
sudo apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null 2>&1
echo "=== runner: $(uname -a), cores $(nproc), checks: $wanted, mingw: $(command -v x86_64-w64-mingw32-gcc)"

check() {
    name="$1"
    shift
    case " $wanted " in *" $name "*) ;; *) return;; esac
    ( "$@" > "out/check-$name.log" 2>&1; echo "$?" > "out/check-$name.rc" ) &
}

# The Win32 shell with the features of the shipped azul.dll (build-dll): the registry reads, the first-present marks
check windows env CARGO_TARGET_DIR="$PWD/target-windows" \
    cargo check -p azul-dll --lib --target x86_64-pc-windows-gnu --features build-dll --message-format short
# The iOS shell, with the feature set scripts/mobile-check-all.sh checks it with
check ios env CARGO_TARGET_DIR="$PWD/target-ios" \
    cargo check -p azul-dll --lib --release --target aarch64-apple-ios --no-default-features \
    --features std,logging,link-static,a11y --message-format short
# The Android shell, with the features an APK's libazul is built with (scripts/ci/android_check.sh)
check android env CARGO_TARGET_DIR="$PWD/target-android" bash scripts/ci/android_check.sh aarch64-linux-android
wait

status=0
for name in $wanted; do
    rc="$(cat "out/check-$name.rc" 2>/dev/null || echo missing)"
    if [ "$rc" = "0" ]; then
        echo "CHECK $name: ok"
    else
        status=1
        echo "CHECK $name: FAILED (exit $rc)"
        grep -E "^error|error\[E|warning: unused|^  -->|-->" "out/check-$name.log" | head -60
        echo "--- end of the log"
        tail -25 "out/check-$name.log"
    fi
done
exit "$status"

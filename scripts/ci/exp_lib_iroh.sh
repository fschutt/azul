#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): does the mobile libazul build with the iroh feature (the S0 spike of the
# phone companion)?   bash scripts/ci/exp_lib_iroh.sh <android|ios> <rust-target>
set -euxo pipefail
kind="${1:?}"; target="${2:?}"
rustup target add "$target"
if [ "$kind" = android ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
  yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --licenses >/dev/null 2>&1 || true
  "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --install "ndk;27.0.12077973" >/dev/null
  export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
  features="std,logging,link-static,a11y,android-activity,map-tiles,pdf,iroh"
else
  features="std,logging,link-static,a11y,map-tiles,pdf,iroh"
fi
mkdir -p demo-out
bash scripts/demo_lib.sh "$kind" "$target" "$features" "demo-out/lib-$target"

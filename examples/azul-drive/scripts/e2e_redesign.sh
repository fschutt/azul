#!/bin/sh
# AzDrive on the sidebar family: the scenarios of the redesign (examples/azul-drive/e2e/redesign.json and
# redesign_phone.json) in a headless AzDrive, one process per file, over a Home of its own that holds the
# sample files (`--sample`). The phone's file runs with `--form phone`: a desktop window shows the phone's
# layout (the sidebar a drawer, the preview a pane at the foot).
#
#   examples/azul-drive/scripts/e2e_redesign.sh [AzDrive binary] [file ...]
#
# The binary is the desktop QA build (`bash scripts/ci/desktop_e2e_build.sh linux AzDrive`: the shipped
# libazul plus the debug server, the e2e executor and the test hooks; the app is demo-out/apps/AzDrive-e2e-linux)
# or any AzDrive linked against such a libazul. Every take_screenshot of a scenario lands numbered in
# capture order in AZ_E2E_SHOT_DIR/<file stem> (default demo-out/azdrive-shots). The run's output of a file
# goes to AZDRIVE_E2E_LOGS (default demo-out/azdrive-e2e). Exit status: 0 when no test failed.
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
bin=${1:-${AZDRIVE_BIN:-demo-out/apps/AzDrive-e2e-linux}}
[ $# -gt 0 ] && shift
if [ $# -gt 0 ]; then
    files=$*
else
    files="redesign.json redesign_phone.json"
fi
shots=${AZ_E2E_SHOT_DIR:-$PWD/demo-out/azdrive-shots}
logs=${AZDRIVE_E2E_LOGS:-$PWD/demo-out/azdrive-e2e}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/azdrive-e2e-redesign.XXXXXX")
mkdir -p "$shots" "$logs"
bad=0

for file in $files; do
    stem=${file%.json}
    dir=$tmp/$stem
    mkdir -p "$dir/data" "$dir/home" "$dir/downloads" "$dir/cache" "$dir/systemp" "$shots/$stem"
    form=desktop
    case $stem in
        *phone*) form=phone ;;
    esac
    log=$logs/$stem.log
    status=0
    AZ_BACKEND=headless \
    AZ_E2E="$here/e2e/$file" \
    AZ_E2E_SHOT_DIR="$shots/$stem" \
    AZLIN_DATA="$dir/data" \
    AZLIN_CONFIG="$dir/no-azlin-config.json" \
    AZUL_DRIVES="$dir/drives.json" \
    AZDRIVE_HOME="$dir/home" \
    AZDRIVE_DOWNLOADS="$dir/downloads" \
    AZDRIVE_CACHE="$dir/cache" \
    AZDRIVE_DIALOGS=inline \
    AZDRIVE_SYNC_POLL=3600 \
    AZDRIVE_SYNC_SETTLE_MS=0 \
    TMPDIR="$dir/systemp" \
    AZ_KEYRING_FILE="$dir/keyring.json" \
        "$bin" --screen home --sample --form "$form" > "$log" 2>&1 || status=$?
    esc=$(printf '\033')
    summary=$(sed "s/${esc}\[[0-9;]*m//g" "$log" | grep '^test result:' | tail -n 1 || true)
    echo "$file (exit $status): ${summary:-no report}"
    case $summary in
        *" 0 failed"*) ;;
        *) bad=1 ;;
    esac
    [ "$status" = 0 ] || bad=1
done

# what the shots are
find "$shots" -name '*.png' | sort | head -40
exit "$bad"

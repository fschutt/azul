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
    files="redesign.json redesign_phone.json redesign_diag.json redesign_diag_phone.json"
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
        *phone*)
            form=phone
            # a folder of 80 files besides the sample's: the list scrolls (the selection follows)
            n=1
            while [ "$n" -le 80 ]; do
                num=$(printf '%03d' "$n")
                printf 'file %s\n' "$num" > "$dir/home/file-$num.txt"
                n=$((n + 1))
            done
            ;;
        *)
            # a text longer than the preview reads (64 KB): its banner says only the start is shown
            yes 'a line of a long text, longer than the preview reads' | head -c 300000 > "$dir/home/big.txt"
            ;;
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
    # the captions of the pictures, numbered as the pictures are (capture order)
    sed "s/${esc}\[[0-9;]*m//g" "$log" | sed -n 's/^.*\[e2e\] SHOT \(.*\)$/\1/p' |
        awk '{ printf "%02d %s\n", NR, $0 }' > "$shots/$stem/captions.txt"
    echo "$file (exit $status): ${summary:-no report}"
    # the phone's list follows its scroll: the selection was the rows the top edge passed over the run, and the
    # last row the follow chose is the row at the top edge of the offset it was told (a phone's rows are 48 px
    # and the folder lists 6 folders first: file-NNN is the row NNN + 5, which is the offset over 48)
    case $stem in
        redesign_phone)
            plain=$(sed "s/${esc}\[[0-9;]*m//g" "$log")
            keys=$(printf '%s\n' "$plain" | grep -a '^AZDRIVE_SELECTED 1 ' | awk '{print $3}')
            moved=$(printf '%s\n' "$keys" | sort -u | grep -c . || true)
            echo "$file: the selection was $moved different rows"
            printf '%s\n' "$keys" | uniq | tr '\n' ' ' | sed 's/^/AZDRIVE_SELECTED sequence: /'; echo
            if [ "${moved:-0}" -lt 3 ]; then
                echo "$file: the scroll moved no selection"
                bad=1
            fi
            follow=$(printf '%s\n' "$plain" | grep -a '^AZDRIVE_FOLLOW ' | tail -n 1)
            key=$(printf '%s\n' "$follow" | awk '{print $2}')
            offset=$(printf '%s\n' "$follow" | awk '{print $3}')
            echo "$file: the follow's last report: ${key:-none} at offset ${offset:-none}"
            case $key in
                file-*.txt)
                    n=${key#file-}
                    n=${n%.txt}
                    n=${n#"${n%%[!0]*}"}
                    row=$(( ${offset:-0} / 48 ))
                    if [ $((${n:-0} + 5)) -ne "$row" ] || [ "${offset:-0}" -lt 1400 ]; then
                        echo "$file: the selection is not the row at the top edge (row $row at offset ${offset:-0}, the wheel scrolled 1500 px)"
                        bad=1
                    fi
                    ;;
                *)
                    echo "$file: the follow did not end on a file of the folder"
                    bad=1
                    ;;
            esac
            ;;
    esac
    case $summary in
        *" 0 failed"*) ;;
        *) bad=1 ;;
    esac
    [ "$status" = 0 ] || bad=1
done

# what the shots are
find "$shots" -name '*.png' | sort | head -40
exit "$bad"

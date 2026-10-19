#!/bin/zsh
# AP-01: the wave-23 builders' proof steps in the real apps, for the final test round (after
# the apps are built against libazul with the debug server). One app at a time through the
# capped runner; every step is headless except the brief real-window shots (seconds each, the
# display never woken). A line per step in $OUT/summary.txt: name, exit code, its last line.
#
#   scripts/ap01_proofs/run_final_round.sh [BIN_DIR] [OUT_DIR]
#
# BIN_DIR holds the app binaries (default target/release), libazul from DYLD_LIBRARY_PATH
# (default target/azul-lib). Expected: every step exits 0, except azpdf_resize until the
# engine's drag-step cost is under the 150 ms settle time (see its own docstring).
cd "$(dirname "$0")/../.."
B=${1:-target/release}
OUT=${2:-${AZ_WORK:-$HOME/Development/azul-work}/logs/ap01}
mkdir -p $OUT
export DYLD_LIBRARY_PATH=${DYLD_LIBRARY_PATH:-$PWD/target/azul-lib}
R=scripts/waves/tools/run_capped.sh
: > $OUT/summary.txt
step() {
    local name=$1 secs=$2; shift 2
    caffeinate -ims $R --cap-mb 2500 --seconds $secs --log $OUT/$name.log -- "$@" > $OUT/$name.out 2>&1
    echo "$name EXIT $? $(tail -1 $OUT/$name.out | cut -c1-160)" >> $OUT/summary.txt
}
scenario() {
    # An engine scenario file run inside a real app binary: AZ_E2E names it, AZ_BACKEND the
    # backend (headless, or a brief cpu / gpu window).
    local name=$1 backend=$2 file=$3 bin=$4
    caffeinate -ims env AZ_BACKEND=$backend AZ_E2E=$file AZ_E2E_SHOT_DIR=$OUT/$name-shots $bin \
        > $OUT/$name.out 2>&1
    echo "$name EXIT $? $(grep -E '^test result' $OUT/$name.out | tail -1)" >> $OUT/summary.txt
}

# RUNTIME23 1 (AzWriter caret on open and after Enter), APPS23 AzSetup German, AzMail Cancel.
step azwriter 300 python3 scripts/azwriter_e2e.py --bin $B/AzWriter --out $OUT/azwriter-shots
step azsetup 300 python3 scripts/azsetup_e2e.py --bin $B/AzSetup --out $OUT/azsetup-shots
step azmail_cancel 300 python3 scripts/azmail_e2e.py --bin $B/AzMail --phase cancel
# RUNTIME23 2 (the tab overview takes the keyboard) is azdrive_e2e's step 23b; WIDGETS23's
# address bar hint, toasts and F2 on an icon tile are the widgets proof (headless).
step azdrive 500 python3 scripts/azdrive_e2e.py --bin $B/AzDrive --out $OUT/azdrive-shots
step azdrive_widgets 300 python3 scripts/ap01_proofs/azdrive_widgets_proof.py --bin $B/AzDrive --out $OUT/azdrive-widgets
# APPS23 Tab+Enter in AzClock, AzContacts, AzKeys, AzPdf, AzPhoto, AzVideoCut.
step tab_enter 600 python3 scripts/ap01_proofs/tab_enter_e2e.py $B $OUT/tab-enter
# APPS23 AzTasks close, AzVideoCut saves and meters.
step aztasks 300 python3 scripts/aztasks_e2e.py --bin $B/AzTasks --out $OUT/aztasks-shots
step azvideocut 300 python3 scripts/azvideocut_e2e.py --bin $B/AzVideoCut --out $OUT/azvideocut-shots
# APPS24: AP-11's re-measure of AzNews' first layout (the sample's 891 articles, the node
# count of the window; the runner's log has the peak memory) and its magazine; AP-06g's
# compose window outliving the main window is azmail_e2e's account run.
step aznews 600 python3 scripts/aznews_e2e.py --bin $B/AzNews --out $OUT/aznews-shots
step azmail_account 600 python3 scripts/azmail_e2e.py --bin $B/AzMail --phase account --timeout 400
# AP-08: AzCalendar's attendee tokens, the Azlin-root data folder, and the backstage's text in
# light mode (said to be faint: the contrast stage measures it and leaves the screenshots).
step azcalendar 600 python3 scripts/azcalendar_e2e.py --bin $B/AzCalendar --out $OUT/azcalendar-shots
# AP-04: "AzMeet high CPU?" was never measured: a two-person call, quiet and with video.
step azmeet_cpu 600 python3 scripts/azmeet_cpu.py --bin $B/AzMeet --seconds 15
# RUNTIME23 6 (AzPdf resize).
step azpdf_resize 300 python3 scripts/azpdf_resize_e2e.py --bin $B/AzPdf
# AP-09: AF4's re-check (page 3 and a wheel turn) is in AzPdf's main E2E.
step azpdf 400 python3 scripts/azpdf_e2e.py --bin $B/AzPdf --out $OUT/azpdf-shots
# MEETMAPS23: a simulated drive rebuilds nothing between position updates.
step azmaps_simulate 400 python3 scripts/azmaps_nav_e2e.py --bin $B/AzMaps --only simulate
# CSS23 and LAYOUT23 scenarios in a real app binary: headless, then brief cpu and gpu windows
# (LAYOUT23 2: the background shots are the same on both).
scenario css23 headless tests/e2e/css23_proof.json $B/AzWidgets
scenario layout23 headless tests/e2e/layout23_proof.json $B/AzWidgets
scenario layout23_cpu cpu tests/e2e/layout23_proof.json $B/AzWidgets
scenario layout23_gpu gpu tests/e2e/layout23_proof.json $B/AzWidgets
# LAYOUT23 6: a parent grows with its restyled child.
step layoutperf8 300 target/release/azul-doc e2e scripts/layoutperf8_e2e
# RUNTIME23 8: AzMail's Options window is owned by the main window (a brief real window).
step owned_window 120 python3 scripts/ap01_proofs/owned_window_proof.py $B/AzMail 8833 $OUT/owned
# LAYOUT23 1 (faux bold, CPU and GPU) and MATERIALS22 (the point light, the metal): brief shots.
for backend in cpu gpu; do
    # AzDrive on a home, data and drives of its own (never the user's).
    D=$OUT/azdrive-$backend; mkdir -p $D/home
    step faux_bold_$backend 60 python3 scripts/ap01_proofs/brief_shot.py --bin $B/AzDrive \
        --backend $backend --out $OUT/faux-bold-$backend.png -- --sample --language en \
        --home $D/home --data-dir $D/data --drives $D/drives.json --cache-dir $D/cache \
        --downloads $D/downloads --dialogs inline
    step materials_$backend 60 python3 scripts/ap01_proofs/brief_shot.py --bin $B/AzWidgets \
        --backend $backend --theme flora --scroll Materials --pointer 400,300 \
        --out $OUT/materials-$backend.png
done
echo DONE >> $OUT/summary.txt
cat $OUT/summary.txt

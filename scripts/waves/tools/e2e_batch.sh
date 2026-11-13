#!/bin/zsh
# The parent's E2E batch: every app's end-to-end script against the release binaries, one at a
# time through the capped runner (the 8 GB Mac cannot hold two apps and a build). Build libazul
# and the apps first (suites.sh does). Logs, screenshots and summary.txt in $AZ_WORK/logs/e2e.
# Run one script alone: e2e_batch.sh <name> [<name> ...] (the names below).
cd /Users/fschutt/Development/azul
L=${AZ_WORK:-$HOME/Development/azul-work}/logs/e2e
mkdir -p $L
R=scripts/waves/tools/run_capped.sh
B=target/release
WANT=("$@")
: > $L/summary.txt
e() {
    local name=$1 secs=$2; shift 2
    if (( ${#WANT} )) && (( ! ${WANT[(Ie)$name]} )); then return; fi
    $R --cap-mb 2000 --seconds $secs --log $L/$name.log -- "$@" > $L/$name.out 2>&1
    echo "$name EXIT $? $(tail -1 $L/$name.out | cut -c1-160)" >> $L/summary.txt
}
e azdrive 400 python3 scripts/azdrive_e2e.py --bin $B/AzDrive --out $L/azdrive-shots
e azdrive_add 400 python3 scripts/azdrive_add_e2e.py --bin $B/AzDrive --out $L/azdrive-add-shots
e azdrive_add_recovery 400 python3 scripts/azdrive_add_e2e.py --bin $B/AzDrive --recovery --out $L/azdrive-add-rec-shots
e s3_transfer 300 python3 scripts/s3_transfer_e2e.py --bin $B/examples/s3_transfer
e azbridge 300 python3 scripts/azbridge_e2e.py --bin $B/azul-bridge --azcloud examples/azcloud-api/target/release/azcloud
e azmail 400 python3 scripts/azmail_e2e.py --bin $B/AzMail
e azmaps 400 python3 scripts/azmaps_e2e.py --bin $B/AzMaps --out $L/azmaps-shots
e azreader 300 python3 scripts/azreader_e2e.py --bin $B/AzReader --out $L/azreader-shots
e azplayer 300 python3 scripts/azplayer_e2e.py --bin $B/AzPlayer --out $L/azplayer-shots
e azreview 240 python3 scripts/azreview_e2e.py --bin $B/AzReview --out $L/azreview-shots
e azwriter 300 python3 scripts/azwriter_e2e.py --bin $B/AzWriter --out $L/azwriter-shots
e azshow 300 python3 scripts/azshow_e2e.py --bin $B/AzShow --out $L/azshow-shots
e azsheets 300 python3 scripts/azsheets_e2e.py --bin $B/AzSheets --out $L/azsheets-shots
e azpdf 240 python3 scripts/azpdf_e2e.py --bin $B/AzPdf --out $L/azpdf-shots
e azpdf_resize 240 python3 scripts/azpdf_resize_e2e.py --bin $B/AzPdf
e installmgr 420 python3 scripts/installmgr_e2e.py --bin $B/AzInstallManager --writer-bin $B/AzWriter --out $L/installmgr-shots
# The other apps (each one window under the capped runner; AzMeet starts two apps under its own
# runner, so it runs outside this batch).
e azcalculator 240 python3 scripts/azcalculator_e2e.py --bin $B/AzCalculator --out $L/azcalculator-shots
e azcalendar 300 python3 scripts/azcalendar_e2e.py --bin $B/AzCalendar --out $L/azcalendar-shots
e azclock 300 python3 scripts/azclock_e2e.py --bin $B/AzClock --out $L/azclock-shots
e azcode 300 python3 scripts/azcode_e2e.py --bin $B/AzCode --out $L/azcode-shots
e azcontacts 300 python3 scripts/azcontacts_e2e.py --bin $B/AzContacts --out $L/azcontacts-shots
e azdashboard 240 python3 scripts/azdashboard_e2e.py --bin $B/AzDashboard --out $L/azdashboard-shots
e azerp 240 python3 scripts/azerp_e2e.py --bin $B/AzERP --out $L/azerp-shots
e azpasswordmanager 240 python3 scripts/azpasswordmanager_e2e.py --bin $B/AzPasswordManager --out $L/azpasswordmanager-shots
e azmonitor 240 python3 scripts/azmonitor_e2e.py --bin $B/AzMonitor --out $L/azmonitor-shots
e azmusic 300 python3 scripts/azmusic_e2e.py --bin $B/AzMusic --out $L/azmusic-shots
e aznews 300 python3 scripts/aznews_e2e.py --bin $B/AzNews --out $L/aznews-shots
e aznotes 300 python3 scripts/aznotes_e2e.py --bin $B/AzNotes --out $L/aznotes-shots
e azpaint 240 python3 scripts/azpaint_e2e.py --bin $B/AzPaint --out $L/azpaint-shots
e azphoto 300 python3 scripts/azphoto_e2e.py --bin $B/AzPhoto --out $L/azphoto-shots
e azsetup 240 python3 scripts/azsetup_e2e.py --bin $B/AzSetup --out $L/azsetup-shots
e aztasks 300 python3 scripts/aztasks_e2e.py --bin $B/AzTasks --out $L/aztasks-shots
e azterm 240 python3 scripts/azterm_e2e.py --bin $B/AzTerm --out $L/azterm-shots
e azvideocut 300 python3 scripts/azvideocut_e2e.py --bin $B/AzVideoCut --out $L/azvideocut-shots
e azdrive_pick 300 python3 scripts/azdrive_pick_e2e.py --bin $B/AzDrive
# AzDrive's sync under hostile conditions (headless): every scenario but the 100,000 small files and the 50 GB file,
# then the big file at 4 GB (killed mid-upload, resumed). Needs the azcloud CLI (examples/azcloud-api).
AZCLOUD=examples/azcloud-api/target/release/azcloud
CHAOS_LIGHT=special-files,names,server-errors,flap-upload,kill-every-step,offline-conflict,delete-vs-edit,rename-storm,changed-during,long-names,clock-skew,quota,revoked,sleep,disk-full,dns,slow-dns,iroh-lane,encrypted-bulk,rotation,ransomware
e azdrive_chaos 5400 python3 scripts/azdrive_chaos_e2e.py --bin $B/AzDrive --azcloud $AZCLOUD --only $CHAOS_LIGHT
e azdrive_chaos_big 2400 python3 scripts/azdrive_chaos_e2e.py --bin $B/AzDrive --azcloud $AZCLOUD --only fifty-gb --big-gb 4
echo DONE >> $L/summary.txt

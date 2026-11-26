#!/usr/bin/env bash
# Engine ledger E5, probe v2: the steps of the build_rust9x job (rust.yml) on a windows-2022 runner, with a memory
# logger and, once the build has failed, tests of what fails. Not part of the CI. Run through scripts/remote.sh:
#   scripts/remote.sh run <ref> windows-2022 'bash scripts/e5_probe.sh <variant> [<seconds>]' --codegen --artifacts 'e5/*'
# variants: diag (the job's build, then tests of the failure, then the same build again, then with one job),
#           lean (debug = 0, codegen-units = 256 like [profile.pyext-e2e]), jobs1, nodefender
set -u
variant="${1:-diag}"
box="${2:-6000}"
export TZ=UTC
mkdir -p e5
rm -f e5/stop.flag e5/snap.flag e5/done.flag
stamp() { printf '%(%H:%M:%S)T' -1; }

echo "== e5 probe v2: variant $variant, time box ${box} s, $(date -u +%FT%TZ)"
tgt="target-e5-$variant"
envs=()
case "$variant" in
  diag) ;;
  lean) envs=(CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256) ;;
  jobs1) envs=(CARGO_BUILD_JOBS=1) ;;
  nodefender) ;;
  *) echo "unknown variant $variant"; exit 2 ;;
esac
export CARGO_TARGET_DIR="$tgt"

echo "== the machine"
powershell.exe -NoProfile -Command "\$cs = Get-CimInstance Win32_ComputerSystem; 'RAM_MB=' + [int](\$cs.TotalPhysicalMemory/1MB) + ' logical_cpus=' + \$cs.NumberOfLogicalProcessors; Get-CimInstance Win32_PageFileUsage | ForEach-Object { 'pagefile ' + \$_.Name + ' allocated_mb=' + \$_.AllocatedBaseSize + ' peak_mb=' + \$_.PeakUsage }; \$os = Get-CimInstance Win32_OperatingSystem; 'virtual_total_mb=' + [int](\$os.TotalVirtualMemorySize/1024) + ' ' + \$os.Caption + ' ' + \$os.Version; Get-PSDrive -PSProvider FileSystem | ForEach-Object { 'drive ' + \$_.Name + ' free_gb=' + [int](\$_.Free/1GB) }; try { \$mp = Get-MpComputerStatus; 'defender: realtime=' + \$mp.RealTimeProtectionEnabled + ' antivirus=' + \$mp.AntivirusEnabled + ' service=' + \$mp.AMServiceEnabled } catch { 'defender: ' + \$_ }; try { 'exclusions: ' + ((Get-MpPreference).ExclusionPath -join ' ') } catch {}"
if [ "$variant" = nodefender ]; then
  powershell.exe -NoProfile -Command "Set-MpPreference -DisableRealtimeMonitoring \$true; Add-MpPreference -ExclusionPath 'D:\\','C:\\'; 'after: realtime disabled=' + (Get-MpPreference).DisableRealtimeMonitoring"
fi
echo "== the variant: ${envs[*]:-the environment of the job} CARGO_TARGET_DIR=$CARGO_TARGET_DIR"

echo "== the toolchain (the job's steps: clone, download the asset, unpack, link)"
GIT_TERMINAL_PROMPT=0 git clone --depth 1 --branch rust9x https://github.com/rust9x/rust.git rust9x >/dev/null 2>&1 || echo "(the clone failed; the build does not read it)"
mkdir -p dist rust9x/build/x86_64-pc-windows-msvc
curl -fsSL -o dist/rust9x.tar.gz https://github.com/fschutt/azul/releases/download/rust9x-toolchain/rust9x-stage1-x86_64-pc-windows-msvc.tar.gz || { echo "asset download failed"; exit 3; }
tar -xzf dist/rust9x.tar.gz -C rust9x/build/x86_64-pc-windows-msvc --exclude=stage1/lib/rustlib/rustc-src/rust --exclude=stage1/lib/rustlib/src/rust
rustup toolchain link rust9x "$(pwd)/rust9x/build/x86_64-pc-windows-msvc/stage1"
rustc +rust9x -vV | head -4

echo "== the logger"
cat > e5/logger.ps1 <<'PS'
$ErrorActionPreference = 'Continue'
$log = 'e5\mem.log'
$snaps = 0
function Stamp { (Get-Date).ToUniversalTime().ToString('HH:mm:ss') }
while (-not (Test-Path 'e5\stop.flag')) {
  try {
    $c = Get-Counter -Counter '\Memory\Committed Bytes','\Memory\Commit Limit','\Memory\Available MBytes','\Memory\Pool Nonpaged Bytes','\Memory\Pool Paged Bytes','\Memory\Free System Page Table Entries','\Process(_Total)\Handle Count' -ErrorAction Stop
    $h = @{}
    foreach ($s in $c.CounterSamples) { $h[$s.Path.Substring($s.Path.LastIndexOf('\') + 1)] = $s.CookedValue }
    $all = @(Get-Process -ErrorAction SilentlyContinue | Sort-Object PrivateMemorySize64 -Descending)
    $rustc = @($all | Where-Object { $_.ProcessName -eq 'rustc' })
    $top = ($all | Select-Object -First 5 | ForEach-Object { '{0}:{1}MB/v{2}MB/h{3}' -f $_.ProcessName, [int]($_.PrivateMemorySize64 / 1MB), [int]($_.VirtualMemorySize64 / 1MB), $_.HandleCount }) -join ' '
    $threads = ($all | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
    $line = '{0} committed_mb={1:F0} limit_mb={2:F0} avail_mb={3:F0} nonpaged_mb={4:F0} paged_mb={5:F0} free_ptes={6:F0} handles={7:F0} procs={8} threads={9} rustc={10} top=[{11}]' -f (Stamp), ($h['committed bytes']/1MB), ($h['commit limit']/1MB), $h['available mbytes'], ($h['pool nonpaged bytes']/1MB), ($h['pool paged bytes']/1MB), $h['free system page table entries'], $h['handle count'], $all.Count, $threads, $rustc.Count, $top
    Add-Content -Path $log -Value $line
    if ((Test-Path 'e5\snap.flag') -and $snaps -lt 2) {
      $snaps++
      Remove-Item 'e5\snap.flag' -ErrorAction SilentlyContinue
      Add-Content -Path 'e5\snapshot.txt' -Value ('== snapshot ' + $snaps + ' at ' + (Stamp))
      $all | Select-Object -First 25 Name, Id, @{n='private_mb';e={[int]($_.PrivateMemorySize64/1MB)}}, @{n='ws_mb';e={[int]($_.WorkingSet64/1MB)}}, @{n='virt_mb';e={[int]($_.VirtualMemorySize64/1MB)}}, HandleCount, @{n='threads';e={$_.Threads.Count}} | Format-Table -AutoSize | Out-String -Width 220 | Add-Content -Path 'e5\snapshot.txt'
      (Get-Counter '\Memory\*' -ErrorAction SilentlyContinue).CounterSamples | ForEach-Object { '{0} = {1:N0}' -f $_.Path, $_.CookedValue } | Add-Content -Path 'e5\snapshot.txt'
    }
  } catch { Add-Content -Path $log -Value ('{0} logger error: {1}' -f (Stamp), $_) }
  Start-Sleep -Seconds 4
}
PS
powershell.exe -NoProfile -ExecutionPolicy Bypass -File e5/logger.ps1 &
sleep 12
echo "first logger line: $(head -n 1 e5/mem.log 2>/dev/null | cut -c1-300)"

( while [ ! -f e5/done.flag ]; do
    sleep 60
    echo "[heartbeat $(stamp)] $(tail -n 1 e5/mem.log 2>/dev/null | cut -c1-330) | crates compiled: $(grep -c 'Compiling ' e5/build.log 2>/dev/null) | last: $(grep 'Compiling ' e5/build.log 2>/dev/null | tail -n 1)"
  done ) &

# the canary: every 10 s a NEW GUI-subsystem process (regsvr32) loads a proc-macro DLL that the build made. Exit 4: it loaded (and has
# no DllRegisterServer), 3: LoadLibrary failed. If it fails when the build's rustc does, the state is the system's; if never, rustc's own.
( while [ ! -f e5/done.flag ]; do
    dll="$(find "$tgt/release/build" -name 'serde_derive-*.dll' 2>/dev/null | head -n 1)"
    if [ -n "$dll" ]; then
      regsvr32 /s "$(cygpath -w "$(pwd)/$dll")" >/dev/null 2>&1
      echo "$(stamp) canary exit $?" >> e5/canary.log
    fi
    sleep 10
  done ) &

start_all=$SECONDS
# attempt <name> <seconds> <env words...>: one `cargo build` of the job, its output stamped into e5/build.log
attempt() {
  local name="$1" limit="$2"; shift 2
  echo "== attempt $name begins $(stamp) (env: $*)"
  echo "ATTEMPT $name" >> e5/build.log
  local t0=$SECONDS
  { env "$@" bash -c 'cargo +rust9x build --release --target i686-rust9x-windows-msvc -p azul-dll --features=dist 2>&1; echo "CARGO_EXIT=$?"'; } \
    | while IFS= read -r line; do
        printf '%s %s\n' "$(stamp)" "$line"
        case "$line" in *LoadLibraryExW*|*"can't find crate"*) touch e5/snap.flag ;; esac
      done >> e5/build.log &
  local pid=$!
  while kill -0 "$pid" 2>/dev/null; do
    sleep 15
    if [ $((SECONDS - t0)) -ge "$limit" ] || [ $((SECONDS - start_all)) -ge "$box" ]; then
      echo "== TIME BOX in attempt $name: stopping the build"
      taskkill //F //IM cargo.exe >/dev/null 2>&1; taskkill //F //IM rustc.exe >/dev/null 2>&1
      echo "TIMEBOX" >> e5/build.log
      break
    fi
  done
  wait "$pid" 2>/dev/null
  echo "== attempt $name ended after $((SECONDS - t0)) s: $(grep -a 'CARGO_EXIT=' e5/build.log | tail -n 1)"
}

build_failed() { ! grep -aq 'CARGO_EXIT=0' e5/build.log; }

attempt first 3600 "${envs[@]:-X_E5=1}"

if [ "$variant" = diag ] && build_failed; then
  echo "== TESTS OF THE FAILURE, $(stamp)"
  cat > e5/pe.ps1 <<'PS'
# the PE header of each proc-macro DLL: ImageBase, flags, relocations
Get-ChildItem -Recurse -Path $args[0] -Filter '*.dll' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -match '\\out\\' } | Sort-Object Length -Descending | Select-Object -First 400 | ForEach-Object {
  $b = [IO.File]::ReadAllBytes($_.FullName)
  $pe = [BitConverter]::ToInt32($b, 0x3C)
  $fileChars = [BitConverter]::ToUInt16($b, $pe + 4 + 18)
  $magic = [BitConverter]::ToUInt16($b, $pe + 24)
  $imageBase = [BitConverter]::ToUInt64($b, $pe + 24 + 24)
  $sizeOfImage = [BitConverter]::ToUInt32($b, $pe + 24 + 56)
  $dllChars = [BitConverter]::ToUInt16($b, $pe + 24 + 70)
  'D {0,10} bytes image={1,10} base=0x{2:X} filechars=0x{3:X} dllchars=0x{4:X} {5}' -f $_.Length, $sizeOfImage, $imageBase, $fileChars, $dllChars, $_.Name
  'K highentropy={0} dynamicbase={1} relocs_stripped={2} base=0x{3:X}' -f [bool]($dllChars -band 0x20), [bool]($dllChars -band 0x40), [bool]($fileChars -band 1), $imageBase
}
PS
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File e5/pe.ps1 "$tgt/release/build" > e5/pe_all.txt 2>&1
  grep '^D ' e5/pe_all.txt | head -12 > e5/pe_head.txt
  grep '^K ' e5/pe_all.txt | sort | uniq -c | sort -rn | head -8 > e5/pe_kinds.txt
  echo "-- PE headers of the biggest proc-macro DLLs:"; cut -c1-250 e5/pe_head.txt
  echo "-- kinds over every DLL of the build:"; cat e5/pe_kinds.txt

  cat > e5/loadtest.ps1 <<'PS'
Add-Type -TypeDefinition @'
using System; using System.IO; using System.IO.MemoryMappedFiles; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class Probe {
  [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern IntPtr LoadLibraryExW(string name, IntPtr file, uint flags);
  [DllImport("kernel32.dll")] public static extern bool FreeLibrary(IntPtr h);
  public static string Load(string dll) {
    IntPtr h = LoadLibraryExW(dll, IntPtr.Zero, 8);
    int err = Marshal.GetLastWin32Error();
    if (h != IntPtr.Zero) FreeLibrary(h);
    return h == IntPtr.Zero ? "FAILED error " + err : "ok";
  }
  // maps files one after the other (whole, read-only, like rustc maps the metadata of its dependencies) and tries to
  // load a DLL every `every` files: the mapped bytes at which it first fails
  public static string Stress(string dll, string[] files, int every) {
    List<IDisposable> keep = new List<IDisposable>();
    long total = 0; int n = 0; StringBuilder sb = new StringBuilder();
    foreach (string f in files) {
      try {
        MemoryMappedFile mm = MemoryMappedFile.CreateFromFile(f, FileMode.Open, null, 0, MemoryMappedFileAccess.Read);
        MemoryMappedViewAccessor va = mm.CreateViewAccessor(0, 0, MemoryMappedFileAccess.Read);
        keep.Add(va); keep.Add(mm);
        total += new FileInfo(f).Length; n++;
      } catch (Exception e) { sb.AppendLine("map failed after " + n + " files, " + (total >> 20) + " MB: " + e.Message); break; }
      if (n % every == 0) {
        string r = Load(dll);
        sb.AppendLine(n + " files, " + (total >> 20) + " MB mapped: LoadLibraryExW " + r);
        if (r != "ok") break;
      }
    }
    string last = Load(dll);
    sb.AppendLine("end: " + n + " files, " + (total >> 20) + " MB mapped: LoadLibraryExW " + last);
    foreach (IDisposable k in keep) k.Dispose();
    return sb.ToString();
  }
}
'@
$macros = @(Get-ChildItem -Recurse -Path $args[0] -Filter '*.dll' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -match '\\out\\' -and $_.Name -match '^(serde_derive|derive_more_impl|bincode_derive|ouroboros_macro|zeroize_derive|thiserror_impl)' })
"proc-macro DLLs found: " + $macros.Count
foreach ($m in $macros) { '{0,9} bytes {1} -> {2}' -f $m.Length, $m.Name, [Probe]::Load($m.FullName) }
$files = @(Get-ChildItem -Recurse -Path $args[1] -Include '*.rlib','*.rmeta' -ErrorAction SilentlyContinue | Sort-Object Length -Descending)
"rlib/rmeta files: " + $files.Count + ", " + [int](($files | Measure-Object Length -Sum).Sum / 1MB) + " MB"
if ($macros.Count -gt 0) { [Probe]::Stress($macros[0].FullName, ($files | ForEach-Object { $_.FullName }), 25) }
PS
  echo "-- a fresh process loads the proc-macro DLLs, then maps every rlib/rmeta of the build and loads again:"
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File e5/loadtest.ps1 "$tgt/release/build" "$tgt" 2>&1 | tee e5/loadtest.txt | cut -c1-250

  echo "-- rustc itself, in a fresh process, with each proc-macro DLL as an extern crate (metadata only):"
  for dll in $(find "$tgt/release/build" -name 'serde_derive-*.dll' -o -name 'derive_more_impl-*.dll' -o -name 'bincode_derive-*.dll' 2>/dev/null | head -3); do
    crate="$(basename "$dll" | sed -E 's/-[0-9a-f]+\.dll$//')"
    printf 'extern crate %s;\nfn main() {}\n' "$crate" > e5/probe_macro.rs
    out="$(rustc +rust9x --edition 2021 --emit=metadata -o e5/probe_macro.rmeta --extern "$crate=$dll" e5/probe_macro.rs 2>&1 | head -4 | tr '\n' ' ')"
    echo "$crate: ${out:-ok}"
  done | tee e5/rustc_probe.txt | cut -c1-300

  echo "== attempt 2: the same build again, nothing changed, $(stamp)"
  attempt second 2400 X_E5=1
  if build_failed; then
    echo "== attempt 3: one job, $(stamp)"
    attempt third 2400 CARGO_BUILD_JOBS=1
  fi
fi

touch e5/done.flag e5/stop.flag
sleep 8

echo "== SUMMARY ($variant)"
{
  echo "crates compiled (all attempts): $(grep -ac 'Compiling ' e5/build.log)"
  grep -a 'ATTEMPT\|CARGO_EXIT=\|TIMEBOX' e5/build.log
  echo "-- errors:"
  grep -a 'LoadLibraryExW' e5/build.log | cut -c1-250 | head -8
  echo "LoadLibraryExW lines: $(grep -ac 'LoadLibraryExW' e5/build.log); can't find crate lines: $(grep -ac "can't find crate" e5/build.log)"
  grep -a 'could not compile' e5/build.log | head -8
  grep -a 'Finished' e5/build.log | head -3
  echo "-- the memory log:"
  echo "lines: $(wc -l < e5/mem.log)"
  awk '{ for (i = 1; i <= NF; i++) { split($i, a, "="); if (a[1] == "committed_mb" && a[2] + 0 > m) { m = a[2] + 0; t = $1 } if (a[1] == "limit_mb") lim = a[2] + 0; if (a[1] == "avail_mb" && (mn == "" || a[2] + 0 < mn)) { mn = a[2] + 0; tm = $1 } if (a[1] == "rustc" && a[2] + 0 > r) { r = a[2] + 0; tr = $1 } } } END { print "peak committed_mb=" m " at " t " (limit " lim ")"; print "lowest avail_mb=" mn " at " tm; print "most rustc processes: " r " at " tr }' e5/mem.log
  for first in $(grep -a "LoadLibraryExW\|can't find crate" e5/build.log | head -n 1 | cut -d' ' -f1); do
    echo "first failure at $first; the memory log around it:"
    s="$(awk -F: '{print $1*3600+$2*60+$3}' <<<"$first")"
    awk -v s="$s" '{ split($1, p, ":"); x = p[1]*3600 + p[2]*60 + p[3]; if (x >= s - 30 && x <= s + 10) print }' e5/mem.log | cut -c1-330
  done
  echo "-- the biggest processes (virtual MB, private MB) over the whole run:"
  grep -o 'top=\[[^]]*\]' e5/mem.log | tr ' ' '\n' | grep -o 'rustc:[0-9]*MB/v[0-9]*MB' | sort -t/ -k2 -V | tail -3
  echo "-- the canary (exit 4: loaded, 3: LoadLibrary failed): $(grep -c 'exit 4' e5/canary.log 2>/dev/null) loaded, $(grep -c 'exit 3' e5/canary.log 2>/dev/null) failed, other: $(grep -vc 'exit [34]' e5/canary.log 2>/dev/null)"
  grep -v 'exit 4' e5/canary.log 2>/dev/null | head -20
  echo "-- snapshot:"; head -60 e5/snapshot.txt 2>/dev/null | cut -c1-230
  echo "-- disk:"; df -h . /c 2>/dev/null | head -4
} 2>&1 | tee e5/summary.txt

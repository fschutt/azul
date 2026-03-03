<#
.SYNOPSIS
    Rust GUI Toolkit Tester - QEMU Launcher for Windows 11/MSYS2
    
    FEATURES:
    - WHPX Acceleration (Fast)
    - Fixes Windows 11 Interrupt Freezes (kernel-irqchip=off)
    - Manages distinct disks for X11 (Mint) and Wayland (nwg-live)
#>

# --- CONFIGURATION (Edit these if filenames differ) ---
param (
    [string]$MintIso = "linuxmint.iso",
    [string]$WaylandIso = "nwg-live.iso",
    
    # Disk Names
    [string]$MintDisk = "mint-os.img",        # Main OS installation
    [string]$WaylandDisk = "wayland-data.img", # Scratchpad for Live ISO
    
    # System Specs
    [string]$Ram = "8G",  # 8GB is recommended for Rust compilation
    [int]$Cores = 4       # Give it enough power
)

# --- SETUP ---
$ScriptDir = $PSScriptRoot
$QemuBin = "qemu-system-x86_64.exe"
$QemuImg = "qemu-img.exe"

# 1. Locate QEMU (Check PATH, then check MSYS2 default)
if (-not (Get-Command $QemuBin -ErrorAction SilentlyContinue)) {
    $PotentialPath = "C:\msys64\ucrt64\bin"
    if (Test-Path "$PotentialPath\$QemuBin") {
        $env:Path += ";$PotentialPath"
    } else {
        Write-Error "QEMU not found! Please ensure 'mingw-w64-ucrt-x86_64-qemu' is installed."
        pause; exit
    }
}

# --- HELPER FUNCTIONS ---

function Print-Section {
    param ([string]$Title, [string]$Desc)
    Write-Host "`n$Title" -ForegroundColor Cyan
    Write-Host ("-" * $Title.Length) -ForegroundColor Cyan
    if ($Desc) { Write-Host $Desc -ForegroundColor Gray }
}

function Ensure-Disk {
    param (
        [string]$Filename, 
        [string]$Size, 
        [string]$Description
    )
    $Path = Join-Path $ScriptDir $Filename
    
    if (-not (Test-Path $Path)) {
        Write-Host " [NEW] Creating $Description ($Filename)..." -ForegroundColor Yellow
        Write-Host "       Size: $Size (Sparse - only uses space as needed)" -ForegroundColor DarkGray
        
        # Create disk, pipe to null to keep menu clean
        & $QemuImg create -f qcow2 "$Path" $Size | Out-Null
        
        Write-Host "       [OK] Created." -ForegroundColor Green
    } else {
        Write-Host " [FOUND] Using existing $Description ($Filename)" -ForegroundColor DarkGray
    }
    return $Path
}

function Launch-Qemu {
    param (
        [string]$IsoPath,
        [string]$HddPath,
        [bool]$BootFromIso,
        [string]$ModeName
    )

    Print-Section "Launching VM: $ModeName"
    
    # Logic Explanation
    if ($BootFromIso -and $HddPath) {
        Write-Host " Mode: INSTALLER / LIVE" -ForegroundColor Magenta
        Write-Host " 1. CD-ROM: Loaded (Boot Priority)"
        Write-Host " 2. HDD:    Attached (Target for install/data)"
    }
    elseif ($BootFromIso -and -not $HddPath) {
        Write-Host " Mode: LIVE ONLY (RAM)" -ForegroundColor Magenta
    }
    else {
        Write-Host " Mode: INSTALLED OS" -ForegroundColor Magenta
        Write-Host " 1. HDD:    Booting native installation"
    }

    Write-Host " Specs: $Cores Cores | $Ram RAM | Accel: WHPX" -ForegroundColor DarkGray
    Write-Host " Fixes: kernel-irqchip=off (Win11 interrupt fix applied)" -ForegroundColor DarkGray

    # QEMU Arguments
    $ArgsList = @(
        "-accel", "whpx",
        "-machine", "kernel-irqchip=off",  # <--- CRITICAL FIX FOR WINDOWS 11
        "-m", $Ram,
        "-smp", $Cores,
        "-device", "virtio-vga",           # GPU
        "-display", "gtk,gl=off",          # Display (GTK is most stable on Win)
        "-audiodev", "dsound,id=snd0",     # Audio
        "-device", "intel-hda",
        "-device", "hda-duplex,audiodev=snd0",
        "-net", "nic,model=virtio",        # Network
        "-net", "user"
    )

    # Drive Configuration
    if ($HddPath) {
        # Mount the hard disk (virtio for speed)
        $ArgsList += "-drive", "file=$HddPath,format=qcow2,if=virtio"
    }

    if ($BootFromIso) {
        if (-not (Test-Path $IsoPath)) { Write-Error "ISO not found: $IsoPath"; pause; return }
        $ArgsList += "-cdrom", "$IsoPath"
        $ArgsList += "-boot", "d" # Boot from CD
    } else {
        $ArgsList += "-boot", "c" # Boot from HDD
    }

    Write-Host "`nStarting QEMU..." -ForegroundColor Green
    & $QemuBin $ArgsList
}

# --- MAIN MENU ---

Clear-Host
Print-Section "RUST GUI TOOLKIT TESTER" "Windows 11 / MSYS2 Edition"

Write-Host " 1. Install Linux Mint (X11)" -ForegroundColor White
Write-Host "    - Boots ISO + 40GB Empty Disk." -ForegroundColor DarkGray
Write-Host "    - ACTION: Run installer inside VM -> Select 'Erase Disk'." -ForegroundColor Yellow
Write-Host ""
Write-Host " 2. Run Linux Mint (X11) - PERSISTENT" -ForegroundColor White
Write-Host "    - Boots the installed OS from disk." -ForegroundColor DarkGray
Write-Host "    - No ISO required." -ForegroundColor DarkGray
Write-Host ""
Write-Host " 3. Run Wayland (nwg-live) - LIVE + DATA" -ForegroundColor White
Write-Host "    - Boots Live ISO + 20GB Data Disk." -ForegroundColor DarkGray
Write-Host "    - Use the Data Disk to compile Rust (prevents OOM)." -ForegroundColor DarkGray
Write-Host ""
Write-Host " Q. Quit" -ForegroundColor Gray

$Choice = Read-Host "`nSelect Option"

switch ($Choice) {
    "1" { 
        # Create Main OS Disk (40GB)
        $Disk = Ensure-Disk -Filename $MintDisk -Size "40G" -Description "Mint OS Drive"
        $Iso = Join-Path $ScriptDir $MintIso
        Launch-Qemu -IsoPath $Iso -HddPath $Disk -BootFromIso $true -ModeName "Mint Installer"
    }
    "2" { 
        # Check if installed
        $DiskPath = Join-Path $ScriptDir $MintDisk
        if (-not (Test-Path $DiskPath)) { Write-Error "Run Option 1 to install Mint first!"; pause; exit }
        Launch-Qemu -IsoPath "" -HddPath $DiskPath -BootFromIso $false -ModeName "Mint (Installed)"
    }
    "3" { 
        # Create Data Disk for Wayland (20GB)
        $Disk = Ensure-Disk -Filename $WaylandDisk -Size "20G" -Description "Wayland Data Drive"
        $Iso = Join-Path $ScriptDir $WaylandIso
        Launch-Qemu -IsoPath $Iso -HddPath $Disk -BootFromIso $true -ModeName "Wayland Live + Data"
    }
    "Q" { exit }
    Default { Write-Host "Invalid Selection" -ForegroundColor Red }
}

Write-Host "`nProcess Exited."
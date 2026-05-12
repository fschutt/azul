# examples/powershell/hello-world.ps1
#
# PowerShell port of examples/c/hello-world.c. The bindings work by
# JIT-compiling the embedded C# source (the same `Azul.cs` content
# from `lang_csharp/`) at module import time via `Add-Type`. That
# gives us full access to the `[Azul.*]` wrapper classes — Dom,
# Button, App, WindowCreateOptions — plus the `[Azul.HostInvoker]`
# helpers for refany / callback registration.
#
# Run with:
#   pwsh -File ./hello-world.ps1
#
# Requires:
#   * PowerShell 7+ (Windows PowerShell 5.1 also works on .NET Framework).
#   * `azul.dll` / `libazul.so` / `libazul.dylib` next to this file, or
#     on the dynamic-loader search path (Set-AzulLibraryPath wires it up).

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Azul.psd1') -Force
Set-AzulLibraryPath -Path $PSScriptRoot

# ── Data model ────────────────────────────────────────────────────────
# PSCustomObject works as the model. RefAny holds an opaque Object
# reference in the host-handle table.
$model = [PSCustomObject]@{ Counter = 5 }
$data  = [Azul.HostInvoker]::RefanyCreate($model)

# ── Helper ────────────────────────────────────────────────────────────
# Build an AzString from a PowerShell string. The wrapper class
# constructor isn't exposed for AzString (`internal`), so we call the
# C# `Azul.String.FromUtf8` factory which copies the bytes.
function Convert-AzulString {
    param([Parameter(Mandatory=$true)][string]$Value)
    $bytes  = [System.Text.Encoding]::UTF8.GetBytes($Value)
    $handle = [System.Runtime.InteropServices.GCHandle]::Alloc($bytes,
        [System.Runtime.InteropServices.GCHandleType]::Pinned)
    try {
        $ptr = $handle.AddrOfPinnedObject()
        # AzString_fromUtf8 copies internally, so the pinned bytes can
        # be released immediately after the call returns.
        return [Azul.String]::FromUtf8($ptr, [System.UIntPtr]$bytes.Length)
    } finally {
        $handle.Free()
    }
}

# ── Callbacks ─────────────────────────────────────────────────────────
# PowerShell scriptblocks wrap as .NET delegates via the `-as`
# conversion operator. Signatures match the C# delegate types declared
# in `Azul.HostInvoker`.

$onClick = {
    param([IntPtr]$dataPtr, [IntPtr]$infoPtr)
    $m = [Azul.HostInvoker]::RefanyGet($dataPtr)
    if ($m -is [PSCustomObject]) {
        $m.Counter = $m.Counter + 1
        return 1   # AzUpdate.RefreshDom
    }
    return 0       # AzUpdate.DoNothing
}.GetNewClosure()

$layout = {
    param([IntPtr]$dataPtr, [IntPtr]$infoPtr)
    $m = [Azul.HostInvoker]::RefanyGet($dataPtr)
    if (-not ($m -is [PSCustomObject])) {
        return ([Azul.Dom]::CreateBody()).Raw
    }

    # Counter label, wrapped in a font-size-32 div.
    $counterDom = [Azul.Dom]::CreateText((Convert-AzulString -Value ([string]$m.Counter)))
    $labelDiv   = [Azul.Dom]::CreateDiv().WithCss((Convert-AzulString -Value 'font-size: 32px;'))
    $labelDiv   = $labelDiv.WithChild($counterDom.Raw)

    # Increment button.
    $button = [Azul.Button]::Create((Convert-AzulString -Value 'Increase counter'))
    $button = $button.WithButtonType([Azul.Native.AzButtonType]::Primary)
    $clickCb = [Azul.HostInvoker]::RegisterCallback($onClick)
    $dataClone = [Azul.HostInvoker]::RefanyCreate($m)
    $button = $button.WithOnClick($dataClone, $clickCb)
    $buttonDom = $button.Dom()

    # Body.
    $body = [Azul.Dom]::CreateBody().WithChild($labelDiv.Raw).WithChild($buttonDom)
    return $body.Raw
}.GetNewClosure()

# ── Main ──────────────────────────────────────────────────────────────
# `WindowCreateOptions::Create(layout_callback)` discards host-invoker
# ctx (takes a raw AzLayoutCallbackType fn pointer). Use the default
# value then assign the layout_callback via reflection on Raw.

Write-Host "[ps] converting layout to delegate"
$layoutDelegate = $layout -as [System.Func[IntPtr, IntPtr, object]]
if (-not $layoutDelegate) { Write-Error "layout delegate conversion failed"; exit 1 }
Write-Host "[ps] registering layout callback"
$layoutCb = [Azul.HostInvoker]::RegisterLayoutCallback($layoutDelegate)
Write-Host "[ps] layout callback registered"

Write-Host "[ps] creating WCO"
$wco = [Azul.WindowCreateOptions]::Default()
Write-Host "[ps] WCO created: type=$($wco.GetType().FullName)"
$wcoRaw = $wco.Raw
Write-Host "[ps] wcoRaw type=$($wcoRaw.GetType().FullName)"

# AzWindowCreateOptions and AzFullWindowState are public C# structs
# (StructLayout=Sequential). PowerShell can read their public fields
# directly. Boxed structs need mutation through a temp copy then
# write-back since PowerShell unboxes on field access.
$ws = $wcoRaw.window_state
Write-Host "[ps] ws type=$($ws.GetType().FullName)"
$ws.layout_callback = $layoutCb
$wcoRaw.window_state = $ws

Write-Host "[ps] creating AppConfig"
$cfg = [Azul.AppConfig]::Create()
Write-Host "[ps] cfg type=$($cfg.GetType().FullName)"
$cfgRaw = $cfg.Raw
Write-Host "[ps] cfg.Raw type=$($cfgRaw.GetType().FullName)"
Write-Host "[ps] data type=$($data.GetType().FullName)"
Write-Host "[ps] calling App.Create"
$app = [Azul.App]::Create($data, $cfgRaw)
Write-Host "[ps] App created, calling Run"
$app.Run($wcoRaw)
Write-Host "[ps] App.Run returned"

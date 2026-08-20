Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Azul.psd1') -Force
Set-AzulLibraryPath -Path $PSScriptRoot

$model = [PSCustomObject]@{ Counter = 5 }
$data  = [Azul.HostInvoker]::RefanyCreate($model)

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

    $counterDom = [Azul.Dom]::CreateTextDoNotUseWithoutBlockLevelWrapper((Convert-AzulString -Value ([string]$m.Counter)))
    $labelDiv   = [Azul.Dom]::CreateDiv().WithCss((Convert-AzulString -Value 'font-size: 32px;'))
    $labelDiv   = $labelDiv.WithChild($counterDom.Raw)

    $button = [Azul.Button]::Create((Convert-AzulString -Value 'Increase counter'))
    $button = $button.WithButtonType([Azul.Native.AzButtonType]::Primary)
    $clickCb = [Azul.HostInvoker]::RegisterCallback($onClick)
    $dataClone = [Azul.HostInvoker]::RefanyCreate($m)
    $button = $button.WithOnClick($dataClone, $clickCb)
    $buttonDom = $button.Dom()

    $body = [Azul.Dom]::CreateBody().WithChild($labelDiv.Raw).WithChild($buttonDom)
    return $body.Raw
}.GetNewClosure()

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

# Boxed structs need mutation through a temp copy then write-back:
# PowerShell unboxes on field access.
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

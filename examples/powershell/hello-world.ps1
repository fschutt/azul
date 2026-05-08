# Hello-world example for the Azul PowerShell bindings.
#
# This file is the PowerShell port of `examples/c/hello-world.c`. It
# imports the generated `Azul.psm1` (which JIT-compiles the embedded C#
# layer at import time via Add-Type) and uses the Verb-Noun shims plus
# raw .NET wrapper classes where idiomatic.
#
# Behavioural parity with the C version:
#   - A counter starts at 5
#   - Layout draws a label showing the counter and an "Increase counter"
#     button
#   - Clicking the button increments the counter and refreshes the DOM
#
# Prerequisites:
#   1. azul.dll (Windows) or libazul.so (Linux) or libazul.dylib (macOS)
#      next to this script, or on the system search path.
#   2. Azul.psd1 + Azul.psm1 (generated) in the same directory.
#
# Run:
#
#     pwsh ./hello-world.ps1
#
# Or on Windows PowerShell 5.1:
#
#     powershell -ExecutionPolicy Bypass -File ./hello-world.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Make sure the native library next to this script is found before we
# import the module (Add-Type pre-resolves DllImport at type-init time
# on .NET 6+, but adding the path early is cheap insurance).
Import-Module "$PSScriptRoot/Azul.psd1" -Force
Set-AzulLibraryPath -Path $PSScriptRoot

# ── Data model ─────────────────────────────────────────────────────────
#
# The C example uses AZ_REFLECT_JSON plus a destructor / json round-trip
# pair to register a custom type. PowerShell does not have macros, so we
# stash the model in a [System.Runtime.InteropServices.GCHandle] and pass
# the handle's IntPtr to the framework via RefAny.
#
# SKIPPED: real downcast/up-cast helpers — the C example uses
# MyDataModelRefMut_create + MyDataModel_downcastMut. We approximate by
# resolving the GCHandle directly.

class MyDataModel {
    [uint32]$Counter

    MyDataModel([uint32]$counter) {
        $this.Counter = $counter
    }
}

# Keep delegate instances rooted so the GC does not collect them while
# the native side holds raw function pointers. PowerShell stores script
# blocks differently from C# delegates, so we use [Azul.AzLayoutCallbackType]
# constructed via the type system that Add-Type already loaded for us.
$Script:LayoutDelegate  = $null
$Script:OnClickDelegate = $null

# ── Callback ───────────────────────────────────────────────────────────

$onClickScriptBlock = {
    param([IntPtr]$data, [Azul.AzCallbackInfo]$info)

    # SKIPPED: a real binding would call AzRefAny_downcast_mut here.
    # We assume the IntPtr we stored is a GCHandle and recover the model.
    $handle = [System.Runtime.InteropServices.GCHandle]::FromIntPtr($data)
    if ($handle.Target -is [MyDataModel]) {
        $model = [MyDataModel]$handle.Target
        $model.Counter = $model.Counter + 1
        return [Azul.AzUpdate_Tag]::RefreshDom
    }
    return [Azul.AzUpdate_Tag]::DoNothing
}

# ── Layout ─────────────────────────────────────────────────────────────

$layoutScriptBlock = {
    param([IntPtr]$data, [Azul.AzLayoutCallbackInfo]$info)

    $handle = [System.Runtime.InteropServices.GCHandle]::FromIntPtr($data)
    $model = $null
    if ($handle.Target -is [MyDataModel]) {
        $model = [MyDataModel]$handle.Target
    }

    $counterText = if ($null -ne $model) { $model.Counter.ToString() } else { '?' }

    # Counter label, wrapped in a div to make it block-level.
    $labelStr     = ConvertTo-AzulString $counterText
    $label        = [Azul.NativeMethods]::AzDom_createText($labelStr)
    $labelWrapper = [Azul.NativeMethods]::AzDom_createDiv()
    $fontSizeProp = [Azul.NativeMethods]::AzCssProperty_fontSize(
        [Azul.NativeMethods]::AzStyleFontSize_px(32.0))
    [Azul.NativeMethods]::AzDom_addCssProperty(
        [ref]$labelWrapper,
        [Azul.NativeMethods]::AzCssPropertyWithConditions_simple($fontSizeProp))
    [Azul.NativeMethods]::AzDom_addChild([ref]$labelWrapper, $label)

    # Button.
    $buttonText = ConvertTo-AzulString 'Increase counter'
    $button     = [Azul.NativeMethods]::AzButton_create($buttonText)
    [Azul.NativeMethods]::AzButton_setButtonType(
        [ref]$button, [uint32][Azul.AzButtonType]::Primary)

    # Clone the RefAny so the button takes its own reference.
    $dataClone = [Azul.NativeMethods]::AzRefAny_clone($data)
    [Azul.NativeMethods]::AzButton_setOnClick(
        [ref]$button, $dataClone, $Script:OnClickDelegate)
    $buttonDom = [Azul.NativeMethods]::AzButton_dom($button)

    # Body.
    $body = [Azul.NativeMethods]::AzDom_createBody()
    [Azul.NativeMethods]::AzDom_addChild([ref]$body, $labelWrapper)
    [Azul.NativeMethods]::AzDom_addChild([ref]$body, $buttonDom)

    return [Azul.NativeMethods]::AzDom_style($body, [Azul.NativeMethods]::AzCss_empty())
}

# ── Helpers ────────────────────────────────────────────────────────────

# Allocate an AzString from a managed string by going through a UTF-8
# byte buffer and the public copyFromBytes constructor.
function ConvertTo-AzulString {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true, Position=0)]
        [string]$Text
    )
    $utf8 = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $pinned = [System.Runtime.InteropServices.GCHandle]::Alloc(
        $utf8, [System.Runtime.InteropServices.GCHandleType]::Pinned)
    try {
        $ptr = $pinned.AddrOfPinnedObject()
        return [Azul.NativeMethods]::AzString_copyFromBytes(
            $ptr, [System.UIntPtr]::Zero, [System.UIntPtr]$utf8.Length)
    } finally {
        $pinned.Free()
    }
}

# Wrap a raw IntPtr (a pinned GCHandle) in an AzRefAny.
# SKIPPED: a real implementation would call AzRefAny_new with a
# destructor pointer. This stub uses the C-friendly newC entry point.
function New-AzulRefAnyForHandle {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true, Position=0)]
        [IntPtr]$Ptr
    )
    return [Azul.NativeMethods]::AzRefAny_newC(
        $Ptr,
        [System.UIntPtr]::Zero,
        [uint32]0,
        (ConvertTo-AzulString 'MyDataModel'),
        [IntPtr]::Zero)
}

# ── Main ───────────────────────────────────────────────────────────────

$model = [MyDataModel]::new([uint32]5)

# Pin the model so the native side can reach it through a stable IntPtr
# until we explicitly free it.
$handle = [System.Runtime.InteropServices.GCHandle]::Alloc(
    $model, [System.Runtime.InteropServices.GCHandleType]::Normal)
$data = New-AzulRefAnyForHandle -Ptr ([System.Runtime.InteropServices.GCHandle]::ToIntPtr($handle))

# Materialise the script blocks as typed delegates so the FFI accepts
# them. This step is the PowerShell equivalent of C#'s implicit method
# group conversion.
$Script:LayoutDelegate  = $layoutScriptBlock  -as [Azul.AzLayoutCallbackType]
$Script:OnClickDelegate = $onClickScriptBlock -as [Azul.AzCallbackType]

$window = New-AzulWindowCreateOptions -LayoutCallback $Script:LayoutDelegate
try {
    # SKIPPED: deep field mutation on the FFI struct exposed through
    # `.Raw`. The wrapper class surfaces the raw value-typed struct so we
    # can poke titlebar / dimensions / decoration flags before handing
    # it to App.Run.
    $raw = $window.Raw
    $raw.window_state.title = ConvertTo-AzulString 'Hello World'
    $raw.window_state.size.dimensions.width  = [single]400.0
    $raw.window_state.size.dimensions.height = [single]300.0

    # NoTitleAutoInject: OS draws close/min/max buttons; framework
    # auto-injects a Titlebar with drag support.
    $raw.window_state.flags.decorations =
        [byte][Azul.AzWindowDecorations]::NoTitleAutoInject
    $raw.window_state.flags.background_material =
        [byte][Azul.AzWindowBackgroundMaterial]::Sidebar

    $cfg = [Azul.NativeMethods]::AzAppConfig_create()
    $app = New-AzulApp -Data $data -AppConfig $cfg
    try {
        Invoke-AzulAppRun -Instance $app -WindowOptions $raw
    } finally {
        $app.Dispose()
    }
} finally {
    $window.Dispose()
    $handle.Free()
}

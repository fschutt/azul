# examples/powershell/hello-world.ps1
#
# PowerShell port of examples/c/hello-world.c. PowerShell's bindings work
# by JIT-compiling the embedded C# source (the same `Azul.cs` content from
# `lang_csharp/`) at module import time via Add-Type. That gives us full
# access to the C# `Azul.HostInvoker` class — the host-invoker pattern
# Just Works™ here without a separate PowerShell adapter.
#
# Same shape as examples/csharp/hello-world.cs:
#   * `[Azul.HostInvoker]::RefanyCreate($value)` wraps any value in an
#     AzRefAny held alive by the framework's refcount.
#   * Callbacks are PowerShell scriptblocks wrapped in C# delegates and
#     handed to `[Azul.HostInvoker]::RegisterCallback($delegate)`.
#
# Run with:
#   pwsh -File hello-world.ps1
#
# Requires:
#   * PowerShell 7+ (Windows PowerShell 5.1 also works — the embedded C#
#     compiles on .NET Framework too).
#   * `azul.dll` / `libazul.so` / `libazul.dylib` on the dynamic-loader
#     search path (or call `Set-AzulLibraryPath -Path ...` first).

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Import the generated module. Add-Type compiles the embedded C# the first
# time this module is loaded (~1s on Windows PowerShell 5.1, faster on PS 7+).
Import-Module (Join-Path $PSScriptRoot 'Azul.psd1') -Force

# Make sure the native library is reachable. If hello-world.ps1 lives next
# to azul.dll / libazul.so, this wires the search path.
Set-AzulLibraryPath -Path $PSScriptRoot

# ── Data model ────────────────────────────────────────────────────────
# PSObject is fine for our purposes — RefAny just holds an opaque Object
# reference; PowerShell's PSObject is a real .NET object under the hood.
$model = [PSCustomObject]@{ Counter = 5 }
$data  = [Azul.HostInvoker]::RefanyCreate($model)

# ── Callbacks ─────────────────────────────────────────────────────────
# PowerShell scriptblocks are convertible to delegates via .NET interop.
# We construct concrete delegate types inline (the per-kind "delegate
# void" types live in C# under namespace `Azul.HostInvoker`).

$onClick = {
    param([UInt64]$id, [IntPtr]$dataPtr, [IntPtr]$infoPtr, [IntPtr]$outPtr)
    $m = [Azul.HostInvoker]::RefanyGet($dataPtr)
    if ($m -is [PSCustomObject]) {
        $m.Counter = $m.Counter + 1
        # Write AzUpdate.RefreshDom (1) through the out-pointer.
        [System.Runtime.InteropServices.Marshal]::WriteInt32($outPtr, 1)
    } else {
        # AzUpdate.DoNothing
        [System.Runtime.InteropServices.Marshal]::WriteInt32($outPtr, 0)
    }
}.GetNewClosure()

$layout = {
    param([UInt64]$id, [IntPtr]$dataPtr, [IntPtr]$infoPtr, [IntPtr]$outPtr)
    Write-Error "[azul] layout callback fired (id=$id) — wrappers stub" -ErrorAction Continue
}.GetNewClosure()

# Convert scriptblocks to typed delegates the host-invoker expects.
$clickDelegate  = $onClick -as [Azul.HostInvoker+CallbackInvokerDelegate]
$layoutDelegate = $layout  -as [Azul.HostInvoker+LayoutCallbackInvokerDelegate]

if (-not $clickDelegate -or -not $layoutDelegate) {
    Write-Error "Failed to construct host-invoker delegates."
    exit 1
}

# ── Register with libazul ─────────────────────────────────────────────
$clickCb  = [Azul.HostInvoker]::RegisterCallback($clickDelegate)
$layoutCb = [Azul.HostInvoker]::RegisterLayoutCallback($layoutDelegate)

Write-Host "[azul] host-invoker plumbing wired."
Write-Host "[azul] (Full App.Run wiring requires struct-field setters from"
Write-Host "[azul]  lang_csharp/wrappers.rs which is still a stub today.)"

# Keep the references alive so the GC doesn't collect them between
# registration and the App.Run hand-off.
[void]$clickCb
[void]$layoutCb

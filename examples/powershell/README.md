# Azul — PowerShell

⊘ **Windows-only.** macOS pwsh ships with a CFRunLoop that owns the
main thread, blocking libazul's NSApp.run. The Windows path doesn't
have this constraint.

## Status

- macOS: blocked (won't pursue — pwsh REPL holds the Cocoa main
  thread; the runloop conflicts with libazul's event loop ownership).
- Windows: codegen produces a PowerShell module that embeds the
  C# bindings via `Add-Type` (untested but mechanically should work
  since the C# binding is verified PASS).

## Build + Run (Windows)

```powershell
# 1. Build libazul as azul.dll. From the repo root in a Developer
#    PowerShell prompt:
cd C:\path\to\azul
cargo build --release --features build-dll
# Native library lands at target\release\azul.dll.

# 2. Copy azul.dll next to hello-world.ps1.
copy target\release\azul.dll examples\powershell\

# 3. Run the hello-world. The script Add-Type's the C# bindings
#    inline so no separate .NET project is needed.
cd examples\powershell
pwsh -ExecutionPolicy Bypass -File hello-world.ps1
```

A counter window opens; clicking the button increments the displayed
counter.

## Requirements

- PowerShell 7.x (`pwsh`) recommended; Windows PowerShell 5.x also
  works but `Add-Type` semantics differ slightly.
- .NET 6.0 SDK or later (PowerShell's `Add-Type` uses the in-process
  compiler).
- `azul.dll` next to the script.
- Windows 10+ on x86_64 or aarch64.

## What's idiomatic

The script uses `Add-Type` to inline a small C# wrapper around the
generated `Azul.cs` API surface. PowerShell's runtime then constructs
the App / Dom / WindowCreateOptions objects through that C# layer:

```powershell
$model = [PSCustomObject]@{ Counter = 5 }
$layout = {
    param([IntPtr]$dataPtr, [IntPtr]$infoPtr)
    # ...build Dom via [Azul.Dom]::CreateBody().WithChild(...)...
    return $body.Raw
}
$wco = [Azul.WindowCreateOptions]::Create($layout)
$cfg = [Azul.AppConfig]::Create()
$app = [Azul.App]::Create([Azul.HostInvoker]::RefanyCreate($model), $cfg)
$app.Run($wco)
```

Phase I / J auto-conversion lights up identically through the C#
bindings (strings flow through `Dom.CreateText("hi")`, `Equals` /
`GetHashCode` / `ToString` route through the C-ABI helpers, etc.).

## Files

- `hello-world.ps1` — PowerShell module + main script (verbose; embeds
  the C# Azul namespace and drives App.run).
- `libazul.dylib` / `azul.dll` — prebuilt native library (macOS / Windows).

## macOS workaround?

There is no clean macOS workaround for pwsh — Cocoa requires the main
thread for the NSApp event loop, and pwsh's REPL owns that thread.
The recommended approach on macOS is the C# binding directly
(`cd ../csharp && dotnet run`).

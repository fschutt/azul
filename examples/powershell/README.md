# Azul — PowerShell

⊘ **Windows-only.** macOS pwsh ships with a CFRunLoop that owns the
main thread, blocking libazul's NSApp.run. The Windows path doesn't
have this constraint.

## Status

- macOS: blocked (won't pursue).
- Windows: untested but should work (codegen produces a PowerShell
  module that embeds the C# bindings via Add-Type).

## Files

- `hello-world.ps1` — PowerShell module + main script (verbose; embeds
  the C# Azul namespace and drives App.run).
- `libazul.dylib` / `azul.dll` — prebuilt native library.

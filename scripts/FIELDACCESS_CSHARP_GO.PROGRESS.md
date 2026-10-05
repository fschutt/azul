# Field-access wave: C# / PowerShell / Go

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Worktree branch based on bc606e468 (fix/input-bugs-2026-09-19). No compiling (parent compiles).

## DONE
- C# RED tests 0e388e2eb (lang_csharp/wrappers.rs `mod tests`, fixture `field_fixture_ir` shared with PS/Go).
- C# fix 0ea4e3577: field properties on struct wrapper classes. Scalars in place; string fields
  read via __AzField.ReadString (no consume), set = MakeString + Consume(old, AzString_delete);
  heap-owning class fields are live VIEWS (`_inner` is now a ref property; `__View(__AzRef<T>)`),
  setter = value.__Take() (consume, or deep clone when borrowed) + delete old; POD fields are
  copies (C# rejects `x.Size.dpi = 1`, CS1612, so no silent drop). Method name wins ->
  Get<F>()/Set<F>(v) fallback.

- PowerShell RED tests 5702553fd (lang_powershell/cmdlets.rs `mod tests`).
- PowerShell fix (this commit): cmdlet args = C# `cs_user_args` (Clone no longer takes an
  `[IntPtr]$InstanceArg`), types = C# `cs_param_type` mapped to PS (`[Azul.WindowCreateOptions]`,
  `[string]`, `[Azul.DarkLightMode]` ...); unique cmdlet names (second `New-Azul<T>` becomes
  `New-Azul<T><Method>`); `Copy-Azul*` exported (psm1 + psd1); psm1 header documents field access
  (class fields = live views, value-type fields = copy + assign back). Field access itself comes
  from the C# wrapper properties.

## IN PROGRESS
- Go: exported getter/setter methods; non-destructive `Inner()` accessor.

## Open questions
(none yet)

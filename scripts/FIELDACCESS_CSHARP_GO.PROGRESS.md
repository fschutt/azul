# Field-access wave: C# / PowerShell / Go

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Worktree branch based on bc606e468 (fix/input-bugs-2026-09-19). No compiling (parent compiles).

## DONE
- C# RED tests 0e388e2eb (lang_csharp/wrappers.rs `mod tests`, fixture `field_fixture_ir` shared with PS/Go).
- C# fix (this commit): field properties on struct wrapper classes. Scalars in place; string fields
  read via __AzField.ReadString (no consume), set = MakeString + Consume(old, AzString_delete);
  heap-owning class fields are live VIEWS (`_inner` is now a ref property; `__View(__AzRef<T>)`),
  setter = value.__Take() (consume, or deep clone when borrowed) + delete old; POD fields are
  copies (C# rejects `x.Size.dpi = 1`, CS1612, so no silent drop). Method name wins ->
  Get<F>()/Set<F>(v) fallback.

## IN PROGRESS
- PowerShell: ps_type_of mirrors the C# parameter type; Clone cmdlet has no IntPtr arg; unique cmdlet names.
- Go: exported getter/setter methods; non-destructive `Inner()` accessor.

## Open questions
(none yet)

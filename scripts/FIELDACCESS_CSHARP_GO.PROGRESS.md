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
- PowerShell fix 221ca7f00: cmdlet args = C# `cs_user_args` (Clone no longer takes an
  `[IntPtr]$InstanceArg`), types = C# `cs_param_type` mapped to PS (`[Azul.WindowCreateOptions]`,
  `[string]`, `[Azul.DarkLightMode]` ...); unique cmdlet names (second `New-Azul<T>` becomes
  `New-Azul<T><Method>`); `Copy-Azul*` exported (psm1 + psd1); psm1 header documents field access
  (class fields = live views, value-type fields = copy + assign back). Field access itself comes
  from the C# wrapper properties.

- Go RED tests 492af12d8 (lang_go/wrappers.rs `mod tests`).
- Go fix ca7dab287: `<Field>()` / `Set<Field>(v)` methods on every wrapper (method name wins ->
  `Get<Field>()`); string = GoStr (no consume) / azGoAzString + AzString_delete(old); heap-owning
  wrapper field = borrowed view `&T{ inner: &self.inner.F, borrowed: true }`, setter `v.Raw()`
  (consume, or clone if borrowed) + T_delete(old); other values are copies (deep copy via _clone
  when they own heap memory). Raw() stays the consuming bridge; new non-destructive `Inner() *AzT`.

## IN PROGRESS
(nothing)

## NEXT (parent)
- `cargo test -p azul-doc` for lang_csharp::wrappers::tests, lang_powershell::cmdlets::tests,
  lang_go::wrappers::tests; regenerate target/codegen; build Azul.cs (dotnet), Import-Module
  Azul.psm1 (pwsh 7), `go build` the Go package.

## Open questions
- C#/PS: `_inner` is now a ref-returning property on owning classes (C# 7 ref returns + ref
  lambdas); needs PS 7 Add-Type, which the generated code already requires (`is not`).
- A method taking `self` BY VALUE called on a view / callback-borrowed object still consumes bytes
  it does not own (pre-existing for __Borrow / borrowed Go wrappers). Not changed here.
- PowerShell: `$ws.Size.dimensions.width = 1` mutates a boxed copy and is lost (PowerShell
  value-type semantics); the psm1 header shows copy + assign back.

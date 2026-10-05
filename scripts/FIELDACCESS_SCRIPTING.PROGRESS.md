# Field-access wave - scripting group (Ruby, Lua, PHP-FFI, PHP ext, Perl)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Worktree: .claude/worktrees/agent-a2501398a6097c07c (branch worktree-agent-a2501398a6097c07c, based on bc606e468)
No compiling (parent compiles at integration).

Shared: doc/src/codegen/v2/field_access.rs (language-neutral field classifier: Prim / UnitEnum / Str /
Value{delete, clone}; skips private/pointer/generic/array/callback/callback-wrapper/RefAny and unions
carrying them; skips Vec/String/RefAny/... container structs).

## DONE
- Ruby RED 9356a9fd6 (lang_ruby/mod.rs field_accessor_tests)
- Ruby fix: see next commit (typed accessors; struct fields = live views via Azul._view, no
  finalizer; Azul._own moves wrappers / deep-copies views; _apply_opts releases old + consumes;
  Azul::String args are now consumed too; self-by-value receivers go through _own)

- Ruby fix 1f0e... (see git log: "fix(ruby): typed field accessors")
- Lua RED (test(lua): RED - typed field accessors)
- Lua fix: get_/set_<field> on the methods table; azul._az_ft / _az_delete / _az_clone tables;
  _apply_opts(struct, opts, tn) releases old + deep-copies (_take) instead of aliasing;
  direct cdata field reads stay live views (references, never finalized)

- PHP-FFI RED (test(php): RED - ...)
- PHP-FFI fix: get_/set_<field>; views (`new W($this->ptr->f, $this)`, never freed); ?CData $ptr + $owner;
  intoRaw(); by-value receivers go through intoRaw() (the old `$this->ptr = null` was a TypeError on a
  non-nullable typed property); clone()/toString() lost the extra $instance; toString returns a PHP
  string; Azul::str / Azul::readString

## IN PROGRESS
- PHP ext

## NEXT
- Perl

## Open questions
- field_access.rs may duplicate helpers other groups wrote (unify at integration).

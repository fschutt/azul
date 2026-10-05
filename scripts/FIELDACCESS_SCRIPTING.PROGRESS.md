# Field-access wave - scripting group (Ruby, Lua, PHP-FFI, PHP ext, Perl)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Worktree: .claude/worktrees/agent-a2501398a6097c07c (branch worktree-agent-a2501398a6097c07c, based on bc606e468)
No compiling (parent compiles at integration). ALL LANGUAGES DONE - nothing compiled or run yet.

Shared: doc/src/codegen/v2/field_access.rs (language-neutral field classifier: Prim / UnitEnum / Str /
Value{delete, clone}; skips private/pointer/generic/array/callback/callback-wrapper/RefAny and unions
carrying them; skips Vec/String/RefAny/... container structs). c_layout::field_offsets (new, Perl).

## DONE (RED -> fix)
- Ruby     9356a9fd6 -> c2c146467
- Lua      c0104453a -> 5bd7472e6
- PHP-FFI  addb9f41f -> 62275a896
- PHP ext  6e10b4b8b -> b1a7a6e71 (+ examples/php/hello-world.php sets title + size)
- Perl     3847520ef -> 5d14b151f
- follow-up 8664b92b0 (FieldShape::delete deref; Lua keyword field keys)

## Integration checklist for the parent
- cargo test -p azul-doc field_accessor_tests (5 modules: lang_ruby, lang_lua, lang_php, lang_php_ext, lang_perl)
- regenerate target/codegen; ruby/lua/php/perl hello-worlds; php ext needs the php-extension build
  (php_api.rs is compiled by nothing else).

## Open questions
- field_access.rs may duplicate helpers other groups wrote (unify at integration).
- Perl record layouts are wrong for unions (tag + 256 bytes) and flatten nested structs; the new
  accessors bypass them (C offsets), but by-value record passing still relies on records >= C size.
- examples/perl/hello-world.pl unchanged (raw Azul::FFI records, probe trick).

# AUTOFIX6 progress (wave 6, branch wt/autofix6 from 25d78e309)

Brief: scripts/waves/wave6/AUTOFIX6.md. Files: doc/src/autofix, doc/src/patch (+ minimal wiring in doc/src/main.rs,
noted in the report). Never compile; the parent runs `cargo test -p azul-doc --bin azul-doc -- autofix::` (and `patch::`).

## Plan (one RED test commit, then GREEN, per item)
1. Regression test: a removal targets the module the class is in (178af99a9) - patch_format.rs tests.
2. Gap 1: raw `str` / `&str` / `Optionstr` / `Option<&str>` in a function signature = critical FFI error
   (new check over api.json signatures in mod.rs; any type api.json does not define is UndefinedTypeReference);
   `autofix add` writes `&str` returns as `String` (`azul_css::AzString::from(..)`), `Option<&str>` /
   `Option<String>` as `OptionString` (`.map(|s| azul_css::AzString::from(s)).into()`).
3. Gap 3: std `String` args: MethodArg keeps the type as written (`source_ty`); add converts with
   `{}.into_library_owned_string()`; the signature-drift check reports an existing entry passing it unconverted.
4. Gap 5: ONE wildcard rule (`Type.*`): only methods whose whole signature crosses the FFI as written (no borrowed
   return; every arg / return in its api.json spelling is a scalar, an api.json type or a C-repr workspace type).
5. Gap 2: api.json functions whose Rust method is gone: found by the scan (fn_body `object.m(` / `<path>::m(`, the
   type has no method `m`), reported, removal patches written.
6. Gap 6: no `set_repr` to none (a none means "leave it" -> looped); an unreachable api type whose source has no
   repr is removed; FFI warnings of types removed this round are dropped.
7. Gap 4: pending patches: `autofix add` sees api.json minus the pending removals and supersedes the pending
   removal of the entries it re-adds; patch folders apply in file-name order.
8. `autofix remove module.Type` (whole class): one spec parser for remove and difficult remove.
9. Report scripts/AUTOFIX6_2026_10_03.md.

## Decisions
- Gap 5 rule = the FFI-form rule (no annotation needed; Rust-only helpers that DO have an FFI form stay `pub(crate)`
  per the house rules, or are left out by not using `*`).
- Gap 3 = convert (not only flag): `text.into_library_owned_string()` in the fn_body; `&String` -> `&text.into_..()`,
  `&AzString` -> `&text`.
- Gap 4: an add supersedes a pending function removal of the same entry (drops the name from the pending remove
  patch for the map the add writes into), so the outcome does not depend on the apply order; a pending removal of
  the WHOLE class stops the add with a message (apply first).

## DONE

## IN PROGRESS

## NEXT
- item 1

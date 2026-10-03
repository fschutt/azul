# TOOLS7 - autofix / codegen gaps (wave 7)
Owns: doc/src/autofix/*, doc/src/codegen/*, css/src/macros.rs (the macros only - leave the 77 hand imports in other
files alone, they keep compiling). Read first: scripts/AUTOFIX6_2026_10_03.md ("Seen broken", "Left").
Test commands you write for the parent: `cargo test -p azul-doc --bin azul-doc -- autofix::` (and codegen tests).

0. (FOUND AT THE INTEGRATION, top priority) `autofix add T.m` for a type NOT yet in api.json writes a patch that
   adds the type WITH that one method; several such patches for the same new type each replace the class, so only
   one method per new type survived (2026-10-03: DateRepeatPicker kept `dom`, DateRepeatRule `from_rrule`,
   TextRasterStyle `with_line_height` - every patch reported "Successfully applied"). Re-adding after the type
   existed merged correctly. Fix: a class patch for an existing class merges its functions / constructors (or the
   add writes one patch per type with every requested method, or a multi-method `autofix add T.a T.b`), and the
   apply reports a dropped function. RED test: two add patches for one new type keep both methods.
1. `autofix add` cannot make a static function whose body calls a FREE function: at the wave-6 integration
   Xml.encode_text / encode_attribute (azul_core::xml::html::encode_text), RawImage.from_text / draw_text
   (azul_layout::cpurender::text_image / draw_text) and TextInput.set_text_in (takes &mut CallbackInfo) needed a
   hand-written patch file. Add a form, e.g. `autofix add Class.name --fn <path::to::free_fn>`, that writes the
   entry (args from the free fn's signature, self passed in the codegen's form - see 2, &mut CallbackInfo through
   the `{ let mut info = info; ...(&mut info, ..) }` pattern of ProgressBar.update_progress).
2. The codegen (doc/src/codegen/v2/transmute_helpers.rs) rewrites `object.` and `(<lowercase class>)` /
   `(<lowercase class>,` in a fn_body but NOT a bare `object` argument (a body `draw_text(object, ...)` failed to
   compile); `object` cannot be rewritten blindly (GlContextPtr has real args named `object`). Make the add tool
   emit the supported form and the scan flag a body that uses `object` as a bare argument when the function has a
   self arg (a critical error, RED test).
3. `autofix add` picks a new type's module by its own rule (TextRasterStyle -> css; it lives in
   azul_layout::cpurender, MEDIA6 wanted `image`): derive the module from where the type lives / what uses it, or
   accept `--module`. Also: a type defined in a PRIVATE module re-exported by its parent (cpurender::text_raster)
   got its private path in api.json - the dylib did not compile until the module was made pub: the index must
   prefer the public re-export path (or the scan must flag a private path).
4. AUTOFIX6's list: add BY NAME of a method returning &T writes it by value with no .clone(); Option<T> arguments
   are not converted; &[T] arguments map to {T}VecRef (mostly missing; the convention is XxxVecSlice); the
   .as_str() / .as_slice() suffix splice hits an argument whose name ends with another's; ClassPatch::is_empty
   ignores the remove_* lists; a pending whole-class removal followed by an add of the same class in one round.
5. css/src/macros.rs: impl_option! / impl_result! call impl_option_inner! / impl_result_inner! unqualified - use
   `$crate::` (also qualify RefAny in impl_widget_callback!); module_map.rs writes the "is a Vec type" rule 3x and
   only one includes VecSlice -> one rule (14 *VecSlice types sit in the wrong api.json module; list the moves the
   next scan will make).

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/TOOLS7.PROGRESS.md exact. Finish with the report
scripts/TOOLS7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.

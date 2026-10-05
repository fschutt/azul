# FIELDACCESS_JVM - field accessors for Java + Kotlin wrapper classes

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Base: bc606e468 (worktree fast-forwarded from master to the PR branch tip)

## DONE
- ac731a42c RED Java: lang_java/wrappers.rs tests (getTitle/setTitle, get/setWindowState, bool,
  editWindowState/editSize, TextInputState.setText)

- 735cb6068 Java fix: lang_java/wrappers.rs `FieldShape` / `field_shape` / `emit_field_accessors`
  (get/set/edit per public field) + `__isMovable()` guard on every wrapper

- 2a0bed923 RED Kotlin: lang_kotlin/wrappers.rs tests (title / windowState / sizeToContent properties,
  editWindowState / editSize, TextInputState.setText fallback fun)

- Kotlin fix: lang_kotlin/wrappers.rs `emit_kt_field_accessors` (var/val properties, setX fallback,
  editX { } views; reuses lang_java's FieldShape) + `__isMovable()` on every wrapper

## IN PROGRESS
- (none)

## NEXT
- parent: cargo test -p azul-doc lang_java::wrappers::tests lang_kotlin::wrappers::tests, regenerate
  target/codegen, compile Java + Kotlin (javac / kotlinc) - never compiled here

## Open questions
- Getters return copies (contract item 1), so `opts.getWindowState().setTitle(..)` /
  Kotlin `opts.windowState.title = ..` still only change a copy; the safe nested write is
  `editWindowState(...)` (in-place view) or read-modify-write + set. Documented on every getter.
- No accessors on String / Vec / RefAny / Boxed / callback-wrapper classes (their fields are
  internals: setting a Vec's len would corrupt it).
- Raw JNA value fields (no wrapper class, e.g. AzOptionString) are MOVED in by the setter: the
  caller's Az<T> must not be passed anywhere else afterwards (documented).

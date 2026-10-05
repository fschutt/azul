# FIELDACCESS_JVM - field accessors for Java + Kotlin wrapper classes

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Base: bc606e468 (worktree fast-forwarded from master to the PR branch tip)

## DONE
- ac731a42c RED Java: lang_java/wrappers.rs tests (getTitle/setTitle, get/setWindowState, bool,
  editWindowState/editSize, TextInputState.setText)

- Java fix: lang_java/wrappers.rs `FieldShape` / `field_shape` / `emit_field_accessors`
  (get/set/edit per public field) + `__isMovable()` guard on every wrapper

## IN PROGRESS
- Kotlin RED test + fix (lang_kotlin/wrappers.rs)

## NEXT
- final report

## Open questions

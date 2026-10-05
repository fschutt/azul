# Field-access wave: Python, Node, D, Swift

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. No compiling in this worktree.

## DONE
- Python: RED tests (lang_python.rs `field_access_tests`) + fix.
  Design: struct-typed field getters return a deep copy LINKED to the parent
  (`__azul_parent__` = (parent, field) in the instance `__dict__`; every pyclass gets `dict`).
  Every field setter and every `&mut self` method writes the receiver back through the link
  (`parent.<field> = self`), which recurses up the chain. `&mut self` methods now take
  `mut __slf: PyRefMut<'_, Self>` and return `PyResult<R>`.
  Unverified (no compile): pyo3 0.27 accepts `slf: &Bound<'_, Self>` on #[getter]/#[setter]
  and `pyclass(dict)` under abi3-py310.

- Node: RED tests (lang_node/wrappers.rs `field_access_tests`) + fix.
  `_setField` (used by `with(opts)` and the new setters) releases the old value via
  `lib[<type>_delete]` (type from the generated `_FIELDS` table), then moves the new one in
  (`_moveArg` + `_consume`). New `get`/`set` accessors per field on every Regular struct class;
  struct/union fields read as `_fieldView` views (live `_ptr` getter into the parent), so
  `opts.windowState.title = 'x'` works. A view moved into a by-value parameter or consumed
  receiver hands over `_cloneRaw` (deep copy); POD views copy; no-clone heap views throw.
  Behaviour change: a JS string assigned to a non-AzString field now throws (used to store
  AzString bytes into e.g. an OptionString).

- D: RED test (lang_d/wrappers.rs `field_access_tests`) + fix. `emit_accessor` no longer
  returns early when the getter name is taken by a method; the setter overload is still
  emitted (`void text(<restriction> v)` next to the `string text()` method from get_text).
  The title/window_state/checked cases already met the contract (regression asserts added).

- Swift: RED test (lang_swift/wrappers.rs `field_access_tests`) + fix. When a method owns the
  property name (get_text -> read-only `var text: String`), the field gets
  `public func setText(_ newValue: [UInt32])` (old Vec released with `_delete`, new value moved
  in); plain structs get `public mutating func setX(_:)`. Setter body construction no longer
  unwraps `c_type` (panic-free).

## IN PROGRESS
- (none)

## NEXT
- Parent: run the four `field_access_tests` modules + bug_classes, regenerate target/codegen,
  build python-extension (pyo3 receivers / `dict`), run node + swift + d smoke tests.

## Open questions
- (none)

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

## IN PROGRESS
- D.

## NEXT
- Swift.

## Open questions
- (none)

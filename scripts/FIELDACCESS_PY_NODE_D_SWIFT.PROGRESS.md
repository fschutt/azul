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

## IN PROGRESS
- Node.

## NEXT
- D, Swift.

## Open questions
- (none)

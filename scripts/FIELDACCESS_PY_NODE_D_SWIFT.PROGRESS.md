# Field-access wave: Python, Node, D, Swift

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. No compiling in this worktree.

## DONE
- (none yet)

## IN PROGRESS
- Python: RED tests (lang_python.rs `field_access_tests`).
  Design: struct-typed field getters return a deep copy LINKED to the parent
  (`__azul_parent__` = (parent, field) in the instance `__dict__`; every pyclass gets `dict`).
  Every field setter and every `&mut self` method writes the receiver back through the link
  (`parent.<field> = self`), which recurses up the chain.

## NEXT
- Python fix, Node, D, Swift.

## Open questions
- (none)

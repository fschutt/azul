# Azul — Algol68

⊘ **Toolchain dialect incompat.** `a68g` (Algol 68 Genie) is
installed on macOS-aarch64 but rejects the codegen's
`PROC ... = (...) RESULT: ALIEN "<symbol>" ! "azul";` foreign-
function syntax — that's a different a68 dialect than what a68g
implements.

See `memory/codegen_rehab_status.md`.

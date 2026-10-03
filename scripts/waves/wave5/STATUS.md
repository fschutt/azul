# Wave 5 - status (2026-10-03)

Base 2e92c759b. All eight agents finished; all eight branches merged into fix/input-bugs-2026-09-19 (merge commits,
never rebased) and PUSHED: remote = aa59b2d84 (2026-10-03 ~01:40).

| Task | Branch | Report | Merged | Notes at integration |
|---|---|---|---|---|
| HYGIENE | wt/hygiene | scripts/HYGIENE_2026_10_02.md | yes | 107 files; decl.rs = the one theme-helper module |
| TEXTENG | wt/texteng | scripts/TEXTENG_2026_10_02.md | yes | StyleLineHeight enum; strut from the real font |
| MAILHTML | wt/mailhtml | scripts/MAILHTML_2026_10_02.md | 2d4492699 | fc.rs strut conflict: TEXTENG's code + MAILHTML's half line gap |
| PIM | wt/pim | scripts/PIM_2026_10_02.md | 9c51afcb5 | azul-pim crate, one task store |
| APIEXPORT | wt/apiexport | scripts/APIEXPORT_2026_10_02.md | 9a8c9e4c3 | timeline.rs: both private helpers deleted |
| BLOCKS | wt/blocks | scripts/BLOCKS_2026_10_02.md | 4e671ab6a | ModuleSwitcher + its theme sections deleted |
| RTE | wt/rte | scripts/RTE_2026_10_02.md | 72712fbad | AzMail editor.rs deleted; theme tails joined |
| TABLES | wt/tables | scripts/TABLES_2026_10_02.md | yes | only all.rs both-sides |

After the merges: 47968940f (2 compile errors), autofix tool fixes 178af99a9 (a removal targets the class's real
module) and the new->create fix (a renamed `new` yields to a real `create`), api.json rounds 35205ac3f, 6eb339caa,
59647294f, b4608fd31 (all tool-generated), codegen goldens 0046b04c3 (StyleLineHeight enum), apps aa59b2d84
(`.into()` on Vec args: E0283). The dylib and all 20 apps BUILD.

NOT VERIFIED (the run died at its start in the 2026-10-02 22:41 power loss; pushed on the user's request without it):
the suites (scripts/waves/tools/suites.sh), the calculator E2E, the AzMail send test, and the mail corpus re-measure
(baseline 625 mismatched boxes at 2e92c759b). Run them before (or alongside) the wave-6 integration.

Autofix gaps found during the integration (ledger; AUTOFIX6 in wave 6): raw `str` / `Option<&str>` returns not
flagged; api.json functions whose Rust method is gone not detected; a std `String` arg gets no conversion; `add`
skipped while a `remove` of the entry is pending; `Type.*` exports every public helper; an unreachable struct with
repr none in source loops on a set_repr patch instead of being removed.

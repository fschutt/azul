<!-- The common brief of the three read-only DEDUP review agents (EDITORS, OFFICE, WIDGETS_API), 2026-10-02.
     Their reports: scripts/DEDUP_{EDITORS,OFFICE,WIDGETS_API}_2026_10_02.md. -->
# DEDUP review - common brief (azul, PR #476, branch fix/input-bugs-2026-09-19, repo /Users/fschutt/Development/azul)

READ-ONLY. Do NOT run cargo / rustc / rust-analyzer / any build, do NOT edit, stage, commit, stash or checkout anything in the
repository (another session shares this checkout and has uncommitted files). The ONLY file you write is your report
(scratchpad DEDUP_<AREA>_2026_10_02.md). Write it INCREMENTALLY (append each confirmed finding as you go) so an outage loses
nothing; if the file already exists you are resuming - read it and continue where it stops. Do not spawn subagents.

Range under review: `git log 112cf8342..HEAD` (876 unpushed commits from one night: 20 example apps in examples/azul-*,
~46k lines of new widgets in layout/src/widgets, dll additions, api.json additions). Existing code OUTSIDE the range
counts as the target of a duplicate ("app X re-implements widget Y that already exists").

How the pieces fit: apps in examples/azul-* link the azul dylib and use ONLY the generated `azul` crate API (api.json ->
codegen). Shared, reusable UI belongs in layout/src/widgets/ (the "azul widgets"), exported through api.json (added only
via the `azul-doc autofix` tool - just NAME what should be exported, never write api.json). Apps cannot reach layout
internals, which is a common reason they copy code. Per-app agent reports are in scripts/*_2026_09_30.md and
scripts/*_2026_10_01.md - several already list duplicated helpers; collect every such item into your report, verified.

Find, in your area:
1. DUPLICATION: the same or near-same logic in 2+ places (apps vs apps, apps vs widgets, widgets vs widgets) - both/all
   locations as file:line ranges, how close they are, a quick diff to confirm before recording it.
2. REUSE CANDIDATES: app code that should become an azul widget or helper usable by other apps (e.g. ONE rich-text
   editor shared by AzMail compose / AzNotes / AzWriter / AzTasks notes) - name, target file, API surface, adopters.
3. API.JSON EXPOSURE: things apps re-implement or work around because they are not exported.
4. SEMANTICS / PLACEMENT: types or functions in the wrong module or file, misleading names, one concept with two names.
5. Anything else worth a "next wave" item (bugs, dead code, TODOs, loosened tests) - short, with file:line.

Areas: EDITORS (Mail, Notes, Writer, Tasks, Review, Contacts, Calendar), OFFICE (Sheets, Show, Photo, VideoCut, Paint,
Drive, Meet, Maps, Calculator, Setup, Builder, Shells, Widgets, Storage, AppKit), WIDGETS_API (layout/src/widgets,
core/css/dll, api.json, doc/src/autofix). Report format: header (area, commit), numbered findings grouped by the 5 kinds
(title, evidence, confirmation, proposal, effort S/M/L, risk, benefiting apps), a PRIORITIZED next-wave list, unverified items.

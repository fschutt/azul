# Resume on Monday (written 2026-10-03 evening)

## State at the pause
- Branch `fix/input-bugs-2026-09-19` (PR #476): waves 6, 7 and 8 integrated, COMPILE, pushed (last code push
  0de2a2529; docs after it local + pushed as they came). target/release + target/azul-lib = that build.
- Mail corpus: 20 mails, 944 boxes, 0 mismatched against Chrome (target/refci/mail-wave8-final).
- AzWidgets knob tick: 20-21 ms unprofiled (was 157-309 ms profiled) - ANIMFRAME8 / A11YPATCH8 go further.
- NOT run on waves 6-8: the test suites (scripts/waves/tools/suites.sh), the E2E scripts, WAYLAND8's Linux checks, the WPT
  bless (AZ_WPT_BLESS=1). Do these before the release.
- ../azul-apps: 5 planning commits (973d7d4..985080c: AzPdfMaker, AzSvg, monetization.md) are LOCAL - not pushed
  (the user has not asked to push that repo).

## Agents running at the pause (16) - all commit after every unit and keep a PROGRESS file
Wave-8 follow-ups (scripts/waves/wave8/STATUS.md, base 5745afee6): ANIMFRAME8 aab288d543431cefd, A11YPATCH8
aa100334a0a477111, SYSUI8 a8592cc8171aa7d82.
Wave 9 (scripts/waves/wave9/STATUS.md + PLAN.md, base e537ddbe2): WIDGETS9A a18aaef546b141fde, WIDGETS9B
acb99bc6296facd68, MAIL9 a4d36f4ef063912bd (re-ordered: direct delivery + client DKIM first), PDF9 a90ecd8756e4fac61,
READER9 ad5e2a07b9e50eb78, TERM9 aeeb57dd4eb500ff0, CODE9 aec89a04e113059d5, MEDIA9 a78d3666fb280f6a4, CLOCK9
a77d97682c470b269, KEYS9 ad62e1cc06f76a821, MONITOR9 a21e54e23b6931c52, NEWS9 af8b08e8d81607db5, ERP9 a7949c63ed0ed81a4.
Worktree of each: /Users/fschutt/Development/azul/.claude/worktrees/agent-<id>; branch wt/<task lowercased>; progress
file scripts/<TASK>.PROGRESS.md inside it; report scripts/<TASK>_<date>.md when done.

## Finished since this guide was written (on their branches, NOT merged)
ANIMFRAME8 (knob on transform; one tick decision; expected ~0.5-1.5 ms/tick), A11YPATCH8 (incremental a11y; a telemetry
bug that hid spans fixed), SYSUI8 (SF at its optical size; after the build run scripts/sysui8_look.py record + compare
and fix the widget CSS it flags - macOS UI text gets 10-14 % wider at 11-14 px). Merge these three first.

## How to resume
1. Check: `uptime`, `pmset -g batt`, `df -h /` (>= 20 GB free: `python3 scripts/waves/tools/clean_stale_deps.py`), network.
2. For each task, see where it is: `git log --oneline -3 wt/<task>` and whether `scripts/<TASK>_*.md` (the report) exists
   on its branch (`git show wt/<task>:scripts/` lists it).
3. Not finished:
   - In the SAME Claude Code conversation: SendMessage the agent id: "Resume: read scripts/<TASK>.PROGRESS.md, check
     git status / log -3, continue from NEXT; commit after every unit."
   - In a NEW conversation (ids are gone): start a new agent per task WITHOUT a new worktree (no isolation flag): "You
     continue wave-<n> task <TASK>; your worktree already exists: <path> (branch wt/<task> checked out there, with the
     previous agent's commits). Work ONLY in that worktree with git -C and absolute paths. Read scripts/waves/
     house_rules.md, scripts/waves/wave<n>/PLAN.md (your section), then scripts/<TASK>.PROGRESS.md and continue from
     NEXT to the report. Never compile; commit after every unit; no subagents." (This is how MAIL6 / MEETDRIVE6 were
     continued on 2026-10-03.)
4. Finished: integrate (the next section).

## Integration recipe (as done for waves 6-8)
- Take the other session's uncommitted all.rs lines out first (`git apply -R ~/Development/azul-work/
  other_session_all_rs.patch`), put them back after (`git apply ...`); never stage layout/src/solver3/page_breaks.rs,
  layout/tests/a_padded_table_cell_stays_in_its_row.rs, run_autofix.sh.
- Merge each branch: `bash scripts/waves/tools/merge_one.sh <task> "merge(waveN): <TASK> - <summary>"` (append conflicts
  resolve themselves; theme files flat.rs / flora.rs: keep both, check for a shared closing brace).
- Order: the follow-ups (ANIMFRAME8, A11YPATCH8, SYSUI8) first, then WIDGETS9A / 9B (the apps' TODO(WIDGETS9x) markers
  can then be resolved), then the apps.
- api.json: `./target/release/azul-doc autofix` until "Generated 0 patches" (rebuild azul-doc first if doc/ changed);
  check any remove_fns / set_external patch before applying (a non-pub type can hijack a name - XML8's `Content`);
  new functions: ONE `autofix add T.a T.b U.c ...` run (zsh: split the list with ${=S}); verify every requested
  function landed; module placement exceptions in doc/src/autofix/module_map.rs; then normalize, `codegen all`,
  `python3 css/tools/gen_codegen_lowering.py`.
- Build: `cargo build --release -j 4 -p azul-dll --features build-dll`, copy target/release/libazul.dylib to
  target/azul-lib/, then `AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -j 4 --keep-going -p <every
  examples/azul-* crate>` (the list: scripts in ~/Development/azul-work/logs/app_pkgs.txt, regenerate - new apps).
  (-j 2 on battery.) The build script now re-copies libazul into the apps' OUT_DIR when it changes (7be0bbee0).
- Measure with env vars BEFORE the runner: `AZ_BACKEND=headless AZ_E2E=... scripts/waves/tools/run_capped.sh ... --
  target/release/<App>` (SIP strips DYLD_* through /usr/bin/env). Mail corpus: `python3 scripts/refci/mail_boxes.py`.
- Push: `git fetch`, check fast-forward, `git push origin HEAD:fix/input-bugs-2026-09-19` (never force).

## Open with the user
- Push ../azul-apps (5 local planning commits)?
- The monetization open items (../azul-apps/planning/monetization.md §4): S3 server software, billing interval and
  provider, a free tier, when the mail relay fallback is offered by default.

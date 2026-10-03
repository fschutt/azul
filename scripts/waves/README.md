# Agent waves for PR #476 - how the work is organised, and how to resume it

Everything a wave needs lives HERE, in git (committed and pushed with the PR), so a reboot, a power loss or a lost
machine cannot take it: the 2026-10-02 22:41 power loss (the battery ran empty under -j4 builds, even on AC) wiped
/tmp and with it the earlier scratch copies of these briefs and tools.

    scripts/waves/
      README.md            this file
      house_rules.md       the rules every agent follows (current wave)
      tools/               run_capped.sh (memory cap + machine-wide lock), suites.sh, merge_one.sh, keep_both.py,
                           clean_stale_deps.py, memprobe.py
      wave5/               briefs as given (house_rules.md, the 8 tasks, DEDUP_review.md, PDFFIX.md) + STATUS.md
      wave6/               PLAN.md, the 11 task briefs, STATUS.md (the resume table)
    scripts/pgo/           the PGO + order-file chain for libazul.dylib (README.md has the state)
    scripts/<TASK>_<date>.md / <TASK>.PROGRESS.md   each agent's report and checkpoint (in its branch until merged)

Durable scratch (logs, the other session's uncommitted diffs, locks) is `$AZ_WORK` = `~/Development/azul-work/`,
never /tmp.

## A wave
1. The parent writes `waveN/PLAN.md` + one brief per task (who owns which files, the contracts between tasks), commits.
2. Agents start in their own git worktree (`.claude/worktrees/agent-<id>`), branch `wt/<task>` from the base commit,
   NEVER compile (the parent compiles once - an 8 GB Mac), commit after every unit, keep `scripts/<TASK>.PROGRESS.md`.
3. The parent merges each finished branch (`tools/merge_one.sh`, never rebase), runs autofix to convergence (0 patches,
   0 critical FFI errors; the reports list the api.json entries), codegen, builds the dylib and all apps, runs
   `tools/suites.sh`, the E2E scripts and the mail corpus, then pushes (fast-forward only, never force).

## Resume after an outage (power, network, token limit)
1. `uptime`, `pmset -g batt` (plug in; at -j4 the Mac drains even on AC - use `AZ_JOBS=2` on battery), `df -h /`
   (keep >= 20 GB free: `tools/clean_stale_deps.py`), network.
2. `waveN/STATUS.md` lists every agent id, branch and worktree. A stopped agent resumes from its transcript with
   its context intact: send it a message ("continue from your progress file"); it reads its PROGRESS file first.
   An agent whose transcript is gone: start a new one on the same brief, telling it to continue the existing branch
   from its PROGRESS file.
3. The parent's own state: the ledger (`~/.claude/projects/-Users-fschutt-Development-azul/memory/pr476_ledger_*.md`)
   and `git log`. Before any merge, take the other session's uncommitted `layout/tests/all.rs` lines out
   (`git apply -R $AZ_WORK/other_session_all_rs.patch`) and put them back after; never stage
   `layout/src/solver3/page_breaks.rs`, `layout/tests/a_padded_table_cell_stays_in_its_row.rs`, `run_autofix.sh`.

# Wave 6 - status / resume table

Base: 25d78e309 (launched 2026-10-03 ~02:30). Resume a stopped agent: SendMessage to its id ("continue from your progress file"). Briefs: this folder. House rules: ../house_rules.md.

| Task | Agent id | Branch | Worktree | State |
|---|---|---|---|---|
| INFRA6 | a2920336ddc00cc70 | wt/infra6 | .claude/worktrees/agent-a2920336ddc00cc70 | DONE (report scripts/INFRA6_2026_10_03.md; api: remove FullWindowState.close_callback; Windows/Linux close paths compile first in CI; notes forwarded to MEETDRIVE6, SHEETSHOW6, PIM6, HEADLESS6, MAIL6, WRITER6) |
| MAIL6 | ac5bc58e39fd4ed88 (was a516d913c267d173a) | wt/mail6 | .claude/worktrees/agent-a516d913c267d173a | DONE (report scripts/MAIL6_2026_10_03.md; api: Xml.encode_text / encode_attribute REQUIRED for AzMail to build; E2E sample phase 3 pass / 8 fail on the prebuilt binary - all 8 need this branch; wizard page-2 engine bug RED cc7040ae5 has no owner) |
| MEETDRIVE6 | a2b7fd9b18e455a3c (was ad47cdc4a01c95c37, stopped by the restart) | wt/meetdrive6 | .claude/worktrees/agent-ad47cdc4a01c95c37 | running (continued) |
| SHEETSHOW6 | a763890131c8611a5 | wt/sheetshow6 | .claude/worktrees/agent-a763890131c8611a5 | running |
| MEDIA6 | a0593b6c7ec695651 | wt/media6 | .claude/worktrees/agent-a0593b6c7ec695651 | DONE a831cde97 (report scripts/MEDIA6_2026_10_03.md; engine fix: in-place image swap never reached the solver cache; api: TextRasterStyle, RawImage.from_text/draw_text/fit_within, CallbackInfo.text_image; needs AzString::from_const_str in the generated crate = SMALL6's codegen change; AzReview ui.rs:547 MouseOver->MouseMove left for integration) |
| WRITER6 | ac405e136d75eb9b6 | wt/writer6 | .claude/worktrees/agent-ac405e136d75eb9b6 | running |
| PIM6 | aa92e87efbe651521 | wt/pim6 | .claude/worktrees/agent-aa92e87efbe651521 | running |
| SMALL6 | a2ae56544f3956644 | wt/small6 | .claude/worktrees/agent-a2ae56544f3956644 | DONE 7fc3ac070 (report scripts/SMALL6_2026_10_03.md; api: ShellThemeScope.body; Cargo.lock: appkit/serde_json deps; engine bugs forwarded to WRITER6 / MAILENG6 / HEADLESS6) |
| MAILENG6 | a945948afd8d32441 | wt/maileng6 | .claude/worktrees/agent-a945948afd8d32441 | DONE (report scripts/MAILENG6_2026_10_03.md; items 1-7 + overflow; found-not-fixed: block inside inline dropped, run shaped before its font loads stays invisible, bolder/lighter, the same font-index off-by-one in dll shell2/common/layout.rs:1555) |
| AUTOFIX6 | aa1be8b2b4d1e30ea | wt/autofix6 | .claude/worktrees/agent-aa1be8b2b4d1e30ea | DONE 7cba95017 (report scripts/AUTOFIX6_2026_10_03.md; at integration: review the scan's first "gone functions" list before applying its remove_fns patches) |
| HEADLESS6 | ae490319c7de2d431 | wt/headless6 | .claude/worktrees/agent-ae490319c7de2d431 | DONE (report scripts/HEADLESS6_2026_10_03.md; 22 corpus failures in 7 root causes, 62/62 expected; exit crash; menus as child windows + list_windows; wait_settled op; api: LayoutCallbackInfo.get_window_id; top compile risk: remap_node_ids tail in layout/src/window.rs) |

2026-10-03 ~05:00: power loss + Claude Code restart; all agents had committed everything. Seven resumed by message; MAIL6 and MEETDRIVE6 continued by new agents in the same worktrees.

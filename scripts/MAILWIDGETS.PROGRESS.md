# MAILWIDGETS progress

Branch `wt/mail-widgets` from `wt/fb2-azdrive-explorer` (`82f1d74c9`). Report:
`scripts/MAILWIDGETS_2026_09_30.md`. Task: the reusable widgets for the Outlook-2010-style AzMail
rewrite (MessageList, ReadingPane + InfoBar, ToDoBar, ModuleSwitcher, StatusBar sync, WizardLayout),
flat + flora, light + dark, keyboard, a11y, tests, showcase cards; engine gaps found by reading fixed
RED-first (date_picker inline mode + today, list_view lazy-load hook never wired).

## DONE
- (none yet)

## IN PROGRESS
- RED commit A: date_picker inline + today tests, list_view lazy-load wiring test, statusbar sync
  tests, info_bar (stub dom).

## NEXT
1. GREEN A: date_picker `inline` / `today`, list_view `on_lazy_load_scroll` wired through a shared
   scroll-window hook, statusbar `sync` cluster, info_bar build + flat/flora looks.
2. RED C / GREEN D: message_list, reading_pane, todo_bar, module_switcher, wizard_layout.
3. Manifest entries in `widgets/mod.rs`; showcase cards in `examples/azul-widgets/src/mail.rs`.
4. Report.

## Open questions
- No sibling worktree has `layout/src/widgets/shells/` or a `NavigationPane`: the ModuleSwitcher is
  built standalone here (said so in the report).
- The wheel-ownership lint (`widgets/mod.rs::wheel_ownership`) forbids `Scroll` hooks on manifest
  widgets; the virtualised list reports its window on `ScrollEnd` (a settled gesture) instead.

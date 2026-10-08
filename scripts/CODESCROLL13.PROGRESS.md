# CODESCROLL13 - AzCode lags when scrolling the code view (AzWidgets is smooth)

Worktree branch: worktree-agent-ac7f673a4a5f9d95c (fast-forwarded to cbd0ae0a8).

## DONE
- (nothing committed yet)

## IN PROGRESS
- Measuring: waiting for the lead's rebuilt AzCode / AzWidgets (newer than libazul.dylib 04:16).
- Reading the path: CodeView wheel -> on_wheel -> CodeViewEvent -> AzCode on_code_event -> Update::RefreshDom
  -> whole-window layout callback + reconcile + restyle + relayout + display list.

## NEXT
- Measure AzCode (src/main.rs, huge.rs) and AzWidgets with AZ_PROFILE=cpu + `sample`.

## Open questions

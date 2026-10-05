# Field-access wave - classic group (Ada, Fortran, FreeBASIC, VB6, Pascal)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Branch: worktree-agent-ad852fad2caec2a1b (based on bc606e468). Nothing compiled (house rule).

Shared: `doc/src/codegen/v2/field_access_classic.rs` - field classification (Prim / UnitEnum /
Str / Value{delete, clone}), string copy fn + layout, all derived from the IR.

## DONE
- Ada: RED 99132a402, fix = the commit after it. Adjust deep-copies via Az_X_Deep_Copy;
  no-clone wrappers are Limited_Controlled; nested `package Azul.Fields` with Get_/Set_ per field.
  User: `WS := Get_Window_State (Opts); Set_Title (WS, "Hi"); Sz := Get_Size (WS);
  Sz.Dimensions.Width := 800.0; Set_Size (WS, Sz); Set_Window_State (Opts, WS);`

## IN PROGRESS
- Fortran

## NEXT
- FreeBASIC, VB6, Pascal.

## Open questions

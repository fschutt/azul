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

- Fortran: RED d6ab6bb35, fix = the commit after it. `azul_string_value` no longer frees (new
  consuming `azul_string_take` for String results); `azul_take_<x>` exists for every class, has no
  intent and clears `owned` on move (error stop when borrowed + no clone); owned wrapper args lost
  `intent(in)`; type-bound `get_<f>`/`set_<f>` on every wrapper (method binding wins).
  User: `ws = opts%get_window_state(); call ws%set_title('Hi'); sz = ws%get_size();
  call sz%set_dimensions(d); call ws%set_size(sz); call opts%set_window_state(ws)`

## IN PROGRESS
- FreeBASIC

## NEXT
- VB6, Pascal.

## Open questions

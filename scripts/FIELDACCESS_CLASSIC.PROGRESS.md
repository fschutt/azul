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

- FreeBASIC: RED db8dbd682, fix = the commit after it. Methods/constructors call `func.c_name`;
  receiver = first arg of Method/MethodMut/DeepCopy (Clone() lost the extra arg; by-value
  receiver passes `this.raw` + disowns); `Type String_`; copy ctor + `Operator Let` (deep copy via
  _clone, move without); `TakeRaw()`; `Property <Field>` get/set per field (method name wins ->
  `<Field>Field`); Types topologically ordered by wrapped-field deps; module-level
  `AzulStringRead` (non-consuming) / `AzulStringNew`.
  User: `Dim ws As Azul.FullWindowState = opts.WindowState : ws.Title = "Hi" :
  Dim sz As AzWindowSize = ws.Size : sz.dimensions.width = 800 : ws.Size = sz : opts.WindowState = ws`

- VB6: RED 73300a19e, fix = the commit after it. Class names that are VB6 keywords / runtime
  globals become `Azul<Name>` (AzulString.cls, AzulApp.cls); receiver = first arg of
  Method/MethodMut/DeepCopy (Clone() lost the extra arg); `Friend Sub MoveRawInto`; field
  properties (Public for scalars/String/classes, Friend for UDT/enum types); Azul.bas gets
  AzulStringRead (UTF-8, non-consuming) / AzulStringNew + kernel32 declares.
  User: `Set ws = opts.WindowState: ws.Title = "Hi": sz = ws.Size: sz.dimensions.width = 800:
  ws.Size = sz: Set opts.WindowState = ws`

- Pascal: RED 077dd56a4, fix = the commit after it. Every T* wrapper class gets
  `property <Field>: T read FieldGet<Field> write FieldSet<Field>` (protected accessors; method
  names win -> `<Field>Field`); props join the member set so method params stay unique.
  User: `WS := Opts.WindowState; WS.Title := 'Hi'; Sz := WS.Size; Sz.dimensions.width := 800;
  WS.Size := Sz; Opts.WindowState := WS { consumes + frees WS }`

## IN PROGRESS
(none)

## NEXT
- parent: compile, regenerate target/codegen, run the RED tests (all five + field_access_classic).

## Open questions
- VB6 stdcall vs cdecl: left as documented (wrappers.rs module doc + functions.rs). Not trivially
  fixable in the generator: needs stdcall entry points in the 32-bit libazul.
- VB6 existing `Public Function` methods returning/taking standard-module UDTs are likely rejected
  by VB6 in a class module (same rule that made the new UDT properties `Friend`). Not changed.

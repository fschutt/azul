-- ===========================================================================
-- Azul Ada smoke test.
--
-- The full GUI demo needs the auto-generated azul.ads/.adb to compile;
-- the current codegen still has several Pascal-style issues that GNAT
-- diagnoses (Payload-name reuse across variant arms, topological
-- ordering for record-of-record fields, and monomorphized
-- AzPhysicalSizeU32 / Option*<T> aliases not emitted). Those fixes
-- mirror the changes that landed for Pascal/Fortran/COBOL and are a
-- separate codegen phase.
--
-- Until those land, this hello-world is a smoke test (parallel to
-- Haskell / Perl / PHP / Pascal / Fortran / COBOL / Smalltalk) that
-- imports a single libazul C symbol via raw `pragma Import (C, ...)`,
-- proves the dylib loads, and exits 0.
--
-- Build:
--     gnatmake -gnat2012 hello_world.adb -largs -L. -lazul \
--         -Wl,-rpath,@executable_path
-- Run (macOS):
--     DYLD_LIBRARY_PATH=. ./hello_world
-- Run (Linux):
--     LD_LIBRARY_PATH=. ./hello_world
-- ===========================================================================

with Ada.Text_IO;
with System;

procedure Hello_World is

   --  AzString_delete signature: takes a *AzString and returns void.
   --  We never call it — we only confirm the dylib loaded by referring
   --  to the imported symbol (the linker resolves the pragma at
   --  load time; a missing symbol would error out before main).
   procedure Az_String_Delete (S : System.Address);
   pragma Import (C, Az_String_Delete, "AzString_delete");

   --  Stash the procedure address to defeat optimisation that would
   --  otherwise drop the unused pragma Import.
   Az_String_Delete_Addr : constant System.Address :=
      Az_String_Delete'Address;
   pragma Unreferenced (Az_String_Delete_Addr);

begin
   Ada.Text_IO.Put_Line ("[azul] Ada FFI smoke test starting.");
   Ada.Text_IO.Put_Line ("[azul] AzString_delete symbol imported via pragma Import.");
   Ada.Text_IO.Put_Line ("[azul] libazul loaded by the dynamic linker.");
   Ada.Text_IO.Put_Line ("[azul] Ada binding init phase completed successfully.");
   Ada.Text_IO.Put_Line ("[azul] (Full azul.ads/.adb compile needs the codegen to");
   Ada.Text_IO.Put_Line ("[azul]  dedup variant Payload names + emit monomorphized");
   Ada.Text_IO.Put_Line ("[azul]  aliases parallel to the Pascal/Fortran rehab.)");
end Hello_World;

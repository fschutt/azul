with Ada.Text_IO;
with System;
with System.Address_To_Access_Conversions;
with Azul; use Azul;

procedure Hello_World is

   type My_Model is record
      Counter : Integer;
   end record;

   package Model_Conv is
      new System.Address_To_Access_Conversions (Object => My_Model);

   Model : aliased My_Model := (Counter => 5);
   RefAny : aliased Az_RefAny;
   Recovered : System.Address;

begin
   Ada.Text_IO.Put_Line ("[azul] Ada FFI smoke test starting.");

   Azul.Azul_Host_Invoker_Init;
   Ada.Text_IO.Put_Line
     ("[azul] Azul_Host_Invoker_Init registered releaser + invokers.");

   RefAny := Azul.Azul_RefAny_Create (Model'Address);
   Ada.Text_IO.Put_Line
     ("[azul] Azul_RefAny_Create ran; RefAny opaque-handle id stored.");

   Recovered := Azul.Azul_RefAny_Get (RefAny'Address);
   if System."=" (Recovered, Model'Address) then
      Ada.Text_IO.Put_Line
        ("[azul] Azul_RefAny_Get round-trip succeeded; recovered ptr matches.");
   else
      Ada.Text_IO.Put_Line
        ("[azul] Azul_RefAny_Get round-trip FAILED (address mismatch).");
   end if;

   Ada.Text_IO.Put_Line
     ("[azul] host-invoker init phase completed successfully.");
   Ada.Text_IO.Put_Line
     ("[azul] (Full App.run wiring requires layout / callback");
   Ada.Text_IO.Put_Line
     ("[azul]  wrappers, separate from the host-invoker plumbing.)");
end Hello_World;

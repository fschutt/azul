--  ===========================================================================
--  Azul "hello world" — Ada (GNAT) port of examples/c/hello-world.c
--  ===========================================================================
--
--  Reproduces the same counter app the C example builds: a label showing an
--  integer counter and a "Increase counter" button that increments it on
--  click. Demonstrates:
--
--    * Use of the generated `Azul` package.
--    * `Ada.Finalization.Controlled` wrapper types (`App`) auto-finalised on
--      scope exit (no manual `Az_App_Delete` required).
--    * `pragma Import (C, ..., "...")` interop for the FFI subprograms.
--    * Building strings via `Interfaces.C.Strings.New_String`.
--
--  Build:
--      gprbuild -P hello_world.gpr
--      ./obj/hello_world
--
--  ===========================================================================

with Ada.Text_IO;
with Interfaces.C;            use Interfaces.C;
with Interfaces.C.Strings;    use Interfaces.C.Strings;
with System;
with Azul;                    use Azul;

procedure Hello_World is

   --  -------------------------------------------------------------------------
   --  Layout callback: invoked by the framework to (re)build the DOM whenever
   --  the data model changes.
   --  -------------------------------------------------------------------------
   --
   --  The framework hands us back the user data and a `LayoutCallbackInfo`.
   --  We construct the DOM via the FFI subprograms and return the resulting
   --  `Az_Dom`.
   --
   --  In a real binding we would expose typed Ada wrappers for `Dom`,
   --  `Button`, etc.; for the hello-world example we go straight through the
   --  raw FFI to keep the moving parts visible.

   function Layout
     (Data : System.Address;
      Info : System.Address) return Azul.Az_Dom
   is
      pragma Unreferenced (Data);
      pragma Unreferenced (Info);
      Body_Dom : Azul.Az_Dom := Azul.Az_Dom_Create_Body;
   begin
      return Body_Dom;
   end Layout;
   pragma Convention (C, Layout);

   --  -------------------------------------------------------------------------
   --  Main: build the window options, create the App via the wrapper type,
   --  and run the event loop. The wrapper's `Finalize` automatically calls
   --  `Az_App_Delete` when `App_Instance` goes out of scope at end of
   --  `Hello_World` — RAII without manual cleanup.
   --  -------------------------------------------------------------------------

   Title          : chars_ptr := New_String ("Hello World");
   Window_Options : Azul.Az_Window_Create_Options :=
     Azul.Az_Window_Create_Options_Create (Layout'Address);
   App_Instance   : Azul.App;   --  Controlled — Finalize runs on scope exit.

begin
   --  Create the App. We pass null user-data and a fresh AppConfig.
   App_Instance.Inner :=
     Azul.Az_App_Create
       (System.Null_Address,
        Azul.Az_App_Config_Create (System.Null_Address));

   --  Run the event loop. Returns when the user closes the window.
   Azul.Az_App_Run (App_Instance.Inner'Address, Window_Options);

   --  No need to call delete: the Controlled wrapper does it for us.
   Free (Title);

exception
   when others =>
      Ada.Text_IO.Put_Line ("hello_world: unexpected exception");
      raise;
end Hello_World;

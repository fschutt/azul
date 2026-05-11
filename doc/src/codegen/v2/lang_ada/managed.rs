//! Ada (GNAT) managed-FFI runtime helpers (host-invoker pattern).
//!
//! Ada exposes C function-pointer types via `pragma Convention (C, …)`
//! on procedure access types. The host-invoker pattern routes user
//! callbacks through pointer-arg invokers; libazul's static thunks
//! handle the by-value plumbing.
//!
//! ## Output surface
//!
//! Spec (`azul.ads`):
//! - access-type declarations for the releaser + per-kind invoker
//!   procedure types, with `pragma Convention (C, …)`
//! - pragma Import for AzApp_setHostHandleReleaser,
//!   AzRefAny_newHostHandle, AzRefAny_getHostHandle, plus per-kind
//!   setter/createFromHostHandle bindings
//! - public surface: `function Azul_RefAny_Create (Value : System.Address)
//!   return Az_RefAny;` and `function Azul_RefAny_Get (RefAny :
//!   System.Address) return System.Address;`
//!
//! Body (`azul.adb`):
//! - module-level dynamic-vector handle table (Vector instances would
//!   require Ada.Containers, so we stick with `array of` + manual resize)
//! - releaser procedure + per-kind invoker stubs
//! - `Azul_RefAny_Create` / `Azul_RefAny_Get` implementations
//! - module initialisation block that registers the releaser + invokers

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// Emit spec (`azul.ads`) declarations. Call from inside the
/// `package Azul is` body, after the regular function imports.
pub fn emit_managed_spec(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Managed-FFI runtime helpers (host-invoker pattern).");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    // Access types for the releaser + per-kind invokers.
    builder.line("type Azul_Releaser_Proc is access procedure (Id : Interfaces.C.unsigned_long_long);");
    builder.line("pragma Convention (C, Azul_Releaser_Proc);");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        // Build the procedure access type.
        let mut params = vec!["Id : Interfaces.C.unsigned_long_long".to_string()];
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("Arg{}", i)
            } else {
                pascalize(&arg.name)
            };
            params.push(format!("{} : System.Address", nm));
        }
        if has_return(cb) {
            params.push("Out_Ptr : System.Address".to_string());
        }
        let access_name = format!("Azul_{}_Invoker_Proc", wrapper);
        builder.line(&format!(
            "type {} is access procedure ({});",
            access_name,
            params.join("; ")
        ));
        builder.line(&format!("pragma Convention (C, {});", access_name));
        builder.blank();
    }

    // FFI imports.
    builder.line("procedure Az_App_Set_Host_Handle_Releaser (Releaser : Azul_Releaser_Proc);");
    builder.line("pragma Import (C, Az_App_Set_Host_Handle_Releaser, \"AzApp_setHostHandleReleaser\");");
    builder.blank();
    builder.line("function Az_RefAny_New_Host_Handle (Id : Interfaces.C.unsigned_long_long) return Az_RefAny;");
    builder.line("pragma Import (C, Az_RefAny_New_Host_Handle, \"AzRefAny_newHostHandle\");");
    builder.blank();
    builder.line("function Az_RefAny_Get_Host_Handle (RefAny : System.Address) return Interfaces.C.unsigned_long_long;");
    builder.line("pragma Import (C, Az_RefAny_Get_Host_Handle, \"AzRefAny_getHostHandle\");");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!(
            "procedure Az_App_Set_{w}_Invoker (Invoker : Azul_{w}_Invoker_Proc);",
            w = wrapper
        ));
        builder.line(&format!(
            "pragma Import (C, Az_App_Set_{w}_Invoker, \"AzApp_set{w}Invoker\");",
            w = wrapper
        ));
        builder.blank();
        builder.line(&format!(
            "function Az_{w}_Create_From_Host_Handle (Id : Interfaces.C.unsigned_long_long) return Az_{w};",
            w = wrapper
        ));
        builder.line(&format!(
            "pragma Import (C, Az_{w}_Create_From_Host_Handle, \"Az{w}_createFromHostHandle\");",
            w = wrapper
        ));
        builder.blank();
    }

    // Public surface.
    builder.line("-- Wrap an arbitrary Ada value in a RefAny. Value is just a System.Address —");
    builder.line("-- typically the address of an Ada record or class. The handle table");
    builder.line("-- holds the address until libazul's destructor fires the releaser.");
    builder.line("function Azul_RefAny_Create (Value : System.Address) return Az_RefAny;");
    builder.blank();
    builder.line("-- Recover the address previously wrapped via Azul_RefAny_Create.");
    builder.line("-- Pass a System.Address to the RefAny (the framework hands callbacks");
    builder.line("-- the RefAny by-pointer).");
    builder.line("function Azul_RefAny_Get (RefAny : System.Address) return System.Address;");
    builder.blank();
    builder.line("-- Idempotent — registers the releaser + per-kind invoker stubs.");
    builder.line("procedure Azul_Host_Invoker_Init;");
    builder.blank();
}

/// Emit body (`azul.adb`) implementations. Call inside the
/// `package body Azul is` block.
pub fn emit_managed_body(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Managed-FFI bodies.");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    builder.line("type Handle_Entry is record");
    builder.line("   Id  : Interfaces.C.unsigned_long_long;");
    builder.line("   Ptr : System.Address;");
    builder.line("end record;");
    builder.blank();
    builder.line("type Handle_Array is array (Positive range <>) of Handle_Entry;");
    builder.line("type Handle_Array_Access is access Handle_Array;");
    builder.blank();
    builder.line("Azul_Handles : Handle_Array_Access := null;");
    builder.line("Azul_Next_Handle_Id : Interfaces.C.unsigned_long_long := 0;");
    builder.blank();

    builder.line("function Azul_Alloc_Handle (Value : System.Address)");
    builder.line("   return Interfaces.C.unsigned_long_long");
    builder.line("is");
    builder.line("   New_Handles : Handle_Array_Access;");
    builder.line("   N : Natural;");
    builder.line("   Id : Interfaces.C.unsigned_long_long;");
    builder.line("begin");
    builder.indent();
    builder.line("Azul_Next_Handle_Id := Azul_Next_Handle_Id + 1;");
    builder.line("Id := Azul_Next_Handle_Id;");
    builder.line("if Azul_Handles = null then");
    builder.line("   Azul_Handles := new Handle_Array (1 .. 1);");
    builder.line("   Azul_Handles (1) := (Id => Id, Ptr => Value);");
    builder.line("else");
    builder.line("   N := Azul_Handles'Length;");
    builder.line("   New_Handles := new Handle_Array (1 .. N + 1);");
    builder.line("   New_Handles (1 .. N) := Azul_Handles.all;");
    builder.line("   New_Handles (N + 1) := (Id => Id, Ptr => Value);");
    builder.line("   Azul_Handles := New_Handles;");
    builder.line("end if;");
    builder.line("return Id;");
    builder.dedent();
    builder.line("end Azul_Alloc_Handle;");
    builder.blank();

    builder.line("function Azul_Lookup_Handle (Id : Interfaces.C.unsigned_long_long)");
    builder.line("   return System.Address");
    builder.line("is");
    builder.line("begin");
    builder.indent();
    builder.line("if Azul_Handles = null then");
    builder.line("   return System.Null_Address;");
    builder.line("end if;");
    builder.line("for I in Azul_Handles'Range loop");
    builder.line("   if Azul_Handles (I).Id = Id then");
    builder.line("      return Azul_Handles (I).Ptr;");
    builder.line("   end if;");
    builder.line("end loop;");
    builder.line("return System.Null_Address;");
    builder.dedent();
    builder.line("end Azul_Lookup_Handle;");
    builder.blank();

    // Releaser procedure. Body removes the matching entry by replacing
    // with the last slot then shrinking the array.
    builder.line("procedure Azul_Releaser_Impl (Id : Interfaces.C.unsigned_long_long);");
    builder.line("pragma Convention (C, Azul_Releaser_Impl);");
    builder.blank();
    builder.line("procedure Azul_Releaser_Impl (Id : Interfaces.C.unsigned_long_long) is");
    builder.line("   New_Handles : Handle_Array_Access;");
    builder.line("   N : Natural;");
    builder.line("begin");
    builder.indent();
    builder.line("if Azul_Handles = null then");
    builder.line("   return;");
    builder.line("end if;");
    builder.line("N := Azul_Handles'Length;");
    builder.line("for I in Azul_Handles'Range loop");
    builder.line("   if Azul_Handles (I).Id = Id then");
    builder.line("      if N = 1 then");
    builder.line("         Azul_Handles := null;");
    builder.line("      else");
    builder.line("         Azul_Handles (I) := Azul_Handles (N);");
    builder.line("         New_Handles := new Handle_Array (1 .. N - 1);");
    builder.line("         New_Handles.all := Azul_Handles (1 .. N - 1);");
    builder.line("         Azul_Handles := New_Handles;");
    builder.line("      end if;");
    builder.line("      return;");
    builder.line("   end if;");
    builder.line("end loop;");
    builder.dedent();
    builder.line("end Azul_Releaser_Impl;");
    builder.blank();

    // Per-kind invoker stubs.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = wrapper.to_lowercase();
        let mut params = vec!["Id : Interfaces.C.unsigned_long_long".to_string()];
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("Arg{}", i)
            } else {
                pascalize(&arg.name)
            };
            params.push(format!("{} : System.Address", nm));
        }
        if has_return(cb) {
            params.push("Out_Ptr : System.Address".to_string());
        }
        let proc_name = format!("Azul_{}_Invoker_Stub", snake);
        builder.line(&format!(
            "procedure {} ({});",
            proc_name,
            params.join("; ")
        ));
        builder.line(&format!("pragma Convention (C, {});", proc_name));
        builder.blank();
        builder.line(&format!(
            "procedure {} ({}) is",
            proc_name,
            params.join("; ")
        ));
        builder.line("begin");
        builder.indent();
        builder.line(&format!(
            "-- {} stub — first-pass plumbing only; user-callback dispatch",
            wrapper
        ));
        builder.line("-- is the second-pass agent's job.");
        // Reference unused params so Ada doesn't warn about them.
        builder.line("pragma Unreferenced (Id);");
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("Arg{}", i)
            } else {
                pascalize(&arg.name)
            };
            builder.line(&format!("pragma Unreferenced ({});", nm));
        }
        if has_return(cb) {
            builder.line("pragma Unreferenced (Out_Ptr);");
        }
        builder.line("null;");
        builder.dedent();
        builder.line(&format!("end {};", proc_name));
        builder.blank();
    }

    // Public azul_refany_create / azul_refany_get
    builder.line("function Azul_RefAny_Create (Value : System.Address) return Az_RefAny is");
    builder.line("   Id : constant Interfaces.C.unsigned_long_long :=");
    builder.line("      Azul_Alloc_Handle (Value);");
    builder.line("begin");
    builder.indent();
    builder.line("return Az_RefAny_New_Host_Handle (Id);");
    builder.dedent();
    builder.line("end Azul_RefAny_Create;");
    builder.blank();

    builder.line("function Azul_RefAny_Get (RefAny : System.Address) return System.Address is");
    builder.line("   Id : constant Interfaces.C.unsigned_long_long :=");
    builder.line("      Az_RefAny_Get_Host_Handle (RefAny);");
    builder.line("begin");
    builder.indent();
    builder.line("if Id = 0 then");
    builder.line("   return System.Null_Address;");
    builder.line("end if;");
    builder.line("return Azul_Lookup_Handle (Id);");
    builder.dedent();
    builder.line("end Azul_RefAny_Get;");
    builder.blank();

    // Init procedure.
    builder.line("procedure Azul_Host_Invoker_Init is");
    builder.line("begin");
    builder.indent();
    builder.line("Az_App_Set_Host_Handle_Releaser (Azul_Releaser_Impl'Access);");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = wrapper.to_lowercase();
        builder.line(&format!(
            "Az_App_Set_{w}_Invoker (Azul_{snake}_Invoker_Stub'Access);",
            w = wrapper,
            snake = snake
        ));
    }
    builder.dedent();
    builder.line("end Azul_Host_Invoker_Init;");
    builder.blank();
}

fn pascalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut up = true;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            for u in c.to_uppercase() {
                out.push(u);
            }
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

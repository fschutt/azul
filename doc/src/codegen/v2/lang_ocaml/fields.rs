//! Field accessors of the per-class modules: `get_<field>` / `set_<field>`
//! / `update_<field>` (see `raw_field_access` for the contract).
//!
//! - `get_<field> x`: an independent value - a primitive or plain data
//!   copied out, a string decoded (the field is not consumed), a heap-owning
//!   value deep-copied through `Az<T>_clone` (a record comes back armed
//!   with its finaliser);
//! - `set_<field> x v`: releases the old value with `Az<T>_delete`, then
//!   moves `v` in - a record argument is consumed (`disposed <- true`, so
//!   its finaliser never frees the bytes the field now owns);
//! - `update_<field> x f`: nested writes. `Ctypes.getf` copies, so
//!   `set_title (get_window_state opts)` would change a copy; instead `f`
//!   receives a working copy of the field, edits it with the setters, and
//!   the copy is written back when `f` returns (or raises):
//!
//!   ```ocaml
//!   Azul.WindowCreateOptions.update_window_state opts (fun ws ->
//!     Azul.FullWindowState.set_title ws "Hello";
//!     Azul.FullWindowState.update_size ws (fun sz ->
//!       Azul.WindowSize.set_dimensions sz (Azul.LogicalSize.create 400.0 300.0)))
//!   ```
//!
//!   The working copy belongs to the field: `f` must not move it into a
//!   C call or keep it.
//!
//! An api.json method of the same name wins; the accessor is then spelled
//! `<verb>_<field>_field`.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        ir::{CodegenIR, FunctionDef, StructDef},
        raw_field_access::{accessible_fields, RawFieldKind},
    },
    functions::{ocaml_binding_name, should_emit_function},
    map_type_to_ocaml_typ, ocaml_ffi_type_name, ocaml_wrapper_type_name, sanitize_identifier,
    to_snake_case, unit_enum_module,
};

/// One accessor: its `.mli` signature and its implementation lines (a
/// leading `>` marks a body line, one level deeper than the header).
pub(super) struct Accessor {
    pub sig: String,
    pub body: Vec<String>,
}

/// The field accessors of class `s`, in field order. `taken` holds the
/// names the module already defines; the chosen names are added to it.
pub(super) fn field_accessors(
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    records: &BTreeSet<&str>,
    has_wrapper: bool,
    taken: &mut BTreeSet<String>,
) -> Vec<Accessor> {
    let emitted = |f: &FunctionDef| should_emit_function(f, ir, config);
    let recv = if has_wrapper { "self.raw" } else { "self" };
    let ffi = ocaml_ffi_type_name(&s.name);
    let mut out = Vec::new();

    for f in accessible_fields(s, ir, config, &emitted) {
        let t = f.def.type_name.trim();
        let snake = to_snake_case(&f.def.name);
        let acc = format!("{}_field_{}", ffi, sanitize_identifier(&snake));
        let fp = format!("Ctypes.(addr {} |-> {})", recv, acc);
        let mut pick = |verb: &str| -> Option<String> {
            let plain = format!("{}_{}", verb, snake);
            let name = if taken.contains(&plain) {
                format!("{}_field", plain)
            } else {
                plain
            };
            taken.insert(name.clone()).then_some(name)
        };

        match &f.kind {
            RawFieldKind::Prim { .. } => {
                let ty = map_type_to_ocaml_typ(t, ir);
                if let Some(g) = pick("get") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> {}", g, ty),
                        body: vec![
                            format!("let {} (self : t) : {} =", g, ty),
                            format!(">Ctypes.getf {} {}", recv, acc),
                        ],
                    });
                }
                if let Some(n) = pick("set") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> {} -> unit", n, ty),
                        body: vec![
                            format!("let {} (self : t) (v : {}) : unit =", n, ty),
                            format!(">Ctypes.setf {} {} v", recv, acc),
                        ],
                    });
                }
            }
            RawFieldKind::Pod => {
                if let Some(m) = unit_enum_module(t, ir) {
                    let ty = format!("{}.t", m);
                    if let Some(g) = pick("get") {
                        out.push(Accessor {
                            sig: format!("val {} : t -> {}", g, ty),
                            body: vec![
                                format!("let {} (self : t) : {} =", g, ty),
                                format!(
                                    ">match {}.of_int (Ctypes.getf {} {}) with Some __v -> __v | None -> invalid_arg \"{}\"",
                                    m, recv, acc, g
                                ),
                            ],
                        });
                    }
                    if let Some(n) = pick("set") {
                        out.push(Accessor {
                            sig: format!("val {} : t -> {} -> unit", n, ty),
                            body: vec![
                                format!("let {} (self : t) (v : {}) : unit =", n, ty),
                                format!(">Ctypes.setf {} {} ({}.to_int v)", recv, acc, m),
                            ],
                        });
                    }
                    continue;
                }
                let ty = map_type_to_ocaml_typ(t, ir);
                if !ty.ends_with("Ctypes.structure") {
                    continue;
                }
                if let Some(g) = pick("get") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> {}", g, ty),
                        body: vec![
                            format!("let {} (self : t) : {} =", g, ty),
                            format!(">Ctypes.getf {} {}", recv, acc),
                        ],
                    });
                }
                if let Some(n) = pick("set") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> {} -> unit", n, ty),
                        body: vec![
                            format!("let {} (self : t) (v : {}) : unit =", n, ty),
                            format!(">Ctypes.setf {} {} v", recv, acc),
                        ],
                    });
                }
                if let Some(u) = pick("update") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> ({} -> unit) -> unit", u, ty),
                        body: vec![
                            format!("let {} (self : t) (f : {} -> unit) : unit =", u, ty),
                            format!(">let __fp = {} in", fp),
                            ">let __v = Ctypes.(!@ __fp) in".to_string(),
                            ">Fun.protect ~finally:(fun () -> Ctypes.(__fp <-@ __v)) (fun () -> f __v)"
                                .to_string(),
                        ],
                    });
                }
            }
            RawFieldKind::Str { delete } => {
                let delete = ocaml_binding_name(delete);
                if let Some(g) = pick("get") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> string", g),
                        body: vec![
                            format!("let {} (self : t) : string =", g),
                            format!(">azul_string_of_az (Ctypes.getf {} {})", recv, acc),
                        ],
                    });
                }
                if let Some(n) = pick("set") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> string -> unit", n),
                        body: vec![
                            format!("let {} (self : t) (v : string) : unit =", n),
                            ">let __new = azul_az_string v in".to_string(),
                            format!(">let __fp = {} in", fp),
                            format!(">{} __fp;", delete),
                            ">Ctypes.(__fp <-@ __new)".to_string(),
                        ],
                    });
                }
            }
            RawFieldKind::Heap { delete, clone } => {
                let delete = ocaml_binding_name(delete);
                let clone = clone.as_deref().map(ocaml_binding_name);
                if records.contains(t) {
                    let r = ocaml_wrapper_type_name(t);
                    if let Some(clone) = &clone {
                        if let Some(g) = pick("get") {
                            out.push(Accessor {
                                sig: format!("val {} : t -> {}", g, r),
                                body: vec![
                                    format!("let {} (self : t) : {} =", g, r),
                                    format!(">make_{} ({} {})", r, clone, fp),
                                ],
                            });
                        }
                    }
                    if let Some(n) = pick("set") {
                        out.push(Accessor {
                            sig: format!("val {} : t -> {} -> unit", n, r),
                            body: vec![
                                format!("let {} (self : t) (v : {}) : unit =", n, r),
                                format!(
                                    ">if v.disposed then invalid_arg \"{}: the value was already consumed\";",
                                    n
                                ),
                                format!(">let __fp = {} in", fp),
                                format!(">{} __fp;", delete),
                                ">Ctypes.(__fp <-@ v.raw);".to_string(),
                                ">v.disposed <- true".to_string(),
                            ],
                        });
                    }
                    if let Some(u) = pick("update") {
                        out.push(Accessor {
                            sig: format!("val {} : t -> ({} -> unit) -> unit", u, r),
                            body: vec![
                                format!("let {} (self : t) (f : {} -> unit) : unit =", u, r),
                                format!(">let __fp = {} in", fp),
                                // A working copy that owns nothing: no
                                // finaliser, and `disposed` so no consuming
                                // call frees it either.
                                format!(
                                    ">let (__v : {}) = {{ raw = Ctypes.(!@ __fp); disposed = true }} in",
                                    r
                                ),
                                ">Fun.protect ~finally:(fun () -> Ctypes.(__fp <-@ __v.raw)) (fun () -> f __v)"
                                    .to_string(),
                            ],
                        });
                    }
                    continue;
                }
                let ty = map_type_to_ocaml_typ(t, ir);
                if !ty.ends_with("Ctypes.structure") {
                    continue;
                }
                if let Some(clone) = &clone {
                    if let Some(g) = pick("get") {
                        out.push(Accessor {
                            sig: format!("val {} : t -> {}", g, ty),
                            body: vec![
                                format!("let {} (self : t) : {} =", g, ty),
                                format!(">{} {}", clone, fp),
                            ],
                        });
                    }
                }
                if let Some(n) = pick("set") {
                    out.push(Accessor {
                        sig: format!("val {} : t -> {} -> unit", n, ty),
                        body: vec![
                            format!("let {} (self : t) (v : {}) : unit =", n, ty),
                            format!(">let __fp = {} in", fp),
                            format!(">{} __fp;", delete),
                            ">Ctypes.(__fp <-@ v)".to_string(),
                        ],
                    });
                }
            }
        }
    }
    out
}

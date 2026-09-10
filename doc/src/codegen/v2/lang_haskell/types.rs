//! Haskell type emission: `data`/`newtype` declarations + `Storable`
//! instances for every non-generic IR type.
//!
//! Every `Storable` instance here is layout-exact, and none of its numbers
//! is computed on the Haskell side: `sizeOf`, `alignment` and every member
//! offset are nullary pure foreign imports of the cbits layout oracle
//! (`az_hs_sizeof_<T>` / `az_hs_alignof_<T>` / `az_hs_offsetof_<T>_<m>`,
//! emitted by `cshim.rs` from the SAME [`layout_oracle`] table this module
//! consumes), i.e. `sizeof` / `_Alignof` / `offsetof` as the C compiler
//! evaluates them against `azul.h` on the target platform. GHC evaluates
//! each import once (it is a CAF), so `peek`/`poke` pay no call per use.
//! Nothing guesses a size, and no instance `error`s out of `peek`/`poke`.
//!
//! - **Structs** become `data <Name> = <Name> { field1 :: !T1, ... }` with
//!   `peekByteOff`/`pokeByteOff` at the oracle's offsets. Every non-generic struct is emitted —
//!   including the categories other emitters skip (`Recursive`, `VecRef`, `DestructorOrClone`) —
//!   because a struct that is embedded by value anywhere must have its true size, or every struct
//!   embedding it shifts.
//! - **Unit enums** become a Haskell sum type with `deriving (Show, Eq, Enum, Bounded)` and a
//!   4-byte `Storable` (the C header spells them as `enum`, which is `int`-sized).
//! - **Tagged unions** become a sum type with payload constructors. The `Storable` instance reads
//!   the tag (`uint8_t` for `#[repr(C, u8)]`, the C tag enum otherwise — the same rule `lang_c`
//!   spells into the header) at offset 0 and peeks/pokes each payload member at its oracle offset.
//! - **Monomorphized generic aliases** (`CssPropertyValue<T>` etc.) get the same treatment as the
//!   shape they instantiate.
//! - **Simple type aliases** (`ScanCode = u32`, `X11Visual = *const c_void`) become Haskell `type`
//!   synonyms of the target's representation.
//! - **Callback typedefs** become `newtype <Name> = <Name> (FunPtr ())`.

use std::collections::BTreeMap;

use anyhow::{bail, Result};

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, MonomorphizedKind,
            MonomorphizedVariant, StructDef, TypeAliasDef, TypeCategory,
        },
        lang_c::escape_cpp_keyword_for_c,
    },
    haskell_data_name, haskell_field_name, haskell_variant_name, lower_first, sanitize_doc,
};

// ============================================================================
// Layout oracle table (shared with cshim.rs)
// ============================================================================

/// One C type whose layout the cbits oracle reports: `sizeof` /
/// `_Alignof`, plus `offsetof` for each member in `members`.
pub(super) struct OracleType {
    /// api.json spelling (`Dom`, `OptionRefAny`, `StyleTextColorValue`);
    /// the C type is `Az<ir_name>` and the symbols are
    /// `az_hs_{sizeof,alignof}_<ir_name>` / `az_hs_offsetof_<ir_name>_<suffix>`.
    pub ir_name: String,
    pub members: Vec<OracleMember>,
}

/// One `offsetof` the oracle reports.
pub(super) struct OracleMember {
    /// Member designator as C spells it: `root`, `Some.payload`,
    /// `Exact.payload_1`, `Point.x`.
    pub c_path: String,
    /// Symbol suffix (`root`, `Some_payload`, ...); a valid identifier
    /// fragment in both C and Haskell.
    pub suffix: String,
}

/// Every type `Azul.Types` needs oracle numbers for, in emission order.
/// `cshim.rs` emits one C function per entry and member from this exact
/// list, so the Haskell imports and the C definitions cannot drift.
pub(super) fn layout_oracle(ir: &CodegenIR, config: &CodegenConfig) -> Vec<OracleType> {
    let mut out = Vec::new();
    for s in &ir.structs {
        if should_emit_struct(s, config) && !s.fields.is_empty() {
            out.push(OracleType {
                ir_name: s.name.clone(),
                members: struct_members(&s.fields),
            });
        }
    }
    for e in &ir.enums {
        if should_emit_enum(e, config) && e.is_union && !e.variants.is_empty() {
            let mut members = Vec::new();
            for v in &e.variants {
                let names: Vec<String> = match &v.kind {
                    EnumVariantKind::Unit => Vec::new(),
                    EnumVariantKind::Tuple(types) if types.len() == 1 => {
                        vec!["payload".to_string()]
                    }
                    EnumVariantKind::Tuple(types) => {
                        (0..types.len()).map(|i| format!("payload_{}", i)).collect()
                    }
                    EnumVariantKind::Struct(fields) => fields
                        .iter()
                        .map(|f| escape_cpp_keyword_for_c(&f.name))
                        .collect(),
                };
                for n in names {
                    members.push(OracleMember {
                        c_path: format!("{}.{}", v.name, n),
                        suffix: format!("{}_{}", v.name, n),
                    });
                }
            }
            out.push(OracleType {
                ir_name: e.name.clone(),
                members,
            });
        }
    }
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        match ta.monomorphized_def.as_ref().map(|m| &m.kind) {
            Some(MonomorphizedKind::Struct { fields }) if !fields.is_empty() => {
                out.push(OracleType {
                    ir_name: ta.name.clone(),
                    members: struct_members(fields),
                });
            }
            Some(MonomorphizedKind::TaggedUnion { variants, .. }) if !variants.is_empty() => {
                let members = variants
                    .iter()
                    .filter(|v| v.payload_type.is_some())
                    .map(|v| OracleMember {
                        c_path: format!("{}.payload", v.name),
                        suffix: format!("{}_payload", v.name),
                    })
                    .collect();
                out.push(OracleType {
                    ir_name: ta.name.clone(),
                    members,
                });
            }
            _ => {}
        }
    }
    out
}

fn struct_members(fields: &[FieldDef]) -> Vec<OracleMember> {
    fields
        .iter()
        .map(|f| {
            let c = escape_cpp_keyword_for_c(&f.name);
            OracleMember {
                c_path: c.clone(),
                suffix: c,
            }
        })
        .collect()
}

/// Haskell name of the oracle import for a type's size / alignment.
fn oracle_size_binding(lname: &str) -> String {
    format!("c_az_hs_sizeof_{}", lname)
}

fn oracle_align_binding(lname: &str) -> String {
    format!("c_az_hs_alignof_{}", lname)
}

fn oracle_offset_binding(lname: &str, suffix: &str) -> String {
    format!("c_az_hs_offsetof_{}_{}", lname, suffix)
}

/// `foreign import ccall unsafe "az_hs_sizeof_<T>" c_az_hs_sizeof_<t> :: CSize`
/// and friends: nullary pure imports, evaluated once by GHC.
fn emit_oracle_imports(
    builder: &mut CodeBuilder,
    ir_name: &str,
    lname: &str,
    members: &[OracleMember],
) {
    builder.line(&format!(
        "foreign import ccall unsafe \"az_hs_sizeof_{}\"",
        ir_name
    ));
    builder.line(&format!("    {} :: CSize", oracle_size_binding(lname)));
    builder.line(&format!(
        "foreign import ccall unsafe \"az_hs_alignof_{}\"",
        ir_name
    ));
    builder.line(&format!("    {} :: CSize", oracle_align_binding(lname)));
    for m in members {
        builder.line(&format!(
            "foreign import ccall unsafe \"az_hs_offsetof_{}_{}\"",
            ir_name, m.suffix
        ));
        builder.line(&format!(
            "    {} :: CSize",
            oracle_offset_binding(lname, &m.suffix)
        ));
    }
}

fn offset_expr(lname: &str, suffix: &str) -> String {
    format!("(fromIntegral {})", oracle_offset_binding(lname, suffix))
}

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_type_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Struct data declarations + Storable instances (layout from the cbits oracle)");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    // Module-scoped registry of every `foreign import` binding this
    // module has already emitted: binding name -> C symbol it is bound
    // to. Emission below is driven by a *per-struct* loop while some
    // bindings are keyed on a struct's *element* type, so two structs
    // sharing an element (e.g. `StringVec` and `IcuStringVec`, both
    // over `String`) would otherwise emit the same declaration twice
    // and trip GHC-29916 "Multiple declarations of ...".
    let mut foreign_imports: BTreeMap<String, String> = BTreeMap::new();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic struct {} (no Haskell equivalent over the C ABI)",
                    s.name
                ));
            }
            continue;
        }
        emit_struct_decl(builder, s, ir, &mut foreign_imports)?;
    }

    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Enum data declarations + Storable instances");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            if !e.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic enum {} (no Haskell equivalent over the C ABI)",
                    e.name
                ));
            }
            continue;
        }
        if e.is_union {
            emit_tagged_union_decl(builder, e, ir);
        } else {
            emit_unit_enum_decl(builder, e);
        }
    }

    // Monomorphized generic aliases (`CssPropertyValue<StringSet>` =
    // `StringSetValue` etc.) and simple aliases (`ScanCode = u32`).
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Type aliases (monomorphized generics as real types, simple ones as synonyms)");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        match &ta.monomorphized_def {
            Some(md) => emit_monomorphized_alias(builder, ta, &md.kind, ir),
            None => emit_simple_alias(builder, ta, ir),
        }
    }

    // Callback typedefs are C function pointers. A newtype over `FunPtr ()`
    // gives them an exact pointer-sized `Storable`, so a struct field of
    // that type (`LayoutCallback.cb`) round-trips instead of being dropped
    // on poke.
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Callback typedefs (C function pointers)");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        let name = haskell_data_name(&cb.name);
        builder.line(&format!(
            "newtype {} = {} (FunPtr ()) deriving (Show, Eq)",
            name, name
        ));
        builder.line(&format!("instance Storable {} where", name));
        builder.indent();
        builder.line("sizeOf _ = sizeOf (undefined :: FunPtr ())");
        builder.line("alignment _ = alignment (undefined :: FunPtr ())");
        builder.line(&format!("peek p = {} <$> peek (castPtr p)", name));
        builder.line(&format!("poke p ({} f) = poke (castPtr p) f", name));
        builder.dedent();
        builder.blank();
    }

    Ok(())
}

// ============================================================================
// Inclusion filters
// ============================================================================

/// Every non-generic struct the header declares gets a layout-exact
/// declaration — a struct that any other struct embeds by value must have
/// its true size, whatever its category.
pub fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name) && s.generic_params.is_empty()
}

pub fn should_emit_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&e.name) && e.generic_params.is_empty()
}

// ============================================================================
// Struct emission
// ============================================================================

fn emit_struct_decl(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    foreign_imports: &mut BTreeMap<String, String>,
) -> Result<()> {
    let name = haskell_data_name(&s.name);
    for d in &s.doc {
        builder.line(&format!("-- | {}", sanitize_doc(d)));
    }

    let fields: Vec<(String, String)> = s
        .fields
        .iter()
        .map(|f| {
            (
                haskell_field_name(&s.name, &f.name),
                haskell_field_type(&f.type_name, f.ref_kind, ir),
            )
        })
        .collect();
    let field_docs: Vec<Option<String>> = s.fields.iter().map(|f| f.doc.clone()).collect();
    emit_record_type(
        builder,
        &s.name,
        &name,
        &fields,
        &field_docs,
        &struct_members(&s.fields),
    );

    // AzString -> Haskell String decoder, keyed on the IR's String category
    // rather than the name so it stays honest if a second string type
    // ever appears.
    if matches!(s.category, TypeCategory::String) {
        emit_string_to_string_helper(builder, s);
    }

    // AzVec<T> -> Haskell list helper, keyed on the (ptr, len, cap,
    // destructor) field pattern every codegen-emitted Vec type has.
    if let Some(elem_ty) = detect_vec_elem_type(s) {
        emit_vec_to_list_helper(builder, s, &elem_ty, ir, foreign_imports)?;
    }

    Ok(())
}

/// Emit `data Name = Name { f1 :: !T1, ... }` plus its `Storable` instance
/// over the oracle. Shared by structs and monomorphized struct aliases.
fn emit_record_type(
    builder: &mut CodeBuilder,
    ir_name: &str,
    name: &str,
    fields: &[(String, String)],
    field_docs: &[Option<String>],
    members: &[OracleMember],
) {
    if fields.is_empty() {
        // The C header spells an empty struct with a `uint8_t _dummy`
        // member, so it is exactly one byte.
        builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
        builder.line(&format!("instance Storable {} where", name));
        builder.indent();
        builder.line("sizeOf _ = 1");
        builder.line("alignment _ = 1");
        builder.line(&format!("peek _ = pure {}", name));
        builder.line("poke _ _ = pure ()");
        builder.dedent();
        builder.blank();
        return;
    }

    builder.line(&format!("data {} = {}", name, name));
    builder.indent();
    for (i, (fname, hty)) in fields.iter().enumerate() {
        let prefix = if i == 0 { "{ " } else { ", " };
        if let Some(Some(doc)) = field_docs.get(i) {
            builder.line(&format!("-- ^ {}", sanitize_doc(doc)));
        }
        // `!(T)` — the strictness annotation binds tighter than type
        // application, so `!Ptr ()` is a parse error without the parens.
        builder.line(&format!("{}{} :: !({})", prefix, fname, hty));
    }
    builder.line("} deriving (Show)");
    builder.dedent();
    builder.blank();

    let lname = lower_first(name);
    emit_oracle_imports(builder, ir_name, &lname, members);
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line(&format!(
        "sizeOf _ = fromIntegral {}",
        oracle_size_binding(&lname)
    ));
    builder.line(&format!(
        "alignment _ = fromIntegral {}",
        oracle_align_binding(&lname)
    ));
    let mut peek_expr = format!("peek p = {}", name);
    for (i, _) in fields.iter().enumerate() {
        let op = if i == 0 { "<$>" } else { "<*>" };
        peek_expr.push_str(&format!(
            " {} peekByteOff p {}",
            op,
            offset_expr(&lname, &members[i].suffix)
        ));
    }
    builder.line(&peek_expr);
    builder.line("poke p x = do");
    builder.indent();
    for (i, (fname, _)) in fields.iter().enumerate() {
        builder.line(&format!(
            "pokeByteOff p {} ({} x)",
            offset_expr(&lname, &members[i].suffix),
            fname
        ));
    }
    builder.dedent();
    builder.dedent();
    builder.blank();
}

/// True if this struct matches the codegen-emitted Vec shape:
/// fields = [ptr : *mut|*const T, len : usize, cap : usize, destructor : <Self>Destructor]
/// Returns the element type (T) on match.
fn detect_vec_elem_type(s: &StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    let f_ptr = &s.fields[0];
    let f_len = &s.fields[1];
    let f_cap = &s.fields[2];
    if f_ptr.name != "ptr" || f_len.name != "len" || f_cap.name != "cap" {
        return None;
    }
    if f_len.type_name.trim() != "usize" || f_cap.type_name.trim() != "usize" {
        return None;
    }
    // Element type from the ptr field. The ref_kind / type_name carry
    // the raw-pointer marker.
    let raw = f_ptr.type_name.trim();
    let elem = raw
        .strip_prefix("*mut ")
        .or_else(|| raw.strip_prefix("*const "))
        .map(str::trim)
        .unwrap_or(raw);
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

fn emit_vec_to_list_helper(
    builder: &mut CodeBuilder,
    s: &StructDef,
    elem_rust: &str,
    ir: &CodegenIR,
    foreign_imports: &mut BTreeMap<String, String>,
) -> Result<()> {
    use super::super::ir::FunctionKind;
    let vec_name = haskell_data_name(&s.name);
    let elem_haskell = haskell_field_type(elem_rust, FieldRefKind::Owned, ir);
    let lname = lower_first(&vec_name);
    let helper = format!("{}ToList", lname);

    // When the element type has a `_clone` export, the shim layer provides
    // `Az<X>_clone_via` (input ptr + output ptr). Each list entry then owns
    // an independent heap allocation — closing the Vec later doesn't
    // dangle the yielded `Storable` peeks. Without `_clone`, fall back to
    // the shallow `peekElemOff` path (POD elements).
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == elem_rust && matches!(f.kind, FunctionKind::DeepCopy));
    let clone_via_binding = format!(
        "az_{}_clone_via_internal",
        lower_first(&haskell_data_name(elem_rust))
    );
    let clone_via_symbol = format!("Az{}_clone_via", elem_rust);

    if has_clone {
        // A local foreign-import bound to the same C symbol the FFI module
        // imports. Re-declaring the symbol in *another* module is fine
        // (each module links its own import); WITHIN a module a repeated
        // declaration is GHC-29916, and this loop can produce one because
        // the binding is keyed on the *element* type while the helper runs
        // once per *Vec* struct (`StringVec` + `IcuStringVec` over
        // `String`). `foreign_imports` makes the declaration emit once.
        match foreign_imports.get(&clone_via_binding) {
            None => {
                foreign_imports.insert(clone_via_binding.clone(), clone_via_symbol.clone());
                builder.line(&format!(
                    "foreign import ccall safe \"{}\"",
                    clone_via_symbol
                ));
                builder.indent();
                builder.line(&format!(
                    "{} :: Ptr {} -> Ptr {} -> IO ()",
                    clone_via_binding, elem_haskell, elem_haskell
                ));
                builder.dedent();
                builder.blank();
            }
            Some(prev) if *prev == clone_via_symbol => {
                builder.line(&format!(
                    "-- `{}` (= C `{}`) already declared above for another Vec over `{}`.",
                    clone_via_binding, clone_via_symbol, elem_rust
                ));
            }
            Some(prev) => {
                bail!(
                    "Haskell codegen: foreign-import binding `{}` would be bound to two different \
                     C symbols in Azul.Types: `{}` (already emitted) and `{}` (required by `{}` \
                     over element `{}`). The generated binding name must be made unique per C \
                     symbol.",
                    clone_via_binding,
                    prev,
                    clone_via_symbol,
                    s.name,
                    elem_rust
                );
            }
        }
    }

    builder.line("-- | Decode the underlying buffer into a Haskell list.");
    if has_clone {
        builder.line(&format!(
            "-- Each element is cloned via `Az{}_clone_via` so the yielded list",
            elem_rust
        ));
        builder.line("-- entries own independent heap allocations and survive the Vec being");
        builder.line("-- closed.");
    } else {
        builder.line(&format!(
            "-- WARNING: no `Az{}_clone` export — falls back to shallow peekElemOff;",
            elem_rust
        ));
        builder.line("-- yielded entries dangle if the Vec is closed before they're consumed.");
    }
    builder.line(&format!(
        "{} :: {} -> IO [{}]",
        helper, vec_name, elem_haskell
    ));
    builder.line(&format!("{} v = do", helper));
    builder.indent();
    let ptr_field = haskell_field_name(&s.name, "ptr");
    let len_field = haskell_field_name(&s.name, "len");
    builder.line(&format!("let __p = {} v", ptr_field));
    builder.line(&format!("    __n = fromIntegral ({} v) :: Int", len_field));
    if has_clone {
        builder.line(&format!(
            "let __elem_sz = sizeOf (undefined :: {})",
            elem_haskell
        ));
        builder.line("mapM (\\i -> Foreign.Marshal.Alloc.alloca $ \\__out -> do");
        builder.line("    let __ep = __p `Foreign.Ptr.plusPtr` (i * __elem_sz)");
        builder.line(&format!("    {} __ep __out", clone_via_binding));
        builder.line("    peek __out) [0 .. __n - 1]");
    } else {
        builder.line("mapM (peekElemOff __p) [0 .. __n - 1]");
    }
    builder.dedent();
    builder.blank();

    Ok(())
}

// ============================================================================
// Unit enum emission
// ============================================================================

fn emit_unit_enum_decl(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);
    for d in &e.doc {
        builder.line(&format!("-- | {}", sanitize_doc(d)));
    }
    let variants: Vec<String> = e
        .variants
        .iter()
        .map(|v| haskell_variant_name(&e.name, &v.name))
        .collect();
    emit_unit_enum_type(builder, &name, &variants);
}

/// `data Name = V0 | V1 | ...` with a 4-byte `Storable` (C `enum`).
fn emit_unit_enum_type(builder: &mut CodeBuilder, name: &str, variants: &[String]) {
    if variants.is_empty() {
        builder.line(&format!("-- SKIPPED: unit enum {} has no variants", name));
        builder.blank();
        return;
    }

    builder.line(&format!("data {}", name));
    builder.indent();
    for (i, vname) in variants.iter().enumerate() {
        let prefix = if i == 0 { "= " } else { "| " };
        builder.line(&format!("{}{}", prefix, vname));
    }
    builder.line("deriving (Show, Eq, Enum, Bounded)");
    builder.dedent();
    builder.blank();

    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line("sizeOf _ = 4");
    builder.line("alignment _ = 4");
    builder.line("peek p = do");
    builder.indent();
    builder.line("w <- peek (castPtr p :: Ptr Word32)");
    builder.line("case w of");
    builder.indent();
    for (idx, vname) in variants.iter().enumerate() {
        builder.line(&format!("{} -> pure {}", idx, vname));
    }
    builder.line(&format!(
        "_ -> error (\"Azul.Types.peek {}: unknown discriminator \" ++ show w)",
        name
    ));
    builder.dedent();
    builder.dedent();
    builder.line("poke p v = case v of");
    builder.indent();
    for (idx, vname) in variants.iter().enumerate() {
        builder.line(&format!(
            "{} -> poke (castPtr p :: Ptr Word32) {}",
            vname, idx
        ));
    }
    builder.dedent();
    builder.dedent();
    builder.blank();
}

// ============================================================================
// Tagged-union emission
// ============================================================================

/// One constructor of a tagged union: its Haskell name and, per payload
/// member (in C declaration order), the Haskell type and the oracle suffix
/// of its offset.
struct UnionVariant {
    ctor: String,
    payloads: Vec<(String, String)>,
}

/// The tag `lang_c` spells for this repr: `uint8_t` iff the repr contains
/// "u8", else the C tag enum (`int`-sized). The Haskell side reads/writes
/// it through the matching word type.
fn tag_haskell_type(repr: Option<&str>) -> &'static str {
    if repr.map(|r| r.contains("u8")).unwrap_or(false) {
        "Word8"
    } else {
        "Word32"
    }
}

fn emit_tagged_union_decl(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let name = haskell_data_name(&e.name);
    for d in &e.doc {
        builder.line(&format!("-- | {}", sanitize_doc(d)));
    }
    let variants: Vec<UnionVariant> = e
        .variants
        .iter()
        .map(|v| {
            let types: Vec<String> = match &v.kind {
                EnumVariantKind::Unit => Vec::new(),
                EnumVariantKind::Tuple(payload) => payload
                    .iter()
                    .map(|(t, rk)| haskell_field_type(t, *rk, ir))
                    .collect(),
                EnumVariantKind::Struct(fields) => fields
                    .iter()
                    .map(|f| haskell_field_type(&f.type_name, f.ref_kind, ir))
                    .collect(),
            };
            let suffixes: Vec<String> = match &v.kind {
                EnumVariantKind::Unit => Vec::new(),
                EnumVariantKind::Tuple(types) if types.len() == 1 => {
                    vec![format!("{}_payload", v.name)]
                }
                EnumVariantKind::Tuple(types) => (0..types.len())
                    .map(|i| format!("{}_payload_{}", v.name, i))
                    .collect(),
                EnumVariantKind::Struct(fields) => fields
                    .iter()
                    .map(|f| format!("{}_{}", v.name, escape_cpp_keyword_for_c(&f.name)))
                    .collect(),
            };
            UnionVariant {
                ctor: haskell_variant_name(&e.name, &v.name),
                payloads: types.into_iter().zip(suffixes).collect(),
            }
        })
        .collect();
    let members: Vec<OracleMember> = variants
        .iter()
        .flat_map(|v| v.payloads.iter())
        .map(|(_, suffix)| OracleMember {
            c_path: String::new(),
            suffix: suffix.clone(),
        })
        .collect();
    emit_union_type(
        builder,
        &e.name,
        &name,
        &variants,
        &members,
        tag_haskell_type(e.repr.as_deref()),
    );

    // Tag-byte discriminator accessors for Option/Result-shaped enums
    // (`optionXIsSome :: Ptr OptionX -> IO Bool`): cheap checks that don't
    // require peeking the payload.
    if is_option_shape(e) {
        emit_option_tag_helpers(builder, e);
    } else if is_result_shape(e) {
        emit_result_tag_helpers(builder, e);
    }
}

/// `data Name = V0 | V1 T | ...` plus the `Storable` instance over the
/// oracle. Shared by IR enums and monomorphized union aliases.
fn emit_union_type(
    builder: &mut CodeBuilder,
    ir_name: &str,
    name: &str,
    variants: &[UnionVariant],
    members: &[OracleMember],
    tag_ty: &str,
) {
    if variants.is_empty() {
        builder.line(&format!(
            "-- SKIPPED: tagged-union {} has no variants",
            name
        ));
        builder.blank();
        return;
    }

    builder.line(&format!("data {}", name));
    builder.indent();
    for (i, v) in variants.iter().enumerate() {
        let prefix = if i == 0 { "= " } else { "| " };
        if v.payloads.is_empty() {
            builder.line(&format!("{}{}", prefix, v.ctor));
        } else {
            let payloads: Vec<String> =
                v.payloads.iter().map(|(t, _)| paren_if_needed(t)).collect();
            builder.line(&format!("{}{} {}", prefix, v.ctor, payloads.join(" ")));
        }
    }
    builder.line("deriving (Show)");
    builder.dedent();
    builder.blank();

    let lname = lower_first(name);
    emit_oracle_imports(builder, ir_name, &lname, members);
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line(&format!(
        "sizeOf _ = fromIntegral {}",
        oracle_size_binding(&lname)
    ));
    builder.line(&format!(
        "alignment _ = fromIntegral {}",
        oracle_align_binding(&lname)
    ));
    builder.line("peek p = do");
    builder.indent();
    builder.line(&format!("tag <- peek (castPtr p :: Ptr {})", tag_ty));
    builder.line("case tag of");
    builder.indent();
    for (idx, v) in variants.iter().enumerate() {
        if v.payloads.is_empty() {
            builder.line(&format!("{} -> pure {}", idx, v.ctor));
        } else {
            let mut expr = format!("{} -> {}", idx, v.ctor);
            for (j, (_, suffix)) in v.payloads.iter().enumerate() {
                let op = if j == 0 { "<$>" } else { "<*>" };
                expr.push_str(&format!(
                    " {} peekByteOff p {}",
                    op,
                    offset_expr(&lname, suffix)
                ));
            }
            builder.line(&expr);
        }
    }
    builder.line(&format!(
        "_ -> error (\"Azul.Types.peek {}: unknown discriminator \" ++ show tag)",
        name
    ));
    builder.dedent();
    builder.dedent();
    builder.line("poke p v = case v of");
    builder.indent();
    for (idx, v) in variants.iter().enumerate() {
        if v.payloads.is_empty() {
            builder.line(&format!(
                "{} -> poke (castPtr p :: Ptr {}) {}",
                v.ctor, tag_ty, idx
            ));
        } else {
            let binders: Vec<String> = (0..v.payloads.len()).map(|j| format!("a{}", j)).collect();
            builder.line(&format!("{} {} -> do", v.ctor, binders.join(" ")));
            builder.indent();
            builder.line(&format!("poke (castPtr p :: Ptr {}) {}", tag_ty, idx));
            for (j, b) in binders.iter().enumerate() {
                builder.line(&format!(
                    "pokeByteOff p {} {}",
                    offset_expr(&lname, &v.payloads[j].1),
                    b
                ));
            }
            builder.dedent();
        }
    }
    builder.dedent();
    builder.dedent();
    builder.blank();
}

/// Emit a `azStringToString :: AzString -> IO String` helper that
/// decodes the wrapped UTF-8 bytes via the U8Vec's (ptr, len) fields.
fn emit_string_to_string_helper(builder: &mut CodeBuilder, s: &StructDef) {
    let Some(field) = s.fields.first() else {
        return;
    };
    let field_name = haskell_field_name(&s.name, &field.name);
    let lname = lower_first(&haskell_data_name(&s.name));
    builder.line("-- | Decode the wrapped UTF-8 bytes into a Haskell String.");
    builder.line(&format!(
        "{}ToString :: {} -> IO String",
        lname,
        haskell_data_name(&s.name)
    ));
    builder.line(&format!("{}ToString s = do", lname));
    builder.indent();
    builder.line(&format!("let __vec = {} s", field_name));
    builder.line("    __p = u8VecPtr __vec");
    builder.line("    __n = fromIntegral (u8VecLen __vec) :: Int");
    builder.line("__bytes <- mapM (peekElemOff __p) [0 .. __n - 1]");
    builder.line("pure (decodeUtf8 __bytes)");
    builder.dedent();
    builder.blank();
}

/// True when this enum has exactly two variants named (None, Some) — the
/// AzOption pattern. Variant order in the enum is irrelevant; tag values
/// come from declaration position in the C ABI.
fn is_option_shape(e: &EnumDef) -> bool {
    e.variants.len() == 2
        && e.variants.iter().any(|v| v.name == "None")
        && e.variants.iter().any(|v| v.name == "Some")
}

fn is_result_shape(e: &EnumDef) -> bool {
    e.variants.len() == 2
        && e.variants.iter().any(|v| v.name == "Ok")
        && e.variants.iter().any(|v| v.name == "Err")
}

fn emit_tag_predicate(
    builder: &mut CodeBuilder,
    fn_name: &str,
    ty_name: &str,
    tag_ty: &str,
    expected: usize,
) {
    builder.line(&format!("{} :: Ptr {} -> IO Bool", fn_name, ty_name));
    builder.line(&format!("{} p = do", fn_name));
    builder.indent();
    builder.line(&format!("tag <- peek (castPtr p :: Ptr {})", tag_ty));
    builder.line(&format!("pure (tag == {})", expected));
    builder.dedent();
}

fn emit_option_tag_helpers(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);
    let lname = lower_first(&name);
    let tag_ty = tag_haskell_type(e.repr.as_deref());
    let none_idx = e.variants.iter().position(|v| v.name == "None").unwrap();
    let some_idx = e.variants.iter().position(|v| v.name == "Some").unwrap();
    builder.line("-- | True if the underlying Option is the None variant (reads only the tag).");
    emit_tag_predicate(
        builder,
        &format!("{}IsNone", lname),
        &name,
        tag_ty,
        none_idx,
    );
    emit_tag_predicate(
        builder,
        &format!("{}IsSome", lname),
        &name,
        tag_ty,
        some_idx,
    );
    builder.blank();
}

fn emit_result_tag_helpers(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);
    let lname = lower_first(&name);
    let tag_ty = tag_haskell_type(e.repr.as_deref());
    let ok_idx = e.variants.iter().position(|v| v.name == "Ok").unwrap();
    let err_idx = e.variants.iter().position(|v| v.name == "Err").unwrap();
    builder.line("-- | True if the underlying Result is the Ok variant (reads only the tag).");
    emit_tag_predicate(builder, &format!("{}IsOk", lname), &name, tag_ty, ok_idx);
    emit_tag_predicate(builder, &format!("{}IsErr", lname), &name, tag_ty, err_idx);
    builder.blank();
}

// ============================================================================
// Type aliases
// ============================================================================

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &TypeAliasDef,
    kind: &MonomorphizedKind,
    ir: &CodegenIR,
) {
    let name = haskell_data_name(&ta.name);
    for d in &ta.doc {
        builder.line(&format!("-- | {}", sanitize_doc(d)));
    }
    match kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            let ctors: Vec<String> = variants
                .iter()
                .map(|v| haskell_variant_name(&ta.name, v))
                .collect();
            emit_unit_enum_type(builder, &name, &ctors);
        }
        MonomorphizedKind::Struct { fields } => {
            let hs_fields: Vec<(String, String)> = fields
                .iter()
                .map(|f| {
                    (
                        haskell_field_name(&ta.name, &f.name),
                        haskell_field_type(&f.type_name, f.ref_kind, ir),
                    )
                })
                .collect();
            let docs: Vec<Option<String>> = fields.iter().map(|f| f.doc.clone()).collect();
            emit_record_type(
                builder,
                &ta.name,
                &name,
                &hs_fields,
                &docs,
                &struct_members(fields),
            );
        }
        MonomorphizedKind::TaggedUnion { repr, variants } => {
            let hs_variants: Vec<UnionVariant> = variants
                .iter()
                .map(|v: &MonomorphizedVariant| UnionVariant {
                    ctor: haskell_variant_name(&ta.name, &v.name),
                    payloads: v
                        .payload_type
                        .iter()
                        .map(|p| {
                            (
                                haskell_field_type(p, v.payload_ref_kind, ir),
                                format!("{}_payload", v.name),
                            )
                        })
                        .collect(),
                })
                .collect();
            let members: Vec<OracleMember> = hs_variants
                .iter()
                .flat_map(|v| v.payloads.iter())
                .map(|(_, suffix)| OracleMember {
                    c_path: String::new(),
                    suffix: suffix.clone(),
                })
                .collect();
            emit_union_type(
                builder,
                &ta.name,
                &name,
                &hs_variants,
                &members,
                tag_haskell_type(repr.as_deref()),
            );
        }
    }
}

/// `type ScanCode = Word32`, `type X11Visual = Ptr ()`: a simple alias is
/// a synonym of its target's representation, so a field of that type has
/// the target's exact `Storable`.
fn emit_simple_alias(builder: &mut CodeBuilder, ta: &TypeAliasDef, ir: &CodegenIR) {
    let name = haskell_data_name(&ta.name);
    let target = map_owned_type(&ta.target, ir);
    if target == name {
        return;
    }
    for d in &ta.doc {
        builder.line(&format!("-- | {}", sanitize_doc(d)));
    }
    builder.line(&format!("type {} = {}", name, target));
    builder.blank();
}

// ============================================================================
// Type-mapping helpers
// ============================================================================

/// Map an IR field type + ref-kind to a Haskell type expression.
pub fn haskell_field_type(type_name: &str, ref_kind: FieldRefKind, ir: &CodegenIR) -> String {
    match ref_kind {
        FieldRefKind::Owned => map_owned_type(type_name, ir),
        FieldRefKind::Ref
        | FieldRefKind::RefMut
        | FieldRefKind::Ptr
        | FieldRefKind::PtrMut
        | FieldRefKind::Boxed
        | FieldRefKind::OptionBoxed => {
            format!("Ptr {}", paren_if_needed(&map_owned_type(type_name, ir)))
        }
    }
}

/// Wrap a multi-token type expression in parens so it can be applied to
/// (`Ptr (RefAny)` is fine, `Ptr Ptr ()` is not).
fn paren_if_needed(s: &str) -> String {
    let needs = s.contains(' ') && !(s.starts_with('(') && s.ends_with(')'));
    if needs {
        format!("({})", s)
    } else {
        s.to_string()
    }
}

pub fn map_owned_type(type_name: &str, ir: &CodegenIR) -> String {
    let t = type_name.trim();

    // Pointer / reference forms in the type string itself.
    if let Some(rest) = t.strip_prefix("*const ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix("*mut ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix("&mut ") {
        return pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = t.strip_prefix('&') {
        return pointer_form(rest.trim(), ir);
    }

    match t {
        "bool" => "CBool".to_string(),
        "u8" | "c_uchar" => "Word8".to_string(),
        "i8" | "c_char" => "Int8".to_string(),
        "char" => "CChar".to_string(),
        "u16" => "Word16".to_string(),
        "i16" => "Int16".to_string(),
        "u32" | "c_uint" => "Word32".to_string(),
        "i32" | "c_int" => "Int32".to_string(),
        "u64" => "Word64".to_string(),
        "i64" => "Int64".to_string(),
        "f32" => "CFloat".to_string(),
        "f64" => "CDouble".to_string(),
        "usize" => "CSize".to_string(),
        "isize" => "CIntPtr".to_string(),
        "c_void" | "()" | "void" => "()".to_string(),
        _ => {
            if ir.find_struct(t).is_some()
                || ir.find_enum(t).is_some()
                || ir.find_type_alias(t).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == t)
            {
                haskell_data_name(t)
            } else {
                // Unknown type — keep as opaque pointer so the binding
                // still type-checks.
                "(Ptr ())".to_string()
            }
        }
    }
}

fn pointer_form(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(Ptr ())".to_string();
    }
    if inner == "c_char" || inner == "u8" {
        // C-string / byte-buffer pointers: still typed for the user's
        // benefit, but the underlying repr is the same as `Ptr Word8`.
        return "(Ptr Word8)".to_string();
    }
    format!("(Ptr {})", paren_if_needed(&map_owned_type(inner, ir)))
}

#[cfg(test)]
mod tests {
    use super::{
        super::super::ir::{EnumVariantDef, FunctionDef, FunctionKind},
        *,
    };

    fn field(fname: &str, ty: &str) -> FieldDef {
        FieldDef {
            name: fname.to_string(),
            type_name: ty.to_string(),
            doc: None,
            is_public: true,
            ref_kind: FieldRefKind::Owned,
        }
    }

    fn plain_struct(name: &str, fields: Vec<FieldDef>, category: TypeCategory) -> StructDef {
        StructDef {
            name: name.to_string(),
            doc: vec![],
            fields,
            external_path: None,
            module: "vec".to_string(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".to_string()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        }
    }

    /// A Vec-shaped struct: `[ptr: *const <elem>, len, cap, destructor]` —
    /// exactly the shape `detect_vec_elem_type` keys on.
    fn vec_struct(name: &str, elem: &str) -> StructDef {
        plain_struct(
            name,
            vec![
                field("ptr", &format!("*const {}", elem)),
                field("len", "usize"),
                field("cap", "usize"),
                field("destructor", &format!("{}Destructor", name)),
            ],
            TypeCategory::Vec,
        )
    }

    fn destructor_union(name: &str, repr: &str) -> EnumDef {
        EnumDef {
            name: name.to_string(),
            doc: vec![],
            variants: vec![
                EnumVariantDef {
                    name: "DefaultRust".into(),
                    doc: None,
                    kind: EnumVariantKind::Unit,
                },
                EnumVariantDef {
                    name: "External".into(),
                    doc: None,
                    kind: EnumVariantKind::Tuple(vec![("*mut c_void".into(), FieldRefKind::Owned)]),
                },
            ],
            external_path: None,
            module: "vec".to_string(),
            derives: vec![],
            has_explicit_derive: false,
            is_union: true,
            repr: Some(repr.to_string()),
            is_send_safe: true,
            traits: Default::default(),
            generic_params: vec![],
            category: TypeCategory::DestructorOrClone,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
        }
    }

    fn deep_copy_fn(class_name: &str) -> FunctionDef {
        FunctionDef {
            c_name: format!("Az{}_deepCopy", class_name),
            class_name: class_name.to_string(),
            method_name: "deepCopy".to_string(),
            kind: FunctionKind::DeepCopy,
            args: vec![],
            return_type: Some(class_name.to_string()),
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        }
    }

    fn fixture_ir() -> CodegenIR {
        let mut ir = CodegenIR::new();
        ir.structs.push(plain_struct(
            "String",
            vec![field("vec", "U8Vec")],
            TypeCategory::String,
        ));
        ir.structs.push(vec_struct("U8Vec", "u8"));
        ir.structs.push(vec_struct("StringVec", "String"));
        ir.structs.push(vec_struct("IcuStringVec", "String"));
        ir.enums.push(destructor_union("U8VecDestructor", "C"));
        ir.enums.push(destructor_union("StringVecDestructor", "C"));
        ir.enums
            .push(destructor_union("IcuStringVecDestructor", "C, u8"));
        ir.functions.push(deep_copy_fn("String"));
        ir
    }

    /// The element type carries a `_clone` export, so every Vec over it
    /// wants the `az_<elem>_clone_via_internal` foreign import. Two such
    /// Vec structs must still produce exactly ONE declaration — GHC
    /// rejects a repeat with GHC-29916 "Multiple declarations of ...".
    ///
    /// Regression guard for `StringVec` + `IcuStringVec` (both over
    /// `String`), which broke the Haskell binding build.
    fn emit_two_vecs_over_same_elem() -> String {
        let ir = fixture_ir();
        let config = CodegenConfig::c_header();
        let mut builder = CodeBuilder::new(&config.indent);
        emit_type_decls(&mut builder, &ir, &config).expect("emit must succeed");
        builder.finish()
    }

    /// Extract every `foreign import` binding name from Haskell source.
    fn foreign_import_names(src: &str) -> Vec<String> {
        let lines: Vec<&str> = src.lines().collect();
        let mut names = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.trim_start().starts_with("foreign import") {
                continue;
            }
            let Some(next) = lines.get(i + 1) else {
                continue;
            };
            let t = next.trim_start();
            let end = t
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\''))
                .unwrap_or(t.len());
            names.push(t[..end].to_string());
        }
        names
    }

    #[test]
    fn vec_clone_via_foreign_import_emitted_once_per_module() {
        let src = emit_two_vecs_over_same_elem();

        let names = foreign_import_names(&src);
        let hits = names
            .iter()
            .filter(|n| n.as_str() == "az_azString_clone_via_internal")
            .count();
        assert_eq!(
            hits, 1,
            "the shared clone-via foreign import must be declared exactly once per module, got {} \
             declaration(s) in:\n{}",
            hits, src
        );

        // ...and no OTHER foreign import may repeat either.
        let mut sorted = names.clone();
        sorted.sort();
        let unique = {
            let mut u = sorted.clone();
            u.dedup();
            u
        };
        assert_eq!(sorted, unique, "duplicate foreign imports: {:?}", names);
    }

    #[test]
    fn every_vec_still_gets_its_own_to_list_helper() {
        // Deduplicating the shared foreign import must NOT skip the
        // per-Vec decoder.
        let src = emit_two_vecs_over_same_elem();
        assert!(
            src.contains("stringVecToList :: StringVec -> IO [AzString]"),
            "missing stringVecToList in:\n{}",
            src
        );
        assert!(
            src.contains("icuStringVecToList :: IcuStringVec -> IO [AzString]"),
            "missing icuStringVecToList in:\n{}",
            src
        );
        // The second Vec's decoder still calls the shared binding.
        assert_eq!(
            src.matches("az_azString_clone_via_internal __ep __out")
                .count(),
            2,
            "both decoders must call the shared clone-via binding:\n{}",
            src
        );
    }

    /// Every number a Storable instance uses is an import of the cbits
    /// oracle: sizes, alignments and — the half the old binding lacked —
    /// the member offsets, so padded fields are read where the C compiler
    /// put them instead of at an unpadded running sum.
    #[test]
    fn storable_layout_comes_from_the_oracle() {
        let src = emit_two_vecs_over_same_elem();
        let u8vec = src
            .split("instance Storable U8Vec where")
            .nth(1)
            .expect("U8Vec instance");
        assert!(
            u8vec.contains("sizeOf _ = fromIntegral c_az_hs_sizeof_u8Vec"),
            "U8Vec size:\n{}",
            u8vec
        );
        assert!(
            u8vec.contains(
                "peek p = U8Vec <$> peekByteOff p (fromIntegral c_az_hs_offsetof_u8Vec_ptr) <*> \
                 peekByteOff p (fromIntegral c_az_hs_offsetof_u8Vec_len) <*> peekByteOff p \
                 (fromIntegral c_az_hs_offsetof_u8Vec_cap) <*> peekByteOff p (fromIntegral \
                 c_az_hs_offsetof_u8Vec_destructor)"
            ),
            "U8Vec offsets:\n{}",
            u8vec
        );
        assert!(
            src.contains("foreign import ccall unsafe \"az_hs_offsetof_U8Vec_destructor\""),
            "oracle import for the destructor offset:\n{}",
            src
        );
        let dtor = src
            .split("instance Storable U8VecDestructor where")
            .nth(1)
            .expect("U8VecDestructor instance");
        assert!(
            dtor.contains("tag <- peek (castPtr p :: Ptr Word32)"),
            "a #[repr(C)] union reads an int-sized tag:\n{}",
            dtor
        );
        assert!(
            dtor.contains(
                "1 -> U8VecDestructor_External <$> peekByteOff p (fromIntegral \
                 c_az_hs_offsetof_u8VecDestructor_External_payload)"
            ),
            "destructor payload offset:\n{}",
            dtor
        );
        let dtor_u8 = src
            .split("instance Storable IcuStringVecDestructor where")
            .nth(1)
            .expect("IcuStringVecDestructor instance");
        assert!(
            dtor_u8.contains("tag <- peek (castPtr p :: Ptr Word8)"),
            "a #[repr(C, u8)] union reads a byte tag:\n{}",
            dtor_u8
        );
        assert!(
            !src.contains("not implemented"),
            "no Storable instance may error out of peek/poke:\n{}",
            src
        );
    }

    /// The oracle table `cshim.rs` emits C definitions from must list
    /// exactly the symbols `Azul.Types` imports.
    #[test]
    fn oracle_table_matches_the_imports() {
        let ir = fixture_ir();
        let config = CodegenConfig::c_header();
        let src = emit_two_vecs_over_same_elem();
        let table = layout_oracle(&ir, &config);
        let mut symbols: Vec<String> = Vec::new();
        for t in &table {
            symbols.push(format!("az_hs_sizeof_{}", t.ir_name));
            symbols.push(format!("az_hs_alignof_{}", t.ir_name));
            for m in &t.members {
                symbols.push(format!("az_hs_offsetof_{}_{}", t.ir_name, m.suffix));
            }
        }
        for sym in &symbols {
            assert!(
                src.contains(&format!("\"{}\"", sym)),
                "Azul.Types never imports oracle symbol {}",
                sym
            );
        }
        let imported = src.matches("foreign import ccall unsafe \"az_hs_").count();
        assert_eq!(
            imported,
            symbols.len(),
            "every oracle import must have a definition in the table"
        );
    }

    #[test]
    fn duplicate_declaration_guard_rejects_the_regressed_output() {
        // The module-scope guard in `mod.rs` is what turns a re-introduced
        // duplicate into a codegen failure instead of a GHC-29916 error
        // three CI jobs later. Feed it the exact shape this bug produced.
        let regressed = concat!(
            "module Azul.Types where\n",
            "\n",
            "foreign import ccall unsafe \"AzString_clone_via\"\n",
            "    az_azString_clone_via_internal :: Ptr AzString -> Ptr AzString -> IO ()\n",
            "\n",
            "foreign import ccall unsafe \"AzString_clone_via\"\n",
            "    az_azString_clone_via_internal :: Ptr AzString -> Ptr AzString -> IO ()\n",
        );
        let err = super::super::check_no_duplicate_declarations("src/Azul/Types.hs", regressed)
            .expect_err("the guard must reject a duplicated declaration");
        let msg = err.to_string();
        assert!(
            msg.contains("az_azString_clone_via_internal"),
            "error must name the offending binding, got: {}",
            msg
        );
    }

    #[test]
    fn duplicate_declaration_guard_accepts_the_fixed_output() {
        let src = emit_two_vecs_over_same_elem();
        super::super::check_no_duplicate_declarations("src/Azul/Types.hs", &src)
            .expect("deduplicated output must pass the module-scope guard");
    }

    #[test]
    fn duplicate_declaration_guard_ignores_non_declarations() {
        // Record fields, inline `::` annotations and Haddock prose must
        // not be mistaken for module-scope declarations, or the guard
        // would fire on perfectly valid output.
        let src = concat!(
            "{- |\n",
            "Prose that mentions foo :: Int and foo :: Int again.\n",
            "-}\n",
            "{-# LANGUAGE ForeignFunctionInterface #-}\n",
            "module Azul.Types where\n",
            "\n",
            "data A = A\n",
            "    { aPtr :: !(Ptr ())\n",
            "    , aLen :: !(CSize)\n",
            "    } deriving (Show)\n",
            "\n",
            "data B = B\n",
            "    { aPtr :: !(Ptr ())\n",
            "    , aLen :: !(CSize)\n",
            "    } deriving (Show)\n",
            "\n",
            "instance Storable A where\n",
            "    peek p = do\n",
            "        v0 <- peekByteOff p (0) :: IO (Ptr ())\n",
            "        v1 <- peekByteOff p (sizeOf (undefined :: (Ptr ()))) :: IO (CSize)\n",
            "        pure (A v0 v1)\n",
        );
        super::super::check_no_duplicate_declarations("src/Azul/Types.hs", src)
            .expect("valid output must not trip the guard");
    }
}

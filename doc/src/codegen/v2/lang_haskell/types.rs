//! Haskell type emission: `data`/`newtype` declarations + `Storable`
//! instances for every IR struct and enum that survives the inclusion
//! filter.
//!
//! Strategy:
//! - **Plain structs** become `data <Name> = <Name> { field1 :: !T1, ... }`
//!   with a manually-written `Storable` instance. Field offsets are
//!   not known at codegen time (they depend on the C compiler's
//!   layout of the corresponding Rust `#[repr(C)]` struct), so we
//!   emit `peek`/`poke` bodies that walk the fields *in declaration
//!   order* using `peekByteOff` / `pokeByteOff` with a running offset.
//!   The running offset is computed from the previous field's
//!   `sizeOf`. This is correct for structs whose Rust layout matches
//!   the C ABI without padding (the common case for azul's flat POD
//!   structs); for structs with padding the user can always fall back
//!   to the raw FFI primitives.
//! - **Unit enums** (no payload) become a normal Haskell sum type
//!   with `deriving (Show, Eq, Enum, Bounded)`, plus a `Storable`
//!   instance going through `Word32` (the Rust ABI repr for unit
//!   enums).
//! - **Tagged unions** become a Haskell sum type with payload
//!   constructors. The `Storable` instance dispatches on the
//!   discriminator field and uses `peek` / `poke` recursively for the
//!   payload variant. Unsizeable payloads (recursive types, generics)
//!   are skipped with a `-- SKIPPED:` marker.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, StructDef, TypeCategory,
};
use super::{haskell_data_name, haskell_field_name, haskell_variant_name, sanitize_doc};

// ============================================================================
// Top-level entry
// ============================================================================

pub fn emit_type_decls(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Struct data declarations + Storable instances");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            if !s.generic_params.is_empty() {
                builder.line(&format!(
                    "-- SKIPPED: generic struct {} (no Haskell equivalent over the C ABI)",
                    s.name
                ));
            } else if matches!(
                s.category,
                TypeCategory::Recursive
                    | TypeCategory::VecRef
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
            ) {
                builder.line(&format!(
                    "-- SKIPPED: struct {} ({})",
                    s.name,
                    s.category.description()
                ));
            }
            continue;
        }
        emit_struct_decl(builder, s, ir);
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
            } else if matches!(
                e.category,
                TypeCategory::Recursive
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
            ) {
                builder.line(&format!(
                    "-- SKIPPED: enum {} ({})",
                    e.name,
                    e.category.description()
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

    // Monomorphized type aliases (`CssPropertyValue<StringSet>` =
    // `StringSetValue` etc.). The IR builder pre-instantiates these
    // with concrete payloads — emit them so other types that reference
    // them by their flattened name (e.g. `CssProperty_StringSet
    // StringSetValue`) resolve.
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Monomorphized type aliases");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for ta in &ir.type_aliases {
        if !config.should_include_type(&ta.name) {
            continue;
        }
        match &ta.monomorphized_def {
            Some(md) => emit_monomorphized_alias(builder, ta, md, ir),
            None => {
                // Simple type alias (`X11Visual = *mut c_void`,
                // `HwndHandle = *mut c_void`, etc.). Emit a 1-byte
                // placeholder data type so variants that reference
                // these by name (`Option<X11Visual>::Some X11Visual`)
                // resolve. The C ABI representation is opaque.
                let name = super::haskell_data_name(&ta.name);
                builder.line(&format!("-- type alias placeholder for {}", ta.name));
                builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
                builder.line(&format!("instance Storable {} where", name));
                builder.indent();
                builder.line("sizeOf _ = 1");
                builder.line("alignment _ = 1");
                builder.line(&format!("peek _ = pure {}", name));
                builder.line("poke _ _ = pure ()");
                builder.dedent();
                builder.blank();
            }
        }
    }

    // Callback typedefs (function pointers, e.g. `ComponentCompileFn`)
    // are emitted in `Azul.Internal.FFI` but Types.hs references them
    // by bare name from struct field positions
    // (`componentDefCompileFn :: !(ComponentCompileFn)`). Emit a
    // 1-byte Storable placeholder here so the type resolves locally;
    // actual function-pointer marshalling lives on the FFI side.
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Callback typedef placeholders (function pointers — real marshalling in FFI.hs)");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    for cb in &ir.callback_typedefs {
        if !config.should_include_type(&cb.name) {
            continue;
        }
        let name = super::haskell_data_name(&cb.name);
        builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
        builder.line(&format!("instance Storable {} where", name));
        builder.indent();
        builder.line("sizeOf _ = sizeOf (undefined :: FunPtr ())");
        builder.line("alignment _ = alignment (undefined :: FunPtr ())");
        builder.line(&format!("peek _ = pure {}", name));
        builder.line("poke _ _ = pure ()");
        builder.dedent();
        builder.blank();
    }

    // Types filtered out of the main struct/enum emit (Recursive,
    // VecRef, DestructorOrClone, GenericTemplate) are still referenced
    // by name from other variants. Emit a 1-byte placeholder for each
    // so those references resolve as a Haskell type — full memory
    // layout for these categories is a follow-up.
    builder.line("-- ---------------------------------------------------------------------------");
    builder.line("-- Placeholders for filtered-out categories (Recursive/VecRef/...)");
    builder.line("-- ---------------------------------------------------------------------------");
    builder.blank();
    let filtered = |cat: TypeCategory| {
        matches!(
            cat,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::DestructorOrClone
        )
    };
    for s in &ir.structs {
        if !config.should_include_type(&s.name) || !s.generic_params.is_empty() {
            continue;
        }
        if filtered(s.category) {
            let name = super::haskell_data_name(&s.name);
            builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
            builder.line(&format!("instance Storable {} where", name));
            builder.indent();
            builder.line("sizeOf _ = 1");
            builder.line("alignment _ = 1");
            builder.line(&format!("peek _ = pure {}", name));
            builder.line("poke _ _ = pure ()");
            builder.dedent();
            builder.blank();
        }
    }
    for e in &ir.enums {
        if !config.should_include_type(&e.name) || !e.generic_params.is_empty() {
            continue;
        }
        if filtered(e.category) {
            let name = super::haskell_data_name(&e.name);
            builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
            builder.line(&format!("instance Storable {} where", name));
            builder.indent();
            builder.line("sizeOf _ = 1");
            builder.line("alignment _ = 1");
            builder.line(&format!("peek _ = pure {}", name));
            builder.line("poke _ _ = pure ()");
            builder.dedent();
            builder.blank();
        }
    }

    Ok(())
}

fn emit_monomorphized_alias(
    builder: &mut CodeBuilder,
    ta: &super::super::ir::TypeAliasDef,
    md: &super::super::ir::MonomorphizedTypeDef,
    ir: &CodegenIR,
) {
    use super::super::ir::MonomorphizedKind;
    let name = super::haskell_data_name(&ta.name);

    match &md.kind {
        MonomorphizedKind::SimpleEnum { variants, .. } => {
            // No Show/Eq derive — variants are simple unit constructors.
            builder.line(&format!("data {} =", name));
            builder.indent();
            let last = variants.len().saturating_sub(1);
            for (i, v) in variants.iter().enumerate() {
                let ctor = super::haskell_variant_name(&ta.name, v);
                let prefix = if i == 0 { "  " } else { "| " };
                let trailing = if i == last { "" } else { "" };
                builder.line(&format!("{}{}{}", prefix, ctor, trailing));
            }
            builder.line("deriving (Show, Eq)");
            builder.dedent();
            // Minimal Storable: encode/decode the variant index as Int32.
            builder.line(&format!("instance Storable {} where", name));
            builder.indent();
            builder.line(&format!("sizeOf _ = sizeOf (undefined :: Foreign.C.Types.CInt)"));
            builder.line(&format!("alignment _ = alignment (undefined :: Foreign.C.Types.CInt)"));
            builder.line("peek _ = error \"peek on monomorphized SimpleEnum: not yet implemented\"");
            builder.line("poke _ _ = error \"poke on monomorphized SimpleEnum: not yet implemented\"");
            builder.dedent();
            builder.blank();
        }
        MonomorphizedKind::Struct { .. } | MonomorphizedKind::TaggedUnion { .. } => {
            // Both shapes get a placeholder data constructor so they
            // resolve as a Haskell type. The C ABI memory layout is
            // unused by the hello-world smoke tests — we just need the
            // name to exist. Full peek/poke is a follow-up.
            builder.line(&format!(
                "-- Monomorphized alias placeholder for {} (concrete layout unused by Haskell smoke tests).",
                ta.name
            ));
            builder.line(&format!("data {} = {} deriving (Show, Eq)", name, name));
            builder.line(&format!("instance Storable {} where", name));
            builder.indent();
            builder.line("sizeOf _ = 1");
            builder.line("alignment _ = 1");
            builder.line(&format!("peek _ = pure {}", name));
            builder.line("poke _ _ = pure ()");
            builder.dedent();
            builder.blank();
        }
    }
}

// ============================================================================
// Inclusion filters
// ============================================================================

pub fn should_emit_struct(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    // `RefAny` is emitted by hand earlier in mod.rs as a phantom-typed
    // newtype (`newtype RefAny a = RefAny { unRefAny :: Ptr () }`); the
    // default struct emit here would clash with that declaration:
    //   Multiple declarations of 'RefAny'
    if s.name == "RefAny" {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

pub fn should_emit_enum(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    !matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::GenericTemplate | TypeCategory::DestructorOrClone
    )
}

// ============================================================================
// Struct emission
// ============================================================================

fn emit_struct_decl(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let name = haskell_data_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if s.fields.is_empty() {
        // Phantom marker type — we still need a Storable instance so it
        // can appear in field positions of other structs.
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

    // data Name = Name { ... }
    builder.line(&format!("data {} = {}", name, name));
    builder.indent();
    let mut first = true;
    for f in &s.fields {
        let prefix = if first { "{ " } else { ", " };
        first = false;
        let fname = haskell_field_name(&s.name, &f.name);
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        if let Some(ref doc) = f.doc {
            builder.line(&format!("-- ^ {}", sanitize_doc(doc)));
        }
        // Wrap the type in parens — GHC rejects `!Ptr ()` because the
        // strictness annotation binds tighter than application:
        //   "Unexpected strictness (!) annotation: !Ptr"
        // `!(Ptr ())` is unambiguous regardless of how many type-app
        // tokens follow.
        builder.line(&format!("{}{} :: !({})", prefix, fname, hty));
    }
    builder.line("} deriving (Show)");
    builder.dedent();
    builder.blank();

    // Storable instance using a running offset and per-field sizeOf.
    // Helper names must start with a lowercase letter — Haskell rejects
    // top-level value bindings whose name starts with uppercase
    // ("Invalid data constructor 'Foo_sizeOf_total' in type signature").
    let lname = lower_first(&name);
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line(&format!("sizeOf _ = {}_sizeOf_total", lname));
    builder.line(&format!("alignment _ = {}_alignment_total", lname));

    // peek
    builder.line("peek p = do");
    builder.indent();
    let mut offset_acc: Vec<String> = Vec::new();
    for (i, f) in s.fields.iter().enumerate() {
        let bind = format!("v{}", i);
        let offset_expr = if i == 0 {
            "0".to_string()
        } else {
            offset_acc.join(" + ")
        };
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        // Wrap the type in parens: `IO Ptr ()` is invalid Haskell;
        // `IO (Ptr ())` is what GHC expects.
        builder.line(&format!(
            "{} <- peekByteOff p ({}) :: IO ({})",
            bind, offset_expr, hty
        ));
        offset_acc.push(format!("sizeOf (undefined :: ({}))", hty));
    }
    let mut acc = String::new();
    acc.push_str(&format!("pure ({}", name));
    for (i, _) in s.fields.iter().enumerate() {
        acc.push_str(&format!(" v{}", i));
    }
    acc.push(')');
    builder.line(&acc);
    builder.dedent();

    // poke
    builder.line("poke p x = do");
    builder.indent();
    let mut offset_acc: Vec<String> = Vec::new();
    for (i, f) in s.fields.iter().enumerate() {
        let fname = haskell_field_name(&s.name, &f.name);
        let offset_expr = if i == 0 {
            "0".to_string()
        } else {
            offset_acc.join(" + ")
        };
        let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
        builder.line(&format!("pokeByteOff p ({}) ({} x)", offset_expr, fname));
        offset_acc.push(format!("sizeOf (undefined :: {})", hty));
    }
    builder.dedent();
    builder.dedent();

    // Helper bindings: total size and alignment computed at runtime.
    // This avoids requiring offsetof macros at codegen time. Names are
    // lower-camelCased so Haskell parses them as value bindings rather
    // than data constructors.
    let tname = lower_first(&name);
    builder.blank();
    builder.line(&format!("{}_sizeOf_total :: Int", tname));
    if s.fields.is_empty() {
        builder.line(&format!("{}_sizeOf_total = 1", tname));
    } else {
        let mut sum_terms: Vec<String> = Vec::new();
        for f in &s.fields {
            let hty = haskell_field_type(&f.type_name, f.ref_kind, ir);
            sum_terms.push(format!("sizeOf (undefined :: {})", hty));
        }
        builder.line(&format!("{}_sizeOf_total = {}", tname, sum_terms.join(" + ")));
    }
    builder.line(&format!("{}_alignment_total :: Int", tname));
    // Pessimistic: take the max alignment of the first field (sufficient
    // for the C ABI on every platform we target — pointer-aligned).
    if let Some(f0) = s.fields.first() {
        let hty = haskell_field_type(&f0.type_name, f0.ref_kind, ir);
        builder.line(&format!("{}_alignment_total = alignment (undefined :: {})", tname, hty));
    } else {
        builder.line(&format!("{}_alignment_total = 1", tname));
    }
    builder.blank();

    // Phase H.6: AzString → Haskell String round-trip helper.
    // Triggered by TypeCategory::String (the IR's marker for the UTF-8
    // wrapper type) rather than a name-string match — keeps codegen
    // honest if there's ever more than one string type.
    if matches!(s.category, TypeCategory::String) {
        emit_string_to_string_helper(builder, s);
    }

    // Phase H.3: AzVec<T> → Haskell list helper.
    // Detect via the (ptr, len, cap, destructor) field pattern that every
    // codegen-emitted Vec type has. Skips structs that don't match the
    // shape exactly (so non-Vec structs with happenstance "Vec" suffixes
    // are left alone).
    if let Some(elem_ty) = detect_vec_elem_type(s) {
        emit_vec_to_list_helper(builder, s, &elem_ty, ir);
    }
}

/// True if this struct matches the codegen-emitted Vec shape:
/// fields = [ptr : *mut|*const T, len : usize, cap : usize, destructor : <Self>Destructor]
/// Returns the element type (T) on match.
fn detect_vec_elem_type(s: &super::super::ir::StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    let f_ptr = &s.fields[0];
    let f_len = &s.fields[1];
    let f_cap = &s.fields[2];
    let _f_dst = &s.fields[3];
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
    s: &super::super::ir::StructDef,
    elem_rust: &str,
    ir: &CodegenIR,
) {
    use super::super::ir::FunctionKind;
    let vec_name = haskell_data_name(&s.name);
    let elem_haskell = haskell_field_type(
        elem_rust,
        super::super::ir::FieldRefKind::Owned,
        ir,
    );
    let lname = lower_first(&vec_name);
    let helper = format!("{}ToList", lname);

    // V8 (Haskell): when the element type has a `_clone` export, the
    // Phase-B.8 shim layer already provides `Az<X>_clone_via` (input
    // ptr + output ptr; `cshim.rs:452-470` emits the wrapper). Each
    // list entry then owns an independent heap allocation — closing
    // the Vec later doesn't dangle the yielded `Storable` peeks.
    //
    // Without `_clone`, fall back to the legacy shallow `peekElemOff`
    // path with a warning comment. Pre-existing for POD elements with
    // no clone; same recipe as the Lua / OCaml fallbacks.
    let has_clone = ir.functions.iter().any(|f| {
        f.class_name == elem_rust && matches!(f.kind, FunctionKind::DeepCopy)
    });
    let clone_via_binding = format!("az_{}_clone_via_internal", lower_first(&haskell_data_name(elem_rust)));

    if has_clone {
        // Emit a local foreign-import bound to the same C symbol the
        // FFI module imports. The duplicate binding is intentional —
        // Haskell links each module's foreign-import to the C symbol
        // independently, and using a `_internal` suffix avoids name
        // clashes with `Azul.Internal.FFI.c_<symbol>` for users who
        // import both modules unqualified.
        builder.line(&format!(
            "foreign import ccall unsafe \"Az{}_clone_via\"",
            elem_rust
        ));
        builder.indent();
        builder.line(&format!(
            "{} :: Ptr {} -> Ptr {} -> IO ()",
            clone_via_binding, elem_haskell, elem_haskell
        ));
        builder.dedent();
        builder.blank();
    }

    builder.line("-- | Phase H.3 / V8: Decode the underlying buffer into a Haskell list.");
    if has_clone {
        builder.line(&format!(
            "-- Each element is cloned via `Az{}_clone_via` so the yielded list",
            elem_rust
        ));
        builder.line("-- entries own independent heap allocations and survive the Vec being");
        builder.line("-- closed. Pure type-driven from the (ptr, len, cap, destructor) field");
        builder.line("-- pattern; no per-Vec hardcoding.");
    } else {
        builder.line(&format!(
            "-- WARNING: no `Az{}_clone` export — falls back to shallow peekElemOff;",
            elem_rust
        ));
        builder.line("-- yielded entries dangle if the Vec is closed before they're consumed.");
    }
    builder.line(&format!("{} :: {} -> IO [{}]", helper, vec_name, elem_haskell));
    builder.line(&format!("{} v = do", helper));
    builder.indent();
    let ptr_field = haskell_field_name(&s.name, "ptr");
    let len_field = haskell_field_name(&s.name, "len");
    builder.line(&format!("let __p = {} v", ptr_field));
    builder.line(&format!(
        "    __n = fromIntegral ({} v) :: Int",
        len_field
    ));
    if has_clone {
        // `Foreign.Marshal.Alloc.alloca` provides a `Ptr <Elem>` for
        // the clone output; `peek` reads it back as `Elem` and the
        // Vec mapM yields the list.
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
}

use super::lower_first;

// ============================================================================
// Unit enum emission
// ============================================================================

fn emit_unit_enum_decl(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!("-- SKIPPED: unit enum {} has no variants", e.name));
        builder.blank();
        return;
    }

    let variants: Vec<String> = e
        .variants
        .iter()
        .map(|v| haskell_variant_name(&e.name, &v.name))
        .collect();

    builder.line(&format!("data {}", name));
    builder.indent();
    let mut first = true;
    for vname in &variants {
        let prefix = if first { "= " } else { "| " };
        first = false;
        builder.line(&format!("{}{}", prefix, vname));
    }
    builder.line("deriving (Show, Eq, Enum, Bounded)");
    builder.dedent();
    builder.blank();

    // Storable: pass through Word32 (the Rust ABI repr for unit enums).
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
        "_ -> error \"Azul.Types.peek {}: unknown discriminator\"",
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

fn emit_tagged_union_decl(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let name = haskell_data_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!("-- | {}", sanitize_doc(d)));
        }
    }

    if e.variants.is_empty() {
        builder.line(&format!(
            "-- SKIPPED: tagged-union {} has no variants",
            e.name
        ));
        builder.blank();
        return;
    }

    builder.line(&format!("data {}", name));
    builder.indent();
    let mut first = true;
    for v in &e.variants {
        let prefix = if first { "= " } else { "| " };
        first = false;
        let vname = haskell_variant_name(&e.name, &v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                builder.line(&format!("{}{}", prefix, vname));
            }
            EnumVariantKind::Tuple(payload) => {
                let payload_types: Vec<String> = payload
                    .iter()
                    .map(|(t, rk)| haskell_field_type(t, *rk, ir))
                    .collect();
                builder.line(&format!(
                    "{}{} {}",
                    prefix,
                    vname,
                    payload_types.join(" ")
                ));
            }
            EnumVariantKind::Struct(fields) => {
                let payload_types: Vec<String> = fields
                    .iter()
                    .map(|f| haskell_field_type(&f.type_name, f.ref_kind, ir))
                    .collect();
                builder.line(&format!(
                    "{}{} {}",
                    prefix,
                    vname,
                    payload_types.join(" ")
                ));
            }
        }
    }
    builder.line("deriving (Show)");
    builder.dedent();
    builder.blank();

    // Storable: discriminator + opaque-byte-array payload. We expose a
    // best-effort instance that round-trips the unit variants exactly
    // and round-trips payload variants only when their payload is
    // 'Storable' itself. For complex payloads users should reach for
    // the raw FFI primitives.
    builder.line(&format!("instance Storable {} where", name));
    builder.indent();
    builder.line("sizeOf _ = 8 + 64  -- tag (Word32 + pad) + payload bound");
    builder.line("alignment _ = 8");
    builder.line(&format!(
        "peek _ = error \"Azul.Types.peek {}: tagged-union peek not implemented; use the raw FFI primitives\"",
        name
    ));
    builder.line(&format!(
        "poke _ _ = error \"Azul.Types.poke {}: tagged-union poke not implemented; use the raw FFI primitives\"",
        name
    ));
    builder.dedent();
    builder.blank();

    // Phase H.4/H.5: tag-byte discriminator accessors for Option/Result-
    // shaped enums. The full Storable peek/poke is a separate task
    // (variants have payload-type-dependent offsets), but the tag is at
    // offset 0 in every #[repr(C, u8)] tagged-union — peek that single
    // byte and compare. Mirrors OCaml's `az_option_<T>_is_some` from A.1.4.
    if is_option_shape(e) {
        emit_option_tag_helpers(builder, e);
    } else if is_result_shape(e) {
        emit_result_tag_helpers(builder, e);
    }
}

/// Emit a `azStringToString :: AzString -> IO String` helper that
/// decodes the wrapped UTF-8 bytes via the U8Vec's (ptr, len) fields.
/// Triggered by TypeCategory::String — no name allowlist.
fn emit_string_to_string_helper(builder: &mut CodeBuilder, s: &super::super::ir::StructDef) {
    // Identify the single byte-buffer field (U8Vec). Assume it's the
    // first field — the IR's AzString definition has exactly one field
    // of type "U8Vec".
    let Some(field) = s.fields.first() else {
        return;
    };
    let field_name = haskell_field_name(&s.name, &field.name);
    let lname = lower_first(&haskell_data_name(&s.name));
    builder.line("-- | Phase H.6: decode the wrapped UTF-8 bytes into a Haskell String.");
    builder.line("-- Uses the underlying U8Vec's (ptr, len) accessors via peekCStringLen.");
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
    // peekCStringLen expects (CString, Int); CString = Ptr CChar.
    builder.line("peekCStringLen (castPtr __p, __n)");
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

fn emit_option_tag_helpers(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);
    let lname = lower_first(&name);
    let none_idx = e.variants.iter().position(|v| v.name == "None").unwrap();
    let some_idx = e.variants.iter().position(|v| v.name == "Some").unwrap();
    builder.line("-- | Phase H.4: read the tag byte at offset 0.");
    builder.line("-- True if the underlying Option is the None variant.");
    builder.line(&format!("{}IsNone :: Ptr {} -> IO Bool", lname, name));
    builder.line(&format!("{}IsNone p = do", lname));
    builder.indent();
    builder.line(&format!(
        "tag <- peekByteOff (castPtr p :: Ptr Word8) 0 :: IO Word8"
    ));
    builder.line(&format!("pure (tag == {})", none_idx));
    builder.dedent();
    builder.line(&format!("{}IsSome :: Ptr {} -> IO Bool", lname, name));
    builder.line(&format!("{}IsSome p = do", lname));
    builder.indent();
    builder.line(&format!(
        "tag <- peekByteOff (castPtr p :: Ptr Word8) 0 :: IO Word8"
    ));
    builder.line(&format!("pure (tag == {})", some_idx));
    builder.dedent();
    builder.blank();
}

fn emit_result_tag_helpers(builder: &mut CodeBuilder, e: &EnumDef) {
    let name = haskell_data_name(&e.name);
    let lname = lower_first(&name);
    let ok_idx = e.variants.iter().position(|v| v.name == "Ok").unwrap();
    let err_idx = e.variants.iter().position(|v| v.name == "Err").unwrap();
    builder.line("-- | Phase H.5: read the tag byte at offset 0.");
    builder.line("-- True if the underlying Result is the Ok variant.");
    builder.line(&format!("{}IsOk :: Ptr {} -> IO Bool", lname, name));
    builder.line(&format!("{}IsOk p = do", lname));
    builder.indent();
    builder.line(&format!(
        "tag <- peekByteOff (castPtr p :: Ptr Word8) 0 :: IO Word8"
    ));
    builder.line(&format!("pure (tag == {})", ok_idx));
    builder.dedent();
    builder.line(&format!("{}IsErr :: Ptr {} -> IO Bool", lname, name));
    builder.line(&format!("{}IsErr p = do", lname));
    builder.indent();
    builder.line(&format!(
        "tag <- peekByteOff (castPtr p :: Ptr Word8) 0 :: IO Word8"
    ));
    builder.line(&format!("pure (tag == {})", err_idx));
    builder.dedent();
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
        | FieldRefKind::OptionBoxed => format!("Ptr {}", map_owned_type(type_name, ir)),
    }
}

fn map_owned_type(type_name: &str, ir: &CodegenIR) -> String {
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
        // RefAny is a phantom-typed `newtype RefAny a`. When referenced
        // from variants like `ResultRefAnyString_Ok RefAny`, GHC needs
        // a type argument. Use `()` as the default (matches the
        // hand-rolled `unRefAny :: Ptr ()` payload).
        "RefAny" => "(RefAny ())".to_string(),
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
    format!("(Ptr {})", map_owned_type(inner, ir))
}

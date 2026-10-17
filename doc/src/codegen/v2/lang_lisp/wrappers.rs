//! Idiomatic CLOS-wrapper emission for the Common Lisp generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit:
//!
//! - A `(defclass <name> (azul-handle) ((ptr :initarg :ptr :accessor <name>-ptr)))`. The `ptr`
//!   slot holds a FOREIGN POINTER to a buffer of the wrapper's own that stores the value: every
//!   by-value return of a wrapped class is boxed into such a buffer (see
//!   [`emit_internal_boxing`]), so `&self` methods can hand the pointer straight to C and by-value
//!   arguments are copied out of it.
//! - A `(defmethod close-<name> ((obj <name>)))` that calls the matching `%az-<name>-delete`, frees
//!   the buffer and nulls out the pointer slot. CL has no RAII; users invoke this manually or via
//!   the macro below.
//! - A `(defmacro with-<name> ((var ...) &body body) ...)` that wraps the constructor call in
//!   `unwind-protect` so the close method runs on non-local exit.
//! - Idiomatic functions:
//!   - `(make-<name> ...)` for `Constructor` / `Default`.
//!   - `(<name>-<method> obj ...)` for `Method` / `MethodMut`.
//!   - `(<name>-<method> ...)` for `StaticMethod`.
//!
//!   A wrapper passed BY VALUE is moved: after the call its buffer is freed and its pointer
//!   nulled, so a later `close-<name>` is a no-op instead of a double free.
//! - Field accessors `(<name>-<field> obj)` / `(setf (<name>-<field> obj) v)` for every public
//!   by-value field (see [`emit_field_accessors`]).
//!
//! Plain POD types without a `_delete` get no CLOS wrapper; users
//! manipulate them through `cffi:foreign-slot-value` directly.
//!
//! Tagged unions get a minimal helper layer: a `tag` reader and per
//! unit-variant constructor functions. Payload-bearing variants are
//! left to the user (they can construct the FFI struct via
//! `cffi:foreign-alloc` / `with-foreign-object`).

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FieldDef, FieldRefKind, FunctionArg,
            FunctionDef, FunctionKind, StructDef, TypeCategory,
        },
        managed_host_invoker, managed_lang_helpers,
    },
    functions::should_emit_function,
    ident_to_kebab, idiomatic_class_name, map_type_to_cffi, raw_fn_name, to_kebab_case,
};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Idiomatic CLOS wrappers (in :azul package).");
    builder.line(";;");
    builder.line(";; Conventions:");
    builder.line(";;   (make-foo ...)        -- constructor");
    builder.line(";;   (foo-method obj ...)  -- instance method");
    builder.line(";;   (close-foo obj)       -- explicit destructor; nulls the pointer");
    builder.line(";;   (with-foo (var ...) body...) -- unwind-protect helper");
    builder.line(";;   (foo-field obj) / (setf (foo-field obj) v) -- field accessors");
    builder.line(";; A wrapper passed by value is MOVED: it is closed for you after the call.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    let lx = Lx::new(ir, config);
    emit_handle_runtime(builder, &lx);

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_struct_wrapper(builder, s, &lx);
    }

    // Tagged-union enums: minimal helpers (tag reader + unit constructors).
    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e);
    }

    Ok(())
}

// =============================================================================
// Inclusion filters
// =============================================================================

pub(super) fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    has_delete_function(&s.name, ir)
}

fn should_emit_union_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    if matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::DestructorOrClone | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    e.is_union
}

fn has_delete_function(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class_name && f.kind == FunctionKind::Delete)
}

// =============================================================================
// Shared generator context
// =============================================================================

/// What the wrapper emitters need to know beyond one struct.
struct Lx<'a> {
    ir: &'a CodegenIR,
    config: &'a CodegenConfig,
    /// The IR String class, when it is wrapped and has a byte constructor
    /// and a `_delete`: `(class, copy-from-bytes raw fn, inner vec slot,
    /// data-pointer slot, length slot, inner vec struct)`.
    string: Option<StringInfo>,
}

struct StringInfo {
    class: String,
    from_bytes: String,
    vec_slot: String,
    vec_struct: String,
    ptr_slot: String,
    len_slot: String,
}

impl<'a> Lx<'a> {
    fn new(ir: &'a CodegenIR, config: &'a CodegenConfig) -> Self {
        let string = ir
            .structs
            .iter()
            .find(|s| s.category == TypeCategory::String)
            .filter(|s| should_emit_wrapper(s, ir, config))
            .and_then(|s| {
                // The byte constructor of the class in the IR's String
                // category. The IR has no `FunctionKind` for it - it is an
                // ordinary api.json constructor - so the method name is the
                // only handle there is.
                let ctor = ir
                    .functions_for_class(&s.name)
                    // allow-api-name: no kind or shape distinguishes this constructor.
                    .find(|f| f.method_name == "copy_from_bytes")?;
                if !should_emit_function(ctor, ir, config) {
                    return None;
                }
                // The bytes live in the String's one field (a byte Vec):
                // its pointer field and its `usize` length field.
                let vec = s.fields.first()?;
                let inner = ir.find_struct(vec.type_name.trim())?;
                let ptr = inner.fields.iter().find(|f| {
                    f.ref_kind != FieldRefKind::Owned || f.type_name.trim().starts_with('*')
                })?;
                let len = inner
                    .fields
                    .iter()
                    .find(|f| f.type_name.trim() == "usize")?;
                Some(StringInfo {
                    class: s.name.clone(),
                    from_bytes: raw_fn_name(&ctor.c_name),
                    vec_slot: ident_to_kebab(&vec.name),
                    vec_struct: to_kebab_case(&inner.name),
                    ptr_slot: ident_to_kebab(&ptr.name),
                    len_slot: ident_to_kebab(&len.name),
                })
            });
        Self { ir, config, string }
    }

    /// The struct behind `t` when it gets a CLOS wrapper class.
    fn wrapped(&self, t: &str) -> Option<&'a StructDef> {
        self.ir
            .find_struct(t.trim())
            .filter(|s| should_emit_wrapper(s, self.ir, self.config))
    }

    fn is_string(&self, t: &str) -> bool {
        self.string.as_ref().is_some_and(|s| s.class == t.trim())
    }

    /// The `%az-*` binding of `class`'s function of `kind`, when emitted.
    fn class_fn(&self, class: &str, kind: FunctionKind) -> Option<String> {
        self.ir
            .functions_for_class(class.trim())
            .find(|f| f.kind == kind && should_emit_function(f, self.ir, self.config))
            .map(|f| raw_fn_name(&f.c_name))
    }
}

/// Qualify a CFFI type expression produced by [`map_type_to_cffi`] (which
/// targets `:azul-internal`) for use from `:azul`.
fn qualify_cffi(expr: &str) -> String {
    if let Some(rest) = expr.strip_prefix("(:struct ") {
        format!("(:struct azul-internal::{}", rest)
    } else if let Some(rest) = expr.strip_prefix("(:union ") {
        format!("(:union azul-internal::{}", rest)
    } else if expr.starts_with(':') || expr.starts_with('(') {
        expr.to_string()
    } else {
        format!("azul-internal::{}", expr)
    }
}

// =============================================================================
// Runtime helpers shared by every wrapper
// =============================================================================

/// `azul-handle` (the base class), the move helpers and the String
/// conversions, emitted once into `:azul` before the first class.
fn emit_handle_runtime(builder: &mut CodeBuilder, lx: &Lx) {
    for line in [
        ";; Every wrapper object keeps its value in a foreign buffer of its own (PTR).",
        ";; A wrapper passed by value - as an argument or to a field setter - is MOVED:",
        ";; its buffer is freed and PTR nulled, so CLOSE on it is a harmless no-op.",
        "(defclass azul-handle ()",
        "  ((ptr :initarg :ptr :initform (cffi:null-pointer)))",
        "  (:documentation \"Base of every wrapper class. PTR is a foreign buffer holding the wrapped value, null once it was moved or closed.\"))",
        "(export 'azul-handle :azul)",
        "",
        "(defun %unwrap (x)",
        "  \"The foreign pointer of wrapper X; any other X (a foreign pointer, a plist) unchanged.\"",
        "  (if (typep x 'azul-handle) (slot-value x 'ptr) x))",
        "",
        "(defun %consume (x)",
        "  \"Marks wrapper X as moved: its bytes now belong to the callee, so only X's own buffer is freed and its close disarmed.\"",
        "  (when (typep x 'azul-handle)",
        "    (let ((p (slot-value x 'ptr)))",
        "      (when (and (cffi:pointerp p) (not (cffi:null-pointer-p p)))",
        "        (cffi:foreign-free p)))",
        "    (setf (slot-value x 'ptr) (cffi:null-pointer)))",
        "  nil)",
        "",
        "(defun %plist-keys (x to-internal)",
        "  \"A CFFI struct plist with its slot keys turned into keywords (or, with TO-INTERNAL, back into the :azul-internal slot names), recursively.\"",
        "  (if (and (consp x)",
        "           (handler-case (evenp (length x)) (error () nil))",
        "           (loop for k in x by #'cddr always (and k (symbolp k))))",
        "      (loop for (k v) on x by #'cddr",
        "            nconc (list (intern (symbol-name k) (if to-internal :azul-internal :keyword))",
        "                        (%plist-keys v to-internal)))",
        "      x))",
        "",
        "(defun %move-in (fp type v release)",
        "  \"Writes V into the TYPE field at FP: RELEASE (when non-nil) frees the old value first, then V's bytes move in and a wrapper V is marked moved.\"",
        "  (let ((src (%unwrap v)))",
        "    (when (and (cffi:pointerp src) (cffi:null-pointer-p src))",
        "      (error \"azul: ~S was already moved or closed\" v))",
        "    (when release (funcall release fp))",
        "    (azul-internal::%azul-store fp type src)",
        "    (%consume v)",
        "    nil))",
        "",
    ] {
        if line.is_empty() {
            builder.blank();
        } else {
            builder.line(line);
        }
    }

    let Some(st) = lx.string.as_ref() else {
        return;
    };
    let class = idiomatic_class_name(&st.class);
    let string_struct = to_kebab_case(&st.class);
    builder.line(&format!("(defun {}-from-lisp (s)", class));
    builder.line(&format!(
        "  \"A fresh {} holding a UTF-8 copy of the Lisp string S.\"",
        class.to_uppercase()
    ));
    builder.line(
        "  (multiple-value-bind (buf n) (cffi:foreign-string-alloc s :encoding :utf-8 :null-terminated-p nil)",
    );
    builder.line("    (unwind-protect");
    builder.line(&format!(
        "         (make-instance '{} :ptr (azul-internal::{} buf 0 n))",
        class, st.from_bytes
    ));
    builder.line("      (cffi:foreign-free buf))))");
    builder.line(&format!("(export '{}-from-lisp :azul)", class));
    builder.blank();
    builder.line("(defun %string-value (sp)");
    builder.line(&format!(
        "  \"The Lisp string decoded from the {} at SP; SP is only read, never consumed.\"",
        class.to_uppercase()
    ));
    builder.line(&format!(
        "  (let* ((vp (cffi:foreign-slot-pointer sp '(:struct azul-internal::{}) 'azul-internal::{}))",
        string_struct, st.vec_slot
    ));
    builder.line(&format!(
        "         (data (cffi:foreign-slot-value vp '(:struct azul-internal::{}) 'azul-internal::{}))",
        st.vec_struct, st.ptr_slot
    ));
    builder.line(&format!(
        "         (len (cffi:foreign-slot-value vp '(:struct azul-internal::{}) 'azul-internal::{})))",
        st.vec_struct, st.len_slot
    ));
    builder.line("    (if (or (zerop len) (cffi:null-pointer-p data))");
    builder.line("        \"\"");
    builder.line("        (cffi:foreign-string-to-lisp data :count len :encoding :utf-8))))");
    builder.blank();
    builder.line(&format!("(defun {}-to-lisp (s)", class));
    builder.line(&format!(
        "  \"The Lisp string held by the {} wrapper (or foreign pointer) S, which stays valid.\"",
        class.to_uppercase()
    ));
    builder.line("  (%string-value (%unwrap s)))");
    builder.line(&format!("(export '{}-to-lisp :azul)", class));
    builder.blank();
    builder.line("(defun %string-arg (v)");
    builder.line(&format!(
        "  \"V as something a {} parameter takes: a Lisp string becomes a fresh wrapper.\"",
        class.to_uppercase()
    ));
    builder.line(&format!("  (if (stringp v) ({}-from-lisp v) v))", class));
    builder.blank();
}

/// Emitted into `:azul-internal` after the bindings: the byte helpers and,
/// per wrapped class, the CFFI translation that makes a by-value value a
/// foreign pointer to a buffer of its own instead of a plist.
///
/// A plist cannot carry these values: it reads every overlapping variant of
/// every nested union (an inactive variant's bytes are not a valid enum or
/// bool) and writing it back rewrites those bytes. A buffer is copied
/// bit for bit.
pub fn emit_internal_boxing(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder
        .line(";;; ----------------------------------------------------------------------------");
    builder
        .line(";;; By-value wrapped classes travel as foreign pointers to a buffer of their own.");
    builder
        .line(";;; ----------------------------------------------------------------------------");
    builder.blank();
    for line in [
        "(defun %azul-copy-bytes (dst src n)",
        "  \"Copies N bytes from SRC to DST; returns DST.\"",
        "  (dotimes (i n dst)",
        "    (setf (mem-aref dst :uint8 i) (mem-aref src :uint8 i))))",
        "",
        "(defun %azul-box (src type)",
        "  \"A fresh foreign buffer holding a bitwise copy of the TYPE value at SRC.\"",
        "  (let ((n (foreign-type-size type)))",
        "    (%azul-copy-bytes (foreign-alloc :uint8 :count (max n 1)) src n)))",
        "",
        "(defun %azul-store (dst type value)",
        "  \"Writes VALUE - a foreign pointer to a TYPE value, or a CFFI plist - at DST.\"",
        "  (if (pointerp value)",
        "      (%azul-copy-bytes dst value (foreign-type-size type))",
        "      (setf (mem-ref dst type) value)))",
        "",
    ] {
        if line.is_empty() {
            builder.blank();
        } else {
            builder.line(line);
        }
    }
    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        let k = to_kebab_case(&s.name);
        builder.line(&format!(
            "(defmethod translate-from-foreign (p (type {}-tclass))",
            k
        ));
        builder.line(&format!("  (%azul-box p '(:struct {})))", k));
        builder.blank();
        builder.line(&format!(
            "(defmethod translate-into-foreign-memory (value (type {}-tclass) p)",
            k
        ));
        builder.line("  (if (pointerp value)");
        builder.line(&format!(
            "      (%azul-copy-bytes p value (foreign-type-size '(:struct {})))",
            k
        ));
        builder.line("      (call-next-method)))");
        builder.blank();
    }
}

// =============================================================================
// Struct wrapper emission
// =============================================================================

fn emit_struct_wrapper(builder: &mut CodeBuilder, s: &StructDef, lx: &Lx) {
    let ir = lx.ir;
    let class = idiomatic_class_name(&s.name);
    let close_sym = format!("close-{}", class);
    let with_sym = format!("with-{}", class);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    // (defclass app (azul-handle) ((ptr ...))) - the slot is the base
    // class's, this only adds the per-class accessor.
    builder.line(&format!("(defclass {} (azul-handle)", class));
    builder.indent();
    builder.line("((ptr :initarg :ptr");
    builder.line(&format!("        :accessor {}-ptr", class));
    builder.line("        :initform (cffi:null-pointer))))");
    builder.dedent();
    builder.blank();

    // Export the class name.
    builder.line(&format!("(export '{} :azul)", class));
    builder.line(&format!("(export '{}-ptr :azul)", class));

    // close-<name>: release the value and the buffer if not moved yet.
    let delete_raw = raw_fn_name(&format!("Az{}_delete", s.name));
    builder.line(&format!("(defmethod {} ((obj {}))", close_sym, class));
    builder.indent();
    builder.line(&format!("(let ((p ({}-ptr obj)))", class));
    builder.indent();
    builder.line("(when (and (cffi:pointerp p) (not (cffi:null-pointer-p p)))");
    builder.line(&format!("  (azul-internal::{} p)", delete_raw));
    builder.line("  (cffi:foreign-free p))");
    builder.line(&format!("(setf ({}-ptr obj) (cffi:null-pointer))))", class));
    builder.dedent();
    builder.dedent();
    builder.blank();
    builder.line(&format!("(export '{} :azul)", close_sym));

    // Idiomatic constructors / methods / static methods.
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&s.name).collect();

    // Track the first usable constructor (for with-<name> sugar).
    let mut ctor_for_with: Option<&FunctionDef> = None;
    // Every name defined for this class so far: a field accessor never
    // replaces an api.json method of the same name.
    let mut taken: BTreeSet<String> = BTreeSet::new();
    taken.insert(format!("{}-ptr", class));

    for func in &funcs {
        if func.kind.is_trait_function() {
            continue; // Skip Delete/PartialEq/Cmp/Hash/Debug -- close-<name> covers Delete.
        }
        let lisp_method = idiomatic_method_name(&func.method_name);
        match func.kind {
            FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default
            | FunctionKind::EnumVariantConstructor => {
                if ctor_for_with.is_none()
                    && matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default)
                {
                    ctor_for_with = Some(func);
                }
                // Constructor / Default -> `make-<class>` (replace the method name).
                let public_name = match func.kind {
                    FunctionKind::Constructor | FunctionKind::Default => {
                        if func.method_name == "new" {
                            format!("make-{}", class)
                        } else {
                            format!("make-{}-{}", class, lisp_method)
                        }
                    }
                    _ => format!("{}-{}", class, lisp_method),
                };
                emit_call_wrapper(builder, lx, &class, &public_name, func, false);
                taken.insert(public_name);
            }
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy => {
                let public_name = format!("{}-{}", class, lisp_method);
                emit_call_wrapper(builder, lx, &class, &public_name, func, true);
                taken.insert(public_name);
            }
            _ => {}
        }
    }

    emit_field_accessors(builder, s, &class, lx, &taken);

    // with-<name>: convenience macro that wraps the chosen constructor
    // call in unwind-protect. If no constructor was found we emit a
    // generic form that takes a pre-built object.
    emit_with_macro(builder, &class, &with_sym, &close_sym, ctor_for_with);

    builder.blank();
}

/// One `(defun ...)` around one C function.
///
/// - `&self` (and any pointer argument) gets the wrapper's buffer pointer;
/// - a wrapped class passed by value is copied out of its buffer and then
///   MOVED (`%consume`d) after the call - the callee owns those bytes now;
/// - a String passed by value may also be a Lisp string;
/// - a returned wrapped class comes back as a fresh wrapper object.
fn emit_call_wrapper(
    builder: &mut CodeBuilder,
    lx: &Lx,
    class: &str,
    public_name: &str,
    func: &FunctionDef,
    has_self: bool,
) {
    let raw = raw_fn_name(&func.c_name);

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    let mut params: Vec<String> = Vec::with_capacity(func.args.len());
    let mut call_args: Vec<String> = Vec::with_capacity(func.args.len());
    let mut rebinds: Vec<String> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    for (i, a) in func.args.iter().enumerate() {
        let owned = a.ref_kind == ArgRefKind::Owned;
        if has_self && i == 0 {
            params.push("obj".to_string());
            call_args.push(format!("({}-ptr obj)", class));
            if owned {
                consumed.push("obj".to_string());
            }
            continue;
        }
        let name = ident_to_kebab(&a.name);
        params.push(name.clone());
        call_args.push(call_arg(lx, a, &name, owned, &mut rebinds, &mut consumed));
    }

    let ret_class = func
        .return_type
        .as_deref()
        .and_then(|r| lx.wrapped(r))
        .map(|s| idiomatic_class_name(&s.name));
    let call = format!("(azul-internal::{} {})", raw, call_args.join(" "));
    let call = call.replace(" )", ")");

    builder.line(&format!("(defun {} ({})", public_name, params.join(" ")));
    builder.indent();
    if rebinds.is_empty() && consumed.is_empty() {
        match &ret_class {
            Some(rc) => builder.line(&format!("(make-instance '{} :ptr {}))", rc, call)),
            None => builder.line(&format!("{})", call)),
        }
    } else {
        let mut bindings = rebinds;
        bindings.push(format!("(r {})", call));
        builder.line(&format!("(let* ({})", bindings.join(" ")));
        builder.indent();
        for c in &consumed {
            builder.line(&format!("(%consume {})", c));
        }
        match &ret_class {
            Some(rc) => builder.line(&format!("(make-instance '{} :ptr r)))", rc)),
            None => builder.line("r))"),
        }
        builder.dedent();
    }
    builder.dedent();
    builder.line(&format!("(export '{} :azul)", public_name));
    builder.blank();
}

/// The call-site expression of one non-receiver argument.
fn call_arg(
    lx: &Lx,
    a: &FunctionArg,
    name: &str,
    owned: bool,
    rebinds: &mut Vec<String>,
    consumed: &mut Vec<String>,
) -> String {
    // Callback-typed args accept a plain Lisp function.
    if let Some(cb) = a.callback_info.as_ref() {
        return format!(
            "(azul:register-callback \"{}\" {})",
            cb.callback_wrapper_name, name
        );
    }
    if lx.wrapped(&a.type_name).is_none() {
        return name.to_string();
    }
    if owned {
        if lx.is_string(&a.type_name) {
            rebinds.push(format!("({} (%string-arg {}))", name, name));
        }
        consumed.push(name.to_string());
    }
    format!("(%unwrap {})", name)
}

// =============================================================================
// Field accessors
// =============================================================================

/// How one field of a wrapped struct is read and written.
enum FieldShape {
    /// A C scalar or a unit enum (a keyword): read and written in place.
    Scalar,
    /// The String class: read as a Lisp string, written from a Lisp string
    /// or a string wrapper after the old one is released.
    Str { delete: String },
    /// A wrapped class: read as a fresh wrapper holding a deep copy,
    /// written by moving a wrapper in after the old value is released.
    Wrapper {
        class: String,
        ty: String,
        delete: String,
        clone: Option<String>,
    },
    /// Any other struct / union value: read as a keyword plist (only when
    /// it owns no heap memory), written from a plist or a foreign pointer.
    Value { ty: String, delete: Option<String> },
}

fn field_shape(f: &FieldDef, lx: &Lx) -> Option<FieldShape> {
    let ir = lx.ir;
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let t = f.type_name.trim();
    if t.contains('<') || t.starts_with('*') || t.starts_with('&') || t.starts_with('[') {
        return None;
    }
    // Callbacks, their wrappers and the type-erased handle are wired up by
    // the closure plumbing, never by writing bytes into a field.
    if managed_host_invoker::is_callback_wrapper(ir, t)
        || managed_lang_helpers::is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
    {
        return None;
    }
    let cffi = map_type_to_cffi(t, ir);
    if cffi == ":pointer" || cffi == ":void" || cffi == ":string" {
        return None;
    }
    if lx.is_string(t) {
        return Some(FieldShape::Str {
            delete: lx.class_fn(t, FunctionKind::Delete)?,
        });
    }
    if let Some(s) = lx.wrapped(t) {
        return Some(FieldShape::Wrapper {
            class: idiomatic_class_name(&s.name),
            ty: qualify_cffi(&cffi),
            delete: lx.class_fn(t, FunctionKind::Delete)?,
            clone: lx.class_fn(t, FunctionKind::DeepCopy),
        });
    }
    if cffi.starts_with("(:struct ") || cffi.starts_with("(:union ") {
        let included = ir.find_struct(t).is_some()
            || ir.find_enum(t).is_some()
            || ir.find_type_alias(t).is_some();
        if !included || !lx.config.should_include_type(t) {
            return None;
        }
        let owns_heap = managed_lang_helpers::has_delete_function(t, ir);
        let delete = lx.class_fn(t, FunctionKind::Delete);
        // A heap-owning value without a reachable `_delete` cannot be
        // replaced without leaking the old one.
        if owns_heap && delete.is_none() {
            return None;
        }
        return Some(FieldShape::Value {
            ty: qualify_cffi(&cffi),
            delete,
        });
    }
    // Scalars: C primitives, simple aliases of them and unit enums.
    if cffi.starts_with(':') || ir.find_enum(t).is_some_and(|e| !e.is_union) {
        return Some(FieldShape::Scalar);
    }
    None
}

/// `(<class>-<field> obj)` and `(setf (<class>-<field> obj) v)` for every
/// public by-value field of a wrapped struct:
///
/// - a getter returns an independent value - a String decodes to a Lisp
///   string without consuming the field, a wrapped class comes back as a
///   fresh wrapper around a deep copy (`_clone`; none -> no getter), a
///   plain value as a keyword plist (none for heap-owning values);
/// - a setter releases the field's old value (`_delete`), then moves the
///   new one in - a wrapper argument is consumed (its close becomes a
///   no-op), a Lisp string becomes a fresh String.
///
/// Getters return COPIES, so a nested write is read-modify-write:
///
/// ```lisp
/// (let ((ws (window-create-options-window-state opts)))   ; a deep copy
///   (setf (full-window-state-title ws) "Hello")
///   (setf (window-create-options-window-state opts) ws))  ; moves ws back in
/// ```
///
/// An api.json method of the same name wins over the getter; the setter
/// (a `(setf ...)` function, its own namespace) is always emitted.
fn emit_field_accessors(
    builder: &mut CodeBuilder,
    s: &StructDef,
    class: &str,
    lx: &Lx,
    taken: &BTreeSet<String>,
) {
    let st = format!("(:struct azul-internal::{})", to_kebab_case(&s.name));
    let mut first = true;
    for f in &s.fields {
        let Some(shape) = field_shape(f, lx) else {
            continue;
        };
        let slot = format!("azul-internal::{}", ident_to_kebab(&f.name));
        let accessor = format!("{}-{}", class, ident_to_kebab(&f.name));
        if accessor == format!("{}-ptr", class) {
            continue;
        }
        if first {
            first = false;
            builder.line(&format!(
                ";; Field accessors of {}. Getters return COPIES (a String as a Lisp string,",
                class
            ));
            builder.line(
                ";; a wrapped class as a fresh deep-copied wrapper, a plain value as a keyword",
            );
            builder.line(
                ";; plist); setters release the old value, then move the new one in. A nested",
            );
            builder.line(";; write is read-modify-write:");
            builder.line(";;   (let ((ws (window-create-options-window-state opts)))");
            builder.line(";;     (setf (full-window-state-title ws) \"Hello\")");
            builder.line(";;     (setf (window-create-options-window-state opts) ws))");
        }
        let fp = format!(
            "(cffi:foreign-slot-pointer ({}-ptr obj) '{} '{})",
            class, st, slot
        );
        let in_place = format!(
            "(cffi:foreign-slot-value ({}-ptr obj) '{} '{})",
            class, st, slot
        );
        if let Some(d) = &f.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
        let getter: Option<String> = match &shape {
            FieldShape::Scalar => Some(in_place.clone()),
            FieldShape::Str { .. } => Some(format!("(%string-value {})", fp)),
            FieldShape::Wrapper {
                class: fc,
                clone: Some(c),
                ..
            } => Some(format!(
                "(make-instance '{} :ptr (azul-internal::{} {}))",
                fc, c, fp
            )),
            FieldShape::Wrapper { clone: None, .. } => None,
            FieldShape::Value { ty, delete: None } => {
                Some(format!("(%plist-keys (cffi:mem-ref {} '{}) nil)", fp, ty))
            }
            FieldShape::Value {
                delete: Some(_), ..
            } => None,
        };
        if let Some(body) = getter {
            if !taken.contains(&accessor) {
                builder.line(&format!("(defun {} (obj)", accessor));
                builder.line(&format!(
                    "  \"A copy of the `{}` field of a {}.\"",
                    f.name,
                    class.to_uppercase()
                ));
                builder.line(&format!("  {})", body));
            }
        }
        let setter = match &shape {
            FieldShape::Scalar => format!("(setf {} v)", in_place),
            FieldShape::Str { delete } => {
                let string_ty = format!(
                    "(:struct azul-internal::{})",
                    to_kebab_case(
                        &lx.string
                            .as_ref()
                            .map(|s| s.class.clone())
                            .unwrap_or_default()
                    )
                );
                format!(
                    "(%move-in {} '{} (%string-arg v) #'azul-internal::{})",
                    fp, string_ty, delete
                )
            }
            FieldShape::Wrapper { ty, delete, .. } => {
                format!("(%move-in {} '{} v #'azul-internal::{})", fp, ty, delete)
            }
            FieldShape::Value { ty, delete } => format!(
                "(%move-in {} '{} (%plist-keys v t) {})",
                fp,
                ty,
                delete
                    .as_ref()
                    .map_or("nil".to_string(), |d| format!("#'azul-internal::{}", d))
            ),
        };
        builder.line(&format!("(defun (setf {}) (v obj)", accessor));
        builder.line(&format!(
            "  \"Replaces the `{}` field of a {} (the old value is released, a wrapper V is moved in).\"",
            f.name,
            class.to_uppercase()
        ));
        builder.line(&format!("  {}", setter));
        builder.line("  v)");
        builder.line(&format!("(export '{} :azul)", accessor));
        builder.blank();
    }
}

fn emit_with_macro(
    builder: &mut CodeBuilder,
    class: &str,
    with_sym: &str,
    close_sym: &str,
    ctor: Option<&FunctionDef>,
) {
    builder.line(&format!(
        ";; ({} (var ctor-args...) body...) -- unwind-protect-wrapped binding.",
        with_sym
    ));
    if ctor.is_none() {
        // No constructor in the IR — emitting a `(let ((,var (first
        // ctor-args))) ...)` macro both (a) fails READ because the
        // comma quote on the `ctor-args` reference doesn't survive the
        // SKIP-the-ctor branch, and (b) is conceptually wrong: the
        // caller would have to provide an already-built object via the
        // first ctor-arg, at which point a plain `let` is clearer.
        // Skip the macro entirely; users use raw `unwind-protect`.
        builder.line(&format!(
            ";; SKIPPED: no constructor for `{}` in the IR — use raw",
            class
        ));
        builder.line(&format!(
            ";; `(unwind-protect (progn ...body...) ({} obj))` instead.",
            close_sym
        ));
        builder.blank();
        return;
    }
    builder.line(&format!(
        "(defmacro {} ((var &rest ctor-args) &body body)",
        with_sym
    ));
    builder.indent();
    // Use `make-<class>` (already exported) for the constructor.
    builder.line(&format!(
        "`(let ((,var (apply #'make-{} (list ,@ctor-args))))",
        class
    ));
    builder.indent();
    builder.line(&format!(
        "   (unwind-protect (progn ,@body) ({} ,var))))",
        close_sym
    ));
    builder.dedent();
    builder.dedent();
    builder.line(&format!("(export '{} :azul)", with_sym));
    builder.blank();
}

// =============================================================================
// Tagged-union helpers (minimal)
// =============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class = idiomatic_class_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }
    builder.line(&format!(";; Tagged-union helpers for {}.", class));

    let union_kebab = to_kebab_case(&e.name); // e.g. "az-option-i64"
    for v in &e.variants {
        let variant = ident_to_kebab(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                let public = format!("{}-make-{}", class, variant);
                builder.line(&format!("(defun {} ()", public));
                builder.indent();
                builder.line(&format!(
                    ";; Construct a fresh {} with tag = {}.",
                    class, v.name
                ));
                builder.line(&format!(
                    "(let ((u (cffi:foreign-alloc '(:union azul-internal::{}))))",
                    union_kebab
                ));
                builder.indent();
                builder.line(&format!(
                    "(setf (cffi:foreign-slot-value (cffi:foreign-slot-pointer u '(:union \
                     azul-internal::{}) '{}) '(:struct azul-internal::{}-variant-{}) \
                     'azul-internal::tag) :{})",
                    union_kebab, variant, union_kebab, variant, variant
                ));
                builder.line("u))");
                builder.dedent();
                builder.dedent();
                builder.line(&format!("(export '{} :azul)", public));
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                builder.line(&format!(
                    ";; SKIPPED: variant {}.{} has payload -- construct via cffi:foreign-alloc + \
                     slot setters.",
                    class, v.name
                ));
            }
        }
    }
    builder.blank();
}

// =============================================================================
// Helpers
// =============================================================================

fn idiomatic_method_name(method_name: &str) -> String {
    // Lisp loves kebab-case; convert camelCase / snake_case uniformly.
    let mut out = String::new();
    let mut prev_lower = false;
    for c in method_name.chars() {
        if c == '_' {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            prev_lower = false;
        } else if c.is_uppercase() {
            if prev_lower && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase();
        }
    }
    if out.is_empty() {
        return "op".to_string();
    }
    // Special-case: `new` as a method name reads better as `new` itself
    // when paired with `make-<class>-new` -- but we already convert the
    // `new` constructor to `make-<class>` upstream, so leave it as-is.
    out
}

fn sanitize_comment(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::super::super::{bug_classes::ir, config::CodegenConfig};

    fn lisp() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| super::super::generate(ir(), &CodegenConfig::c_header()).unwrap())
    }

    /// The text of the top-level form starting with `head`.
    fn form(head: &str) -> &'static str {
        let out = lisp();
        let start = out
            .find(head)
            .unwrap_or_else(|| panic!("no `{head}` in azul.lisp"));
        let rest = &out[start..];
        let end = rest[1..].find("\n(").map_or(rest.len(), |e| e + 1);
        &rest[..end]
    }

    #[test]
    fn a_wrapper_holds_a_foreign_pointer_because_by_value_returns_are_boxed_not_plists() {
        let from =
            form("(defmethod translate-from-foreign (p (type az-window-create-options-tclass))");
        assert!(
            from.contains("(%azul-box p '(:struct az-window-create-options))"),
            "{from}"
        );
        let into = form(
            "(defmethod translate-into-foreign-memory (value (type az-window-create-options-tclass) p)",
        );
        assert!(into.contains("(if (pointerp value)"), "{into}");
        assert!(into.contains("(call-next-method)"), "{into}");
        // A `&self` method hands over the handle's pointer.
        let run = form("(defun app-run (obj");
        assert!(run.contains("(app-ptr obj)"), "{run}");
        assert!(run.contains("(%consume root-window)"), "{run}");
    }

    #[test]
    fn closing_a_wrapper_deletes_the_value_and_frees_its_buffer() {
        let close = form("(defmethod close-window-create-options ((obj window-create-options))");
        assert!(
            close.contains("(azul-internal::%az-window-create-options-delete p)"),
            "{close}"
        );
        assert!(close.contains("(cffi:foreign-free p)"), "{close}");
    }

    #[test]
    fn a_by_value_wrapper_argument_is_consumed_after_the_call() {
        let with_child = form("(defun dom-with-child (obj");
        assert!(with_child.contains("(%consume obj)"), "{with_child}");
        assert!(with_child.contains("(%consume child)"), "{with_child}");
        assert!(with_child.contains("(%unwrap child)"), "{with_child}");
    }

    #[test]
    fn the_title_getter_decodes_the_string_without_consuming_it() {
        let get = form("(defun full-window-state-title (obj)");
        assert!(
            get.contains(
                "(%string-value (cffi:foreign-slot-pointer (full-window-state-ptr obj) '(:struct \
                 azul-internal::az-full-window-state) 'azul-internal::title))"
            ),
            "{get}"
        );
    }

    #[test]
    fn the_title_setter_releases_the_old_string_then_moves_the_new_one_in() {
        let set = form("(defun (setf full-window-state-title) (v obj)");
        assert!(set.contains("(%string-arg v)"), "{set}");
        assert!(set.contains("#'azul-internal::%az-string-delete"), "{set}");
        assert!(set.contains("(%move-in "), "{set}");
    }

    #[test]
    fn the_window_state_getter_deep_copies_and_the_setter_deletes_then_consumes() {
        let get = form("(defun window-create-options-window-state (obj)");
        assert!(
            get.contains(
                "(make-instance 'full-window-state :ptr \
                 (azul-internal::%az-full-window-state-clone "
            ),
            "{get}"
        );
        let set = form("(defun (setf window-create-options-window-state) (v obj)");
        assert!(
            set.contains("#'azul-internal::%az-full-window-state-delete"),
            "{set}"
        );
        let mv = form("(defun %move-in (fp type v release)");
        assert!(mv.contains("(funcall release fp)"), "{mv}");
        assert!(mv.contains("(%consume v)"), "{mv}");
    }

    #[test]
    fn a_plain_value_field_reads_and_writes_as_a_keyword_plist() {
        let get = form("(defun full-window-state-size (obj)");
        assert!(get.contains("(%plist-keys (cffi:mem-ref "), "{get}");
        assert!(
            get.contains("'(:struct azul-internal::az-window-size)"),
            "{get}"
        );
        let set = form("(defun (setf full-window-state-size) (v obj)");
        assert!(set.contains("(%plist-keys v t)"), "{set}");
    }

    #[test]
    fn a_bool_field_reads_and_writes_in_place() {
        let get = form("(defun full-window-state-window-focused (obj)");
        assert!(
            get.contains(
                "(cffi:foreign-slot-value (full-window-state-ptr obj) '(:struct \
                 azul-internal::az-full-window-state) 'azul-internal::window-focused)"
            ),
            "{get}"
        );
        assert!(lisp().contains("(defun (setf full-window-state-window-focused) (v obj)"));
    }

    #[test]
    fn the_text_field_stays_writable_next_to_the_get_text_method() {
        assert!(lisp().contains("(defun text-input-state-get-text (obj"));
        let set = form("(defun (setf text-input-state-text) (v obj)");
        assert!(set.contains("#'azul-internal::%az-u32-vec-delete"), "{set}");
    }
}

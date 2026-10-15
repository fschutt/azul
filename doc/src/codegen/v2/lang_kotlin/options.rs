//! Native optionals for Kotlin.
//!
//! Every `Option*` mirror crosses the wrapper surface as a Kotlin nullable,
//! in both directions: a parameter takes `T?` (`null` is None), a field is a
//! `var x: T?` property, a return value is a `T?`. So `?.`, `?:` and
//! `?: return Update.DoNothing` work on everything the API hands out. `T` is
//! `kotlin.String` for text, the wrapper class (`FilePath`) for a type the
//! binding owns, and the mirror's `toNullable()` type for anything else.
//!
//! Which Options convert, and how, is the Java binding's decision
//! (`lang_java::options::jvm_option`), so both JVM bindings agree. The
//! conversion lives in one generated object, `AzulOptions`, with an
//! `in<Option>` / `take<Option>` pair per type.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{CodegenIR, FieldRefKind, FunctionKind, TypeCategory},
        lang_java::{
            functions::native_class_for_class,
            options::{jvm_option, Payload},
        },
    },
    ffi_type_name, kotlin_class_name, ref_kind_kt_field,
};

/// One Option type the Kotlin surface converts.
pub(super) struct KtOption {
    /// The api.json name (`OptionString`): it names the bridge functions.
    pub name: String,
    /// The Kotlin type of the value inside, without `?`.
    pub value: String,
    pub kind: Kind,
}

pub(super) enum Kind {
    Str,
    /// A wrapper class.
    Class,
    /// The `toNullable()` type: a Kotlin primitive or a JNA struct.
    Value,
}

impl KtOption {
    /// The nullable the surface shows (`kotlin.String?`).
    pub(super) fn native(&self) -> String {
        format!("{}?", self.value)
    }

    pub(super) fn into_raw(&self, expr: &str) -> String {
        format!("AzulOptions.in{}({})", self.name, expr)
    }

    pub(super) fn take(&self, expr: &str) -> String {
        format!("AzulOptions.take{}({})", self.name, expr)
    }
}

pub(super) fn kt_option(type_name: &str, ir: &CodegenIR) -> Option<KtOption> {
    let opt = jvm_option(type_name, ir)?;
    let payload = ir.option_payload(type_name.trim())?;
    let (value, kind) = match opt.payload {
        Payload::Str => ("kotlin.String".to_string(), Kind::Str),
        Payload::Class(_) => (kotlin_class_name(payload, ir), Kind::Class),
        Payload::Value(_) => {
            let (kt, _) = ref_kind_kt_field(payload, &FieldRefKind::Owned, ir);
            (kt.trim_end_matches('?').to_string(), Kind::Value)
        }
    };
    Some(KtOption {
        name: opt.name,
        value,
        kind,
    })
}

/// `internal object AzulOptions`: the `in` / `take` pair of every converted
/// Option.
pub(super) fn emit_options_object(b: &mut CodeBuilder, ir: &CodegenIR) {
    let string_class = ir
        .structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))
        .map(|s| s.name.clone());
    b.line("/**");
    b.line(" * The Kotlin nullable of every Option type, in both directions: `in*` builds the");
    b.line(" * native option (null is None, a wrapper is moved in), `take*` takes one apart");
    b.line(" * (the option is consumed, None is null).");
    b.line(" */");
    b.line("internal object AzulOptions {");
    b.indent();
    if let Some(s) = &string_class {
        let native = native_class_for_class(s, ir);
        let ffi = ffi_type_name(s);
        b.line(&format!("fun str(v: kotlin.String): {}.ByValue {{", ffi));
        b.indent();
        b.line("val __b = v.toByteArray(Charsets.UTF_8)");
        b.line("val __m = Memory(maxOf(1, __b.size).toLong())");
        b.line("__m.write(0, __b, 0, __b.size)");
        b.line(&format!("return {}.{}_fromUtf8(__m, __b.size.toLong())", native, ffi));
        b.dedent();
        b.line("}");
        b.blank();
        b.line(&format!("fun readStr(s: {}): kotlin.String {{", ffi));
        b.indent();
        b.line("s.write()");
        b.line("val __vp = s.pointer.getPointer(0) ?: return \"\"");
        b.line("val __vl = s.pointer.getLong(8)");
        b.line("if (__vl <= 0) return \"\"");
        b.line("return __vp.getByteArray(0, __vl.toInt()).toString(Charsets.UTF_8)");
        b.dedent();
        b.line("}");
        b.blank();
    }
    for e in &ir.enums {
        let Some(opt) = kt_option(&e.name, ir) else {
            continue;
        };
        let ffi = ffi_type_name(&e.name);
        let native = native_class_for_class(&e.name, ir);
        let none = format!("{}.{}_none()", native, ffi);
        let some = |v: &str| format!("{}.{}_some({})", native, ffi, v);
        let delete = ir
            .functions
            .iter()
            .any(|f| f.class_name == e.name && matches!(f.kind, FunctionKind::Delete))
            .then(|| format!("{}.{}_delete(o.pointer)", native, ffi));
        let n = opt.native();
        match &opt.kind {
            Kind::Str => {
                b.line(&format!(
                    "fun in{}(v: {}): {}.ByValue = if (v == null) {} else {}",
                    opt.name,
                    n,
                    ffi,
                    none,
                    some("str(v)")
                ));
                b.line(&format!("fun take{}(o: {}): {} {{", opt.name, ffi, n));
                b.indent();
                b.line("val __s = o.toNullable()?.let { readStr(it) }");
                if let Some(d) = &delete {
                    b.line(d);
                }
                b.line("return __s");
                b.dedent();
                b.line("}");
            }
            Kind::Class => {
                b.line(&format!(
                    "fun in{}(v: {}): {}.ByValue = if (v == null) {} else {}",
                    opt.name,
                    n,
                    ffi,
                    none,
                    some("v.__takeRaw()")
                ));
                // The payload's bytes move into a fresh wrapper's memory; the
                // option holds nothing else, so it is not freed.
                b.line(&format!("fun take{}(o: {}): {} {{", opt.name, ffi, n));
                b.indent();
                b.line("val __n = o.toNullable() ?: return null");
                b.line("val __m = Memory(__n.size().toLong())");
                b.line("__m.write(0, __n.pointer.getByteArray(0, __n.size()), 0, __n.size())");
                b.line(&format!("return {}(__m)", opt.value));
                b.dedent();
                b.line("}");
            }
            Kind::Value => {
                let arg = if opt.value.starts_with("Az") {
                    // A JNA struct crosses by value through its `.ByValue` twin.
                    format!(
                        "run {{ v.write(); (Structure.newInstance({t}.ByValue::class.java, v.pointer) as {t}.ByValue).also {{ it.read() }} }}",
                        t = opt.value
                    )
                } else {
                    "v".to_string()
                };
                b.line(&format!(
                    "fun in{}(v: {}): {}.ByValue = if (v == null) {} else {}",
                    opt.name,
                    n,
                    ffi,
                    none,
                    some(&arg)
                ));
                b.line(&format!("fun take{}(o: {}): {} = o.toNullable()", opt.name, ffi, n));
            }
        }
        b.blank();
    }
    b.dedent();
    b.line("}");
    b.blank();
}

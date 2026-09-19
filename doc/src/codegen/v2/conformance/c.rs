//! The C rendering of the conformance plan: `conformance/c/conformance.c`.
//!
//! C calls the C API directly, so this program is the ground truth the other
//! bindings are measured against: a check that fails here is a libazul
//! defect, one that fails only in another language is that binding's.
//!
//! Build and run (the e2e matrix does this in its `conformance` mode):
//!
//! ```sh
//! cc -std=c11 -O0 conformance.c -I<codegen> -L<lib> -lazul -o conformance
//! AZ_MEMTEST_N=1 ./conformance
//! ```

use std::fmt::Write as _;

use super::{CallbackCase, ConformancePlan, DeriveCase, Recipe, VariantCase, VecCase};

/// The C spelling of a plan type.
fn c_type(ty: &str) -> String {
    match ty {
        "bool" => "bool".into(),
        "u8" => "uint8_t".into(),
        "u16" => "uint16_t".into(),
        "u32" => "uint32_t".into(),
        "u64" => "uint64_t".into(),
        "usize" => "size_t".into(),
        "i8" => "int8_t".into(),
        "i16" => "int16_t".into(),
        "i32" => "int32_t".into(),
        "i64" => "int64_t".into(),
        "isize" => "ptrdiff_t".into(),
        "f32" => "float".into(),
        "f64" => "double".into(),
        "char" => "uint32_t".into(),
        other => format!("Az{other}"),
    }
}

/// A C expression that makes a fresh value.
fn make(r: &Recipe) -> String {
    match r {
        Recipe::Primitive { ty } => format!("({})0", c_type(ty)),
        Recipe::String => "AzString_fromUtf8((const uint8_t*)\"azul\", 4)".into(),
        Recipe::Default { c_fn, .. }
        | Recipe::UnitVariant { c_fn, .. }
        | Recipe::None { c_fn, .. }
        | Recipe::EmptyVec { c_fn, .. } => format!("{c_fn}()"),
    }
}

/// An identifier-safe suffix for a function name.
fn ident(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// A C string literal for a check's label.
fn label(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn derive_case(o: &mut String, d: &DeriveCase) {
    let t = c_type(&d.ty);
    let name = &d.ty;
    let _ = writeln!(o, "static void check_derive_{}(void) {{", ident(name));
    let _ = writeln!(o, "    {t} a = {};", make(&d.make));
    // `b` is a clone (or, for a Copy type, a copy): equal by construction.
    let has_b = d.clone.is_some() || d.is_copy;
    if let Some(clone) = &d.clone {
        let _ = writeln!(o, "    {t} b = {clone}(&a);");
    } else if d.is_copy {
        let _ = writeln!(o, "    {t} b = a;");
    }
    if let Some(debug) = &d.debug {
        let _ = writeln!(
            o,
            "    {{ AzString s = {debug}(&a); CHECK(s.vec.len > 0, {}); AzString_delete(&s); }}",
            label(&format!("{name}: Debug formats empty"))
        );
    }
    if has_b {
        if let Some(eq) = &d.partial_eq {
            let _ = writeln!(o, "    CHECK({eq}(&a, &b), {});", label(&format!("{name}: a clone is not PartialEq-equal")));
        }
        if let Some(pc) = &d.partial_cmp {
            let _ = writeln!(o, "    CHECK({pc}(&a, &b) == 1, {});", label(&format!("{name}: partial_cmp(clone) is not Equal")));
        }
        if let Some(c) = &d.cmp {
            let _ = writeln!(o, "    CHECK({c}(&a, &b) == 1, {});", label(&format!("{name}: cmp(clone) is not Equal")));
        }
        if let Some(h) = &d.hash {
            let _ = writeln!(o, "    CHECK({h}(&a) == {h}(&b), {});", label(&format!("{name}: a clone hashes differently")));
        }
    }
    if let (Some(del), false) = (&d.delete, d.is_copy) {
        let _ = writeln!(o, "    {del}(&a);");
        if has_b {
            let _ = writeln!(o, "    {del}(&b);");
        }
    }
    let _ = writeln!(o, "}}\n");
}

fn variant_case(o: &mut String, v: &VariantCase, n: usize) {
    let t = c_type(&v.ty);
    let _ = writeln!(o, "static void check_variant_{n}(void) {{ /* {}::{} */", v.ty, v.variant);
    let args: Vec<String> = v.args.iter().map(make).collect();
    let _ = writeln!(o, "    {t} v = {}({});", v.c_fn, args.join(", "));
    let _ = writeln!(o, "    CHECK(1, {});", label(&format!("{}::{} constructs", v.ty, v.variant)));
    if let Some(del) = &v.delete {
        let _ = writeln!(o, "    {del}(&v);");
    } else {
        let _ = writeln!(o, "    (void)v;");
    }
    let _ = writeln!(o, "}}\n");
}

fn vec_case(o: &mut String, v: &VecCase) {
    let t = c_type(&v.ty);
    let et = c_type(v.element.ty());
    let e = make(&v.element);
    let _ = writeln!(o, "static void check_vec_{}(void) {{", ident(&v.ty));
    let _ = writeln!(o, "    {et} items[3] = {{ {e}, {e}, {e} }};");
    let _ = writeln!(o, "    {t} v = {}(items, 3);", v.copy_from_ptr);
    let _ = writeln!(o, "    CHECK({}(&v) == 3, {});", v.len, label(&format!("{}: 3 elements in, not 3 out", v.ty)));
    if let Some(del) = &v.delete {
        let _ = writeln!(o, "    {del}(&v);");
    }
    if let Some(del) = &v.element_delete {
        let _ = writeln!(o, "    for (int i = 0; i < 3; i++) {del}(&items[i]);");
    }
    let _ = writeln!(o, "}}\n");
}

fn callback_case(o: &mut String, c: &CallbackCase) {
    let t = c_type(&c.wrapper);
    let _ = writeln!(o, "static void check_callback_{}(void) {{", ident(&c.wrapper));
    let _ = writeln!(o, "    {t} w = {}(42);", c.from_handle);
    let _ = writeln!(o, "    CHECK(1, {});", label(&format!("{}: created from a host handle", c.wrapper)));
    if let Some(del) = &c.delete {
        let _ = writeln!(o, "    {del}(&w);");
    }
    let _ = writeln!(o, "}}\n");
}

/// `conformance.c`.
pub fn render(plan: &ConformancePlan) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "// Auto-generated by azul-doc codegen v2 (conformance). DO NOT EDIT.\n\
         // The conformance plan in C, the ground truth for every other binding:\n\
         // every constant, every derive round-trip, every variant constructor,\n\
         // every Vec and every host-invokable callback kind. Runs the plan\n\
         // AZ_MEMTEST_N times (default 1): scripts/run_memtest.sh compares peak\n\
         // RSS across N (leaks) and runs it under a debugger (crashes).\n\
         #include \"azul.h\"\n\
         #include <stdbool.h>\n\
         #include <stdint.h>\n\
         #include <stdio.h>\n\
         #include <stdlib.h>\n\n\
         static long checks = 0;\n\
         static long failures = 0;\n\
         #define CHECK(cond, what) do {{ checks++; if (!(cond)) {{ if (failures < 200) fprintf(stderr, \"FAIL %s\\n\", what); failures++; }} }} while (0)\n"
    );

    // The host-invoker entry points libazul exports but azul.h does not declare.
    for c in &plan.callbacks {
        let _ = writeln!(o, "extern DLLIMPORT {} {}(uint64_t handle);", c_type(&c.wrapper), c.from_handle);
    }
    let _ = writeln!(o);

    let _ = writeln!(o, "static void check_constants(void) {{");
    for k in &plan.constants {
        let _ = writeln!(
            o,
            "    CHECK({} == ({}){}, {});",
            k.c_name,
            c_type(&k.type_name),
            k.value,
            label(&format!("{}::{} is {}", k.class, k.name, k.value))
        );
    }
    let _ = writeln!(o, "}}\n");

    for d in &plan.derives {
        derive_case(&mut o, d);
    }
    for (n, v) in plan.variants.iter().enumerate() {
        variant_case(&mut o, v, n);
    }
    for v in &plan.vecs {
        vec_case(&mut o, v);
    }
    for c in &plan.callbacks {
        callback_case(&mut o, c);
    }

    let _ = writeln!(o, "static void run_plan(void) {{\n    check_constants();");
    for d in &plan.derives {
        let _ = writeln!(o, "    check_derive_{}();", ident(&d.ty));
    }
    for n in 0..plan.variants.len() {
        let _ = writeln!(o, "    check_variant_{n}();");
    }
    for v in &plan.vecs {
        let _ = writeln!(o, "    check_vec_{}();", ident(&v.ty));
    }
    for c in &plan.callbacks {
        let _ = writeln!(o, "    check_callback_{}();", ident(&c.wrapper));
    }
    let _ = writeln!(o, "}}\n");

    let _ = writeln!(
        o,
        "int main(void) {{\n    \
         const char* n_env = getenv(\"AZ_MEMTEST_N\");\n    \
         long n = n_env ? strtol(n_env, NULL, 10) : 1;\n    \
         if (n < 1) n = 1;\n    \
         for (long i = 0; i < n && failures == 0; i++) run_plan();\n    \
         printf(\"conformance c: %ld iteration(s), %ld checks, %ld failure(s); {} types have no generic constructor\\n\", n, checks, failures);\n    \
         return failures ? 1 : 0;\n}}",
        plan.unconstructible.len()
    );
    o
}

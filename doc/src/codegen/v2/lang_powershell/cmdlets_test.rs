//! The parameter types of the generated cmdlets, as PowerShell reads them.
//!
//! The cmdlets take their parameter types from the C# wrapper methods they forward to, and C#
//! spells an optional value `T?` and a class of the global namespace `global::Azul.X`. Written
//! into a `param(...)` block unchanged, `[Azul.AzU8VecRef?]$OptData` is not a type literal:
//! PowerShell stops at "Missing ] at end of attribute or type literal", `Azul.psm1` does not
//! parse (75 errors for 25 parameters), and `Import-Module` of the binding fails before the first
//! cmdlet is read (the PowerShell leg of the language board on Windows, website run
//! 45738701245; PowerShell 7.6.10 on windows-2022, run 45754551758).

use super::*;

#[test]
fn an_optional_value_is_nullable_of_its_type() {
    assert_eq!(ps_type_literal("AzU8VecRef?"), "[Nullable[Azul.AzU8VecRef]]");
    assert_eq!(ps_type_literal("DarkLightMode?"), "[Nullable[Azul.DarkLightMode]]");
    assert_eq!(ps_type_literal("UIntPtr?"), "[Nullable[UIntPtr]]");
    assert_eq!(ps_type_literal("long?"), "[Nullable[int64]]");
    assert_eq!(ps_type_literal("int?"), "[Nullable[int32]]");
}

#[test]
fn an_optional_string_or_wrapper_class_is_the_type_itself_and_takes_null() {
    assert_eq!(ps_type_literal("string?"), "[string]");
    assert_eq!(ps_type_literal("global::Azul.DbScope?"), "[Azul.DbScope]");
    assert_eq!(ps_type_literal("global::Azul.CssPropertyWithConditionsVec?"), "[Azul.CssPropertyWithConditionsVec]");
}

#[test]
fn a_type_that_is_not_optional_is_what_it_was() {
    assert_eq!(ps_type_literal("string"), "[string]");
    assert_eq!(ps_type_literal("uint"), "[uint32]");
    assert_eq!(ps_type_literal("float"), "[single]");
    assert_eq!(ps_type_literal("IntPtr"), "[IntPtr]");
    assert_eq!(ps_type_literal("WindowCreateOptions"), "[Azul.WindowCreateOptions]");
    assert_eq!(ps_type_literal("AzWindowSize"), "[Azul.AzWindowSize]");
}

#[test]
fn no_parameter_of_the_real_api_keeps_a_csharp_spelling() {
    let ir = crate::codegen::v2::bug_classes::ir();
    let mut b = CodeBuilder::new("    ");
    generate_cmdlets(&mut b, ir, &CodegenConfig::c_header()).expect("cmdlets");
    let out = b.finish();
    let mut declared = 0;
    for line in out.lines() {
        let t = line.trim_start();
        // A parameter declaration: `[Type]$Name,`.
        if t.starts_with('[') && t.contains("]$") {
            declared += 1;
            assert!(
                !t.contains('?') && !t.contains("::"),
                "a parameter declaration in a C# spelling, which PowerShell does not parse: {t}"
            );
        }
    }
    assert!(declared > 1_000, "only {declared} parameter declarations found: the scan does not read the cmdlets");
    // The optional values of the API are there, spelled in PowerShell.
    assert!(out.contains("[Nullable[Azul."), "no optional value parameter in the generated cmdlets");
}

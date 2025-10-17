use std::sync::Arc;

use regex::Regex;

/// Pre-compiled regex patterns used throughout autofix
///
/// All regexes are compiled once at initialization and reused,
/// avoiding redundant compilation during analysis.
#[derive(Clone)]
pub struct CompiledRegexes {
    /// Matches enum definition with body: `enum TypeName { ... }`
    pub enum_definition: Arc<Regex>,

    /// Matches struct definition with body: `struct TypeName { ... }`
    pub struct_definition: Arc<Regex>,

    /// Matches enum variant: `VariantName` or `VariantName(Type)`
    pub enum_variant: Arc<Regex>,

    /// Matches struct field: `field: Type` or `pub field: Type`
    pub struct_field: Arc<Regex>,

    /// Matches non-exhaustive pattern error: `pattern 'X' not covered`
    pub pattern_not_covered: Arc<Regex>,

    /// Matches multiple patterns not covered: `patterns 'X' and 'Y' not covered`
    pub patterns_not_covered: Arc<Regex>,

    /// Matches compiler suggestion: `use path::to::Type;`
    pub use_suggestion: Arc<Regex>,

    /// Matches unresolved import error: `unresolved import 'path'`
    pub unresolved_import: Arc<Regex>,

    /// Matches WindowState struct (special case for oracle)
    pub window_state_struct: Arc<Regex>,

    /// Matches CallbackType enum (special case for oracle)
    pub callback_type_enum: Arc<Regex>,

    /// Matches macro invocations: `!(args)`
    pub macro_invocation: Arc<Regex>,

    /// Matches rust code blocks in markdown: ```rust\n...\n```
    pub markdown_rust_block: Arc<Regex>,
}

impl CompiledRegexes {
    /// Compile all regexes at initialization
    ///
    /// This is called once at the start of autofix to avoid
    /// re-compiling regexes during analysis.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            // Pattern for enum definition - we use a placeholder for type_name
            // and create type-specific regexes on demand
            enum_definition: Arc::new(Regex::new(r"(?s)enum\s+(\w+)\s*\{([^}]+)\}")?),

            struct_definition: Arc::new(Regex::new(r"(?s)struct\s+(\w+)\s*\{([^}]+)\}")?),

            enum_variant: Arc::new(Regex::new(r"(\w+)(?:\s*\(([^)]+)\))?\s*,?")?),

            struct_field: Arc::new(Regex::new(r"(?:pub\s+)?(\w+)\s*:\s*([^,]+),?")?),

            pattern_not_covered: Arc::new(Regex::new(r"pattern `([^`]+)` not covered")?),

            patterns_not_covered: Arc::new(Regex::new(
                r"patterns? `([^`]+)`(?: and `([^`]+)`)? not covered",
            )?),

            use_suggestion: Arc::new(Regex::new(r"use ([a-z_]+::[a-zA-Z0-9_:]+);")?),

            unresolved_import: Arc::new(Regex::new(
                r"error\[E0432\].*unresolved import `([^`]+)`",
            )?),

            window_state_struct: Arc::new(Regex::new(r"(?s)struct\s+WindowState\s*\{([^}]+)\}")?),

            callback_type_enum: Arc::new(Regex::new(r"(?s)enum\s+CallbackType\s*\{([^}]+)\}")?),

            macro_invocation: Arc::new(Regex::new(r"!\s*\(((?:[^()]|\([^()]*\))*)\)")?),

            markdown_rust_block: Arc::new(Regex::new(r"```(?:rust|rs)\s*\n([\s\S]*?)```")?),
        })
    }

    /// Create a type-specific enum definition regex
    ///
    /// Since enum/struct names vary, we need to create regexes with
    /// the specific type name embedded. This still saves compilation
    /// because the base pattern is pre-compiled.
    pub fn enum_for_type(&self, type_name: &str) -> Result<Regex, regex::Error> {
        Regex::new(&format!(
            r"(?s)enum\s+{}\s*\{{([^}}]+)\}}",
            regex::escape(type_name)
        ))
    }

    /// Create a type-specific struct definition regex
    pub fn struct_for_type(&self, type_name: &str) -> Result<Regex, regex::Error> {
        Regex::new(&format!(
            r"(?s)struct\s+{}\s*\{{([^}}]+)\}}",
            regex::escape(type_name)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_all_regexes() {
        let regexes = CompiledRegexes::new().expect("Should compile all regexes");

        // Test enum variant regex
        let text = "VariantA, VariantB(u32), VariantC";
        let matches: Vec<_> = regexes.enum_variant.captures_iter(text).collect();
        assert_eq!(matches.len(), 3);

        // Test struct field regex
        let text = "field1: u32, pub field2: String,";
        let matches: Vec<_> = regexes.struct_field.captures_iter(text).collect();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_type_specific_regexes() {
        let regexes = CompiledRegexes::new().unwrap();

        // Test enum for specific type
        let enum_re = regexes.enum_for_type("MyEnum").unwrap();
        let text = "enum MyEnum { A, B, C }";
        assert!(enum_re.is_match(text));

        // Test struct for specific type
        let struct_re = regexes.struct_for_type("MyStruct").unwrap();
        let text = "struct MyStruct { field: u32 }";
        assert!(struct_re.is_match(text));
    }
}

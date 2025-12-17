use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;
use syn::{
    visit::{self, Visit},
    ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMacro, ItemMod, ItemStatic, ItemStruct, ItemTrait,
    ItemType,
};

/// A visitor that collects Rust symbols during parsing
pub struct SymbolCollector {
    crate_name: String,
    base_path: PathBuf,
    current_path: Vec<String>,
    symbols: BTreeMap<String, SymbolInfo>,
    current_file: PathBuf,
}

impl SymbolCollector {
    pub fn new(crate_name: &str, base_path: &Path) -> Self {
        Self {
            crate_name: crate_name.to_string(),
            base_path: base_path.to_path_buf(),
            current_path: Vec::new(),
            symbols: BTreeMap::new(),
            current_file: PathBuf::new(),
        }
    }

    pub fn get_symbols(&self) -> &BTreeMap<String, SymbolInfo> {
        &self.symbols
    }

    fn current_module_path(&self) -> Vec<String> {
        let rel_path = match self.current_file.strip_prefix(&self.base_path) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return vec![self.crate_name.clone()],
        };

        let mut module_path = vec![self.crate_name.clone()];

        // Process path components
        let components: Vec<_> = rel_path.components().collect();
        for (i, comp) in components.iter().enumerate() {
            if let std::path::Component::Normal(name) = comp {
                let name_str = name.to_string_lossy();

                // Skip src directory
                if i == 0 && name_str == "src" {
                    continue;
                }

                // Handle special files
                if i == components.len() - 1 {
                    let file_name = name_str.to_string();
                    if file_name == "mod.rs" {
                        // mod.rs doesn't add to path
                        continue;
                    } else if file_name == "lib.rs" || file_name == "main.rs" {
                        // lib.rs and main.rs are the crate root
                        module_path = vec![self.crate_name.clone()];
                        continue;
                    } else if file_name.ends_with(".rs") {
                        // Regular .rs file, add without extension
                        let module_name = file_name.trim_end_matches(".rs");
                        module_path.push(module_name.to_string());
                    }
                } else {
                    // Directory name becomes a module
                    module_path.push(name_str.to_string());
                }
            }
        }

        module_path
    }

    /*
    fn add_symbol(&mut self, name: &str, symbol_type: SymbolType, doc: String) {
        // Base path from file location
        let mut path = self.current_module_path();

        // Add current traversal path for nested items
        for part in &self.current_path {
            path.push(part.clone());
        }

        // Add the item name
        path.push(name.to_string());
        let full_path = path.join("::");

        self.symbols.insert(
            full_path.clone(),
            SymbolInfo {
                identifier: name.to_string(),
                symbol_type,
                hover_text: doc,
                source_vertex_id: format!("{:?}", self.current_file),
                span_bytes: None, // Old add_symbol, span is None
            },
        );
    }
    */
    // New method to include span
    fn add_symbol_with_span(
        &mut self,
        name: &str,
        symbol_type: SymbolType,
        doc: String,
        span: Option<(usize, usize)>,
    ) {
        // Base path from file location
        let mut path = self.current_module_path();

        // Add current traversal path for nested items
        for part in &self.current_path {
            path.push(part.clone());
        }

        // Add the item name
        path.push(name.to_string());
        let full_path = path.join("::");

        self.symbols.insert(
            full_path.clone(),
            SymbolInfo {
                identifier: name.to_string(),
                symbol_type,
                hover_text: doc,
                source_vertex_id: format!("{:?}", self.current_file),
                span_lines: span, // Changed from span_bytes
            },
        );
    }

    // Helper to extract line numbers from syn::Span
    fn get_span_lines<T: syn::spanned::Spanned>(item: &T) -> Option<(usize, usize)> {
        let span = item.span();
        Some((span.start().line, span.end().line))
    }

    fn enter_module(&mut self, module_name: &str) {
        self.current_path.push(module_name.to_string());
    }

    fn exit_module(&mut self) {
        self.current_path.pop();
    }

    fn extract_doc(&self, attrs: &[syn::Attribute]) -> String {
        let mut doc = String::new();
        for attr in attrs {
            if attr.path().is_ident("doc") {
                if let Ok(meta) = attr.meta.require_name_value() {
                    if let syn::Expr::Lit(expr_lit) = &meta.value {
                        if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                            let doc_line = lit_str.value();

                            // If docstring has a '#', only include content up to that
                            if let Some(idx) = doc_line.find('#') {
                                if !doc.is_empty() {
                                    doc.push('\n');
                                }
                                doc.push_str(doc_line[..idx].trim());
                                continue;
                            }

                            if !doc.is_empty() {
                                doc.push('\n');
                            }
                            doc.push_str(doc_line.trim());
                        }
                    }
                }
            }
        }
        doc
    }

    #[allow(dead_code)]
    fn has_async_attr(&self, attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| attr.path().is_ident("async"))
    }
}

impl<'ast> Visit<'ast> for SymbolCollector {
    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        let doc = self.extract_doc(&i.attrs);
        let span = syn::spanned::Spanned::span(i);
        let span_bytes = span.start().line.checked_sub(1).and_then(|start_line| {
            span.end()
                .line
                .checked_sub(1)
                .map(|end_line| (start_line, end_line))
        });
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Struct, doc, span_bytes);

        // Add fields
        if let syn::Fields::Named(fields) = &i.fields {
            for field in &fields.named {
                if let Some(ident) = &field.ident {
                    let field_doc = self.extract_doc(&field.attrs);
                    let field_span_bytes = Self::get_span_lines(field);

                    // Format field type if available
                    let mut hover_text = field_doc.clone(); // Clone field_doc as it's used later by itself
                    if let syn::Type::Path(type_path) = &field.ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            hover_text = format!("{} (type: {})", field_doc, segment.ident);
                        }
                    }
                    // Need to construct the full path for the field before calling
                    // add_symbol_with_span
                    let mut field_path_parts = self.current_module_path();
                    for part in &self.current_path {
                        field_path_parts.push(part.clone());
                    }
                    field_path_parts.push(i.ident.to_string());
                    // field_path_parts.push(ident.to_string()); // add_symbol_with_span will add
                    // the ident

                    // Temporarily change current_path for add_symbol_with_span to correctly build
                    // the field's full path
                    let original_current_path = self.current_path.clone();
                    let mut temp_path = self.current_path.clone();
                    temp_path.push(i.ident.to_string());
                    self.current_path = temp_path;

                    self.add_symbol_with_span(
                        ident.to_string().as_str(),
                        SymbolType::Field,
                        hover_text,
                        field_span_bytes,
                    );

                    self.current_path = original_current_path; // Restore current_path
                }
            }
        }

        visit::visit_item_struct(self, i);
    }

    fn visit_item_enum(&mut self, i: &'ast ItemEnum) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Enum, doc, span_bytes);

        // Add variants
        for variant in &i.variants {
            let variant_doc = self.extract_doc(&variant.attrs);
            let variant_span_bytes = Self::get_span_lines(variant);

            // Temporarily change current_path for add_symbol_with_span to correctly build the
            // variant's full path
            let original_current_path = self.current_path.clone();
            let mut temp_path = self.current_path.clone();
            temp_path.push(i.ident.to_string());
            self.current_path = temp_path;

            self.add_symbol_with_span(
                variant.ident.to_string().as_str(),
                SymbolType::Variant,
                variant_doc,
                variant_span_bytes,
            );

            self.current_path = original_current_path; // Restore current_path
        }

        visit::visit_item_enum(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        let doc = self.extract_doc(&i.attrs);

        // First parameter being &self or &mut self indicates a method
        let is_method = i
            .sig
            .inputs
            .iter()
            .next()
            .map_or(false, |arg| matches!(arg, syn::FnArg::Receiver(_)));

        let symbol_type = if is_method {
            SymbolType::Method
        } else {
            SymbolType::Function
        };

        // Capture function signature with arguments
        let mut signature = String::new();

        // Check if function is async
        let is_async = i.sig.asyncness.is_some();
        if is_async {
            signature.push_str("async ");
        }

        signature.push_str("fn ");
        signature.push_str(&i.sig.ident.to_string());
        signature.push('(');

        // Add function arguments
        for (idx, input) in i.sig.inputs.iter().enumerate() {
            if idx > 0 {
                signature.push_str(", ");
            }

            match input {
                syn::FnArg::Receiver(r) => {
                    if r.reference.is_some() {
                        signature.push('&');
                        if let Some(lt) = &r.lifetime() {
                            signature.push('\'');
                            signature.push_str(&lt.ident.to_string());
                            signature.push(' ');
                        }
                        if r.mutability.is_some() {
                            signature.push_str("mut ");
                        }
                    } else if r.mutability.is_some() {
                        signature.push_str("mut ");
                    }
                    signature.push_str("self");
                }
                syn::FnArg::Typed(pat_type) => {
                    if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                        signature.push_str(&pat_ident.ident.to_string());
                        signature.push_str(": ");

                        // Add type information
                        let type_str = match &*pat_type.ty {
                            syn::Type::Path(type_path) => {
                                if let Some(segment) = type_path.path.segments.last() {
                                    segment.ident.to_string()
                                } else {
                                    "unknown".to_string()
                                }
                            }
                            syn::Type::Reference(type_ref) => {
                                let mut ref_str = String::from("&");
                                if type_ref.mutability.is_some() {
                                    ref_str.push_str("mut ");
                                }
                                if let syn::Type::Path(type_path) = &*type_ref.elem {
                                    if let Some(segment) = type_path.path.segments.last() {
                                        ref_str.push_str(&segment.ident.to_string());
                                    } else {
                                        ref_str.push_str("unknown");
                                    }
                                } else {
                                    ref_str.push_str("unknown");
                                }
                                ref_str
                            }
                            _ => "unknown".to_string(),
                        };

                        signature.push_str(&type_str);
                    }
                }
            }
        }

        signature.push(')');

        // Add return type if not unit
        if let syn::ReturnType::Type(_, return_type) = &i.sig.output {
            signature.push_str(" -> ");
            let return_str = match &**return_type {
                syn::Type::Path(type_path) => {
                    if let Some(segment) = type_path.path.segments.last() {
                        segment.ident.to_string()
                    } else {
                        "unknown".to_string()
                    }
                }
                _ => "unknown".to_string(),
            };
            signature.push_str(&return_str);
        }

        // Add attributes to hover text
        let hover_text = format!("{}\nSignature: {}", doc, signature);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(
            &i.sig.ident.to_string(),
            symbol_type,
            hover_text,
            span_bytes,
        );
        visit::visit_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);

        // Capture function signature
        let mut signature = String::new();

        // Check if function is async
        let is_async = i.sig.asyncness.is_some();
        if is_async {
            signature.push_str("async ");
        }

        signature.push_str("fn ");
        signature.push_str(&i.sig.ident.to_string());
        signature.push('(');

        // Add function arguments
        for (idx, input) in i.sig.inputs.iter().enumerate() {
            if idx > 0 {
                signature.push_str(", ");
            }

            match input {
                syn::FnArg::Receiver(r) => {
                    if r.reference.is_some() {
                        signature.push('&');
                        if let Some(lt) = &r.lifetime() {
                            signature.push('\'');
                            signature.push_str(&lt.ident.to_string());
                            signature.push(' ');
                        }
                        if r.mutability.is_some() {
                            signature.push_str("mut ");
                        }
                    } else if r.mutability.is_some() {
                        signature.push_str("mut ");
                    }
                    signature.push_str("self");
                }
                syn::FnArg::Typed(pat_type) => {
                    // Same type handling as in visit_item_fn
                    if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                        signature.push_str(&pat_ident.ident.to_string());
                        signature.push_str(": ");

                        // Type extraction (same as in visit_item_fn)
                        let type_str = match &*pat_type.ty {
                            syn::Type::Path(type_path) => {
                                if let Some(segment) = type_path.path.segments.last() {
                                    segment.ident.to_string()
                                } else {
                                    "unknown".to_string()
                                }
                            }
                            syn::Type::Reference(type_ref) => {
                                let mut ref_str = String::from("&");
                                if type_ref.mutability.is_some() {
                                    ref_str.push_str("mut ");
                                }
                                if let syn::Type::Path(type_path) = &*type_ref.elem {
                                    if let Some(segment) = type_path.path.segments.last() {
                                        ref_str.push_str(&segment.ident.to_string());
                                    } else {
                                        ref_str.push_str("unknown");
                                    }
                                } else {
                                    ref_str.push_str("unknown");
                                }
                                ref_str
                            }
                            _ => "unknown".to_string(),
                        };

                        signature.push_str(&type_str);
                    }
                }
            }
        }

        signature.push(')');

        // Return type handling
        if let syn::ReturnType::Type(_, return_type) = &i.sig.output {
            signature.push_str(" -> ");
            let return_str = match &**return_type {
                syn::Type::Path(type_path) => {
                    if let Some(segment) = type_path.path.segments.last() {
                        segment.ident.to_string()
                    } else {
                        "unknown".to_string()
                    }
                }
                _ => "unknown".to_string(),
            };
            signature.push_str(&return_str);
        }

        let hover_text = format!("{}\nSignature: {}", doc, signature);

        // Add method to symbol map with proper type
        self.add_symbol_with_span(
            &i.sig.ident.to_string(),
            SymbolType::Method,
            hover_text,
            span_bytes,
        );

        // Continue visiting nested items
        visit::visit_impl_item_fn(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast ItemTrait) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Trait, doc, span_bytes);

        self.enter_module(&i.ident.to_string());
        visit::visit_item_trait(self, i);
        self.exit_module();
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        // Get the type name for the impl
        let type_name = match &*i.self_ty {
            syn::Type::Path(type_path) => {
                if let Some(segment) = type_path.path.segments.last() {
                    segment.ident.to_string()
                } else {
                    "Unknown".to_string()
                }
            }
            _ => "Unknown".to_string(),
        };

        // Store original path components
        let original_path = self.current_path.clone();

        // Use the type name directly rather than "impl Type"
        self.enter_module(&type_name);

        // Visit all impl items
        visit::visit_item_impl(self, i);

        // Restore the original path
        self.current_path = original_path;
    }

    fn visit_item_const(&mut self, i: &'ast ItemConst) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Const, doc, span_bytes);
        visit::visit_item_const(self, i);
    }

    fn visit_item_static(&mut self, i: &'ast ItemStatic) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Static, doc, span_bytes);
        visit::visit_item_static(self, i);
    }

    fn visit_item_mod(&mut self, i: &'ast ItemMod) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::Module, doc, span_bytes);

        if let Some(content) = &i.content {
            self.enter_module(&i.ident.to_string());

            for item in &content.1 {
                visit::visit_item(self, item);
            }

            self.exit_module();
        }
    }

    fn visit_item_type(&mut self, i: &'ast ItemType) {
        let doc = self.extract_doc(&i.attrs);
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&i.ident.to_string(), SymbolType::TypeAlias, doc, span_bytes);
        visit::visit_item_type(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast ItemMacro) {
        let doc = self.extract_doc(&i.attrs);
        let name = i
            .ident
            .as_ref()
            .map_or("unnamed_macro".to_string(), |id| id.to_string());
        let span_bytes = Self::get_span_lines(i);
        self.add_symbol_with_span(&name, SymbolType::Macro, doc, span_bytes);
        visit::visit_item_macro(self, i);
    }
}

/// Determine crate name from Cargo.toml
fn get_crate_name(project_root: &Path) -> String {
    let cargo_path = project_root.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_path) {
        if let Some(name_line) = content.lines().find(|line| line.trim().starts_with("name")) {
            if let Some(equals_pos) = name_line.find('=') {
                let name = name_line[equals_pos + 1..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                // Convert hyphens to underscores for Rust crate naming
                return name.replace('-', "_");
            }
        }
    }
    // Default name
    "crate".to_string()
}

/// Symbol types we can identify
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum SymbolType {
    Struct,
    Enum,
    Function,
    Trait,
    Impl,
    Const,
    Static,
    Module,
    Field,
    Variant,
    Method,
    TypeAlias,
    Macro,
    Crate,
    Unknown(String),
}

/// Information about a symbol
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SymbolInfo {
    pub identifier: String,
    pub symbol_type: SymbolType,
    pub hover_text: String,
    pub source_vertex_id: String,
    pub span_lines: Option<(usize, usize)>, // Changed from span_bytes to span_lines (line numbers)
}

/// A hierarchical representation of symbols
#[derive(Debug, Clone)]
pub enum SymbolNode {
    EnumVariant {
        name: String,
        doc: String,
    },
    Field {
        name: String,
        doc: String,
    },
    Function {
        name: String,
        signature: String,
        doc: String,
    },
    Leaf {
        name: String,
        symbol_type: SymbolType,
        doc: String,
    },
}

/// Symbol hierarchy
#[derive(Debug, Default)]
pub struct SymbolHierarchy {
    symbols: BTreeMap<String, BTreeMap<String, SymbolNode>>,
}

impl SymbolHierarchy {
    pub fn new() -> Self {
        Self {
            symbols: BTreeMap::new(),
        }
    }

    pub fn add_symbol(&mut self, path: &str, symbol_type: &SymbolType, doc: &str) {
        let parts: Vec<&str> = path.rsplitn(2, "::").collect();

        // Extract name and parent path
        let (name, _parent) = match parts.len() {
            1 => (parts[0], ""),
            _ => (parts[0], parts[1]),
        };

        // For enum variants, extract the parent enum path
        if matches!(symbol_type, SymbolType::Variant) {
            if let Some(enum_path) = extract_parent_enum(path, name) {
                // let doc = extract_doc_comment(doc); // Original doc parameter used below for
                // SymbolNode::Leaf
                self.symbols
                    .entry(format!("ENUMS"))
                    .or_default()
                    .entry(enum_path.to_string())
                    .or_insert_with(|| SymbolNode::Leaf {
                        name: enum_path.to_string(),
                        symbol_type: SymbolType::Enum,
                        doc: extract_doc_comment(doc), // Use extracted doc for the enum leaf
                    });

                // Add variant to enum
                return;
            }
        }

        // For other symbols
        let category = format!("{:?}S", symbol_type).to_uppercase();

        match symbol_type {
            SymbolType::Function => {
                let signature = extract_function_signature(doc); // doc is the original hover_text here
                let extracted_doc = extract_doc_comment(doc); // This is the actual doc part
                self.symbols
                    .entry(category)
                    .or_default()
                    .entry(path.to_string())
                    .or_insert_with(|| SymbolNode::Function {
                        name: name.to_string(),
                        signature,
                        doc: extracted_doc,
                    });
            }
            SymbolType::Field => {
                if let Some(_struct_path) = extract_parent_struct(path, name) {
                    // _struct_path unused
                    let _extracted_doc = extract_doc_comment(doc); // doc is original hover_text,
                                                                   // _extracted_doc is unused
                                                                   // Add field to struct
                }
            }
            _ => {
                let extracted_doc = extract_doc_comment(doc); // doc is original hover_text
                self.symbols
                    .entry(category)
                    .or_default()
                    .entry(path.to_string())
                    .or_insert_with(|| SymbolNode::Leaf {
                        name: name.to_string(),
                        symbol_type: symbol_type.clone(),
                        doc: extracted_doc,
                    });
            }
        }
    }

    pub fn from_symbol_map(symbols: &BTreeMap<String, SymbolInfo>) -> Self {
        let mut hierarchy = Self::new();

        // First pass: add all base symbols
        for (path, info) in symbols {
            hierarchy.add_symbol(path, &info.symbol_type, &info.hover_text);
        }

        // Second pass: organize variants and fields
        for (path, info) in symbols {
            match &info.symbol_type {
                SymbolType::Variant => {
                    if let Some(enum_path) = extract_parent_path(path) {
                        let doc = extract_doc_comment(&info.hover_text);
                        hierarchy.add_enum_variant(&enum_path, path, doc);
                    }
                }
                SymbolType::Field => {
                    if let Some(struct_path) = extract_parent_path(path) {
                        let doc = extract_doc_comment(&info.hover_text);
                        hierarchy.add_struct_field(&struct_path, path, doc);
                    }
                }
                _ => {}
            }
        }

        hierarchy
    }

    fn add_enum_variant(&mut self, enum_path: &str, variant_path: &str, doc: String) {
        // Extract variant name from path
        let name = variant_path.rsplit("::").next().unwrap_or(variant_path);

        // Find the enum in the hierarchy
        if let Some(enums) = self.symbols.get_mut("ENUMS") {
            if let Some(SymbolNode::Leaf { .. }) = enums.get_mut(enum_path) {
                // Add the variant as a child of the enum
                // If this is the first variant, initialize the enum as a parent
                // TODO: This logic for replacing the enum leaf is incomplete and might not
                // correctly handle multiple variants or preserve the original
                // enum's documentation properly. For now, focusing on using the
                // variant's 'doc'.
                let mut variants = BTreeMap::new();
                variants.insert(
                    name.to_string(),
                    SymbolNode::EnumVariant {
                        name: name.to_string(),
                        doc,
                    },
                );

                // Replace the enum leaf with a parent node
                // (In a real implementation, you'd have a proper parent node type)
            }
        }
    }

    fn add_struct_field(&mut self, _struct_path: &str, _field_path: &str, _doc: String) {
        // Similar implementation to add_enum_variant
    }

    pub fn format(&self) -> String {
        let mut result = String::new();
        result.push_str("crate lsp: Found symbols\n");

        for (category, symbols) in &self.symbols {
            if symbols.is_empty() {
                continue;
            }

            result.push_str(&format!("{}:\n", category));

            for (path, node) in symbols {
                match node {
                    SymbolNode::Leaf { doc, .. } => {
                        result.push_str(&format!("  {}", path));
                        if !doc.is_empty() {
                            result.push_str(&format!(" /* {} */", doc));
                        }
                        result.push('\n');
                    }
                    SymbolNode::Function { signature, doc, .. } => {
                        result.push_str(&format!("  {}: {}", path, signature));
                        if !doc.is_empty() {
                            result.push_str(&format!(" /* {} */", doc));
                        }
                        result.push('\n');
                    }
                    // Handle other node types
                    _ => {}
                }
            }

            result.push('\n');
        }

        result
    }
}

fn extract_parent_path(path: &str) -> Option<String> {
    path.rsplitn(2, "::").nth(1).map(|s| s.to_string())
}

fn extract_parent_enum(path: &str, _variant_name: &str) -> Option<String> {
    // Logic to extract parent enum path from a variant path
    path.rsplitn(2, "::").nth(1).map(|s| s.to_string())
}

fn extract_parent_struct(path: &str, _field_name: &str) -> Option<String> {
    // Logic to extract parent struct path from a field path
    path.rsplitn(2, "::").nth(1).map(|s| s.to_string())
}

/// Parse a single Rust file and collect symbols
fn parse_file(path: &Path, collector: &mut SymbolCollector) -> Result<(), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

    collector.current_file = path.to_path_buf();

    let syntax = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse file {}: {}", path.display(), e))?;

    collector.visit_file(&syntax);

    Ok(())
}

/// Walk through a directory and parse all Rust files
pub fn parse_directory(dir_path: &Path) -> Result<BTreeMap<String, SymbolInfo>, String> {
    let crate_name = get_crate_name(dir_path);
    let mut collector = SymbolCollector::new(&crate_name, dir_path);

    let walker = WalkBuilder::new(dir_path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path();

                if path.extension().map_or(false, |ext| ext == "rs") {
                    if let Err(e) = parse_file(path, &mut collector) {
                        eprintln!("Error processing {}: {}", path.display(), e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error walking directory: {}", e);
            }
        }
    }

    Ok(collector.get_symbols().clone())
}

/// Main function to gather symbols from a project
pub fn gather_project_symbols(project_root: &Path) -> Result<SymbolHierarchy, String> {
    let symbols = parse_directory(project_root)?;
    Ok(SymbolHierarchy::from_symbol_map(&symbols))
}

/// Format symbols in a human-readable way
pub fn format_symbols(symbols: &BTreeMap<String, SymbolInfo>) -> String {
    organize_symbols(symbols)
}

/// Get symbols as a string in the same format as the LSIF output
pub fn get_project_symbols_string(project_root: &Path) -> Result<String, String> {
    let symbols = parse_directory(project_root)?;
    Ok(format_symbols(&symbols))
}

/// Creates a better organized symbol hierarchy
pub fn organize_symbols(symbols: &BTreeMap<String, SymbolInfo>) -> String {
    // Normalize symbol types
    let mut normalized_symbols: BTreeMap<String, SymbolInfo> = BTreeMap::new();
    for (path, info) in symbols {
        let mut normalized_info = info.clone();
        normalized_info.symbol_type = normalize_symbol_type(info);
        normalized_symbols.insert(path.clone(), normalized_info);
    }

    // Map enum variants to parent enums
    let mut enum_variants: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parent_enums: BTreeSet<String> = BTreeSet::new();

    for (path, info) in &normalized_symbols {
        if path.matches("::").count() == 2 {
            let parts: Vec<&str> = path.split("::").collect();
            if parts.len() == 3 {
                let enum_path = format!("{}::{}", parts[0], parts[1]);
                if let Some(enum_info) = normalized_symbols.get(&enum_path) {
                    if enum_info.symbol_type == SymbolType::Enum {
                        enum_variants
                            .entry(enum_path.clone())
                            .or_insert_with(Vec::new)
                            .push(path.clone());
                        parent_enums.insert(path.clone());
                    }
                }
            }
        }
    }

    // Map fields to parent structs
    let mut struct_fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parent_structs: BTreeSet<String> = BTreeSet::new();

    for (path, info) in &normalized_symbols {
        if path.matches("::").count() == 2 && info.symbol_type == SymbolType::Field {
            let parts: Vec<&str> = path.split("::").collect();
            if parts.len() == 3 {
                let struct_path = format!("{}::{}", parts[0], parts[1]);
                if let Some(struct_info) = normalized_symbols.get(&struct_path) {
                    if struct_info.symbol_type == SymbolType::Struct {
                        struct_fields
                            .entry(struct_path.clone())
                            .or_insert_with(Vec::new)
                            .push(path.clone());
                        parent_structs.insert(path.clone());
                    }
                }
            }
        }
    }

    // Organize symbols by type
    let mut output = String::from("crate lsp: Found symbols\n");
    let mut by_type: BTreeMap<String, Vec<(String, &SymbolInfo)>> = BTreeMap::new();

    // Group symbols by type
    for (path, info) in &normalized_symbols {
        if parent_enums.contains(path) || parent_structs.contains(path) {
            continue; // Skip enum variants and struct fields
        }

        let category = format!("{:?}S", info.symbol_type).to_uppercase();
        by_type
            .entry(category)
            .or_default()
            .push((path.clone(), info));
    }

    // Generate output
    for (category, symbols) in by_type {
        if symbols.is_empty() {
            continue;
        }

        output.push_str(&format!("{}:\n", category));

        for (path, info) in symbols {
            match info.symbol_type {
                SymbolType::Enum => {
                    let doc = extract_doc_comment(&info.hover_text);
                    output.push_str(&format!("  {}", path));
                    if !doc.is_empty() {
                        output.push_str(&format!(" /* {} */", doc));
                    }
                    output.push_str("\n");

                    // Add enum variants
                    if let Some(variants) = enum_variants.get(&path) {
                        for variant_path in variants {
                            let variant_name = extract_name(variant_path);
                            if let Some(variant_info) = normalized_symbols.get(variant_path) {
                                let doc = extract_doc_comment(&variant_info.hover_text);
                                output.push_str(&format!("    {}", variant_name));
                                if !doc.is_empty() {
                                    output.push_str(&format!(" /* {} */", doc));
                                }
                                output.push_str("\n");
                            }
                        }
                    }
                }
                SymbolType::Struct => {
                    let doc = extract_doc_comment(&info.hover_text);
                    output.push_str(&format!("  {}", path));
                    if !doc.is_empty() {
                        output.push_str(&format!(" /* {} */", doc));
                    }
                    output.push_str("\n");

                    // Add struct fields
                    if let Some(fields) = struct_fields.get(&path) {
                        output.push_str("    ");
                        let field_names: Vec<_> = fields
                            .iter()
                            .filter_map(|field_path| {
                                normalized_symbols.get(field_path).map(|field_info| {
                                    let name = extract_name(field_path);
                                    let doc = extract_doc_comment(&field_info.hover_text);
                                    if !doc.is_empty() {
                                        format!("{} /* {} */", name, doc)
                                    } else {
                                        name
                                    }
                                })
                            })
                            .collect();
                        output.push_str(&field_names.join("\n    "));
                        output.push_str("\n");
                    }
                }
                SymbolType::Function => {
                    let signature = extract_function_signature(&info.hover_text);
                    let doc = extract_doc_comment(&info.hover_text);
                    output.push_str(&format!("  {}: {}", path, signature));
                    if !doc.is_empty() {
                        output.push_str(&format!(" /* {} */", doc));
                    }
                    output.push_str("\n");
                }
                _ => {
                    let doc = extract_doc_comment(&info.hover_text);
                    output.push_str(&format!("  {}", path));
                    if !doc.is_empty() {
                        output.push_str(&format!(" /* {} */", doc));
                    }
                    output.push_str("\n");
                }
            }
        }
        output.push_str("\n");
    }

    output
}

/// Normalizes the Unknown symbol types to their proper types
fn normalize_symbol_type(symbol: &SymbolInfo) -> SymbolType {
    match &symbol.symbol_type {
        SymbolType::Unknown(type_str) => match type_str.to_lowercase().as_str() {
            "struct" => SymbolType::Struct,
            "enum" => SymbolType::Enum,
            "function" => SymbolType::Function,
            "trait" => SymbolType::Trait,
            "impl" => SymbolType::Impl,
            "const" => SymbolType::Const,
            "static" => SymbolType::Static,
            "module" => SymbolType::Module,
            "field" => SymbolType::Field,
            "variant" => SymbolType::Variant,
            "method" => SymbolType::Method,
            "typealias" => SymbolType::TypeAlias,
            "macro" => SymbolType::Macro,
            "crate" => SymbolType::Crate,
            _ => SymbolType::Unknown(type_str.clone()),
        },
        other => other.clone(),
    }
}

/// Extracts documentation from hover text
fn extract_doc_comment(hover_text: &str) -> String {
    // Check for documentation after --- marker
    if let Some(doc_section) = hover_text.split("---").nth(1) {
        return doc_section.trim().to_string();
    }
    String::new()
}

#[allow(dead_code)]
fn extract_rust_code_block(markdown: &str) -> String {
    // Find all Rust code blocks using simple string parsing
    let blocks = extract_all_rust_code_blocks(markdown);

    // Take the second code block if available (first is usually just namespace)
    if blocks.len() > 1 {
        return blocks[1].clone();
    }

    // If there's no second block, use the first one
    blocks.into_iter().next().unwrap_or_default()
}

/// Extract all rust code blocks from markdown
fn extract_all_rust_code_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = markdown;

    while let Some(start_marker_pos) = remaining.find("```rust") {
        let after_marker = &remaining[start_marker_pos + 7..]; // Skip "```rust"

        // Skip to end of line (past any additional markers)
        let code_start = after_marker.find('\n').map(|i| i + 1).unwrap_or(0);
        let code_content = &after_marker[code_start..];

        // Find the closing ```
        if let Some(end_pos) = code_content.find("```") {
            let code = code_content[..end_pos].trim().to_string();
            blocks.push(code);
            remaining = &code_content[end_pos + 3..];
        } else {
            break;
        }
    }

    // Also check for ```rs blocks
    remaining = markdown;
    while let Some(start_marker_pos) = remaining.find("```rs\n") {
        let after_marker = &remaining[start_marker_pos + 6..]; // Skip "```rs\n"

        if let Some(end_pos) = after_marker.find("```") {
            let code = after_marker[..end_pos].trim().to_string();
            blocks.push(code);
            remaining = &after_marker[end_pos + 3..];
        } else {
            break;
        }
    }

    blocks
}

/// Extracts function signature from hover text
fn extract_function_signature(hover_text: &str) -> String {
    // Get the second code block if available (first is usually namespace)
    let blocks = extract_all_rust_code_blocks(hover_text);
    if blocks.len() > 1 {
        return blocks[1].clone();
    }
    String::new()
}

/// Extracts name from path
fn extract_name(path: &str) -> String {
    path.split("::").last().unwrap_or(path).to_string()
}

/// Finds the byte span of a specific Rust item within file content.
///
/// `item_path_suffix`: The item name (e.g., "MyStruct" or "my_function").
/// `file_path_for_module_context`: The actual path to the Rust file (e.g., `src/module.rs`).
/// `project_root`: Path to the project root.
pub fn find_item_span(
    file_content: &str,
    item_path_suffix: &str,
    file_path_for_module_context: &Path,
    project_root: &Path,
) -> Result<Option<(usize, usize)>, String> {
    let syntax_tree = syn::parse_file(file_content)
        .map_err(|e| format!("Failed to parse file content: {}", e))?;

    let crate_name = get_crate_name(project_root);
    let mut collector = SymbolCollector::new(&crate_name, project_root);
    collector.current_file = file_path_for_module_context.to_path_buf();
    collector.visit_file(&syntax_tree);

    let collected_symbols = collector.get_symbols();

    // We need to find a symbol whose fully qualified name, when considering its module path,
    // ends with the item_path_suffix.
    // Example: If item_path_suffix is "my_function" and file is src/utils.rs (crate_name
    // "my_crate"), we are looking for a symbol like "my_crate::utils::my_function".
    // If item_path_suffix is "MyStruct::new", we are looking for "my_crate::module::MyStruct::new".
    // The SymbolCollector stores full paths like "crate_name::module::ItemName" or
    // "crate_name::module::StructName::FieldName".

    for (full_path, symbol_info) in collected_symbols {
        // Check if the full_path ends with item_path_suffix.
        // We need to be careful if item_path_suffix could contain '::' itself (e.g. for methods or
        // associated items). A simple ends_with check might be too naive if
        // item_path_suffix is just "ItemName" and full_path is
        // "crate::module::ItemName::some_method". The LLM is instructed to provide
        // "ItemName" or "function_name" as suffix. If item_path_suffix contains "::", it
        // implies a nested item, e.g. "MyStruct::my_method"

        let path_parts: Vec<&str> = full_path.split("::").collect();
        let suffix_parts: Vec<&str> = item_path_suffix.split("::").collect();

        if path_parts.ends_with(&suffix_parts) {
            // Additionally, ensure that the item being matched is directly in the module,
            // not a sub-item if the suffix is simple.
            // Example: if item_path_suffix is "MyStruct", full_path "my_crate::my_mod::MyStruct"
            // should match, but "my_crate::my_mod::MyStruct::field" should not.
            // This is implicitly handled if suffix_parts.len() == 1 and path_parts.len() matches
            // the expected module depth + 1. Or more simply, if the symbol's own
            // identifier matches the last part of the suffix.
            if symbol_info.identifier == *suffix_parts.last().unwrap_or(&"") {
                if let Some(span) = symbol_info.span_lines {
                    return Ok(Some(span));
                }
            }
        }
    }

    Ok(None) // Item not found
}

/// Extracts filenames and their corresponding Rust code blocks from Markdown content.
///
/// Assumes filenames are on lines like "File: path/to/file.rs" or "path/to/file.rs",
/// followed by a Rust code block.
pub fn extract_file_code_blocks_from_markdown(
    markdown_content: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut results = Vec::new();
    let lines: Vec<&str> = markdown_content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Check if line contains a .rs filename (with optional "File: " prefix)
        let file_path = if let Some(path) = extract_rs_filename_from_line(line) {
            path
        } else {
            i += 1;
            continue;
        };

        // Look for a rust code block on the next line
        i += 1;
        if i >= lines.len() {
            break;
        }

        let next_line = lines[i].trim();
        if !next_line.starts_with("```rust") && !next_line.starts_with("```rs") {
            continue;
        }

        // Found a code block, extract its content
        i += 1;
        let code_start = i;

        while i < lines.len() && !lines[i].trim().starts_with("```") {
            i += 1;
        }

        if i > code_start {
            let code_block = lines[code_start..i].join("\n").trim().to_string();
            results.push((file_path, code_block));
        }

        i += 1; // Skip the closing ```
    }

    Ok(results)
}

/// Helper: extract a .rs filename from a line like "File: path/to/file.rs" or just
/// "path/to/file.rs"
fn extract_rs_filename_from_line(line: &str) -> Option<String> {
    let line = line.trim();

    // Check for "File: " prefix
    let path_part = if line.starts_with("File:") {
        line.strip_prefix("File:")?.trim()
    } else {
        line
    };

    // Must end with .rs and be a valid-looking path
    if !path_part.ends_with(".rs") {
        return None;
    }

    // Basic validation: should contain only path characters
    let valid = path_part
        .chars()
        .all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '_' || c == '-');

    if valid && !path_part.is_empty() {
        Some(path_part.to_string())
    } else {
        None
    }
}

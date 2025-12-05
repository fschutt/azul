# TypeIndex vs WorkspaceIndex - Unifikationsbericht

## Übersicht

Das Projekt hat **zwei separate Type-Indexing-Systeme**, die parallel existieren:

| Feature | TypeIndex (`autofix/type_index.rs`) | WorkspaceIndex (`patch/index.rs`) |
|---------|-------------------------------------|-----------------------------------|
| **Zweck** | Autofix Debug-Befehle, Typauflösung | Patch-Generierung, `convert_type_info_to_class_patch()` |
| **Parser** | `syn` (AST-basiert) | `syn` (AST-basiert) |
| **Makro-Unterstützung** | ✅ `MacroGenerated` Variant | ❌ Keine Makro-Unterstützung |
| **Indexierte Typen** | ~2236 | Weniger (fehlen Makro-Typen) |

---

## 1. TypeIndex (korrekt)

**Datei:** `doc/src/autofix/type_index.rs`

**TypeDefKind-Varianten:**
```rust
pub enum TypeDefKind {
    Struct { fields, has_repr_c, generic_params, derives },
    Enum { variants, has_repr_c, generic_params, derives },
    TypeAlias { target },
    CallbackTypedef { args, returns },
    MacroGenerated { source_macro, base_type, kind },  // ← WICHTIG!
}

pub enum MacroGeneratedKind {
    Vec,              // impl_vec!(T, TyVec, TyDestructor)
    VecDestructor,    // DestructorType enum
    VecDestructorType,// DestructorTypeType callback
    Option,           // impl_option!(T, OptionT, ...)
    OptionEnumWrapper,
    Result,           // impl_result!(Ok, Err, ResultType)
    CallbackWrapper,  // impl_callback!(...)
    CallbackValue,
}
```

**Stärken:**
- Parst alle bekannten Makros: `impl_vec!`, `impl_option!`, `impl_result!`, `impl_callback!`, `define_dimension_property!`, `define_position_property!`, usw.
- Expandiert Makros zu echten Typdefinitionen
- Wird von `autofix debug type X` verwendet → **korrekte Ergebnisse**

---

## 2. WorkspaceIndex (unvollständig)

**Datei:** `doc/src/patch/index.rs`

**TypeKind-Varianten:**
```rust
pub enum TypeKind {
    Struct { fields, has_repr_c, doc, generic_params, implemented_traits, derives },
    Enum { variants, has_repr_c, doc, generic_params },
    TypeAlias { target, doc },
    CallbackTypedef { args, returns, doc },
    // KEIN MacroGenerated!
}
```

**Schwächen:**
- **Keine Makro-Unterstützung** → Typen wie `DebugMessageVec`, `ResultU8VecDecodeImageError`, usw. werden nicht gefunden
- `find_type_by_string_search()` versucht Makros per Textsuche zu finden, aber erstellt leere Struct-Definitionen ohne Felder
- Wird von `convert_type_info_to_class_patch()` verwendet → **fehlende struct_fields/enum_fields**

---

## 3. Das Problem

Die Funktion `convert_type_info_to_class_patch()` in `workspace.rs` verwendet `WorkspaceIndex`:

```rust
fn convert_type_info_to_class_patch(
    type_info: &ParsedTypeInfo,  // ← kommt von WorkspaceIndex
    ...
) -> ClassPatch {
    match &type_info.kind {
        TypeKind::Struct { fields, ... } => { ... }
        TypeKind::Enum { variants, ... } => { ... }
        TypeKind::TypeAlias { target, ... } => { ... }
        TypeKind::CallbackTypedef { args, returns, ... } => { ... }
        // KEIN MacroGenerated-Handling!
    }
}
```

Wenn ein Typ via Makro generiert wird, findet `WorkspaceIndex` ihn entweder:
1. **Gar nicht** → Typ fehlt komplett
2. **Via String-Suche** → Erstellt leeres Struct ohne Felder

---

## 4. Lösung: Vereinheitlichung

### Option A: TypeIndex als primären Index verwenden (GEWÄHLT)

1. `TypeIndex` erweitern, um `MacroGenerated` zu "expandieren":
   - `MacroGeneratedKind::Vec` → echtes `Struct { ptr, len, cap, destructor }`
   - `MacroGeneratedKind::VecDestructor` → echtes `Enum { DefaultRust, NoDestructor, External }`
   - `MacroGeneratedKind::VecDestructorType` → echtes `CallbackTypedef`
   - usw.

2. `convert_type_info_to_class_patch()` anpassen, um `TypeDefinition` statt `ParsedTypeInfo` zu akzeptieren

3. `WorkspaceIndex` entfernen oder nur für spezifische Zwecke behalten

### Option B: MacroGenerated zu WorkspaceIndex hinzufügen (NICHT GEWÄHLT)

1. `TypeKind::MacroGenerated` zu `patch/index.rs` hinzufügen
2. Makro-Parsing-Logik von `type_index.rs` nach `index.rs` kopieren
3. `convert_type_info_to_class_patch()` für MacroGenerated erweitern

---

## 5. Implementierungsplan für Option A

### Schritt 1: Funktion `expand_macro_generated()` erstellen

In `type_index.rs` eine Funktion hinzufügen, die `MacroGeneratedKind` in echte Typen expandiert:

```rust
pub fn expand_macro_generated(&self) -> TypeDefKind {
    match &self.kind {
        TypeDefKind::MacroGenerated { kind, base_type, .. } => {
            match kind {
                MacroGeneratedKind::Vec => TypeDefKind::Struct {
                    fields: indexmap! {
                        "ptr" => FieldDef { ty: format!("*const {}", base_type), .. },
                        "len" => FieldDef { ty: "usize", .. },
                        "cap" => FieldDef { ty: "usize", .. },
                        "destructor" => FieldDef { ty: format!("{}Destructor", base_type), .. },
                    },
                    has_repr_c: true,
                    ..
                },
                MacroGeneratedKind::VecDestructor => TypeDefKind::Enum {
                    variants: indexmap! {
                        "DefaultRust" => VariantDef { ty: None, .. },
                        "NoDestructor" => VariantDef { ty: None, .. },
                        "External" => VariantDef { ty: Some("...DestructorType"), .. },
                    },
                    has_repr_c: true,
                    ..
                },
                // ... weitere Fälle
            }
        }
        other => other.clone()
    }
}
```

### Schritt 2: `workspace.rs` anpassen

`convert_type_info_to_class_patch()` so ändern, dass es `TypeDefinition` aus `TypeIndex` verwendet.

### Schritt 3: Alte WorkspaceIndex-Nutzung entfernen

Stellen identifizieren, die `WorkspaceIndex` verwenden, und auf `TypeIndex` umstellen.

---

## 6. Betroffene Dateien

- `doc/src/autofix/type_index.rs` - Expansion hinzufügen
- `doc/src/autofix/workspace.rs` - TypeIndex statt WorkspaceIndex verwenden
- `doc/src/patch/index.rs` - Eventuell deprecaten oder entfernen
- `doc/src/autofix/mod.rs` - Imports anpassen

---

## 7. Erwartete Ergebnisse

Nach der Implementierung:
- Alle ~150+ fehlenden Typen bekommen korrekte `struct_fields`/`enum_fields`
- `memtest` kompiliert erfolgreich
- Einheitliches Type-Indexing-System
- Weniger Code-Duplizierung

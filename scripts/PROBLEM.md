# Zusammenfassung: Codegen v2 Implementierung

## Was wir gemacht haben

Wir haben ein neues **konfigurationsgesteuertes Code-Generierungssystem (v2)** für das Azul GUI Framework implementiert. Das Ziel war, die bestehende komplexe Codegen-Logik zu vereinfachen und wartbarer zu machen.

### Erstellte Dateien

```
doc/src/codegen/v2/
├── mod.rs           - Modul-Exporte
├── config.rs        - Konfigurationsstrukturen für verschiedene Ausgaben
├── ir.rs            - Intermediate Representation (IR) Definitionen
├── ir_builder.rs    - Baut IR aus api.json
├── generator.rs     - CodeBuilder und LanguageGenerator Trait
├── lang_rust.rs     - Rust DLL Generator (funktioniert!)
├── lang_c.rs        - C Header Generator (Stub)
├── lang_cpp.rs      - C++ Header Generator (Stub)
└── lang_python.rs   - Python Extension Generator (Stub)
```

## Wo wir Fehler gemacht haben (Lessons Learned)

### 1. Referenzen vs. Pointer für `self`-Parameter

**Fehler:** Wir haben `"self": "ref"` als `*const T` (Pointer) interpretiert.

**Korrekt:** Die alte Codegen verwendet `&T` (Referenz):
```rust
// FALSCH:
"ref" => ArgRefKind::Ptr      // *const ClassName

// RICHTIG:
"ref" => ArgRefKind::Ref      // &ClassName
"refmut" => ArgRefKind::RefMut // &mut ClassName
```

Die alte rust_dll.rs (Zeilen 580-586) zeigt das klar:
```rust
if self_val == "ref" {
    fn_args.push_str(&format!("{}: &{}, ", class_name.to_lowercase(), class_ptr_name));
} else if self_val == "refmut" {
    fn_args.push_str(&format!("{}: &mut {}, ", class_name.to_lowercase(), class_ptr_name));
}
```

### 2. Konstruktoren haben impliziten Rückgabetyp

**Fehler:** Wenn `returns` in api.json fehlt, haben wir `return_type = None` gesetzt.

**Korrekt:** Konstruktoren (aus `class_data.constructors`) geben IMMER den Klassennamen zurück:
```rust
// rust_dll.rs Zeile 335:
let mut returns = class_ptr_name.clone(); // DEFAULT für Konstruktoren
if let Some(return_info) = &constructor.returns {
    // Nur überschreiben wenn explizit angegeben
}
```

### 3. FieldRefKind fehlte Varianten

**Fehler:** Nur 5 Varianten: `Owned, Ref, RefMut, Ptr, PtrMut`

**Korrekt:** Braucht 7 Varianten wie die originale `RefKind`:
```rust
pub enum FieldRefKind {
    Owned,       // T
    Ref,         // &T
    RefMut,      // &mut T
    Ptr,         // *const T
    PtrMut,      // *mut T
    Boxed,       // Box<T>
    OptionBoxed, // Option<Box<T>>
}
```

### 4. String ist KEIN primitiver Typ

**Fehler:** `"String"` war in `is_primitive_type()` enthalten.

**Korrekt:** In Azul ist `String` = `AzString`, ein komplexer Typ mit Vektoren, nicht `std::string::String`.

### 5. Type Alias `ref_kind` für Pointer

Type Aliases wie `AzX11Visual = c_void` mit `ref_kind: "constptr"` müssen zu `*const c_void` werden, nicht nur `c_void`.

## Wie das neue IR-System funktioniert

### Architektur

```
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐     ┌──────────────┐
│  api.json   │ --> │ IR Builder  │ --> │   CodegenIR      │ --> │  Generator   │
│             │     │             │     │ (Structs, Enums, │     │ (Rust/C/Py)  │
│             │     │             │     │  Functions, etc) │     │              │
└─────────────┘     └─────────────┘     └──────────────────┘     └──────────────┘
                                               │
                                               ▼
                                        ┌──────────────┐
                                        │CodegenConfig │
                                        │ - type_prefix│
                                        │ - trait_impl │
                                        │ - etc.       │
                                        └──────────────┘
```

### IR Definitionen (ir.rs)

```rust
pub struct CodegenIR {
    pub type_aliases: Vec<TypeAliasDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub functions: Vec<FunctionDef>,
    pub callbacks: Vec<CallbackDef>,
    pub type_to_external: HashMap<String, String>,  // AzDom -> azul_core::dom::Dom
}

pub struct StructDef {
    pub name: String,
    pub doc: Vec<String>,
    pub fields: Vec<FieldDef>,
    pub external_path: Option<String>,
    pub generic_params: Vec<String>,  // ["T", "U"]
    pub traits: TypeTraits,
    pub repr: Option<String>,
}

pub struct FunctionDef {
    pub c_name: String,           // AzDom_new
    pub class_name: String,       // Dom
    pub method_name: String,      // new
    pub kind: FunctionKind,       // Constructor, Method, MethodMut, StaticMethod, Delete, etc.
    pub args: Vec<FunctionArg>,
    pub return_type: Option<String>,
    pub fn_body: Option<String>,  // Aus api.json
}

pub enum FunctionKind {
    Constructor,    // Aus class_data.constructors
    StaticMethod,   // Kein self-Argument
    Method,         // &self
    MethodMut,      // &mut self
    Delete,         // Generiert für Drop
    DeepCopy,       // Generiert für Clone
    PartialEq,      // Generiert für PartialEq
    PartialCmp,     // Generiert für PartialOrd
    Cmp,            // Generiert für Ord
    Hash,           // Generiert für Hash
}
```

### Config (config.rs)

```rust
pub struct CodegenConfig {
    pub type_prefix: String,                    // "Az"
    pub external_crate_replacement: Option<(String, String)>,  // ("azul_dll::", "crate::")
    pub trait_impl_mode: TraitImplMode,         // UsingTransmute, UsingCApi, Derive
    pub generate_trait_functions: bool,
}

pub enum TraitImplMode {
    UsingTransmute,  // DLL: impl Clone { transmute(...).clone() }
    UsingCApi,       // azul.rs: impl Clone { unsafe { AzType_deepCopy(self) } }
    Derive,          // Just #[derive(Clone)]
}
```

### Warum v2 besser ist

1. **Konfigurationsgesteuert**: Eine IR, mehrere Ausgaben durch verschiedene Configs
2. **Trennung von Concerns**: IR-Building getrennt von Code-Generierung
3. **Wartbar**: Änderungen an Typ-Behandlung nur an einer Stelle
4. **Erweiterbar**: Neue Sprachen durch Implementierung von `LanguageGenerator`
5. **Testbar**: IR kann unabhängig von Generatoren getestet werden

## Python Extension Implementierung (TODO)

### Unterschiede zu DLL

1. **Andere Attribute**: `#[pyclass]` statt `#[repr(C)]`
2. **Typ-Filterung**: Rekursive Typen und VecRef-Typen müssen übersprungen werden
3. **Callback-Trampolines**: Python-Callables → Rust extern "C" fn
4. **RefAny-Handling**: Python PyObject → RefAny Wrapper
5. **Unsendable**: Alle Typen brauchen `#[pyclass(unsendable)]`

### Skip-Typen

```rust
const RECURSIVE_TYPES: &[&str] = &[
    "XmlNode", "XmlNodeChild", "XmlNodeChildVec", "Xml", "ResultXmlXmlError",
];

const VECREF_TYPES: &[&str] = &[
    "GLuintVecRef", "GLintVecRef", "U8VecRef", "Refstr", // ... etc
];
```

### Callback-Trampoline Muster

Für jeden Callback-Typ (z.B. `LayoutCallbackType`):

```rust
/// Wrapper für Python-Daten in RefAny
#[repr(C)]
pub struct AppDataTy {
    pub _py_app_data: Option<Py<PyAny>>,
    pub _py_layout_callback: Option<Py<PyAny>>,
}

/// Trampoline: extern "C" fn die Python aufruft
extern "C" fn invoke_py_layout_callback(
    app_data: azul_core::refany::RefAny,
    info: azul_core::callbacks::LayoutCallbackInfo
) -> azul_core::styled_dom::StyledDom {
    Python::with_gil(|py| {
        // Extrahiere Python-Objekte aus RefAny
        // Rufe Python-Callback auf
        // Konvertiere Ergebnis zurück zu Rust
    })
}
```

### Python-Struct-Generation

```rust
#[pyclass(unsendable)]  // Weil Typen transitiv Pointer enthalten
pub struct Dom {
    pub(crate) inner: __dll_api_inner::dll::AzDom,
}

#[pymethods]
impl Dom {
    #[new]
    fn new(node_type: NodeType) -> Self {
        Self {
            inner: __dll_api_inner::dll::AzDom::new(node_type.inner)
        }
    }
    
    // &mut self Methoden werden ÜBERSPRUNGEN wegen unsendable → frozen
}
```

### Implementierungsschritte

1. **IR erweitern** in ir.rs:
   - `is_recursive_type: bool` auf StructDef/EnumDef
   - `is_vecref_type: bool`
   - `is_callback_typedef: bool`
   - `callback_signature: Option<CallbackSignature>` für Callback-Typen

2. **IR-Builder erweitern** in ir_builder.rs:
   - Erkennung von Callback-Typen (haben `callback_typedef` Feld)
   - Erkennung von Callback+Data Pair Structs (haben CallbackType + RefAny Felder)
   - Markierung von rekursiven/VecRef Typen

3. **PythonGenerator implementieren** in lang_python.rs:
   - `generate_python_types()`: #[pyclass] Structs mit inner-Feld
   - `generate_pymethods()`: Methoden die inner konvertieren
   - `generate_callback_trampolines()`: extern "C" Trampolines
   - `generate_helper_functions()`: AzString ↔ String Konvertierung
   - `generate_module_init()`: #[pymodule] mit allen Typen

4. **PythonConfig nutzen**:
   - `skip_types`: Rekursive und VecRef Typen
   - `callback_types`: Typen die Trampolines brauchen
   - `base.trait_impl_mode = UsingTransmute`: Clone/Drop via transmute

### Wichtige Hinweise für Python

1. **Keine &mut self Methoden**: `unsendable` impliziert `frozen` in PyO3 0.27+
2. **Transmute für Konvertierung**: Python-Wrapper ↔ C-API Typ via transmute
3. **GIL-Safety**: Alle Python-Aufrufe in `Python::with_gil()`
4. **RefAny**: Wrapper-Struct mit `Py<PyAny>` für Python-Objekte

---

Implement the v2 version of azul-doc codegen for the "python-extension" and then test that it builds.
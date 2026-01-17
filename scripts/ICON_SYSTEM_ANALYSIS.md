# Azul Icon System - Vollständige Analyse & Implementierungsplan

## 1. Aktueller Stand

### 1.1 Architektur-Übersicht

Das Icon-System besteht aus zwei Hauptteilen:

1. **Core (`core/src/icon.rs`)**: Infrastruktur
   - `IconProviderHandle`: Arc<Mutex<BTreeMap<String, BTreeMap<String, RefAny>>>> - Nested map pack_name → (icon_name → RefAny)
   - `IconResolverCallbackType`: extern "C" Callback-Typ
   - `resolve_icons_in_styled_dom()`: Hauptfunktion für Icon-Ersetzung
   - **Kein IconPack/FontPack Struct** - Differenzierung via RefAny::downcast

2. **Layout (`layout/src/icon.rs`)**: Default-Implementierungen
   - `default_icon_resolver()`: Standard-Resolver
   - `ImageIconData`, `FontIconData`: Marker-Structs für RefAny-Downcasting

### 1.2 Neuer Datenfluss

```
AppConfig.icons                                   User registriert Icons in AppConfig
       │
       ▼
create_icon_provider_from_config()                Konvertiert AppIconConfig → IconProviderHandle
       │
       ▼
IconProviderHandle.icons                          BTreeMap<pack_name, BTreeMap<icon_name, RefAny>>
       │
       ▼
resolve_icons_in_styled_dom()                     Vor Layout: alle NodeType::Icon ersetzen
       │
       ├──► provider.lookup(icon_name)            Lookup über alle Packs (first match)
       │         │
       │         ▼
       │    resolver_callback(data?, original, style)   Callback aufrufen
       │         │
       │         ▼
       └──► StyledDom replacement                 Ersetzt Icon-Node mit neuem Inhalt
```

### 1.3 Probleme mit dem aktuellen Design

#### Problem 1: Font-Pack mit "__font_pack__" Magic Key

Der Resolver bekommt bei `lookup("home")` nur `None`, weil "home" nicht im Pack ist - nur "__font_pack__".

#### Problem 2: Resolver-Signatur hat redundante Parameter

`icon_name`, `default_size`, `a11y_label` sind alle bereits im `original_icon_dom` enthalten.

#### Problem 3: Kein Default Material Icons Font eingebaut

Es gibt keinen automatischen Material Icons Registration beim App-Start.

---

## 2. Gewünschtes Design

### 2.1 Kernkonzept

> "We just get the icon passed and the StyledDom of the <icon> node (which already contains the a11y text, etc.). Then we just return the modified / processed StyledDom based on the system style + input."

**Minimale Resolver-Signatur:**
```rust
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: Option<RefAny>,      // Die registrierten Daten (ImageIconData, FontIconData, etc.)
    original_icon_dom: &StyledDom,  // Das Original-Icon-Node mit inline styles + a11y + icon_name
    system_style: &SystemStyle,
) -> StyledDom;
```

**Wichtig:**
- `icon_name` ist nicht nötig - bereits im `original_icon_dom` enthalten (NodeType::Icon(name))
- `default_size` ist nicht nötig - CSS Resolver bestimmt die Größe
- `a11y_label` ist nicht nötig - bereits im `original_icon_dom` enthalten
- Bei `icon_data == None` → früh returnen mit leerem Div (nicht original klonen)

### 2.2 Style-Mapping mit SystemStyle-Awareness

Styles werden **selektiv** kopiert, basierend auf SystemStyle:

```rust
fn copy_appropriate_styles(
    original: &NodeData,
    system_style: &SystemStyle,
) -> CssPropertyWithConditionsVec {
    let mut result = Vec::new();
    
    for prop in original.get_css_props().iter() {
        // Skip color if user prefers grayscale
        if system_style.prefers_reduced_color {
            match prop.inner_property() {
                CssProperty::TextColor(_) | CssProperty::BackgroundColor(_) => continue,
                _ => {}
            }
        }
        
        result.push(prop.clone());
    }
    
    result.into()
}
```

### 2.3 RefAny-Downcasting-System

**Für Image Icons:**
```rust
pub struct ImageIconData {
    pub image: ImageRef,
    pub width: f32,   // Dupliziert aus ImageRef beim Registrieren
    pub height: f32,  // Dupliziert aus ImageRef beim Registrieren
}
```

**Für Font Icons:**
```rust
pub struct FontIconData {
    pub font: FontRef,
    pub icon_char: String,  // Der Char für dieses spezifische Icon
}
```

### 2.4 Material Icons mit `material-icons` Crate

Die `material-icons` Crate (https://docs.rs/material-icons/0.2.0/) bietet:

- `material_icons::FONT` - Embedded TTF Font-Bytes (Apache 2.0 lizenziert)
- `material_icons::Icon` - Enum mit allen ~2000 Icon-Namen
- `material_icons::icon_to_char(Icon::Home)` → `'\u{e88a}'` - Codepoint-Mapping

---

## 3. API-Funktionen (für api.json)

### 3.1 Icon Registration API

Die API ist generisch und erfordert keine neuen Structs:

```c
// Register a single image icon
void AzIconProviderHandle_registerImageIcon(
    AzIconProviderHandle* provider,
    AzString pack_name,      // "app-icons", "my-pack", etc.
    AzString icon_name,      // "logo", "favicon", etc.
    AzImageRef image         // Das Bild
);

// Register icons from a ZIP file (file names become icon names)
void AzIconProviderHandle_registerIconsFromZip(
    AzIconProviderHandle* provider,
    AzString pack_name,      // "zip-icons"
    AzU8Vec zip_bytes        // ZIP file content
);

// Register a font icon
void AzIconProviderHandle_registerFontIcon(
    AzIconProviderHandle* provider,
    AzString pack_name,      // "material-icons"
    AzString icon_name,      // "home"
    AzFontRef font,          // Font reference
    AzString icon_char       // "\ue88a" oder "home" (ligature)
);

// Unregister an icon
void AzIconProviderHandle_unregisterIcon(
    AzIconProviderHandle* provider,
    AzString pack_name,
    AzString icon_name
);

// Unregister an entire icon pack
void AzIconProviderHandle_unregisterPack(
    AzIconProviderHandle* provider,
    AzString pack_name
);

// Check if icon exists
bool AzIconProviderHandle_hasIcon(
    AzIconProviderHandle* provider,
    AzString icon_name
);
```

### 3.2 Rust-Implementierung

```rust
impl IconProviderHandle {
    pub fn register_image_icon(&self, pack_name: &str, icon_name: &str, image: ImageRef) {
        let data = ImageIconData { image };
        self.register_icon(pack_name, icon_name, RefAny::new(data));
    }
    
    pub fn register_icons_from_zip(&self, pack_name: &str, zip_bytes: &[u8]) {
        if let Some(pack) = load_icon_pack_from_zip(pack_name, zip_bytes) {
            self.add_pack(pack);
        }
    }
    
    pub fn register_font_icon(&self, pack_name: &str, icon_name: &str, font: FontRef, icon_char: &str) {
        let data = FontIconData { font, icon_char: icon_char.to_string() };
        self.register_icon(pack_name, icon_name, RefAny::new(data));
    }
}
```

---

## 4. ZIP-Resolution für Image Packs

### 4.1 Implementierung

```rust
use std::path::Path;

/// Lädt alle Bilder aus einem ZIP und gibt (icon_name, ImageRef, width, height) zurück
fn load_images_from_zip(zip_bytes: &[u8]) -> Vec<(String, ImageRef, f32, f32)> {
    use crate::zip::{ZipFile, ZipReadConfig};
    
    let mut result = Vec::new();
    let config = ZipReadConfig::default();
    let entries = match ZipFile::list(zip_bytes, &config) {
        Ok(e) => e,
        Err(_) => return result,
    };
    
    for entry in entries.iter() {
        if entry.path.ends_with('/') { continue; } // Skip directories
        
        let file_bytes = match ZipFile::get_single_file(zip_bytes, entry, &config) {
            Ok(Some(b)) => b,
            _ => continue,
        };
        
        // Decode as image
        if let Ok(raw_image) = decode_raw_image_from_any_bytes(&file_bytes) {
            // Icon name = filename without extension (using std::path::Path)
            let path = Path::new(&entry.path);
            let icon_name = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            
            let width = raw_image.width as f32;
            let height = raw_image.height as f32;
            
            if let Some(image) = ImageRef::new_rawimage(raw_image) {
                result.push((icon_name, image, width, height));
            }
        }
    }
    
    result
}
```

**Beispiel ZIP-Struktur:**
```
icons.zip/
  ├── home.png      → register_icon("home", ImageIconData{...})
  ├── settings.png  → register_icon("settings", ImageIconData{...})
  └── logo.ico      → register_icon("logo", ImageIconData{...})
```

---

## 5. Vollständiger Implementierungsplan

### Phase 1: Core-Änderungen (`core/src/icon.rs`)

#### 5.1.1 Resolver-Signatur vereinfachen

**Vorher:**
```rust
pub type IconResolverCallbackType = extern "C" fn(
    icon_name: &AzString,
    icon_data: Option<RefAny>,
    default_size: f32,
    a11y_label: &AzString,
    system_style: &SystemStyle,
) -> StyledDom;
```

**Nachher:**
```rust
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: Option<RefAny>,
    original_icon_dom: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom;
```

#### 5.1.2 `IconPack` Struct komplett entfernen

**Vorher:**
```rust
pub struct IconPack {
    pub name: String,
    pub icons: BTreeMap<String, RefAny>,
    pub default_size: f32,
}
```

**Nachher:** Kein IconPack Struct mehr. Stattdessen nested map in IconProviderHandle:
```rust
pub struct IconProviderInner {
    /// Nested map: pack_name → (icon_name → RefAny)
    pub icons: BTreeMap<String, BTreeMap<String, RefAny>>,
    pub resolver: IconResolverCallbackType,
}
```

Die Differenzierung zwischen Image/Font/SVG/Lottie erfolgt via `RefAny::downcast`.
Ganze Packs können einfach via `icons.remove(pack_name)` entfernt werden.

#### 5.1.3 `resolve_icons_in_styled_dom()` anpassen

- Extrahiere das StyledDom des einzelnen Icon-Nodes
- Bei `lookup == None` → leeres Div zurückgeben

### Phase 2: Layout-Änderungen (`layout/src/icon.rs`)

#### 5.2.1 `ImageIconData` behält Größe (dupliziert)

```rust
pub struct ImageIconData {
    pub image: ImageRef,
    pub width: f32,   // Dupliziert aus ImageRef beim Registrieren
    pub height: f32,  // Dupliziert aus ImageRef beim Registrieren
}
```

Die Größe wird beim Registrieren aus dem `ImageRef` extrahiert und gespeichert.

#### 5.2.2 `FontIconData` mit icon_char

```rust
pub struct FontIconData {
    pub font: FontRef,
    pub icon_char: String,
}
```

#### 5.2.3 `create_font_icon_pack()` entfernen

Die Funktion mit "__font_pack__" magic key wird entfernt.

#### 5.2.4 `default_icon_resolver()` neu

```rust
pub extern "C" fn default_icon_resolver(
    icon_data: Option<RefAny>,
    original_dom: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    // No icon found → empty div
    let Some(mut data) = icon_data else {
        return StyledDom::from(Dom::div());
    };
    
    // Try ImageIconData
    if let Some(img) = data.downcast_ref::<ImageIconData>() {
        return create_image_icon_from_original(img, original_dom, system_style);
    }
    
    // Try FontIconData
    if let Some(font_icon) = data.downcast_ref::<FontIconData>() {
        return create_font_icon_from_original(font_icon, original_dom, system_style);
    }
    
    // Unknown data type → empty div
    StyledDom::from(Dom::div())
}

fn create_image_icon_from_original(
    img: &ImageIconData,
    original: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    let mut dom = Dom::create_image(img.image.clone());
    
    // Copy appropriate styles from original (respecting SystemStyle)
    if let Some(original_node) = original.node_data.as_ref().first() {
        let filtered_styles = copy_appropriate_styles(original_node, system_style);
        dom.root.set_css_props(filtered_styles);
        
        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(*a11y.clone());
        }
    }
    
    StyledDom::create(&mut dom, Css::empty())
}

fn create_font_icon_from_original(
    font_icon: &FontIconData,
    original: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    let mut dom = Dom::create_text(&font_icon.icon_char);
    
    // Add font family
    let font_prop = CssPropertyWithConditions::simple(
        CssProperty::font_family(StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::Ref(font_icon.font.clone())
        ]))
    );
    
    if let Some(original_node) = original.node_data.as_ref().first() {
        let mut props = copy_appropriate_styles(original_node, system_style);
        props.as_mut().push(font_prop);
        dom.root.set_css_props(props);
        
        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(*a11y.clone());
        }
    }
    
    StyledDom::create(&mut dom, Css::empty())
}

/// Copy styles from original, filtering based on SystemStyle preferences
fn copy_appropriate_styles(
    original_node: &NodeData,
    system_style: &SystemStyle,
) -> CssPropertyWithConditionsVec {
    let original_props = original_node.get_css_props();
    let mut result = Vec::new();
    
    for prop in original_props.as_ref().iter() {
        // Skip color properties if user prefers reduced color/grayscale
        if system_style.prefers_reduced_color {
            match prop.inner_property() {
                CssProperty::TextColor(_) | 
                CssProperty::BackgroundColor(_) |
                CssProperty::BorderTopColor(_) |
                CssProperty::BorderBottomColor(_) |
                CssProperty::BorderLeftColor(_) |
                CssProperty::BorderRightColor(_) => continue,
                _ => {}
            }
        }
        
        result.push(prop.clone());
    }
    
    CssPropertyWithConditionsVec::from_vec(result)
}
```

#### 5.2.5 Neue API-Methoden für IconProviderHandle

```rust
impl IconProviderHandle {
    pub fn register_image_icon(&self, pack_name: &str, icon_name: &str, image: ImageRef) {
        // Größe wird beim Registrieren aus ImageRef extrahiert
        let (width, height) = image.get_dimensions();
        let data = ImageIconData { image, width, height };
        self.register_icon(pack_name, icon_name, RefAny::new(data));
    }
    
    pub fn register_icons_from_zip(&self, pack_name: &str, zip_bytes: &[u8]) {
        // Iteriert über ZIP entries, registriert jedes Bild einzeln
        for (icon_name, image, width, height) in load_images_from_zip(zip_bytes) {
            let data = ImageIconData { image, width, height };
            self.register_icon(pack_name, &icon_name, RefAny::new(data));
        }
    }
    
    pub fn register_font_icon(&self, pack_name: &str, icon_name: &str, font: FontRef, icon_char: &str) {
        let data = FontIconData { font, icon_char: icon_char.to_string() };
        self.register_icon(pack_name, icon_name, RefAny::new(data));
    }
}
```

#### 5.2.6 `register_material_icons()` - Registriert direkt in Provider

```rust
#[cfg(feature = "icons")]
pub fn register_material_icons(provider: &IconProviderHandle) {
    use material_icons::{Icon, icon_to_char, FONT};
    
    // Parse the embedded font
    let Some(font_ref) = FontRef::parse(FONT) else { return; };
    
    // Register initial set of icons (3 for C example, expand later)
    let icons = [
        ("home", Icon::Home),
        ("settings", Icon::Settings),
        ("search", Icon::Search),
    ];
    
    for (name, icon) in icons {
        let icon_char = icon_to_char(icon).to_string();
        let data = FontIconData {
            font: font_ref.clone(),
            icon_char,
        };
        provider.register_icon("material-icons", name, RefAny::new(data));
    }
}
```

### Phase 3: Cargo.toml aktualisieren

```toml
# In layout/Cargo.toml
[dependencies]
material-icons = { version = "0.2", optional = true }

[features]
icons = ["std", "zip_support", "dep:material-icons"]

# In dll/Cargo.toml - "icons" wird Teil von "build-dll"
[features]
build-dll = ["icons", ...]  # icons als Teil von build-dll
```

### Phase 4: api.json aktualisieren

API-Funktionen werden via `azul-doc autofix add` / `azul-doc autofix remove` hinzugefügt (weniger fehleranfällig als manuelles Editieren).

Beispiel:

```json
{
  "IconProviderHandle": {
    "functions": {
      "register_image_icon": {
        "fn_args": [
          {"self": "refmut"},
          {"pack_name": "String"},
          {"icon_name": "String"},
          {"image": "ImageRef"}
        ],
        "fn_body": "object.register_image_icon(pack_name.as_str(), icon_name.as_str(), image)"
      },
      "register_icons_from_zip": {
        "fn_args": [
          {"self": "refmut"},
          {"pack_name": "String"},
          {"zip_bytes": "U8Vec"}
        ],
        "fn_body": "object.register_icons_from_zip(pack_name.as_str(), zip_bytes.as_ref())"
      },
      "register_font_icon": {
        "fn_args": [
          {"self": "refmut"},
          {"pack_name": "String"},
          {"icon_name": "String"},
          {"font": "FontRef"},
          {"icon_char": "String"}
        ],
        "fn_body": "object.register_font_icon(pack_name.as_str(), icon_name.as_str(), font, icon_char.as_str())"
      },
      "has_icon": {
        "fn_args": [
          {"self": "ref"},
          {"icon_name": "String"}
        ],
        "returns": {"type": "bool"},
        "fn_body": "object.has_icon(icon_name.as_str())"
      },
      "unregister_icon": {
        "fn_args": [
          {"self": "refmut"},
          {"pack_name": "String"},
          {"icon_name": "String"}
        ],
        "fn_body": "object.unregister_icon(pack_name.as_str(), icon_name.as_str())"
      },
      "unregister_pack": {
        "fn_args": [
          {"self": "refmut"},
          {"pack_name": "String"}
        ],
        "fn_body": "object.unregister_pack(pack_name.as_str())"
      }
    }
  }
}
```

---

## 6. Dateien die geändert werden

| Datei | Änderung |
|-------|----------|
| `core/src/icon.rs` | Resolver-Signatur (nur 3 Params), IconPack Struct entfernen → flat map |
| `layout/src/icon.rs` | ImageIconData mit duplizierter Größe, FontIconData mit icon_char, neue API-Methoden, material icons |
| `layout/Cargo.toml` | `material-icons` Dependency hinzufügen |
| `api.json` | Resolver-Signatur, neue Funktionen (keine neuen Structs!) |

---

## 7. Erwartetes Ergebnis

Nach der Implementierung:

```c
// C-Beispiel
AzAppConfig config = AzAppConfig_create();
AzIconProviderHandle* provider = &config.icon_provider;  // Direkt als Feld auf AppConfig

// Einzelnes Image-Icon registrieren
AzIconProviderHandle_registerImageIcon(provider, 
    az_str("app"), az_str("favicon"), my_image);

// Icons aus ZIP laden
AzIconProviderHandle_registerIconsFromZip(&provider,
    az_str("zip-pack"), zip_bytes);

// Font-Icon registrieren
AzIconProviderHandle_registerFontIcon(&provider,
    az_str("custom-font"), az_str("star"), my_font, az_str("\u2605"));

// Icons verwenden (Material Icons sind bereits registriert)
AzDom icon1 = AzDom_createIcon(az_str("favicon"));   // → ImageIconData → Bild
AzDom icon2 = AzDom_createIcon(az_str("home"));      // → FontIconData → Material Icon
AzDom icon3 = AzDom_createIcon(az_str("unknown"));   // → None → leeres Div
```

Alle Icons werden korrekt aufgelöst:
- Styles vom Original-DOM werden kopiert (gefiltert nach SystemStyle)
- Größe kommt aus CSS, nicht aus "default_size"
- Unbekannte Icons → leeres Div (kein Crash)

---

## 8. Testing

### 8.1 Testprozess

Nach der Implementierung wird der Test wie folgt durchgeführt:

1. **DLL und Headers neu generieren:**
   ```bash
   cargo build -p azul-dll --features "build-dll"
   # "icons" Feature ist Teil von "build-dll"
   # Headers werden automatisch generiert
   ```

2. **C-Beispiel kompilieren und starten:**
   ```bash
   cd examples/c
   make icons
   AZUL_DEBUG=8765 ./icons &
   ```

3. **DOM-Struktur via Debug API prüfen:**
   ```bash
   # DOM-Tree abrufen (siehe DEBUG_API.md)
   curl -X POST http://localhost:8765/ -d '{"op": "get_dom_tree"}'
   
   # Oder Display-List für detaillierte Rendering-Info
   curl -X POST http://localhost:8765/ -d '{"op": "get_display_list"}'
   ```

### 8.2 Erwartetes Ergebnis

**Vorher (ohne Icon-Resolution):**
```json
{
  "node_type": "Icon",
  "icon_name": "home",
  ...
}
```

**Nachher (mit Icon-Resolution):**
```json
{
  "node_type": "Text",
  "text": "\ue88a",
  "font_family": "Material Icons",
  ...
}
```

Oder für Image-Icons:
```json
{
  "node_type": "Image",
  "image_id": "favicon",
  ...
}
```

### 8.3 Testkriterien

- ✅ Kein `NodeType::Icon` mehr im finalen DOM-Tree
- ✅ Registrierte Icons werden zu `Text` (Font) oder `Image` Nodes
- ✅ Nicht-registrierte Icons werden zu leeren `Div` Nodes
- ✅ Styles vom Original-Icon werden übernommen (width, height, etc.)
- ✅ Accessibility-Info wird korrekt kopiert

//! Module mapping for api.json
//!
//! This module defines the mapping from type names to api.json modules.
//! The mapping is based on semantic grouping rather than source file location.

use std::collections::BTreeMap;

/// Canonical module names in api.json
pub const MODULES: &[&str] = &[
    "app",
    "component",
    "window",
    "callbacks",
    "dom",
    "menu",
    "css",
    "widgets",
    "gl",
    "image",
    "font",
    "svg",
    "xml",
    "dialog",
    "time",
    "task",
    "str",
    "vec",
    "option",
    "error",
    "http",
    "zip",
    "fluent",
    "icu",
];

/// Keywords that map to specific modules
/// If a type name contains any of these keywords (case-insensitive), it goes to that module
pub fn get_module_keywords() -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut map = BTreeMap::new();

    // Vec module - all vector types and their destructors
    map.insert(
        "vec",
        vec!["vecdestructor", "vecdestructortype", "vecref", "vecrefmut"],
    );

    // Option module - all option types
    map.insert("option", vec!["option"]);

    // Error/Result module
    map.insert("error", vec!["error", "result"]);

    // CSS module - styling properties
    map.insert(
        "css",
        vec![
            "pixel",
            "style",
            "layout",
            "color",
            "border",
            "margin",
            "padding",
            "font",
            "background",
            "gradient",
            "shadow",
            "transform",
            "animation",
            "flex",
            "grid",
            "align",
            "justify",
            "overflow",
            "position",
            "display",
            "visibility",
            "opacity",
            "filter",
            "blend",
            "cursor",
            "scrollbar",
            "size",
            "width",
            "height",
            "top",
            "bottom",
            "left",
            "right",
            "radius",
            "spacing",
            "gap",
            "wrap",
            "direction",
            "content",
            "text",
            "letter",
            "word",
            "line",
            "white",
            "vertical",
            "horizontal",
            "inset",
            "outline",
            "decoration",
            "indent",
            "hyphens",
            "hanging",
            "break",
            "orphans",
            "widows",
            "column",
            "counter",
            "list",
            "caption",
            "empty",
            "table",
            "quote",
            "shape",
            "clip",
            "mask",
            "perspective",
            "backface",
            "writing",
            "unicode",
            "initial",
            "normalize",
            "angle",
            "percentage",
            "float",
            "clear",
            "zindex",
            "srgb",
            "rgb",
            "hsl",
            "hsv",
            "cascade",    // CascadeInfo
            "extendmode", // ExtendMode
            "flow",       // FlowInto, FlowFrom, FlowIntoValue, FlowFromValue
            "arithmetic", // ArithmeticCoefficients
        ],
    );

    // Window module
    map.insert(
        "window",
        vec![
            "window",
            "monitor",
            "videomode",
            "hwaccel",
            "vsync",
            "dpi",
            "hidpi",
            "fullscreen",
            "maximize",
            "minimize",
            "decorat",
            "theme",
            "icon",
            "cursor",
            "attention",
            "ime",
            "platform",
            "handle",
            "wayland",
            "x11",
            "xcb",
            "xlib",
            "macos",
            "ios",
            "android",
            "windows",
            "web",
        ],
    );

    // DOM module
    map.insert(
        "dom",
        vec![
            "dom",
            "node",
            "attribute",
            "accessibility",
            "tabindex",
            "focus",
            "hover",
            "event",
            "callback",
            "inline",
            "tag",
            "touchstate",
            "mousestate",
            "keyboardstate",
            "debugstate",
            "hittest",    // HitTest, HitTestItem, etc.
            "virtualkey", // VirtualKeyCode
            "scancode",   // ScanCode
            "keycode",    // KeyCode
            "drag",       // DragData, DragState, DragEffect
            "drop",       // DropEffect
            "clipboard",  // ClipboardContent
            "selection",  // Selection, SelectionManager, SelectionState
            "gesture",    // GestureAndDragManager
            "input",      // InputSample, InputSession
            "bidi",       // BidiDirection, BidiLevel
            "idorclass",  // IdOrClass
            "aria",       // SmallAriaInfo
        ],
    );

    // Callbacks module
    map.insert(
        "callbacks",
        vec![
            "callbackinfo",
            "callbackreturn",
            "callbacktype",
            "marshaled",
            "virtualizedviewcallback",
            "timercallback",
            "threadcallback",
            "rendercallback",
            "writebackcallback",
            "layoutcallback",
            "refany", // RefAny, RefCount
            "refcount",
            "update",      // Update enum
            "edgetype",    // EdgeType
            "grapheme",    // GraphemeClusterId
            "scrollstate", // ScrollState
            "pentilt",     // PenTilt
            "penstate",    // PenState
            "changeset",   // ChangesetId
            "undoable",    // UndoableOperation
        ],
    );

    // GL module
    map.insert(
        "gl",
        vec![
            "gl",
            "opengl",
            "glcontext",
            "texture",
            "shader",
            "vertex",
            "buffer",
            "uniform",
            "attrib",
            "program",
            "framebuffer",
            "renderbuffer",
            "sync",
            "debugmessage", // DebugMessage
        ],
    );

    // SVG module
    map.insert(
        "svg",
        vec![
            "svg",
            "svgnode",
            "svgpath",
            "svgcircle",
            "svgrect",
            "svgline",
            "path",
            "circle",
            "rect",
            "line",
            "polygon",
            "curve",
            "stroke",
            "fill",
            "tessellat",
        ],
    );

    // Component module - component system types
    map.insert(
        "component",
        vec![
            "componentid",
            "componentdatafield",
            "componentdef",
            "componentlibrary",
            "componentmap",
            "componentsource",
            "compiletarget",
        ],
    );

    // XML module
    map.insert("xml", vec!["xml", "xhtml", "parse", "stream"]);

    // Image module
    map.insert(
        "image",
        vec![
            "image", "rawimage", "decode", "encode", "jpeg", "png", "gif", "bmp",
        ],
    );

    // Font module
    map.insert(
        "font",
        vec![
            "fontref",
            "fontmetric",
            "parsedfont",
            "loadedfont",
            "glyph",
            "panose",
        ],
    );

    // Menu module
    map.insert("menu", vec!["menu", "menuitem", "menupopup", "contextmenu"]);

    // Dialog module
    map.insert(
        "dialog",
        vec!["dialog", "msgbox", "filepicker", "colorpicker"],
    );

    // Time module
    map.insert(
        "time",
        vec!["instant", "duration", "systemtime", "systemtick"],
    );

    // Task module
    map.insert(
        "task",
        vec![
            "thread", // Thread, ThreadId, ThreadInner, ThreadSender, etc.
            "threadsend",
            "threadreceive",
            "threadwrite",
            "taskcallback",
            "sender",
            "receiver",
            "channel",
            "timer", // Timer types
        ],
    );

    // App module
    map.insert(
        "app",
        vec!["appconfig", "apploglevel", "apptermination", "renderer"],
    );

    // Str module
    map.insert("str", vec!["string", "refstr", "azstring"]);

    // HTTP module - network requests
    map.insert(
        "http",
        vec![
            "http",
            "httpresponse",
            "httprequest",
            "httpconfig",
            "download",
            "urlreachable",
        ],
    );

    // ZIP module - archive handling
    map.insert(
        "zip",
        vec![
            "zip",
            "zipentry",
            "ziparchive",
            "zipextract",
            "zipcreate",
        ],
    );

    // Fluent module - localization
    map.insert(
        "fluent",
        vec![
            "fluent",
            "locale",
            "localizer",
            "translate",
            "langpack",
            "languagepack",
        ],
    );

    // ICU module - internationalization
    map.insert(
        "icu",
        vec![
            "icu",
            "datetime",
            "dateformat",
            "numberformat",
            "plural",
            "listformat",
        ],
    );

    // Widgets module - UI components
    map.insert(
        "widgets",
        vec![
            "button",
            "checkbox",
            "textinput",
            "numberinput",
            "colorinput",
            "fileinput",
            "dropdown",
            "listview",
            "treeview",
            "progressbar",
            "slider",
            "scrollbar",
            "tab",
            "ribbon",
            "label",
            "frame",
            "nodegraph",
        ],
    );

    map
}

/// Paths to exclude from the workspace index (tests, examples, etc.)
pub fn should_exclude_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();

    // Exclude test directories
    if path_str.contains("/tests/") || path_str.contains("/test/") {
        return true;
    }

    // Exclude example directories
    if path_str.contains("/examples/") || path_str.contains("/example/") {
        return true;
    }

    // Exclude benchmark directories
    if path_str.contains("/benches/") || path_str.contains("/bench/") {
        return true;
    }

    // Exclude build scripts
    if path_str.ends_with("build.rs") {
        return true;
    }

    false
}

/// Determine the correct api.json module for a type based on its name
///
/// Priority:
/// 1. OptionFoo -> "option" (MUST come first to handle OptionFooVec correctly)
/// 2. FooVec, FooVecDestructor, FooVecDestructorType -> "vec"
/// 3. FooError, ResultFoo -> "error"
/// 4. Find all matching keywords across all modules, pick the longest match On tie, pick the first
///    module in MODULES order
/// 5. "misc" (with warning)
pub fn determine_module(type_name: &str) -> (String, bool) {
    let lower_name = type_name.to_lowercase();

    // Priority 1: Option types (MUST come before Vec check to handle OptionFooVec correctly)
    // e.g., OptionStringVec is an Option<StringVec>, not a Vec type
    if lower_name.starts_with("option") {
        return ("option".to_string(), false);
    }

    // Priority 2: Vec types go to vec module
    if lower_name.ends_with("vec")
        || lower_name.ends_with("vecdestructor")
        || lower_name.ends_with("vecdestructortype")
        || lower_name.ends_with("vecref")
        || lower_name.ends_with("vecrefmut")
    {
        return ("vec".to_string(), false);
    }

    // Priority 3: Error/Result types
    if lower_name.ends_with("error") || lower_name.starts_with("result") {
        return ("error".to_string(), false);
    }

    // Priority 4: Find longest matching keyword across all modules
    // Collect all matches: (module_name, matched_keyword, keyword_length, module_order)
    let mut matches: Vec<(&str, &str, usize, usize)> = Vec::new();

    // First check module names themselves as keywords
    for (order, module) in MODULES.iter().enumerate() {
        if *module != "vec" && *module != "option" && *module != "error" {
            if lower_name.contains(module) {
                matches.push((module, module, module.len(), order));
            }
        }
    }

    // Then check all keywords
    let keywords = get_module_keywords();
    for module in MODULES.iter() {
        if let Some(module_keywords) = keywords.get(module) {
            let order = MODULES
                .iter()
                .position(|m| m == module)
                .unwrap_or(usize::MAX);
            for keyword in module_keywords {
                if lower_name.contains(keyword) {
                    matches.push((module, keyword, keyword.len(), order));
                }
            }
        }
    }

    if matches.is_empty() {
        // Priority 5: Misc (with warning)
        return ("misc".to_string(), true);
    }

    // Sort by: longest keyword first, then by module order (first in MODULES wins)
    matches.sort_by(|a, b| {
        // Compare by length (descending)
        match b.2.cmp(&a.2) {
            std::cmp::Ordering::Equal => {
                // On tie, compare by module order (ascending)
                a.3.cmp(&b.3)
            }
            other => other,
        }
    });

    (matches[0].0.to_string(), false)
}

/// Check if a type is in the correct module and return the correct module if not
/// Uses determine_module to figure out where the type should be based on its name
/// If the type is already in the correct module, returns None
/// If the type should be moved, returns Some(target_module)
pub fn get_correct_module(type_name: &str, current_module: &str) -> Option<String> {
    let (correct_module, _) = determine_module(type_name);
    if correct_module != current_module {
        Some(correct_module)
    } else {
        None
    }
}

/// Types that should be excluded from the C API entirely
/// These contain non-FFI-safe types like BTreeMap, HashMap, Arc, VecDeque
pub const INTERNAL_ONLY_TYPES: &[&str] = &[
    // Manager types with BTreeMap/HashMap internals
    "ScrollManager",
    "HoverManager",
    "GpuStateManager",
    "GpuValueCache",
    "SelectionManager",
    "VirtualizedViewManager",
    "FocusManager",
    "GestureAndDragManager",
    "DragDropManager",
    "FileDropManager",
    "UndoRedoManager",
    "TextInputManager",
    // Types with BTreeMap fields
    "HitTest",
    "FullHitTest",
    "DragData",
    // Cache types with HashMap/Arc
    "LayoutCache",
    "TextLayoutCache",
    // Types with Arc<T> fields
    "ShapedGlyph",
    "ShapedCluster",
    "LogicalItem",
    // Types with VecDeque
    "NodeUndoRedoStack",
];

/// Check if a type should be internal-only (not exported to C API)
pub fn is_internal_only_type(type_name: &str) -> bool {
    INTERNAL_ONLY_TYPES.contains(&type_name)
}

/// FFI difficulty score for a type field
/// Higher score = more difficult to port to C
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FfiDifficulty {
    /// Primitive types, repr(C) structs - trivial
    Easy = 0,
    /// Option<T> where T is Easy - needs wrapper
    Medium = 1,
    /// Vec<T>, String - needs destructor
    Hard = 2,
    /// BTreeMap, HashMap, Arc, VecDeque - not directly portable
    VeryHard = 3,
    /// Generic types, trait objects - requires redesign
    Impossible = 4,
}

/// Analyze a type string and return its FFI difficulty
pub fn analyze_ffi_difficulty(type_str: &str) -> FfiDifficulty {
    // Check for impossible patterns first
    if type_str.contains("dyn ") || type_str.contains("impl ") {
        return FfiDifficulty::Impossible;
    }

    // Very hard patterns - requires redesign
    if type_str.contains("BTreeMap")
        || type_str.contains("HashMap")
        || type_str.contains("Arc<")
        || type_str.contains("Rc<")
        || type_str.contains("VecDeque")
        || type_str.contains("Box<dyn")
        || type_str.contains("Mutex")
        || type_str.contains("RwLock")
    {
        return FfiDifficulty::VeryHard;
    }

    // Note: Vec<T> and String are NOT flagged as difficult anymore
    // because the api.json system already has wrappers for these (StringVec, OptionString, etc.)
    // Only truly non-FFI-safe types are flagged

    // Medium patterns - Option types need wrapper but are generally fine
    if type_str.starts_with("Option<") {
        // Check if the inner type is problematic
        if type_str.contains("BTreeMap")
            || type_str.contains("HashMap")
            || type_str.contains("Arc<")
        {
            return FfiDifficulty::VeryHard;
        }
    }

    FfiDifficulty::Easy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_types() {
        assert_eq!(determine_module("StringVec").0, "vec");
        assert_eq!(determine_module("DomVecDestructor").0, "vec");
        assert_eq!(determine_module("NodeDataVecDestructorType").0, "vec");
        assert_eq!(determine_module("U8VecRef").0, "vec");
    }

    #[test]
    fn test_option_types() {
        assert_eq!(determine_module("OptionCallback").0, "option");
        assert_eq!(determine_module("OptionWindowState").0, "option");
    }

    #[test]
    fn test_error_types() {
        assert_eq!(determine_module("XmlParseError").0, "error");
        assert_eq!(determine_module("DecodeImageError").0, "error");
        assert_eq!(determine_module("ResultU8VecEncodeImageError").0, "error");
    }

    #[test]
    fn test_module_name_matching() {
        assert_eq!(determine_module("CssProperty").0, "css");
        assert_eq!(determine_module("WindowFlags").0, "window");
        assert_eq!(determine_module("DomNodeId").0, "dom");
        assert_eq!(determine_module("SvgPath").0, "svg");
        assert_eq!(determine_module("GlContext").0, "gl");
    }

    #[test]
    fn test_keyword_matching() {
        assert_eq!(determine_module("PixelValue").0, "css");
        assert_eq!(determine_module("BorderRadius").0, "css");
        assert_eq!(determine_module("LayoutWidth").0, "css");
        assert_eq!(determine_module("ThreadSendMsg").0, "task");
        assert_eq!(determine_module("TextureFlags").0, "gl");
    }

    #[test]
    fn test_longest_match_wins() {
        // "contextmenu" (11 chars) beats "menu" (4 chars)
        assert_eq!(determine_module("ContextMenuMouseButton").0, "menu");
        // "callbackinfo" (12 chars) beats "callback" (8 chars)
        assert_eq!(determine_module("TimerCallbackInfo").0, "callbacks");
        // "svg" module name should win for SvgNode
        assert_eq!(determine_module("SvgNode").0, "svg");
    }

    #[test]
    fn test_misc_fallback() {
        let (module, is_warning) = determine_module("CompletelyUnknownType");
        assert_eq!(module, "misc");
        assert!(is_warning);
    }

    #[test]
    fn test_get_correct_module() {
        // RefAny has no matching keywords, should go to misc
        assert_eq!(
            get_correct_module("RefAny", "refany"),
            Some("misc".to_string())
        );
        // CascadeInfo has no matching keywords, should go to misc
        assert_eq!(
            get_correct_module("CascadeInfo", "style"),
            Some("misc".to_string())
        );
        // SvgStrokeStyle contains "svg" and "style", svg is a module name so should go to svg
        assert_eq!(
            get_correct_module("SvgStrokeStyle", "style"),
            Some("svg".to_string())
        );
        // Already in correct module
        assert_eq!(get_correct_module("CssProperty", "css"), None);
    }

    #[test]
    fn test_exclude_paths() {
        use std::path::Path;
        assert!(should_exclude_path(Path::new("/foo/tests/some_test.rs")));
        assert!(should_exclude_path(Path::new("/foo/examples/demo.rs")));
        assert!(should_exclude_path(Path::new("/foo/build.rs")));
        assert!(!should_exclude_path(Path::new("/foo/src/lib.rs")));
    }
}

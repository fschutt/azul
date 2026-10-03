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
    // The app shells (`azul_layout::widgets::shells`): the eleven window
    // layouts and the pieces they share (OfficeShell, ShellNavigationPane,
    // ShellCommandPalette, ...). Their own module, so a binding reads
    // `from azul.shells import ShellNavigationPane`; the smaller items stay
    // in `widgets`. Every class name carries "Shell", and the source path
    // routes them too (`module_from_external_path`).
    "shells",
    "gl",
    "image",
    "font",
    "svg",
    "xml",
    "json",
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
    // Capability / platform subsystems — pulled out of the old "misc" catch-all.
    // Each also has an exact source-path arm in `module_from_external_path` so
    // irregular names (DirEntry, Wt*, Detected*) still route correctly.
    "audio",
    "video",
    "screen",
    "camera",
    "biometric",
    "sensor",
    "gamepad",
    "gesture",
    "tray",
    "notification",
    "webtransport",
    "iroh",
    "db",
    "file",
    "fmt",
    "pdf",
    "url",
    "uuid",
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
            "cascade",     // CascadeInfo
            "extendmode",  // ExtendMode
            "flow",        // FlowInto, FlowFrom, FlowIntoValue, FlowFromValue
            "arithmetic",  // ArithmeticCoefficients
            "visualbox",   // VisualBox (overflow-clip-margin)
            "boxorstatic", // BoxOrStatic, BoxOrStaticString, BoxOrStaticImageRef
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
            "hittest",     // HitTest, HitTestItem, etc.
            "virtualkey",  // VirtualKeyCode
            "scancode",    // ScanCode
            "keycode",     // KeyCode
            "drag",        // DragData, DragState, DragEffect
            "drop",        // DropEffect
            "clipboard",   // ClipboardContent
            "selection",   // Selection, SelectionManager, SelectionState
            "gesture",     // GestureAndDragManager
            "input",       // InputSample, InputSession
            "bidi",        // BidiDirection, BidiLevel
            "idorclass",   // IdOrClass
            "aria",        // SmallAriaInfo
            "geolocation", // GeolocationProbeConfig (backs NodeType::GeolocationProbe)
            "locationfix", // LocationFix (delivered by the geolocation backends)
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
        vec!["zip", "zipentry", "ziparchive", "zipextract", "zipcreate"],
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
    // System tray. Every keyword is the FULL `tray`-prefixed stem rather than
    // just "tray", because matching picks the LONGEST keyword and several of
    // these would otherwise lose to a shorter-but-unrelated module:
    // TrayIconImage/TrayIconData match "icon" (image), TrayScrollAxis matches
    // "scroll" (widgets). Spelling the stems out makes tray win by length
    // instead of relying on MODULES order to break a tie.
    map.insert(
        "tray",
        vec![
            "tray",
            "trayicon",   // TrayIconData, TrayIconImage, TrayIconSource
            "trayevent",  // TrayEvent, TrayEventType
            "trayscroll", // TrayScrollAxis — beats "scroll"
            "traycategory",
            "traystatus",
        ],
    );
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
            "maptile",     // MapTileId, MapTileLayer
            "mapviewport", // MapViewport
            "mapwidget",   // MapWidget
        ],
    );

    // App shells. Every class name carries "shell" (or "scaffold"), and the
    // longer stems are spelled out so a shell type whose name also holds a
    // dom or css word ("ShellNavigationPaneEvent" has "event",
    // "ShellThemeAccentColors" has "color") still resolves here by length.
    map.insert(
        "shells",
        vec![
            "shell",
            "scaffold",
            "shellpane",           // ShellPane, ShellPaneKind
            "shellon",             // ShellOnPaneFocus, ShellOnPaneResize
            "shellnavigation",     // ShellNavigationPane, ShellNavigationGroup, ShellNavigationModule
            "shellcommandpalette", // ShellCommandPalette
            "shellpalette",        // ShellPaletteCommand
            "shellsettings",       // ShellSettingsLayout, ShellSettingsSection
            "shellemptystate",     // ShellEmptyState
            "shelltheme",          // ShellThemeScope, ShellThemeAccent, ShellThemeAccentColors
            "shellbottomtab",      // ShellBottomTab
            "officeshell",
            "documentshell",
            "canvasshell",
            "timelineshell",
            "pimshell",
            "browsershell",
            "recordsshell",
            "mediashell",
            "developershell",
            "utilityshell",
            "callshell",
            "mobileshell",
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
/// 4. Known-difficult names (`DIFFICULT_TYPE_MODULES`) — manual overrides for collisions the
///    keyword heuristic gets wrong
/// 5. Find all matching keywords across all modules, pick the longest match On tie, pick the first
///    module in MODULES order
/// 6. "misc" (with warning)
///    Known-difficult type names, matched BEFORE any keyword heuristic.
///
/// The keyword matcher is a substring search ranked by match length. That
/// works for the overwhelming majority of names and is deliberately kept, but
/// it has no notion of word boundaries, so a short keyword can win inside a
/// longer unrelated word. `TabletPadState` went to `css` because the css
/// keyword "table" is inside "TABLEt-PadState".
///
/// That failure is not self-correcting. A CONFIDENT keyword match that agrees
/// with the type's current module short-circuits the external-path check in
/// [`get_correct_module_with_path`], so the type is then reported as CORRECTLY
/// PLACED and can never move — `autofix modules` said everything was fine while
/// the type sat in the wrong module.
///
/// This table exists so a collision like that is fixed by naming the ONE
/// difficult case, rather than by tuning a keyword (which changes ranking for
/// every other type) or by hard-coding a module for every class. Entries are
/// PREFIX matches, so one line covers a family.
///
/// A whole-word matcher was tried as the general fix and rejected: it cannot
/// recover acronyms no camel splitter can split (`GLfloat` -> "g", "lfloat")
/// and it proposed several actively wrong moves that substring ranking gets
/// right (`FontMetrics` -> css, `SvgParseOptions` -> xml). Registering a class
/// is rare and the API moves little, so naming the exceptions is cheaper and
/// far easier to audit than a cleverer matcher.
///
/// Structural types (`OptionFoo`, `FooVec`, `FooError`) are resolved BEFORE
/// this table, so an entry here never steals `OptionTabletPadState` from
/// `option`.
const DIFFICULT_TYPE_MODULES: &[(&str, &str)] = &[
    // The pagination family behind `CallbackInfo::query_pagination`
    // (9g-ii-f-i): paged-media page setup, headers / footers, margin-box
    // content, break policy and the break positions that come back. The
    // keyword pass scattered it over css ("margin", "break", "counter") and
    // misc; it belongs with `Pdf`, the print path these types were written
    // for (printpdf flips the break flags on). Spelled in full where a prefix
    // would reach further: "Page" alone would capture PageInfo-unrelated
    // names; "PageSetup" deliberately covers PageSetupOverride.
    ("BreakKind", "pdf"),
    ("BreakPolicy", "pdf"),
    ("CounterFormat", "pdf"),
    ("FakePageConfig", "pdf"),
    ("HeaderFooterConfig", "pdf"),
    ("MarginBoxContent", "pdf"),
    ("MarginBoxCallback", "pdf"),
    ("MarginBoxCustom", "pdf"),
    ("PageBreakPosition", "pdf"),
    ("PageInfo", "pdf"),
    ("PageMargins", "pdf"),
    ("PageSequence", "pdf"),
    ("PageSetup", "pdf"),
    ("PaginationInfo", "pdf"),
    // `AppConfig::natural_scroll` (9b-ii-b-i-a) belongs beside AppConfig, not
    // in `image` where the keyword pass filed it.
    ("NaturalScroll", "app"),
    // The system-style tween durations sit beside `SystemStyle` and
    // `ScrollPhysics` in css; by path (`azul_core::resources::`) the tool
    // would file it under `image`, and no word of the name is a keyword.
    ("SystemAnimations", "css"),
    // One family, one module: the Svg* geometry/style/options types were
    // scattered over css, gl, option and svg by whichever word of the name
    // won (`SvgParseOptions` -> option, `SvgFillStyle` -> css). `SvgParseError`
    // stays in error (structural names are settled before this table).
    ("Svg", "svg"),
    // A recolor IS a color mapping: these two sit beside `IconColorMapping`
    // in css, not with the icon provider handles in window.
    ("IconModeColors", "css"),
    ("IconRecolor", "css"),
    // System-wide hotkeys are grabbed for the APP (one App-owned manager,
    // whatever window declares them), so they sit beside `App` and
    // `AppConfig`. Without the entry "GLobalHotkey" contains the
    // OpenGL module's own name and every one of them was filed under `gl`.
    // Spelled "GlobalHotkey", NOT "Global" - fifth word-boundary trap (see
    // Tablet/Table, Dial/Dialog, Hid/Hidpi, Media/MediaType below); nothing
    // else in the API starts with it today. `GlobalHotkeyError` and the
    // Result/Option wrappers are routed by the structural rules first.
    ("GlobalHotkey", "app"),
    ("HotkeyModifiers", "app"),
    // "Tablet*" collides with the css keyword "table".
    ("Tablet", "gesture"),
    // "Haptic*" has no keyword in any module, so it fell through to "misc".
    // Haptics is the OUTPUT half of the input stack - every platform exposes
    // it through the same subsystem that reports pens and dials - so it
    // belongs with the other input types rather than in the junk drawer.
    ("Haptic", "gesture"),
    // A dial (Surface Dial, Apple Watch crown) is an input device, not
    // miscellany. Spelled in FULL rather than as a "Dial" prefix, because
    // that prefix also captures "Dialog*" and dragged `DialogAriaInfo` out
    // of the dialog module - the same word-boundary trap as Tablet/Table
    // above, caught by `autofix modules` on the very first run.
    ("DialState", "gesture"),
    // Generic HID landed in "misc". `core/src/hid.rs` frames itself as the
    // escape hatch FOR the gamepad path - flight sticks and wheels that are
    // not Xbox-shaped, the same split SDL draws between joystick and gamepad
    // events - so that is where someone looking for controller input looks.
    // Spelled in full, NOT as a "Hid" prefix: that also matches
    // `HidpiAdjustedBounds` (HiDPI), which has nothing to do with input
    // devices. Same word-boundary trap as Tablet/Table and Dial/Dialog.
    ("HidDevice", "gamepad"),
    ("HidReport", "gamepad"),
    // The system media session - what the desktop's media widget shows. No
    // keyword matched, so both landed in "misc"; "audio" is where the audio
    // sink and the rest of the playback surface already live, and it is where
    // someone wiring up a music player looks.
    // Spelled in FULL, not as a "Media" prefix: that also matches `MediaType`,
    // which is the CSS `@media` type and belongs to css. FOURTH instance of
    // this trap after Tablet/Table, Dial/Dialog and Hid/Hidpi - a prefix table
    // is a word-boundary bug generator, so every new entry gets checked
    // against the existing type names before it is added.
    ("MediaPlaybackState", "audio"),
    ("NowPlayingInfo", "audio"),
    // A raw pointer/pen sample. "Input" matched nothing and it fell into
    // "dom", which is actively misleading - it is not a node type. `PenState`
    // and `PenTilt`, which it now carries, are both in "callbacks", and a
    // sample is read from a callback and nowhere else.
    ("InputSample", "callbacks"),
    // Form validity. No keyword matched, so both landed in "misc"; they are
    // read from a callback (`get_validity_state`) and nowhere else, which is
    // where `PenState`, `PenTilt` and `InputSample` above already live.
    ("ValidityState", "callbacks"),
    ("ValidityReason", "callbacks"),
    // "event" is a dom keyword, so the transport events sorted into dom next to the DOM events.
    ("Wt", "webtransport"),
    ("Iroh", "iroh"),
    // The CPU rasterizer's text style (`RawImage::from_text` / `draw_text`,
    // `CallbackInfo::text_image`): "Style" filed it under css; it belongs
    // beside `RawImage` in image (MEDIA6). Spelled in full - "Text" alone
    // would capture every text type.
    ("TextRasterStyle", "image"),
    // The voice echo canceller (VIDEO8): no keyword matched, so it landed in
    // "misc" and AzMeet's `azul::audio::EchoCanceller` did not resolve; it
    // belongs beside `AudioEncoder` / `AudioDecoder` in audio. In full - "Echo"
    // alone is too broad.
    ("EchoCanceller", "audio"),
];

/// Module for a known-difficult type name, if it is one.
fn difficult_type_module(type_name: &str) -> Option<&'static str> {
    DIFFICULT_TYPE_MODULES
        .iter()
        .find(|(prefix, _)| type_name.starts_with(prefix))
        .map(|(_, module)| *module)
}

/// Byte offsets where a CamelCase (or snake_case) word starts in `name`:
/// `AccordionVariant` -> {0, 9}, `CSSProperty` -> {0, 3}, `node_id` -> {0, 5}.
fn word_starts(name: &str) -> Vec<usize> {
    let b = name.as_bytes();
    let mut starts = Vec::new();
    for i in 0..b.len() {
        let c = b[i];
        if c == b'_' {
            continue;
        }
        let prev = if i == 0 { None } else { Some(b[i - 1]) };
        let next = b.get(i + 1).copied();
        let is_start = match prev {
            None => true,
            Some(b'_') => true,
            Some(p) => {
                // lower->Upper (`nI`), digit boundaries, or the last capital of
                // an acronym run (`SSP` in `CSSProperty`: `P` precedes a lower)
                (c.is_ascii_uppercase() && !p.is_ascii_uppercase())
                    || (c.is_ascii_uppercase()
                        && p.is_ascii_uppercase()
                        && next.is_some_and(|n| n.is_ascii_lowercase()))
            }
        };
        if is_start {
            starts.push(i);
        }
    }
    starts
}

/// `keyword` occurs in `type_name` as whole words: it starts on a word
/// boundary and ends on one. `aria` is INSIDE `AccordionVariant` but is not
/// a word of it; `node` is a word of `NodeId`, `tabindex` of `TabIndex`.
fn keyword_is_whole_word(type_name: &str, keyword: &str) -> bool {
    let lower = type_name.to_lowercase();
    let starts = word_starts(type_name);
    let mut from = 0;
    while let Some(off) = lower[from..].find(keyword) {
        let i = from + off;
        let end = i + keyword.len();
        let ends_on_boundary =
            end == lower.len() || starts.contains(&end) || lower.as_bytes()[end] == b'_';
        if starts.contains(&i) && ends_on_boundary {
            return true;
        }
        from = i + 1;
    }
    false
}

/// THE "is a Vec type" rule: the generated `*Vec` and its destructor / ref
/// helpers, all of which live in the `vec` module. One function for the
/// name-only classifier (`determine_module`), the widget rule
/// (`widget_module_for`) and the move check (`get_correct_module_with_path`):
/// three hand-written copies of this list drifted apart (only one counted
/// `vecslice`) and misfiled 14 widget slices. A `*VecSlice` is not one of
/// them - it is a borrowed view of its element and lives with the element.
/// `Option*Vec` is an Option: every caller tests the `Option` prefix first.
pub fn is_vec_family(type_name: &str) -> bool {
    let lower = type_name.to_lowercase();
    ["vec", "vecdestructor", "vecdestructortype", "vecref", "vecrefmut"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

/// (module, is_guess). `is_guess` is true when no keyword matched (`misc`)
/// AND when the winning keyword is only a substring of the name, not one of
/// its words - `aria` inside `AccordionVariant` chose `dom` for a widget
/// enum (2026-10-01). A guess still names a module; the caller that has
/// the source path lets the path decide instead.
pub fn determine_module(type_name: &str) -> (String, bool) {
    let lower_name = type_name.to_lowercase();

    // Priority 1: Option types (MUST come before Vec check to handle OptionFooVec correctly)
    // e.g., OptionStringVec is an Option<StringVec>, not a Vec type
    if lower_name.starts_with("option") {
        return ("option".to_string(), false);
    }

    // Priority 2: Vec types go to vec module
    if is_vec_family(type_name) {
        return ("vec".to_string(), false);
    }

    // Priority 3: Error/Result types
    if lower_name.ends_with("error") || lower_name.starts_with("result") {
        return ("error".to_string(), false);
    }

    // Priority 4: known-difficult names, matched manually BEFORE the
    // heuristic. Only collisions the keyword matcher gets wrong live here —
    // everything else is still classified automatically below.
    if let Some(module) = difficult_type_module(type_name) {
        return (module.to_string(), false);
    }

    // Priority 5: Find longest matching keyword across all modules
    // Collect all matches:
    // (module_name, matched_keyword, keyword_length, module_order, is_module_name)
    let mut matches: Vec<(&str, &str, usize, usize, bool)> = Vec::new();

    // First check module names themselves as keywords
    for (order, module) in MODULES.iter().enumerate() {
        if *module != "vec" && *module != "option" && *module != "error"
            && lower_name.contains(module) {
                matches.push((module, module, module.len(), order, true));
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
                    matches.push((module, keyword, keyword.len(), order, false));
                }
            }
        }
    }

    if matches.is_empty() {
        // Priority 6: Misc (with warning)
        return ("misc".to_string(), true);
    }

    // Sort by: longest keyword first; on equal length a MODULE-NAME match
    // outranks a generic keyword (a type containing a module's own name is
    // stronger evidence than a shared word — "FilePath" contains the module
    // name "file" AND svg's generic keyword "path", both length 4: `file`
    // must win); remaining ties fall to module order (first in MODULES wins).
    // A keyword that is a WORD of the name outranks every substring match,
    // whatever their lengths: `component` (a word of `ComponentDefaultValue`)
    // beats the longer `defaultvalue`-style accidents, `font` in `FontMetrics`
    // beats `metrics`. Among equals the old order holds.
    let whole = |m: &(&str, &str, usize, usize, bool)| keyword_is_whole_word(type_name, m.1);
    matches.sort_by(|a, b| match whole(b).cmp(&whole(a)) {
        std::cmp::Ordering::Equal => match b.2.cmp(&a.2) {
            std::cmp::Ordering::Equal => match b.4.cmp(&a.4) {
                std::cmp::Ordering::Equal => a.3.cmp(&b.3),
                other => other,
            },
            other => other,
        },
        other => other,
    });

    let (module, keyword, ..) = matches[0];
    (module.to_string(), !keyword_is_whole_word(type_name, keyword))
}

/// THE module of a type whose source is a widget (`azul_layout::widgets::`),
/// used by every path that places or checks a widget type (the add command,
/// the scan's additions, the scan's move check) - two rules used to disagree,
/// so the scan moved callback wrappers the add had put in `dom` out to
/// `shells` and the next modify landed in an empty stub (2026-10-01).
/// Widget types live in `widgets` (the app shells in `shells`) EXCEPT the
/// by-concern types, which match the established placement: `*CallbackType`
/// -> callbacks, `*Callback` -> dom, `Option*` -> option, the `*Vec` family
/// -> vec (a `*VecSlice` stays with its widget). `None` for a non-widget
/// path: the caller falls back to `determine_module`.
pub fn widget_module_for(type_name: &str, full_path: &str) -> Option<String> {
    if !full_path.starts_with("azul_layout::widgets::") {
        return None;
    }
    let module = if type_name.ends_with("CallbackType") {
        "callbacks"
    } else if type_name.ends_with("Callback") {
        "dom"
    } else if type_name.starts_with("Option") {
        "option"
    } else if is_vec_family(type_name) {
        "vec"
    } else if full_path.starts_with("azul_layout::widgets::shells::") {
        // The app shells (OfficeShell, ShellNavigationPane, the S1..S11
        // shells) have a module of their own, apart from the smaller
        // widgets: `from azul.shells import ShellNavigationPane`.
        "shells"
    } else {
        "widgets"
    };
    Some(module.to_string())
}

/// Whether the name alone settles the module: `Option*`, the `*Vec` family,
/// `*Error`, `Result*` (the scan never lets a path or a table move these).
fn is_structural(type_name: &str) -> bool {
    let lower = type_name.to_lowercase();
    lower.starts_with("option")
        || is_vec_family(type_name)
        || lower.ends_with("error")
        || lower.starts_with("result")
}

/// THE module of a type api.json does not have yet, `(module, is_guess)`,
/// for every path that adds one (`autofix add`, its dependency types, the
/// scan's additions, an Add patch without a module): the placement the
/// scan's move check ([`get_correct_module_with_path`]) keeps, decided in
/// its order - a structural name, the exceptions table, the widget rule, a
/// confident keyword, then the module of the source path, else the
/// keyword's guess (`misc`). The add used a rule of its own (widget rule,
/// else keywords), so the next scan moved a keyword-less name out of
/// `misc` and a widget's `*Error` out of `widgets`.
pub fn new_type_module(type_name: &str, full_path: &str) -> (String, bool) {
    let (by_name, is_guess) = determine_module(type_name);
    if is_structural(type_name) && !is_guess {
        return (by_name, false);
    }
    if let Some(forced) = difficult_type_module(type_name) {
        return (forced.to_string(), false);
    }
    if let Some(module) = widget_module_for(type_name, full_path) {
        return (module, false);
    }
    if !is_guess {
        return (by_name, false);
    }
    match module_from_external_path(full_path) {
        Some(module) => (module, false),
        None => (by_name, true),
    }
}

/// Check if a type is in the correct module and return the correct module if not.
/// Uses the external path (if available) as the primary signal, falling back to
/// keyword-based `determine_module` if no external path is provided.
/// If the type is already in the correct module, returns None.
/// If the type should be moved, returns Some(target_module).
pub fn get_correct_module(type_name: &str, current_module: &str) -> Option<String> {
    get_correct_module_with_path(type_name, current_module, None)
}

/// Like `get_correct_module` but accepts an optional external path for better accuracy.
pub fn get_correct_module_with_path(
    type_name: &str,
    current_module: &str,
    external_path: Option<&str>,
) -> Option<String> {
    let (name_module, is_warning) = determine_module(type_name);

    // Hard-coded module assignments (Vec/Option/Error) always win — they're
    // structural. A `*VecSlice` is NOT structural: it lives with its element
    // (a widget's slice with its widget, below). Counting it here answered
    // with the name's keyword before the widget rule was asked, so
    // `CellGridRangeVecSlice` was "correct" in css (DEDUP_WIDGETS_API F17).
    if is_structural(type_name) && !is_warning {
        if name_module != current_module {
            return Some(name_module);
        } else {
            return None; // structural type is in correct module
        }
    }

    // The manual override table wins over the external-path short-circuit
    // below - that is what the table is FOR. It used to be consulted only
    // through `determine_module`, AFTER that short-circuit, so a type whose
    // source module already mapped to its current api.json module could
    // never be moved by naming it: `NaturalScroll` (in `azul_core::resources`,
    // which maps to `image`) sat in `image` with a table entry saying `app`.
    // Structural types are settled above, so a prefix entry never steals
    // `HidDeviceVec` from `vec`.
    if let Some(forced) = difficult_type_module(type_name) {
        return (forced != current_module).then(|| forced.to_string());
    }

    // A widget type is placed by the one widget rule, never by keywords.
    if let Some(module) = external_path.and_then(|p| widget_module_for(type_name, p)) {
        return (module != current_module).then_some(module);
    }

    // If the external path CONFIRMS the current module, the type is correctly
    // placed — don't let a coincidental name keyword pull it out. This protects
    // e.g. `VideoWidget` / `CameraWidget` / `GamepadButton` (azul_layout::widgets::,
    // already in `widgets`) from being dragged into `video` / `camera` / `gamepad`
    // by their names. We only RETURN here on a confirming match; a non-matching
    // path is NOT authoritative (many modules — e.g. `component` — are organized
    // by concern, not by source path), so we fall through to keyword matching.
    if let Some(path) = external_path {
        if let Some(path_module) = module_from_external_path(path) {
            if path_module == current_module {
                return None;
            }
        }
    }

    // For non-structural types: if keyword matching is confident, trust it
    if !is_warning {
        if name_module != current_module {
            return Some(name_module);
        } else {
            return None;
        }
    }

    // Keyword matching fell through to "misc" — use external path as tiebreaker
    if let Some(path) = external_path {
        if let Some(path_module) = module_from_external_path(path) {
            if path_module != current_module {
                return Some(path_module);
            } else {
                return None;
            }
        }
    }

    // Both keyword and path failed — suggest misc
    if name_module != current_module {
        return Some(name_module);
    }

    None
}

/// Derive the api.json module name from a Rust external path like
/// "azul_css::css::BoxOrStaticString".
fn module_from_external_path(path: &str) -> Option<String> {
    // azul_css::* → css (all CSS types)
    if path.starts_with("azul_css::") {
        return Some("css".to_string());
    }
    // azul_core submodules
    if path.starts_with("azul_core::dom::") {
        return Some("dom".to_string());
    }
    if path.starts_with("azul_core::window::") {
        return Some("window".to_string());
    }
    if path.starts_with("azul_core::callbacks::") {
        return Some("callbacks".to_string());
    }
    // The callback wrappers that need `CallbackInfo` live one crate up
    // (`azul_layout::callbacks::{Callback, ResumeCallback, ...}`); by name
    // alone `ResumeCallback` matches dom's generic `callback` keyword.
    if path.starts_with("azul_layout::callbacks::") {
        return Some("callbacks".to_string());
    }
    // `RequestId` (the resumable-API handle) has no keyword of its own and
    // would otherwise fall to `misc`; every `azul_core::task::` type is a
    // timer / thread / request primitive of the `task` module.
    if path.starts_with("azul_core::task::") {
        return Some("task".to_string());
    }
    if path.starts_with("azul_core::a11y::") {
        return Some("dom".to_string());
    }
    if path.starts_with("azul_core::resources::") {
        return Some("image".to_string());
    }
    if path.starts_with("azul_core::styled_dom::") {
        return Some("dom".to_string());
    }
    if path.starts_with("azul_core::diff::") {
        return Some("dom".to_string());
    }
    if path.starts_with("azul_core::events::") {
        return Some("dom".to_string());
    }
    // Geolocation POD types (LocationFix, GeolocationProbeConfig) back the
    // `NodeType::GeolocationProbe` dom node, so they belong in the dom module.
    if path.starts_with("azul_core::geolocation::") {
        return Some("dom".to_string());
    }
    // azul_layout submodules
    if path.starts_with("azul_layout::icu::") {
        return Some("icu".to_string());
    }
    if path.starts_with("azul_layout::xml::") {
        return Some("dom".to_string());
    }
    // The app shells have a module of their own (`shells`), apart from the
    // smaller widgets: checked BEFORE the widgets arm, which would otherwise
    // claim the path.
    if path.starts_with("azul_layout::widgets::shells::") {
        return Some("shells".to_string());
    }
    // Widget types (Button, TextInput, MapWidget, …) live in the `widgets`
    // module regardless of their Rust submodule (e.g. `widgets::map::MapWidget`).
    // Without this arm, types in a nested widget submodule fell through to the
    // `misc` fallback, producing spurious "move to misc" patches.
    if path.starts_with("azul_layout::widgets::") {
        return Some("widgets".to_string());
    }

    // Capability / platform subsystems — formerly dumped in "misc". Routed by
    // their exact source path so irregular type names (DirEntry, Wt*, Detected*,
    // OkCancel) land in the right module without risky name-keyword matching.
    // azul_core::*
    if path.starts_with("azul_core::json::") {
        return Some("json".to_string());
    }
    if path.starts_with("azul_core::audio::") {
        return Some("audio".to_string());
    }
    if path.starts_with("azul_core::video::") {
        return Some("video".to_string());
    }
    if path.starts_with("azul_core::screencap::") {
        return Some("screen".to_string());
    }
    if path.starts_with("azul_core::camera::") {
        return Some("camera".to_string());
    }
    if path.starts_with("azul_core::biometric::") {
        return Some("biometric".to_string());
    }
    if path.starts_with("azul_core::sensors::") {
        return Some("sensor".to_string());
    }
    if path.starts_with("azul_core::gamepad::") {
        return Some("gamepad".to_string());
    }
    if path.starts_with("azul_core::db::") {
        return Some("db".to_string());
    }
    // Routed by path rather than by name: a `Tray*` name-keyword arm would also
    // capture unrelated future types, and the module is small enough that the
    // exact path is both safer and self-documenting.
    if path.starts_with("azul_core::tray::") {
        return Some("tray".to_string());
    }
    // Native notifications: the tray's sibling, routed the same way - by path,
    // so no name keyword of another module claims one of its types.
    if path.starts_with("azul_core::notification::") {
        return Some("notification".to_string());
    }
    if path.starts_with("azul_core::url::") {
        return Some("url".to_string());
    }
    if path.starts_with("azul_core::xml::") {
        return Some("xml".to_string());
    }
    // azul_layout::*
    if path.starts_with("azul_layout::file::") {
        return Some("file".to_string());
    }
    if path.starts_with("azul_layout::desktop::file::") {
        return Some("file".to_string());
    }
    // The resumable-http result structs (`HttpGetResult`, ...) end in
    // `Result`, which the `error` keyword outweighs (6 > 4); their source
    // module is the authority.
    if path.starts_with("azul_layout::http::") {
        return Some("http".to_string());
    }
    // Same for the image-decode result struct (`ImageDecodeResult`).
    if path.starts_with("azul_layout::image::") {
        return Some("image".to_string());
    }
    // The CPU rasterizer draws into a `RawImage` (text to pixels too).
    if path.starts_with("azul_layout::cpurender::") {
        return Some("image".to_string());
    }
    if path.starts_with("azul_layout::fmt::") {
        return Some("fmt".to_string());
    }
    if path.starts_with("azul_layout::managers::gesture::") {
        return Some("gesture".to_string());
    }
    if path.starts_with("azul_layout::desktop::dialogs::") {
        return Some("dialog".to_string());
    }
    // The UUID mint (`Uuid::v4` / `Uuid::short`). Routed by path: the bare
    // name "Uuid" carries no keyword any other module could not also claim.
    if path.starts_with("azul_layout::uuid::") {
        return Some("uuid".to_string());
    }
    // azul_dll::unified::* backends
    if path.starts_with("azul_dll::unified::audio::") {
        return Some("audio".to_string());
    }
    if path.starts_with("azul_dll::unified::sqlite::") {
        return Some("db".to_string());
    }
    if path.starts_with("azul_dll::unified::pdf::") {
        return Some("pdf".to_string());
    }
    if path.starts_with("azul_dll::unified::video_codec::")
        || path.starts_with("azul_dll::desktop::extra::video_codec::")
    {
        return Some("video".to_string());
    }
    if path.starts_with("azul_dll::unified::webtransport::") {
        return Some("webtransport".to_string());
    }
    if path.starts_with("azul_dll::unified::iroh::") {
        return Some("iroh".to_string());
    }

    None
}

/// Types that should be excluded from the C API entirely
/// These contain non-FFI-safe types like BTreeMap, HashMap, Arc, VecDeque
pub const INTERNAL_ONLY_TYPES: &[&str] = &[
    // Manager types with BTreeMap/HashMap internals
    "ScrollManager",
    "HoverManager",
    "GpuStateManager",
    "GpuValueCache",
    "VirtualViewManager",
    "FocusManager",
    "GestureAndDragManager",
    "FileDropManager",
    "UndoRedoManager",
    "TextInputManager",
    // Types with BTreeMap fields
    "HitTest",
    "FullHitTest",
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
    fn a_keyword_counts_only_as_a_whole_word_of_the_name() {
        assert!(keyword_is_whole_word("NodeId", "node"));
        assert!(keyword_is_whole_word("TabIndex", "tabindex"));
        assert!(keyword_is_whole_word("StyledDom", "dom"));
        assert!(keyword_is_whole_word("CSSProperty", "property"));
        assert!(keyword_is_whole_word("css_property", "css"));
        assert!(!keyword_is_whole_word("AccordionVariant", "aria"));
        assert!(!keyword_is_whole_word("Random", "dom"));
        assert_eq!(word_starts("AccordionVariant"), vec![0, 9]);
        assert_eq!(word_starts("CSSProperty"), vec![0, 3]);
    }

    /// A plain widget struct/enum that the keyword matcher files elsewhere by a
    /// coincidental word in its name is still a widget: the scan moves it to
    /// `widgets`. `AccordionVariant` sat in `dom` for one round (2026-10-01).
    #[test]
    fn a_widget_enum_misfiled_by_its_name_is_moved_to_widgets() {
        assert_eq!(
            get_correct_module_with_path(
                "AccordionVariant",
                "dom",
                Some("azul_layout::widgets::accordion::AccordionVariant"),
            ),
            Some("widgets".to_string())
        );
        assert_eq!(
            get_correct_module_with_path(
                "AccordionVariant",
                "widgets",
                Some("azul_layout::widgets::accordion::AccordionVariant"),
            ),
            None
        );
    }

    /// The scan's move check and the add command place widget types by the
    /// same rule: a shell's callback wrapper stays in dom, its typedef in
    /// callbacks, its Vec in vec, the shell itself in shells.
    #[test]
    fn the_move_check_uses_the_widget_rule_of_the_add_command() {
        let path = |t: &str| format!("azul_layout::widgets::shells::command_palette::{t}");
        for (name, module) in [
            ("ShellCommandPaletteOnRunCallback", "dom"),
            ("ShellCommandPaletteOnRunCallbackType", "callbacks"),
            ("OptionShellCommandPaletteOnRun", "option"),
            ("ShellPaletteCommandVec", "vec"),
            ("ShellPaletteCommandVecSlice", "shells"),
            ("ShellCommandPalette", "shells"),
        ] {
            assert_eq!(widget_module_for(name, &path(name)).as_deref(), Some(module), "{name}");
            assert_eq!(get_correct_module_with_path(name, module, Some(&path(name))), None, "{name}");
        }
        assert_eq!(
            get_correct_module_with_path(
                "ShellCommandPaletteOnRunCallback",
                "shells",
                Some(&path("ShellCommandPaletteOnRunCallback"))
            )
            .as_deref(),
            Some("dom")
        );
    }

    /// The by-concern widget placements stay: callback wrappers in dom,
    /// callback typedefs in callbacks, options in option, vecs in vec.
    #[test]
    fn the_by_concern_widget_placements_are_not_moved() {
        for (name, module) in [
            ("ButtonOnClickCallback", "dom"),
            ("ButtonOnClickCallbackType", "callbacks"),
            ("OptionButtonOnClick", "option"),
            ("RibbonTabVec", "vec"),
        ] {
            assert_eq!(
                get_correct_module_with_path(name, module, Some("azul_layout::widgets::button::X")),
                None,
                "{name} in {module}"
            );
        }
    }

    /// `autofix add` placed a new type by a rule of its own (the widget rule,
    /// else the name's keywords) and the scan's move check by another: a name
    /// no keyword knows went to `misc` and the next scan moved it to its
    /// path's module; a widget's `*Error` went to `widgets` and the scan moved
    /// it to `error`. One rule for a new type: the placement the scan keeps.
    /// And `TextRasterStyle` (the CPU rasterizer's text style, beside
    /// `RawImage`) went to `css` by the word "Style"; MEDIA6 wanted `image` -
    /// the exceptions table names it, so the scan moves the one api.json has.
    #[test]
    fn a_new_type_goes_where_the_scan_keeps_it() {
        let raster = "azul_layout::cpurender::text_raster::TextRasterStyle";
        assert_eq!(new_type_module("TextRasterStyle", raster), ("image".to_string(), false));
        assert_eq!(
            get_correct_module_with_path("TextRasterStyle", "css", Some(raster)).as_deref(),
            Some("image"),
            "the one api.json already has moves to image"
        );
        assert_eq!(
            new_type_module("Quux", "azul_layout::cpurender::quux::Quux").0,
            "image",
            "a name no keyword knows goes where it lives"
        );
        assert_eq!(
            new_type_module("ListViewError", "azul_layout::widgets::list_view::ListViewError").0,
            "error"
        );
        // DEDUP_WIDGETS_API F17: a `*VecSlice` lives with its element - a
        // widget's slice in widgets, even when a word of its name is a
        // keyword of another module ("grid", "range")
        let slice = "azul_layout::widgets::cell_grid::CellGridRangeVecSlice";
        assert_eq!(new_type_module("CellGridRangeVecSlice", slice).0, "widgets");
        assert_eq!(get_correct_module_with_path("CellGridRangeVecSlice", "css", Some(slice)).as_deref(), Some("widgets"));
        for (name, path) in [
            ("TextRasterStyle", raster),
            ("Quux", "azul_layout::cpurender::quux::Quux"),
            ("OptionTextRasterStyle", "azul_layout::cpurender::text_raster::OptionTextRasterStyle"),
            ("TextRasterError", "azul_layout::cpurender::text_raster::TextRasterError"),
            ("ListViewError", "azul_layout::widgets::list_view::ListViewError"),
            ("FocusTarget", "azul_core::dom::FocusTarget"),
            ("ComponentFoo", "azul_core::xml::ComponentFoo"),
            ("ButtonOnClickCallback", "azul_layout::widgets::button::ButtonOnClickCallback"),
            ("ShellPaletteCommandVecSlice", "azul_layout::widgets::shells::p::ShellPaletteCommandVecSlice"),
            ("WindowFlags", "azul_core::window::WindowFlags"),
            ("SvgFillStyle", "azul_layout::svg::SvgFillStyle"),
            ("Quux", "azul_layout::nowhere::Quux"),
        ] {
            let (module, _) = new_type_module(name, path);
            assert_eq!(
                get_correct_module_with_path(name, &module, Some(path)),
                None,
                "{name} added to {module} must stay there"
            );
        }
    }

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
        // Equal-length tie between a module-NAME match and a generic keyword:
        // "filepath" contains module name "file" (4) and svg keyword "path"
        // (4) — the module name must win (FilePath was mis-bucketed into svg).
        assert_eq!(determine_module("FilePath").0, "file");
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
        // RefAny belongs to `callbacks`: "refany" is in that module's keyword
        // list (see the `// RefAny, RefCount` entry), because RefAny is the
        // callback data payload. This assertion used to expect "misc" with the
        // comment "has no matching keywords" — true when it was written, stale
        // once the keyword was added. Nothing caught the drift because azul-doc's
        // tests run in no CI job (audit D1). Getting this wrong is not cosmetic:
        // autofix MOVES types between modules, so a wrong answer edits the wrong
        // file.
        assert_eq!(
            get_correct_module("RefAny", "refany"),
            Some("callbacks".to_string())
        );
        // Same story as RefAny: "cascade" is now a `css` keyword (see the
        // `// CascadeInfo` entry), so CascadeInfo belongs to css, not misc.
        assert_eq!(
            get_correct_module("CascadeInfo", "style"),
            Some("css".to_string())
        );
        // SvgStrokeStyle contains "svg" and "style", svg is a module name so should go to svg
        assert_eq!(
            get_correct_module("SvgStrokeStyle", "style"),
            Some("svg".to_string())
        );
        // Already in correct module
        assert_eq!(get_correct_module("CssProperty", "css"), None);
    }

    /// Every class the resumable-API remodel adds must be a fixpoint of the
    /// autofix pipeline in the module it was placed in: a wrong verdict here
    /// means `autofix` moves the class on the next CI run.
    #[test]
    fn test_resumable_api_classes_stay_put() {
        let stays = |name: &str, module: &str, external: &str| {
            assert_eq!(
                get_correct_module_with_path(name, module, Some(external)),
                None,
                "{name} (external {external}) must stay in `{module}`"
            );
        };
        // Phase 0 - the primitive.
        stays("RequestId", "task", "azul_core::task::RequestId");
        stays(
            "ResumeCallbackType",
            "callbacks",
            "azul_layout::callbacks::ResumeCallbackType",
        );
        stays(
            "ResumeCallback",
            "callbacks",
            "azul_layout::callbacks::ResumeCallback",
        );
        // Phase 1 - result structs pinned by their source module; by name
        // alone the `result` keyword would send every one of them to `error`.
        stays(
            "FileOpenResult",
            "dialog",
            "azul_layout::desktop::dialogs::FileOpenResult",
        );
        stays(
            "FileOpenMultiResult",
            "dialog",
            "azul_layout::desktop::dialogs::FileOpenMultiResult",
        );
        stays(
            "ColorPickResult",
            "dialog",
            "azul_layout::desktop::dialogs::ColorPickResult",
        );
        stays(
            "FileReadBytesResult",
            "file",
            "azul_layout::file::FileReadBytesResult",
        );
        stays(
            "FileReadStringResult",
            "file",
            "azul_layout::file::FileReadStringResult",
        );
        stays(
            "ImageDecodeResult",
            "image",
            "azul_layout::image::ImageDecodeResult",
        );
        stays(
            "FilePathVecSlice",
            "file",
            "azul_layout::file::FilePathVecSlice",
        );
        // Phase 2.
        stays("HttpGetResult", "http", "azul_layout::http::HttpGetResult");
        stays(
            "HttpBytesResult",
            "http",
            "azul_layout::http::HttpBytesResult",
        );
        stays(
            "HttpReachableResult",
            "http",
            "azul_layout::http::HttpReachableResult",
        );
        // Phase 3.
        stays(
            "SaveTarget",
            "dialog",
            "azul_layout::desktop::dialogs::SaveTarget",
        );
        stays(
            "SaveTargetKind",
            "dialog",
            "azul_layout::desktop::dialogs::SaveTargetKind",
        );
        stays(
            "SaveTargetResult",
            "dialog",
            "azul_layout::desktop::dialogs::SaveTargetResult",
        );
        stays(
            "FileDirListResult",
            "file",
            "azul_layout::file::FileDirListResult",
        );
        // Phase 4.
        stays(
            "AudioDeviceListResult",
            "audio",
            "azul_dll::unified::audio::AudioDeviceListResult",
        );
        // These two sit next to `DecodedVideo` / `ScreenRecorder`, whose Rust
        // home maps to `video`; the keyword verdict (not the path) keeps them.
        stays(
            "VideoDecodeResult",
            "image",
            "azul_dll::unified::video_codec::pipeline::VideoDecodeResult",
        );
        stays(
            "ScreenRecordingResult",
            "screen",
            "azul_dll::unified::video_codec::ScreenRecordingResult",
        );
        // Phase R.
        stays("DbConfig", "db", "azul_core::db::DbConfig");
        stays("DbOpenResult", "db", "azul_core::db::DbOpenResult");
        stays("DbSyncStatus", "db", "azul_core::db::DbSyncStatus");
        stays("DbMergeCallback", "db", "azul_core::db::DbMergeCallback");
        stays(
            "DbMergeCallbackType",
            "db",
            "azul_core::db::DbMergeCallbackType",
        );
        stays("DbConflict", "db", "azul_core::db::DbConflict");
        stays(
            "DbCollectionScopeVecSlice",
            "db",
            "azul_core::db::DbCollectionScopeVecSlice",
        );
        stays(
            "DbOpenResult",
            "db",
            "azul_dll::unified::sqlite::DbOpenResult",
        );
        // Structural verdicts are not pins and must keep winning.
        stays("FilePathVec", "vec", "azul_layout::file::FilePathVec");
        stays(
            "OptionFileOpenResult",
            "option",
            "azul_layout::desktop::dialogs::OptionFileOpenResult",
        );
        stays("ResultDbDbError", "error", "azul_core::db::ResultDbDbError");
        stays("DbError", "error", "azul_core::db::DbError");
    }

    /// The app shells resolve to their own module, `shells`, by name - the
    /// S-shells by their "shell" stem, the shared pieces by the longer stems
    /// that outrank the dom and css words inside their names - and by their
    /// source path.
    #[test]
    fn shell_types_resolve_to_shells_by_name_and_by_path() {
        for name in [
            "OfficeShell",
            "ShellPane",
            "ShellPaneKind",
            "ShellOnPaneFocus",
            "ShellOnPaneResize",
            "ShellNavigationPane",
            "ShellNavigationPaneEvent",
            "ShellNavigationPaneEventKind",
            "ShellNavigationGroup",
            "ShellNavigationModule",
            "ShellCommandPalette",
            "ShellPaletteCommand",
            "ShellSettingsLayout",
            "ShellSettingsSection",
            "ShellEmptyState",
            "ShellThemeScope",
            "ShellThemeAccent",
            "ShellThemeAccentColors",
            "ShellBottomTab",
            "DocumentShell",
            "CanvasShell",
            "TimelineShell",
            "PimShell",
            "BrowserShell",
            "RecordsShell",
            "MediaShell",
            "DeveloperShell",
            "UtilityShell",
            "CallShell",
            "MobileShell",
        ] {
            let (module, is_warning) = determine_module(name);
            assert_eq!(module, "shells", "{name} must resolve to shells");
            assert!(!is_warning, "{name} must resolve confidently");
        }
        assert_eq!(
            module_from_external_path("azul_layout::widgets::shells::office_shell::OfficeShell"),
            Some("shells".to_string())
        );
        assert_eq!(
            module_from_external_path("azul_layout::widgets::button::Button"),
            Some("widgets".to_string()),
            "the smaller widgets stay in widgets"
        );
        // The by-concern types keep the established placement.
        assert_eq!(determine_module("OptionShellPane").0, "option");
        assert_eq!(determine_module("ShellPaneVec").0, "vec");
    }

    #[test]
    fn test_exclude_paths() {
        use std::path::Path;
        assert!(should_exclude_path(Path::new("/foo/tests/some_test.rs")));
        assert!(should_exclude_path(Path::new("/foo/examples/demo.rs")));
        assert!(should_exclude_path(Path::new("/foo/build.rs")));
        assert!(!should_exclude_path(Path::new("/foo/src/lib.rs")));
    }

    /// The manual override runs BEFORE the keyword heuristic, and is what
    /// keeps `Tablet*` out of `css`.
    ///
    /// Without it the css keyword "table" matches as a substring inside
    /// "TABLEt-PadState" and wins, CONFIDENTLY — which then short-circuits the
    /// external-path check in `get_correct_module_with_path`, so the type is
    /// reported as correctly placed and can never move.
    #[test]
    fn difficult_names_are_matched_before_the_keyword_heuristic() {
        for name in [
            "TabletPadState",
            "TabletDeviceInfo",
            "TabletToolKind",
            "TabletDeviceInfoVecSlice",
        ] {
            let (module, is_warning) = determine_module(name);
            assert_eq!(module, "gesture", "{name} must resolve to gesture");
            assert!(!is_warning, "{name} must resolve confidently");
        }
    }

    /// Haptics and dials had NO keyword in any module, so they landed in
    /// "misc" - the junk drawer - which is where a binding user would never
    /// look for an input type.
    #[test]
    fn haptic_and_dial_types_resolve_to_gesture() {
        for name in [
            "HapticPattern",
            "HapticTarget",
            "HapticRequest",
            "DialState",
        ] {
            let (module, is_warning) = determine_module(name);
            assert_eq!(module, "gesture", "{name} must resolve to gesture");
            assert!(!is_warning, "{name} must resolve confidently");
        }
    }

    /// The `DialState` entry is spelled in full for exactly this reason: a
    /// "Dial" prefix also matches "Dialog", and it really did pull
    /// `DialogAriaInfo` out of the dialog module.
    #[test]
    fn the_dial_override_does_not_capture_dialog_types() {
        assert_eq!(difficult_type_module("DialState"), Some("gesture"));
        assert_eq!(difficult_type_module("DialogAriaInfo"), None);
        assert_eq!(determine_module("DialogAriaInfo").0, "dialog");
    }

    /// A "Hid" prefix would also match `HidpiAdjustedBounds` (HiDPI), so the
    /// HID entries are spelled in full. Third instance of this trap.
    #[test]
    fn the_hid_override_does_not_capture_hidpi() {
        assert_eq!(difficult_type_module("HidDevice"), Some("gamepad"));
        assert_eq!(difficult_type_module("HidReport"), Some("gamepad"));
        assert_eq!(difficult_type_module("HidpiAdjustedBounds"), None);
        assert_ne!(determine_module("HidpiAdjustedBounds").0, "gamepad");
    }

    /// A "Media" prefix would also capture `MediaType`, the CSS `@media` type,
    /// and drag it out of css into audio. Fourth word-boundary trap in this
    /// table, so it gets the same guard the other three have.
    #[test]
    fn the_media_session_entries_do_not_capture_the_css_media_type() {
        assert_eq!(difficult_type_module("MediaPlaybackState"), Some("audio"));
        assert_eq!(difficult_type_module("NowPlayingInfo"), Some("audio"));
        assert_eq!(difficult_type_module("EchoCanceller"), Some("audio"));
        assert_eq!(difficult_type_module("MediaType"), None);
        assert_ne!(determine_module("MediaType").0, "audio");
    }

    /// The sample belongs with the pen types it carries, not in "dom".
    #[test]
    fn the_input_sample_override_lands_with_the_other_callback_types() {
        assert_eq!(difficult_type_module("InputSample"), Some("callbacks"));
        assert_eq!(difficult_type_module("ValidityState"), Some("callbacks"));
        assert_eq!(difficult_type_module("ValidityReason"), Some("callbacks"));
        // The Option wrapper is routed by the option rule, not by this table,
        // and must keep going to "option" rather than following the prefix.
        assert_eq!(determine_module("OptionInputSample").0, "option");
    }

    /// "GlobalHotkey" contains the OpenGL module's own name ("GLobal"), and a
    /// module-name match outranks a keyword, so every global-hotkey type was
    /// filed under `gl`. They are app-wide registrations and belong beside
    /// `App`; the structural rules (error / Result / Option) must still win.
    #[test]
    fn global_hotkey_types_resolve_to_app_not_gl() {
        for name in [
            "GlobalHotkey",
            "GlobalHotkeyId",
            "GlobalHotkeyStatus",
            "HotkeyModifiers",
            // The declarative API (2026-09-28): the plural "GlobalHotkeys"
            // of the AppConfig callback is still the same prefix.
            "GlobalHotkeyCallbackData",
            "GlobalHotkeyInfo",
            "GlobalHotkeyOwner",
            "GlobalHotkeyState",
            "GlobalHotkeyEvent",
            "GlobalHotkeysCallback",
            "GlobalHotkeysCallbackInfo",
            "GlobalHotkeysCallbackType",
        ] {
            let (module, is_warning) = determine_module(name);
            assert_eq!(module, "app", "{name} must resolve to app");
            assert!(!is_warning, "{name} must resolve confidently");
        }
        assert_eq!(determine_module("GlobalHotkeyError").0, "error");
        assert_eq!(
            determine_module("ResultGlobalHotkeyGlobalHotkeyError").0,
            "error"
        );
        assert_eq!(determine_module("GlobalHotkeyInfoVec").0, "vec");
        assert_eq!(determine_module("GlobalHotkeyCallbackDataVec").0, "vec");
        assert_eq!(determine_module("OptionGlobalHotkeyEvent").0, "option");
        assert_eq!(determine_module("OptionGlobalHotkeysCallback").0, "option");
        // Spelled "GlobalHotkey", not "Global": the fifth word-boundary trap
        // this table would otherwise have grown.
        assert_eq!(difficult_type_module("GlobalCss"), None);
    }

    /// The override is a PREFIX match, so it must not capture the css `table`
    /// family it exists to be distinguished from.
    #[test]
    fn the_tablet_override_does_not_capture_table_types() {
        assert_eq!(difficult_type_module("TabletPadState"), Some("gesture"));
        assert_eq!(difficult_type_module("TableLayout"), None);
        assert_eq!(difficult_type_module("StyleTableLayout"), None);
    }

    #[test]
    fn transport_events_stay_with_their_transport() {
        for (name, module) in [
            ("WtEvent", "webtransport"),
            ("WtEventKind", "webtransport"),
            ("IrohEvent", "iroh"),
            ("IrohEventKind", "iroh"),
            ("IrohEndpoint", "iroh"),
            ("IrohLoadBalancer", "iroh"),
            ("IrohPeerCapacity", "iroh"),
            ("IrohTileRole", "iroh"),
        ] {
            assert_eq!(determine_module(name).0, module, "{name}");
            assert_eq!(get_correct_module(name, module), None, "{name}");
        }
        assert_eq!(determine_module("OptionIrohEvent").0, "option");
        assert_eq!(
            get_correct_module_with_path(
                "IrohPeerStats",
                "misc",
                Some("azul_dll::unified::iroh::IrohPeerStats")
            ),
            Some("iroh".to_string())
        );
    }

    /// Structural types are resolved BEFORE the override table, so an entry
    /// there cannot steal `OptionFoo` / `FooVec` from their own modules.
    #[test]
    fn structural_types_still_win_over_the_override() {
        assert_eq!(determine_module("OptionTabletPadState").0, "option");
        assert_eq!(determine_module("TabletDeviceInfoVec").0, "vec");
    }

    /// The override is for EXCEPTIONS: ordinary names must still be
    /// classified automatically by the keyword matcher.
    #[test]
    fn ordinary_names_are_still_classified_automatically() {
        assert_eq!(difficult_type_module("StyledDom"), None);
        assert_eq!(difficult_type_module("FontMetrics"), None);
        // ...and still reach a module through the heuristic.
        assert!(!determine_module("FontMetrics").0.is_empty());
    }

    /// A widget's `*VecSlice` stays with its widget, whatever keyword its
    /// name contains: `CellGridRangeVecSlice` holds "grid" (a css keyword),
    /// `ToDoTaskVecSlice` "task", `WizardOptionVecSlice` "option",
    /// `WizardComponentVecSlice` "component", the node graph's slices
    /// "node" / "input" (dom). The move check treated `vecslice` as a
    /// structural suffix and returned the keyword answer before it asked the
    /// widget rule, so 14 widget slices sat in css / task / option /
    /// component / dom (DEDUP_WIDGETS_API F17).
    #[test]
    fn a_widget_vec_slice_is_placed_by_the_widget_rule_not_by_its_name() {
        for (name, file) in [
            ("CellGridRangeVecSlice", "cell_grid"),
            ("CellGridSizeVecSlice", "cell_grid"),
            ("TimelineClipVecSlice", "timeline"),
            ("ToDoTaskVecSlice", "todo_bar"),
            ("WizardOptionVecSlice", "wizard_pages"),
            ("WizardComponentVecSlice", "wizard_pages"),
            ("InputConnectionVecSlice", "node_graph"),
            ("NodeTypeFieldVecSlice", "node_graph"),
            ("OutputNodeAndIndexVecSlice", "node_graph"),
        ] {
            let path = format!("azul_layout::widgets::{file}::{name}");
            assert_eq!(
                widget_module_for(name, &path).as_deref(),
                Some("widgets"),
                "{name}: the add command's rule"
            );
            assert_eq!(
                get_correct_module_with_path(name, "widgets", Some(&path)),
                None,
                "{name} in widgets must stay there"
            );
            let (keyword_module, _) = determine_module(name);
            assert_eq!(
                get_correct_module_with_path(name, &keyword_module, Some(&path)).as_deref(),
                Some("widgets"),
                "{name} in {keyword_module} must move to widgets"
            );
        }
    }

    /// One "is a Vec type" rule: the name-only classifier, the widget rule
    /// and the move check agree on every member of the Vec family, and none
    /// of them counts a `*VecSlice` as one (a slice is a borrowed view of its
    /// element and lives with it: `StringVecSlice` in `str`, `DomVecSlice` in
    /// `dom`).
    #[test]
    fn the_three_vec_rules_agree_and_a_vec_slice_is_not_a_vec() {
        let widget = |t: &str| format!("azul_layout::widgets::ribbon::{t}");
        for name in [
            "RibbonTabVec",
            "RibbonTabVecDestructor",
            "RibbonTabVecDestructorType",
            "RibbonTabVecRef",
            "RibbonTabVecRefMut",
        ] {
            assert_eq!(determine_module(name).0, "vec", "{name}");
            assert_eq!(widget_module_for(name, &widget(name)).as_deref(), Some("vec"), "{name}");
            assert_eq!(
                get_correct_module_with_path(name, "widgets", Some(&widget(name))).as_deref(),
                Some("vec"),
                "{name}"
            );
        }
        assert_ne!(determine_module("RibbonTabVecSlice").0, "vec");
        assert_eq!(
            widget_module_for("RibbonTabVecSlice", &widget("RibbonTabVecSlice")).as_deref(),
            Some("widgets")
        );
        // Non-widget slices keep their established, element-based modules.
        assert_eq!(
            get_correct_module_with_path(
                "StringVecSlice",
                "str",
                Some("azul_css::corety::StringVecSlice")
            ),
            None
        );
        assert_eq!(
            get_correct_module_with_path(
                "DomVecSlice",
                "dom",
                Some("azul_core::dom::DomVecSlice")
            ),
            None
        );
    }
}

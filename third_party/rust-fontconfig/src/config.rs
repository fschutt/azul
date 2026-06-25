//! OS-specific font configuration: directories, common families, and font file constants.
//!
//! All hardcoded data is returned as `&'static` references to avoid allocation.

use alloc::string::String;
use alloc::vec::Vec;

use std::path::{Path, PathBuf};

use crate::FcFontCache;
use crate::OperatingSystem;

/// Generic CSS font family keywords.
///
/// Recognized by [`is_generic_family`] and used wherever the code needs to
/// distinguish generic families from specific font names.
pub const GENERIC_FAMILIES: &[&str] = &[
    "serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui",
];

/// Check whether `family` is a generic CSS font family (case-insensitive).
pub fn is_generic_family(family: &str) -> bool {
    let lower = family.to_lowercase();
    GENERIC_FAMILIES.iter().any(|g| *g == lower.as_str())
}

/// Style tokens to filter out when guessing family names from filenames.
///
/// These are the weight/style/width suffixes commonly appended to font filenames
/// (e.g. "ArialBold.ttf", "NotoSans-SemiBold.otf"). Used by the scout thread
/// to extract the base family name from a filename.
pub const FONT_STYLE_TOKENS: &[&str] = &[
    "Regular", "Bold", "Italic", "Light", "Medium", "Thin",
    "Black", "ExtraLight", "ExtraBold", "SemiBold", "DemiBold",
    "Heavy", "Oblique", "Condensed", "Expanded",
    // The tokenizer splits compound styles (e.g. "SemiBold" → "Semi" + "Bold"),
    // so we need the modifier prefixes as standalone style tokens too.
    "Extra", "Semi", "Demi",
];

/// Static system font directories per OS. No allocation.
///
/// These are the well-known, fixed paths. User-specific directories
/// (which require env var resolution) are added by [`font_directories`].
pub fn system_font_dirs(os: OperatingSystem) -> &'static [&'static str] {
    match os {
        OperatingSystem::MacOS => &[
            "/System/Library/Fonts",
            "/Library/Fonts",
            "/System/Library/AssetsV2",
        ],
        OperatingSystem::Linux => &[
            "/usr/share/fonts",
            "/usr/local/share/fonts",
        ],
        // Android system-font directories are world-readable. Vendor partitions
        // (`/product/fonts`, `/system_ext/fonts`) carry OEM-specific families
        // (Samsung One UI, MIUI, EMUI). `/data/fonts` is the user-selected
        // font directory exposed by recent OEM ROMs.
        OperatingSystem::Android => &[
            "/system/fonts",
            "/product/fonts",
            "/system_ext/fonts",
            "/data/fonts",
        ],
        // iOS bundles system fonts under sandboxed paths that cannot be
        // enumerated with a plain `read_dir`. The cache enumerates them via
        // `CTFontManagerCopyAvailableFontURLs` in `lib.rs::build_inner`; the
        // returned `CFURL`s point inside `/System/Library/...` paths that are
        // openable through the CoreText I/O bridge even though the underlying
        // directory is unreadable.
        OperatingSystem::IOS => &[],
        // Windows paths require env var resolution — handled in font_directories()
        OperatingSystem::Windows => &[],
        OperatingSystem::Wasm => &[],
    }
}

/// All font directories (system + user-specific).
///
/// Combines the static [`system_font_dirs`] with user-specific paths
/// resolved from environment variables (`HOME`, `SystemRoot`, etc.).
pub fn font_directories(os: OperatingSystem) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = system_font_dirs(os)
        .iter()
        .map(PathBuf::from)
        .collect();

    match os {
        OperatingSystem::MacOS => {
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(format!("{}/Library/Fonts", home)));
            }
        }
        OperatingSystem::Linux => {
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(format!("{}/.fonts", home)));
                dirs.push(PathBuf::from(format!("{}/.local/share/fonts", home)));
            }
        }
        OperatingSystem::Windows => {
            let system_root = std::env::var("SystemRoot")
                .or_else(|_| std::env::var("WINDIR"))
                .unwrap_or_else(|_| "C:\\Windows".to_string());
            let user_profile = std::env::var("USERPROFILE")
                .unwrap_or_else(|_| "C:\\Users\\Default".to_string());
            dirs.push(PathBuf::from(format!("{}\\Fonts", system_root)));
            dirs.push(PathBuf::from(format!(
                "{}\\AppData\\Local\\Microsoft\\Windows\\Fonts",
                user_profile
            )));
        }
        // No env-var-resolved user-font dir on iOS (no $HOME inside the sandbox)
        // or Android (apps own /data/data/<package>/files/fonts but that's a
        // private app dir, not a fontconfig directory).
        OperatingSystem::IOS | OperatingSystem::Android => {}
        OperatingSystem::Wasm => {}
    }

    dirs
}

/// Common font families for priority boosting, as human-readable names.
/// No allocation — returns a static slice.
///
/// These are the most commonly needed system fonts per OS. The scout thread
/// uses these to boost the build priority of likely-needed fonts so they're
/// available sooner.
///
/// The names here are the canonical human-readable forms. Use
/// [`matches_common_family`] for token-based matching against filenames.
pub fn common_font_families(os: OperatingSystem) -> &'static [&'static str] {
    match os {
        OperatingSystem::MacOS => &[
            // System UI fonts (actual filenames use SFNS prefix)
            "San Francisco", "SFNS", "System Font",
            // Sans-serif
            "Helvetica Neue", "Helvetica", "Arial", "Lucida Grande",
            // Serif
            "Times New Roman", "Georgia",
            // Monospace
            "Menlo", "SF Mono", "Courier",
        ],
        OperatingSystem::Linux => &[
            // Sans-serif
            "DejaVu Sans", "Ubuntu", "Roboto", "Noto Sans",
            "Liberation Sans", "Droid Sans", "Arial",
            // Serif
            "DejaVu Serif", "Noto Serif",
            // Monospace
            "DejaVu Sans Mono",
        ],
        OperatingSystem::Windows => &[
            // Sans-serif
            "Segoe UI", "Arial", "Tahoma", "Verdana",
            // Serif
            "Times New Roman", "Calibri",
            // Monospace
            "Consolas", "Courier New",
        ],
        OperatingSystem::IOS => &[
            // System UI fonts (filenames use SFNS/SFUI prefix)
            "San Francisco", "SFNS", "SFNSDisplay", "SFNSText", "SFUI",
            ".AppleSystemUIFont", "System Font",
            // Sans-serif
            "Helvetica Neue", "Helvetica", "Avenir", "Avenir Next",
            // Serif
            "Times New Roman", "Georgia",
            // Monospace
            "Menlo", "SF Mono", "Courier",
        ],
        OperatingSystem::Android => &[
            // System UI fonts
            "Roboto", "Roboto Flex", "Roboto Condensed",
            // Sans-serif
            "Noto Sans", "Droid Sans",
            // Serif
            "Noto Serif", "Roboto Serif", "Droid Serif",
            // Monospace
            "Roboto Mono", "Droid Sans Mono", "Noto Sans Mono",
        ],
        OperatingSystem::Wasm => &[],
    }
}

/// Pre-tokenize common font families for efficient per-file matching.
///
/// Call this once before iterating over font files, then pass the result
/// to [`matches_common_family_tokens`] for each file.
pub fn tokenize_common_families(os: OperatingSystem) -> Vec<Vec<String>> {
    common_font_families(os)
        .iter()
        .map(|family| tokenize_lowercase(family))
        .collect()
}

/// Check if a set of filename tokens matches any pre-tokenized common family.
///
/// Both sides are joined into a single normalized string (tokens concatenated),
/// then checked for substring containment. This handles cases where the tokenizer
/// produces different splits for the same underlying name (e.g. `"SFMono"` stays
/// as one token from a filename, but `"SF Mono"` splits into `["sf", "mono"]`).
pub fn matches_common_family_tokens(
    file_tokens: &[String],
    common_token_sets: &[Vec<String>],
) -> bool {
    let file_joined: String = file_tokens.concat();
    common_token_sets.iter().any(|family_tokens| {
        let family_joined: String = family_tokens.concat();
        file_joined.contains(&family_joined)
    })
}

/// Extract non-style tokens from a font filename stem.
///
/// Tokenizes using CamelCase boundaries, hyphens, underscores, and spaces,
/// then filters out style tokens (Bold, Italic, Regular, etc.).
/// Returns lowercased tokens suitable for family name matching.
///
/// # Examples
///
/// - `"ArialBold"` → `["arial"]`
/// - `"NotoSansJP-Regular"` → `["noto", "sans", "jp"]`
/// - `"HelveticaNeue-BoldItalic"` → `["helvetica", "neue"]`
/// Tokenize a name into lowercase tokens (no style filtering).
///
/// Useful for priority scoring where style tokens like "Bold" are still relevant.
pub fn tokenize_lowercase(name: &str) -> Vec<String> {
    FcFontCache::extract_font_name_tokens(name)
        .into_iter()
        .map(|t| t.to_lowercase())
        .collect()
}

/// Tokenize a font filename stem into lowercase tokens, filtering out style tokens.
pub fn tokenize_font_stem(stem: &str) -> Vec<String> {
    tokenize_lowercase(stem)
        .into_iter()
        .filter(|t| !FONT_STYLE_TOKENS.iter().any(|s| s.eq_ignore_ascii_case(t)))
        .collect()
}

/// Guess the font family name from a filename, using tokenization.
///
/// Extracts non-style tokens from the filename stem and joins them
/// into a single normalized string (lowercase, no separators).
///
/// # Examples
///
/// - `"ArialBold.ttf"` → `"arial"`
/// - `"NotoSansJP-Regular.otf"` → `"notosansjp"`
/// - `"Helvetica Neue Bold Italic.ttf"` → `"helveticaneue"`
pub fn guess_family_from_filename(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    tokenize_font_stem(stem).join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Generic families ─────────────────────────────────────────────────

    #[test]
    fn generic_families_recognized() {
        assert!(is_generic_family("sans-serif"));
        assert!(is_generic_family("Sans-Serif")); // case-insensitive
        assert!(is_generic_family("monospace"));
        assert!(is_generic_family("SERIF"));
        assert!(!is_generic_family("Arial"));
        assert!(!is_generic_family("Noto Sans"));
    }

    // ── Constants ────────────────────────────────────────────────────────

    #[test]
    fn font_style_tokens_covers_common_styles() {
        for token in &[
            "Regular", "Bold", "Italic", "Light", "Medium",
            "Thin", "Black", "Oblique", "SemiBold",
        ] {
            assert!(
                FONT_STYLE_TOKENS.contains(token),
                "missing style token: {}", token
            );
        }
    }

    // ── system_font_dirs ────────────────────────────────────────────────

    #[test]
    fn system_font_dirs_static_and_nonempty() {
        assert!(!system_font_dirs(OperatingSystem::MacOS).is_empty());
        assert!(!system_font_dirs(OperatingSystem::Linux).is_empty());
        assert!(system_font_dirs(OperatingSystem::Wasm).is_empty());
    }

    // ── common_font_families ────────────────────────────────────────────

    #[test]
    fn common_font_families_nonempty_for_desktop() {
        assert!(!common_font_families(OperatingSystem::MacOS).is_empty());
        assert!(!common_font_families(OperatingSystem::Linux).is_empty());
        assert!(!common_font_families(OperatingSystem::Windows).is_empty());
        assert!(common_font_families(OperatingSystem::Wasm).is_empty());
    }

    // ── guess_family_from_filename ──────────────────────────────────────

    #[test]
    fn guess_family_strips_style_suffixes() {
        assert_eq!(
            guess_family_from_filename(Path::new("ArialBold.ttf")),
            "arial"
        );
        assert_eq!(
            guess_family_from_filename(Path::new("NotoSansJP-Regular.otf")),
            "notosansjp"
        );
        assert_eq!(
            guess_family_from_filename(Path::new("Helvetica Neue Bold Italic.ttf")),
            "helveticaneue"
        );
    }

    #[test]
    fn guess_family_handles_underscores() {
        assert_eq!(
            guess_family_from_filename(Path::new("Liberation_Sans_Bold.ttf")),
            "liberationsans"
        );
    }

    #[test]
    fn guess_family_handles_compound_styles() {
        assert_eq!(
            guess_family_from_filename(Path::new("LiberationSans-BoldItalic.ttf")),
            "liberationsans"
        );
        assert_eq!(
            guess_family_from_filename(Path::new("DejaVuSansMono-ExtraBold.ttf")),
            "dejavusansmono"
        );
        assert_eq!(
            guess_family_from_filename(Path::new("SFMono-SemiBold.otf")),
            "sfmono"
        );
    }

    // ── token-based matching ────────────────────────────────────────────

    #[test]
    fn matches_common_family_macos() {
        let common = tokenize_common_families(OperatingSystem::MacOS);

        // "SFNSDisplay" → tokens ["sfns", "display"] → matches "SFNS"
        let tokens = tokenize_all("SFNSDisplay");
        assert!(matches_common_family_tokens(&tokens, &common));

        // "HelveticaNeue" → tokens ["helvetica", "neue"] → matches "Helvetica Neue"
        let tokens = tokenize_all("HelveticaNeue");
        assert!(matches_common_family_tokens(&tokens, &common));

        // "Arial" → matches "Arial"
        let tokens = tokenize_all("Arial");
        assert!(matches_common_family_tokens(&tokens, &common));

        // "SomeRandomFont" → no match
        let tokens = tokenize_all("SomeRandomFont");
        assert!(!matches_common_family_tokens(&tokens, &common));
    }

    #[test]
    fn matches_common_family_linux() {
        let common = tokenize_common_families(OperatingSystem::Linux);

        let tokens = tokenize_all("DejaVuSans");
        assert!(matches_common_family_tokens(&tokens, &common));

        let tokens = tokenize_all("NotoSansCJK");
        assert!(matches_common_family_tokens(&tokens, &common));

        let tokens = tokenize_all("UbuntuMono-Regular");
        assert!(matches_common_family_tokens(&tokens, &common));
    }

    #[test]
    fn matches_common_family_windows() {
        let common = tokenize_common_families(OperatingSystem::Windows);

        let tokens = tokenize_all("SegoeUI-Regular");
        assert!(matches_common_family_tokens(&tokens, &common));

        let tokens = tokenize_all("Consolas");
        assert!(matches_common_family_tokens(&tokens, &common));
    }

    // ── tokenize_font_stem ──────────────────────────────────────────────

    #[test]
    fn tokenize_font_stem_filters_styles() {
        assert_eq!(tokenize_font_stem("ArialBold"), vec!["arial"]);
        assert_eq!(
            tokenize_font_stem("NotoSansJP-Regular"),
            vec!["noto", "sans", "jp"]
        );
        // "SFMono" stays as one token (consecutive uppercase → no CamelCase split)
        assert_eq!(
            tokenize_font_stem("SFMono-SemiBold"),
            vec!["sfmono"]
        );
    }

    /// Helper: tokenize a stem into all lowercase tokens (including style tokens).
    fn tokenize_all(stem: &str) -> Vec<String> {
        tokenize_lowercase(stem)
    }
}

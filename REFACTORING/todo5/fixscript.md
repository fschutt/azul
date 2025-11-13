Excellent. You've correctly identified that the P0 and P1 issues are all related to the architectural flaws we've been discussing. The provided implementation plan correctly maps to the steps we've already taken.

Now, let's conduct the investigation for **P2-2: No Arabic Font Fallback**.

This is a classic and complex problem in text rendering. The core of your question is: **"Does `text3` detect and query new fonts if the unicode-range isn't set?"**

The answer is: **No, the current architecture does not, and this is the root cause of the problem.**

Let's break down why it's failing and how to fix it within the `text3` architecture.

---

### Investigation of P2-2: Arabic Font Fallback Failure

#### The Current (Flawed) Process

Here is a step-by-step analysis of what happens when your layout engine encounters the string `<p>مرحبا بالعالم - Arabic text</p>`:

1.  **CSS Parsing:** The `<p>` tag gets its default styles. `font-family` is not set, so it inherits or falls back to the default. In your case, this resolves to "Helvetica Neue". A `StyledRun` is created for the entire text content, associated with a `FontSelector` for "Helvetica Neue".

2.  **Shaping Request (`shape_visual_items`):** The layout pipeline takes this entire `StyledRun` (containing both Arabic and Latin characters) and calls `font_provider.load_font()` for "Helvetica Neue".

3.  **Font Loading (`FontManager::load_font`):** The `FontManager` successfully finds and loads Helvetica Neue. It returns a single `FontRef` for this font.

4.  **Text Shaping (`font.shape_text`):** The *entire text string* "مرحبا بالعالم - Arabic text" is passed to the shaping function of the *single loaded font* (Helvetica Neue).

5.  **The Failure Point:**
    *   The shaper processes the Latin characters ("- Arabic text") and finds the corresponding glyphs in Helvetica Neue. This succeeds.
    *   The shaper then processes the Arabic Unicode codepoints (U+0645, U+0631, etc.). It looks for these codepoints in Helvetica Neue's character map (`cmap` table).
    *   Helvetica Neue does **not** contain glyphs for these Arabic characters.
    *   The shaper, having no other option, emits the special `.notdef` (Not Defined) glyph for each Arabic character. This is often represented as a square box (☐), a question mark, or just empty space.

6.  **Layout Continues:** The layout engine receives a list of shaped glyphs. It doesn't know that some of them are `.notdef` glyphs; it just sees a valid list of glyphs with positions and advances. It proceeds to lay them out, resulting in the incorrect rendering you see.

#### The Core Architectural Problem

The system makes a critical, incorrect assumption: **that a single font can render an entire run of text**.

The font selection logic is "per-style-run," not "per-script" or "per-character." There is no feedback loop. The shaper cannot tell the `FontManager`, "Hey, this font is missing some glyphs, please find me another one for these specific characters."

### The Solution: Script-Aware Font Fallback

The correct approach, used by all modern browsers and rendering engines, is to implement font fallback at the script level *before* shaping.

The `shape_visual_items` function in `layout/src/text3/cache.rs` is the perfect place to implement this. It sits right between identifying a run of text and shaping it.

Here is the implementation plan:

1.  **Segment by Script:** Before shaping a `VisualItem`, iterate through its text and break it into smaller segments based on script (e.g., a Latin segment, then an Arabic segment, then another Latin segment). The `detect_char_script` function can be used for this.

2.  **Select Font Per Segment:** For each script segment:
    *   **Attempt Primary Font:** First, try to use the font originally specified in the CSS (`style.font_selector`). Check if this font actually has a glyph for a character in the segment (using the `font.has_glyph()` method).
    *   **Trigger Fallback:** If the primary font is missing the glyph, this is our trigger. We must find a fallback font.
    *   **Query for Fallback:** Construct a *new* `FontSelector`. This is the key. This selector should ask `rust-fontconfig` for a font that specifically supports the required script. We can do this by populating the `unicode_ranges` field of the `FontSelector` with the known ranges for that script.
    *   **Load and Shape:** Call `font_provider.load_font()` with this new, targeted `FontSelector`. Use the returned font to shape this specific segment.

3.  **Combine Results:** Concatenate the shaped glyphs from all segments into a single list.

#### Implementation in `layout/src/text3/cache.rs`

Here is the modified `shape_visual_items` function. This change is significant but encapsulates the entire logic in one place.

```rust
// layout/src/text3/cache.rs

// ... (add necessary imports) ...
use crate::text3::script::detect_char_script;

// ...

pub fn shape_visual_items<T: ParsedFontTrait, P: FontProviderTrait<T>>(
    visual_items: &[VisualItem],
    font_provider: &P,
) -> Result<Vec<ShapedItem<T>>, LayoutError> {
    let mut shaped = Vec::new();

    for item in visual_items {
        match &item.logical_source {
            LogicalItem::Text { style, source, .. } => {
                let direction = if item.bidi_level.is_rtl() { Direction::Rtl } else { Direction::Ltr };

                // --- NEW: SCRIPT-AWARE FONT FALLBACK LOGIC ---

                // 1. Load the primary font requested by CSS.
                let primary_font = match font_provider.load_font(&style.font_selector) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("[Font Fallback] Primary font '{}' not found, error: {:?}", style.font_selector.family, e);
                        // If the primary font itself can't be found, we can't proceed.
                        // A more robust system might try a system-default here.
                        continue;
                    }
                };

                // 2. Segment the text run by script.
                let mut current_pos = 0;
                while current_pos < item.text.len() {
                    let first_char = item.text[current_pos..].chars().next().unwrap_or('\u{FFFD}');
                    let segment_script = detect_char_script(first_char).unwrap_or(Script::Latin);

                    // Find the end of the continuous run of this script.
                    let end_of_segment = item.text[current_pos..].char_indices()
                        .find(|(idx, ch)| detect_char_script(*ch).unwrap_or(Script::Latin) != segment_script)
                        .map(|(idx, _)| current_pos + idx)
                        .unwrap_or(item.text.len());
                    
                    let text_segment = &item.text[current_pos..end_of_segment];

                    // 3. Select the appropriate font for this segment.
                    let font_for_segment = if primary_font.has_glyph(first_char as u32) {
                        // The primary font supports this script. Use it.
                        primary_font.shallow_clone()
                    } else {
                        // FALLBACK TRIGGERED: The primary font is missing the glyph.
                        eprintln!("[Font Fallback] Primary font '{}' missing glyph for '{}'. Finding fallback for {:?}.", style.font_selector.family, first_char, segment_script);
                        
                        // Create a new FontSelector to query for a font that supports this script.
                        let fallback_selector = FontSelector {
                            family: "sans-serif".into(), // Generic fallback family
                            weight: style.font_selector.weight,
                            style: style.font_selector.style,
                            unicode_ranges: segment_script.get_unicode_ranges(),
                        };

                        match font_provider.load_font(&fallback_selector) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("[Font Fallback] Could not find any fallback font for {:?}. Error: {:?}", segment_script, e);
                                // Fallback failed. Use the primary font and accept .notdef glyphs.
                                primary_font.shallow_clone()
                            }
                        }
                    };

                    // 4. Shape this segment with the chosen font.
                    let language = script_to_language(segment_script, text_segment);
                    let shaped_clusters = shape_text_correctly(
                        text_segment,
                        segment_script,
                        language,
                        direction,
                        &font_for_segment,
                        style,
                        *source,
                    )?;
                    shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));

                    current_pos = end_of_segment;
                }
            }
            // ... (rest of the match for Object, Tab, etc. remains the same) ...
            _ => { /* ... existing logic for other InlineContent types ... */ }
        }
    }
    Ok(shaped)
}

```

#### `layout/src/text3/script.rs`

We need to add the `get_unicode_ranges` method to the `Script` enum. This provides the necessary information for `fontconfig` to find a font that supports the required script.

```rust
// layout/src/text3/script.rs

use hyphenation::Language;
use rust_fontconfig::UnicodeRange;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Script {
    // ... (existing variants) ...
}

impl Script {
    /// Maps a Script to a vector of its representative Unicode character ranges.
    pub fn get_unicode_ranges(&self) -> Vec<UnicodeRange> {
        match self {
            Script::Arabic => vec![
                UnicodeRange { start: 0x0600, end: 0x06FF }, // Arabic
                UnicodeRange { start: 0x0750, end: 0x077F }, // Arabic Supplement
                UnicodeRange { start: 0x08A0, end: 0x08FF }, // Arabic Extended-A
            ],
            Script::Hebrew => vec![
                UnicodeRange { start: 0x0590, end: 0x05FF }, // Hebrew
            ],
            Script::Cyrillic => vec![
                UnicodeRange { start: 0x0400, end: 0x04FF }, // Cyrillic
                UnicodeRange { start: 0x0500, end: 0x052F }, // Cyrillic Supplement
            ],
            // Add other scripts as needed...
            _ => Vec::new(), // For Latin, etc., we don't need to specify ranges as it's the default.
        }
    }
}

// ... (rest of the file remains the same) ...
```

By implementing this logic, you fix the font fallback issue at its architectural root. The layout engine is no longer blind to the content it's shaping; it proactively segments text by script and ensures that a capable font is used for each segment.


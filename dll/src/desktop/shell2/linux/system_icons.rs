//! The `"system"` icon pack: the DESKTOP's own icons, loaded at startup.
//!
//! A freedesktop icon theme is a directory of SVGs on disk
//! (`/usr/share/icons/<theme>/actions/16/arrow-right.svg`), and the theme the
//! session actually uses is already discovered
//! (`SystemStyle::linux.icon_theme`). Loading a handful of them means a
//! drop-down's arrow, a submenu's indicator and a tree's expander are the same
//! shapes Qt and GTK draw, instead of the hardcoded glyphs azul reached for
//! (`"▶"` in the menu renderer, which is a different weight, size and baseline
//! from every native one).
//!
//! Breeze's icons are written to be recoloured — the paths carry
//! `fill="currentColor"` — so ONE asset serves light and dark, tinted with the
//! palette we already detected. `currentColor` is substituted textually before
//! parsing, because it is a CSS cascade value and there is no cascade here.
//!
//! Registered under the pack name `system`, addressable either bare
//! (`arrow-right`, first-match-wins across packs) or explicitly
//! (`system:arrow-right`) when a name would otherwise collide with an app's
//! own pack.

use azul_core::icon::IconProviderHandle;
use azul_css::props::basic::color::ColorU;
use azul_css::system::SystemStyle;

/// The icons worth having: every one is a shape a widget draws today.
///
/// Kept small on purpose — this runs on the startup path, and each entry is a
/// file probe plus an SVG rasterisation.
const WANTED: &[&str] = &[
    // Indicators: submenu arrows, drop-down chevrons, tree expanders,
    // spinner steppers. Four shapes cover all of them.
    "arrow-up",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    // A menu's checked mark and a checkbox's tick.
    "checkmark",
    "dialog-ok",
    // Close affordances: a toast's dismiss, an alert's ✕, a CSD close button.
    "window-close",
    "dialog-close",
    // Navigation, for back/forward affordances.
    "go-previous",
    "go-next",
    // Steppers on a number input.
    "list-add",
    "list-remove",
    // A search field's magnifier and a hamburger.
    "edit-find",
    "application-menu",
    // CSD window controls — the most-looked-at icons in the whole window.
    "window-minimize",
    "window-maximize",
    "window-restore",
];

/// Where freedesktop themes live, most specific first.
/// The size a `scalable` icon is drawn at when the theme states none.
///
/// freedesktop names its directories by NOMINAL size (`actions/16`), and that
/// number is the size the icon was DESIGNED for - it is what the desktop draws
/// it at, and what this app must lay it out at. `scalable` has no number, so
/// an indicator-sized default stands in.
const SCALABLE_NOMINAL_PX: u32 = 16;

/// Every directory an icon may live in, paired with its NOMINAL size.
fn icon_theme_dirs(theme: &str) -> Vec<(String, u32)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    for root in [
        alloc::format!("{home}/.local/share/icons"),
        alloc::format!("{home}/.icons"),
        "/usr/share/icons".to_string(),
        "/usr/local/share/icons".to_string(),
    ] {
        // The sizes a UI indicator is drawn at. 16 first: these are pixel-hinted
        // designs, and the 16px variant is the one drawn beside 10-11pt text.
        //
        // BOTH directory orders, and the symbolic/scalable trees, because real
        // themes disagree and we only read SVG. Breeze puts SVGs in
        // `actions/16`; Mint-Y's `actions/{16,22,24}` are PNG-ONLY and its
        // only SVGs are in `actions/symbolic`; Adwaita inverts the nesting
        // entirely (`symbolic/actions`, `scalable/actions`, `16x16/actions`).
        // Looking only in `{theme}/actions/{16,22,24,scalable}` found nothing
        // at all in a Mint session, so every system icon silently fell back to
        // the app's built-ins - at a different nominal size, which is what
        // "the icons are scaled 2x" looks like.
        for (size, nominal) in [
            ("16", 16),
            ("16x16", 16),
            ("22", 22),
            ("22x22", 22),
            ("24", 24),
            ("24x24", 24),
            ("symbolic", SCALABLE_NOMINAL_PX),
            ("scalable", SCALABLE_NOMINAL_PX),
        ] {
            out.push((alloc::format!("{root}/{theme}/actions/{size}"), nominal));
            out.push((alloc::format!("{root}/{theme}/{size}/actions"), nominal));
        }
    }
    out
}

/// Read one named icon out of the theme, following the theme's `Inherits`
/// chain one level (Breeze Dark inherits Breeze, and only some icons are
/// overridden in the dark variant).
/// Returns the SVG source and the NOMINAL size of the directory it came from
/// - the size the desktop draws this icon at. The bitmap is rasterised larger
/// than that for crispness, so the nominal size has to travel with it or the
/// icon lays out at its oversampled bitmap size (see
/// `azul_layout::icon::register_image_icon_sized`).
/// What OTHER desktops call the same indicator.
///
/// [`WANTED`] uses the freedesktop/Breeze spellings, and GNOME-lineage themes
/// - which is what a Mint, Ubuntu or Fedora desktop actually ships - name half
/// of them differently. On Mint 22.2 that cost 7 of 17 icons: `arrow-up` does
/// not exist anywhere in the Mint-Y -> Adwaita chain, `go-up` does; `checkmark`
/// does not, `object-select` does; `application-menu` does not, `open-menu`
/// does. Each list is tried in order after the canonical name.
fn icon_name_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "arrow-up" => &["go-up", "pan-up"],
        "arrow-down" => &["go-down", "pan-down"],
        "arrow-left" => &["go-previous", "pan-start", "pan-left"],
        "arrow-right" => &["go-next", "pan-end", "pan-right"],
        "checkmark" => &["object-select", "emblem-ok", "checkbox-checked"],
        "dialog-ok" => &["emblem-ok", "object-select", "gtk-ok"],
        "dialog-close" => &["window-close", "gtk-close"],
        "application-menu" => &["open-menu", "view-more", "gtk-menu"],
        "edit-find" => &["system-search"],
        "window-restore" => &["view-restore"],
        _ => &[],
    }
}

fn read_icon_svg(theme: &str, name: &str) -> Option<(Vec<u8>, u32)> {
    // The WHOLE inheritance chain, not one level. Mint-Y-Sand inherits
    // `Mint-Y,Adwaita,gnome,hicolor` and ships no `actions` icons of its own,
    // so stopping at the first parent stopped exactly one theme short of the
    // one that has the file. Bounded and visited-checked: `Inherits=` is
    // user-editable and can be cyclic.
    // EVERY name on the `Inherits=` line, breadth-first - not just the first.
    // Mint-Y-Sand inherits `Mint-Y,Adwaita,gnome,hicolor`, and the icons its
    // own tree lacks are in Adwaita, the SECOND entry: following only the
    // first left 7 of 17 indicators unresolved.
    let mut chain = alloc::vec![theme.to_string()];
    let mut seen = alloc::vec![theme.to_ascii_lowercase()];
    let mut cursor = 0usize;
    while cursor < chain.len() && chain.len() < 16 {
        let step = chain[cursor].clone();
        cursor += 1;
        for parent in icon_theme_parents(&step) {
            let key = parent.to_ascii_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            chain.push(parent);
        }
    }

    // The canonical name first, everywhere, before any alias: a theme that
    // ships `checkmark` must win over one further down the chain that only
    // has `object-select`.
    for candidate in core::iter::once(name).chain(icon_name_aliases(name).iter().copied()) {
        for step in &chain {
            for (dir, nominal) in icon_theme_dirs(step) {
                // `foo-symbolic.svg` is how the symbolic trees spell `foo`, and
                // a symbolic icon is the one meant for exactly this job: a
                // small, single-colour UI indicator that takes the desktop's
                // foreground.
                for file in [
                    alloc::format!("{dir}/{candidate}.svg"),
                    alloc::format!("{dir}/{candidate}-symbolic.svg"),
                ] {
                    if let Ok(bytes) = std::fs::read(&file) {
                        return Some((bytes, nominal));
                    }
                }
            }
        }
    }
    None
}

/// Every entry of a theme's `Inherits=` line, in order.
fn icon_theme_parents(theme: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for root in [
        alloc::format!("{home}/.local/share/icons"),
        alloc::format!("{home}/.icons"),
        "/usr/share/icons".to_string(),
        "/usr/local/share/icons".to_string(),
    ] {
        let Ok(text) = std::fs::read_to_string(alloc::format!("{root}/{theme}/index.theme")) else {
            continue;
        };
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("Inherits=") {
                return rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(alloc::string::ToString::to_string)
                    .collect();
            }
        }
    }
    Vec::new()
}

/// The first entry of a theme's `Inherits=` line, if it has one.
#[allow(dead_code)]
fn icon_theme_parent(theme: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for root in [
        alloc::format!("{home}/.local/share/icons"),
        "/usr/share/icons".to_string(),
    ] {
        let Ok(text) = std::fs::read_to_string(alloc::format!("{root}/{theme}/index.theme")) else {
            continue;
        };
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("Inherits=") {
                if let Some(first) = rest.split(',').next() {
                    let first = first.trim();
                    if !first.is_empty() && first != theme {
                        return Some(first.to_string());
                    }
                }
            }
        }
    }
    None
}

/// The recursion limit for reference resolution.
///
/// An icon that references itself (`<use href="#a">` inside `#a`) is malformed
/// but perfectly possible, and this code reads FILES FROM DISK that no build
/// of ours produced. A bound is not defensive programming here, it is the only
/// thing between a broken theme and a hung startup.
const MAX_REFERENCE_DEPTH: usize = 8;

/// Pre-resolve everything an icon points at, so the markup that reaches the
/// DOM stands on its own.
///
/// A freedesktop icon is not a flat drawing. It carries an embedded
/// stylesheet (`.ColorScheme-Text { color:#232629; }`), its paths take their
/// paint from it indirectly (`style="fill:currentColor"` + a
/// `class="ColorScheme-Text"`), and it may pull shapes in through
/// `<use href="#id">`. None of those resolve by themselves here: the SVG-to-DOM
/// path builds nodes, not a cascade, so a `currentColor` that is never
/// substituted falls through to SVG's default of opaque BLACK - which is
/// exactly how every themed icon came out black on a dark panel.
///
/// So the references are resolved FIRST, in the source:
///
/// * `.ColorScheme-Text` is repointed at the palette tint, while
///   `NegativeText` / `PositiveText` / `NeutralText` keep the colours their
///   author chose (a close button is red on purpose);
/// * every element's `currentColor` becomes the colour ITS class names, not a
///   single global substitution - that is what kept the negative red;
/// * `<use href="#id">` is replaced by the element it names, recursively, to
///   [`MAX_REFERENCE_DEPTH`].
///
/// `url(#gradient)` references are deliberately LEFT ALONE: the definitions
/// travel with the document, and the DOM renderer resolves them properly
/// (`SvgNodeData::{LinearGradient, RadialGradient, GradientStop}`) - flattening
/// them here would throw away the one thing that path does better.
fn resolve_svg_references(svg: &[u8], tint: ColorU) -> Vec<u8> {
    let Ok(text) = core::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    let hex = hex_of(tint);
    let classes = color_scheme_classes(text, &hex);
    let inlined = inline_use_elements(text, MAX_REFERENCE_DEPTH);
    let recoloured = rewrite_color_scheme_rules(&inlined, &classes);
    resolve_current_color(&recoloured, &classes, &hex).into_bytes()
}

/// Turn the icon's `.ColorScheme-* { color: … }` rules into `fill` rules, with
/// `ColorScheme-Text` repointed at the palette tint.
///
/// The rules are KEPT as rules rather than folded into each element, and that
/// is the point: the fill then comes from the CASCADE, so a `:hover` or
/// `:focus` rule - the icon's own, or one a widget adds around it - simply
/// works. Baking the colour into every element's inline style made the glyph
/// a fixed picture that nothing downstream could restyle, which is why a
/// desktop's close button could never turn red on hover.
///
/// `color` becomes `fill` because that is the property an SVG shape paints
/// with here (`fill` is an accepted spelling of `background-color`); the
/// authored `color` would set text colour and paint nothing.
fn rewrite_color_scheme_rules(text: &str, classes: &[(String, String)]) -> String {
    let mut out = text.to_string();
    for (name, value) in classes {
        // Rewrite `.<name> { … color: <old> … }` in place. Only the FIRST
        // declaration block per class is touched, which is the only one a
        // freedesktop icon has.
        let Some(class_at) = out.find(&alloc::format!(".{name}")) else {
            continue;
        };
        let Some(brace) = out[class_at..].find('{') else {
            continue;
        };
        let block_start = class_at + brace;
        let Some(color_at) = out[block_start..].find("color:") else {
            continue;
        };
        let decl_start = block_start + color_at;
        let after = &out[decl_start + "color:".len()..];
        let decl_end = decl_start + "color:".len() + after.find(';').unwrap_or(after.len());
        out.replace_range(decl_start..decl_end, &alloc::format!("fill:{value}"));
    }
    out
}

fn hex_of(c: ColorU) -> String {
    alloc::format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// The `.ColorScheme-*` class colours declared in the icon's own `<style>`
/// block, with `ColorScheme-Text` repointed at the palette tint.
///
/// Everything else keeps its authored value: `NegativeText` is the red a
/// close/delete icon is drawn in, and repainting it in the window's text
/// colour would erase the one thing that marks it as destructive.
fn color_scheme_classes(text: &str, tint_hex: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(dot) = rest.find(".ColorScheme-") {
        rest = &rest[dot + 1..];
        let Some(brace) = rest.find('{') else { break };
        let name = rest[..brace].trim().to_string();
        let Some(color_at) = rest[brace..].find("color:") else {
            continue;
        };
        let after = &rest[brace + color_at + "color:".len()..];
        let end = after.find(';').unwrap_or(after.len());
        let value = after[..end].trim().to_string();
        let value = if name == "ColorScheme-Text" {
            tint_hex.to_string()
        } else {
            value
        };
        out.push((name, value));
    }
    out
}

/// Replace `currentColor` inside each element with the colour that element's
/// `class` resolves to (falling back to the tint).
///
/// Scoped per element rather than globally: a single document mixes
/// `ColorScheme-Text` paths with `ColorScheme-NegativeText` ones, and one
/// blanket substitution paints the whole icon in the same colour.
fn resolve_current_color(text: &str, classes: &[(String, String)], tint_hex: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let (tag, after) = rest.split_at(end);
        if element_color(tag, classes).is_some() {
            // The element names a class, and that class now carries the fill.
            // Its inline `currentColor` has to GO rather than be substituted:
            // an inline declaration beats a class rule, so leaving it there
            // would pin the colour and defeat every `:hover` rule downstream.
            out.push_str(&strip_current_color_declarations(tag));
        } else {
            // No class to inherit from: SVG's `currentColor` has nothing to
            // resolve against here, so the palette tint stands in.
            out.push_str(&tag.replace("currentColor", tint_hex));
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Drop `fill:currentColor` / `fill="currentColor"` (and the stroke forms)
/// from one element's markup, leaving its class rule to supply the paint.
fn strip_current_color_declarations(tag: &str) -> String {
    let mut out = tag.to_string();
    for property in ["fill", "stroke"] {
        for spelling in [
            alloc::format!("{property}:currentColor;"),
            alloc::format!("{property}:currentColor"),
            alloc::format!("{property}=\"currentColor\""),
            alloc::format!("{property}='currentColor'"),
        ] {
            out = out.replace(&spelling, "");
        }
    }
    out
}

/// The colour one element's `class` attribute resolves to.
fn element_color(tag: &str, classes: &[(String, String)]) -> Option<String> {
    let class_at = tag.find("class=")?;
    let after = &tag[class_at + "class=".len()..];
    let quote = after.chars().next()?;
    let rest = after.get(1..)?;
    let end = rest.find(quote)?;
    let names = &rest[..end];
    names
        .split_whitespace()
        .find_map(|n| classes.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone()))
}

/// Replace `<use href="#id">` with the element it names, to `depth` levels.
///
/// The DOM path models `<use>` (`SvgNodeData::Use`) but resolving it needs the
/// document, which the icon renderer does not hand it - so the reference is
/// pre-resolved here, where the whole file is in scope.
fn inline_use_elements(text: &str, depth: usize) -> String {
    if depth == 0 || !text.contains("<use") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut substituted = false;
    while let Some(at) = rest.find("<use") {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let (tag, after) = rest.split_at(end);
        match referenced_id(tag).and_then(|id| element_with_id(text, &id)) {
            Some(target) => {
                out.push_str(&target);
                substituted = true;
            }
            // A dangling reference: drop the `<use>` rather than emit markup
            // pointing at nothing.
            None => {}
        }
        rest = after;
    }
    out.push_str(rest);
    if substituted {
        // The injected element may itself contain a `<use>`.
        return inline_use_elements(&out, depth - 1);
    }
    out
}

/// The `#id` a `<use>` tag points at, from `href` or `xlink:href`.
fn referenced_id(tag: &str) -> Option<String> {
    for key in ["xlink:href=", "href="] {
        let Some(at) = tag.find(key) else { continue };
        let after = &tag[at + key.len()..];
        let quote = after.chars().next()?;
        let rest = after.get(1..)?;
        let end = rest.find(quote)?;
        if let Some(id) = rest[..end].strip_prefix('#') {
            return Some(id.to_string());
        }
    }
    None
}

/// The full source of the element carrying `id`, children included.
///
/// A depth counter over `<`/`>` rather than a parse: the input is already
/// well-formed XML by the time it gets here (the theme ships it, and a
/// malformed file simply fails to parse later), and this runs once per icon at
/// startup.
fn element_with_id(text: &str, id: &str) -> Option<String> {
    let needle = alloc::format!("id=\"{id}\"");
    let at = text.find(&needle)?;
    let start = text[..at].rfind('<')?;
    let rest = &text[start..];
    let first_end = rest.find('>')?;
    // Self-closing: the element IS the tag.
    if rest[..first_end].ends_with('/') {
        return Some(rest[..=first_end].to_string());
    }
    let tag_name: String = rest[1..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == ':' || *c == '-')
        .collect();
    let open = alloc::format!("<{tag_name}");
    let close = alloc::format!("</{tag_name}>");
    let mut depth = 1usize;
    let mut cursor = first_end + 1;
    while depth > 0 {
        let next_open = rest[cursor..].find(&open).map(|i| cursor + i);
        let next_close = rest[cursor..].find(&close).map(|i| cursor + i)?;
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                cursor = o + open.len();
            }
            _ => {
                depth -= 1;
                cursor = next_close + close.len();
            }
        }
    }
    Some(rest[..cursor].to_string())
}

/// Load the desktop's icons into the `"system"` pack.
///
/// Best-effort by design: a session with no icon theme, a theme that ships no
/// SVGs, or a build without the rasteriser simply registers nothing, and every
/// caller keeps the glyph fallback it already had.
pub fn register_system_icons(provider: &mut IconProviderHandle, style: &SystemStyle) {
    let Some(theme) = style.linux.icon_theme.as_option() else {
        return;
    };
    let theme = theme.as_str();
    // The tint: what the desktop paints its own foreground with.
    let tint = style
        .colors
        .text
        .into_option()
        .unwrap_or(ColorU::new_rgb(0, 0, 0));

    let (icons, from_cache) = rendered_icons(theme, tint);
    let mut registered = 0usize;
    for (name, markup) in icons {
        let Some(dom) = icon_dom(&markup) else {
            continue;
        };
        azul_layout::icon::register_dom_icon(provider, "system", &name, dom);
        registered += 1;
    }

    crate::plog_debug!(
        "[system-icons] icon theme '{}': registered {}/{} into the \"system\" pack{}",
        theme,
        registered,
        WANTED.len(),
        if from_cache { " (cached)" } else { "" }
    );
}

/// Re-read the desktop's icons into a LIVE provider after a theme change.
///
/// `register_system_icons` runs once, at `App::create`, against the provider
/// handle the config owns - and the artwork it produces is theme-dependent
/// twice over: KDE ships `breeze` and `breeze-dark` as two directories, and
/// the tint comes from the palette. A session that switched to dark kept
/// serving the light-tinted set, which is dark glyphs on a dark panel.
///
/// Cheap when nothing moved: the render cache is keyed by (theme, tint), so a
/// call that changes neither costs one map lookup and a clone of the handles.
pub(crate) fn refresh_system_icons(
    provider: &azul_core::icon::SharedIconProvider,
    style: &SystemStyle,
) {
    let Some(theme) = style.linux.icon_theme.as_option() else {
        return;
    };
    let tint = style
        .colors
        .text
        .into_option()
        .unwrap_or(ColorU::new_rgb(0, 0, 0));
    let (icons, from_cache) = rendered_icons(theme.as_str(), tint);
    if from_cache {
        // Same theme AND same tint: what is registered is already right, and
        // re-registering would only flush the resolution cache for nothing.
        return;
    }
    let mut count = 0usize;
    for (name, markup) in icons {
        let Some(dom) = icon_dom(&markup) else {
            continue;
        };
        provider.register_icon(
            "system",
            &name,
            azul_core::refany::RefAny::new(azul_layout::icon::DomIconData::new(dom)),
        );
        count += 1;
    }
    crate::plog_debug!(
        "[system-icons] theme changed to '{}': re-registered {} icons",
        theme.as_str(),
        count
    );
}

/// One prepared icon: its name and its self-contained SVG markup.
///
/// MARKUP, not pixels. The icon is spliced into the DOM as a live `<svg>`
/// subtree, so it scales with the display instead of being resampled from a
/// bitmap, and - the reason this changed - CSS reaches it: a `:hover` on the
/// button recolours the glyph, which a rasterised icon can never do.
type PreparedIcon = (String, String);

/// The rendered icon set for a theme, CACHED BY THEME.
///
/// Keyed by `(icon theme, tint)` because both change together and both change
/// the pixels: KDE ships `breeze` and `breeze-dark` as two directories, and a
/// colour-scheme switch repoints `Icons/Theme` at the other one AND moves the
/// foreground colour the icons are tinted with. Without the key, a session
/// that started light kept serving light-tinted icons after the switch -
/// dark glyphs on a dark panel.
///
/// Cached rather than re-read because this is not cheap: a file probe per
/// icon, an XML parse, a layout pass and a rasterisation, times
/// `WANTED.len()`. A switch BACK to the previous theme re-renders rather than
/// serving a stale entry - the cache holds one theme, which is the only one
/// that can be on screen.
fn rendered_icons(theme: &str, tint: ColorU) -> (Vec<PreparedIcon>, bool) {
    use std::sync::Mutex;

    type Key = (String, [u8; 4]);
    static CACHE: Mutex<Option<(Key, Vec<PreparedIcon>)>> = Mutex::new(None);

    let key: Key = (theme.to_string(), [tint.r, tint.g, tint.b, tint.a]);
    let mut guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((cached_key, icons)) = guard.as_ref() {
        if *cached_key == key {
            return (icons.clone(), true);
        }
    }

    // THROUGH THE SAME RESOLUTION as every theme icon, not raw.
    //
    // This glyph is written here rather than read from the theme, and it was
    // pushed straight into the pack while the loop below resolved everything
    // else - so its `.ColorScheme-Text { color: … }` rule was never turned
    // into a `fill`, and its `fill:currentColor` never resolved. It parsed, it
    // laid out, it had a viewBox and a size, and it painted NOTHING: the close
    // button was simply missing from the title bar while minimize and maximize
    // were there.
    let close_glyph = String::from_utf8(resolve_svg_references(
        titlebar_close_glyph(tint).as_bytes(),
        tint,
    ))
    .unwrap_or_else(|_| titlebar_close_glyph(tint));
    let mut out: Vec<PreparedIcon> = vec![("titlebar-close".to_string(), close_glyph)];
    for name in WANTED {
        let Some((svg, _nominal_px)) = read_icon_svg(theme, name) else {
            continue;
        };
        let resolved = resolve_svg_references(&svg, tint);
        let Ok(markup) = String::from_utf8(resolved) else {
            continue;
        };
        out.push(((*name).to_string(), markup));
    }
    *guard = Some((key, out.clone()));
    (out, false)
}

/// The TITLEBAR's close glyph: a plain cross, in the titlebar text colour.
///
/// Not the theme's `window-close`, and that is the point. A freedesktop theme
/// draws `window-close` as a red circled X - correct for the ACTION ("close
/// this document", in a menu or a task manager), wrong for a window control,
/// which is why a titlebar built from it reads as permanently alarmed. KDE
/// does not use a second icon for the hover state either: its window buttons
/// do not come from the icon theme at all (`breezedecoration.so` paints them),
/// as a plain cross in the titlebar's foreground colour with a RED BACKGROUND
/// on hover - the colour already detected as
/// `TitlebarMetrics::close_button_hover_background`.
///
/// So the resting glyph is neutral and the red is the button's hover fill,
/// which is what every desktop actually does. The cross is drawn here rather
/// than lifted out of the theme's file: the artwork in an icon theme is
/// licensed work, and a symmetric X of round numbers is not something to
/// copy.
///
/// It carries `.ColorScheme-Text` like every theme icon, so the same tinting
/// and the same `:hover` restyling apply to it.
fn titlebar_close_glyph(tint: ColorU) -> String {
    alloc::format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
  <defs><style>.ColorScheme-Text {{ color:{}; }}</style></defs>
  <path class="ColorScheme-Text" style="fill:currentColor"
        d="M 4.8,4 L 8,7.2 L 11.2,4 L 12,4.8 L 8.8,8 L 12,11.2 L 11.2,12 L 8,8.8 L 4.8,12 L 4,11.2 L 7.2,8 L 4,4.8 Z"/>
</svg>"##,
        hex_of(tint)
    )
}

/// One theme SVG -> a live DOM subtree.
///
/// NOT a bitmap. The XML parser maps `<path>`, `<use>`, `<linearGradient>` and
/// `<stop>` onto real DOM nodes, an `<svg>` carries its viewBox and intrinsic
/// size, and a shape's `fill`/`stroke` are ordinary CSS clipped to the
/// geometry - so an icon can simply BE part of the document. Three things fall
/// out of that, and all three were wrong with a raster:
///
///   * it scales with the display instead of being resampled from a fixed
///     bitmap, so a 16px glyph is sharp at any DPI and any zoom;
///   * CSS reaches it. A `:hover` on the button recolours the glyph, which is
///     how a desktop's close button turns red - a rasterised icon has its
///     colours baked and can never do it;
///   * there is nothing to cache, invalidate or garbage-collect on the GPU.
fn icon_dom(markup: &str) -> Option<azul_core::dom::Dom> {
    let parsed = azul_layout::xml::parse_xml(markup).ok()?;
    Some(azul_layout::xml::dom_from_parsed_xml(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINT: ColorU = ColorU {
        r: 0x12,
        g: 0x34,
        b: 0x56,
        a: 255,
    };

    fn resolved(svg: &str) -> String {
        String::from_utf8(resolve_svg_references(svg.as_bytes(), TINT)).unwrap()
    }

    /// Breeze's icons are authored for recolouring - the paths say
    /// `fill="currentColor"` and the stylesheet sets `.ColorScheme-Text`.
    /// Neither resolves by itself here, so the reference is resolved in the
    /// SOURCE - but into the RULE, not onto the element.
    ///
    /// That is the difference between a glyph that can be restyled and a
    /// fixed picture: an inline declaration beats a class rule, so a baked
    /// `fill` would pin the colour and defeat every `:hover` a widget adds.
    #[test]
    fn the_tint_lands_in_the_class_rule_not_on_the_element() {
        let text = resolved(
            r#"<svg><style>.ColorScheme-Text { color:#eff0f1; }</style>
            <path d="M0 0" class="ColorScheme-Text" fill="currentColor"/></svg>"#,
        );
        assert!(
            text.contains("fill:#123456"),
            "the class rule carries the tint, as a FILL: {text}"
        );
        assert!(
            text.contains(r#"class="ColorScheme-Text""#),
            "and the element keeps the class that selects it: {text}"
        );
        assert!(
            !text.contains("currentColor"),
            "no unresolved currentColor may survive: {text}"
        );
        assert!(
            !text.contains(r##"fill="#123456""##),
            "the colour must NOT be baked onto the element - that is what \
             makes it unrestylable: {text}"
        );
    }

    /// THE bug a single global substitution causes: one document mixes
    /// `ColorScheme-Text` with `ColorScheme-NegativeText`, and a close icon is
    /// red ON PURPOSE. Each class keeps its own colour.
    #[test]
    fn each_class_keeps_its_own_colour() {
        let text = resolved(
            r#"<svg><defs><style>
                .ColorScheme-Text { color:#eff0f1; }
                .ColorScheme-NegativeText { color:#da4453; }
            </style></defs>
            <path id="a" class="ColorScheme-Text" style="fill:currentColor"/>
            <path id="b" class="ColorScheme-NegativeText" style="fill:currentColor"/>
            </svg>"#,
        );
        assert!(
            text.contains("fill:#123456"),
            "the Text class takes the tint: {text}"
        );
        assert!(
            text.contains("fill:#da4453"),
            "the NegativeText class keeps its authored red: {text}"
        );
        // Both elements keep the class that selects them.
        assert!(text.contains(r#"class="ColorScheme-Text""#));
        assert!(text.contains(r#"class="ColorScheme-NegativeText""#));
    }

    /// An element with no class still has to resolve to SOMETHING    /// An element with no class still has to resolve to SOMETHING - the SVG
    /// default is opaque black, which is invisible on a dark panel.
    #[test]
    fn an_unclassed_element_falls_back_to_the_tint() {
        let text = resolved(r#"<svg><path style="fill:currentColor"/></svg>"#);
        assert!(text.contains("#123456"), "{text}");
    }

    /// `<use href="#id">` is pre-resolved: the DOM path models the node but
    /// has no document to look the target up in.
    #[test]
    fn a_use_element_is_replaced_by_what_it_points_at() {
        let text = resolved(
            r##"<svg><defs><path id="shape" d="M1 2"/></defs><use href="#shape"/></svg>"##,
        );
        assert_eq!(
            text.matches("M1 2").count(),
            2,
            "the referenced path must be injected at the use site: {text}"
        );
        assert!(!text.contains("<use"), "no unresolved <use> may survive");
    }

    /// A `<use>` pointing at nothing must not leave markup addressing a
    /// missing id, and must not loop.
    #[test]
    fn a_dangling_or_self_referential_use_terminates() {
        let dangling = resolved(r##"<svg><use href="#nope"/></svg>"##);
        assert!(!dangling.contains("<use"), "{dangling}");

        // Self-reference: bounded by MAX_REFERENCE_DEPTH rather than hanging.
        let cyclic = resolved(r##"<svg><g id="a"><use href="#a"/></g></svg>"##);
        assert!(cyclic.contains("<svg"), "must still return markup");
    }

    /// Gradients are LEFT ALONE on purpose: the definitions travel with the
    /// document and the DOM renderer resolves them properly. Rewriting them
    /// here would throw away the one thing that path does better.
    #[test]
    fn gradient_references_are_left_for_the_dom_renderer() {
        let text = resolved(
            r##"<svg><defs><linearGradient id="g"><stop stop-color="#f00"/></linearGradient></defs>
            <rect fill="url(#g)"/></svg>"##,
        );
        assert!(text.contains("url(#g)"), "the reference survives: {text}");
        assert!(
            text.contains("linearGradient"),
            "and so does its definition: {text}"
        );
    }

    /// The whole point: a theme SVG becomes a live DOM subtree with real
    /// shape nodes - not a bitmap, and not a stub.
    ///
    /// Asserted on the STRUCTURE rather than on pixels, because the structure
    /// is what makes the rest possible: shape nodes are what the renderer
    /// paints, what CSS can restyle on `:hover`, and what scales with the
    /// display.
    #[test]
    fn a_theme_svg_becomes_a_dom_with_shape_nodes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
          <defs><style>.ColorScheme-Text { color:#232629; }</style></defs>
          <path style="fill:currentColor" class="ColorScheme-Text"
                d="M 2,2 L 14,2 L 14,14 L 2,14 Z"/>
        </svg>"##;
        let resolved = resolve_svg_references(svg, ColorU::new_rgb(0xff, 0x00, 0x00));
        let markup = String::from_utf8(resolved).expect("utf8");
        let dom = icon_dom(&markup).expect("a well-formed icon parses");

        fn walk(node: &azul_core::dom::Dom, svgs: &mut usize, paths: &mut usize) {
            match node.root.get_node_type() {
                azul_core::dom::NodeType::Svg => *svgs += 1,
                azul_core::dom::NodeType::SvgPath => *paths += 1,
                _ => {}
            }
            for child in node.children.as_ref() {
                walk(child, svgs, paths);
            }
        }
        let (mut svgs, mut paths) = (0, 0);
        walk(&dom, &mut svgs, &mut paths);
        assert_eq!(svgs, 1, "the <svg> survives as a node");
        assert_eq!(paths, 1, "and so does its shape");
    }

    /// The titlebar's close glyph is a NEUTRAL cross, not the theme's red
    /// circled X.
    ///
    /// `window-close` in an icon theme is the "close this document" ACTION and
    /// is red at rest; a titlebar built from it reads as permanently alarmed.
    /// The desktop's own titlebar draws a plain cross in the foreground colour
    /// and supplies the red as a HOVER FILL, which is what the button style
    /// does - so the glyph must carry the tint and nothing else.
    #[test]
    fn the_titlebar_close_glyph_is_neutral_and_takes_the_tint() {
        let glyph = titlebar_close_glyph(TINT);
        assert!(
            glyph.contains("#123456"),
            "the glyph takes the palette tint: {glyph}"
        );
        assert!(
            !glyph.to_ascii_lowercase().contains("da4453"),
            "and carries NO red of its own - the red is the button's hover \
             fill: {glyph}"
        );
        assert!(
            glyph.contains("ColorScheme-Text"),
            "it is classed like every theme icon, so the same tinting and the \
             same :hover restyling reach it: {glyph}"
        );
        // ... and the icon that is actually REGISTERED paints.
        //
        // This used to resolve `glyph` by hand and assert the result parsed -
        // which is not what the pack contained. The pack got the RAW glyph,
        // `currentColor` never resolved, and the close button painted nothing
        // while this test stayed green. Read the registered markup instead, so
        // the premise is the shipped one.
        let (icons, _) = rendered_icons("definitely-not-a-real-theme", TINT);
        let (_, registered) = icons
            .iter()
            .find(|(name, _)| name == "titlebar-close")
            .expect("the glyph is registered");
        assert!(
            !registered.contains("currentColor"),
            "the registered glyph must carry a resolved fill, not an unresolved \
             `currentColor` that paints nothing: {registered}"
        );
        assert!(
            registered.contains("fill:#123456") || registered.contains("fill=\"#123456\""),
            "and that fill is the palette tint: {registered}"
        );
        assert!(icon_dom(registered).is_some(), "the glyph must parse");
    }

    /// It is REGISTERED, under a name of its own - the theme's action icon
    /// stays reachable for the decorative uses that want it.
    #[test]
    fn the_titlebar_glyph_is_registered_beside_the_theme_icons() {
        let (icons, _) = rendered_icons("definitely-not-a-real-theme", TINT);
        assert!(
            icons.iter().any(|(name, _)| name == "titlebar-close"),
            "even a session with no icon theme at all gets the window control"
        );
        assert!(
            !icons.iter().any(|(name, _)| name == "window-close"),
            "while the theme's own icons are absent, as there is no theme"
        );
    }

    /// Non-UTF8 bytes are not an SVG we can rewrite; hand them back untouched
    /// rather than panicking on the startup path.
    #[test]
    fn invalid_utf8_is_passed_through() {
        let bytes = [0xff, 0xfe, 0x00];
        assert_eq!(
            resolve_svg_references(&bytes, ColorU::new_rgb(1, 2, 3)),
            bytes.to_vec()
        );
    }

    /// The search covers the per-size directories a freedesktop theme uses,
    /// user themes before system ones — a user override must win.
    #[test]
    fn the_search_path_prefers_user_themes_and_covers_the_size_dirs() {
        let dirs = icon_theme_dirs("breeze-dark");
        assert!(dirs
            .iter()
            .any(|(d, _)| d.ends_with("/breeze-dark/actions/16")));
        assert!(dirs
            .iter()
            .any(|(d, _)| d.ends_with("/breeze-dark/actions/scalable")));
        let first_system = dirs.iter().position(|(d, _)| d.starts_with("/usr/share"));
        let first_user = dirs.iter().position(|(d, _)| d.contains("/.local/share"));
        if let (Some(sys), Some(user)) = (first_system, first_user) {
            assert!(user < sys, "a user-installed theme must be searched first");
        }
    }

    /// Each directory carries the NOMINAL size freedesktop names it by - the
    /// size the desktop draws that icon at, which has to survive the trip to
    /// the icon provider or the icon lays out at its oversampled bitmap size.
    #[test]
    fn every_size_directory_reports_the_size_it_is_named_after() {
        for (dir, nominal) in icon_theme_dirs("breeze") {
            // Both nestings are searched - `{theme}/actions/16` (Breeze) and
            // `{theme}/16x16/actions` (Adwaita) - so the SIZE segment is
            // whichever of the last two is not the category.
            let mut tail = dir.rsplit('/');
            let last = tail.next().unwrap_or("");
            let prev = tail.next().unwrap_or("");
            let size_seg = if last == "actions" { prev } else { last };
            match size_seg {
                "scalable" | "symbolic" => assert_eq!(
                    nominal, SCALABLE_NOMINAL_PX,
                    "a scalable/symbolic icon has no stated size; the indicator default stands in"
                ),
                px => assert_eq!(
                    px.trim_end_matches(|c: char| !c.is_ascii_digit())
                        .split('x')
                        .next()
                        .and_then(|n| n.parse::<u32>().ok()),
                    Some(nominal),
                    "{dir} must report {px}, not {nominal}"
                ),
            }
        }
    }

    /// Every indicator [`WANTED`] asks for must be reachable under SOME name a
    /// GNOME-lineage theme actually ships, or it silently falls back to the
    /// app's own icon at a different size.
    #[test]
    fn the_gnome_spellings_are_reachable_for_every_breeze_name() {
        for name in ["arrow-up", "arrow-down", "checkmark", "application-menu", "dialog-close"] {
            assert!(
                !icon_name_aliases(name).is_empty(),
                "{name} is a Breeze spelling with no GNOME alias - it will not resolve on Mint, \
                 Ubuntu or Fedora"
            );
        }
        // The aliases are real freedesktop names, not invented ones.
        assert!(icon_name_aliases("arrow-up").contains(&"go-up"));
        assert!(icon_name_aliases("checkmark").contains(&"object-select"));
        assert!(icon_name_aliases("application-menu").contains(&"open-menu"));
    }

    /// The lookup only reads SVG, and themes disagree wildly about where SVGs
    /// live. Mint-Y's `actions/{16,22,24}` are PNG-only and its SVGs are in
    /// `actions/symbolic`; Adwaita nests the other way round. Searching one
    /// layout found nothing on a Mint desktop.
    #[test]
    fn the_search_covers_both_directory_nestings_and_the_symbolic_trees() {
        let dirs: Vec<String> = icon_theme_dirs("Mint-Y").into_iter().map(|(d, _)| d).collect();
        let has = |suffix: &str| dirs.iter().any(|d| d.ends_with(suffix));
        assert!(has("/Mint-Y/actions/16"), "Breeze-style sized actions dir");
        assert!(has("/Mint-Y/actions/symbolic"), "Mint-Y keeps its SVGs here");
        assert!(has("/Mint-Y/symbolic/actions"), "Adwaita-style nesting");
        assert!(has("/Mint-Y/scalable/actions"), "Adwaita-style scalable");
        assert!(has("/Mint-Y/16x16/actions"), "Adwaita-style size dir");
    }
}

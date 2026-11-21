# Native widget look: reference specs and the azul gap (2026-09-28)

**The request (user, verbatim):** "the "slider" doesn't look native at all on macOS - research how different "native" elements look and gather reference images for all operating systems and versions and so on."

The same message said three more things:
- the Spinner "should be more like the macOS spinner with the various rotated dots, or another spinner type like the Microsoft spinner";
- the custom titlebar on macOS "is almost the Windows titlebar height";
- it "doesn't use the "system:ui:bold" font that MacOS would use, isn't centered and we should be able to configure the background + border-bottom".

**Scope.** Research only. No source file was edited and nothing was compiled. This file is the only tracked output.

**Reference images.** They are in `target/native-reference/`, which is untracked and never committed; the screenshots belong to Apple, Microsoft, GNOME and KDE. Links below are relative to this file (`../target/native-reference/...`). Names follow `<os>-<version>-<widget>-<variant>.<ext>`. There are 239 files, about 14 MB.

## How each figure was obtained

Every number carries one of these tags:

| Tag | Meaning |
|---|---|
| **[L]** | **Measured locally on macOS 15.5 (24F74) through AppKit itself.** A JXA script (`osascript -l JavaScript`) builds real `NSSlider` / `NSProgressIndicator` / `NSWindow` / ... objects in an off-screen window. The window is an `NSWindow` subclass that reports itself key/main/active (`isKeyWindow`, `_hasActiveAppearance`), so controls draw their *active* look. Each control is rendered with `cacheDisplayInRect:toBitmapImageRep:` into an sRGB bitmap at 2x or 4x. Sizes come from `-fittingSize`, `-alignmentRectForFrame:`, `NSSliderCell -barRectFlipped:`/`-knobRectFlipped:`, `-rectOfTickMarkAtIndex:` and `NSWindow -standardWindowButton:` frames. Structure comes from walking the `CALayer` tree and `animationForKey:`. Colours come from `NSAppearance setCurrentAppearance:` + `NSColor colorUsingColorSpace:sRGB`. No input was driven and the user's screen was never captured. Caveat: a vibrancy material (`NSVisualEffectView`) renders its fallback colour off-screen, so titlebar background values are approximate. Transparent pixels were composited over the nominal `windowBackgroundColor` (#ECECEC / #323232). |
| **documented** | Read from a source file or doc page, cited by number: [A*] Apple, [W*] Windows, [G*] GNOME, [K*] KDE, [I*] iOS, [M*] Material. |
| **(measured)** | Pixel-measured from a reference image. |
| **(derived)** | Computed from documented values (colour mixing, box math). |
| **(unverified)** | Recalled or snippet-only; treat as a lead, not a fact. |

Web research ran as four parallel research passes (macOS, Windows, GNOME+KDE, iOS+Android). The local AppKit probe is authoritative for macOS 15. Where the web and [L] disagree, [L] wins, and the disagreement is noted.

---

## 0. The three complaints, answered

1. **Slider.** azul's slider is a 200x16 grey pill (#cccccc, radius 8) with a 16x16 solid-blue dot *inside* it. No native slider looks like that.
   - Every platform draws a **thin track** (macOS 4pt, Win11 4px, GNOME 4px, KDE 6px, iOS 4pt, M3 16dp) and a **filled accent segment** from the minimum to the knob.
   - The knob is **larger than the track and sits on top of it**: macOS 15 is a white 20pt circle with a hairline and a soft shadow; Win11 is a white 22px disc with an accent dot inside; GNOME is a white 20px circle; macOS 26 is a white 20x16 lozenge.
   - azul has neither the fill segment nor the white knob, the border or the shadow. Its track is 4x too thick, and the accent sits on the knob instead of on the track.
2. **Spinner.** azul's spinner is a *static* bordered ring. Its doc comment says azul has no CSS animation; that is **stale**, because `@keyframes`, `animation: ... infinite` and `transform: rotate()` now exist.
   - **macOS 11-15 [L]:** **8 capsule spokes**, 4pt wide at 32pt, running from r = 6.5 to r = 16. Colour is label black (or white in dark mode) with opacity ramping from **0.55 at the head down to 0.06**. The head moves **clockwise, one revolution per 0.8 s**, as a 24-frame sprite (30 fps). It is spokes, not dots; the old 12-spoke spinner is pre-Big Sur.
   - **Windows 11:** one accent arc, stroke = 0.094 x size, that grows to 180° and shrinks again over a **2 s** loop while the whole ring spins about 450°/s.
   - **GNOME (AdwSpinner):** a 15% track circle plus a 100% arc, 1.2 s per turn.
3. **Titlebar.** macOS 15's plain titlebar is **28pt** [L]: traffic lights are 12pt circles whose left edges sit at x = 8, 28 and 48, vertically centred; the title is centred in the *window*, in `NSFont.titleBarFont` = SF **13pt Bold** (weight 0.4 = `NSFontWeightBold`) [L].
   - The demo's hand-rolled bar is 38px, left-aligned, padded 82px, and uses `font-weight: bold` on `system:ui`. 38 is the **unified-compact *toolbar* height** [L]. With a 38px bar on a 28pt titlebar window, AppKit keeps the traffic lights centred on y = 14 while azul centres its text on y = 19.
   - The Titlebar widget itself defaults to 28px on macOS, but:
     - `system:title:bold` / `system:ui:bold` resolve to **Helvetica Neue Bold** on macOS, not SF Pro. `css/src/system.rs` `macos_fallback_chain` skips "System Font" for bold weights because fontconfig has no bold instance of the variable `SFNS.ttf`.
     - `TitlebarMetrics::macos()` says weight 600; native is 700.
     - The text is vertically centred by `padding-top = (28-13)/2`, which lands it about 1-2px low.
     - The widget has **no border-bottom / separator field**, and macOS discovery fills no titlebar background.
     - Details and fixes are in section 5.3.

---

## 1. Summary: widget x platform, with reference images

Cells list the headline shape and size, then the image(s). macOS 15 images marked [L] are genuine AppKit renders; everything else is copied from vendor docs.

| Widget | macOS 13-15 | macOS 26 Tahoe | Windows 11 (10) | GNOME 45-49 | KDE Plasma 6 | iOS 17/18 -> 26 | Android M3 |
|---|---|---|---|---|---|---|---|
| **Slider** | 4pt track, accent fill, 20pt white knob; with ticks a vertical capsule knob and no fill. [L] [light](../target/native-reference/macos-15-slider-light.png), [dark](../target/native-reference/macos-15-slider-dark.png), [inactive](../target/native-reference/macos-15-slider-light-inactive.png) | 6pt track, 20x16 white lozenge, dot ticks below. [light](../target/native-reference/macos-26-slider-light.png), [ticks](../target/native-reference/macos-26-slider-ticks-light.png), [dark](../target/native-reference/macos-26-slider-dark.png), [circular](../target/native-reference/macos-26-slider-circular-light.png) | 4px track r2, 22px white disc with 12/14/10px accent dot. [light](../target/native-reference/windows-11-slider-light.png), [ticks](../target/native-reference/windows-11-slider-ticks-light.png), [controls L](../target/native-reference/windows-11-controls-light.png)/[D](../target/native-reference/windows-11-controls-dark.png). Win10: 2px track, 8x24 rectangle thumb | 4px pill trough, 20px white knob with shadow. [light](../target/native-reference/gnome-47-slider-hig-light.png), [dark](../target/native-reference/gnome-47-slider-hig-dark.png), [gtk4](../target/native-reference/gnome-gtk4-slider-docs-light.png) | 6px groove r3, 18px circle handle, 8px ticks. [hig](../target/native-reference/kde-plasma6-slider-spinbox-hig.png) | 17/18: 4pt track, 28pt white circle with a soft shadow. [17 L](../target/native-reference/ios-17-slider-light.png)/[D](../target/native-reference/ios-17-slider-dark.png). 26: 6pt track, **37x24 white capsule**, glass lens while dragging, tick dots. [26 L](../target/native-reference/ios-26-slider-light.png)/[D](../target/native-reference/ios-26-slider-dark.png), [ticks](../target/native-reference/ios-26-slider-ticks-light.png) | 16dp track, **4x44dp bar handle** with 6dp gaps, stop dot. [anatomy](../target/native-reference/android-m3-slider-anatomy.png), [stop](../target/native-reference/android-m3-slider-stopindicator.png), [centered](../target/native-reference/android-m3-slider-centered.png). Legacy: 4dp track, 20dp circle |
| **Spinner** | 8 capsule spokes, 0.55->0.06 ramp, 0.8 s/rev. [L] [GIF light](../target/native-reference/macos-15-spinner-regular-light.gif), [GIF dark](../target/native-reference/macos-15-spinner-regular-dark.gif), [sizes](../target/native-reference/macos-15-spinner-sizes-static-light.png), [24-frame sheet](../target/native-reference/macos-15-spinner-spritesheet-24frames-light.png), [HIG 14](../target/native-reference/macos-14-spinner-hig-light.png) | same 8 spokes (kit) | ProgressRing arc, 2 s loop. [GIF](../target/native-reference/windows-11-progressring-indeterminate.gif), [frames](../target/native-reference/windows-11-progressring-indeterminate-framestrip.png). Win10: 5-6 orbiting dots (no image) | AdwSpinner arc on a 15% track, 1.2 s/rev. [light](../target/native-reference/gnome-49-spinner-adw-light.png), [dark](../target/native-reference/gnome-49-spinner-adw-dark.png), [HIG](../target/native-reference/gnome-47-spinner-hig-light.png) | rotating `process-working` gear icon, 2 s/rev. [svg](../target/native-reference/kde-plasma6-busyindicator-process-working-symbolic-16.svg), [plasma ring](../target/native-reference/kde-plasma6-busyindicator-plasma-busywidget.svg) | UIActivityIndicatorView: **8 spokes**, 20pt (medium) / 37pt (large), 1 s per revolution in 8 steps. No local image; see [HIG refresh control](https://developer.apple.com/tutorials/images/com.apple.HIG/refresh-controls@2x.png) | arc, 4dp stroke, 40dp, 6 s cycle; Expressive LoadingIndicator = 7 morphing shapes, 48dp. [circular](../target/native-reference/android-m3-circularprogress-indeterminate.gif), [wavy](../target/native-reference/android-m3-circularprogress-wavy-indeterminate.gif), [loading](../target/native-reference/android-m3-loadingindicator.gif), [contained](../target/native-reference/android-m3-loadingindicator-contained.gif) |
| **Titlebar** | 28pt; 12pt lights at a 20pt pitch; centred 13pt Bold title; 0.5pt separator. Unified toolbar 52 / compact 38: title left, 15pt semibold. [L] [std L](../target/native-reference/macos-15-titlebar-standard-light.png)/[D](../target/native-reference/macos-15-titlebar-standard-dark.png)/[inactive](../target/native-reference/macos-15-titlebar-standard-light-inactive.png), [unified](../target/native-reference/macos-15-titlebar-unified-toolbar-light.png), [compact](../target/native-reference/macos-15-titlebar-unified-compact-light.png), [Finder](../target/native-reference/macos-15-titlebar-unified-finder-light.png) | 14pt lights at 23pt (reported); window radius 26 (toolbar) / about 16. [Finder](../target/native-reference/macos-26-titlebar-unified-finder-light.png), [anatomy](../target/native-reference/macos-26-titlebar-toolbar-anatomy-light.png), [window states](../target/native-reference/macos-26-titlebar-window-states-light.png) | 32px (48 tall); 46px caption buttons; left title 12px Segoe UI Variable; close hover #C42B1C. [overview](../target/native-reference/windows-11-titlebar-overview.png), [48px](../target/native-reference/windows-11-titlebar-search.png), [Mica L](../target/native-reference/windows-11-mica-window-light.png)/[D](../target/native-reference/windows-11-mica-window-dark.png), [tabs](../target/native-reference/windows-11-titlebar-tabs.png) | 46px headerbar; bold 11pt centred title; 20/24px round buttons. [49 L](../target/native-reference/gnome-49-headerbar-light.png)/[D](../target/native-reference/gnome-49-headerbar-dark.png), [flat](../target/native-reference/gnome-49-headerbar-flat-light.png), [47](../target/native-reference/gnome-47-headerbar-hig-light.png) | about 28px; Noto Sans 10 regular, centred; #dee0e2 / #292c30. [light](../target/native-reference/kde-plasma6-titlebar-light-125pct.png), [dark](../target/native-reference/kde-plasma6-titlebar-dark-125pct.png) | 44pt nav bar, 17pt semibold centred title, hairline (large title 96). 26: 54pt, no background, 44pt glass buttons. [26 L](../target/native-reference/ios-26-navbar-light.png)/[D](../target/native-reference/ios-26-navbar-dark.png), [with tab bar](../target/native-reference/ios-26-navbar-tabbar-light.png) | top app bar 64dp (small / center-aligned), title-large 22sp; medium 112, large 152. [small](../target/native-reference/android-m3-topappbar-small.png), [anatomy](../target/native-reference/android-m3-topappbar-anatomy.png), [medium](../target/native-reference/android-m3-topappbar-medium-flexible.png), [large](../target/native-reference/android-m3-topappbar-large-flexible.png) |
| Push button | 20pt (16/13/28), r about 5, white face. [L] [light](../target/native-reference/macos-15-button-push-light@4x.png), [dark](../target/native-reference/macos-15-button-push-dark@4x.png), [gallery](../target/native-reference/macos-15-controls-gallery-light.png) | 24 regular / 36 XL, capsule for large+ | 32px r4; elevation border gradient | 34px r6/9, bold | about 34px r4.5 | 34 (bordered) / 50 (large); 26: capsules and glass. [26](../target/native-reference/ios-26-button-light.png), [overview](../target/native-reference/ios-26-controls-overview-light.png) vs [17.5](../target/native-reference/ios-26-controls-overview-ios17baseline-light.png) | 40dp, full radius; Expressive 32/40/56/96/136. [group](../target/native-reference/android-m3-buttongroup-connected.png), [split](../target/native-reference/android-m3-splitbutton-anatomy.png) |
| Checkbox / radio | 14pt box r3 / 14pt circle. [L] [on](../target/native-reference/macos-15-checkbox-on-light@4x.png), [off](../target/native-reference/macos-15-checkbox-off-light@4x.png), [radio](../target/native-reference/macos-15-radio-on-light@4x.png) | 16pt, flat 5% fill. [on](../target/native-reference/macos-26-checkbox-on-light.png), [radio](../target/native-reference/macos-26-radio-on-light.png) | 20px r4 / 20px | 20px (14 + pad) | 16px box r4 | none (switch, or a checkmark in a list row) | 18dp box r2, 2dp stroke / 20dp radio, 10dp dot |
| Switch | 38x22, knob about 20. [L] [on](../target/native-reference/macos-15-switch-on-light@4x.png), [off](../target/native-reference/macos-15-switch-off-light@4x.png) | 44x20 regular (kit) | 40x20, knob 12/14/17 | 46x26, knob 20. [img](../target/native-reference/gnome-47-switch-hig-light.png) | 36x18 (qqc2). [img](../target/native-reference/kde-plasma6-switch-hig.png) | 51x31 -> 26: **63x28** with a 37x24 pill knob. [17](../target/native-reference/ios-17-switch-insetgrouped-light.png), [26 on](../target/native-reference/ios-26-switch-on-light.png)/[off](../target/native-reference/ios-26-switch-off-light.png) | 52x32, knob 16/24/28. [anatomy](../target/native-reference/android-m3-switch-anatomy.png) |
| Text field | 21pt (19/15). [L] [light](../target/native-reference/macos-15-textfield-light@4x.png) | 24 regular, r6 | 32px r4, 2px accent bottom when focused | 34px r6/9, 10% fill | about 30px r4.5 | roundedRect 34pt r5. [17](../target/native-reference/ios-17-textfield-roundedrect-light.png), [26](../target/native-reference/ios-26-textfield-light.png) | 56dp filled (top r4) / outlined |
| Pop-up / dropdown | 20pt with accent chevron square. [L] [img](../target/native-reference/macos-15-popup-button-light@4x.png) | button metrics | 32px, chevron E70D | 34px button + pan-down | about 32px | pop-up / pull-down button + menu. [26 menu](../target/native-reference/ios-26-menu-light.png) | exposed dropdown 56dp, menu items 48dp |
| Segmented | 22pt r about 6, white selected pill. [L] [img](../target/native-reference/macos-15-segmented-light@4x.png), [HIG 14](../target/native-reference/macos-14-segmented-selectone-light.png) | accent selected, capsule for large. [toolbar](../target/native-reference/macos-26-titlebar-segmented-in-toolbar-light.png) | SelectorBar, 3px pill | AdwToggleGroup 34/28 r9/6. [img](../target/native-reference/gnome-49-togglegroup-light.png) | none (checkable tool buttons) | 32pt r9 -> 26 capsule. [17](../target/native-reference/ios-17-segmented-light.png), [26](../target/native-reference/ios-26-segmented-light.png) | segmented buttons 40dp, full radius, check icon |
| Progress bar | 6pt capsule. [L] [img](../target/native-reference/macos-15-progressbar-light@4x.png), [HIG](../target/native-reference/macos-14-progressbar-determinate-light.png) | regular 10 / small 6 | 1px track, 3px bar. [img](../target/native-reference/windows-11-progressbar-determinate.png), [indet.](../target/native-reference/windows-11-progressbar-indeterminate.gif) | 8px pill. [img](../target/native-reference/gnome-47-progressbar-hig-light.png) | 6px r3 | 4pt capsule. [26](../target/native-reference/ios-26-progress-light.png) | 4dp + 4dp gap + stop dot. [anatomy](../target/native-reference/android-m3-progress-anatomy.png), [indet.](../target/native-reference/android-m3-linearprogress-indeterminate.gif), [wavy](../target/native-reference/android-m3-linearprogress-wavy-indeterminate.gif) |
| Tabs / lists / scroll / split | tab strip = segmented; rows 24, inset pill; scroller 15 / overlay 16. [L] [tabs+split](../target/native-reference/macos-15-tabs-table-scroller-split-light.png), [rows](../target/native-reference/macos-15-table-rows-light.png) | sidebar = floating glass | TabView 32 + 8 strip; list 40; scroll 2->6px | AdwTabBar 34; rows 50. [tabs](../target/native-reference/gnome-49-tabbar-light.png), [list](../target/native-reference/gnome-49-boxedlist-light.png) | tabs 30-34 with 3px highlight bar | rows 44 -> 52; inset radius 10 -> 26; tab bar 49 -> floating 62. [row](../target/native-reference/ios-17-list-disclosure-light.png), [26 tab bar](../target/native-reference/ios-26-tabbar-light.png) | rows 56/72/88dp; tabs 48dp with a 3dp indicator |
| Popover / dialog / menu | -- | alert with capsule buttons. [alert](../target/native-reference/macos-26-alert-light.png), [popover](../target/native-reference/macos-26-popover-light.png), [menu](../target/native-reference/macos-26-titlebar-notes-toolbar-menu-light.png) | radius 8, acrylic | popover r12/15. [menu](../target/native-reference/gnome-49-popovermenu-light.png), [alert](../target/native-reference/gnome-49-alertdialog-light.png) | r5 | alert 270pt r14 -> 26: r about 33.5 with capsule buttons. [17](../target/native-reference/ios-17-alert-light.png), [26](../target/native-reference/ios-26-alert-light.png), [menu 17](../target/native-reference/ios-17-contextmenu-light.png), [26 sheet](../target/native-reference/ios-26-sheet-medium-light.png) | dialog r28; menu r4 (Expressive 16), items 48dp |

---

## 2. Slider

### 2.1 macOS 13-15 (`NSSlider`), measured [L] on 15.5

Images: [light](../target/native-reference/macos-15-slider-light.png), [dark](../target/native-reference/macos-15-slider-dark.png), [light, inactive window](../target/native-reference/macos-15-slider-light-inactive.png), [dark, inactive window](../target/native-reference/macos-15-slider-dark-inactive.png).

Each image stacks, top to bottom: regular, small and mini sliders; the same three with 11 tick marks; a disabled slider; and, on the right, a vertical slider and a circular slider.

| Property | Regular | Small | Mini | Notes |
|---|---|---|---|---|
| Frame height (`sizeToFit`) | 28 | 20 | 17 | from `-fittingSize` |
| Track (bar rect) | **4pt**, inset 2pt from each end | 4pt | **3pt** | fully rounded (radius = h/2) |
| Knob hit rect | 24x28 | 20x20 | 17x17 | `knobRectFlipped:` |
| Visible knob | **white circle, 19pt fill + 0.5pt hairline, about 20pt** | about 16pt | about 13pt | [L] pixels |
| Knob hairline | #C7C7C7 on light (black at about 15%) | | | |
| Knob shadow | about 0.5pt y offset, about 1.5pt blur, black about 20% (bottom rows #BABABA, #D6D6D6, #E5E5E5) | | | |
| Knob, dark | **white at 50% alpha** (layer contents a = 128), about #999999 over #323232 | | | The knob "knocks out" the track: a copy of the knob layer sits inside the track and tick layers with `compositingFilter = destOut`, so no track shows through the translucent knob. |
| Filled segment | accent: #0A82FF light / #1769E6 dark (`controlAccentColor` #007AFF) | | | min end to knob centre |
| Filled, **inactive window** | black at about 10% (#D5D5D5) / white at about 19% (#595959) | | | the accent disappears when the window loses key status |
| Unfilled track | black **5%** (a = 13, #E0E0E0 on #ECECEC) / white **10%** (a = 26, #474747 on #323232) | | | slight top-inner shade (216 -> 224 -> 218) |
| With tick marks | knob becomes a **vertical capsule 8x19** (+0.5 hairline) | 8x15 | 7x12 | **no accent fill** when ticks are shown |
| Tick marks | 2x8pt, centred across the track, black 13% (#C9C9C9) / white 18% | 2x8 | 1x7 | `rectOfTickMarkAtIndex:` 2x16 in the 28pt frame |
| Disabled | same geometry, fill removed, knob and track fainter | | | |
| Vertical | bar 4x172 in a 24pt-wide frame, fills from the bottom | | | |
| Circular | 28x30 frame, small dot knob | | | |
| Layer tree | `NSSlider` > `NSSliderTrack` (4pt) + `NSSliderTickMarks` + `NSSliderKnob` | | | [L] |
| Focus ring | `keyboardFocusIndicatorColor` = #0067F4 @ 0.498 (light) / #1AA9FF @ 0.498 (dark) [L]; about 3pt ring around the **knob** only, and only with Full Keyboard Access on (this machine: `canBecomeKeyView = false`, so it was not rendered) | | | ring width 3-3.5pt per WebKit and Qt [A10][A13] |
| Animation | none on drag; the knob tracks the pointer 1:1 | | | |

Web cross-check for 13-15: WebKit's legacy emulation uses a 5pt track and hard-codes a knob thickness of 17 [A10]. Qt draws the ticked knob as an "elongated" pill [A13]. Both are consistent with [L].

### 2.2 macOS 26 Tahoe (Liquid Glass)

Images: [no ticks](../target/native-reference/macos-26-slider-light.png) ([dark](../target/native-reference/macos-26-slider-dark.png)), [ticks](../target/native-reference/macos-26-slider-ticks-light.png) ([dark](../target/native-reference/macos-26-slider-ticks-dark.png)), [tick labels](../target/native-reference/macos-26-slider-ticklabels-light.png), [circular](../target/native-reference/macos-26-slider-circular-light.png), [window states (small ticked slider)](../target/native-reference/macos-26-titlebar-window-states-light.png).

| Property | Value | Source |
|---|---|---|
| Frame heights mini / small / regular / large / XL | 16 / 20 / 24 / 28 / 36 (new `.extraLarge` size) | [A12], [A6] |
| Track | **6pt** for regular/large/XL, **4pt** for small/mini; capsule ends. HIG image: 6pt (measured) | [A12], [A1] |
| Fill | accent (#0088FF, the new 26 blue) from the minimum, **or from `neutralValue`**; `tintProminence = .none` removes it | [A1], [A6] |
| Unfilled | black about 6% (+ hairline ring rgba(0,0,0,.06)) / white 10% | [A12], [A1] (measured) |
| Knob | **horizontal lozenge, wider than tall, with or without ticks**. mini 16x12, small 18x14, **regular 20x16 (r 8)**, large/XL 24x20. HIG image 24x20 (measured). Exception: ticked small/mini knobs are *vertical* pills 10x18 / 8x16 | [A12], [A1] |
| Knob colours | white (light) / #DEDEDE to #E7E8E9 (dark); wide soft halo shadow (about 15pt) | [A12], [A1] (measured) |
| Pressed | the knob becomes a Liquid Glass lens (regular 25x20) that refracts the track | [A12] |
| Ticks | **2pt round dots below the track**, 3-4pt gap, `tertiaryLabelColor` (black 26% / white 32%); disabled uses quaternaryLabel; end dots align with the knob centre at min and max | [A1] (measured), [A12] |
| Disabled | accent at 25% | [A12] |

The next release, macOS 27 "Golden Gate" (released 2026-09-14 per [A25]), keeps the lozenge slider. It changes window radii and the traffic lights, not controls [A21][A25].

### 2.3 Windows

Images: [Win11 slider](../target/native-reference/windows-11-slider-light.png), [Win11 ticks + value tooltip](../target/native-reference/windows-11-slider-ticks-light.png), [Win11 controls light](../target/native-reference/windows-11-controls-light.png) / [dark](../target/native-reference/windows-11-controls-dark.png), [Win32 trackbar (Aero-era image)](../target/native-reference/windows-10-win32-trackbar-legacy-aero.png).

| Property | Windows 10 (UWP/WUX) | Windows 11 (WinUI 3 / Fluent) | Src |
|---|---|---|---|
| Control height | min 32 (15 + track + 15) | min 32 (14 + 4 + 14) | [W7] [W1] |
| Track | 2px, square | **4px, radius 2** (`SliderTrackThemeHeight`, `SliderTrackCornerRadius`) | [W7] [W1] |
| Unfilled | BaseMediumLow #66000000 / #66FFFFFF (hover #99...) | `ControlStrongFillColorDefault` **#72000000 / #8BFFFFFF** (measured #8A8A8A on #F9F9F9) | [W7] [W1] |
| Filled | SystemAccentColor | `AccentFillColorDefault` = SystemAccentColor**Dark1** (L) / **Light2** (D); hover at 0.9 opacity, pressed 0.8 (#005FB8 / #60CDFF measured) | [W1] [W28] |
| Thumb | **8x24 rectangle** (r 4 in later builds); accent; hover near-black #171717 | **22x22 disc** (18 layout, margin -2), radius 10, 1px `ControlElevationBorderBrush` (top #EBEBEB, bottom #D0D0D0), fill `ControlSolidFillColorDefault` #FFFFFF / #454545 | [W7] [W1] |
| Inner dot | -- | accent ellipse, base 12px, scale 0.86 rest / **1.167 hover (14px)** / **0.71 pressed (10px)**; 167 ms (rest) / 250 ms (hover, pressed), KeySpline 0,0,0,1 | [W1] |
| Ticks | 4px tall, BaseMediumLow; inline ticks white | outside TickBar 4px tall, **1px wide**, 4px from the track, `ControlStrongFillColorDefault` (measured #8D8D8D); inline ticks = track height in `ControlFillColorInputActive` | [W1] [W27] |
| Focus | FocusVisualMargin -7,0,-7,0 | **2px outer ring `FocusStrokeColorOuter` (#E4000000 / #FFFFFF) + 1px inner ring `FocusStrokeColorInner` (#B3FFFFFF / #B3000000)**, radius 4 | [W1] [W22] |
| Disabled | #CCCCCC / #333333 | track #51000000 / #3FFFFFFF; value and thumb #37000000 / #28FFFFFF | [W1] |
| Font (header) | 14px | 14px (`ControlContentThemeFontSize`) | [W1] |

### 2.4 GNOME (libadwaita) and KDE Plasma 6 (Breeze)

Images: [GNOME light](../target/native-reference/gnome-47-slider-hig-light.png) / [dark](../target/native-reference/gnome-47-slider-hig-dark.png), [GTK4 docs](../target/native-reference/gnome-gtk4-slider-docs-light.png), [KDE slider + spinbox](../target/native-reference/kde-plasma6-slider-spinbox-hig.png), [KDE dark range slider](../target/native-reference/kde-plasma6-titlebar-dark-125pct.png).

| Property | GNOME (GtkScale, libadwaita 1.4-1.8) | KDE (QSlider, Breeze) | Src |
|---|---|---|---|
| Widget box | `min-height:10px; padding:12px`, 34px total | `PM_SliderThickness` 20 (+ 8px ticks + 2px margin) | [G1] [K1] |
| Track | **4px**, `border-radius:99px` | **6px** (`Slider_GrooveThickness`), radius 3 | [G1] [K1] [K2] |
| Unfilled | currentColor 15% = rgba(0,0,6,.12) / rgba(255,255,255,.15), about #dcdcde / #434347 | WindowText a 0.2 x 0.7 over Window, #d2d4d5 / #3f4144; 1px pen | [G2] [K2] |
| Filled | `--accent-bg-color` (#3584e4 default) | Highlight #3daee9 at a 0.7 over window, #72c2eb (measured) / #3484ae | [G1] [K3] |
| Knob | **20x20 circle**, white (#d2d2d2 dark); ring `0 0 0 1px rgba(0,0,6,.10)` + `0 2px 4px rgba(0,0,6,.20)`, about 22px visually | 18px circle in a 20px rect, Button colour (#fcfcfc / #292c30), 1px outline #d1d1d2 / #535659, 1px shadow a .125 | [G1] [K2] |
| Hover / press | trough 20%, highlight gets a 10% overlay, knob white | outline -> DecorationHover #3daee9; shadow hidden while pressed | [G1] [K2] |
| Ticks | 1x6px, 6px from the trough, 55% dim (1.8). With marks on one side, the knob is a rotated square with one sharp corner (a pointer) | length 8, margin 2; ticks past the value are Highlight | [G1] [K1] [K3] |
| Focus | **2px outline on the knob, accent @ 50%**; animates from offset 6px (transparent) to 0 over 200 ms ease-out-quad | no ring; knob outline -> DecorationFocus | [G5] [K2] |
| Disabled | `filter: opacity(50%)` | groove only, no fill | [G1] [K3] |
| Animation | 200 ms `cubic-bezier(.25,.46,.45,.94)` | 100 ms linear x AnimationDurationFactor | [G4] [K4] |

### 2.5 iOS and Android Material 3

Images:
- iOS 17 (SwiftUI docs render, version unstated): [light](../target/native-reference/ios-17-slider-light.png), [dark](../target/native-reference/ios-17-slider-dark.png), [with labels](../target/native-reference/ios-17-slider-labels-light.png);
- iOS 26 native UISlider: [light](../target/native-reference/ios-26-slider-light.png), [dark](../target/native-reference/ios-26-slider-dark.png), [disabled](../target/native-reference/ios-26-slider-disabled-light.png), [ticks](../target/native-reference/ios-26-slider-ticks-light.png), [overview](../target/native-reference/ios-26-controls-overview-light.png), [lens illustration](../target/native-reference/ios-26-glass-lens-illustration.png);
- M3: [anatomy](../target/native-reference/android-m3-slider-anatomy.png), [stop indicator](../target/native-reference/android-m3-slider-stopindicator.png), [centered](../target/native-reference/android-m3-slider-centered.png).

Apple publishes almost no pixel numbers. Values tagged (F) are from Flutter's Cupertino clone and (Ion) from Ionic's iOS styles. iOS 26 captures come from Codename One's native goldens at about 2.833 px/pt [I19].

| Property | iOS 17/18 | iOS 26 | Material 3 (2024+ / Expressive; [legacy v0.192]) | Src |
|---|---|---|---|---|
| Control height | 44 pt (F) | -- | 44dp (= handle); [48dp] | [I1] [M3] [M6] |
| Track | **4 pt** capsule (measured; Ion 4px) | **6 pt** capsule (measured) | **16dp**; outer corners full, **2dp inner corners** beside the handle; Expressive S/M/L/XL tracks 24/40/56/96; [4dp] | [I5] [I2] [I19] [I22] [M3] [M4] |
| Fill | systemBlue #007AFF / #0A84FF | **#0088FF / #0091FF** (measured (0,136,255)) | primary #6750A4 / #D0BCFF | [I7] [I19] [M1] |
| Unfilled | `systemFill` rgba(120,120,128,.20) / (.36) (Apple: "for the track of a slider") | #E6E6E6 on white, #191919 on black (measured) | secondary-container #E8DEF8 / #4A4458; [surface-container-highest] | [I3] [I19] [M3] |
| Thumb | **white 28 pt circle** (F; Ion 26px; measured about 27), white in dark mode too | **white horizontal capsule 37x24 pt** at rest | **4x44dp vertical bar**, full radius, primary; [20dp circle] | [I1] [I2] [I19] [I22] [M3] |
| Thumb shadow | (F) 0.5pt ring rgba(0,0,0,.04) + `0 3 8 α.15`, `0 1 1 α.16`, `0 3 1 α.10`; Ion `0 .5px 4px rgba(0,0,0,.12), 0 6px 13px rgba(0,0,0,.12)` | soft, about 10pt spread, darker below | none; [level 1] | [I1] [I2] [I19] |
| Thumb-track gap | none (overlaps) | none | **6dp** each side | [M3] |
| Pressed | -- | thumb becomes a **clear glass lens** magnifying the track (recreation: 58x38 pt) | handle width 4 -> **2dp**; value label appears (inverse-surface bubble, label-large) | [I17] [I22] [M4] [M6] |
| Ticks | none before 26 | **new**: about 2.7pt dots, #C6C6C8, **4pt below** the track (`UISlider.TrackConfiguration`, SwiftUI `SliderTick`) | 4dp dots inside the track (secondary-container on active, primary on inactive); stop indicator = 4dp dot 8dp from the end | [I23] [I18] [M3] [M4] |
| Neutral value | -- | `neutralValue`: fill starts from it | centered slider | [I18] [M4] |
| Disabled | -- | see image | handle and active track on-surface 38%, inactive 12% | [M3] |
| Focus | not found | not found | handle narrows to 2dp; optional inset ring (web: 3px secondary, 2px offset) | [M3] [M2] |
| Motion | (F) snap 500 ms fastOutSlowIn | elastic, momentum | label in 400 ms emphasized, out 150 ms | [I1] [M6] |

### 2.6 Cross-platform slider digest (regular size)

| | macOS 15 | macOS 26 | Win 11 | GNOME | KDE | iOS 17/18 | iOS 26 | M3 (2024+) |
|---|---|---|---|---|---|---|---|---|
| Track | 4 | 6 | 4 | 4 | 6 | 4 | 6 | 16dp (legacy 4) |
| Knob | 20 circle | 20x16 lozenge | 22 disc + 12 dot | 20 circle | 18 circle | 28 circle | 37x24 capsule | 4x44 bar (legacy 20 circle) |
| Knob fill | white / white 50% | white / #DEDEDE | white / #454545 | white / #d2d2d2 | button colour | white (both modes) | white; glass while dragged | primary |
| Fill | accent | accent | accent (Dark1 / Light2) | accent | highlight 70% | systemBlue | #0088FF | primary, 6dp gap each side of the handle |
| Ticks | across the track, 2x8, knob becomes a capsule | dots below | 1x4 lines below | 1x6 below | 8 long | none | 2.7pt dots 4pt below | 4dp dots inside the track |
| Focus | 3pt accent 50% ring | accent 25%, 3.5pt | 2+1 px black/white | 2px accent 50% | outline colour | -- | -- | handle narrows to 2dp + optional ring |

---

## 3. Spinner / activity indicator

### 3.1 macOS 11-15 (`NSProgressIndicator`, style `.spinning`), measured [L] on 15.5

Images: [animated GIF light](../target/native-reference/macos-15-spinner-regular-light.gif) / [dark](../target/native-reference/macos-15-spinner-regular-dark.gif) (all 24 frames at 33 ms), [the 24-frame sprite sheet AppKit uses](../target/native-reference/macos-15-spinner-spritesheet-24frames-light.png) ([dark](../target/native-reference/macos-15-spinner-spritesheet-24frames-dark.png)), [regular/small/mini/large, static](../target/native-reference/macos-15-spinner-sizes-static-light.png), [HIG 2023 image](../target/native-reference/macos-14-spinner-hig-light.png).

| Property | Value |
|---|---|
| Sizes | regular **32**, small **16**, mini **10**, large = 32 |
| Structure | one `CALayer` whose `contents` is a **1536x64 sprite sheet = 24 frames of 64x64 px** (32pt @2x) |
| Animation | `CAKeyframeAnimation` named `CUIIndeterminateProgressAnimation`, keyPath `contentsRect`, `calculationMode = discrete`, **24 values**, `keyTimes` i/24, **duration 0.8 s**, `repeatCount` infinite. That is 30 fps, one revolution per 0.8 s |
| Spokes | **8 capsules** (round caps), one every 45° |
| Spoke geometry @32pt | width **4pt** (= D/8); from r = **6.5pt** (0.40 R) to r = **16pt** (the outer cap touches the bounds); length about 9.5pt. Scales linearly (16pt: 2pt wide, r = 3.25 to 8) |
| Colour | pure black (light) / pure white (dark); only the alpha varies |
| Opacity ramp (frame 0) | head **140/255 = 0.55** at 12 o'clock. Going counter-clockwise behind the head: 122, 105, 87, 69, 51, 33, **15 (0.06)**. That is a linear 0.07 step, so the dimmest spoke sits just clockwise of the head |
| Motion | the head advances **clockwise**. The spoke ahead of it fades **in** over 3 frames (15 -> 54 -> 93 -> 132 -> 140) while every other spoke fades out linearly. It is not a 45° jump |
| Per-spoke opacity curve | peak 0.55 at t = 0, linear decay to 0.06 at t = 7/8 x 0.8 s = 0.7 s, linear rise back to 0.55 by 0.8 s. Spoke k (clockwise) is phase-shifted by k x 0.1 s |
| Reduce motion | keeps spinning [A12] |

Web cross-check:
- The 26/27 design kit gives the same numbers: `rect x=14 y=0 w=4 h=10 rx=2` rotated in 45° steps, ramp **0.55 -> 0.06, step 0.07**, 0.8 s [A12].
- The 2023 HIG image measures 0.80 -> 0.10 [A2]. That is a composited illustration; the live 15.5 sprite says 0.55 -> 0.06.
- Pre-Big Sur (<= 10.15) had **12** thin spokes (2.75 wide at 32pt, r 7.5 -> 13.5), 50 ms per step and 0.6 s per turn [A22].

### 3.2 Windows

Images: [Win11 indeterminate ring GIF](../target/native-reference/windows-11-progressring-indeterminate.gif), [its 27 frames](../target/native-reference/windows-11-progressring-indeterminate-framestrip.png), ["Signing in" example](../target/native-reference/windows-11-progressring-indeterminate-example.gif), [determinate ring](../target/native-reference/windows-11-progressring-determinate.png).

| Property | Windows 10 UWP ProgressRing | Windows 11 WinUI 3 ProgressRing | Src |
|---|---|---|---|
| Visual | 5 dots (a 6th in the "Large" state) orbiting the centre | **one round-capped arc** (Lottie) + optional track circle (invisible by default) | [W9] [W4] [W5] |
| Size | dot diameter 0.1 W (+1 if W <= 40); 32px ring = 4.2px dots | default **32**, min 16 (docs: 20); uses the smaller of width and height | [W6] [W4] [W26] |
| Stroke | -- | ring centre-line radius **0.4375 x size**, stroke **0.09375 x size** (16 -> 1.5, 32 -> 3, 64 -> 6). The `ProgressRingStrokeThickness = 4` resource is ignored by the Lottie | [W5] |
| Colour | accent | `AccentFillColorDefault` (#005FB8 / #60CDFF measured); track `ControlFillColorTransparent` | [W4] |
| Cycle | each dot follows the angle keyframes -110° @0 s (spline .13,.21,.1,.7) -> 10° @0.433 -> 93° @1.2 (linear) -> 205° @1.617 (.57,.17,.95,.75) -> 357° @2.017 (0,.19,.07,.72) -> 439° @2.783 -> 585° @3.217 (0,0,.95,.37). Dots start 0.167 s apart and are offset -6° each; each blinks off 3.22-3.47 s | **2.0 s loop**. The container rotates 0 -> 450° over 0-1 s and -> 900° at 2 s (easing .167,.167,.833,.833). Arc A: TrimEnd 0 -> 0.5 over 0-1 s. Arc B: TrimStart 0 -> 0.5 over 1-2 s. So the arc **grows to 180° and then shrinks from the tail**, starting at 3 o'clock, clockwise | [W9] [W5] |

### 3.3 GNOME and KDE

Images: [AdwSpinner 64px light](../target/native-reference/gnome-49-spinner-adw-light.png) / [dark](../target/native-reference/gnome-49-spinner-adw-dark.png), [HIG spinner](../target/native-reference/gnome-47-spinner-hig-light.png), [KDE process-working icon](../target/native-reference/kde-plasma6-busyindicator-process-working-symbolic-16.svg), [KDE legacy 15-frame sprite](../target/native-reference/kde-plasma6-busyindicator-process-working-22-sprite.svg), [Plasma busy ring](../target/native-reference/kde-plasma6-busyindicator-plasma-busywidget.svg).

| Property | GNOME AdwSpinner (1.6+, GNOME 47+) | Old GtkSpinner | KDE | Src |
|---|---|---|---|---|
| Size | natural 16, drawn at min(w,h), **capped at 64** | 16 | qqc2 BusyIndicator 36 (2 grid units); KBusyIndicatorWidget 16 | [G11] [G15] [K9] [K11] |
| Shape | full-circle **track at 15%** + **arc at 100%**; stroke `lerp(2.5, 7, (r-8)/24)` (16px -> 2.5, 32 -> 4, 64 -> 7); round caps | `process-working-symbolic` icon | rotating `process-working-symbolic` (Breeze: an 8-tooth gear); Plasma: a highlight-coloured ring with a tail | [G10] [K9] [K10] [K13] |
| Rotation | linear, **1200 ms** per turn; ignores `enable-animations` | 1 s linear | **2000 ms** linear, phase-synced to the wall clock; fades in and out over 100 ms OutCubic | [G10] [G13] [K9] |
| Arc length | oscillates 2.7° -> 162° (formula below); one grow/shrink cycle about 1.59 s | fixed | fixed | [G10] |

The AdwSpinner arc formula [G10], with a = base angle and `ease` = ease-in-out-sine:

```
l = 2.65π; m = a mod l
ts = m > 1.1π ? 1 : ease(m / 1.1π)
te = m < 0.4π ? 0 : m > 1.75π ? 1 : ease((m - 0.4π) / 1.35π)
start = a + lerp(0.015π, 0.9π, ts) - m*(0.9/2.65) + 0.35π
end   = a + lerp(0, 0.885π, te)     - m*(0.9/2.65) + 0.35π
```

### 3.4 iOS and Material 3

Images:
- M3 circular: [indeterminate](../target/native-reference/android-m3-circularprogress-indeterminate.gif), [wavy](../target/native-reference/android-m3-circularprogress-wavy-indeterminate.gif);
- M3 Expressive LoadingIndicator: [plain](../target/native-reference/android-m3-loadingindicator.gif), [contained](../target/native-reference/android-m3-loadingindicator-contained.gif), [anatomy](../target/native-reference/android-m3-loadingindicator-anatomy.png);
- iOS: no local image of `UIActivityIndicatorView`.

| Property | iOS 17/18 `UIActivityIndicatorView` | iOS 26 | M3 CircularProgressIndicator | Src |
|---|---|---|---|---|
| Size | `.medium` **20x20** (default), `.large` **37x37** | unchanged (not in Apple's changed-controls list) | **40dp** (MDC S 28, XS 20) | [I13] [I1] [I16] [M3] |
| Elements | **8 spokes** (capsules) | 8 (measured on a refresh control) | one arc, stroke **4dp**, round caps (2024; earlier butt caps) | [I1] [I2] [I8] [M3] |
| Spoke geometry | (F) width r/5 (2pt at r = 10), radial r/3 -> r, i.e. capsule; (Ion) 14 -> 26 in a 64 box, stroke 7 | 3pt wide, ink from 0.38r to r | arc radius 18dp; track gap 4dp | [I1] [I2] [I8] [M4] |
| Colour | (F) #3C3C44 / #EBEBF5 | head = secondaryLabel | primary; indeterminate track transparent | [I1] [I8] [M3] |
| Opacity ramp | (F) .576, .478, .380, .282, .184 x4 | measured .60 -> .09 in eight 0.07 steps | -- | [I1] [I8] |
| Motion | **1.0 s per revolution, 8 discrete steps of 125 ms**, clockwise | same as far as known | Compose M3: 6000 ms cycle; base rotation 0 -> 1080° linear, plus 90° jumps at 0/1500/3000/4500 ms (300 ms each); arc 36° -> 313° -> 36°, standard easing (0.2,0,0,1). Legacy web: arc 1333 ms, rotation 1568 ms linear, easing (0.4,0,0.2,1) | [I1] [I2] [M4] [M2] [M5] |

**M3 Expressive LoadingIndicator** [M3][M4][M6]:
- 48dp container (full radius), 38dp shape; the contained variant uses a primary-container circle.
- Seven Material shapes morph in a loop: SoftBurst -> Cookie9 -> Pentagon -> Pill -> Sunny -> Cookie4 -> Oval.
- Each morph takes 650 ms on spring(damping 0.6, stiffness 200), adds +90°, and the whole indicator rotates 360° per 4666 ms linear.

Note the contrast with macOS: iOS steps discretely at 8 frames/s, while macOS cross-fades the head across 24 frames per 0.8 s (section 3.1).

---

## 4. Titlebar / window chrome

### 4.1 macOS 13-15, measured [L] on 15.5

Images:
- standard 28pt bar: [light](../target/native-reference/macos-15-titlebar-standard-light.png), [dark](../target/native-reference/macos-15-titlebar-standard-dark.png), [light inactive](../target/native-reference/macos-15-titlebar-standard-light-inactive.png), [dark inactive](../target/native-reference/macos-15-titlebar-standard-dark-inactive.png);
- unified toolbar, 52pt: [light](../target/native-reference/macos-15-titlebar-unified-toolbar-light.png), [dark](../target/native-reference/macos-15-titlebar-unified-toolbar-dark.png), [inactive](../target/native-reference/macos-15-titlebar-unified-toolbar-light-inactive.png);
- unified compact, 38pt: [light](../target/native-reference/macos-15-titlebar-unified-compact-light.png), [dark](../target/native-reference/macos-15-titlebar-unified-compact-dark.png);
- a real Sequoia Finder: [image](../target/native-reference/macos-15-titlebar-unified-finder-light.png).

| Property | Standard titled window | `toolbarStyle = .unified` | `.unifiedCompact` | `.expanded` |
|---|---|---|---|---|
| Bar height | **28** (`frameRect - contentRect`; with `FullSizeContentView`, `contentLayoutRect` loses 28) | **52** | **38** | 44 (title row + toolbar row) |
| Traffic-light button frames | 14x16 at x = **7, 27, 47**, y = 6 (vertically centred) | x = 19, 39, 59, y = 18 | x = 12, 32, 52, y = 11 | x = 7, 27, 47, y = 22 |
| Traffic-light circles | **12pt diameter** (24px @2x), centres x = 14, 34, 54 -> **20pt pitch, 8pt gap, 8pt left inset**, centred on the bar's midline | same circles, centred on 26 | centred on 19 | on the title row |
| Colours (active) | close **#FF5F57**, minimise **#FEBC2E**, zoom **#28C840**, each with a slightly darker 0.5pt rim | | | |
| Colours (inactive window) | all three **#D6D6D6** (light) / **#4C4C4C** (dark) | | | |
| Title font | **`NSFont.titleBarFont` = `.AppleSystemUIFaceHeadline` 13pt, weight trait 0.4 = `NSFontWeightBold`**, the same as `boldSystemFontOfSize:13` (`.AppleSystemUIFontBold`) | **`.SFNS-Semibold` 15pt** | `.SFNS-Semibold` 15pt | 13pt Bold |
| Title position | `NSTextField` 88x16 at x = 196 in a 480-wide window: **centred on the window width** (not on the space right of the buttons). 16pt line box at y = 7 (bottom-up): box top at 5pt, so the text centre sits about 1pt above the bar's midline | **left-aligned** at x = 91 | left-aligned at x = 80 | centred |
| Title colour | measured darkest pixel #4B4B4B on #FBFBFB (light) / #B4B4B4 on #383838 (dark); `windowFrameTextColor` = black / white @ 0.847, drawn vibrant | | | |
| Title colour, inactive | #B0B0B0 (light) / #696969 (dark), about tertiaryLabel | | | |
| Background | vibrancy material (`NSVisualEffectView`); off-screen fallback **#FBFBFB** (light) / **#383838** (dark); inactive #F0F0F0 / #282828 | | | |
| Top edge | 1px highlight at the very top (#FDFDFD / #606060): the window's inner rim | | | |
| Separator | 1px (0.5pt) **#D0D0D0** + 1px #E9E9E9 (light); 1px **#000000** + #1E1E1E (dark). `titlebarSeparatorStyle` default `.automatic` (0); options `.none` / `.line` / `.shadow` | | | |
| Window corner radius | **10pt** [A21] | | | |
| `titlebarAppearsTransparent` + `FullSizeContentView` (azul's `NoTitle` / `NoTitleAutoInject`) | same 28pt band and the same traffic-light positions; no background, no separator, no title (`titleVisibility = .hidden`) | | | |

### 4.2 macOS 26 Tahoe

Images: [Tahoe Finder](../target/native-reference/macos-26-titlebar-unified-finder-light.png), [toolbar anatomy light](../target/native-reference/macos-26-titlebar-toolbar-anatomy-light.png) / [dark](../target/native-reference/macos-26-titlebar-toolbar-anatomy-dark.png), [key / main / inactive windows](../target/native-reference/macos-26-titlebar-window-states-light.png), [Notes toolbar with menu](../target/native-reference/macos-26-titlebar-notes-toolbar-menu-light.png), [segmented control in toolbar](../target/native-reference/macos-26-titlebar-segmented-in-toolbar-light.png).

| Property | Value | Src |
|---|---|---|
| Traffic lights | **14pt at a 23pt pitch** (reported via `standardWindowButton` on 26.6.2); centre about (25.5, 25.5) in a toolbar window. Custom titlebars that copy the 15 layout end up about 2pt too high | [A17] (unverified snippet), [A20] (measured), [A23] |
| Window radius | **26pt** with a toolbar; **about 16pt** titlebar-only; 10 before 26; 20 in macOS 27 | [A18] [A19] [A21] |
| Titlebar-only height | undocumented; GitHub Desktop proposes 32px to re-centre the larger lights | [A23] |
| Toolbar | unified about 52pt; items sit on **glass capsules about 34pt tall**, grouped by kind; `.prominent` tints the glass with the accent | [A20] (measured), [A6] |
| Separator | replaced by the **scroll edge effect** (soft: fade + blur; hard: opaque) | [A6] [A7] |
| Sidebar | floating glass pane inset about 8pt | [A6] [A20] |
| Title | title and subtitle beside the glass, not on it | [A3] |
| Inactive | glass "recedes", traffic lights grey | [A3] [A8] |
| Colours | system blue **#0088FF** (light) / **#0091FF** (dark) | [A4] [A16] |

### 4.3 Windows

Images: [Win11 title bar anatomy @2x](../target/native-reference/windows-11-titlebar-overview.png), [back button](../target/native-reference/windows-11-titlebar-backbutton.png), [48px with search](../target/native-reference/windows-11-titlebar-search.png), [tabs in the title bar](../target/native-reference/windows-11-titlebar-tabs.png), [Mica window light](../target/native-reference/windows-11-mica-window-light.png) / [dark](../target/native-reference/windows-11-mica-window-dark.png).

| Property | Windows 10 | Windows 11 | Src |
|---|---|---|---|
| Height | caption about 22-23px system metric (SM_CYCAPTION 23 unverified; registry CaptionHeight -330 twips = 22); Chromium's emulated buttons are 29px tall | **32** standard; **48** when the bar holds interactive content (`TitleBarHeightOption.Tall`); WinUI TitleBar control 32 / 48 | [W31] [W16] [W10] [W12] [W13] |
| Caption buttons | 46 wide (Chromium: 45 + 1) | **46 wide**, height = bar height | [W15] [W16] [W14] |
| Glyphs | 10x10 1px paths | Segoe Fluent Icons **E921** minimise, **E922** maximise, **E923** restore, **E8BB** close; 10px | [W15] [W10] |
| Min/max hover / pressed | foreground at 10% / 20% (#1A / #33 alpha) | `SubtleFillColorSecondary` #09000000 / #0FFFFFFF; pressed `SubtleFillColorTertiary` #06000000 / #0AFFFFFF | [W16] [W14] |
| Close hover / pressed | **#E81123**, white glyph / #F1707A | **#C42B1C**, white glyph / #C42B1C at 0.9 with the glyph at 0.7; apps cannot override these | [W15] [W14] [W11] |
| Transitions | background fades back over 200 ms | background 150 ms, glyph 100 ms | [W15] [W14] |
| Inactive | foreground at a 0x66 | glyphs `TextFillColorDisabled` #5C000000 / #5DFFFFFF; title `TextFillColorTertiary`; WinUI TitleBar deactivated opacity 0.5 | [W17] [W14] [W13] |
| Title | Segoe UI 9pt (12px), **left-aligned**; icon at x = 8, title at x = 28 | **Segoe UI Variable, Caption 12/16 Regular, left-aligned**; 16px icon 16px from the left edge, title 16px after the icon | [W18] [W10] [W13] [W23] |
| Background | white, or the accent colour when `ColorPrevalence = 1` (text white if 0.25R+0.625G+0.125B <= 128) | **Mica**; falls back to `SolidBackgroundFillColorBase` #F3F3F3 / #202020 (Mica Alt #DADADA / #0A0A0A) | [W17] [W25] |
| Window border | 1px, active #A8262626, inactive #80555555; optional accent blend | 1px stroke + shadow (elevation 128); `DWMWA_BORDER_COLOR` (34) | [W17] [W24] |
| Corner radius | 0 | **8** (`DWMWCP_ROUND`); 0 when maximised or snapped | [W19] [W20] |

### 4.4 GNOME and KDE

Images:
- GNOME 49 headerbar: [light](../target/native-reference/gnome-49-headerbar-light.png), [dark](../target/native-reference/gnome-49-headerbar-dark.png), [flat](../target/native-reference/gnome-49-headerbar-flat-light.png), [window](../target/native-reference/gnome-49-window-light.png);
- GNOME 47 headerbar: [light](../target/native-reference/gnome-47-headerbar-hig-light.png), [buttons](../target/native-reference/gnome-47-headerbar-hig-buttons-light.png);
- KDE Breeze titlebar (at 125%): [light](../target/native-reference/kde-plasma6-titlebar-light-125pct.png), [dark](../target/native-reference/kde-plasma6-titlebar-dark-125pct.png).

| Property | GNOME AdwHeaderBar | KDE Breeze decoration | Src |
|---|---|---|---|
| Height | **46px** as a window titlebar (47 standalone; 40 shrunk); measured 46 | max(font, button) + 2 x smallSpacing top and bottom; Noto Sans 10 gives 20px buttons and **about 28px** (derived; 34-35 measured at 125%) | [G7] [K6] [K7] [K19] |
| Background | `--headerbar-bg-color` #ffffff; dark #303030 (45-47) / #2e2e32 (48+) | `[Colors:Header] BackgroundNormal` **#dee0e2** / **#292c30** (6.4+; #31363b before); inactive #eff0f1 / #202326 | [G3] [K5] [K8] |
| Bottom edge | standalone: `inset 0 -1px` shade rgba(0,0,6,.12) / (.36); as a titlebar: outer `0 1px` + `0 2px 4px` shade @ 50%; `.flat`: none | no separator; the header colour merges with the app toolbar | [G7] [G9] [K6] |
| Title | **bold (700) 11pt system font** (Cantarell <= 47, Adwaita Sans 48+), **centred**, padding 0 12px; subtitle smaller + 55% | `[WM] activeFont` **Noto Sans 10 regular**, `AlignCenterFullWidth` (**centred on the full width**), middle elision; #232629 / #fcfcfc | [G7] [G18] [K6] [K14] |
| Buttons | 34x34 hit box, 3px spacing; visible **circle 20px (45-48) / 24px (49)** filled currentColor 10% (hover 15%, active 30%, 200 ms); default layout `appmenu:close`; close circle 11px from the edge | size = 2 x gridUnit; hover: circle filled with the text colour; **close hover: red circle #ff98a2** (active) / #da4453 | [G7] [K6] |
| Window radius | **12px (<= 47) -> 15px (48+)** | 5px (6.2+) | [G4] [G8] [K6] |
| Unfocused | bg -> window bg, contents `filter: opacity(.5)`, 200 ms | Header[Inactive] colours | [G7] [K6] |

### 4.5 iOS navigation bar and the M3 top app bar

Images:
- iOS 26: [nav bar light](../target/native-reference/ios-26-navbar-light.png) / [dark](../target/native-reference/ios-26-navbar-dark.png), [nav + tab bar](../target/native-reference/ios-26-navbar-tabbar-light.png), [toolbar groups](../target/native-reference/ios-26-toolbar-groups-light.png);
- M3: [small](../target/native-reference/android-m3-topappbar-small.png), [anatomy](../target/native-reference/android-m3-topappbar-anatomy.png), [medium flexible](../target/native-reference/android-m3-topappbar-medium-flexible.png), [large flexible](../target/native-reference/android-m3-topappbar-large-flexible.png).

| Property | iOS 17/18 `UINavigationBar` | iOS 26 | M3 top app bar | Src |
|---|---|---|---|---|
| Height | **44 pt** (+ status bar 47-62 depending on the device); large title **96** | reports **54 pt**, with the 44pt item row top-aligned | small / center-aligned **64dp**; medium **112**; large **152**; flexible 112-152 | [I1] [I9] [I24] [M1] [M3] |
| Title | **17pt semibold, centred**, tracking -0.43; large title 34 bold | 17 semibold centred; new subtitle API | **title-large 22/28 regular, start-aligned at 16dp** (center-aligned variant centred); medium headline-small 24, large headline-medium 28 | [I1] [I4] [I17] [M1] [M3] |
| Buttons | 17pt tint text, 16pt edge padding, back chevron + label | **44pt glass circles**; text pills; grouped image items share one capsule; back = chevron-only circle | 24dp icons in 48dp targets, 4dp outer padding | [I1] [I23] [I25] [M3] |
| Background | blurred chrome ((F) rgba(249,249,249,.94) / rgba(29,29,29,.94), blur σ10) | **none**: content scrolls under the bar | surface #FEF7FF / #141218; on scroll surface-container #F3EDF7 / #211F26 | [I1] [I17] [M1] |
| Separator | hairline 1 physical px ((F) rgba(0,0,0,.30)); hidden at the scroll edge since iOS 15 | none; scroll-edge effect `soft` (blur + fade) or `hard` | none | [I1] [I3] [I18] |

---

## 5. azul gap analysis: slider, spinner and titlebar

State read on branch `fix/input-bugs-2026-09-19`, 2026-09-28. azul ships two widget themes, `UiTheme::Flat` (the default; a Bootstrap-like palette of #0d6efd, #dee2e6 and #ced4da) and `UiTheme::Flora` (skeuomorphic, with gloss layers). **There is no native/system theme.** Neither theme varies by OS, although the cascade can: `DynamicSelector::Os(OsCondition::{MacOS, Windows, Linux, IOS, Android})`, `DynamicSelector::OsVersion`, `Theme`, `:backdrop` (window unfocused), `:hover`, `:active`, `:focus`, `:disabled` and `:checked` all exist (`css/src/dynamic_selector.rs`). **azul has no `outline` property**, so focus rings must be `box-shadow` spreads or borders.

### 5.1 Slider

**What azul draws.** `layout/src/widgets/slider.rs:147-259`:
- The track is a 200x16px flex row with `border-radius: 8px` and background `#cccccc`.
- The thumb is its only child: 16x16, radius 8, background `#0d6efd`, positioned by `margin-left = fraction x (200 - 16)`.
- `commit_value` (`slider.rs:451`) moves the thumb with `set_css_property(margin-left)` on `get_first_child(track)`.
- `themes/flat.rs:1360-1455` only adds dark twins (track `DARK_CONTROL_BACKGROUND`, thumb `DARK_ACCENT_BACKGROUND`).
- `themes/flora.rs:1756+` adds a gloss layer to the thumb and `DARK_TRACK`.

| Aspect | azul (Flat and Flora) | Native (every platform) | Gap |
|---|---|---|---|
| Track thickness | **16px** (= knob height) | 4 (macOS 15, Win11, GNOME, iOS), 6 (macOS 26, KDE), 16dp (M3 2024+) | 4x too thick; reads as a scrollbar or toggle |
| Knob vs track | knob *inside* the track, same height | knob *larger* than the track, centred on it (20 vs 4) | wrong proportion |
| Filled segment | **none** | accent from min (or neutral value) to the knob centre | missing: this is the most recognisable cue |
| Knob colour | solid accent #0d6efd | white (dark: white 50% on macOS 15, #454545 Win11, #d2d2d2 GNOME) + hairline + shadow | accent is on the wrong element |
| Knob border / shadow | none | 0.5pt #C7C7C7 + about 1.5pt blur (macOS 15); 1px gradient (Win11); ring + 0 2px 4px (GNOME) | missing |
| Hover / pressed | none | Win11 inner dot 12 -> 14 -> 10 (167/250 ms); GNOME trough 20%; macOS 26 glass lens | missing |
| Focus | none (no `:focus` rule) | accent 50% ring around the knob (macOS, GNOME); 2+1px black/white (Win) | missing: a keyboard-only user cannot see which slider is focused |
| Inactive window | nothing | macOS fill turns grey (#D5D5D5 / #595959) | missing (`:backdrop` exists) |
| Disabled | nothing | 50% (GNOME), #37000000 (Win) | missing |
| Ticks / sizes / vertical | none | ticks on all platforms; macOS mini/small/regular/large; vertical on all | API missing |
| Width | fixed 200 | stretches to its container (min widths only) | should be `width:100%` / flex-grow |

**What to change, in order:**
1. **DOM: three children instead of one.** The outer node keeps the hit area and callbacks and is the full knob height (28 macOS / 32 Win / 34 GNOME), `position: relative`. Inside it:
   - a `rail` (absolute, full width, 4px tall, vertically centred, pill radius);
   - a `fill` (absolute, same geometry, `width = knob centre`, `background: system:accent`);
   - a `knob` (absolute, `left = fraction x (W - knob)`).
   
   `commit_value` must then update two properties (knob `left`/`margin-left` and fill `width`) and address children by class, not `get_first_child`. The fill could instead be a hard-stop `linear-gradient(to right, accent X%, rest X%)` on the rail: one property, no extra node.
2. **Per-OS values.** Put these in a new `UiTheme::Native`, or as `Os`-conditional declarations in Flat.

| Token | macOS 11-15 | macOS 26+ | Windows 11 | GNOME | KDE |
|---|---|---|---|---|---|
| hit height | 28 (small 20, mini 17) | 24 (16/20/28/36) | 32 | 34 | 20+ |
| rail | 4px, r2 | 6px, r3 | 4px, r2 | 4px, r99 | 6px, r3 |
| rail colour L / D | rgba(0,0,0,.05) / rgba(255,255,255,.10) | rgba(0,0,0,.06) / rgba(255,255,255,.10) | rgba(0,0,0,.447) / rgba(255,255,255,.545) | rgba(0,0,6,.12) / rgba(255,255,255,.15) | #d2d4d5 / #3f4144 |
| fill | `system:accent`; `:backdrop` rgba(0,0,0,.10) / rgba(255,255,255,.19) | accent | accent **Dark1 / Light2** | accent-bg | highlight @ .7 |
| knob | 20 circle, white / rgba(255,255,255,.5) | 20x16 lozenge r8, white / #DEDEDE | 22 disc white / #454545 + 12px accent dot (14 hover, 10 pressed) | 20 circle white / #d2d2d2 | 18 circle, button colour |
| knob edge | `box-shadow: 0 0 0 .5px rgba(0,0,0,.15), 0 .5px 1.5px rgba(0,0,0,.2)` | `0 0 1px rgba(0,0,0,.05), 0 0 4px rgba(0,0,0,.05), 0 0 15px rgba(0,0,0,.1)` | 1px border rgba(0,0,0,.06) (bottom .16) | `0 0 0 1px rgba(0,0,6,.1), 0 2px 4px rgba(0,0,6,.2)` | 1px #d1d1d2 / #535659 |
| focus (`:focus` on the outer node, shown on the knob) | `0 0 0 3px` `keyboardFocusIndicatorColor` (accent @ .5) | accent @ .25, 3.5px | 2px #E4000000 outer + 1px #B3FFFFFF inner | 2px accent @ .5 | outline -> #3daee9 |
| ticks | 2x8 across the rail; knob becomes an 8x20 capsule; no fill | 2pt dots 3-4pt below, tertiary label | 1x4 lines 4px below | 1x6, 6px below | 8px |
| motion | none | glass lens on press | dot scale 167/250 ms `cubic-bezier(0,0,0,1)` | 200 ms `cubic-bezier(.25,.46,.45,.94)` | 100 ms |

3. **Width:** default to `flex-grow: 1; min-width: 120px` (macOS and GNOME) instead of a fixed 200.
4. **API** (optional, later): `with_tick_marks(count, snap)`, `with_control_size(Mini|Small|Regular|Large|ExtraLarge)`, `with_orientation(Vertical)` and `with_neutral_value(v)` (macOS 26, M3 centered slider).
5. **Missing tokens used above:** see section 7. The main ones are `system:control-fill` (black 5% / white 10%), `system:knob` (white / white 50%), `system:focus-ring` (already in `SystemStyle.focus_visuals.focus_ring_color` but not addressable as `system:`), and on Windows `system:accent-dark-1` / `system:accent-light-2`.

### 5.2 Spinner

**What azul draws.** `layout/src/widgets/spinner.rs:49-165` is a **static** 24px bordered circle: `border: max(size/8, 2)px solid`, top edge #0d6efd, the other three #d0d4d9. It is one node with no theme dispatch. The module doc says "Azul has no declarative CSS animation", and that is **no longer true**:
- `css/src/css.rs` parses `@keyframes`;
- `CssProperty::Animation` carries `StyleAnimation { name, duration, delay, iterations: Count | Infinite, timing: Ease | Linear | EaseIn | EaseOut | EaseInOut | Spring* | CubicBezier }`;
- `switch.rs:155` already declares one;
- `transform: rotate()` and `conic-gradient()` exist.

Two limits remain. There is **no `steps()` timing**, and **negative `animation-delay` is rejected**: the delay is a `CssDuration`, which refuses negative values (`css/src/props/basic/time.rs:565`, `negative_durations_are_rejected_in_both_units`).

**Native styles and how azul could produce each:**

| Style | Used by | Build in azul |
|---|---|---|
| **Spokes** (8 capsules, opacity wave) | macOS 11-26, iOS (section 3.4) | 8 absolute children in a `size x size` box. Each spoke: `width: size/8; height: 0.3*size; border-radius: size/16; left: 50%-size/16; top: 0; transform-origin: 50% (size/2); transform: rotate(k*45deg); background: rgba(0,0,0,.55)` (dark: rgba(255,255,255,.55)). Each also gets `animation: az-spoke 0.8s linear infinite` with `@keyframes az-spoke { 0% {opacity:1} 87.5% {opacity:.11} 100% {opacity:1} }` and spoke k **phase-shifted by k x 0.1s**. This needs **negative delays** (`-0.1s x (8-k)`), a one-line parser relaxation that the CSS spec allows. The workaround is 8 phase-rotated `@keyframes` blocks. A positive delay alone makes all spokes sit at full opacity for up to 0.7s at mount. Sizes: 32 regular, 16 small, 10 mini. |
| **Rotating arc** (grows and shrinks) | Win11 ProgressRing (2s), GNOME AdwSpinner (1.2s + 15% track), M3 circular (section 3.4) | Needs a **trimmed stroke**, which CSS borders cannot animate. Options: an SVG-path primitive with `stroke-dasharray` and `stroke-dashoffset`; a `conic-gradient` arc plus a ring mask (needs `mask-image` or a radial hole); or a render-callback image. A fixed-length arc (a border quarter plus `rotate` linear infinite) is the cheap approximation. It looks like Win10/Bootstrap, not like Win11. |
| **Orbiting dots** | Win10 ProgressRing | 5 dot nodes, each a `rotate` keyframe track with the per-segment splines from section 3.2 (-110° -> 10° -> 93° -> 205° -> 357° -> 439° -> 585°, 3.217s), staggered 0.167s. Needs per-stop timing in `@keyframes`. |
| **Rotating icon** | KDE (2s), old GTK (1s) | `transform: rotate` 0 -> 360° linear infinite on an icon node. Works today. |

**Recommendation.**
- Add `Spinner::style: SpinnerStyle { Native, Spokes, Arc, Dots, Icon }` and a theme dispatch like `Slider::dom()`.
- `Native` picks Spokes on macOS/iOS, Arc on Windows 11 / GNOME / Android, and Icon on KDE.
- Ship **Spokes first**: it is pure CSS today, plus negative delays. It is also exactly what the user asked for ("the macOS spinner with the various rotated dots").
- Default size 16 or 32 to match native (azul's 24 matches nothing). Colour `system:text` at 0.55 on macOS (the sprite is pure black or white, not the accent), accent on Windows / GNOME / M3.
- Update the stale module doc.

### 5.3 Titlebar

**What azul does.** `layout/src/widgets/titlebar.rs`, `css/src/system.rs`, `dll/src/desktop/shell2/common/layout.rs:2163`:

| Issue | Where | Native macOS 15 [L] | Fix |
|---|---|---|---|
| **Bold title renders in Helvetica Neue, not SF** | `system.rs` `macos_fallback_chain`: `UiBold \| TitleBold => [HELVETICA_NEUE, LUCIDA_GRANDE]`, "System Font has no Bold variant in fontconfig" | `titleBarFont` = SF (`.AppleSystemUIFaceHeadline`) 13pt, weight 0.4 = **Bold** | `/System/Library/Fonts/SFNS.ttf` is **one variable font** ("System Font"). Axes: `wght` 1-1000, `opsz` 17-96 (default 28), `wdth`, `GRAD`. It has **369 named instances**, including **Bold (wght 700)** and Semibold (590) (inspected with fontTools). Teach the font resolver to select the `wght = 700` instance, or to apply the axis, for `system:ui:bold` / `system:title:bold` instead of skipping to Helvetica. Also set `opsz` = point size, clamped to 17+. AppKit uses the Text optical size (opsz <= 19) at 13pt; the default instance is opsz 28 (Display), which is tighter and wider-spaced. SFNS also has a `trak` table (size-specific tracking) that AppKit applies. |
| Weight constant | `TitlebarMetrics::macos()` `title_font_weight: 600` | 700 (standard bar); 590/600 at 15pt only for toolbar-style bars | 700 |
| Height (demo) | `examples/azul-widgets/src/lib.rs:876`: `height: 38px` with `WindowDecorations::NoTitle` | 28 for a titlebar-only window; 38/52 *only* with an `NSToolbar` (`.unifiedCompact` / `.unified`), which is what moves the traffic lights to y = 11 / y = 18 | Use 28. If a taller bar is wanted, have the macOS shell attach an empty `NSToolbar` and set `toolbarStyle` so AppKit re-centres the lights. Expose this as a window option (for example `MacTitlebarStyle::{Standard, UnifiedCompact, Unified}`), and have `SystemStyle.metrics.titlebar.height` report 28/38/52 to match. |
| Horizontal centring (demo) | demo: `padding-left: 82px`, title `flex-grow: 1`, left-aligned | standard bar: centred on the **window** width; toolbar styles: left-aligned at x = 80 (compact) / 91 (unified) | Use the `Titlebar` widget, whose title-only mode *is* window-centred through symmetric padding. Or follow the toolbar convention: left, 15pt semibold, x = 80/91. |
| Vertical centring | `build_title_style`: `padding-top = (height - font_size)/2` = 7 for 28/13; the line box is about 16, so the text centre lands at about y = 15 | 16pt line box at y = 5 -> centre y = 13 (1pt *above* the midline) | `display:flex; align-items:center` on the bar, title `line-height: 16px`, optional `margin-top: -1px`. Or `padding-top: (h - 16)/2 - 1`. |
| Traffic-light reserve | `TitlebarMetrics::macos().button_area_width = 78`, padding 8 | lights occupy x = 8..60 (standard), 20..72 (unified), 13..65 (compact); 12pt circles at a 20pt pitch | Fill `button_area_width` and `height` at runtime from `standardWindowButton(.closeButton/.zoomButton).frame` and `superview.frame.height`. AppKit already knows the numbers, including 26's 14pt/23pt lights. |
| **No border-bottom / separator** | `Titlebar` has `background_color`, `background_inactive`, `title_color_inactive`, `button_hover_color`, `close_hover_color`, but no separator; `create_csd_stylesheet` hard-codes `border-bottom: 1px rgb(200,200,200)/(60,60,60)` | 0.5pt #D0D0D0 (+ #E9E9E9) light, 0.5pt #000 (+ #1E1E1E) dark; `titlebarSeparatorStyle` .automatic / .none / .line / .shadow; GNOME: inset 1px shade or `.flat` none; KDE: none | Add `separator_color: OptionColorU`, `separator_color_inactive`, `separator_width: f32` (0.5 on HiDPI macOS, 1 elsewhere) and `with_background(..)`, `with_border_bottom(width, color)` builders. Add matching `TitlebarMetrics.separator_*` fields filled per platform. For the auto-injected bar (`inject_software_titlebar` -> `Titlebar::from_system_style`), expose the same through a window-level option so apps can configure it without building their own bar. |
| Background on macOS | `background_active`/`background_inactive` are never filled by the macOS discovery, so the injected bar is transparent over the window | titlebar material: about #FBFBFB / #383838 active and about #F0F0F0 / #282828 inactive (off-screen fallback; on screen the material is desktop-tinted) | Default `system:window-background` for transparent-titlebar windows (what AppKit shows under a transparent titlebar). Offer a `system:titlebar-background` token with the measured fallbacks. |
| Title colours | `from_system_style`: `tm.text_active` -> `colors.text` -> #4c4c4c / #e5e5e5 | `windowFrameTextColor` black / white 0.847 (vibrant, looks like #4B4B4B / #B4B4B4); inactive about `tertiaryLabelColor` (#B0B0B0 / #696969) | Set `text_active = windowFrameTextColor` and `text_inactive = tertiaryLabelColor` in the macOS discovery. |
| CSD buttons on macOS | `build_button_container` draws icon glyphs (`system:titlebar-close`, ...) in hover boxes, the Windows/Linux idiom | 12pt circles #FF5F57 / #FEBC2E / #28C840 with a darker 0.5pt rim; glyphs (x, -, the fullscreen arrows) appear only while the group is hovered; inactive #D6D6D6 / #4C4C4C; 26: 14pt at a 23pt pitch | For `WindowDecorations::None` + CSD on macOS, render traffic-light circles instead of glyph buttons. |
| Windows 11 | `TitlebarMetrics::windows()`: 32, 138 = 3 x 46, Segoe UI Variable **Text** 12 / 400, **centred** title (the widget centres everywhere) | 32 (48 tall), 46px buttons, **left-aligned** title 16px after a 16px icon, `Caption` 12/16 Regular, Mica or #F3F3F3 / #202020, close hover #C42B1C, 8px radius | Add a `title_align` field (`Center` for macOS / GNOME / KDE, `Left` for Windows). Button hover colours #09000000 / #0FFFFFFF, close #C42B1C. |
| GNOME / KDE | `TitlebarMetrics::linux_gnome()`: **35px, Cantarell 11px bold**; KDE reuses the GNOME metrics | GNOME: **46px, bold 11pt (14.67px)**, 20/24px round buttons, 15px window radius (48+). KDE: about 28px, Noto Sans 10pt regular, centred, #dee0e2 / #292c30 | Fix the numbers: 11pt means 14.67px, not 11px. Add a `linux_kde()` metrics set. |

---

## 6. The other widgets: desktop specs vs azul

Each table's last row is azul's current default, with its file. Unless a column says "D", colours are light-mode.

In the GNOME column, *cc N%* means `color-mix(currentColor N%)`, and "6/9" means radius 6px on GNOME 45-47 and 9px on 48+.

### 6.1 Push button

Images: [macOS 15 light](../target/native-reference/macos-15-button-push-light@4x.png) / [dark](../target/native-reference/macos-15-button-push-dark@4x.png), [gallery](../target/native-reference/macos-15-controls-gallery-light.png) / [dark](../target/native-reference/macos-15-controls-gallery-dark.png), [GNOME raised](../target/native-reference/gnome-49-button-raised-light.png) / [suggested](../target/native-reference/gnome-49-button-suggested-light.png) / [flat](../target/native-reference/gnome-49-button-flat-light.png), [Win11 controls](../target/native-reference/windows-11-controls-light.png).

| Platform | Height (reg/small/mini/large) | Radius | Face L / D | Border / shadow | Font | States | Src |
|---|---|---|---|---|---|---|---|
| macOS 15 | **20** / 16 / 13 / 28 (alignment rect; frame 32/27/16/40) | about 5 | white / white 25% (#656565 on #323232) | hairline + 0.5pt shadow | 13 / 11 / 9 regular | default button = accent fill, white text | [L] |
| macOS 26 | 24 / 20 / 16 / 28 / **36 XL** | 6 / 5 / 4; **capsule** for large+ | black 5% / white 7%; pressed 15% / 16% | -- | 13/11/10 at weight 500 | prominent = accent; destructive = red 25% fill | [A12] [A6] |
| Win11 | **32** (padding 11,5,11,6) | 4 | `ControlFillColorDefault` #B3FFFFFF / #0FFFFFFF -> Secondary (hover) -> Tertiary (pressed) | 1px `ControlElevationBorderBrush` (bottom #29000000) | 14 Segoe UI Variable | 83 ms brush transition; accent button `AccentFillColor*` | [W33] |
| Win10 | about 32 (padding 8,4,8,5) | 0 | #33000000 | 2px, hover #66000000 | 14 | -- | [W7] |
| GNOME | **34** (24 + 5 + 5) | 6/9 | cc 10% (hover 15%, active 30%) | none | **bold** | `.suggested-action` accent; focus 2px inset accent 50% | [G6] |
| KDE | about 34 | 4.5 | #fcfcfc / #292c30 | 1px outline #c6c8c9-ish + 1px bottom shadow | regular | hover outline #3daee9 | [K1] [K2] |
| **azul** (`button.rs:300-360`) | about 29 (padding 6/12 + text + 2 border) | **4** | per `ButtonType` palette | 1px | inherits | hover/active via `button_states` | -- |

### 6.2 Checkbox and radio

Images: [macOS 15 checkbox on](../target/native-reference/macos-15-checkbox-on-light@4x.png) / [off](../target/native-reference/macos-15-checkbox-off-light@4x.png) / [dark](../target/native-reference/macos-15-checkbox-on-dark@4x.png), [radio](../target/native-reference/macos-15-radio-on-light@4x.png), [macOS 26 checkbox](../target/native-reference/macos-26-checkbox-on-light.png) / [mixed](../target/native-reference/macos-26-checkbox-mixed-light.png) / [radio](../target/native-reference/macos-26-radio-on-light.png), [GNOME checkbox](../target/native-reference/gnome-47-checkbox-hig-light.png) / [radio](../target/native-reference/gnome-47-radio-hig-light.png), [KDE checkbox](../target/native-reference/kde-plasma6-checkbox-hig.png) / [radio](../target/native-reference/kde-plasma6-radio-hig.png).

| Platform | Box | Radius | Off | On | Glyph | Src |
|---|---|---|---|---|---|---|
| macOS 15 | **14** (small 12, mini 10) | about 3 | white + #C1C1C1 border / white 20% | accent (#3897FF measured, a light vertical gradient) | white check; radio: 14pt circle with a white centre dot of about 6pt | [L] |
| macOS 26 | 16 / 14 / 12 / 18 | 5.5 / 4.5 / 3.5 / 6.5 | **flat black 5%, no border** | accent | check 9.3x8.9; radio dot 4.8 | [A12] [A5] |
| Win11 | **20** | 4 | 1px `ControlStrongStrokeColorDefault` #72000000 + fill #06000000 | accent fill + stroke | E73E, 12px; radio dot 12 rest / 14 hover / 10 pressed (250 ms) | [W33] |
| GNOME | 14 + pad 3 = 20 | 6 (radio 100%) | inset 2px ring cc 15% | accent-bg, white `check-symbolic` | 14px | [G14] |
| KDE | 16 (metric 20) | 4 | Button + 1px #c6c8c9 | Highlight border, fill Button + Highlight 30% | 2px stroke; radio dot 6 | [K1] [K2] |
| **azul** (`check_box.rs:132-160`, `radio_group.rs:138-146`) | 14 (checkbox), **16** radio, 8 dot | -- | 1px border | -- | -- | -- |

### 6.3 Switch

Images: [macOS 15 on](../target/native-reference/macos-15-switch-on-light@4x.png) / [off](../target/native-reference/macos-15-switch-off-light@4x.png) / [dark](../target/native-reference/macos-15-switch-on-dark@4x.png), [GNOME](../target/native-reference/gnome-47-switch-hig-light.png), [KDE](../target/native-reference/kde-plasma6-switch-hig.png).

| Platform | Track | Knob | Colours | Motion | Src |
|---|---|---|---|---|---|
| macOS 15 | **38x22** (small 32x18, mini 26x15) | about 20, white (dark white 75% = #CCCCCC) | on = accent; off = black about 9% | about 0.2 s (unmeasured) | [L] |
| macOS 26 | 44x20 regular (mini 36x16 ... XL 80x36) | capsule 26x16 regular | thumb white 87% / 85%; off black 6% / white 10% | knob stretches on press, 170 ms ease-out | [A12] |
| Win11 | **40x20**, r 10, 1px stroke | **12 rest / 14 hover / 17x14 pressed** | off: stroke #72000000, knob `TextFillColorSecondary`; on: accent, knob white | 83 ms | [W33] |
| GNOME | **46x26** (knob 20, pad 3) | 20, white, `0 2px 4px rgba(0,0,6,.2)` | off cc 15%; on accent | 100 ms ease-out-cubic | [G14] [G15] |
| KDE (qqc2) | 36x18, 12px track | 18 | on: Highlight border + 50% fill | 100 ms | [K9] |
| **azul** (`switch.rs:107-114`) | **36x20**, r 10, padding 2 | 16 | -- | spring 150 ms (`switch.rs:155`) | -- |

### 6.4 Text field, text area, search

Images: [macOS 15](../target/native-reference/macos-15-textfield-light@4x.png), [macOS 14 HIG](../target/native-reference/macos-14-textfield-light.png), [GNOME entry](../target/native-reference/gnome-47-entry-hig-light.png), [GNOME entry row](../target/native-reference/gnome-49-entryrow-light.png).

| Platform | Height | Radius | Background | Border | Focus | Src |
|---|---|---|---|---|---|---|
| macOS 15 | **21** / 19 / 15; search 22 / 19 / 17 (capsule) | 0 (square bezel) | `textBackgroundColor` #FFFFFF / #1E1E1E | 1px #B1B1B1 outer + #E0E0E0 inner (HIG, measured) | 3pt `keyboardFocusIndicatorColor` ring | [L] [A2] |
| macOS 26 | 24 regular (16 ... 36) | 4 / 5 / 6 / 7 / 9 | #FFF / #1E1E1E | 1px black 8% / white 4% | accent 25% 3.5pt + 1px accent 15% hairline | [A12] [A13] |
| Win11 | **32** (min width 64), padding 10,5,6,6 | 4 | `ControlFillColorDefault`; focused #FFFFFF / #B31E1E1E | bottom 1px #72000000, others #0F000000; **focused: 2px accent bottom** | as border | [W33] |
| GNOME | **34**, padding 0 9px | 6/9 | cc 10% (no border) | none | 2px inset accent 50% | [G14] |
| KDE | about 30 | 4.5 | View #ffffff / #141618 | 1px mix(Window, Text, .2) | outline #3daee9 | [K1] [K2] |
| **azul** (`text_input.rs:84-124`) | min **22** | -- | -- | 1px | -- | -- |

### 6.5 Pop-up / dropdown, combo box, segmented control, stepper / number field

Images: [macOS 15 pop-up](../target/native-reference/macos-15-popup-button-light@4x.png), [macOS 15 segmented](../target/native-reference/macos-15-segmented-light@4x.png) / [dark](../target/native-reference/macos-15-segmented-dark@4x.png), [macOS 14 segmented HIG](../target/native-reference/macos-14-segmented-selectone-light.png), [GNOME toggle group](../target/native-reference/gnome-49-togglegroup-light.png) / [dark](../target/native-reference/gnome-49-togglegroup-dark.png), [GNOME linked](../target/native-reference/gnome-49-linked-light.png), [GNOME spin button](../target/native-reference/gnome-47-spinbutton-hig-light.png), [GNOME spin row](../target/native-reference/gnome-49-spinrow-light.png), [KDE spinbox](../target/native-reference/kde-plasma6-slider-spinbox-hig.png).

| Widget | macOS 15 [L] | macOS 26 | Win11 | GNOME | KDE | azul |
|---|---|---|---|---|---|---|
| Pop-up / dropdown | **20** (16/13/28), r about 5, white face, **accent 16x16 square with up/down chevrons** at the right | button metrics, chevron 7x12 @ 1.5 stroke; menu rows 24/22/20 | 32, r4, chevron E70D 12px `TextFillColorSecondary`; list items with a 3x16 accent pill | 34 button + `pan-down-symbolic` 16 | about 32 | `drop_down.rs` (no fixed metrics) |
| Pull-down | 20, single down chevron in the accent square | -- | -- | menu button | -- | -- |
| Combo box | 20 (16/13), text field + accent chevron button | button width 30/25/22 | editable text padding 11,5,38,6 | entry + menubutton (linked) | line edit + 20px arrow | `combobox.rs` |
| Segmented | **22** (18/15/30), r about 6, selected segment = white raised pill, 1pt separators | accent-selected, container black 5%, capsule for large | SelectorBar: no fill, 3px accent underline pill | AdwToggleGroup: bg cc 10% r9 pad 3; toggles 28 r6 bold; checked white + shadow | checkable tool buttons | `segmented.rs:160` r6, padding 6/12 |
| Stepper (+/-) | **13x20** (11x16, 9x13), two chevrons stacked | widths 13 ... 30 | NumberBox inline buttons 32 wide, E70E/E70D | +/- flat buttons 22 wide, separators 10% | 20px arrow column | `number_input.rs`. azul's `stepper.rs` is a **wizard step indicator** (28px circles), not a +/- stepper |
| Date picker | textual 24 tall + stepper (`NSDatePicker` fitted 160x28) | -- | CalendarDatePicker 32; flyout rows 40 | GtkCalendar, selected day accent r6/9 | -- | `date_picker.rs` |
| Colour well | 38x24 (frame 44x28), rounded, inner swatch | `.default` / `.minimal` / `.expanded` styles (13+) | ColorPicker, slider radius 6 | `button.color`, swatch radius = button - 4.5 | -- | `color_input.rs` |

### 6.6 Progress bar

Images: [macOS 15](../target/native-reference/macos-15-progressbar-light@4x.png), [macOS 14 HIG determinate](../target/native-reference/macos-14-progressbar-determinate-light.png) / [indeterminate](../target/native-reference/macos-14-progressbar-indeterminate-light.png) / [circular](../target/native-reference/macos-14-progress-circular-light.png), [Win11 determinate](../target/native-reference/windows-11-progressbar-determinate.png) / [indeterminate GIF](../target/native-reference/windows-11-progressbar-indeterminate.gif), [GNOME](../target/native-reference/gnome-47-progressbar-hig-light.png) / [OSD](../target/native-reference/gnome-49-progressbar-osd-light.png).

| Platform | Track | Radius | Colours | Indeterminate | Src |
|---|---|---|---|---|---|
| macOS 15 | **6pt** (frame 18 / 12) | 3 (capsule) | fill accent #0A82FF; track black about 10% / white about 12% | animated (not measured here) | [L] [A2] |
| macOS 26 | regular 10, small 6 | capsule | accent | comet ping-pongs, 1.9 s per cycle | [A12] |
| Win11 | **1px track**, 3px indicator | 0.5 / 1.5 | track #72000000; fill accent; paused #9D5D00; error #C42B1C | two bars (0.4W and 0.6W) sweep, 2 s period, spline (0.4,0,0.6,1) | [W33] |
| GNOME | **8px** | 99 | trough cc 15%, fill accent | -- | [G14] |
| KDE | 6px | 3 | groove 14%, fill highlight 70% | 14px stripes, 800 ms per step | [K1] |
| **azul** (`progressbar.rs:242`) | **15px** | -- | -- | -- | -- |

### 6.7 Tabs, list/table rows, tree disclosure, scrollbars, split divider

Images: [macOS 15 tab view + scrollers + split views](../target/native-reference/macos-15-tabs-table-scroller-split-light.png) / [dark](../target/native-reference/macos-15-tabs-table-scroller-split-dark.png), [macOS 15 table rows](../target/native-reference/macos-15-table-rows-light.png) / [dark](../target/native-reference/macos-15-table-rows-dark.png), [macOS 26 disclosure](../target/native-reference/macos-26-disclosure-triangle-light.png), [Win11 TabView](../target/native-reference/windows-11-tabview-mica.png), [GNOME tab bar](../target/native-reference/gnome-49-tabbar-light.png), [GNOME boxed list](../target/native-reference/gnome-49-boxedlist-light.png) / [dark](../target/native-reference/gnome-49-boxedlist-dark.png), [GNOME expander row](../target/native-reference/gnome-49-expanderrow-light.png).

| Widget | macOS 15 [L] | Win11 | GNOME | KDE | azul |
|---|---|---|---|---|---|
| Tabs | `NSTabView` = a **segmented control centred on the top edge** of a rounded content box (content inset 10) | TabView: 32 tabs + 8 strip = 40; selected tab radius 8 top, bg #F9F9F9 / #282828 | AdwTabBar 34, selected cc 10%, r6/9 | 30-34 tall, **3px highlight bar** | `tabs.rs:123` height 21 |
| Table rows | row height **24** (medium), header 28, intercell 17x0. `.inset` style: first row at y = 10, **selection pill inset 10pt, r about 5**. Selected = `selectedContentBackgroundColor` #0064E1 / #0059D1 (unfocused #DCDCDC / #464646); alternating #FFFFFF / #F4F5F5 | ListView min 40, r4, hover #09000000, selected `SubtleFillSecondary` + 3x16 accent pill; TreeView 28, indent 16 | ActionRow 50, boxed list r12 card; plain rows hover 4%, selected accent 25% | selected = solid highlight | `list_view.rs`, `tree_view.rs` |
| Disclosure | triangle 13x13 (chevron `>` rotates to `v`); `roundedDisclosure` button 20x20 | chevrons E76C / E70D, 8px glyph in 12x12 | `pan-end` -> `pan-down` 16px; expander row rotates 0.5 turn in 200 ms | 10px chevron | -- |
| Scrollbar | legacy **15** (small 11); overlay container 16, knob 7 -> 11 on hover, black/white 50% | 12 wide; 2px visible contracted -> 6px expanded; arrows fade in; 400 ms delay, 167 ms expand | overlay: 3px idle -> 8px hover, 20/40/60% | 21px non-overlay, 8px handle | `SystemStyle.scrollbar` |
| Split divider | `.thin` **1**, `.thick` **9** (with a centre dimple), `.paneSplitter` **10** | pane 320 / compact 48 | 1px line (`.wide` 5) | 1px + 12px invisible grab | `split_pane.rs:170` **6** |

### 6.8 Popover, dialog, menu, tooltip, accordion

Images: [macOS 26 alert](../target/native-reference/macos-26-alert-light.png) / [dark](../target/native-reference/macos-26-alert-dark.png), [macOS 26 popover](../target/native-reference/macos-26-popover-light.png), [macOS 26 menu](../target/native-reference/macos-26-titlebar-notes-toolbar-menu-light.png), [GNOME popover menu](../target/native-reference/gnome-49-popovermenu-light.png), [GNOME popover](../target/native-reference/gnome-47-popover-hig-light.png), [GNOME alert](../target/native-reference/gnome-49-alertdialog-light.png) / [floating dialog](../target/native-reference/gnome-49-dialog-floating-light.png), [Win11 focus visual redlines](../target/native-reference/windows-11-focusvisual-redlines.png).

| Widget | macOS | Win11 | GNOME | KDE | azul |
|---|---|---|---|---|---|
| Menu | font 13pt (`menuFont` [L]); rounded item highlight r about 5 since 11; 26: leading SF-symbol column, rows 24/22/20 | presenter r8 acrylic, padding 0,2; items margin 4,2, r4, padding 11,8,11,9, 14px | min-height 32, padding 0 12px, r6/9, hover 10% | items 4/4 margins | `menubar.rs` |
| Popover | 26: about 20pt radius (low confidence) | Flyout r8, padding 16,15,16,17, acrylic, elevation 32 | r12 (<= 47) / 15 (48+), bg #fff / #36363a, arrow 24x12 | r5 | `popover.rs` |
| Dialog / alert | 26: stacked capsule buttons, accent primary | ContentDialog 320-548 wide, r8, padding 24, title 20 semibold, smoke #4D000000 | AlertDialog r18, 300-372 wide, spring open (damping .62) | -- | `modal.rs` |
| Tooltip | `toolTipsFont` = **11pt** [L] | 12px, padding 9,6,9,8, r4, acrylic, max width 320 | rgba(0,0,6,.8) bg, white text, r9, padding 6 10 | ToolTipBase #f7f7f7 / #292c30, r5 | `tooltip.rs:60-123`: r4, 12px |
| Accordion | `NSDisclosureButton` + group box; SwiftUI `DisclosureGroup` (chevron rotates) | Expander: min 48, r4, card bg #B3FFFFFF; expand 333 ms `0,0,0,1`, collapse 167 ms | AdwExpanderRow (arrow 200 ms) | -- | `accordion.rs:203-312`: r6, 14px |
| Focus ring (all) | `keyboardFocusIndicatorColor` accent 50%, 3-3.5pt, follows the control shape | 2px outer (#E4000000 / #FFF) + 1px inner (#B3FFFFFF / #B3000000), margin 1-3px | 2px accent 50%, offset 0 (inset -2 on buttons) | outline recolour to #3daee9 | none by default (no `outline` property) |

### 6.9 Mobile: iOS 17/18 -> 26 and Material 3

Images:
- iOS 26 overview: [iOS 26](../target/native-reference/ios-26-controls-overview-light.png) vs [iOS 17.5](../target/native-reference/ios-26-controls-overview-ios17baseline-light.png);
- iOS 17: [switch form](../target/native-reference/ios-17-switch-insetgrouped-light.png), [segmented](../target/native-reference/ios-17-segmented-light.png), [stepper](../target/native-reference/ios-17-stepper-light.png), [text field](../target/native-reference/ios-17-textfield-roundedrect-light.png), [alert](../target/native-reference/ios-17-alert-light.png), [context menu](../target/native-reference/ios-17-contextmenu-light.png), [tab items](../target/native-reference/ios-17-tabbar-items-light.png), [date pill](../target/native-reference/ios-17-datepicker-compact-light.png);
- iOS 26: [switch on](../target/native-reference/ios-26-switch-on-light.png), [buttons](../target/native-reference/ios-26-button-light.png), [glass button](../target/native-reference/ios-26-glass-text-button-light.png), [tab bar](../target/native-reference/ios-26-tabbar-light.png), [minimised tab bar](../target/native-reference/ios-26-tabbar-minimized-light.png), [menu](../target/native-reference/ios-26-menu-light.png), [menu sizes](../target/native-reference/ios-26-menu-sizes.png), [alert](../target/native-reference/ios-26-alert-light.png), [stacked alert](../target/native-reference/ios-26-alert-stacked-light.png), [sheet](../target/native-reference/ios-26-sheet-medium-light.png), [wheel picker](../target/native-reference/ios-26-picker-wheel-light.png), [glass materials](../target/native-reference/ios-26-glass-material-regular-over-light.png);
- M3: [switch](../target/native-reference/android-m3-switch-anatomy.png), [connected button group](../target/native-reference/android-m3-buttongroup-connected.png).

| Widget | iOS 17/18 | iOS 26 | Material 3 |
|---|---|---|---|
| Button | bordered 34, large about 50; rounded rect; tinted bg alpha .12 / .26 (F); pressed opacity .4 | **capsule** by default; `.glass`, `.glassProminent`; bordered 38pt | baseline **40dp**, full radius; Expressive XS-XL 32/40/56/96/136; label-large 14/20 medium; icon 18; hover raises to level 1; disabled container 12%, label 38% |
| Checkbox / radio | none | none | box **18dp**, r2, 2dp outline #49454F; radio 20dp ring 2dp, dot 10dp; 40dp state layer |
| Switch | **51x31**, thumb 27, on #34C759 / #30D158, off secondarySystemFill | **63x28**, knob **37x24 pill**, 22pt travel; the press lens is 59x40 | track **52x32**, knob 16 (off) / 24 (on) / 28 (pressed); off track surface-container-highest with a 2dp outline |
| Text field | roundedRect **34pt**, r5, 1px (207,207,207); body 17 | unchanged | **56dp**; filled (top r4, surface-container-highest, 1 -> 2dp indicator) or outlined (r4, 1 -> 2dp primary); label body-large -> body-small |
| Search | 36pt, r9, tertiarySystemFill | bottom glass capsule, about 48pt | -- |
| Segmented | **32pt**, r9, thumb r7, 2pt inset, white thumb + `0 3 8 α.12` shadow; 13pt medium | 32pt capsule, white capsule thumb 28 tall, glass while dragged | segmented buttons **40dp**, full radius, 1dp outline, 18dp check |
| Stepper | about **94x32**, r about 8, tertiarySystemFill, 1x18 divider | changed, no metrics | none (text field + icon buttons) |
| Progress bar | 4pt capsule (bar style 2.5) | 4pt capsule, #0088FF on about #E4E4E5 | **4dp** + 4dp gap + 4dp stop dot; indeterminate 1750 ms (Compose); wavy variant |
| Tabs | tab bar **49pt**, icons 25, labels 10pt medium | **floating glass capsule 62pt**, 21pt from the edges, selected pill about 94x54, minimises to 48 | primary tabs **48dp** (64 with icon), **3dp indicator with 3dp top corners**; nav bar 80dp with a 64x32 pill |
| List rows | **44pt**, inset grouped r **10** | **52pt**, r **26** | **56 / 72 / 88dp** |
| Disclosure | chevron about 7x11.5, tertiaryLabel | -- | (no tree; expand icon rotates 180°) |
| Date / time | wheel 216pt, rows 32, band tertiarySystemFill r8 | band becomes a 34pt capsule | modal 360x568, r28, 40dp day circles; time dial 256dp |
| Tooltip | none on iPhone | none | plain **r4**, inverse-surface, body-small 12/16, 24dp min height; rich r12 |
| Alert / dialog | **270pt** wide, r13-14, 44pt button rows | about 290-300 wide, r **about 33.5**, **capsule buttons 46-48** | r **28**, surface-container-high, width 280-560, padding 24, headline-small |
| Menu | width 250, r **13**, rows 44 | r **about 34**, rows 44, morphs from the button | r **4**, items **48dp**; Expressive r16, items 44dp |
| Scroll indicator | 3pt (8 while dragging), 35% black / 50% white, fades after 1.2 s | -- | Android 4dp, fade 250 ms after 400 ms |
| Split divider | not found | -- | drag handle 4x48dp, 12x52 while pressed |

**Foundation tokens.**
- iOS:
  - systemFill (120,120,128) at .20 / .36; separator (60,60,67) at .29 / (84,84,88) at .6; label tiers .6 / .3 / .18.
  - Type: body 17/22, headline 17 semibold, footnote 13/18, caption 12/16.
  - Default animation: spring(response 0.55, damping 1.0).
- iOS 26: controls are capsules; lists 26; menus and alerts about 34; sheet top about 36. Corners follow the concentric rule `inner = max(outer - inset, min)` [I29][I30].
- M3:
  - Colours: primary #6750A4 / #D0BCFF, surface #FEF7FF / #141218, on-surface #1D1B20 / #E6E0E9, outline #79747E / #938F99.
  - Shape scale 0/4/8/12/16/20/28/32/48/full.
  - State layers: hover 8%, focus and press 10%, drag 16%. Disabled: content 38%, container 12%.
  - Web focus ring: 3px secondary, 2px offset.
  - Easing: emphasized (0.2,0,0,1), standard (0.2,0,0,1), legacy (0.4,0,0.2,1).
  - Springs: expressive spatial default .8 / 380; effects default 1 / 1600.

Full tables are in the iOS/M3 research notes. Sources are [I1]-[I32] and [M1]-[M9].

---

## 7. `system:` tokens and `SystemStyle` fields: coverage and gaps

### 7.1 Existing colour tokens vs platform sources

azul has 24 `SystemColorRef`s (`css/src/props/basic/color.rs:1460`). Each resolves against `SystemStyle.colors` and falls back to `SystemColorRef::fallback(dark)`.

The fallbacks already match macOS 15 [L] closely:
- #ECECEC / #323232 window;
- label 0.85;
- #0064E1 / #0058D0 selection (native dark #0059D1);
- #B3D7FF / #3F638B text selection.

The **`defaults::macos_modern_light/dark()`** base styles (`css/src/system.rs:2593/2644`) do *not* match. They are the values used when discovery fails, and under `AZ_THEME` pins:

| Field | Base style (light / dark) | Native 15 [L] (light / dark) |
|---|---|---|
| window background | **#FFFFFF** / #2C2C2E | #ECECEC / #323232 |
| background | **#F2F2F7** (an iOS grouped-background colour) | #FFFFFF / #1E1E1E |
| selection | **accent @ 50%** | #0064E1 / #0059D1 (opaque) |
| text alpha | 221 | 216 |
| `corner_radius` | 8 | about 5 on push buttons |

Macros used in the table:
- `q!` = macOS discovery reads it unconditionally;
- `q_if_known!` = macOS reads it only if the selector exists;
- `GetSysColor(n)` = the Windows discovery.

| Token | macOS source (read by azul?) | macOS 15 L / D [L] | Windows (read by azul?) | GNOME / libadwaita | KDE |
|---|---|---|---|---|---|
| `system:text` | `labelColor` (yes) | #000 .847 / #FFF .847 | `GetSysColor(8)` (yes); WinUI `TextFillColorPrimary` #E4000000 / #FFF | `--window-fg-color` | `[Colors:Window] ForegroundNormal` #232629 / #fcfcfc |
| `system:secondary-text` | `secondaryLabelColor` (yes) | .498 / .549 | `TextFillColorSecondary` #9E000000 / #C5FFFFFF (not read) | 55% dim | ForegroundInactive #707d8a / #a1a9b1 |
| `system:tertiary-text` | `tertiaryLabelColor` (yes) | .259 / .247 | `TextFillColorTertiary` #72000000 / #87FFFFFF | -- | -- |
| `system:disabled-text` | `disabledControlTextColor` (yes) | .247 / .247 | `GetSysColor(17)` (yes) | 50% opacity | ColorEffects:Disabled |
| `system:placeholder-text` | `placeholderTextColor` (yes) | .247 / .247 | `TextFillColorSecondary` | 55% | -- |
| `system:accent` | `controlAccentColor` (yes) | #007AFF (26: #0088FF) | **`DwmGetColorizationColor`** (yes). That is the *frame colourisation* with alpha, not the UI accent. Read `UISettings.GetColorValue(Accent)` instead | portal `accent-color` / `--accent-bg-color` #3584e4 | `[General] AccentColor` / Selection #3daee9 |
| `system:accent-text` | `alternateSelectedControlTextColor` (yes) | #FFF | `TextOnAccentFillColorPrimary` #FFF / **#000** (dark text on Light2 accent) | `--accent-fg-color` #fff | ForegroundActive |
| `system:button-face` | `controlColor` (yes) | #FFF / white .247 | `GetSysColor(15)`; WinUI `ControlFillColorDefault` #B3FFFFFF / #0FFFFFFF | cc 10% | `[Colors:Button]` #fcfcfc / #292c30 |
| `system:button-text` | `controlTextColor` (yes) | .847 | `GetSysColor(18)` | -- | -- |
| `system:window-background` | `windowBackgroundColor` (yes) | #ECECEC / #323232 | `GetSysColor(5)`; WinUI `SolidBackgroundFillColorBase` #F3F3F3 / #202020 | #fafafb / #222226 (48+) | Window #eff0f1 / #202326 |
| `system:background` | `textBackgroundColor` (yes) | #FFF / #1E1E1E | -- | view #fff / #1d1d20 | View #fff / #141618 |
| `system:control-background` | `controlBackgroundColor` (yes) | #FFF / #1E1E1E | `ControlFillColorInputActive` | -- | View |
| `system:selection-background` | `selectedContentBackgroundColor` (yes) | #0064E1 / #0059D1 | `GetSysColor(13)` or the accent | accent 25-39% (lists) | Selection #3daee9 |
| `system:selection-background-inactive` | `unemphasizedSelectedContentBackgroundColor` (yes) | #DCDCDC / #464646 | -- | view-fg 10% | -- |
| `system:text-selection-background` | `selectedTextBackgroundColor` (yes) | follows the user's highlight colour (#B3D7FF / #3F638B for blue) | `SystemAccentColor` | accent-bg 30% | Selection |
| `system:separator` | `separatorColor` (yes) | #000 .098 / #FFF .098 | `GetSysColor(16)` (yes); WinUI `DividerStrokeColorDefault` #0F000000 / #15FFFFFF | shade rgba(0,0,6,.07) | mix(Window, Text, .2) |
| `system:grid` | `gridColor` (yes) | #E6E6E6 / #1A1A1A | -- | -- | -- |
| `system:link` | `linkColor` (yes) | #0068DA / #419CFF | `GetSysColor(26)` | accent | #2980b9 / #1d99f3 |
| `system:find-highlight` | `findHighlightColor` (yes) | #FFFF00 | -- | -- | -- |
| `system:under-page-background` | `underPageBackgroundColor` (yes) | #969696 .9 / #282828 | -- | -- | -- |
| `system:sidebar-background` / `-selection` | not published (a material) | -- | -- | `--sidebar-bg-color` | -- |
| `system:selection-text` / `-inactive` | `selectedTextColor` / `unemphasizedSelectedTextColor` (yes) | -- | `GetSysColor(14)` | -- | -- |

### 7.2 Missing tokens (needed by native slider, spinner, titlebar and the other controls)

| Proposed token | Purpose | macOS source [L] | Windows | GNOME | KDE |
|---|---|---|---|---|---|
| `system:focus-ring` | knob / field focus ring | `keyboardFocusIndicatorColor` #0067F4 .498 / #1AA9FF .498. **Already read** into `focus_visuals.focus_ring_color`, but not addressable from CSS | `FocusStrokeColorOuter` #E4000000 / #FFF (+ inner #B3FFFFFF / #B3000000) | accent @ 50% | DecorationFocus |
| `system:control-fill` (+ `-secondary`, `-tertiary`) | slider rail, switch off-track, flat button face, 26 checkbox | `systemFillColor` black .098 / white .098; secondary .078; tertiary .047; quaternary .027; quinary .008 (14+) | `ControlStrongFillColorDefault` #72000000 / #8BFFFFFF (rail); `ControlAltFillColorSecondary` | cc 15% (trough), cc 10% (buttons) | WindowText .14 |
| `system:knob` | slider and switch knob face | `controlColor` (#FFF / white .247); the slider knob uses white / white .5 | `ControlSolidFillColorDefault` #FFF / #454545 | `$slider_color` #fff / #d2d2d2 | Button |
| `system:quaternary-text` | tick marks (26), faint glyphs | `quaternaryLabelColor` .098 | -- | -- | -- |
| `system:accent-dark-1`, `system:accent-light-2` (or `system:accent-fill`) | WinUI fills use Dark1 (light) / Light2 (dark), **not** the base accent | = accent | `UISettings.GetColorValue(AccentDark1/AccentLight2)` | -- | -- |
| `system:titlebar-background` / `-inactive` | CSD / injected titlebar | material; measured #FBFBFB / #383838, inactive #F0F0F0 / #282828 | Mica fallback #F3F3F3 / #202020 | `--headerbar-bg-color` #fff / #2e2e32 | `[Colors:Header]` #dee0e2 / #292c30 (inactive #eff0f1 / #202326) |
| `system:titlebar-text` / `-inactive` | title | `windowFrameTextColor` .847; inactive about `tertiaryLabelColor` | `TextFillColorPrimary`; inactive Tertiary | `--headerbar-fg-color`; backdrop 50% | Header ForegroundNormal / Inactive |
| `system:titlebar-separator` | bar bottom edge | measured #D0D0D0 / #000 (0.5pt) | none (Mica) | `--headerbar-shade-color` rgba(0,0,6,.12) / (.36) | none |
| `system:alternate-row` | zebra rows | `alternatingContentBackgroundColors[1]` #F4F5F5 / white .047 | -- | -- | View BackgroundAlternate #f7f7f7 / #1d1f22 |
| `system:header-text` | table headers | `headerTextColor` | -- | -- | -- |
| `system:critical` / `-caution` / `-success` | destructive buttons, error bars | `systemRedColor` #FF3B30 / #FF453A, green #28CD41 / #32D74B | `SystemFillColorCritical` #C42B1C / #FF99A4 ... | `--error-bg-color` ... | Negative #da4453 ... |

### 7.3 Missing `SystemStyle` fields and discovery calls

| Field | Why | Where to read it |
|---|---|---|
| `SystemFonts.title_font` / `title_font_size` / **`title_font_weight`**, `menu_font(_size)`, `small_font_size`, `tooltip_font_size` | the title must be SF Bold 13 and tooltips 11 | `NSFont titleBarFontOfSize:0` (13, weight 0.4) [L], `menuFontOfSize:0` (13), `smallSystemFontSize` (11), `toolTipsFontOfSize:0` (11), `labelFontSize` (10), `systemFontSizeForControlSize:` (13/11/9/13). Windows `SPI_GETNONCLIENTMETRICS` lfCaptionFont / lfMenuFont / lfStatusFont. GNOME `titlebar-font` 'Adwaita Sans Bold 11'. KDE `[WM] activeFont` |
| `TitlebarMetrics.height`, `button_area_width`, `padding_horizontal` **at runtime** | 28 / 38 / 52 on 15; lights 14pt / 23pt on 26 | macOS: `standardWindowButton:` frames + superview height [L]. Windows: `AppWindowTitleBar.LeftInset/RightInset/Height`, `DwmGetWindowAttribute(DWMWA_CAPTION_BUTTON_BOUNDS)`. GNOME: 46 (libadwaita). KDE: font-derived |
| `TitlebarMetrics.separator_color` (+ inactive), `title_align` | section 5.3 | as in 7.2 |
| `TitlebarMetrics.window_corner_radius` | CSD shadow and clip: 10 (macOS <= 15), 16/26 (26), 20 (27), 8 (Win11), 12/15 (GNOME), 5 (KDE) | OS version table; Windows `DWMWA_WINDOW_CORNER_PREFERENCE` |
| `SystemMetrics.control_size_heights` (mini/small/regular/large/xl) | native control heights | macOS table in section 6 [L]; 26: 16/20/24/28/36 |
| `accent_palette: [Dark3..Light3]` | WinUI fills | `UISettings.GetColorValue` 2..8 |
| `animation.duration_factor` from KDE `AnimationDurationFactor`, GNOME `enable-animations` | motion | kdeglobals `[KDE]`, gsettings |

---

## 8. Prioritised change list

Effort: S = one file or constant, M = a widget rework, L = an engine feature.

### P0: the three things the user complained about

1. **Titlebar font (S-M).** Make `system:ui:bold` / `system:title:bold` resolve to SF Pro Bold on macOS.
   - The font is `/System/Library/Fonts/SFNS.ttf` ("System Font"): a variable font with axes `wght` 1-1000, `opsz` 17-96, `wdth` and `GRAD`, and 369 named instances. Bold is wght 700; Semibold is 590.
   - Either select the named instance or apply the `wght` axis in the font loader, instead of the Helvetica Neue fallback in `css/src/system.rs` `macos_fallback_chain`.
   - Also set `opsz` from the point size, which gives Text-size SF at 13pt.
   - Verify by rendering "Window Title" at 13px bold beside [the native render](../target/native-reference/macos-15-titlebar-standard-light.png).
2. **Titlebar metrics (S).** In `TitlebarMetrics::macos()`:
   - set `title_font_weight` to 700;
   - keep the height at 28 and the button area at about 60 + 8;
   - add a `title_align` field (`Center` on macOS / GNOME / KDE, `Left` on Windows).
   
   In `Titlebar::build_title_style`, centre vertically with flex and a 16px line box (native text centre = bar centre - 1pt). In `examples/azul-widgets`, replace the hand-rolled 38px left-aligned bar with `Titlebar`, or make it 28px, centred and in `system:title:bold`.
3. **Titlebar background and border-bottom (S-M).**
   - Add `separator_color`, `separator_color_inactive` and `separator_width` to `Titlebar` and `TitlebarMetrics`, and `with_background` / `with_border_bottom` builders.
   - Let the auto-injected bar (`inject_software_titlebar`) take them from a window option.
   - In the macOS discovery, fill `background_active` / `background_inactive` / `text_active` / `text_inactive` / separator. Measured: #FBFBFB / #383838 bar, #D0D0D0 / #000 0.5pt line, `windowFrameTextColor`, and tertiary text for inactive.
   - Replace the hard-coded `border-bottom: 1px rgb(200,200,200)` in `create_csd_stylesheet` with the metric.
4. **Slider rework (M).** Three nodes (rail, fill, knob) as described in section 5.1:
   - 4px rail at black 5% / white 10%;
   - `system:accent` fill;
   - 20px white knob with `0 0 0 .5px rgba(0,0,0,.15), 0 .5px 1.5px rgba(0,0,0,.2)`;
   - `:focus` ring `0 0 0 3px` focus colour;
   - `:backdrop` grey fill;
   - `:disabled` at 50%;
   - `width: 100%` by default.
   
   Put the per-OS numbers from the section 5.1 table behind `Os` / `OsVersion` conditions: macOS 26 uses a 6px rail and a 20x16 lozenge; Win11 a 22px disc with a 12/14/10 accent dot; GNOME a 4px pill rail with a 20px knob.
5. **Spinner: macOS spokes (M).**
   - 8 capsule nodes: width D/8, from r = 0.4R to R, black / white at 0.55.
   - `@keyframes` opacity 1 -> .11 -> 1 (peak at 0, trough at 87.5%), 0.8s linear infinite, phase k x 0.1s clockwise.
   - Allow **negative `animation-delay`** in the `animation` parser (S): it is standard CSS and the cleanest way to phase-shift. Without it, use 8 rotated keyframe sets.
   - Default size 16 (small) or 32 (regular). Delete the stale "no CSS animation" doc.

### P1: native look for the rest, per OS

6. **`UiTheme::Native` (L, the umbrella).** A third theme whose values come from `SystemStyle.platform` + `os_version`, using the per-widget tables in sections 2-6. Alternatively, make `Flat` carry `Os`-conditional declarations. Start with button (20pt macOS / 32 Win / 34 GNOME), checkbox (14 / 20 / 20), switch (38x22 / 40x20 / 46x26), text field (21 / 32 / 34), pop-up (20 / 32 / 34), segmented (22 / - / 34) and progress bar (6 / 1+3 / 8).
7. **Spinner arc style (L).** Win11 ProgressRing (stroke 0.094 x size, 180° grow/shrink, 2 s), GNOME AdwSpinner (15% track + oscillating arc, 1.2 s) and M3 circular all need a **trimmed-stroke primitive**: SVG-like `stroke-dasharray` / `stroke-dashoffset` on a circle path, or `mask-image` for a conic-gradient ring. Until then, ship the rotating fixed arc as an approximation, and the KDE icon rotation (works today).
8. **Focus rings everywhere (M).** azul has no `outline`. Either add `outline` + `outline-offset` (CSS standard), or document `box-shadow: 0 0 0 Npx system:focus-ring` as the idiom. Native: macOS 3pt accent 50%; Win 2px + 1px black/white; GNOME 2px accent 50%.
9. **Control sizes (M).** A `ControlSize { Mini, Small, Regular, Large, ExtraLarge }` on the size-bearing widgets. macOS heights [L]: push 13/16/20/28 (26: 16/20/24/28/36); fonts 9/11/13/13.
10. **Traffic-light CSD on macOS (M).** For `WindowDecorations::None` on macOS, draw 12pt circles at a 20pt pitch (14 / 23 on 26) in #FF5F57 / #FEBC2E / #28C840, with hover glyphs and inactive #D6D6D6 / #4C4C4C, instead of the Windows-style glyph buttons.
11. **Windows accent shades (S).** Read `UISettings.GetColorValue(Accent, AccentDark1, AccentLight2, ...)`. The current `DwmGetColorizationColor` value is the frame colourisation (with alpha) and is also copied into `selection_background`. Add `system:accent-dark-1` / `system:accent-light-2` (or a derived `system:accent-fill`).

### P2: tokens, metrics and polish

12. New `system:` tokens (section 7.2): `focus-ring`, `control-fill` (+ secondary / tertiary), `knob`, `quaternary-text`, `titlebar-background` / `-text` / `-separator` (+ inactive), `alternate-row`, `header-text`, `critical` / `caution` / `success`.
13. **macOS discovery additions** (`dll/src/desktop/shell2/macos/system_style.rs`):
    - `titleBarFont`, `menuFont`, `toolTipsFont` and `smallSystemFontSize`;
    - `systemFillColor` tiers, `quaternaryLabelColor`, `windowFrameTextColor`, `headerTextColor` and `alternatingContentBackgroundColors`;
    - runtime traffic-light geometry.
14. **Fix `defaults::macos_modern_*`** (window #ECECEC / #323232, background #FFF / #1E1E1E, opaque selection #0064E1 / #0059D1, text a = 216, corner radius 5). Add a `macos_tahoe_*` set (accent #0088FF / #0091FF, 16/26 window radius, larger controls).
15. **Linux titlebar metrics:** GNOME 46px, bold 11pt (14.67px, not 11px), 20/24px round buttons; a separate `linux_kde()` (about 28px, Noto Sans 10 regular, `[Colors:Header]`).
16. **Split divider** 1px (macOS thin, GNOME, KDE) with a wider invisible grab area, instead of 6px. **Progress bar** 6px (macOS), 3px on a 1px track (Win11), 8px (GNOME), instead of 15. **Tooltip** font 11pt (macOS) or 12px (Win).
17. **Slider extras:** tick marks (macOS 15: 2x8 across the rail with a capsule knob; 26: dots below), vertical orientation, `neutral_value`.

---

## 9. Sources

**[L] Local AppKit probe** (macOS 15.5, 24F74). The JXA scripts were throwaway, run from the session scratchpad with `osascript -l JavaScript`. The method is described at the top of this file. Re-running it needs an NSWindow subclass overriding `isKeyWindow`, `isMainWindow`, `_hasActiveAppearance`, `_hasActiveAppearanceIgnoringKeyFocus`, `_hasActiveControls`, `_hasKeyAppearance` and `_hasMainAppearance`. Without these, sliders draw the inactive grey fill.

### Apple

- [A1] HIG Sliders: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/sliders.json
- [A2] HIG Progress indicators: .../progress-indicators.json
- [A3] HIG Windows, Toolbars: .../windows.json, .../toolbars.json
- [A4] HIG Color: .../color.json
- [A5] HIG Typography, Accessibility, Split views, Toggles: .../typography.json etc.
- [A6] WWDC25 session 310, "Build an AppKit app with the new design": https://developer.apple.com/videos/play/wwdc2025/310/
- [A7] WWDC25 session 356, "Get to know the new design system": https://developer.apple.com/videos/play/wwdc2025/356/
- [A8] WWDC25 session 219, "Meet Liquid Glass": https://developer.apple.com/videos/play/wwdc2025/219/
- [A9] WWDC20 session 10104, "Adopt the new look of macOS": https://developer.apple.com/videos/play/wwdc2020/10104/
- [A10] WebKit `RenderThemeMac.mm` and `platform/graphics/mac/controls/*Mac.mm`: https://raw.githubusercontent.com/WebKit/WebKit/main/Source/WebCore/rendering/mac/RenderThemeMac.mm
- [A11] WebKit `RenderThemeCocoa.mm` (the form-control refresh)
- [A12] MacVue tokens, third party, measured against the macOS 26/27 kits: https://cdn.jsdelivr.net/gh/antonreshetov/macvue@main/packages/core/tokens/
- [A13] Qt `qmacstyle_mac.mm`: https://raw.githubusercontent.com/qt/qtbase/dev/src/plugins/styles/mac/qmacstyle_mac.mm
- [A14] Firefox `ScrollbarDrawingCocoa.cpp`
- [A15] DesertColors NSColor dump: https://cdn.jsdelivr.net/gh/holtwick/DesertColors@master/desertcolors.css
- [A16] https://swiftuicolors.com/macos-colors
- [A17] terum-skills PR #214 (search snippet only; unverified)
- [A18] Zed discussion #38233 (snippet)
- [A19] https://lapcatsoftware.com/articles/2026/3/1.html, https://mjtsai.com/blog/2025/10/16/tahoe-window-corners/
- [A20] Apple Support "Get to know the Finder" (15.0 and 26 screenshots)
- [A21] MacRumors thread on `NSConvolutionOverride1` (snippet)
- [A22] YRKSpinningProgressIndicator (old 12-spoke clone)
- [A23] VS Code #279769, GitHub Desktop #21135 (snippets)
- [A24] pxlnv.com Big Sur observations, Six Colors (snippets)
- [A25] https://www.macrumors.com/2026/06/09/macos-golden-gate-liquid-glass/ (verified reachable)

### Windows

Base URL for the WinUI files: `https://raw.githubusercontent.com/microsoft/microsoft-ui-xaml/main/controls/dev/`.

- [W1] `CommonStyles/Slider_themeresources.xaml`
- [W2] `CommonStyles/Common_themeresources_any.xaml`
- [W3] `CommonStyles/CornerRadius_themeresources.xaml`
- [W4] `ProgressRing/ProgressRing.xaml` + `_themeresources`
- [W5] `ProgressRing/AnimatedVisuals/ProgressRingIndeterminate.cpp`
- [W6] `ProgressRing/ProgressRing.cpp`
- [W7] `.../main/dxaml/xcp/dxaml/themes/generic.xaml` (Windows 10 WUX)
- [W8] `winui2/main/dev/CommonStyles/*_themeresources_v1.xaml`
- [W9] third-party WPF port of the UWP ProgressRing: https://raw.githubusercontent.com/kwonganding/wpf.controls/master/Util.Controls.V1.0/Util.Controls/Control/ProgressRing.xaml
- [W10] https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design
- [W11] https://learn.microsoft.com/en-us/windows/apps/develop/title-bar
- [W12] https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.windowing.titlebarheightoption
- [W13] `TitleBar/TitleBar_themeresources.xaml`
- [W14] Windows Terminal `MinMaxCloseControl.xaml` (main)
- [W15] Windows Terminal `MinMaxCloseControl.xaml` (release-1.0)
- [W16] Chromium `windows_caption_button.cc`
- [W17] Chromium `native_chrome_color_mixer_win.cc`
- [W18] https://learn.microsoft.com/en-us/windows/uwp/ui-input/title-bar
- [W19] .../signature-experiences/geometry
- [W20] .../modernize/ui/apply-rounded-corners
- [W21] .../motion/timing-and-easing
- [W22] .../input/guidelines-for-visualfeedback
- [W23] .../signature-experiences/typography
- [W24] .../signature-experiences/layering
- [W25] .../style/mica
- [W26] .../controls/progress-controls
- [W27] .../controls/slider
- [W28] .../signature-experiences/color
- [W29] GetSysColor docs
- [W30] Fluent 2 web tokens: https://cdn.jsdelivr.net/npm/@fluentui/tokens@1.0.0-alpha.24/
- [W31] WindowMetrics registry defaults (winaero, tenforums)
- [W32] Win32 trackbar docs
- [W33] other WinUI 3 `*_themeresources.xaml` files (Button, CheckBox, RadioButton, ToggleSwitch, TextBox, ToolTip, ScrollBar, MenuFlyout, ContentDialog, ListViewItem, ComboBox, NumberBox, ProgressBar, TabView, TreeView, Expander, SelectorBar, NavigationView, ColorPicker)
- [W34] WinUI 2.6 release notes

### GNOME

Base URL for the libadwaita files: `https://cdn.jsdelivr.net/gh/GNOME/libadwaita@1.8.0/`, also compared at 1.4.0, 1.6.0, 1.7.0 and main.

- [G1] `src/stylesheet/widgets/_scale.scss`
- [G2] `_colors.scss`
- [G3] `_defaults.scss` (per version)
- [G4] `_common.scss`
- [G5] `_drawing.scss`
- [G6] `widgets/_buttons.scss`
- [G7] `widgets/_header-bar.scss`
- [G8] `widgets/_window.scss`
- [G9] `widgets/_toolbars.scss`
- [G10] `src/adw-spinner-paintable.c`
- [G11] `src/adw-spinner.c`
- [G12] `src/adw-easing.c`
- [G13] `widgets/_spinner.scss`
- [G14] the other `widgets/*.scss` files
- [G15] GTK4 `gtkrange.c`, `gtkswitch.c`, `gtkpopover.c`, `gtkspinner.c` (jsdelivr gh/GNOME/gtk@main)
- [G16] `adw-accent-color.c`, `adw-settings-impl-portal.c`
- [G17] `adw-alert-dialog.c`
- [G18] gsettings-desktop-schemas 47.1 / 48.0 / 49.0
- [G19] `org.freedesktop.portal.Settings.xml`
- [G20] GNOME HIG: https://developer.gnome.org/hig/
- [G21] libadwaita doc images
- [G22] https://docs.gtk.org/gtk4/scales.png

### KDE

Base URL: `https://invent.kde.org/plasma/breeze/-/raw/master/`.

- [K1] `kstyle/breezemetrics.h`
- [K2] `kstyle/breezehelper.cpp`
- [K3] `kstyle/breezestyle.cpp`
- [K4] `kstyle/breeze.kcfg` and animations
- [K5] `colors/BreezeLight.colors` / `BreezeDark.colors`
- [K6] `kdecoration/breezedecoration.cpp`, `breezebutton.cpp`
- [K7] kdecoration `decorationsettings.cpp`
- [K8] kwin `decorationpalette.cpp`
- [K9] qqc2-desktop-style `BusyIndicator.qml`, `SwitchIndicator.qml`
- [K10] libplasma `BusyIndicator.qml` and `busywidget.svg`
- [K11] kwidgetsaddons `kbusyindicatorwidget.cpp`
- [K12] kirigami `units.cpp`
- [K13] breeze-icons `process-working-symbolic.svg`
- [K14] plasma-integration `kfontsettingsdata.cpp`
- [K15] kcolorscheme `kcolorscheme.cpp`
- [K16] plasma-workspace `colorsapplicator.cpp`
- [K17] xdg-desktop-portal-kde `settings.cpp`
- [K18] https://develop.kde.org/hig/
- [K19] Plasma 6.4 announcement screenshots (about 125% scale)

### iOS

Most iOS figures come from the Flutter Cupertino and Ionic clones or were measured from screenshots; Apple publishes almost no pixel numbers.

- [I1] Flutter Cupertino: https://cdn.jsdelivr.net/gh/flutter/flutter@master/packages/flutter/lib/src/cupertino/ (slider, activity_indicator, switch, sliding_segmented_control, nav_bar, colors, dialog, context_menu, scrollbar, text_field, picker, bottom_tab_bar, button)
- [I2] Ionic iOS: https://cdn.jsdelivr.net/gh/ionic-team/ionic-framework@main/core/src/components/ (`*.ios.vars.scss`, `spinner-configs.ts`)
- [I3] UIKit doc JSON: https://developer.apple.com/tutorials/data/documentation/uikit/ (systemfill, indicatorstyle, scrolledgeappearance, ...)
- [I4] HIG Typography JSON
- [I5] SwiftUI documentation renders: https://developer.apple.com/tutorials/images/com.apple.SwiftUI/
- [I6] SwiftUI `Animation.default`
- [I7] HIG Color
- [I8] HIG refresh-control image (iOS 26)
- [I9] useyourloaf iPhone screen sizes
- [I10] expo-apple-colors
- [I11]-[I14] snippets (UIButton configuration sizes, Programming iOS 14, ProgressView size, tab bar practices)
- [I15] HIG iOS 26 pages (materials, tab bars, toolbars, buttons, color)
- [I16] Adopting Liquid Glass; Applying Liquid Glass to custom views
- [I17] WWDC25 sessions 219, 356, 284, 323
- [I18] API JSON: UISlider.TrackConfiguration, SliderTick, ControlSize.extraLarge, UIButton.Configuration, UICornerConfiguration, UIScrollEdgeEffect
- [I19] Codename One native iOS 26 goldens: https://raw.githubusercontent.com/codenameone/CodenameOne/master/scripts/fidelity-app/goldens/ios-26-metal/
- [I20] Codename One `GlassRecipe.java`
- [I21] flutter-settings-ui PR #209 (switch measured on the simulator)
- [I22] LiquidGlassKit
- [I23] https://sarunw.com/posts/swiftui-native-controls-ios-26/
- [I24]-[I28] nav bar height issue, bar-button pill issue, Form cell lldb readout, FabBar constants, learnui guide
- [I29] Flutter PR #189963 (display corner radii)
- [I30] nilcoalescing concentric rectangles
- [I31] Liquid Glass recreations (liquid_glass_widgets, nikdelvin/liquid-glass, kube.io)
- [I32] HIG images: https://developer.apple.com/tutorials/images/com.apple.HIG/

### Material 3

- [M1] material-web tokens v0.192: https://cdn.jsdelivr.net/gh/material-components/material-web@main/tokens/versions/v0_192/ (the files under `tokens/v0_192/` are 120-byte `@forward` stubs)
- [M2] material-web internals (focus ring, elevation, circular and linear progress)
- [M3] Compose M3 tokens: https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/ (SliderTokens, LoadingIndicatorTokens, CircularProgressIndicatorTokens, AppBar*, Button*, SwitchTokens, MenuTokens, DragHandleTokens, motion, state)
- [M4] Compose M3 components (Slider.kt, ProgressIndicator.kt, LoadingIndicator.kt, MaterialShapes.kt, AppBar.kt, Menu.kt, ...)
- [M5] Compose M2 `ProgressIndicator.kt` (the classic spinner)
- [M6] MDC-Android: https://raw.githubusercontent.com/material-components/material-components-android/master/lib/java/com/google/android/material/
- [M7] MDC-Android doc images: .../docs/components/assets/
- [M8] AOSP framework `ViewConfiguration.java`, `config.xml`, `dimens.xml`
- [M9] m3.material.io spec pages are JavaScript-only. **No number in this report comes from them.**

## 10. Image inventory (`target/native-reference/`, untracked)

There are 239 files. The `macos-15-*` files (except `macos-15-titlebar-unified-finder-light.png`) were rendered locally [L] and are genuine AppKit output. Every other file was downloaded; its source is in 10.1.

- **macOS 13/14 (HIG, 2023 era)** (11): `macos-14-progress-circular-dark.png`, `macos-14-progress-circular-light.png`, `macos-14-progressbar-determinate-dark.png`, `macos-14-progressbar-determinate-light.png`, `macos-14-progressbar-indeterminate-dark.png`, `macos-14-progressbar-indeterminate-light.png`, `macos-14-segmented-selectone-dark.png`, `macos-14-segmented-selectone-light.png`, `macos-14-spinner-hig-dark.png`, `macos-14-spinner-hig-light.png`, `macos-14-textfield-light.png`
- **macOS 15 Sequoia, rendered locally [L] unless named *-finder** (51): `macos-15-button-push-dark@4x.png`, `macos-15-button-push-light@4x.png`, `macos-15-checkbox-off-dark@4x.png`, `macos-15-checkbox-off-light@4x.png`, `macos-15-checkbox-on-dark@4x.png`, `macos-15-checkbox-on-light@4x.png`, `macos-15-controls-gallery-dark-inactive.png`, `macos-15-controls-gallery-dark.png`, `macos-15-controls-gallery-light-inactive.png`, `macos-15-controls-gallery-light.png`, `macos-15-popup-button-dark@4x.png`, `macos-15-popup-button-light@4x.png`, `macos-15-progressbar-dark@4x.png`, `macos-15-progressbar-light@4x.png`, `macos-15-radio-on-dark@4x.png`, `macos-15-radio-on-light@4x.png`, `macos-15-segmented-dark@4x.png`, `macos-15-segmented-light@4x.png`, `macos-15-slider-dark-inactive.png`, `macos-15-slider-dark.png`, `macos-15-slider-light-inactive.png`, `macos-15-slider-light.png`, `macos-15-spinner-regular-dark.gif`, `macos-15-spinner-regular-light.gif`, `macos-15-spinner-sizes-static-light.png`, `macos-15-spinner-spritesheet-24frames-dark.png`, `macos-15-spinner-spritesheet-24frames-light.png`, `macos-15-switch-off-dark@4x.png`, `macos-15-switch-off-light@4x.png`, `macos-15-switch-on-dark@4x.png`, `macos-15-switch-on-light@4x.png`, `macos-15-table-rows-dark.png`, `macos-15-table-rows-light-inactive.png`, `macos-15-table-rows-light.png`, `macos-15-tabs-table-scroller-split-dark.png`, `macos-15-tabs-table-scroller-split-light.png`, `macos-15-textfield-dark@4x.png`, `macos-15-textfield-light@4x.png`, `macos-15-titlebar-standard-dark-inactive.png`, `macos-15-titlebar-standard-dark.png`, `macos-15-titlebar-standard-light-inactive.png`, `macos-15-titlebar-standard-light.png`, `macos-15-titlebar-unified-compact-dark-inactive.png`, `macos-15-titlebar-unified-compact-dark.png`, `macos-15-titlebar-unified-compact-light-inactive.png`, `macos-15-titlebar-unified-compact-light.png`, `macos-15-titlebar-unified-finder-light.png`, `macos-15-titlebar-unified-toolbar-dark-inactive.png`, `macos-15-titlebar-unified-toolbar-dark.png`, `macos-15-titlebar-unified-toolbar-light-inactive.png`, `macos-15-titlebar-unified-toolbar-light.png`
- **macOS 26 Tahoe (HIG / Apple Support)** (25): `macos-26-alert-dark.png`, `macos-26-alert-light.png`, `macos-26-checkbox-mixed-light.png`, `macos-26-checkbox-off-light.png`, `macos-26-checkbox-on-dark.png`, `macos-26-checkbox-on-light.png`, `macos-26-disclosure-triangle-light.png`, `macos-26-popover-light.png`, `macos-26-radio-off-light.png`, `macos-26-radio-on-light.png`, `macos-26-slider-circular-dark.png`, `macos-26-slider-circular-light.png`, `macos-26-slider-dark.png`, `macos-26-slider-light.png`, `macos-26-slider-ticklabels-dark.png`, `macos-26-slider-ticklabels-light.png`, `macos-26-slider-ticks-dark.png`, `macos-26-slider-ticks-light.png`, `macos-26-titlebar-notes-toolbar-menu-light.png`, `macos-26-titlebar-segmented-in-toolbar-light.png`, `macos-26-titlebar-toolbar-anatomy-dark.png`, `macos-26-titlebar-toolbar-anatomy-light.png`, `macos-26-titlebar-unified-finder-light.png`, `macos-26-titlebar-window-states-dark.png`, `macos-26-titlebar-window-states-light.png`
- **Windows 10 / Win32** (2): `windows-10-win32-trackbar-legacy-aero.png`, `windows-10-win32-trackbar-selrange-legacy-aero.png`
- **Windows 11 (Microsoft Learn)** (19): `windows-11-controls-dark.png`, `windows-11-controls-light.png`, `windows-11-focusvisual-redlines.png`, `windows-11-geometry-corners.png`, `windows-11-mica-window-dark.png`, `windows-11-mica-window-light.png`, `windows-11-progressbar-determinate.png`, `windows-11-progressbar-indeterminate.gif`, `windows-11-progressring-determinate.png`, `windows-11-progressring-indeterminate-example.gif`, `windows-11-progressring-indeterminate-framestrip.png`, `windows-11-progressring-indeterminate.gif`, `windows-11-slider-light.png`, `windows-11-slider-ticks-light.png`, `windows-11-tabview-mica.png`, `windows-11-titlebar-backbutton.png`, `windows-11-titlebar-overview.png`, `windows-11-titlebar-search.png`, `windows-11-titlebar-tabs.png`
- **GNOME 45-49 (HIG, libadwaita docs, GTK docs)** (43): `gnome-47-checkbox-hig-light.png`, `gnome-47-entry-hig-light.png`, `gnome-47-headerbar-hig-buttons-light.png`, `gnome-47-headerbar-hig-dark.png`, `gnome-47-headerbar-hig-light.png`, `gnome-47-popover-hig-light.png`, `gnome-47-progressbar-hig-light.png`, `gnome-47-radio-hig-light.png`, `gnome-47-slider-hig-dark.png`, `gnome-47-slider-hig-light.png`, `gnome-47-spinbutton-hig-light.png`, `gnome-47-spinner-hig-dark.png`, `gnome-47-spinner-hig-light.png`, `gnome-47-switch-hig-dark.png`, `gnome-47-switch-hig-light.png`, `gnome-49-alertdialog-light.png`, `gnome-49-boxedlist-dark.png`, `gnome-49-boxedlist-light.png`, `gnome-49-button-flat-light.png`, `gnome-49-button-raised-light.png`, `gnome-49-button-suggested-light.png`, `gnome-49-comborow-light.png`, `gnome-49-dialog-floating-light.png`, `gnome-49-entryrow-light.png`, `gnome-49-expanderrow-light.png`, `gnome-49-headerbar-dark.png`, `gnome-49-headerbar-flat-light.png`, `gnome-49-headerbar-light.png`, `gnome-49-linked-light.png`, `gnome-49-popovermenu-light.png`, `gnome-49-progressbar-osd-light.png`, `gnome-49-spinner-adw-dark.png`, `gnome-49-spinner-adw-light.png`, `gnome-49-spinrow-light.png`, `gnome-49-splitbutton-light.png`, `gnome-49-switchrow-light.png`, `gnome-49-tabbar-dark.png`, `gnome-49-tabbar-light.png`, `gnome-49-togglegroup-dark.png`, `gnome-49-togglegroup-light.png`, `gnome-49-window-dark.png`, `gnome-49-window-light.png`, `gnome-gtk4-slider-docs-light.png`
- **KDE Plasma 6** (9): `kde-plasma6-busyindicator-plasma-busywidget.svg`, `kde-plasma6-busyindicator-process-working-22-sprite.svg`, `kde-plasma6-busyindicator-process-working-symbolic-16.svg`, `kde-plasma6-checkbox-hig.png`, `kde-plasma6-radio-hig.png`, `kde-plasma6-slider-spinbox-hig.png`, `kde-plasma6-switch-hig.png`, `kde-plasma6-titlebar-dark-125pct.png`, `kde-plasma6-titlebar-light-125pct.png`
- **iOS 17/18 (SwiftUI documentation renders)** (17): `ios-17-alert-dark.png`, `ios-17-alert-light.png`, `ios-17-contextmenu-light.png`, `ios-17-datepicker-compact-light.png`, `ios-17-list-disclosure-light.png`, `ios-17-segmented-dark.png`, `ios-17-segmented-light.png`, `ios-17-slider-dark.png`, `ios-17-slider-labels-light.png`, `ios-17-slider-light.png`, `ios-17-stepper-dark.png`, `ios-17-stepper-light.png`, `ios-17-switch-insetgrouped-dark.png`, `ios-17-switch-insetgrouped-light.png`, `ios-17-tabbar-items-light.png`, `ios-17-textfield-roundedrect-dark.png`, `ios-17-textfield-roundedrect-light.png`
- **iOS 26 (Codename One goldens, sarunw, HIG)** (44): `ios-26-alert-light.png`, `ios-26-alert-stacked-light.png`, `ios-26-button-flat-light.png`, `ios-26-button-light.png`, `ios-26-button-pressed-light.png`, `ios-26-button-raised-light.png`, `ios-26-controls-overview-ios17baseline-light.png`, `ios-26-controls-overview-light.png`, `ios-26-glass-icon-button-light.png`, `ios-26-glass-lens-illustration.png`, `ios-26-glass-material-clear.png`, `ios-26-glass-material-regular-over-dark.png`, `ios-26-glass-material-regular-over-light.png`, `ios-26-glass-panel-grey-light.png`, `ios-26-glass-panel-photo-light.png`, `ios-26-glass-text-button-light.png`, `ios-26-menu-light.png`, `ios-26-menu-sizes.png`, `ios-26-navbar-dark.png`, `ios-26-navbar-light.png`, `ios-26-navbar-tabbar-light.png`, `ios-26-picker-wheel-light.png`, `ios-26-progress-light.png`, `ios-26-searchfield-toolbar.png`, `ios-26-segmented-light.png`, `ios-26-segmented-sheet-light.png`, `ios-26-sheet-medium-light.png`, `ios-26-slider-dark.png`, `ios-26-slider-disabled-light.png`, `ios-26-slider-light.png`, `ios-26-slider-ticks-light.png`, `ios-26-switch-off-dark.png`, `ios-26-switch-off-light.png`, `ios-26-switch-on-dark.png`, `ios-26-switch-on-light.png`, `ios-26-tabbar-accessory-collapsed.png`, `ios-26-tabbar-accessory-expanded.png`, `ios-26-tabbar-dark.png`, `ios-26-tabbar-geom-light.png`, `ios-26-tabbar-light.png`, `ios-26-tabbar-minimized-light.png`, `ios-26-tabbar-search-tab.png`, `ios-26-textfield-light.png`, `ios-26-toolbar-groups-light.png`
- **Android Material 3 (MDC-Android docs)** (18): `android-m3-buttongroup-connected.png`, `android-m3-circularprogress-indeterminate.gif`, `android-m3-circularprogress-wavy-indeterminate.gif`, `android-m3-linearprogress-indeterminate.gif`, `android-m3-linearprogress-wavy-indeterminate.gif`, `android-m3-loadingindicator-anatomy.png`, `android-m3-loadingindicator-contained.gif`, `android-m3-loadingindicator.gif`, `android-m3-progress-anatomy.png`, `android-m3-slider-anatomy.png`, `android-m3-slider-centered.png`, `android-m3-slider-stopindicator.png`, `android-m3-splitbutton-anatomy.png`, `android-m3-switch-anatomy.png`, `android-m3-topappbar-anatomy.png`, `android-m3-topappbar-large-flexible.png`, `android-m3-topappbar-medium-flexible.png`, `android-m3-topappbar-small.png`

### 10.1 Download sources

| Files | Source URL |
|---|---|
| `macos-26-slider-{light,dark}` | https://developer.apple.com/tutorials/images/com.apple.HIG/sliders-no-tick-marks@2x.png (dark: `~dark@2x.png`) |
| `macos-26-slider-ticks-*, -ticklabels-*, -circular-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/sliders-tick-marks / sliders-labels / sliders-circular @2x.png |
| `macos-14-spinner-hig-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/progress-indicator-intermediate-spinner@2x.png |
| `macos-14-progressbar-determinate-*, -indeterminate-*, macos-14-progress-circular-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/progress-indicator-{determinate-bar,intermediate-bar,determinate-circle}@2x.png |
| `macos-26-titlebar-window-states-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/window-states@2x.png |
| `macos-26-titlebar-toolbar-anatomy-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/toolbars-mac-window-anatomy@2x.png |
| `macos-26-titlebar-notes-toolbar-menu-light` | https://developer.apple.com/tutorials/images/com.apple.HIG/toolbars-notes-app-expanded-icons@2x.png |
| `macos-26-titlebar-segmented-in-toolbar-light` | https://developer.apple.com/tutorials/images/com.apple.HIG/tab-views-top@2x.png |
| `macos-15-titlebar-unified-finder-light` | https://help.apple.com/assets/67D1B1065D0706C4AD080C54/67D1B10E5D0706C4AD080C79/en_US/5efb4dc160ec1878baf3abe36dc3ba10.png |
| `macos-26-titlebar-unified-finder-light` | https://help.apple.com/assets/69DD569682238CF8EC0621E2/69DD569982238CF8EC0621E9/en_US/4c09ae5b39875fee3cde6fbc1fbf89d3.png |
| `macos-26-checkbox-*, macos-26-radio-*` | https://developer.apple.com/tutorials/images/com.apple.HIG/checkbox-* / radio-button-* @2x.png |
| `macos-26-alert-*, -popover-light, -disclosure-triangle-light` | https://developer.apple.com/tutorials/images/com.apple.HIG/alert-macos / attached-popover / disclosure-triangle-after @2x.png |
| `macos-14-segmented-selectone-*, macos-14-textfield-light` | https://developer.apple.com/tutorials/images/com.apple.HIG/segmented-control-one-choice / text-fields-formatted-text @2x.png |
| `windows-11-controls-{light,dark}` | https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/images/color_{light,dark}_controls_940.png |
| `windows-11-geometry-corners` | https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/images/geometry_rounded_corners_1880.png |
| `windows-11-slider-light, -slider-ticks-light` | https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/images/controls/slider.png, .../images/slider-ticks.png |
| `windows-11-progressring-*, -progressbar-*` | https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/images/{progressring-indeterminate.gif, progress-ring-indeterminate-example.gif, progress-ring.jpg, progressbar-indeterminate.gif, progressbar-determinate.png}; the `-framestrip.png` was generated locally from the GIF |
| `windows-11-titlebar-{overview,backbutton,search,tabs}` | https://learn.microsoft.com/en-us/windows/apps/design/basics/images/titlebar/{titlebar-overview,back-button,search,tabs}.png |
| `windows-11-mica-window-*, -tabview-mica` | https://learn.microsoft.com/en-us/windows/apps/design/style/images/materials/mica-{light,dark}-theme.png, .../style/images/mica-tabs.png |
| `windows-11-focusvisual-redlines` | https://learn.microsoft.com/en-us/windows/apps/develop/input/images/focus-rect-redlines.png |
| `windows-10-win32-trackbar-*` | https://learn.microsoft.com/en-us/windows/win32/controls/images/tkb-simple.png, tkb-selrange.png (Vista/7 Aero rendering) |
| `gnome-47-*-hig-*` | https://developer.gnome.org/hig/_images/{sliders,spinner,header-bar,header-bar-buttons,switches,checkboxes,radio-buttons,text-fields,spin-button,progress-bar,popover}.png (dark: `-dark.png`) |
| `gnome-49-* (all)` | https://cdn.jsdelivr.net/gh/GNOME/libadwaita@1.8.0/doc/images/{spinner,header-bar,flat-header-bar,window,buttons-raised,buttons-suggested-action,buttons-flat,split-button,linked-controls,toggle-group,boxed-lists,entry-row,spin-row,switch-row,combo-row,expander-row,tab-bar,popover-menu-list,osd-progress-bar,alert-dialog,dialog-floating}.png (dark: `-dark.png`) |
| `gnome-gtk4-slider-docs-light` | https://docs.gtk.org/gtk4/scales.png |
| `kde-plasma6-slider-spinbox-hig, -checkbox-hig, -radio-hig, -switch-hig` | https://develop.kde.org/hig/{slider-and-spinbox,checkboxes-with-obvious-opposite-states,radio-buttons-with-non-obvious-opposite-states,switch-with-obvious-opposite-state}.png |
| `kde-plasma6-titlebar-{light,dark}-125pct` | https://kde.org/announcements/plasma/6/6.4.0/{history,tablet_config}.png |
| `kde-plasma6-busyindicator-*.svg` | https://invent.kde.org/frameworks/breeze-icons/-/raw/master/icons/{status/16/process-working-symbolic.svg, animations/22/process-working.svg}; https://invent.kde.org/plasma/libplasma/-/raw/master/src/desktoptheme/breeze/widgets/busywidget.svg |
| `ios-17-* (all)` | https://developer.apple.com/tutorials/images/com.apple.SwiftUI/{SwiftUI-Slider-simple, SwiftUI-Slider-withStepAndLabels, SwiftUI-Form-iOS, Picker-3-iOS, SwiftUI-Stepper-value-step-range, SwiftUI-Alert-OK, SwiftUI-TextField-roundedBorderStyle, View-contextMenu-1-iOS, TabView-1, Picker-1-iOS, SwiftUI-DatePicker-basic}@2x.png (dark: `~dark@2x.png`) |
| `ios-26-slider-{light,dark,disabled-light}, -switch-*, -button*, -glass-{icon,text}-button, -glass-panel-*, -tabbar-{light,dark,geom-light}, -navbar-*, -progress-light, -picker-wheel-light, -textfield-light` | https://raw.githubusercontent.com/codenameone/CodenameOne/master/scripts/fidelity-app/goldens/ios-26-metal/{Slider,Switch,Button,RaisedButton,FlatButton,GlassIcon,GlassText,GlassPanelGrey,GlassPanelPhoto,Tabs,TabsGeom,Toolbar,ProgressBar,Spinner,TextField}_*.png |
| `ios-26-controls-overview-*, -navbar-tabbar-light, -tabbar-minimized-light, -sheet-medium-light, -slider-ticks-light, -segmented-light, -toolbar-groups-light` | https://sarunw.com/posts/swiftui-native-controls-ios-26/ (images fetched via the wsrv.nl proxy) |
| `ios-26-alert-*, -menu-*, -glass-lens-illustration, -glass-material-*, -tabbar-accessory-*, -tabbar-search-tab, -searchfield-toolbar, -segmented-sheet-light` | https://developer.apple.com/tutorials/images/com.apple.HIG/{alert-ios, buttons-roles-alert, menu-secondary-actions-expanded, small-medium-large-menu-layouts, human-interface-guidelines-page-image-card, materials-ios-liquid-glass-*, tab-bar-with-accessory-*, search-fields-search-as-tab-prominent, search-fields-ios-toolbar-with-items, segmented-controls-calendar-new-event}@2x.png |
| `android-m3-* (all)` | https://raw.githubusercontent.com/material-components/material-components-android/master/docs/components/assets/{slider/slider-anatomy.png, slider/slider-standard-stopindicator.png, slider/slider-centered-horizontal.png, loadingindicator/{loading-indicator.gif, loading-indicator-contained.gif, anatomy.png}, progressindicator/{circular-indeterminate.gif, wavy-indeterminate-circular.gif, linear-indeterminate.gif, wavy-indeterminate-linear.gif, progressindicators-anatomy.png}, topappbar/{topappbar-anatomy,topappbar-small,medium-flexible-light,large-flexible-light}.png, switch/switch-anatomy.png, buttons/{connected-button-group,splitbutton-anatomy}.png} |

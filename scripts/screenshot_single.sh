#!/bin/bash
# Screenshot the C examples for the website.
#
# Usage:
#   ./scripts/screenshot_single.sh                    # hello-world
#   ./scripts/screenshot_single.sh widgets            # one example
#   ./scripts/screenshot_single.sh hello-world calc   # several, ONE dll build
#   ./scripts/screenshot_single.sh all                # every example on the index
#   ./scripts/screenshot_single.sh widgets 8780       # legacy: trailing port, ignored
#
# Runs on Linux, macOS and Windows (MSYS/MinGW) unchanged: build the library,
# compile each example against it, run it twice under the AZ_E2E scenario
# runner — once pinned light, once pinned dark — and keep the native window
# capture from each run.
#
# Output per example, in target/examples-temp/<example>/:
#   <example>.<os>.light.png   <example>.<os>.dark.png
#   <example>_screenshot.png   (the light one; what CI's artifact step copies)
# Those are the names examples/assets/screenshots and api.json use, so
# publishing a fresh set is a straight copy.
#
# ── Why AZ_E2E and not AZ_DEBUG ──────────────────────────────────────────────
# The debug server is an in-process HTTP listener on a real port. Taking one
# screenshot needed all of it — a port to allocate, a liveness poll, a curl
# round trip, a graceful-close request — and it had to be COMPILED IN
# (`--features debug-server`), which is a feature the shipped library does NOT
# carry. `e2e-scripting` compiles the same op dispatcher with no socket at all:
# the scenario is a JSON file, the app runs it and exits with a test-runner
# verdict, and there is nothing to connect to.
#
# And `build-dll` ALREADY ENABLES `e2e-scripting` ("AZ_E2E SCRIPTING, in every
# shipped dylib", dll/Cargo.toml). So this script asks for no feature the normal
# build does not already have, which is what makes the build below a cache hit
# instead of a rebuild — and is why it can no longer overwrite an artifact
# someone else built. Adding `debug-server` is what used to force both.
#
# ── Why two processes and not one theme toggle ───────────────────────────────
# The theme is pinned with AZ_THEME=light|dark, read as the window is created.
# Nothing touches the user's desktop. The old script toggled the DESKTOP's
# appearance (gsettings / osascript / the Windows registry) and captured both
# frames from one process, which on any desktop that does not keep its
# appearance in gsettings — KDE reads ~/.config/kdeglobals, XFCE reads xfconf —
# silently produced two identical light captures.

set -e
unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }

is_windows() { [[ "$(uname)" == MINGW* ]] || [[ "$(uname)" == MSYS* ]] || [[ "$(uname)" == CYGWIN* ]]; }
is_linux()   { [ "$(uname)" == "Linux" ]; }
is_macos()   { [ "$(uname)" == "Darwin" ]; }

if is_macos; then OS_TAG="mac"
elif is_windows; then OS_TAG="windows"
elif is_linux; then OS_TAG="linux"
else log_error "Unsupported platform: $(uname)"; exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TEMP_ROOT="$ROOT_DIR/target/examples-temp"
HEADER_DIR="$ROOT_DIR/target/codegen"
RUN_TIMEOUT="${AZ_SCREENSHOT_TIMEOUT:-180}"   # per process, must outlast settle_ms

# The seven examples the website's index page shows (`show_on_index` in
# api.json). `all` expands to exactly these.
INDEX_EXAMPLES=(hello-world widgets opengl infinity async xhtml calc)

# ── Arguments ───────────────────────────────────────────────────────────────
# A bare number is the old debug-server port. Nothing listens any more, but the
# CI matrix still passes one per example, so drop it rather than treat "8780"
# as an example name.
EXAMPLES=()
for arg in "$@"; do
    case "$arg" in
        ''|*[!0-9]*)
            if [ "$arg" = "all" ]; then
                EXAMPLES+=("${INDEX_EXAMPLES[@]}")
            else
                EXAMPLES+=("$arg")
            fi
            ;;
        *) log_info "Ignoring legacy port argument '$arg' (nothing listens any more)" ;;
    esac
done
[ ${#EXAMPLES[@]} -eq 0 ] && EXAMPLES=(hello-world)

# ── Profile ─────────────────────────────────────────────────────────────────
# `release` — the workspace's LOCAL profile (lto = false, codegen-units = 16),
# as `.cargo/config.toml` already asks for: "Local release builds should remain
# fast; CI handles LTO via --profile prod-release". An unconditional
# `prod-release` default here quietly broke that rule.
#
# Measured, because the obvious story is wrong: on a full chain rebuild the two
# profiles cost about the SAME (release 12m11s, prod-release 11m38s) — the time
# is in azul-css -> azul-core -> azul-layout, not in LTO. What `release` buys a
# screenshot run is not speed, it is SEPARATION: a developer's machine has a
# `release` tree already and no `prod-release` one, and building into
# `target/prod-release` here would replace the library CI publishes from that
# same path.
#
# `AZ_SCREENSHOT_PROFILE` overrides. CI sets it to `prod-release`, where that
# profile is already built and the feature set is identical, so the build below
# costs nothing there.
PROFILE="${AZ_SCREENSHOT_PROFILE:-release}"
LIB_DIR="$ROOT_DIR/target/$PROFILE"

if is_macos; then DLL_PATH="$LIB_DIR/libazul.dylib"
elif is_linux; then DLL_PATH="$LIB_DIR/libazul.so"
else DLL_PATH="$LIB_DIR/azul.dll"
fi

# ── Step 1: the library, once for every example in this run ─────────────────
log_step "Building libazul (profile=$PROFILE, features=build-dll)..."

if [ -n "$AZ_SCREENSHOT_SKIP_BUILD" ] && [ -f "$DLL_PATH" ]; then
    log_warn "AZ_SCREENSHOT_SKIP_BUILD set - reusing $DLL_PATH as-is"
else
    cd "$ROOT_DIR"
    cargo build --profile "$PROFILE" -p azul-dll --features build-dll
fi

if [ ! -f "$DLL_PATH" ]; then
    log_error "No library at $DLL_PATH"
    exit 1
fi
log_success "Library: $DLL_PATH ($(ls -lh "$DLL_PATH" | awk '{print $5}'))"

# ── Stale generated headers in the examples tree ────────────────────────────
# `target/codegen` is the ONLY source of truth for azul.h and the azul*.hpp
# set; `cargo run -p azul-doc -- codegen all` regenerates every one of them.
# Copies nevertheless accumulate next to the examples, because that is how CI
# builds them (`cp target/codegen/azul.h examples/c/` before each compile step)
# and nothing removes them afterwards. They are untracked, so they simply sit
# there going stale — and a QUOTED `#include "azul.h"` searches the source
# file's own directory before any `-I`, so a stale sibling silently WINS over
# the header this script passes.
#
# That is not a warning, a link error or a version mismatch: the example
# compiles clean against a header whose structs no longer match the library's,
# and dies with SIGSEGV a few hundred milliseconds into `App::run`. One found
# here was three weeks old (4.98 MB against the generated 5.66 MB).
#
# Deleted rather than refreshed: nothing in this script reads them, and a file
# that is not there cannot shadow anything.
for stray in "$ROOT_DIR"/examples/c/azul*.h "$ROOT_DIR"/examples/c/azul*.hpp \
             "$ROOT_DIR"/examples/cpp/*/azul*.hpp "$ROOT_DIR"/examples/cpp/azul*.hpp; do
    if [ -f "$stray" ]; then
        rm -f "$stray"
        log_info "Removed stale generated header $(basename "$(dirname "$stray")")/$(basename "$stray")"
    fi
done

if [ ! -f "$HEADER_DIR/azul.h" ]; then
    log_error "Header not found: $HEADER_DIR/azul.h"
    log_info "Run 'cargo run --release -p azul-doc -- codegen all' first"
    exit 1
fi

# MinGW links an import library, not the DLL, and looks for `libazul.dll`.
# Made next to the real one in the build directory, so it is made once per
# build rather than copied per example.
if is_windows && [ ! -f "$LIB_DIR/libazul.dll" ]; then
    cp "$DLL_PATH" "$LIB_DIR/libazul.dll"
fi

# ── Step 2: the shared asset tree ───────────────────────────────────────────
# The examples resolve assets relative to the binary as "../assets/...", and
# each binary runs in target/examples-temp/<example>/, so "../assets" means
# target/examples-temp/assets. Nothing put it there, so every example that
# loads one came up empty: opengl.c reads ../assets/testdata.json and rendered
# no geometry, icons.c reads ../assets/images/favicon.ico. Copied once per run,
# and only when the source tree is newer.
ASSETS_SRC="$ROOT_DIR/examples/assets"
ASSETS_DST="$TEMP_ROOT/assets"
mkdir -p "$TEMP_ROOT"
if [ -d "$ASSETS_SRC" ]; then
    if [ ! -d "$ASSETS_DST" ] || [ "$ASSETS_SRC" -nt "$ASSETS_DST" ]; then
        mkdir -p "$ASSETS_DST"
        cp -R "$ASSETS_SRC/." "$ASSETS_DST/"
        touch "$ASSETS_DST"
        log_info "Refreshed $ASSETS_DST"
    else
        log_info "Assets up to date: $ASSETS_DST"
    fi
else
    log_warn "examples/assets not found - asset-loading examples will render empty"
fi

# ── Step 3: the scenario both runs of every example replay ──────────────────
#
# `"real": true` on the waits is load-bearing. A plain `wait` advances the
# runner's INJECTABLE clock and yields one turn of the shell's loop — exact,
# free, and the right thing for the assertion corpus, but it buys no time for
# anything outside the process. The map example fetches its tiles over HTTPS;
# with a virtual `wait` of 15 000 ms the whole scenario returned in 0.45 s and
# captured the empty tile grid. `wait_frame` then arms the frame barrier so the
# capture cannot land between a repaint request and the frame it asked for.
#
# No `close` step: the runner's result printer exits the process with the
# scenario's verdict once the last step reports. A `close` here would race the
# teardown against that report and surface as "the window closed before the
# tests reported".
# How long to let an example settle before the capture. Most reach their final
# frame almost immediately; the ones that go to the NETWORK do not, and a
# screenshot taken too early is not a slow screenshot, it is a WRONG one —
# `async` captured its empty tile grid with the `z6/34/22` coordinate
# placeholders still showing, identically in both themes, which is exactly what
# the light/dark guard below then rejected.
settle_ms() {
    case "$1" in
        # Vector map tiles over HTTPS, painted only once they arrive.
        async) echo "${AZ_SCREENSHOT_SETTLE_MS:-15000}" ;;
        *)     echo "${AZ_SCREENSHOT_SETTLE_MS:-2500}" ;;
    esac
}

write_scenario() {
    local name=$1
    local dir="$TEMP_ROOT/$name"
    mkdir -p "$dir"
    cat > "$dir/screenshot.e2e.json" <<EOF
{
  "name": "screenshot_${name//-/_}",
  "description": "Capture the native window of the ${name} example for the website. The theme is pinned from outside with AZ_THEME, so this one scenario serves both the light and the dark run.",
  "steps": [
    { "op": "wait", "ms": $(settle_ms "$name"), "real": true },
    { "op": "wait_frame" },
    { "op": "wait", "ms": 750, "real": true },
    { "op": "take_native_screenshot" }
  ]
}
EOF
}

# ── Desktop light/dark switching ────────────────────────────────────────────
#
# The theme is switched on the DESKTOP, not just pinned in the app, and that
# buys two things a pin cannot:
#
#  * the WINDOW DECORATIONS follow. KWin draws the titlebar from the desktop's
#    colour scheme and `AZ_THEME` cannot reach it, so a pinned-light capture on
#    a Breeze Dark session came out as a light window under a dark titlebar.
#  * it exercises the REAL path. A pin tells the app what to think; switching
#    the desktop makes the app DISCOVER it, through `discover()` and the
#    portal, the same way it would for a user.
#
# `AZ_THEME` is still set, as a backstop for environments with no desktop to
# switch — CI under Xvfb, a bare WM — where it at least themes the client area.
#
# The desktop is put back the way it was found on ANY exit path (see the trap):
# a script that leaves the machine in dark mode because that happened to be the
# last capture is one nobody runs twice.
DESKTOP_THEME_TOOL=""
SAVED_DESKTOP_THEME=""

detect_theme_tool() {
    if is_macos; then
        command -v osascript >/dev/null 2>&1 && { echo "macos"; return; }
    elif is_windows; then
        command -v powershell >/dev/null 2>&1 && { echo "windows"; return; }
    else
        # KDE first: on a Plasma session gsettings exists but nothing reads it.
        if [ "$(printf '%s' "$XDG_CURRENT_DESKTOP" | tr a-z A-Z)" = "KDE" ] \
           && command -v plasma-apply-colorscheme >/dev/null 2>&1; then
            echo "kde"; return
        fi
        if command -v xfconf-query >/dev/null 2>&1 \
           && xfconf-query -c xsettings -p /Net/ThemeName >/dev/null 2>&1; then
            echo "xfce"; return
        fi
        if command -v gsettings >/dev/null 2>&1 \
           && gsettings get org.gnome.desktop.interface color-scheme >/dev/null 2>&1; then
            echo "gnome"; return
        fi
    fi
    echo ""
}

save_desktop_theme() {
    case "$DESKTOP_THEME_TOOL" in
        kde)   LC_ALL=C plasma-apply-colorscheme --list-schemes 2>/dev/null \
                   | sed -n 's/^ \* \(.*\) (current color scheme)$/\1/p' ;;
        gnome) gsettings get org.gnome.desktop.interface color-scheme 2>/dev/null ;;
        xfce)  xfconf-query -c xfwm4 -p /general/theme 2>/dev/null ;;
        macos) defaults read -g AppleInterfaceStyle 2>/dev/null || echo Light ;;
        windows) powershell -Command "(Get-ItemProperty -Path HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name AppsUseLightTheme).AppsUseLightTheme" 2>/dev/null ;;
    esac
}

set_desktop_theme() {
    local theme=$1
    case "$DESKTOP_THEME_TOOL" in
        kde)
            if [ "$theme" = "dark" ]; then
                plasma-apply-colorscheme BreezeDark >/dev/null 2>&1
            else
                plasma-apply-colorscheme BreezeLight >/dev/null 2>&1
            fi
            ;;
        gnome)
            if [ "$theme" = "dark" ]; then
                gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark' 2>/dev/null
                gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita-dark' 2>/dev/null
            else
                gsettings set org.gnome.desktop.interface color-scheme 'default' 2>/dev/null
                gsettings set org.gnome.desktop.interface gtk-theme 'Adwaita' 2>/dev/null
            fi
            ;;
        xfce)
            # The WM theme decides polarity on XFCE (USER RULING 2026-09-04:
            # "azwriter should use what the titlebar uses").
            if [ "$theme" = "dark" ]; then
                xfconf-query -c xfwm4 -p /general/theme -s "Adwaita-dark" 2>/dev/null
                xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita-dark" 2>/dev/null
            else
                xfconf-query -c xfwm4 -p /general/theme -s "Adwaita" 2>/dev/null
                xfconf-query -c xsettings -p /Net/ThemeName -s "Adwaita" 2>/dev/null
            fi
            ;;
        macos)
            local v=false
            [ "$theme" = "dark" ] && v=true
            osascript -e "tell app \"System Events\" to tell appearance preferences to set dark mode to $v" >/dev/null 2>&1
            ;;
        windows)
            local v=1
            [ "$theme" = "dark" ] && v=0
            powershell -Command "New-ItemProperty -Path HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name AppsUseLightTheme -Value $v -Type Dword -Force; New-ItemProperty -Path HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name SystemUsesLightTheme -Value $v -Type Dword -Force" >/dev/null 2>&1
            ;;
        *) return 1 ;;
    esac
    # Let the settings daemon publish the change. The app reads it at startup,
    # so this only has to beat the launch below.
    sleep 1
    return 0
}

restore_desktop_theme() {
    [ -z "$DESKTOP_THEME_TOOL" ] && return 0
    [ -z "$SAVED_DESKTOP_THEME" ] && return 0
    case "$DESKTOP_THEME_TOOL" in
        kde)   plasma-apply-colorscheme "$SAVED_DESKTOP_THEME" >/dev/null 2>&1 ;;
        gnome) gsettings set org.gnome.desktop.interface color-scheme "$SAVED_DESKTOP_THEME" 2>/dev/null ;;
        xfce)  xfconf-query -c xfwm4 -p /general/theme -s "$SAVED_DESKTOP_THEME" 2>/dev/null ;;
        macos)
            if [ "$SAVED_DESKTOP_THEME" = "Dark" ]; then
                osascript -e 'tell app "System Events" to tell appearance preferences to set dark mode to true' >/dev/null 2>&1
            else
                osascript -e 'tell app "System Events" to tell appearance preferences to set dark mode to false' >/dev/null 2>&1
            fi
            ;;
        windows)
            powershell -Command "New-ItemProperty -Path HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name AppsUseLightTheme -Value $SAVED_DESKTOP_THEME -Type Dword -Force; New-ItemProperty -Path HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize -Name SystemUsesLightTheme -Value $SAVED_DESKTOP_THEME -Type Dword -Force" >/dev/null 2>&1
            ;;
    esac
    log_info "Desktop appearance restored to $SAVED_DESKTOP_THEME"
}

DESKTOP_THEME_TOOL="$(detect_theme_tool)"
if [ -n "$DESKTOP_THEME_TOOL" ] && [ -z "$AZ_SCREENSHOT_NO_DESKTOP_SWITCH" ]; then
    SAVED_DESKTOP_THEME="$(save_desktop_theme)"
    trap restore_desktop_theme EXIT INT TERM
    log_info "Desktop theme switching via '$DESKTOP_THEME_TOOL' (currently: ${SAVED_DESKTOP_THEME:-unknown})"
else
    DESKTOP_THEME_TOOL=""
    log_warn "No desktop theme switcher - decorations will not follow the capture's theme"
fi

# ── Per-example work ────────────────────────────────────────────────────────

# Compile one example against the library IN ITS BUILD DIRECTORY. Nothing is
# copied: the 53 MB (prod-release) to 261 MB (release) library used to be
# copied into all seven example folders on every run, and a copy is also how a
# stale library outlives the build that replaced it. An rpath (Linux, macOS) or
# PATH entry (Windows) pointed at target/<profile> cannot go stale.
compile_example() {
    local name=$1
    local dir="$TEMP_ROOT/$name"
    local src="$ROOT_DIR/examples/c/${name}.c"
    local bin="$dir/$name"
    is_windows && bin="$dir/${name}.exe"

    mkdir -p "$dir"

    # Skip when the binary is newer than everything it is built from. Seven
    # examples that changed in none of these rebuilt seven times a run.
    if [ -f "$bin" ] && [ "$bin" -nt "$src" ] && [ "$bin" -nt "$DLL_PATH" ] \
       && [ "$bin" -nt "$HEADER_DIR/azul.h" ]; then
        log_info "$name: up to date, not recompiling" >&2
        echo "$bin"
        return 0
    fi

    # COMPILE A COPY, never `examples/c/<name>.c` in place. `#include "azul.h"`
    # is a QUOTED include, so the compiler searches the source file's own
    # directory before any `-I`, and `examples/c/` collects an `azul.h` of its
    # own: CI copies the generated header in there before building the C
    # examples (`cp target/codegen/azul.h examples/c/`), and that untracked copy
    # then sits in the tree going stale. Building in place bound these examples
    # to a three-week-old header — 4.98 MB against the generated 5.66 MB — and
    # the resulting struct-layout mismatch across the FFI boundary was a
    # deterministic SIGSEGV a few hundred ms into `App::run`, with no diagnostic
    # of any kind. Out here the only `azul.h` in scope is the one `-I` names.
    local local_src="$dir/${name}.c"
    cp "$src" "$local_src"
    src="$local_src"

    if is_macos; then
        clang -o "$bin" -I"$HEADER_DIR" "$src" \
            -L"$LIB_DIR" -lazul \
            -framework AppKit -framework OpenGL -framework CoreGraphics \
            -framework CoreText -framework CoreFoundation \
            -Wl,-rpath,"$LIB_DIR"
    elif is_linux; then
        # Source BEFORE libraries (ld resolves left to right). X11/GL are
        # dlopen'd at run time, so only pthread/m/dl are needed here.
        gcc -o "$bin" -I"$HEADER_DIR" "$src" \
            -L"$LIB_DIR" -lazul \
            -lpthread -lm -ldl \
            -Wl,-rpath,"$LIB_DIR"
    else
        gcc -o "$bin" -I"$HEADER_DIR" "$src" \
            -L"$LIB_DIR" -lazul \
            -lopengl32 -lgdi32 -luser32 -lkernel32 -lm
    fi

    [ -f "$bin" ] || return 1
    echo "$bin"
}

# Run one example once, with the theme pinned, and keep its capture.
run_theme() {
    local name=$1 bin=$2 theme=$3
    local dir="$TEMP_ROOT/$name"
    local shot_dir="$dir/shots-$theme"
    local out_file="$dir/${name}.${OS_TAG}.${theme}.png"

    rm -rf "$shot_dir"
    mkdir -p "$shot_dir"
    cd "$dir"

    if [ -n "$DESKTOP_THEME_TOOL" ]; then
        set_desktop_theme "$theme" || true
    fi

    local -a launcher=()
    if is_linux && [ -z "$DISPLAY" ]; then
        log_info "No DISPLAY set, using xvfb-run"
        launcher=(xvfb-run -a)
    fi
    if is_windows; then
        # No rpath on Windows: the loader finds azul.dll on PATH.
        export PATH="$LIB_DIR:$PATH"
    fi

    local rc=0
    env AZ_THEME="$theme" AZ_E2E="$dir/screenshot.e2e.json" AZ_E2E_SHOT_DIR="$shot_dir" \
        timeout "$RUN_TIMEOUT" "${launcher[@]}" "$bin" \
        > "$dir/stdout.$theme.log" 2> "$dir/stderr.$theme.log" || rc=$?

    if [ $rc -ne 0 ]; then
        log_error "$name exited $rc under AZ_THEME=$theme"
        tail -40 "$dir/stderr.$theme.log" || true
        return 1
    fi

    # One capture op, so one file — but glob rather than hard-code shot-000.png
    # so an added step cannot silently make this pick up the wrong frame.
    local shot
    shot=$(ls "$shot_dir"/shot-*.png 2>/dev/null | tail -1)
    if [ -z "$shot" ] || [ ! -s "$shot" ]; then
        log_error "$name: no screenshot written to $shot_dir"
        tail -40 "$dir/stderr.$theme.log" || true
        return 1
    fi

    mv "$shot" "$out_file"
    rmdir "$shot_dir" 2>/dev/null || true
    log_success "$name $theme: $out_file ($(ls -lh "$out_file" | awk '{print $5}'))"
}

capture_example() {
    local name=$1
    local dir="$TEMP_ROOT/$name"
    local src="$ROOT_DIR/examples/c/${name}.c"

    log_step "=== $name ($OS_TAG) ==="

    if [ ! -f "$src" ]; then
        log_error "Example source not found: $src"
        log_info "Available examples:"
        ls -1 "$ROOT_DIR/examples/c/"*.c 2>/dev/null | xargs -I{} basename {} .c || echo "  (none)"
        return 1
    fi

    local bin
    if ! bin=$(compile_example "$name"); then
        log_error "$name: failed to compile"
        return 1
    fi

    write_scenario "$name"
    log_info "$name: settling $(settle_ms "$name") ms before each capture"

    run_theme "$name" "$bin" light || return 1
    run_theme "$name" "$bin" dark || return 1

    # The two captures must actually differ. A theme that is requested and not
    # honoured is silent — the run passes, two identical frames get committed,
    # and the site shows the same picture under both switches. That is what the
    # old desktop-appearance toggle did on every desktop whose appearance does
    # not live in gsettings, for months. Byte equality is a weak test and a
    # sufficient one: nothing else about these two runs differs.
    local light="$dir/${name}.${OS_TAG}.light.png"
    local dark="$dir/${name}.${OS_TAG}.dark.png"
    if cmp -s "$light" "$dark"; then
        log_error "$name: light and dark captures are byte-identical - AZ_THEME was not honoured"
        return 1
    fi

    # The unsuffixed file is the canonical capture: what CI copies into its
    # screenshot artifact. The light one.
    cp "$light" "$dir/${name}_screenshot.png"
    return 0
}

# ── Run ─────────────────────────────────────────────────────────────────────
FAILED=()
PASSED=()
for example in "${EXAMPLES[@]}"; do
    if capture_example "$example"; then
        PASSED+=("$example")
    else
        FAILED+=("$example")
    fi
done

echo
log_info "=========================================="
log_info "SUMMARY  (profile=$PROFILE, os=$OS_TAG)"
log_info "=========================================="
for e in "${PASSED[@]}"; do
    log_success "$e -> $TEMP_ROOT/$e/${e}.${OS_TAG}.{light,dark}.png"
done
for e in "${FAILED[@]}"; do
    log_error "$e FAILED (logs in $TEMP_ROOT/$e/stderr.*.log)"
done

if [ ${#FAILED[@]} -ne 0 ]; then
    exit 1
fi
log_success "ALL CAPTURES OK"
exit 0

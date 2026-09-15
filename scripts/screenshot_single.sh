#!/bin/bash
# Screenshot the C examples for the website.
#
# THE ONLY PRODUCER of the example screenshots on /ui. They are static: taken
# by hand, per OS, with this script, which installs them into
# examples/assets/screenshots; commit them from there. CI neither takes nor
# overwrites them. Publishing a fresh set for one OS:
#
#   ./scripts/screenshot_single.sh all
#   git add examples/assets/screenshots/*.<os>*.png
#
# Each passing example is installed as <example>.<os>.light.png,
# <example>.<os>.dark.png and <example>.<os>.png (the light one, the name
# api.json points at). AZ_SCREENSHOT_NO_INSTALL=1 leaves them in
# target/examples-temp/<example>/ only. Make sure api.json's `screenshot.<os>` for each example is
# `<example>.<os>.png` — `Example::load` derives the light/dark names from it.
# Flip it in the SAME commit as the images: pointed at files that do not exist
# yet, the loader falls through to calculator.png on the live site.
#
# Usage:
#   ./scripts/screenshot_single.sh                    # hello-world
#   ./scripts/screenshot_single.sh widgets            # one example
#   ./scripts/screenshot_single.sh hello-world calc   # several, ONE dll build
#   ./scripts/screenshot_single.sh all                # every example on the index
#
# Runs on Linux, macOS and Windows (MSYS/MinGW) unchanged: build the library,
# compile each example against it, run it twice under the AZ_E2E scenario
# runner — once with the desktop in light mode, once in dark — and keep the native window
# capture from each run.
#
# Output per example, in target/examples-temp/<example>/:
#   <example>.<os>.light.png   <example>.<os>.dark.png
# and, unless AZ_SCREENSHOT_NO_INSTALL is set, the same files (plus
# <example>.<os>.png) in examples/assets/screenshots/.
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
# Each theme gets its own run: the desktop is switched first, then the example
# starts and has to DISCOVER the system theme itself (see "Desktop light/dark
# switching" below). One process toggled mid-run silently produced two
# identical light captures on any desktop that does not keep its appearance
# where the old script looked — KDE reads ~/.config/kdeglobals, XFCE xfconf.

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
# A bare number is the old debug-server port (the AZ_DEBUG version of this
# script took one). Dropped rather than treated as an example named "8780".
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
# `target/prod-release` here would replace the library a release build
# publishes from that same path.
#
# `AZ_SCREENSHOT_PROFILE` overrides — e.g. `prod-release` to capture the
# shipped codegen when that tree is already built.
PROFILE="${AZ_SCREENSHOT_PROFILE:-release}"
LIB_DIR="$ROOT_DIR/target/$PROFILE"

if is_macos; then DLL_PATH="$LIB_DIR/libazul.dylib"
elif is_linux; then DLL_PATH="$LIB_DIR/libazul.so"
else DLL_PATH="$LIB_DIR/azul.dll"
fi

# ── Step 0: generated bindings that match api.json ─────────────────────────
# target/codegen (azul.h, and the Rust the library includes) is generated from
# api.json by azul-doc, and nothing regenerates it on its own. After a pull that
# changed the API, the examples here are new and the header is not: the async
# example failed to compile on macOS with "unknown type name 'AzHttpClient'".
# Regenerate whenever the header is older than api.json or the generator, and
# before the library build, which compiles the generated Rust.
needs_codegen=""
if [ ! -f "$HEADER_DIR/azul.h" ]; then
    needs_codegen="no $HEADER_DIR/azul.h"
elif [ "$ROOT_DIR/api.json" -nt "$HEADER_DIR/azul.h" ]; then
    needs_codegen="api.json is newer than the generated header"
elif [ -n "$(find "$ROOT_DIR/doc/src" -name '*.rs' -newer "$HEADER_DIR/azul.h" -print -quit 2>/dev/null)" ]; then
    needs_codegen="the generator (doc/src) is newer than the generated header"
fi
if [ -n "$needs_codegen" ]; then
    if [ -n "$AZ_SCREENSHOT_SKIP_BUILD" ]; then
        log_warn "Bindings are stale ($needs_codegen) but AZ_SCREENSHOT_SKIP_BUILD is set - not regenerating"
    else
        log_step "Regenerating bindings: $needs_codegen"
        (cd "$ROOT_DIR" && cargo run --release -p azul-doc -- codegen all) || { log_error "codegen failed"; exit 1; }
        # The library includes the generated Rust; make sure cargo rebuilds it.
        touch "$ROOT_DIR/dll/src/lib.rs"
    fi
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

# Probe the compiler first: a broken toolchain can exit 1 with no output (e.g. foreign DLLs on PATH shadowing cc1's).
CC_BIN="gcc"; is_macos && CC_BIN="clang"
cc_probe="$TEMP_ROOT/.cc-probe"
mkdir -p "$cc_probe"
echo 'int main(void){return 0;}' > "$cc_probe/probe.c"
if ! "$CC_BIN" -o "$cc_probe/probe" "$cc_probe/probe.c" 2>"$cc_probe/probe.log"; then
    log_error "$CC_BIN cannot compile a trivial program - the toolchain is broken, not the examples"
    if [ -s "$cc_probe/probe.log" ]; then
        cat "$cc_probe/probe.log"
    else
        log_error "...and it printed no diagnostic at all, which on Windows means"
        log_error "the compiler's own DLLs did not load. Check PATH: $(command -v "$CC_BIN")"
        is_windows && log_info "e.g. PATH=/c/msys64/ucrt64/bin:\$PATH ./scripts/screenshot_single.sh $*"
    fi
    exit 1
fi
rm -rf "$cc_probe"

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
  "description": "Capture the native window of the ${name} example for the website. The theme comes from the desktop (or AZ_THEME where there is none), so this one scenario serves both the light and the dark run.",
  "steps": [
    { "op": "wait", "ms": $(settle_ms "$name"), "real": true },
    { "op": "wait_frame" },
    { "op": "wait", "ms": 750, "real": true },
    { "op": "take_native_screenshot", "render_shadow": true }
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
# AZ_THEME only where there is no desktop to switch; otherwise the app must discover the theme itself.
#
# The desktop is put back the way it was found on ANY exit path (see the trap):
# a script that leaves the machine in dark mode because that happened to be the
# last capture is one nobody runs twice.
DESKTOP_THEME_TOOL=""
SAVED_DESKTOP_THEME=""

# Light (1) / dark (0), written and broadcast like the Settings app does.
windows_set_light_theme() {
    local v=$1
    powershell -NoProfile -Command "
        \$k = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize'
        New-ItemProperty -Path \$k -Name AppsUseLightTheme    -Value $v -Type Dword -Force | Out-Null
        New-ItemProperty -Path \$k -Name SystemUsesLightTheme -Value $v -Type Dword -Force | Out-Null
        Add-Type -Namespace Az -Name U32 -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr h, uint m, System.UIntPtr w, string l, uint f, uint t, out System.UIntPtr r);'
        \$r = [System.UIntPtr]::Zero
        # HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5 s per window
        [void][Az.U32]::SendMessageTimeout([System.IntPtr]0xffff, 0x1A, [System.UIntPtr]::Zero, 'ImmersiveColorSet', 2, 5000, [ref]\$r)
    " >/dev/null 2>&1
}

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
            windows_set_light_theme "$v"
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
            windows_set_light_theme "$SAVED_DESKTOP_THEME"
            ;;
    esac
    log_info "Desktop appearance restored to $SAVED_DESKTOP_THEME"
}

# ── KWin ScreenShot2 authorization (KDE Plasma on Wayland) ─────────────────
#
# On a KDE Wayland session the native capture goes through KWin's
# `org.kde.KWin.ScreenShot2` D-Bus interface — KWin has neither
# ext-image-copy-capture nor the ext-foreign-toplevel-list a portable window
# capture needs. KWin answers that interface only for an application it can
# match: it resolves the caller's PID to /proc/<pid>/exe and looks for an
# installed .desktop file whose `Exec` is that same binary and whose
# `X-KDE-DBUS-Restricted-Interfaces` names the interface. So each example gets a
# throwaway .desktop entry for the duration of its run, and the KService cache
# (ksycoca) is rebuilt so KWin can see it. All of them are removed on exit.
KWIN_AUTH_FILES=()
KWIN_SYCOCA=""
if is_linux && [ "$XDG_SESSION_TYPE" = "wayland" ] \
   && [ "$(printf '%s' "$XDG_CURRENT_DESKTOP" | tr a-z A-Z)" = "KDE" ]; then
    KWIN_SYCOCA="$(command -v kbuildsycoca6 || command -v kbuildsycoca5 || true)"
fi

authorize_kwin_screenshot() {
    local name=$1 bin=$2
    [ -z "$KWIN_SYCOCA" ] && return 0
    local apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
    local file="$apps/azul-screenshot-$name.desktop"
    mkdir -p "$apps"
    # KWin compares CANONICAL paths, so resolve symlinks here too.
    cat > "$file" <<EOF
[Desktop Entry]
Type=Application
Name=azul screenshot ($name)
Exec=$(readlink -f "$bin")
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
EOF
    KWIN_AUTH_FILES+=("$file")
    "$KWIN_SYCOCA" >/dev/null 2>&1 || true
}

# Make the window of process $1 the ACTIVE one, through a KWin script.
#
# KWin's ScreenShot2 captures the active window, and KWin's focus-stealing
# prevention keeps focus on whatever the user was typing into when a new window
# maps: launched from a busy terminal, 4 of 7 examples came back as the
# terminal (the capture refuses those — it checks the caption). A script
# setting the active window goes through `Workspace::activateWindow`, which
# does not consult focus-stealing prevention.
#
# It runs HERE, from the shell, while the app sits idle in its settle wait — not
# inside the capture. Activation makes KWin ping the window, and a window that
# cannot answer (because its thread is blocked in a D-Bus capture) is marked
# "(Not Responding)", which the capture then photographs in the titlebar.
kwin_activate_pid() {
    local pid=$1 dir=$2
    [ -z "$KWIN_SYCOCA" ] && return 0
    local plugin="azul-screenshot-activate-$pid"
    local js="$dir/activate-$pid.js"
    # `var`, no arrow functions: Plasma 5's QJSEngine. windowList/activeWindow
    # is Plasma 6, clientList/activeClient Plasma 5.
    cat > "$js" <<EOF
(function () {
  var list = typeof workspace.windowList === 'function' ? workspace.windowList() : workspace.clientList();
  for (var i = 0; i < list.length; i++) {
    if (list[i].pid === $pid) {
      if ('activeWindow' in workspace) { workspace.activeWindow = list[i]; } else { workspace.activeClient = list[i]; }
      break;
    }
  }
})();
EOF
    busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s "$plugin" >/dev/null 2>&1 || true
    busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "$js" "$plugin" >/dev/null 2>&1 || return 0
    busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting start >/dev/null 2>&1 || true
    sleep 0.3
    busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s "$plugin" >/dev/null 2>&1 || true
    rm -f "$js"
}

cleanup_on_exit() {
    restore_desktop_theme
    if [ ${#KWIN_AUTH_FILES[@]} -gt 0 ]; then
        rm -f "${KWIN_AUTH_FILES[@]}"
        [ -n "$KWIN_SYCOCA" ] && "$KWIN_SYCOCA" >/dev/null 2>&1
        log_info "Removed ${#KWIN_AUTH_FILES[@]} temporary KWin ScreenShot2 authorization(s)"
    fi
}
trap cleanup_on_exit EXIT INT TERM

DESKTOP_THEME_TOOL="$(detect_theme_tool)"
if [ -n "$DESKTOP_THEME_TOOL" ] && [ -z "$AZ_SCREENSHOT_NO_DESKTOP_SWITCH" ]; then
    SAVED_DESKTOP_THEME="$(save_desktop_theme)"
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
    # The source is compiled from a copy in $dir (see below), so a header copy
    # left in $dir shadows the generated one exactly like a stray in
    # examples/c/ does. One did on macOS: target/examples-temp/async/azul.h,
    # older than the API the example uses.
    rm -f "$dir"/azul*.h "$dir"/azul*.hpp

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
        # The DLL by path: -lazul would pick the static azul.lib in the same directory.
        gcc -o "$bin" -I"$HEADER_DIR" "$src" \
            "$DLL_PATH" \
            -lm
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

    # Pin only when there is no desktop to switch; see "Desktop light/dark
    # switching" above.
    local -a theme_pin=()
    if [ -z "$DESKTOP_THEME_TOOL" ]; then
        theme_pin=(AZ_THEME="$theme")
    fi

    local rc=0
    env "${theme_pin[@]}" AZ_E2E="$dir/screenshot.e2e.json" AZ_E2E_SHOT_DIR="$shot_dir" \
        timeout "$RUN_TIMEOUT" "${launcher[@]}" "$bin" \
        > "$dir/stdout.$theme.log" 2> "$dir/stderr.$theme.log" &
    local launcher_pid=$!

    if [ -n "$KWIN_SYCOCA" ]; then
        # Activate twice early (the window may not be mapped at the first try;
        # startup on Wayland takes up to ~2 s) and once more a second and a half
        # before a long settle ends, so the window is focused and idle — its
        # ping answered — well before the capture step runs.
        local settle; settle=$(settle_ms "$name")
        local app_pid=""
        for t in 1 1; do
            sleep "$t"
            app_pid=$(pgrep -n -f "^${bin}\$" || true)
            [ -n "$app_pid" ] && kwin_activate_pid "$app_pid" "$dir"
        done
        if [ "$settle" -gt 5000 ] && kill -0 "$launcher_pid" 2>/dev/null; then
            sleep $(( (settle - 1500) / 1000 - 2 ))
            app_pid=$(pgrep -n -f "^${bin}\$" || true)
            [ -n "$app_pid" ] && kwin_activate_pid "$app_pid" "$dir"
        fi
    fi

    wait "$launcher_pid" || rc=$?

    if [ $rc -ne 0 ]; then
        log_error "$name exited $rc in the $theme run"
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
    authorize_kwin_screenshot "$name" "$bin"
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
        log_error "$name: light and dark captures are byte-identical - the app did not follow the theme switch"
        return 1
    fi

    if [ -z "$AZ_SCREENSHOT_NO_INSTALL" ]; then
        local dest="$ROOT_DIR/examples/assets/screenshots"
        mkdir -p "$dest"
        cp "$light" "$dest/${name}.${OS_TAG}.light.png" \
            && cp "$dark" "$dest/${name}.${OS_TAG}.dark.png" \
            && cp "$light" "$dest/${name}.${OS_TAG}.png" \
            || { log_error "$name: could not install the captures into $dest"; return 1; }
        log_success "$name: installed ${name}.${OS_TAG}.{light,dark}.png and ${name}.${OS_TAG}.png into examples/assets/screenshots/"
    fi

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
    if [ -z "$AZ_SCREENSHOT_NO_INSTALL" ]; then
        log_success "$e -> examples/assets/screenshots/${e}.${OS_TAG}.{light,dark}.png (+ ${e}.${OS_TAG}.png)"
    else
        log_success "$e -> $TEMP_ROOT/$e/${e}.${OS_TAG}.{light,dark}.png"
    fi
done
for e in "${FAILED[@]}"; do
    log_error "$e FAILED (logs in $TEMP_ROOT/$e/stderr.*.log)"
done

if [ ${#FAILED[@]} -ne 0 ]; then
    exit 1
fi
log_success "ALL CAPTURES OK"
exit 0

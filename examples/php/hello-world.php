<?php
// examples/php/hello-world.php
//
// ──────────────────────────────────────────────────────────────────────
// PHP CALLBACK LIMITATION
// ──────────────────────────────────────────────────────────────────────
// Azul can only invoke callbacks (button.setOnClick, layout, etc.)
// when running inside the **Zend Engine via a native PHP extension**
// — i.e. the planned `php-extension` build of azul-dll (compiled with
// ext-php-rs, loaded via `php -d extension=azul.so`). Standard php-ffi
// REJECTS closure-to-fnpointer conversions by design (memory-safety),
// so this script can NOT actually fire on_click / layout when run as
// `php -d ffi.enable=true hello-world.php` — it will throw at the
// first registerCallback() call with a clear pointer to the docs.
//
// If you want to write desktop applications in PHP today, the FFI
// path covers everything except callbacks (POD wrappers, RefAny
// round-trip, raw libazul function calls). For the full callback
// API, use the native PHP extension once it ships, or another host
// language (Python/Lua/Ruby/Node/C#/Java/Kotlin all have working
// closure callbacks today).
//
// See doc/guide/en/internals/host-invoker.md → "Why PHP is different"
// for the full rationale and a survey of workarounds.
// ──────────────────────────────────────────────────────────────────────
//
// PHP port of examples/c/hello-world.c built against the host-invoker
// runtime helpers in `Azul.php` (see `lang_php/managed.rs`).
//
// Same shape as examples/lua/hello-world.lua and examples/perl/hello-world.pl:
//   * `Azul::refanyCreate($value)` wraps an arbitrary PHP value in an
//     AzRefAny held alive by the framework's refcount.
//   * Callbacks are plain PHP closures handed to
//     `Azul::registerCallback('Callback', $closure)`, which returns the
//     `AzCallback` cdata struct the C ABI expects. Internally that goes
//     through `Az<Kind>_createFromHostHandle(u64)` in libazul; the static
//     thunk dispatches back into PHP via the libffi closure registered at
//     module load.
//
// Requirements:
//   * PHP 7.4 or newer
//   * `ffi` extension enabled (`ffi.enable=true` in php.ini, or
//     `ffi.enable=preload` for FPM/OPcache preloading)
//   * `libazul.so` / `libazul.dylib` / `azul.dll` discoverable on the
//     dynamic-loader search path
//
// Run with:
//
//     php -dffi.enable=true hello-world.php

declare(strict_types=1);

require_once __DIR__ . '/Azul.php';

use Azul\Azul;
use Azul\App;

// ── Data model ────────────────────────────────────────────────────────
//
// `Azul::refanyCreate($value)` stashes the value in a process-wide
// id-keyed hash and returns an AzRefAny whose payload is just the id.
// The destructor that fires on the last clone calls back through
// `AzApp_setHostHandleReleaser` to drop the hash entry.

$model = ['counter' => 5];
$data  = Azul::refanyCreate($model);

// ── Callback: button click ────────────────────────────────────────────

$onClick = static function ($dataPtr, $infoPtr) {
    $ffi = Azul::lib();
    $m = Azul::refanyGet(FFI::isNull($dataPtr) ? FFI::new('AzRefAny') : $dataPtr[0]);
    if (!\is_array($m)) {
        return $ffi->AzUpdate_DoNothing;
    }
    // Mutating $m here doesn't propagate back into Azul::$_handles since
    // PHP arrays are copy-on-write. Stash the model under a fixed id and
    // mutate via reference. For brevity we cheat: get the handle id from
    // the refany and update directly through the static table.
    $id = $ffi->AzRefAny_getHostHandle($dataPtr);
    if ($id !== 0) {
        Azul::$_handles[$id]['counter']++;
    }
    return $ffi->AzUpdate_RefreshDom;
};

// ── Callback: layout (rebuilds DOM each frame) ───────────────────────

$layout = static function ($dataPtr, $infoPtr) use (&$onClick) {
    $ffi = Azul::lib();
    $id = $ffi->AzRefAny_getHostHandle($dataPtr);
    if ($id === 0) {
        return $ffi->AzDom_createBody();
    }
    $m = Azul::$_handles[$id] ?? null;
    if (!\is_array($m)) {
        return $ffi->AzDom_createBody();
    }

    // Counter label, wrapped in a div so font-size sticks.
    $labelStr = (string) $m['counter'];
    $labelBytes = FFI::new('uint8_t[' . \strlen($labelStr) . ']');
    for ($i = 0, $n = \strlen($labelStr); $i < $n; $i++) {
        $labelBytes[$i] = \ord($labelStr[$i]);
    }
    $txt = $ffi->AzString_copyFromBytes(
        FFI::cast('const uint8_t*', FFI::addr($labelBytes[0])),
        0,
        \strlen($labelStr)
    );
    $label = $ffi->AzDom_createText($txt);
    $labelWrapper = $ffi->AzDom_createDiv();
    $ffi->AzDom_addCssProperty(
        FFI::addr($labelWrapper),
        $ffi->AzCssPropertyWithConditions_simple(
            $ffi->AzCssProperty_fontSize($ffi->AzStyleFontSize_px(32.0))
        )
    );
    $ffi->AzDom_addChild(FFI::addr($labelWrapper), $label);

    // Increment button. The user-facing wrapper layer (lang_php/wrappers.rs)
    // will eventually substitute callback args via Azul::registerCallback
    // automatically; until then we wire it explicitly here.
    $btnLabel = 'Increase counter';
    $btnBytes = FFI::new('uint8_t[' . \strlen($btnLabel) . ']');
    for ($i = 0, $n = \strlen($btnLabel); $i < $n; $i++) {
        $btnBytes[$i] = \ord($btnLabel[$i]);
    }
    $btnText = $ffi->AzString_copyFromBytes(
        FFI::cast('const uint8_t*', FFI::addr($btnBytes[0])),
        0,
        \strlen($btnLabel)
    );
    $button = $ffi->AzButton_create($btnText);
    $ffi->AzButton_setButtonType(FFI::addr($button), $ffi->AzButtonType_Primary);
    $dataClone = $ffi->AzRefAny_clone($dataPtr);
    // Hand the closure to the host-invoker plumbing; receives the matching
    // AzCallback cdata struct (cb = static thunk, ctx = host-handle RefAny).
    $cb = Azul::registerCallback('Callback', $onClick);
    $ffi->AzButton_setOnClick(FFI::addr($button), $dataClone, $cb);
    $buttonDom = $ffi->AzButton_dom($button);

    // Body.
    $body = $ffi->AzDom_createBody();
    $ffi->AzDom_addChild(FFI::addr($body), $labelWrapper);
    $ffi->AzDom_addChild(FFI::addr($body), $buttonDom);

    return $ffi->AzDom_style($body, $ffi->AzCss_empty());
};

// ── Main ──────────────────────────────────────────────────────────────

$ffi = Azul::lib();
$layoutCb = Azul::registerCallback('LayoutCallback', $layout);

// WindowCreateOptions::create takes a *raw* LayoutCallbackType, not the
// AzLayoutCallback wrapper. The wrapper-emitter for setOnClick handles
// the by-value wrapper case directly, but for this constructor we extract
// the `cb` field (a fn pointer) and call _default + assign the wrapper to
// preserve the host-handle ctx — same fixup lang_lua does in its emitter.
$window = $ffi->AzWindowCreateOptions_default();
$window->window_state->layout_callback = $layoutCb;

$titleStr = 'Hello World';
$titleBytes = FFI::new('uint8_t[' . \strlen($titleStr) . ']');
for ($i = 0, $n = \strlen($titleStr); $i < $n; $i++) {
    $titleBytes[$i] = \ord($titleStr[$i]);
}
$window->window_state->title = $ffi->AzString_copyFromBytes(
    FFI::cast('const uint8_t*', FFI::addr($titleBytes[0])),
    0,
    \strlen($titleStr)
);
$window->window_state->size->dimensions->width = 400.0;
$window->window_state->size->dimensions->height = 300.0;

// NoTitleAutoInject: OS draws close/min/max buttons; framework
// auto-injects a Titlebar with drag support.
$window->window_state->flags->decorations = $ffi->AzWindowDecorations_NoTitleAutoInject;
$window->window_state->flags->background_material = $ffi->AzWindowBackgroundMaterial_Sidebar;

// Idiomatic wrapper for App so __destruct() fires AzApp_delete automatically.
$app = App::create($data, $ffi->AzAppConfig_create());
$app->run($window);
// $app falls out of scope at end-of-script.

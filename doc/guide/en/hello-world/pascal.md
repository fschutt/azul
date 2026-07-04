---
slug: hello-world/pascal
title: Hello World [Pascal]
language: en
canonical_slug: hello-world/pascal
audience: external
maturity: wip
guide_order: 24
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/pascal/hello-world.pas
last_generated_rev: dab922c5e869ab3c1ff69a2d7f4af1af19a5c27c
generated_at: 2026-07-04T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Pascal]

## Introduction

The Pascal binding is a single generated `Azul` unit (`azul.pas`) for
**Free Pascal 3.2+** that declares every `Az*` function as
`cdecl; external 'azul'`, plus a small "host-invoker" layer that lets
you write callbacks as ordinary Object Pascal classes. There is no
extra runtime and no code generator to run on your side: you download
one `.pas` file and the native library, and `fpc` does the rest.

Two compiler directives make the FFI work and must be present in any
program that uses the unit (the example below has them):

- `{$mode objfpc}{$H+}` — Object Pascal mode: classes, `override`,
  and (`$H+`) `AnsiString` as the default string type.
- `{$PACKRECORDS C}` — forces C-ABI struct layout, so `Az*` records
  passed by value match the Rust `extern "C"` ABI exactly. Without it,
  field offsets silently differ and calls corrupt memory.

## Installation

You need the Free Pascal Compiler (`fpc`, 3.2+ — `apt install
fp-compiler` / `brew install fpc`), the generated `azul.pas` unit, and
the native library. All three downloads come from the release page and
are built by the release CI.

Linux:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.pas
curl -O https://azul.rs/ui/release/$VERSION/hello-world.pas
fpc -Mobjfpc -Sh -Fl. hello-world.pas
LD_LIBRARY_PATH=. ./hello-world
```

macOS:

```sh
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.pas
curl -O https://azul.rs/ui/release/$VERSION/hello-world.pas
fpc -Mobjfpc -Sh -Fl. hello-world.pas
DYLD_LIBRARY_PATH=. ./hello-world
```

Windows:

```sh
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.pas
curl -O https://azul.rs/ui/release/$VERSION/hello-world.pas
fpc -Mobjfpc -Sh -Fl. hello-world.pas
hello-world.exe
```

`-Fl.` adds the current directory to the *linker's* library search
path; the `external 'azul'` declarations inside `azul.pas` already
tell FPC to link against `libazul`, so no `{$linklib}` or `-k-lazul`
is strictly needed as long as the library sits next to your source.

## Simple "Counter" Example

This is the exact program shipped as `examples/pascal/hello-world.pas`:

```pascal
program HelloWorld;

{$mode objfpc}{$H+}
{$PACKRECORDS C}

uses
  ctypes, sysutils,
  Azul;

type
  { Plain data model. }
  TMyModel = class
    Counter: Integer;
    constructor Create(c: Integer);
  end;

  { Click handler: bump counter and request a DOM refresh.
    Button.onClick is TYPED since the typed-callback API change: derive
    from TAzButtonOnClickCallbackInvoker, not the generic TAzCallbackInvoker. }
  TMyClickHandler = class(TAzButtonOnClickCallbackInvoker)
    procedure Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;
  end;

  { Layout handler: build the DOM. }
  TMyLayoutHandler = class(TAzLayoutCallbackInvoker)
    procedure Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;
  end;

constructor TMyModel.Create(c: Integer);
begin
  Counter := c;
end;

function MakeAzString(const s: ansistring): TAzString;
begin
  if Length(s) = 0 then
    Result := AzString_fromUtf8(nil, 0)
  else
    Result := AzString_fromUtf8(PChar(@s[1]), Length(s));
end;

procedure TMyClickHandler.Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer);
var
  m: TObject;
begin
  m := azul_refany_get(PAzRefAny(arg0));
  if (m <> nil) and (m is TMyModel) then
    TMyModel(m).Counter := TMyModel(m).Counter + 1;
  if out_ptr <> nil then
    PAzUpdate(out_ptr)^ := TAzUpdate_RefreshDom;
end;

procedure TMyLayoutHandler.Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer);
var
  m: TObject;
  body, counter_text, label_wrap, button_dom: TAzDom;
  btn: TAzButton;
  click_handler: TMyClickHandler;
  click_cb: TAzButtonOnClickCallback;
  click_data: TAzRefAny;
begin
  m := azul_refany_get(PAzRefAny(arg0));
  if (m = nil) or not (m is TMyModel) then
  begin
    body := AzDom_createBody();
    if out_ptr <> nil then
      PAzDom(out_ptr)^ := body;
    Exit;
  end;

  counter_text := AzDom_createText(MakeAzString(IntToStr(TMyModel(m).Counter)));
  label_wrap := AzDom_createDiv();
  label_wrap := AzDom_withCss(label_wrap, MakeAzString('font-size: 32px;'));
  label_wrap := AzDom_withChild(label_wrap, counter_text);

  click_handler := TMyClickHandler.Create;
  click_cb := azul_register_buttononclickcallback(click_handler);
  click_data := azul_refany_create(TMyModel(m));

  btn := AzButton_create(MakeAzString('Increase counter'));
  btn := AzButton_withButtonType(btn, TAzButtonType_Primary);
  btn := AzButton_withOnClick(btn, click_data, click_cb);
  button_dom := AzButton_dom(btn);

  body := AzDom_createBody();
  body := AzDom_withChild(body, label_wrap);
  body := AzDom_withChild(body, button_dom);

  if out_ptr <> nil then
    PAzDom(out_ptr)^ := body;
end;

var
  model: TMyModel;
  layout_handler: TMyLayoutHandler;
  data: TAzRefAny;
  layout_cb: TAzLayoutCallback;
  wco: TAzWindowCreateOptions;
  cfg: TAzAppConfig;
  app: TAzApp;

begin
  WriteLn('[azul] Pascal full-GUI hello-world starting.');

  model := TMyModel.Create(5);
  data := azul_refany_create(model);

  layout_handler := TMyLayoutHandler.Create;
  layout_cb := azul_register_layoutcallback(layout_handler);

  wco := AzWindowCreateOptions_default();
  wco.window_state.layout_callback := layout_cb;
  wco.window_state.size.dimensions.width := 400.0;
  wco.window_state.size.dimensions.height := 300.0;
  wco.window_state.flags.decorations := TAzWindowDecorations_NoTitleAutoInject;
  wco.window_state.flags.background_material := TAzWindowBackgroundMaterial_Sidebar;

  cfg := AzAppConfig_create();
  app := AzApp_create(data, cfg);
  AzApp_run(@app, wco);
end.
```

Five things to notice.

- **The host-invoker pattern** — FPC cannot hand libazul a per-callback
  function pointer that captures state (there are no closures with a C
  ABI, and struct-by-value callback signatures are off-limits for most
  managed FFIs). Instead, the `Azul` unit registers *one* C stub per
  callback kind with libazul when the unit loads (its `initialization`
  block calls `AzApp_setButtonOnClickCallbackInvoker`,
  `AzApp_setLayoutCallbackInvoker`, and so on — about 20 kinds). Your
  handler is a plain object: subclass the matching invoker class,
  override `Invoke`, then call the matching `azul_register_*` function.
  That returns a small `TAz*Callback` record carrying a numeric handle;
  when the event fires, libazul calls the unit's stub with that handle,
  the stub looks your object up in a handle table and dispatches to
  your `Invoke`.
- **Typed invoker classes** — since the typed-callback change every
  widget event has its own pair: derive from
  `TAzButtonOnClickCallbackInvoker` and register with
  `azul_register_buttononclickcallback` (analogously
  `TAzLayoutCallbackInvoker` / `azul_register_layoutcallback`). Deriving
  from a generic invoker class will not compile against the current
  unit — the `override` has no matching virtual method.
- **`azul_refany_create` / `azul_refany_get`** — the Pascal analogue of
  `RefAny`. `azul_refany_create(TObject)` stores your object in the
  unit's handle table and wraps the handle in a `TAzRefAny` that libazul
  carries around; `azul_refany_get(PAzRefAny(arg0))` in a callback hands
  the *same instance* back. Always guard with `<> nil` and `is` before
  the typecast, and fall back to an empty body / no-op on mismatch.
- **Raw `Invoke` signatures** — `arg0` is the data `PAzRefAny`, `arg1`
  the (unused here) info pointer, and `out_ptr` is where the result is
  written: a click handler stores `PAzUpdate(out_ptr)^ :=
  TAzUpdate_RefreshDom`, a layout handler stores `PAzDom(out_ptr)^ :=
  body`. Forgetting the `out_ptr` write is the classic
  "window opens but nothing happens" bug.
- **By-value builder style** — `AzDom_withChild`, `AzDom_withCss`,
  `AzButton_withOnClick` all *consume* their first argument by value and
  return a new record, hence the reassignment chains
  (`label_wrap := AzDom_withChild(label_wrap, ...)`). Do not keep using
  a record after passing it by value to a consuming function. Strings
  cross the FFI as UTF-8 buffers: the `MakeAzString` helper copies an
  `AnsiString` via `AzString_fromUtf8`.

## Build and run

From the directory containing `azul.pas`, `hello-world.pas` and the
native library:

```sh
fpc -Mobjfpc -Sh -Fl. hello-world.pas

# Linux
LD_LIBRARY_PATH=. ./hello-world
# macOS
DYLD_LIBRARY_PATH=. ./hello-world
# Windows
hello-world.exe
```

`-Mobjfpc -Sh` mirror the `{$mode objfpc}{$H+}` directives on the
command line. If your linker setup does not pick up the library from
`-Fl.`, the fully explicit variant (the one the repository's e2e
harness uses) spells everything out:

```sh
fpc -Mobjfpc -Sh -Fl. -k-L. -k-lazul hello-world.pas
```

Compiling `azul.pas` takes about two seconds and prints two
`Comment level 2 found` warnings — these come from directive text
quoted inside the generated header comment and are harmless.

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `TMyLayoutHandler.Invoke` re-runs, and the new
value renders.

## Common errors

- **`ld: symbol(s) not found` / `cannot find -lazul` at the link
  step** — the native library is not where the linker looks. Build from
  the directory holding `libazul.{so,dylib}` / `azul.dll` and keep
  `-Fl.` (or use the explicit `-k-L. -k-lazul` variant above).
- **Runtime: "library not found"** — the *loader* cannot find the
  library. Export `LD_LIBRARY_PATH=.` (Linux) /
  `DYLD_LIBRARY_PATH=.` (macOS), or place `azul.dll` next to the
  `.exe` on Windows.
- **`There is no method in an ancestor class to be overridden`** — you
  derived from the wrong invoker class or changed the `Invoke`
  signature. It must be exactly
  `Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;`
  on the typed per-widget class (`TAzButtonOnClickCallbackInvoker`,
  `TAzLayoutCallbackInvoker`, ...).
- **Counter does not update on click** — the handler never wrote
  `PAzUpdate(out_ptr)^ := TAzUpdate_RefreshDom`, or `azul_refany_get`
  returned `nil` / a different class and the `is` guard skipped the
  increment. `WriteLn` in the failure branch to verify.
- **Random crashes when passing your own records to `Az*`
  functions** — a unit is missing `{$PACKRECORDS C}`. Every unit that
  declares or passes `Az*` records by value needs it.
- **Growing memory in long-running apps** — handler objects passed to
  `azul_register_*` are currently *not* freed when the native side
  releases the handle. The example creates a fresh `TMyClickHandler`
  per layout pass, which is fine for a demo but leaks a small object
  per relayout; in a long-running app, create your invoker instances
  once and reuse them.
- **Threads** — the handle table behind `azul_refany_create` /
  `azul_register_*` is not synchronized. Register callbacks and create
  RefAnys from the main thread only; do not register/release from
  `AzThread` callbacks concurrently.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Scala]](scala.md)

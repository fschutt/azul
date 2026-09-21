---
slug: hello-world/pascal
title: Hello World [Pascal]
language: en
canonical_slug: hello-world/pascal
audience: external
maturity: mature
guide_order: 24
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/pascal/hello-world.pas
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
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

To use `libazul` from Free Pascal, you need the native prebuilt library and the generated
`Azul` unit (`azul.pas`). The unit wraps the C API in classes: you write a model class,
plain functions as callbacks, and pass ordinary Pascal strings.

The example below needs FPC 3.2.0 or newer. The unit itself builds with FPC 3.0.4 
and newer, see [Supported Compilers](#supported-compilers).

## Installation

You need the Free Pascal Compiler (`brew install fpc` / `apt install fp-compiler`), 
the `azul.pas` unit and the native library from the release page.

macOS:

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll

curl -O https://azul.rs/ui/release/$VERSION/azul.pas
curl -O https://azul.rs/ui/release/$VERSION/hello-world.pas
fpc -Fl. hello-world.pas
DYLD_LIBRARY_PATH=. ./hello-world # hello-world.exe on Windows
```


The `azul.pas` unit links the library by name, so `-Fl.` (the library search path) 
is the only flag you need. You will find the precompiled `.so` and `.dylib` files
for various different architectures on the release page, download and rename 
it to link properly.

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL with the now-generated .rs C-API bindings
cargo build -p azul-dll --release --features build-dll
```

Notice the required `--features build-dll`. The DLL lands in
`target/release/libazul.{so,dylib}` (or `azul.dll`), the unit in 
`target/codegen/azul.pas`. Copy both next to your program.

## Simple "Counter" Example

```pascal
program HelloWorld;

{$mode delphi}

uses
  SysUtils, Azul;

type
  TMyModel = class
    Counter: Integer;
  end;

function OnIncrease(Model: TMyModel; Info: TAzCallbackInfo): TAzUpdate;
begin
  Model.Counter := Model.Counter + 1;
  Result := azRefreshDom;
end;

function Layout(Model: TMyModel; Info: TAzLayoutCallbackInfo): TDom;
var
  LabelDom: TDom;
  Btn: TButton;
begin
  LabelDom := TDom.P(IntToStr(Model.Counter))
    .WithCss('font-size: 32px; margin: 0;');

  Btn := TButton.Create('Increase counter')
    .SetButtonType(azPrimary)
    .OnClick<TMyModel>(OnIncrease);

  Result := TDom.Body.AddChild(LabelDom).AddChild(Btn.Dom);
end;

var
  Model: TMyModel;
  App: TAzApp<TMyModel>;
begin
  Model := TMyModel.Create;
  Model.Counter := 5;

  App := TAzApp<TMyModel>.Create(Model, Layout);
  App.Window.Title := 'Hello World';
  App.Window.Width := 400;
  App.Window.Height := 300;
  App.Run;

  App.Free;
  Model.Free;
end.
```

`TAzApp<TMyModel>` only borrows your model, so you need free the model yourself after the app.

Callbacks are plain functions that receive your model and the callback info. The unit
automatically downcasts the `RefAny` model to the expected type and logs an error (returning `DoNothing`)
if the downcast fails. An exception raised inside a callback is caught and logged the same way, 
so that the app keeps running, even if a callback fails.

Builder methods take ownership of their arguments: `AddChild` consumes the child,
`Btn.Dom` consumes the button, and the unit frees the `TDom` that `Layout` returns.

## Supported Compilers

The example uses `{$mode delphi}`. In `{$mode objfpc}{$H+}`, generics
need `specialize` and function arguments need `@`:

```pascal
type
  TMyApp = specialize TAzApp<TMyModel>;

  Btn := TButton.Create('Increase counter')
    .SetButtonType(azPrimary)
    .specialize OnClick<TMyModel>(@OnIncrease);

  App := TMyApp.Create(Model, @Layout);
```

The `azul.pas` unit is written for Free Pascal and does not compile with Embarcadero Delphi. 
It masks floating-point exceptions when it loads, because the library relies on IEEE-754 NaN and
infinity values. If you enable FPU exceptions in your own code, mask them again before
calling into Azul.

FPC 3.0.x has no generic methods. Pass a typed callback object instead of `OnClick<TMyModel>`:

```pascal
    .OnClick(TAzButtonOnClickCallbackTypedWrapper<TMyModel>.Create(OnIncrease));
```

FPC 3.3.1 adds anonymous functions, so you *can* use inline callbacks, 
however this is not recommended, because the general goal in Azul 
is to separate the callback code from the UI code as much as possible
without any implicit captured variables.

```pascal
    .OnClick<TMyModel>(
      function(Model: TMyModel; Info: TAzCallbackInfo): TAzUpdate
      begin
        Model.Counter := Model.Counter + 1;
        Result := azRefreshDom;
      end);
```

## Build and run

From the directory containing `azul.pas`, `hello-world.pas` and the native library:

```sh
fpc -Fl. hello-world.pas

# macOS
DYLD_LIBRARY_PATH=. ./hello-world
# Linux
LD_LIBRARY_PATH=. ./hello-world
# Windows
hello-world.exe
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `App.Run` opens a native window and runs `Layout` once with your model.
2. The returned DOM is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up
   in the DOM. On click, the framework borrows your data model mutably, runs the click
   callback, observes the refresh return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one,
   and only re-updates and re-paints the counter, not the entire window.

Congratulations! Once you've got the hello-world example running, you've already mastered 80%
of the framework. As you might have guessed, more complex UI and styling are only composing
more Dom objects together and working with the various event filters.

You can now start reading about the [architecture patterns](../architecture.md) or
explore what [methods the `Dom` has to offer](../dom.md).

See you in the next tutorial!

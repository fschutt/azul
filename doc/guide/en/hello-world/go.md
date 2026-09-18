---
slug: hello-world/go
title: Hello World [Go]
language: en
canonical_slug: hello-world/go
audience: external
maturity: mature
guide_order: 17
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/go/main.go
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-18T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
---

# Hello world [Go]

In order to use the `libazul` library from Go (1.18+), you will need to install the
Go bindings from `azul.rs/ui/go`, which provide a fully idiomatic wrapper over the C API.
Internally, they use `ebitengine/purego` to dynamically load the shared library and 
handle callback trampolines at runtime.

Because this relies on `purego`, you do not need a C compiler to build or cross-compile your app, 
making single-binary deployments and cross-platform builds trivial.

## Installation

The easiest way to get started is to create a new module and fetch the bindings using `go get`:

```sh
mkdir hello-world && cd hello-world
go mod init hello-world
go get azul.rs/ui/go
```

Then download the pre-compiled native library for your platform into the same folder:

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
```

## Simple "Counter" Example

```go
package main

import (
	"fmt"
	"runtime"

	azul "azul.rs/ui/go"
)

type counterModel struct {
	Counter int
}

func onClick(model *counterModel, _ *azul.CallbackInfo) azul.Update {
	model.Counter++
	return azul.Update_RefreshDom
}

func layout(model *counterModel, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.DomCreateBody()
	label := azul.DomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	button := azul.ButtonCreate(azul.Str("Increase counter"))
	button.OnClick(model, azul.Bind(onClick))

	body.SetCss(azul.Str("p { font-size: 32px; margin: 0; }"))
	body.AddChild(label)
	body.AddChild(button.Dom())
	
	return body
}

func main() {
	// Dynamically load the embedded/downloaded shared library
	if err := azul.LoadLibrary(""); err != nil {
		panic(err)
	}

	data := &counterModel{Counter: 5}
	window := azul.NewWindowCreateOptions(azul.Bind(layout))
	app := azul.AppCreate(data, azul.AppConfigCreate())
	app.Run(window)
}
```

There are certain Go-specific things this example shows:

1. Everything is a flat `azul` namespace (otherwise, Go would have tons of problems with recursive imports).
2. Use `azul.Bind` to create the callbacks and `azul.Str` to create strings. These are the only 
   "special" functions that don't exist in the API bindings (otherwise you'd have to manage pointers manually).
3. `azul.LoadLibrary(path)` is required at startup in order to initialize the library - if `path` is an empty string,
   the function will look for the `.dll` in the current working directory. The idea is that you can `//go:embed` the `.dll` itself and then unpack it to a certain path before initializing for a "single binary" deployment.
4. The callbacks will auto-upcast and auto-downcast from `any` to `RefAny` to reduce manual boilerplate. If the
   downcast fails, it will log an error and return whatever the default is (i.e. `Update_DoNothing` or 
   `azul.DomBody()`), to keep the app running, yet also log the error:

  ```
  azul.Bind: type assertion failed, expected *main.profileModel, got *main.counterModel
  ```

See the [Observability Guide](../observability.md) for how to monitor live applications with Prometheus 
and Grafana for live errors (which would show you the `azul.Bind` errors) - additionally, you can write 
unit tests with coverage to make sure no downcast errors happen and that `stdout` is clean of warnings.

### Build and run

```sh
go run
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). 
Click the button: the counter should increment, the layout callback then re-runs, and the 
new value renders.

1. `app.Run(...)` opens a native window and runs the layout callback once with your data model.
2. The returned `Dom` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the `Dom`. On click, the framework borrows your data model mutably, runs the click callback, observes the `Update.RefreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's `Dom` and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations! Once you've got the hello-world example running, you've already mastered 
80% of the framework. As you might have guessed, more complex UI and styling are only composing 
more `Dom` objects together and working with the various event filters. 
### Cross-compilation

Cross-compilation is completely native and frictionless. Because the bindings are 100% pure Go, you don't need a C cross-compiler (like `mingw-w64`) or any special environment variables.

For example, to compile a Windows executable from a Linux or macOS host, simply use `GOOS=windows` and instruct Go to disable CGO:

```sh
# Fetch the Go package
mkdir hello-world && cd hello-world
go mod init hello-world
go get azul.rs/ui/go

# Fetch the Windows DLL target
curl -O https://azul.rs/ui/release/$VERSION/azul.dll

# Cross compile to Windows NATIVELY (no mingw required!)
export GOOS=windows
export GOARCH=amd64
go build -o hello-world.exe .
```

Congratulations! Once you've got the hello-world example running, you've already mastered 80% of the framework. You can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md).

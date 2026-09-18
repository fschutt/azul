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

In order to use the `libazul` library from Go, you will need to install the
Go bindings from `azul.rs/ui/go`, which provide a fully idiomatic wrapper over the C API.
Internally, they use `ebitengine/purego` to dynamically load the shared library and handle callback trampolines at runtime.

Because this relies on `purego`, **you do not need a C compiler to build or cross-compile your app**, making single-binary deployments and cross-platform builds trivial.

## Installation

The easiest way to get started is to create a new module and fetch the bindings using `go get`:

```sh
mkdir hello-world && cd hello-world
go mod init hello-world
go get azul.rs/ui/go
```

Then download the pre-compiled native library for your platform into the same folder:

```sh
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so

# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib

# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
```

You can now compile your Go code normally without needing `cgo` or a C compiler!

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
	body.AddChild(label.Raw())
	body.AddChild(button.Dom().Raw())
	
	return body
}

func getLibPath() string {
	switch runtime.GOOS {
	case "windows":
		return "azul.dll"
	case "darwin":
		return "libazul.dylib"
	default:
		return "libazul.so"
	}
}

func main() {
	// Dynamically load the embedded/downloaded shared library
	if err := azul.LoadLibrary(getLibPath()); err != nil {
		panic(err)
	}

	data := &counterModel{Counter: 5}
	window := azul.NewWindowCreateOptions(azul.Bind(layout))
	app := azul.AppCreate(data, azul.AppConfigCreate().Raw())
	app.RunWindow(window)
}
```

### How it works

The generated `azul-go` package does all of the heavy lifting for you dynamically at runtime:

1. **Dynamic Loading:** `azul.LoadLibrary(path)` dynamically opens the native shared library and wires up all of the Go wrappers using `purego`. This lets you seamlessly `go:embed` the `.dll` or `.so`, extract it to a temp folder, and load it dynamically without cluttering the user's system.
2. **Callbacks:** `purego` dynamically allocates machine-code trampolines in executable memory at runtime. Your Go functions are safely injected across the C ABI, eliminating CGO entirely.
3. **Data Model:** `azul.NewApp(data, config)` takes an `any` and automatically manages a handle to your Go object. The `azul.Bind()` helper uses Go 1.18 generics to automatically downcast the internal handle back into your exact model type and inject it into your callback. If it fails, it prints an error and safely aborts.
4. **Strings:** Go strings cross the boundary seamlessly through `azul.Str(s)`, which copies the bytes into a refcounted `AzString` during the call. The original Go string can be safely garbage-collected immediately.

When you run the app, `app.RunWindow(...)` opens a native window and invokes your layout callback. 

The framework continuously queries whether anything matches the event filters set up in the DOM. On click, the framework borrows your data model mutably, runs the click callback, observes the `.RefreshDom` return value, and automatically re-invokes the layout callback to render the new state.

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

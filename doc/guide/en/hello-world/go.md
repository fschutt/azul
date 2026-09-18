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

The easiest way to get started is to download the pre-packaged bundle, which 
contains `main.go`, `go.mod`, and the generated `azul-go/` directory:

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-go-$VERSION.tar.gz
tar xzf azul-go-$VERSION.tar.gz

# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
go build -o hello-world .
./hello-world

# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
go build -o hello-world .
./hello-world

# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
go build -o hello-world.exe .
hello-world.exe
```

If you prefer to manage the module yourself, you can install the library 
system-wide and simply run `go get azul.rs/ui/go`.

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

func onClick(model *counterModel, _ *azul.CallbackInfo) azul.AzUpdate {
	model.Counter++
	return azul.AzUpdate_RefreshDom
}

func layout(data *azul.RefAny, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.NewDomCreateBody()

	// Retrieve the model to read the current state
	v, _ := azul.RefAnyGet(data)
	model := v.(*counterModel)

	label := azul.NewDomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	button := azul.NewButtonCreate(azul.Str("Increase counter"))
	button.OnClick(data, azul.Bind(onClick))

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

	app := azul.NewAppWithData(&counterModel{Counter: 5}, nil)
	app.RunWindow(azul.NewWindowCreateOptions(layout))
}
```

### How it works

The generated `azul-go` package does all of the heavy lifting for you dynamically at runtime:

1. **Dynamic Loading:** `azul.LoadLibrary(path)` dynamically opens the native shared library and wires up all of the Go wrappers using `purego`. This lets you seamlessly `go:embed` the `.dll` or `.so`, extract it to a temp folder, and load it dynamically without cluttering the user's system.
2. **Callbacks:** `purego` dynamically allocates machine-code trampolines in executable memory at runtime. Your Go functions are safely injected across the C ABI, eliminating CGO entirely.
3. **Data Model:** `azul.NewAppWithData()` holds a handle to your Go object. The `azul.Bind()` helper uses Go 1.18 generics to automatically downcast the internal `RefAny` handle back into your exact model type and inject it into your callback. If it fails, it prints an error and safely aborts.
4. **Strings:** Go strings cross the boundary seamlessly through `azul.Str(s)`, which copies the bytes into a refcounted `AzString` during the call. The original Go string can be safely garbage-collected immediately.

When you run the app, `app.RunWindow(...)` opens a native window and invokes your layout callback. 

The framework continuously queries whether anything matches the event filters set up in the DOM. On click, the framework borrows your data model mutably, runs the click callback, observes the `.RefreshDom` return value, and automatically re-invokes the layout callback to render the new state.

### Cross-compilation

Cross-compilation is completely native and frictionless. Because the bindings are 100% pure Go, you don't need a C cross-compiler (like `mingw-w64`) or any special environment variables.

For example, to compile a Windows executable from a Linux or macOS host, simply use `GOOS=windows` and instruct Go to disable CGO:

```sh
# Fetch the Go package and the Windows DLL target
curl -LO https://azul.rs/ui/release/$VERSION/azul-go-$VERSION.tar.gz
tar xzf azul-go-$VERSION.tar.gz
curl -O https://azul.rs/ui/release/$VERSION/azul.dll

# Cross compile to Windows NATIVELY (no mingw required!)
export GOOS=windows
export GOARCH=amd64
go build -o hello-world.exe .
```

Congratulations! Once you've got the hello-world example running, you've already mastered 80% of the framework. You can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md).

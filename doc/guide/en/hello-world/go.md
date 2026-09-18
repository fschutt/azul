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
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
---

# Hello world in Go

The Go bindings provide a fully idiomatic wrapper over the Azul C API. The generated `azul.rs/ui/go` package automatically handles CGO trampolines, pointer conversions, and memory management for you.

Because this relies on `cgo`, you will need a **C compiler** at build time (`gcc` on Linux, Xcode Command Line Tools on macOS, or MinGW on Windows) in addition to Go 1.21+, and `CGO_ENABLED=1`. 

> [!WARNING]
> Cross-compiling `cgo` programs is generally painful — build on the target platform if you can.

## Installation

The easiest way to get started is to download the pre-packaged bundle, which contains `main.go`, `go.mod`, the generated `azul-go/` directory, and `azul.h`:

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-go-$VERSION.tar.gz
tar xzf azul-go-$VERSION.tar.gz

# linux (requires gcc on PATH)
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -lpthread -lm -ldl" go build -o hello-world .
LD_LIBRARY_PATH=. ./hello-world

# macos (requires Xcode CLT: xcode-select --install)
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText -framework CoreFoundation" go build -o hello-world .
DYLD_LIBRARY_PATH=. ./hello-world


# windows (requires MinGW gcc on PATH)
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
set CGO_ENABLED=1
set CGO_CFLAGS=-I.
set CGO_LDFLAGS=azul.dll.lib
go build -o hello-world.exe .
hello-world.exe
```

If you prefer to manage the module yourself, you can install the library system-wide and simply run `go get azul.rs/ui/go`.

## Simple "Counter" Example

This is the exact `main.go` shipped in the release. Notice how it imports `azul.rs/ui/go` and uses pure Go types and methods:

```go
package main

import (
	"fmt"

	azul "azul.rs/ui/go"
)

type counterModel struct {
	Counter int
}

func onClick(data *azul.RefAny, _ *azul.CallbackInfo) azul.AzUpdate {
	v, ok := azul.RefAnyGet(data)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	
	model, ok := v.(*counterModel)
	if !ok {
		return azul.AzUpdate_DoNothing
	}
	
	model.Counter++
	return azul.AzUpdate_RefreshDom
}

func layout(data *azul.RefAny, _ *azul.LayoutCallbackInfo) *azul.Dom {
	body := azul.NewDomCreateBody()

	v, ok := azul.RefAnyGet(data)
	if !ok {
		return body
	}
	
	model, ok := v.(*counterModel)
	if !ok {
		return body
	}

	label := azul.NewDomCreatePWithText(azul.Str(fmt.Sprintf("%d", model.Counter)))

	button := azul.NewButtonCreate(azul.Str("Increase counter"))
	button.OnClick(data, onClick)

	body.SetCss(azul.Str("p { font-size: 32px; margin: 0; }"))
	body.AddChild(label.Raw())
	body.AddChild(button.Dom().Raw())
	
	return body
}

func main() {
	app := azul.NewAppWithData(&counterModel{Counter: 5}, nil)
	app.RunWindow(azul.NewWindowCreateOptions(layout))
}
```

### How it works

The generated `azul-go` package does all of the heavy `cgo` lifting for you:

1. **Callbacks:** It automatically emits the `//export` C-ABI trampolines and fn-pointer cast helpers, so your callbacks are plain Go functions.
2. **Data Model:** `azul.NewAppWithData()` wraps your struct in an `azul.RefAny` that holds a handle to the Go object. Using `azul.RefAnyGet(data)` safely retrieves the same instance, allowing you to type-assert and mutate it in-place.
3. **Strings:** Go strings cross the boundary seamlessly through `azul.Str(s)`, which copies the bytes into a refcounted `AzString` during the call. The original Go string can be safely garbage-collected immediately.

When you run the app, `app.RunWindow(...)` opens a native window and invokes your layout callback. 

The framework continuously queries whether anything matches the event filters set up in the DOM. On click, the framework borrows your data model mutably, runs the click callback, observes the `.RefreshDom` return value, and automatically re-invokes the layout callback to render the new state.

Congratulations! Once you've got the hello-world example running, you've already mastered 80% of the framework. You can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md).

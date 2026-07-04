---
slug: hello-world/ruby
title: Hello World [Ruby]
language: en
canonical_slug: hello-world/ruby
audience: external
maturity: wip
guide_order: 19
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/ruby/hello-world.rb
last_generated_rev: 39416ebc681c6423bfdefa94dc996f613184ea0b
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Ruby]

## Introduction

The Ruby binding uses the [`ffi`](https://github.com/ffi/ffi) gem to call the
prebuilt `libazul` native library. You write idiomatic Ruby — a plain object, a
`lambda` per callback, and the smart `WindowCreateOptions.create_with_layout(&block)`
factory — and the binding wraps your object and registers the callbacks for you.

## Installation

You need **Ruby 2.6+** (system Ruby on macOS works), the **`ffi` gem**, and the
native `libazul` library.

There is no `azul` gem on rubygems.org yet (the name is taken by an unrelated
project - do not `gem install azul`). Install manually:

```sh
# --user-install avoids Gem::FilePermissionError on macOS system Ruby;
# pin 1.15.x if you are on Ruby 2.6 (ffi >= 1.16 may not build there).
gem install --user-install ffi -v 1.15.5
# download the native library:
wget -O libazul.dylib https://azul.rs/ui/release/$VERSION/libazul.dylib   # macOS
wget -O libazul.so    https://azul.rs/ui/release/$VERSION/libazul.so      # linux
# windows: download https://azul.rs/ui/release/$VERSION/azul.dll
```

Then drop the generated `azul.rb` binding next to your script and run with `-I.`:

```sh
wget https://azul.rs/ui/release/$VERSION/azul.rb
```

## Simple "Counter" Example

```ruby
require 'azul'

# Plain Ruby object - the "single source of truth" for app state.
class MyDataModel
  attr_accessor :counter
  def initialize(counter); @counter = counter; end
end

model = MyDataModel.new(5)
data  = Azul::RefAny.wrap(model)   # wrap into a handle for the framework

# Click callback: a lambda. unwrap recovers your object from the handle.
on_click = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next Azul::Update::DoNothing if m.nil?
  m.counter += 1
  Azul::Update::RefreshDom
end

# Layout callback: f(data) -> Dom.
layout = lambda do |data_ptr, _info|
  m = Azul::RefAny.unwrap(data_ptr)
  next Azul::Dom.create_body if m.nil?

  # Style a node with a plain CSS string via .with_css.
  label = Azul::Dom.create_div
    .with_css('font-size: 32px;')
    .with_child(Azul::Dom.create_text(m.counter.to_s))

  # Smart .on_click(data, &block) wraps refany + registers internally.
  button = Azul::Button.create('Increase counter')
    .with_button_type(Azul::ButtonType::Primary)
    .on_click(m, on_click)

  Azul::Dom.create_body
    .with_child(label)
    .with_child(button.dom)
end

# Smart factory hides the manual layout_callback splice; .with(opts) recursively
# assigns nested fields and auto-converts Ruby Strings to AzString.
window = Azul::WindowCreateOptions.create_with_layout(layout).with(
  window_state: {
    title: 'Hello World',
    size: { dimensions: { width: 400.0, height: 300.0 } },
    flags: {
      decorations: Azul::WindowDecorations::NoTitleAutoInject,
      background_material: Azul::WindowBackgroundMaterial::Sidebar,
    },
  },
)

app = Azul::App.create(data, Azul::AppConfig.create)
app.run(window)
```

Four things to notice.

- **`Azul::RefAny.wrap` / `.unwrap`** — wrap any Ruby object into a handle; the same
  object is handed back to callbacks. `unwrap` returns `nil` on mismatch, so guard
  with `next ... if m.nil?`.
- **Callbacks are lambdas** with the signature `|data_ptr, info|`. Use `next` (not
  `return`) to yield a value out of a lambda block.
- **Smart builders.** `WindowCreateOptions.create_with_layout(lambda)` and
  `Button.create(...).on_click(model, fn)` hide the register + splice; `.with(hash)`
  drops the field-drilling boilerplate.
- **Style with CSS strings.** `Dom#with_css('font-size: 32px;')` parses a plain CSS
  string — no manual property construction needed (`Azul::String#to_s`,
  `Option#to_opt`, `Result#unwrap`, `Vec#to_a` also exist).

## Build and run

```sh
ruby -I. hello-world.rb
```

(`-I.` tells Ruby to look in the current dir for `azul.rb`.) You should see the window
pictured on the [hello-world landing page](../hello-world.md).

## Common errors

- **`cannot load such file -- azul`** — `azul.rb` isn't on the load path. Run with
  `ruby -I. hello-world.rb`.
- **`Could not open library 'libazul'`** — put `libazul.dylib` / `libazul.so` next
  to `azul.rb`, or point `AZ_LIB_DIR` at its directory
  (`AZ_LIB_DIR=. ruby -I. hello-world.rb`). Note that `DYLD_LIBRARY_PATH` does
  NOT work with macOS system Ruby — SIP strips `DYLD_*` variables.
- **Counter does not advance** — the lambda yielded `Azul::Update::DoNothing`. Remember
  `next` returns from a block.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Node.js]](node.md)

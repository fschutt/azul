---
slug: architecture/routing
title: Routing
language: en
canonical_slug: architecture/routing
audience: external
maturity: mature
guide_order: 41
topic_only: false
short_desc: URL patterns, route params, and per-route layout callbacks
prerequisites: [dom, events/callbacks]
tracked_files:
  - core/src/resources.rs
  - layout/src/callbacks.rs
default-search-keys:
  - Route
  - RouteVec
  - RouteMatch
  - AppConfig
  - LayoutCallbackInfo
  - CallbackInfo
---

# Routing

In order to structure a larger application with multiple screens, you'd usually set up "routing", to distinguish `/settings/profile` from the `/main` UI. By default all routes simply use your `layout()` as the "default fallback". The framework automatically extracts parameters (e.g. `/profile/:uuid`) and you can access them from the `LayoutCallbackInfo` in your `layout()` callback. Additionally you can trigger a new route to load from a `CallbackInfo`, for example inside of a click event handler.

## Registering routes

You can register routes on the `AppConfig` before passing it to
`App::create`:

```rust,no_run
use azul::prelude::*;

extern "C" fn layout_home(_: RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }
extern "C" fn layout_user(_: RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }
extern "C" fn layout_settings(_: RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }

fn main() {
    let mut config = AppConfig::create();
    config.add_route("/", layout_home);
    config.add_route("/user/:id", layout_user);
    config.add_route("/settings", layout_settings);

    let app = App::create(initial_data, config);
    app.run(WindowCreateOptions::create(layout_home));
}
```

Adding a route that already exists (same pattern) replaces the
previous registration. The window registered callback (passed to `WindowCreateOptions::create`) is the default fallback, if no routing is registered.

## Pattern syntax

Patterns are slash-separated segments:

- A literal matches that exact path component
- A `:name` placeholder matches any path component and captures
  it as a parameter

| Pattern         | Request                  | Match?                     |
|---|---|---|
| `"/"`           | `"/"`                 | yes; no params            |
| `"/about"`      | `"/about"`            | yes; no params            |
| `"/about"`      | `"/settings"`         | no                        |
| `"/user/:id"`   | `"/user/42"`          | yes; `id = "42"`          |
| `"/user/:id"`   | `"/user/42/edit"`     | no (segment count mismatch) |
| `"/post/:slug"` | `"/post/hello-world"` | yes; `slug = "hello-world"` |

Patterns are matched with "specificity sorting", so that the most specific matching route wins, which is what you'd naturally expect.

## Using routes

The `LayoutCallbackInfo` internally knows which route the callback should render use `info.get_route_pattern()` and `info.get_route_param(key)` to get a requested parameter as a string.

```rust,ignore
extern "C" fn layout_user(_: RefAny, info: LayoutCallbackInfo) -> Dom {
    let id = info.get_route_param("id");
    Dom::create_h1_with_text(format!("User #{}", id.as_str()).into())
}
```

`info.get_route_pattern()` is the pattern the framework matched, set to `/` by default.
`info.get_route_param(key)` reads an extracted parameter, returns an empty String when
there is none.

Inside a callback hander (e.g. button click), you can query the `CallbackInfo` with the same APIs. Additonally, here you can use `set_route_param(key, value)` to modify a route in place. This will always trigger a `Update::RefreshDom` to fire automatically.

```rust,ignore
extern "C" fn next_page(data: RefAny, mut info: CallbackInfo) -> Update {
    let cur: u32 = info.get_route_param("page").as_str()
        .parse().unwrap_or(1);
    info.set_route_param("page", (cur + 1).to_string());
    Update::DoNothing  // set_route_param already triggers a refresh
}
```

The idea is that the framework can later use this information on the web with the `history.replaceState()` Browser API, to keep the address bar in sync. However, since the web backend is still unstable, this has not been implemented yet.

## Switching routes

`CallbackInfo::switch_route` finally allows you to switch to a new route - fundamentally similar to modifying a paraemter. Here you can also provide the values for the URL in a `StringPairVec`:

```rust,ignore
extern "C" fn open_settings(_: RefAny, mut info: CallbackInfo) -> Update {
    info.switch_route("/settings", StringPairVec::new());
    Update::RefreshDom
}

extern "C" fn open_user(data: RefAny, mut info: CallbackInfo) -> Update {
    let id = match data.downcast_ref::<u64>() { Some(i) => *i, None => return Update::DoNothing };
    let params = vec![StringPair { key: "id".into(), value: id.to_string().into() }].into();
    info.switch_route("/user/:id", params);
    Update::RefreshDom
}
```

The framework then swaps the active layout callback on the window and fires a `RefreshDom`. The focus and scroll positions are transplanted onto the new UI, if possible (otherwise they are reset).

On web, `switch_route` will call `history.pushState()` so the back
button works as users expect. However, as noted above, this isn't stable yet.

## Example

A typical app keeps each top-level view in its own callback and
shares a model:

```rust,no_run
use azul::prelude::*;

struct AppModel {
    users: Vec<User>,
    current_filter: String,
}

extern "C" fn layout_home(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(home_content(&model))
}

extern "C" fn layout_user(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    let id = info.get_route_param("id");
    let user = model.users.iter().find(|u| u.id == id.as_str());

    let body = match user {
        Some(u) => user_detail(u),
        None => not_found_page(id.as_str()),
    };
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(body)
}

extern "C" fn layout_settings(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(settings_panel(&model))
}

fn main() {
    let mut config = AppConfig::create();
    config.add_route("/", layout_home);
    config.add_route("/user/:id", layout_user);
    config.add_route("/settings", layout_settings);

    let app = App::create(RefAny::new(initial_model()), config);
    app.run(WindowCreateOptions::create(layout_home));
}
```

### Go

```go
package main

import "github.com/fschutt/azul-go"

func layoutHome(data azul.RefAny, info azul.LayoutCallbackInfo) azul.Dom { /* ... */ }
func layoutUser(data azul.RefAny, info azul.LayoutCallbackInfo) azul.Dom { /* ... */ }
func layoutSettings(data azul.RefAny, info azul.LayoutCallbackInfo) azul.Dom { /* ... */ }

func main() {
    config := azul.AppConfigCreate()
    config.AddRoute("/", layoutHome)
    config.AddRoute("/user/:id", layoutUser)
    config.AddRoute("/settings", layoutSettings)

    // ...
}
```

### Haskell

```haskell
import Azul

layoutHome :: RefAny -> LayoutCallbackInfo -> IO Dom
layoutUser :: RefAny -> LayoutCallbackInfo -> IO Dom
layoutSettings :: RefAny -> LayoutCallbackInfo -> IO Dom

main :: IO ()
main = do
    config <- appConfigCreate
              >>= appConfigAddRoute "/" layoutHome
              >>= appConfigAddRoute "/user/:id" layoutUser
              >>= appConfigAddRoute "/settings" layoutSettings
    
    -- ...
```

## HTTP Endpoint

In the planned web API, the idea is that registering the routes before startup on the `AppConfig` gives the framework enough information so that it can generate automatic XML sitemaps and match incoming HTTP requests automatically (i.e. directly route the match on a `GET /user/42` to return the respective HTML), without any need for client-side JS or WASM. This is why it's recommended to use the framework-native routing over building your own version - so that the same routing map drives both the server-rendered "first-load" HTML and the client-side re-routing.

See [Deploying to the web](../deploying/web.md) for the WASM pipeline and how the web host serves routes.

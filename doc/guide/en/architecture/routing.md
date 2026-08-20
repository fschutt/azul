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

Routes map a URL pattern to a layout callback. The same registry
drives a desktop app's view-switching and a web build's HTTP
endpoints, so a "settings page" written once shows up at the
`/settings` URL on the web and as a separate top-level layout on
desktop.

## Registering routes

Register routes on the `AppConfig` before passing it to
`App::create`:

```rust,no_run
use azul::prelude::*;

extern "C" fn layout_home(_: &mut RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }
extern "C" fn layout_user(_: &mut RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }
extern "C" fn layout_settings(_: &mut RefAny, _: LayoutCallbackInfo) -> Dom { /* ... */ todo!() }

fn main() {
    let mut config = AppConfig::create();
    config.add_route("/".into(), layout_home.into());
    config.add_route("/user/:id".into(), layout_user.into());
    config.add_route("/settings".into(), layout_settings.into());

    let app = App::create(initial_data, config);
    app.run(WindowCreateOptions::new(layout_home));
}
```

Adding a route that already exists (same pattern) replaces the
previous registration. The first registered route — or the explicit
`"/"` if present — is the initial layout.

## Pattern syntax

Patterns are slash-separated segments. Each segment is either:

- A literal — matches that exact path component.
- A `:name` placeholder — matches any path component and captures
  it as a parameter named `name`.

| Pattern         | Path                  | Match                     |
|---|---|---|
| `"/"`           | `"/"`                 | yes; no params            |
| `"/about"`      | `"/about"`            | yes; no params            |
| `"/about"`      | `"/settings"`         | no                        |
| `"/user/:id"`   | `"/user/42"`          | yes; `id = "42"`          |
| `"/user/:id"`   | `"/user/42/edit"`     | no (segment count mismatch) |
| `"/post/:slug"` | `"/post/hello-world"` | yes; `slug = "hello-world"` |

Patterns are matched in registration order; the first match wins.

## Reading the active route

The `LayoutCallbackInfo` knows which route the callback is rendering:

```rust,ignore
extern "C" fn layout_user(_: &mut RefAny, info: LayoutCallbackInfo) -> Dom {
    let id = info.get_route_param("id".into());
    Dom::create_h1_with_text(format!("User #{}", id.as_str()).into())
}
```

`info.get_route_pattern()` is the pattern the framework matched -
`"/"` when the app configured no routes at all, so a callback that
branches on it always has one string to branch on.
`info.get_route_param(key)` reads one extracted parameter, empty when
there is none.

Inside an event `CallbackInfo`, the same pair reads the same state,
via
`info.get_route_pattern()` (the active pattern) and
`info.get_route_param(key)` (one param). The `set_route_param(key,
value)` helper modifies a param in place — useful for paginated
views that want to bump `?page=2` without a full route switch:

```rust,ignore
extern "C" fn next_page(data: RefAny, mut info: CallbackInfo) -> Update {
    let cur: u32 = info.get_route_param("page".into()).as_str()
        .parse().unwrap_or(1);
    info.set_route_param("page".into(), (cur + 1).to_string().into());
    Update::DoNothing  // set_route_param already triggers a refresh
}
```

On web, `set_route_param` calls `history.replaceState()` so the URL
in the address bar stays in sync without adding a history entry.

## Switching routes from a callback

`CallbackInfo::switch_route` is the imperative form — used when a
button or menu item should navigate elsewhere:

```rust,ignore
extern "C" fn open_settings(_: RefAny, mut info: CallbackInfo) -> Update {
    info.switch_route("/settings".into(), StringPairVec::new());
    Update::RefreshDom
}

extern "C" fn open_user(data: RefAny, mut info: CallbackInfo) -> Update {
    let id = match data.downcast_ref::<u64>() { Some(i) => *i, None => return Update::DoNothing };
    let params = vec![StringPair { key: "id".into(), value: id.to_string().into() }].into();
    info.switch_route("/user/:id".into(), params);
    Update::RefreshDom
}
```

The framework swaps the active layout callback on the next frame,
fires `RefreshDom`, and reconciles the new tree against the
previous one — focus, scroll, and dataset state migrate across
matched nodes the same way a `RefreshDom` from an in-place mutation
does.

On web, `switch_route` calls `history.pushState()` so the back
button works as users expect.

## A practical multi-route layout

A typical app keeps each top-level view in its own callback and
shares a model:

```rust,no_run
use azul::prelude::*;

struct AppModel {
    users: Vec<User>,
    current_filter: String,
}

extern "C" fn layout_home(data: &mut RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(home_content(&model))
}

extern "C" fn layout_user(data: &mut RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    let id = info.get_route_param("id".into());
    let user = model.users.iter().find(|u| u.id == id.as_str());

    let body = match user {
        Some(u) => user_detail(u),
        None => not_found_page(id.as_str()),
    };
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(body)
}

extern "C" fn layout_settings(data: &mut RefAny, info: LayoutCallbackInfo) -> Dom {
    let model = data.downcast_ref::<AppModel>().unwrap();
    Dom::create_body()
        .with_child(navbar(info.get_route_pattern()))
        .with_child(settings_panel(&model))
}

fn main() {
    let mut config = AppConfig::create();
    config.add_route("/".into(), layout_home.into());
    config.add_route("/user/:id".into(), layout_user.into());
    config.add_route("/settings".into(), layout_settings.into());

    let app = App::create(RefAny::new(initial_model()), config);
    app.run(WindowCreateOptions::new(layout_home));
}
```

Pull the navbar into its own component (with `add_component_library`
or a regular function) so the active-link styling — typically a
`.is-active` class on the link whose `href` matches the current
route — only lives in one place.

## Web vs desktop

On a desktop build, the route is purely an in-memory selector for
which layout callback to run. There's no URL bar, no `history`
stack, no `window.location` — the route is application state.
`switch_route` updates that state and triggers a reconcile.

On a web build (compiled to WASM and served through azul's web
host), each registered route also maps to an HTTP endpoint on the
server side: a request to `/user/42` runs `layout_user` with the
extracted params and returns the rendered HTML, so the page is
SEO-readable on first load. `switch_route` then calls
`history.pushState()` for the in-page client-side transition;
`set_route_param` calls `history.replaceState()`. The same callback
code drives both the server-rendered first-load HTML and the
client-side updates.

See [Deploying to the web](../deploying/web.md) for the WASM-build pipeline,
the static asset layout, and how the web host serves routes.

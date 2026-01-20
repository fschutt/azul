# DOM Lifecycle

Azul follows the function paradigm of `fn(State) -> UI`: your 
layout function takes application state and returns a DOM tree. 
When state changes and the callbacks return `Update::RefreshDom`, 
Azul calls `layout()` again to get the new UI.

```rust
fn layout(state: &AppState) -> Dom {
    Dom::div().with_text(&state.message)
}
```

This model is simple and declarative. But it creates a problem.

## The Stateless DOM Problem

Each `layout()` call produces a new DOM tree. The old tree is 
dropped. For a static UI, this is fine. But some components 
need state that persists across layout calls:

- *Video player*: Opening a file, initializing a decoder, and 
  allocating GPU textures takes time. You can't do this 60 times per second.
- *WebGL canvas*: Shader compilation and buffer uploads are expensive. 
  The GL context must survive layout refreshes.
- *Network connection*: A WebSocket handshake shouldn't restart 
  every time the UI updates.

Now, for the video example, you could store the decoder handle 
in your app model and use a cheap reference-counted clone on every 
new `layout()`:

```rust
struct AppState {
    video_url: String,
    decoder: Option<DecoderHandle>, 
}
```

But the problem this creates, is that you now have business logic
polluting your data model, creating problems with serialization,
testing and synchronization with the actual "data" (the `video_url`)
in this example.

To solve this problem, Azul provides two main mechanisms:

1. **Reconciliation** - Azul can analyze which nodes moved, got created or destroyed
    1.1. **Lifecycle events** - Based on that, it can call user-defined callbacks
2. **Merge callbacks** - Azul can auto-transfer data from one frames DOM tree to the next

## Reconciliation

When `layout()` returns a new DOM, Azul compares it with the old DOM in order to try to 
find stable matches between the old `StyledDom` and the new `StyledDom`. Perhaps entire
subtrees are the same, so we could save a lot of work not re-processing them again.

The first indicator here is the `key` property (similar to React) - it represents a stable
key with explicit user defined matching like `video-player-4ac8df`. The second indicator 
is the content hash - Azul will try to intelligently analyze the CSS classes and node 
types, if no key is set. The third and final try is just by matching the position in the 
`StyledDom` - internally, the DOM is just a contigouus array of nodes with subtrees following
their parents in DFS order.

Therefore, keys matter when list order changes:

```rust
// Without keys: removing item 0 causes items 1-N to "unmount" and "remount"
// 
// With keys: only the removed item unmounts
for item in items {
    list.add_child(
        Dom::div()
            .with_key(&item.id)
            .with_text(&item.name)
    );
}
```

For static UI or lists that only append, keys aren't needed.

Azul provides two main lifecycle events:

- `AfterMount`: Node has no match in old DOM - initialize resources here
- `BeforeUnmount`: Node has no match in new DOM - cleanup / destroy resources here

```rust
struct MyAppData {
    // Only store the video path, not the actual player state
    current_video: PathBuf,
    playing: bool,
}

struct MyVideoPlayer {
    video: PathBuf,
    playing: bool,
    lib_handle: Option<*mut c_void>
}

extern "C"
fn my_layout(data: RefAny, info: LayoutCallbackInfo) -> StyledDom {
    let data = data.downcast_ref::<MyAppData>().unwrap();

    let uidata = MyVideoPlayer {
        video: data.video.clone(),
        position. data.playing,
        lib_handle: None, // see initialize_player
    };

    let dataset = RefAny::new(uidata);

    Dom::div()
        .with_callback(On::AfterMount, dataset.clone(), initialize_player)
        .with_callback(On::BeforeUnmount, dataset.clone(), teardown_player)
}

extern "C"
fn initialize_player(data: RefAny, info: CallbackInfo) -> Update {
    let mut player = info.downcast_mut::<MyVideoPlayer>().unwrap();
    if player.lib_handle.is_none() {
        // video player hasn't been initialized - load library here
        player.lib_handle = dlopen("libffmpeg.so");
        player.start_playing(); // video just starts
    }
    Update::DoNothing
}

extern "C"
fn teardown_player(data: RefAny, info: CallbackInfo) -> Update {
    let mut player = info.downcast_mut::<MyVideoPlayer>().unwrap();
    if player.lib_handle.is_some() {
        player.stop_playing();
        dlclose("libffmpeg.so", player.lib_handle);
        player.lib_handle = None;
    }
    Update::DoNothing
}
```

Now, this examplehas one problem: While the initialization / teardown would, thanks 
to automatic reconciliation, only happen on `AfterMount` / `BeforeUnmount`, there is 
no real way to "update" the video player *while* it's running. For example, if there
is a stateful API that requires us to manually "stop" the video playback, any update
would result in a completely new video player being recreated on every `layout()` update.

## Merge Callbacks

As we see above, lifecycle events handle only the problem of "first appearance" and 
"final removal". The video now correctly initializes and starts playing, but we have
no way to pause it without tearing down the entire player again.

To solve this problem, Azul runs "merge callbacks", if they are set up.


Merge callbacks transfer state from the old node to the new node:

```rust
extern "C" 
fn merge_player(mut new: RefAny, old: RefAny) -> RefAny {

    let mut old_state = old.downcast_mut::<MyVideoPlayer>().unwrap();
    let mut new_state = new.downcast_mut::<MyVideoPlayer>().unwrap();

    // First, we can transfer the already-initialized library
    // handle without re-initialization
    new_state.lib_handle = old_state.lib_handle.take();

    // And then we can do stateful actions that act only on the change
    // in the data model, to uphold the `f(State) -> Ui` paradigm
    if new_state.playing != old_state.playing {
        if new_state.playing {
            new_state.lib_handle.start_playing();
        } else {
            new_state.lib_handle.stop_playing();
        }
    }

    new_state
}

// Same function as before, except for the 3 new lines
extern "C"
fn my_layout(data: RefAny, info: LayoutCallbackInfo) -> StyledDom {
    let data = data.downcast_ref::<MyAppData>().unwrap();

    let uidata = MyVideoPlayer {
        video: data.video.clone(),
        position. data.playing,
        lib_handle: None,
    };

    let dataset = RefAny::new(uidata);

    Dom::div()
        .with_callback(On::AfterMount, dataset.clone(), initialize_player)
        .with_callback(On::BeforeUnmount, dataset.clone(), teardown_player)
        
        // !! IMPORTANT !! 
        .with_key("player-video-1") // tell Azul about the stable, unique element key
        .with_dataset(dataset.clone()) // put the dataset onto the DOM node itself
        .with_merge_callback(merge_player) // tell Azul how to "merge" two RefAny<MyVideoPlayer>
}
```

This also means that the `dataset` now becomes the main "storer" of the library handle
state - and since the UI data now lives in the UI, the `teardown_player` is automatically
called once the element finally disappears.

Use merge callbacks when state must survive layout refreshes and you want component-local 
state without polluting AppState (ex. a map component caching map tiles in the HTML UI instead
of them living the app data model - they are data, but they are pure-rendering data).

Now, on `Update::RefreshDom`:

1. `layout()` called → creates new DOM
2. Reconciliation matches old/new nodes
3. Merge callbacks **run for matched nodes with datasets**
4. `AfterMount` fires for unmatched new nodes
5. `BeforeUnmount` fires for unmatched old nodes
6. Old DOM dropped (resources already transferred)

## Conclusion

Using the reconciliation method, Azul automatically migrates internal state,
like focus positions, scroll. However, in difference to other frameworks, Azul
doesn't deny that some components are stateful, but rather gives you a convenient
API to wrap statefulness.

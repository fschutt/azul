## What is two-way data binding?

Two way data binding (at least Azul's definition of it) is when a component (for this example
let's say a text field or a spreadsheet) can update it's own state (for example, to react to
key input or mouse events) **without** the user of the text field doing anything.

For example, a user could write a text field like this:

```rust
struct DataModel {
     text_field_string: String,
}

impl Layout for DataModel {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        Dom::new(NodeType::Label(self.text_field_string))
            .with_callback(On::TextInput, Callback(update_text_field))
    }
}

fn update_text_field(state: &mut AppState<DataModel>, event: &mut CallbackInfo<DataModel>) -> UpdateScreen {
    let ch = state.windows[event.window].get_keyboard_state().current_char?;
    state.modify(|state| state.text_field_string.push(ch));
    Redraw
}
```

However, this has some serious drawbacks - the **user** of the library has to write the 
`update_text_field` callback in his application code, because the callback needs access 
`state.text_field_string` - and how could this be abstracted, you can't access `<T>.text_field_string`
before you know what the generic type `T` is going to be. Traditional toolkits use inheritance here - 
if the user derives from the `TextInput` class, then the callback can access `<T>.text_field_string`,
but this also ties the inheriting class tightly to the `TextInput` (or use something like 
`ObservableProperty` in JavaFX). Dynamically typed languages can avoid this by essentially accessing fields
that don't exist at the time of writing the `TextInput` component, stringifying the fields of the component, 
accessing the field as `T["text_field_string"]`. This however, comes at a serious performance cost 
and the loss of strict typing.

The second problem is that you'd have to write a new callback for each text field you add. If you'd want
to make a second text field, you'd have to make a second callback, or possibly hack around this with macros.
There would be no way to  abstract this further, so if that would be the final solution, every 
library user would be required to copy-paste this code somewhere in his application in order 
to get just one text field working. Luckily, there is a way around this, however, the implementor of the 
`TextInput` has to use unsafe code in order to implement it. 

> ### **Important**
> The **creator** of the `TextInput` has to use unsafe code, but the **user** (the programmer 
> who instantiates the component in his application) does not. If certain guidelines are met 
> (described further below), any component can expose a perfectly safe interface that can't be misused.
>
> This document is only useful if you want to **create** custom components, not if you just
> want to **use** them.

## Writing a text input yourself

For this example, we'll write our own text input. Text input is the simplest way to explain 
and show how two-way data binding works. All standard-library widgets (such as `Spreadsheet`
and `Calendar`) are implemented in the same way, there isn't any special-casing for standard
widgets - every "standard widget" is a custom component in itself (which is a part of what 
makes azul stand out in terms of composability).

### Example code

Here is the full code for the text input we'll write. Without further explanation, 
just try to look at the code and see if you can figure out what it does:

```rust
struct TextInput<T: Layout> {
    callback_id: DefaultCallbackId,
    marker: PhantomData<T>,
}

pub struct TextInputState {
    pub text: String 
}

impl<T: Layout> TextInput<T> {

    pub fn new(
        window: &mut FakeWindow<T>, 
        state_to_bind: &TextInputState, 
        full_data_model: &T) 
    -> Self 
    {
        let ptr = StackCheckedPointer::new(full_data_model, state_to_bind).unwrap();
        let callback_id = window.add_callback(ptr, DefaultCallback(Self::update_text_field));
        Self { callback_id, marker: PhantomData }
    }

    pub fn dom(self, state_to_render: &TextInputState) -> Dom<T> {
        let mut container_div = Dom::new(NodeType::Div).with_class("text-input-container");
        container_div.add_default_callback_id(On::TextInput, self.callback_id);
        container_div.add_child(Dom::new(NodeType::Label(state_to_render.text.clone()));
        container_div
    }

    fn update_text_field(
        data: &StackCheckedPointer<T>, 
        app_state_no_data: AppStateNoData<T>, 
        window_event: &mut CallbackInfo<T>
    ) -> UpdateScreen {
        unsafe { data.invoke_mut(TextInputState::update_state, app_state_no_data, window_event) }
    }
}

impl TextInputState {
    pub fn update_state<T: Layout>(
        &mut self, 
        app_state: AppStateNoData<T>, 
        window_event: &mut CallbackInfo<T>) 
    -> UpdateScreen 
    {
        let ch = app_state.windows[event.window].get_keyboard_state().current_char?;
        self.text.push(ch);
        Redraw
    }
}
```

And from the users side, here's how you'd instantiate and render a text input field that auto-updates the
given `TextInputState`:

```rust
struct MyAppData {
    my_text_input_1: TextInputState,
}

impl Layout for MyAppData {
    fn layout(&self, info: LayoutInfo<Self>) -> Dom<Self> {
        TextInput::new(info.window, &self.my_text_input_1, &self).dom(&self.text_input)
    }
}
```

A few things to note:

- `TextInput::update_text_field()` contains a line of unsafe code, but *it is a private function*. This is good,
  because we know that any unsafe code mistakes can only happen inside of this module, not outside of it. If you
  write your own components, **never make this function public**.
- `TextInputState::update_text_field_inner()` marked as public - this is important for delegating events,
  which we'll get into later on.
- The function signatures for `update_text_field()` and `update_state()` are exactly the same, except for
  the first argument.
- `add_default_callback_id()` has an `On::TextInput` handler, meaning the `update_text_field` is only called when a `TextInput` event is emitted by the application user.
- `add_callback()` requires a `FakeWindow` which the user can get from the `LayoutInfo<T>` during the DOM 
  construction.
- `TextInput` has a `PhantomData<T>` field, so that we can be sure that `Dom<T>`, `DefaultCallback<T>` 
  and so on all use the same type for `T`.

Now we managed to move the code from the user side into a reusable component, but how is it possible
that the component can update its own state (which requires mutable access) while the application itself
is immutably borrowed? And what is `StackCheckedPointer` doing?

### Stack-checked pointers

Updating state automatically requires us to have some form of mutable access to that state. However, 
we can't have any mutable pointers in the `layout(&self) -> Dom<Self>` function, because the
application model is already mutably borrowed. But what Azul knows is that the `DefaultCallback` 
isn't **used** immediately, it's only used once we actually invoke the callback, which can only
happen after the DOM construction is finished (because without a DOM, there would be no callbacks).

The next problem is that we can push a pointer into the DOM, but we can't store references to two 
different types within the Dom. You could wrap everything in a `Vec<Box<Any>>`, and then call something like
`dom.add_callback(Box::new(text_input_state.clone()))` - but then we would need to store the 
`TextInputState` inside the `Dom` which we don't want. What we'd technically want is a 
`Vec<Box<&Any>>`, but then the problem is: how do we get the type of `Box<&Any>` back to a 
`Box<&mut TextInputState>`? And even if you could downcast a `Box<&Any>` to a `Box<&TextInputState>`, you'd 
still need to cast the `&TextInputState` to a `&mut TextInputState`, in order to do anything with it - 
so the boxed trait does nothing for aliasing safety or type safety here. Boxed traits are a useful tool, 
but in this situation, they don't help with type safety at all - whether you cast void pointers or 
downcast a `Box<Any>`, both solutions can crash at runtime, but the `Box<Any>` needs multiple layers of 
indirection and creates problems with mutability.

The solution (and the unsafe part) is to require the programmer to push a `&TextInputState` into the Dom 
(which erases the original `TextInputState` type so that we can store multiple heterogenous types 
(`&TextInputState`, `&CalendarState`, `&MyCoolComponent`) inside a homogeneous `Vec<StackCheckedPointer>`.

![Azul Default Callback - Registering a new default callback](https://i.imgur.com/jkCvKaW.png)

The creator of the component has now to watch out for one thing - in the callback, he gets a 
`StackCheckedPointer` back - which is the same type-erased pointer that he pushed into the DOM earlier.
Now the `StackCheckedPointer` can be casted back to a `&TextInputState`, which "reconstructs" the type:

![Azul Default Callback - Invoking default callbacks](https://i.imgur.com/ubGaSOG.png)

However, if we want to access the `&TextInputState` mutably - isn't `&Thing as *const () as &mut Thing`
undefined behaviour? And how do we know that the pointer didn't point to the heap, and was erased (so that
we don't have a dangling pointer to deleted memory)? To answer these questions, we have to take a look at
how Azul wraps the data model and what is know about it:

![Azul Default Callback Memory Model Stack Heap](https://i.imgur.com/eseO86O.png)

Azul knows that the data model lives inside the `App` struct. So as long as the `App` is active, the lifetime
for the `T` is also valid. Second, Azul knows that any pointer to memory inside of the data model has to be
in the range of `&T as usize` to `&T as usize + sizeof::<T>() as usize`. Meaning:

```rust
use std::mem::size_of;

struct Something {
    a: u32,
    b: Vec<u32>,
}

fn check_stack_or_heap<T, U>(haystack: &T, needle: &U) -> bool {
    needle as usize >= haystack as usize &&
    needle as usize + size_of::<U>() <= haystack as usize + size_of::<T>()
}

fn main() {
    let something = Something { a: u32, b: vec![0, 1, 2, 3] };
    
    // true: The address of `something` is 0x1234, the size of X is `0x1234 + 16 bytes`, so
    // `&something.a` will be at `0x1234 + 4 bytes`, which is in the range of the
    // `something` struct, therefore `something.a` is contained in the `something` struct.
    //
    // Ergo, the memory for `something.a` will live as long as `something` itself
    check_stack_or_heap(&something, &something.a);

    // Also true: The vec itself (adress, len, capacity) is also stack-allocated and lives as
    // long as the `something` struct!
    check_stack_or_heap(&something, &something.b);

    // False: `&something.b[3]` accesses the adress of a heap-allocated element.
    check_stack_or_heap(&something, &something.b[3]);
}
```

The reason why azul can't allow heap-allocated pointers is fairly simple - what happens if a callback is
called that removes the heap element that the `*const ()` points at? All subsequent callbacks wouldn't 
know about this change and would dereference an invalid pointer. The lifetime of the `*const ()` wouldn't
be under the control of azul - while technically possible, it would be bound to be misused. However, there
is an easy workaround for this problem (for example, if you wanted to create multiple, heap-allocated
`TextInputState`s where you don't know how many you need at compile time?), which will be explained further
down the page.

Now what Azul knows about the `*const ()` is that:

1. The pointer is inside the boundaries of `T` to `T + size_of::<T>()` - as long as `T` is alive, 
   the pointer points to valid memory. 
   *Note: this is not valid for enums, only structs, see 
   [#84](https://github.com/maps4print/azul/issues/84) for soundness problems.*
2. If we have unique mutable access to `T`, we also have unique mutable access to the pointer (since 
   we checked that the pointer is a sub-part of the memory of `T`). Therefore, no aliasing occurs, 
   therefore no undefined behaviour or race conditions are possible.

The only thing that Azul doesn't know is the type of `T`. This would technically be solvable if Rust
would allow casting pointers via a `TypeId` (a unique ID that the compiler generates for each type), 
however, this isn't part of the Rust type system (and compiler) right now. So this work is the only
thing that a programmer can potentially mess up:

> ### Summary
> If you use `StackCheckedPointer::invoke_mut()`, then you **must** make sure that the 
> `StackCheckedPointer` gets casted to the same type that you originally pushed into the `Dom`.

## Heap-allocated states

As mentioned earlier, what happens when you do want to create a variable number of `TextInputState`s?
You can't stack-allocate them, because that wouldn't pass the `StackCheckedPointer::new()` test. The way
to solve this is to require a bit of help from the application programmer - first, instead of `.unwrap()`-ing the `StackCheckedPointer`, we simply don't push a `DefaultCallback`:

```rust
struct TextInput<T: Layout> {
    // Make this optional!
    callback_id: Option<DefaultCallbackId>,
    marker: PhantomData<T>,
}

impl<T: Layout> TextInput<T> {

    pub fn new(
        window: &mut FakeWindow<T>, 
        state_to_bind: &TextInputState, 
        full_data_model: &T) 
    -> Self 
    {
        let callback_id = StackCheckedPointer::new(full_data_model, state_to_bind).and_then(|ptr| {
            window.add_callback(ptr, DefaultCallback(Self::update_text_field))
        });
        Self { callback_id, marker: PhantomData }
    }

    pub fn dom(self, state_to_render: &TextInputState) -> Dom<T> {
        let mut container_div = Dom::new(NodeType::Div).with_class("text-input-container");
        if let Some(callback_id) = self.callback_id {
            container_div.add_default_callback_id(On::TextInput, self.callback_id);
        }
        container_div.add_child(Dom::new(NodeType::Label(state_to_render.text.clone()));
        container_div
    }
}
```

Now, if the `TextInputState` is stack-allocated, everything works as expected, but if the `TextInputState`
is stored in a `Vec` - the field will still render, but not react to input events. The idea is the following:
When an application programmer creates a `Vec<TextInputState>`, what he actually wants to know is what the 
index of the hit item was. Note that we originally exposed the `TextInputState::update_state` as **public**,
which is now important. So, a user could have a `Vec<TextInputState>` and then call 
`my_input_states[x].update_state()` inside of a regular callback safely - without any unsafe code.

For this to work, you need to watch out for two things:

- Only items that have any `Callback`s or `DefaultCallback`s attached to them get inserted into the list of
  potential hit-testable items. Since we can't call `div.add_default_callback_id()`, because we have 
  no `DefaultCallbackId`, we need some other way of telling azul that it should hit-test this item.
- To solve the hit-testing situation, the application programmer needs to attach a callback to each one of
  the `TextInputState`s in the `layout()` function (attaching at least one callback to a div makes it
  hit-testable), then the callback can retrieve the index of the clicked `TextInput` by using `CallbackInfo::get_index_in_parent()`.

So the creator of the `TextInput` needs to make the child div hit-testable:

```rust
    if let Some(callback_id) = self.callback_id {
        container_div.add_default_callback_id(On::TextInput, self.callback_id);
    }
```

And the application programmer needs to remember that any heap-allocated 
`TextInputState`s need to be hit-test seperately:

```rust
struct MyApp {
    my_text_inputs: Vec<TextInputState>,
}

impl Layout for DataModel {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        // Tip: Dom<T> implements FromIterator - useful for lists and collections!
        my_text_inputs.iter().map(|text_input| {
            // Note: The "wrapper div" around the text input now has a callback
            // and all rendered text inputs now share the same callback,
            // the .bind() method doesn't need to be called because it wouldn't 
            // succeed anyway (since the TextInputs are on the heap)
            TextInput::new(info.window, &self.my_text_input_1, &self).dom()
                .with_callback(On::TextInput, Callback(update_all_the_text_fields)))
        }).collect()
    }
}

// Calls the public `TextInputState::update_state` function on the correct TextInput
fn update_all_the_text_fields(state: &mut AppState<DataModel>, event: &mut CallbackInfo<DataModel>) -> UpdateScreen {
     let (child_idx, _parent_node_id) = event.get_index_in_parent(event.hit_dom_node)?;
     state.data.lock().ok()?.my_text_inputs[child_idx].update_state(state.without_data(), event)
}
```

Sidenote: Earlier versions of azul allowed you to make the parent hit-testable and the get the index of the
child from a callback attached to the parent. This was impractical, because the child is hierarchically 
inside of the parent, but often not visually (i.e. absolute positioned children that aren't "inside" of
their parents area). This is why the callback has to be attached to all `TextInput`s instead of the parent container.

Of course, you can wrap all of this in another (stack-allocated) component, i.e. `TextInputListComponent` and
manage the delegation of callbacks inside the `TextInputListComponent::dom()` function, for example. This allows
you to create reusable components and callbacks for your custom components and reuse these components as plug-ins
from external libraries.

## Summary

In this chapter you have learned:

- Why the callback model is slightly more complicated than in other frameworks
- Why a text input suddenly stops working if you put it on the heap instead of the stack
- What to watch out for when implementing custom components
- How to create heap-allocated lists of custom components and work around the current limitations
Unit testing can be done by using the `Dom::assert_eq` function, which
tests your DOM against a certain XML string:

```rust
struct DataModel;

fn render_counter(count: usize) -> Dom<DataModel> {
     Dom::div().with_class("test").with_child(
          Dom::label(format!("{}", count))
     )
}

#[test]
fn test_counter_ui() {
    let expected = r#"
        <div class="test">
            <p>5</p>
        </div>
    "#;

    let dom = render_counter(5);

    dom.assert_eq(expected);
}
```

You can technically also use `assert_eq!(expected, got);`, however, if the test fails,
the `Dom::assert_eq` error message has a much nicer format:

```xml
thread 'widgets::test_counter_ui' panicked at '
Expected DOM did not match:

expected: ----------
<div/>
    <div class="test"/>
        <p>4</p>
    </div>
</div>

got: ----------
<div/>
    <div class="test"/>
        <p>5</p>
    </div>
</div>
```

## Good Practices

- Typedef the name of your DataModel in your main.rs, so that it it easier to change
  in future projects.
- It's also a good idea to typedef the `&'a AppState<DataModel>` and the `&'a mut CallbackInfo`,
  in order to keep the code cleaner:

```rust
pub type DataModel = AppData;
pub type State<'a> = &'a mut AppState<DataModel>;
pub type CbInfo<'a, 'b> = &'a mut CallbackInfo<'b, DataModel>;
```

This way you can avoid a bit of typing work since rustc is smart enough to infer
the lifetimes - your callbacks should now look like this:

```rust
fn my_callback(state: State, cb: CbInfo) -> Dom<DataModel> { /* */ }
```
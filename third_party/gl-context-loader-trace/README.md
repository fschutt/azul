# opengl-function-loader

Defines a struct holding OpenGL function pointers as well as the corresponding
functions. Designed for no_std use (with `default-features = false`).

You are responsible for loading the functions yourself:

```rust
fn main() {
    /// ...

    wglMakeContextCurrent(hDC, hRC);

    let context = GenericGlContext {
        glAccum: wglGetProcAddress("glAccum"),
        glActiveTexture: wglGetProcAddress("glActiveTexture"),
        glAlphaFunc: wglGetProcAddress("glAlphaFunc"),
        glAreTexturesResident: wglGetProcAddress("glAreTexturesResident"),
        ...
    };

    // function provided by this library - also available on no_std!
    context.clear_color(0.0, 0.0, 0.0, 0.0);
    context.flush();

    wglMakeContextCurrent(NULL, NULL);
}
```

If a function pointer is set to `0x00000` (i.e. the function is not loaded),
this library will return a "default" value without panicking (i.e.
`Vec::new()`, `String::new()` or `0`). With stdlib support it will
print a "missing function: X" to stdout before returning.

**If stdlib support is enabled, `GenericGlContext` implements `gleam::Gl`!**.
The point of this is to make it possible to integrate crates that use `gleam::Gl`
with custom OpenGL function pointer loaders while using custom loaders on `no_std`.

## License

The code is somewhat copied from servo/gleam, so it retains the original
Apache/MIT license.
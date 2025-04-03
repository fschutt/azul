# azul.rs website

https://fschutt.github.io/azul/

To build documentation and the website, run `cargo build --release` in
this folder and set `BUILD_DLL=windows | mac | linux`, etc. to build the
API for every OS (or set to `all` or `none`). This will trigger building
and packaging the DLL.
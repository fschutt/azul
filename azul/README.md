# azul public api

This crate is the public API of azul. The goal is to compile azul
into a DLL to provide C bindings later on.

```sh
cbindgen --config cbindgen.toml --crate azul --output azul.h
```

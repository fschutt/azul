# azul public api

This crate is the public API of azul. The goal is to compile azul
into a DLL to provide C bindings later on.

```sh
python3 ./gen-api.py
cbindgen --config azul/cbindgen.toml --crate azul --output azul/azul.h
```

# Azul (Python)

Python bindings for the [Azul GUI framework](https://azul.rs/).

Azul is a cross-platform, immediate-mode-styled, retained-mode desktop GUI
toolkit written in Rust, with a CSS-based styling model and a flexbox layout
engine.

```python
from azul import *

def layout(data, info):
    return Dom.create_body().with_child(
        Dom.create_text("Hello, Azul!")
    ).style(Css.empty())

# ... see https://azul.rs/ for the full API.
```

This package ships a native CPython extension module (`azul`) built from the
Rust `azul-dll` crate via [maturin](https://www.maturin.rs/). No external
shared library is required at runtime — the native code is bundled into the
wheel.

## License

MIT. See https://github.com/maps4print/azul.

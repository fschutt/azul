# Needs the azul extension module next to this file: build it with
# `cargo build --release -p azul-dll --features python-extension` and copy
# libazul.so / libazul.dylib to azul.so (azul.pyd on Windows).
from styles import style_btn

style_btn_value = style_btn()
print("style_btn: %d properties" % len(style_btn_value))

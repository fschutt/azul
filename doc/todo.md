Es gibt 892 Fehler. Ich sehe mehrere Kategorien von Problemen:

- Fehlende Typen: CallbackInfoRefData, CallbackChange, CssPropertyCache, FcFontCache, ImageCache, OptionGlContextPtr - das sind interne Typen, die nicht in api.json sind
- `crate :: props` - falsche Modul-Pfade (sollte `azul_css::props` sein)
- VecDestructor-Varianten fehlen: NoDestructor, DefaultRust fehlen, External erwartet Tuple statt Unit
- Fehlende Trait-Implementierungen: Clone, Debug, PartialEq, etc. für viele Typen
- FontRef Felder fehlen: run_destructor, parsed
- Generischer Typ T nicht gefunden

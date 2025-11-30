# XHTML & Workbench

This chapter covers how to work with XHTML templates and the Azul Workbench tool.

## XHTML Templates

Azul supports loading UI layouts from XHTML files. This allows you to separate 
your UI structure from your application logic.

```rust
// Load a DOM from an XHTML file
let dom = Dom::from_file("layout.xhtml")?;
```

## Hot Reloading

The Azul Workbench provides hot-reloading capabilities for rapid UI development. 
Changes to your XHTML files are reflected immediately without recompiling.

## Workbench Tool

The Workbench is a development tool that allows you to:

- Preview UI layouts in real-time
- Edit CSS styles interactively
- Inspect the DOM hierarchy
- Debug layout issues

[Back to overview](https://azul.rs/guide)
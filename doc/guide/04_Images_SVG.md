# Images & SVG

This chapter covers how to work with images and SVG files in Azul.

## Loading Images

Azul supports various image formats through its image decoding utilities. 
You can load images from files or from memory.

```rust
// Load an image from a file
let image = Image::from_file("path/to/image.png")?;

// Load an image from memory
let bytes = include_bytes!("image.png");
let image = Image::from_memory(bytes)?;
```

## SVG Support

Azul includes SVG parsing and rendering capabilities. You can render SVG 
content directly or convert it to a texture.

```rust
// Parse SVG from a string
let svg = Svg::from_string(svg_content)?;

// Render SVG to a texture
let texture = svg.render(width, height)?;
```

## Image Callbacks

For dynamic image rendering, you can use image callbacks that render 
content on demand.

[Back to overview](https://azul.rs/guide)
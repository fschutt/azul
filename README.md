# azul

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Coverage Status](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.23%20stable-blue.svg)]()

azul is a stylable GUI framework using `webrender` and `limn-layout` for rendering

## Design

azul is a library, that, in difference to pretty much all other GUI libraries
uses a functional, data-driven design. `azul` requires your application data to
serialize itself into a user interface. Due to CSS stylesheets, your application can
be styled however you want.

That said, `azul` is probably not the most efficient UI library.

![azul design diagram](https://i.imgur.com/M5NGnBk.png)

## Goals

This library is not done yet. Once it is done, it should support the following:

- Basic elements
	- Label
    - List Box
    - Checkbox
    - Radio
    - Three-state checkbox
    - Dropdown
    - Button
    - Menu
    - Either / Or checkbox
    - GlImage
    - Ordered list (1. 2. 3.)
    - Unordered list

- OpenGL helpers
    - Rectangle
    - Rectangle with borders
    - Circle
    - Dashed / dotted circles

- Layout (parent)
    - direction (horizontal, vertical, horizontal-reverse, vertical-reverse)
    - wrap (nowrap, wrap, wrap-reverse)
    - justify-content: start, end, center, space-between, space-around, space-evenly
    - align-items: start, end, center, stretch
    - align-content: start, end, center, stretch, space-between, space-around

- Layout (child)
    - order: `number`

- Media rules
    - query window width & height

## Use-cases

The goal is to be used in desktop applications that require special rendering
(ex. image / vector editors) as well as games. Currently the backend is tied to
OpenGL.

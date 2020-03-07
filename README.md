# Azul - Desktop GUI framework

## WARNING: The features advertised in this README may not work yet.

<!-- [START badges] -->
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![Coverage Status](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![Rust Compiler Version](https://img.shields.io/badge/rustc-1.33%20stable-blue.svg)]()
<!-- [END badges] -->

> Azul is a free, functional, immediate mode GUI framework that is built on the Mozilla WebRender rendering engine for rapid development
of desktop applications that are written in Rust and use a CSS / DOM model for layout and styling.

###### [Website](https://azul.rs/) | [Tutorial / user guide](https://github.com/maps4print/azul/wiki) | [Video demo](https://www.youtube.com/watch?v=kWL0ehf4wwI) | [Discord Chat](https://discord.gg/nxUmsCG)

## About

Azul is not ready for usage or production yet. For a description of the
project and usage, please read [the wiki](https://github.com/maps4print/azul/wiki/Old-Readme).

Azul will be ready when the 0.1 version releases on crates.io. If you want to
be notified when this happens, please click "Watch Repository > Releases only"
at the top of this page.

There are currently issues with dependency management, linkage on Windows, rendering, redrawing and
documentation issues as well as issues with the layout solver. **These are all known issues, 
please refrain from reporting them over and over again**. The current working branch is 
[`unvendor_dependencies_2`](https://github.com/maps4print/azul/tree/unvendor_dependencies_2),
the examples on that branch should work. As with many opensource repositories,
the programmatic model of Azul is great, but it's not battle-tested or usable yet.

Yes, Azul is still under development, but very, very slowly. The screenshots on azul.rs were
taken before the new layout solver was implemented, that's why the current state differs from
the renderings found on the website.

## License

MIT
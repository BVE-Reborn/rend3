# rend3

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/BVE-Reborn/rend3/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rend3)](https://crates.io/crates/rend3)
[![Documentation](https://docs.rs/rend3/badge.svg)](https://docs.rs/rend3)
![License](https://img.shields.io/crates/l/rend3)
[![Matrix](https://img.shields.io/static/v1?label=rend3%20dev&message=%23rend3&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3:matrix.org)
[![Matrix](https://img.shields.io/static/v1?label=rend3%20users&message=%23rend3-users&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3-users:matrix.org)
[![Discord](https://img.shields.io/discord/451037457475960852?color=7289DA&label=discord)](https://discord.gg/mjxXTVzaDg)


Easy to use, customizable, efficient 3D renderer library built on wgpu.

Library is under active development. While internals are might change in the
future, the external api remains stable, with only minor changes occuring as
features are added.

## Examples

Take a look at the [examples] for getting started with the api. The examples
will show how the core library and helper crates can be used.

[examples]: https://github.com/BVE-Reborn/rend3/tree/trunk/examples

#### Screenshots

These screenshots are from the scene_viewer example.

![scifi-base](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/src/scene_viewer/scifi-base.jpg)
![example](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/src/scene_viewer/screenshot.jpg)
![bistro](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/src/scene_viewer/bistro.jpg)
![emerald-square](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/src/scene_viewer/emerald-square.jpg)

## Crates

The `rend3` ecosystem is composed of a couple core crates which provide most
of the functionality and exensibility to the library, extension crates, and
integration crates

#### Core

- `rend3`: The core crate. Performs all handling of world data, provides the
  Renderer and RenderGraph and defines vocabulary types.
- `rend3-routine`: Implementation of various "Render Routines" on top of the
  RenderGraph. Also provides for re-usable graphics work. Provides PBR
  rendering, Skyboxes, Shadow Rendering, and Tonemapping.

#### Extensions

There are extension crates that are not required, but provide pre-made bits
of useful code that I would recommend using.

- `rend3-framework`: Vastly simplifies correct handling of the window and
  surface across platforms.
- `rend3-gltf`: Modular gltf file and scene loader.

#### Integration

Integration with other external libraries are also offered. Due to external
dependencies, the versions of these may increase at a much higher rate than
the rest of the ecosystem.

- `rend3-anim`: Skeletal animation playback utilities. Currently tied to rend3-gltf.
- `rend3-egui`: Integration with the [egui](https://github.com/emilk/egui)
  immediate mode gui.

## Purpose

`rend3` tries to fulfill the following usecases:
 1. Games and visualizations that need a customizable, and efficient renderer.
 2. Projects that just want to put objects on screen, but want lighting and effects.
 3. A small cog in a big machine: a renderer that doesn't interfere with the rest of the program.

`rend3` is not:
 1. A framework or engine. It does not include all the parts needed to make an
    advanced game or simulation nor care how you structure your program.
    If you want a very basic framework to deal with windowing and event loop management,
    `rend3-framework` can help you. This will always be optional and is just there to help
    with the limited set of cases it can help.

## Future Plans

I have grand plans for this library. An overview can be found in the issue
tracker under the [enhancement] label.

[enhancement]: https://github.com/BVE-Reborn/rend3/labels/enhancement

## Matrix Chatroom

We have a matrix chatroom that you can come and join if you want to chat
about using rend3 or developing it:

[![Matrix](https://img.shields.io/static/v1?label=rend3%20dev&message=%23rend3&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3:matrix.org)
[![Matrix](https://img.shields.io/static/v1?label=rend3%20users&message=%23rend3-users&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3-users:matrix.org)

If discord is more your style, our meta project has a channel which mirrors
the matrix rooms:

[![Discord](https://img.shields.io/discord/451037457475960852?color=7289DA&label=discord)](https://discord.gg/mjxXTVzaDg)

## Helping Out

We welcome all contributions and ideas. If you want to participate or have
ideas for this library, we'd love to hear them!

License: MIT OR Apache-2.0 OR Zlib

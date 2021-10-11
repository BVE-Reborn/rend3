# rend3

![GitHub Workflow Status](https://img.shields.io/github/workflow/status/BVE-Reborn/rend3/CI)
[![Crates.io](https://img.shields.io/crates/v/rend3)](https://crates.io/crates/rend3)
[![Documentation](https://docs.rs/rend3/badge.svg)](https://docs.rs/rend3)
![License](https://img.shields.io/crates/l/rend3)
[![Matrix](https://img.shields.io/static/v1?label=rend3%20matrix&message=%23rend3&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3:matrix.org)
[![Discord](https://img.shields.io/discord/451037457475960852?color=7289DA&label=discord)](https://discord.gg/mjxXTVzaDg)


Easy to use, customizable, efficient 3D renderer library built on wgpu.

Library is under active development. While internals will likely change quite a bit,
the external api will only experience minor changes as features are added.

To use rend3 add the following to your Cargo.toml:

```
rend3 = "0.2.0"
```

## Screenshots

![scifi-base](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/scifi-base.jpg)
![emerald-square](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/emerald-square.jpg)
![example](https://raw.githubusercontent.com/BVE-Reborn/rend3/trunk/examples/scene-viewer/screenshot.jpg)

## Examples

Take a look at the [examples] getting started with the api.

[examples]: https://github.com/BVE-Reborn/rend3/tree/trunk/examples

## Purpose

`rend3` tries to fulfill the following usecases:
 1. Games and visualizations that need a customizable, and efficient renderer.
 2. Small projects that just want to put objects on screen, but want lighting and effects.
 3. A small cog in a big machine: a renderer doesn't interfere with the rest of the program.

`rend3` is not:
 1. A framework or engine. It does not include all the parts needed to make an advanced game or simulation nor care how you structure
    your program. I do have plans for a `rend3-util` (or similar) crate that is a very basic framework for the second use case listed above.

## Helper Crates

This is the primary crate which holds the main [`Renderer`] struct. We have some other crates:
- `rend3-gltf`: contains code to load from a .gltf or .glb file.
- `rend3-pbr`: contains render routines for drawing PBR-style objects.

## GPU Culling

On Vulkan and DX12 "gpu mode" is enabled by default, which uses modern bindless resources and gpu-based culling. This reduces CPU load and allows sigifigantly more powerful culling.

## Future Plans

I have grand plans for this library. An overview can be found in the issue tracker
under the [enhancement] label.

[enhancement]: https://github.com/BVE-Reborn/rend3/labels/enhancement

## Matrix Chatroom

We have a matrix chatroom that you can come and join if you want to chat about using rend3
or developing it:

[![Matrix](https://img.shields.io/static/v1?label=rend3%20matrix&message=%23rend3&color=blueviolet&logo=matrix)](https://matrix.to/#/#rend3:matrix.org)

If discord is more your style, our meta project has a channel which mirrors the matrix:

[![Discord](https://img.shields.io/discord/451037457475960852?color=7289DA&label=discord)](https://discord.gg/mjxXTVzaDg)

## Helping Out

We welcome all contributions and ideas. If you want to participate or have ideas for this library, we'd love to hear them!

License: MIT OR Apache-2.0 OR Zlib

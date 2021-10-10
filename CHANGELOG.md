# Changelog

All notable changes to this project will be documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to cargo's version of [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Per Keep a Changelog there are 6 main categories of changes:

- Added
- Changed
- Deprecated
- Removed
- Fixed
- Security

- [Unreleased](#unreleased)
- [v0.2.0](#v020)
- [v0.1.1](#v011)
- [v0.1.0](#v010)
- [v0.0.6](#v006)
- [v0.0.5](#v005)
- [v0.0.4](#v004)
- [v0.0.3](#v003)
- [v0.0.2](#v002)
- [v0.0.1](#v001)
- [Diffs](#diffs)

## Unreleased
- rend3-egui: Created an initial [egui](https://github.com/emilk/egui) immediate mode GUI integration @MindSwipe

## v0.2.0

Released 2021-10-09

This release saw a signifigant amount of cpu side optimization.
In a 50k object scene, the render loop went from taking 16ms to taking 1.75ms, a 9x speedup

### Added
- rend3: Added an explicit dependency on wgpu-core and wgpu-hal to ensure bug-fixes are included.
- rend3-gltf: Add the ability to turn off image's default features.
- rend3-gltf: Add support for ktx2 and dds images.
- rend3-gltf: Expose implementation functions
- rend3-pbr-bake: Added crate for automatic light baking to a texture.

### Changed
- rend3: IMPORTANT: You now must call output.present() in order for things to show up on screen.
- rend3: `Material` is now a trait and render routines can specify their own material.
  - `rend3::types::Material` is now `rend3_pbr::material::PbrMaterial`
  - `Renderer::update_material` no longer takes a `MaterialChange`, it takes a completely new material.
- rend3: renderlists refactored to have a generic Input and Output.
  - `Renderer::renderer` passes through the Input and Output.
  - OutputFrame is now a user-side only utility.
- rend3: AddTexture* will now create/upload the texture before the call returns.
- rend3: ResourceHandle<T> now prints the actual reference count while debug printing.
- rend3: `CameraProjection` now deals with view matrix, instead of location/yaw/pitch @scoopr
- rend3: Update to glam 0.19 @scoopr
- rend3 & rend3-gltf: update to wgpu 0.11
- rend3-pbr: Input is of type `()` and Output is type `&TextureView`.
- rend3-gltf: Added labels to all the different data types.
- rend3-gltf: Errors now use a `SsoString` instead of a `String` for the URI.
- rend3-gltf: All implementation functions no longer write into a single `&mut LoadedGltfScene`, but return their results directly.

### Fixed
- rend3: No longer require pipeline rebuilds when bind group length updates.
- rend3-pbr: fix rendering of cutout objects in shadow passes.
- rend3-pbr: remove redundant material changes in cpu mode.

## v0.1.1

Released 2021-09-13

### Added
- rend3-pbr: `set_ambient_lighting` sets the ambient light value, making sure no lighting result is less than `ambient * color`.

### Fixed
- rend3: properly exported `ExtendedAdapterInfo`

## v0.1.0

Released 2021-09-11

### Added
- rend3: Materials now have a `Transparency` field that supports Opaque, Cutout, and Blend transparency modes.
- rend3: `AlbedoComponent::TextureVertexValue` to represent `texture * vertex * constant`
- rend3: Mipmaps can be generated automatically on the gpu without the user needing to upload them.
- rend3: `Renderer::add_texture_2d_from_texture` which allows you to make a new texture from a set another texture's mipmaps.
- rend3 & rend3-pbr: Use `wgpu-profiler` to generate GPU timings that show up as `RendererStatistics`.
- rend3 & rend3-pbr: Annotate most code with `profiling` spans.
- rend3 & rend3-pbr: Add a `distance` field that signifies how much space shadows should take up.
- rend3-pbr: All major rendering spans are labeled and show up in renderdoc
- rend3-pbr: Multisampling support
- rend3-pbr: Support for transparency as well as stable gpu-culling to preserve transparency sort order.

### Changed
- rend3: **SUBTLE** All handles are now refcounted.
  - Handles are now `!Copy`. Functions taking handles now accept a reference to a handle.
  - If you want to keep something alive, you need to keep the handle to it alive.
  - `Object`s will keep `Material`s/`Mesh`s alive.
  - `Material`s will keep `Texture`s alive.
  - All resources are removed the `render()` after they are deleted.
- rend3: Externalize all surfaces, adapters, devices, etc.
  - Instead of using a `RendererBuilder`, construct an Instance/Adapter/Device with `rend::create_iad` and pass that to `Renderer::new`.
  - Surfaces are now controlled by the user. There is a convinence function `rend3::configure_surface` to make this smoother.
- rend3: `Texture::width` and `Texture::height` replaced with `Texture::size`
- rend3: `RendererStatistics` is now an alias for `Vec<wgpu_profiler::GpuTimerScopeResult>`
- rend3: `Texture::mip_levels` was split into `mip_count` and `mip_source` allowing you to easily auto-generate mipmaps.
- rend3: Changed limits such that intel gets CPU mode until [wgpu#1111](https://github.com/gfx-rs/wgpu/issues/1111) gets resolved.
- rend3-pbr: creation and resizing's `resolution` argument replaced with options containing resolution and sample count.
  
### Updated
- Dependencies:
  - `glam` 0.17 -> 0.18

### Fixed
- rend3-pbr: Shadow artifacting due to incorrect face culling when rendering shadow passes
- rend3-pbr: CPU mode drawing failed to account for proper vertex offsets
- rend3-pbr: Non-normalized normal maps now work correctly.
- rend3-pbr: Growing the GPU mode texture descriptor list no longer causes panic
- rend3-gltf: albedo-texture UV transform is now respected
- rend3-gltf: image loading now properly caches images

### Removed
- rend3: `RendererBuilder` replaced with explicit calls to `Renderer::new`.
- rend3: `Renderer::delete_*` functions were removed in favor of refcounting.

## v0.0.6

Released 2021-08-22

### Added
- `rend3_types` crate with all datatypes.

### Changed
- `rend3::datatypes` is now renamed to `rend3::types`. It is a reexport of `rend3_types`.
- `rend3::types::TextureFormat` is a reexport of `wgpu_types::TextureFormat`.
- Replaced Renderlists with Render Routines
  - `rend3_list` crate is now `rend3_pbr`.
  - `Swapchain` mentions are now `Surface`.
  - `set_options` is now `set_internal_surface_options`
  - The following are now functions of the render routine:
    - `resize` is on both.
    - `set_background_texture` now takes an `Option<TextureHandle>` and there is no `clear_background_texture`.
- `log` is now used for logging as opposed to `tracing`, so `env_logger` should be used over `wgpu_subscriber`.

### Updated
- `wgpu` 0.7 -> 0.10
- `glam` 0.13 -> 0.17

### Removed
- `span` and `span_transfer`, due to `tracing`'s removal.
- All ties to `switchyard`.
- Shader compiling infrastructure is gone, shaders must be wgsl or pre-compiled to spirv.

## v0.0.5

Released 2021-03-10

### Fixed
- Fixed silly math error when converting to `glam` to `v0.13.0`.

## v0.0.4

Yanked 2021-03-10

Released 2021-03-08

#### Updated
- `glam` to `v0.13.0`

## v0.0.3

Released 2021-03-06

#### Added
- Internal: use cargo-release for all releases

## v0.0.2

Released 2021-03-06

#### Changes
- Update documentation.

## v0.0.1

Released 2021-03-06

#### Added
- First release of `rend3`.

## Diffs

- [Unreleased](https://github.com/BVE-Reborn/rend3/compare/v0.2.0...HEAD)
- [v0.2.0](https://github.com/BVE-Reborn/rend3/compare/v0.1.1...v0.2.0)
- [v0.1.1](https://github.com/BVE-Reborn/rend3/compare/v0.1.0...v0.1.1)
- [v0.1.0](https://github.com/BVE-Reborn/rend3/compare/v0.0.6...v0.1.0)
- [v0.0.6](https://github.com/BVE-Reborn/rend3/compare/v0.0.5...v0.0.6)
- [v0.0.5](https://github.com/BVE-Reborn/rend3/compare/v0.0.4...v0.0.5)
- [v0.0.4](https://github.com/BVE-Reborn/rend3/compare/v0.0.3...v0.0.4)
- [v0.0.3](https://github.com/BVE-Reborn/rend3/compare/v0.0.2...v0.0.3)
- [v0.0.2](https://github.com/BVE-Reborn/rend3/compare/v0.0.1...v0.0.2)

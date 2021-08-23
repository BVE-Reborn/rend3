# Changelog

All notable changes to this project will be documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to cargo's version of [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

- [Unreleased](#unreleased)
- [v0.0.6](#v006)
- [v0.0.5](#v005)
- [v0.0.4](#v004)
- [v0.0.3](#v003)
- [v0.0.2](#v002)
- [v0.0.1](#v001)
- [Diffs](#diffs)

## Unreleased

### Added
- Multisampling support in PBR render routine.

### Changed
- PBR render routine creation and resizing's `resolution` argument replaced with options containing resolution and sample count.

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

- [Unreleased](https://github.com/BVE-Reborn/rend3/compare/v0.0.6...HEAD)
- [v0.0.6](https://github.com/BVE-Reborn/rend3/compare/v0.0.5...v0.0.6)
- [v0.0.5](https://github.com/BVE-Reborn/rend3/compare/v0.0.4...v0.0.5)
- [v0.0.4](https://github.com/BVE-Reborn/rend3/compare/v0.0.3...v0.0.4)
- [v0.0.3](https://github.com/BVE-Reborn/rend3/compare/v0.0.2...v0.0.3)
- [v0.0.2](https://github.com/BVE-Reborn/rend3/compare/v0.0.1...v0.0.2)

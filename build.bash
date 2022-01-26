#!/usr/bin/env bash

set -ex

case $1 in
    shaders)
        shift
        glslc rend3-routine/shaders/src/blit.vert                                        -O -g -o rend3-routine/shaders/spirv/blit.vert.spv
        glslc rend3-routine/shaders/src/blit.frag                                        -O -g -o rend3-routine/shaders/spirv/blit-linear.frag.spv
        glslc rend3-routine/shaders/src/blit.frag   -DSRGB_CONVERT                       -O -g -o rend3-routine/shaders/spirv/blit-srgb.frag.spv
        glslc rend3-routine/shaders/src/cull.comp   -DATOMIC_CULL                        -O -g -o rend3-routine/shaders/spirv/cull-atomic-cull.comp.spv
        glslc rend3-routine/shaders/src/cull.comp   -DPREFIX_CULL                        -O -g -o rend3-routine/shaders/spirv/cull-prefix-cull.comp.spv
        glslc rend3-routine/shaders/src/cull.comp   -DPREFIX_SUM                         -O -g -o rend3-routine/shaders/spirv/cull-prefix-sum.comp.spv
        glslc rend3-routine/shaders/src/cull.comp   -DPREFIX_OUTPUT                      -O -g -o rend3-routine/shaders/spirv/cull-prefix-output.comp.spv
        glslc rend3-routine/shaders/src/depth.vert  -DCPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/depth.vert.cpu.spv
        glslc rend3-routine/shaders/src/depth.frag  -DCPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/depth-opaque.frag.cpu.spv
        glslc rend3-routine/shaders/src/depth.frag  -DCPU_DRIVEN -DALPHA_CUTOUT          -O -g -o rend3-routine/shaders/spirv/depth-cutout.frag.cpu.spv
        glslc rend3-routine/shaders/src/depth.vert  -DGPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/depth.vert.gpu.spv
        glslc rend3-routine/shaders/src/depth.frag  -DGPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/depth-opaque.frag.gpu.spv
        glslc rend3-routine/shaders/src/depth.frag  -DGPU_DRIVEN -DALPHA_CUTOUT          -O -g -o rend3-routine/shaders/spirv/depth-cutout.frag.gpu.spv
        glslc rend3-routine/shaders/src/opaque.vert -DCPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/opaque.vert.cpu.spv
        glslc rend3-routine/shaders/src/opaque.vert -DCPU_DRIVEN -DBAKING                -O -g -o rend3-routine/shaders/spirv/opaque-baking.vert.cpu.spv
        glslc rend3-routine/shaders/src/opaque.frag -DCPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/opaque.frag.cpu.spv
        glslc rend3-routine/shaders/src/opaque.vert -DGPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/opaque.vert.gpu.spv
        glslc rend3-routine/shaders/src/opaque.vert -DGPU_DRIVEN -DBAKING                -O -g -o rend3-routine/shaders/spirv/opaque-baking.vert.gpu.spv
        glslc rend3-routine/shaders/src/opaque.frag -DGPU_DRIVEN                         -O -g -o rend3-routine/shaders/spirv/opaque.frag.gpu.spv
        glslc rend3-routine/shaders/src/skybox.vert                                      -O -g -o rend3-routine/shaders/spirv/skybox.vert.spv
        glslc rend3-routine/shaders/src/skybox.frag                                      -O -g -o rend3-routine/shaders/spirv/skybox.frag.spv

        naga rend3-routine/shaders/spirv/blit.vert.spv                --keep-coordinate-space rend3-routine/shaders/wgsl/blit.vert.wgsl
        naga rend3-routine/shaders/spirv/blit-linear.frag.spv         --keep-coordinate-space rend3-routine/shaders/wgsl/blit-linear.frag.wgsl
        naga rend3-routine/shaders/spirv/blit-srgb.frag.spv           --keep-coordinate-space rend3-routine/shaders/wgsl/blit-srgb.frag.wgsl
        naga rend3-routine/shaders/spirv/cull-prefix-cull.comp.spv    --keep-coordinate-space rend3-routine/shaders/wgsl/cull-prefix-cull.comp.wgsl
        naga rend3-routine/shaders/spirv/cull-prefix-sum.comp.spv     --keep-coordinate-space rend3-routine/shaders/wgsl/cull-prefix-sum.comp.wgsl
        naga rend3-routine/shaders/spirv/cull-prefix-output.comp.spv  --keep-coordinate-space rend3-routine/shaders/wgsl/cull-prefix-output.comp.wgsl
        naga rend3-routine/shaders/spirv/depth.vert.cpu.spv           --keep-coordinate-space rend3-routine/shaders/wgsl/depth.vert.cpu.wgsl
        naga rend3-routine/shaders/spirv/depth-opaque.frag.cpu.spv    --keep-coordinate-space rend3-routine/shaders/wgsl/depth-opaque.frag.cpu.wgsl
        naga rend3-routine/shaders/spirv/depth-cutout.frag.cpu.spv    --keep-coordinate-space rend3-routine/shaders/wgsl/depth-cutout.frag.cpu.wgsl
        naga rend3-routine/shaders/spirv/opaque.vert.cpu.spv          --keep-coordinate-space rend3-routine/shaders/wgsl/opaque.vert.cpu.wgsl
        naga rend3-routine/shaders/spirv/opaque-baking.vert.cpu.spv   --keep-coordinate-space rend3-routine/shaders/wgsl/opaque-baking.vert.cpu.wgsl
        naga rend3-routine/shaders/spirv/opaque.frag.cpu.spv          --keep-coordinate-space rend3-routine/shaders/wgsl/opaque.frag.cpu.wgsl
        naga rend3-routine/shaders/spirv/skybox.vert.spv              --keep-coordinate-space rend3-routine/shaders/wgsl/skybox.vert.wgsl
        naga rend3-routine/shaders/spirv/skybox.frag.spv              --keep-coordinate-space rend3-routine/shaders/wgsl/skybox.frag.wgsl
    ;;
    web-bin)
        shift
        if [ $1 == "release" ]; then
            shift
            BUILD_FLAGS=--release
            WASM_BUILD_DIR=release
        else
            WASM_BUILD_DIR=debug
        fi
        RUSTFLAGS=--cfg=web_sys_unstable_apis cargo build --target wasm32-unknown-unknown $BUILD_FLAGS --bin $@
        mkdir -p target/generated/
        rm -rf target/generated/*
        cp -r examples/$1/resources/ target/generated/ || true
        sed "s/{{example}}/$1/g" > target/generated/index.html < examples/resources/index.html
        wasm-bindgen --out-dir target/generated --target web target/wasm32-unknown-unknown/$WASM_BUILD_DIR/$1.wasm
    ;;
    serve)
        shift
        simple-http-server target/generated -c wasm,html,js -i
    ;;
    ci)
        cargo fmt
        cargo clippy
        cargo test
        cargo rend3-doc
        RUSTFLAGS=--cfg=web_sys_unstable_apis cargo clippy --target wasm32-unknown-unknown --workspace --exclude rend3-imgui --exclude rend3-imgui-example
        cargo deny check
    ;;
    help | *)
        set +x
        echo "rend3 build script"
        echo ""
        echo "Contains helpful sets of commands for rend3's development."
        echo "Building rend3 does not require any of these. Just use cargo as normal."
        echo ""
        echo "Subcommands:"
        echo "help                         This message."
        echo "web-bin [release] <BINARY>   Builds BINARY as wasm, and runs wasm-bindgen on the result."
        echo "shaders                      Calls glslc to build all glsl to spirv, and calls naga to create wgsl from it."
        echo "serve                        Serve a web server from target/generated using simple-http-server."
esac

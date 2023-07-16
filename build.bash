#!/usr/bin/env bash

set -ex

case $1 in
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
        cargo deny --all-features check
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
        echo "serve                        Serve a web server from target/generated using simple-http-server."
esac

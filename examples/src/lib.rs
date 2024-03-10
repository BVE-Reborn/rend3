#![allow(clippy::arc_with_non_send_sync)]

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

mod animation;
mod cube;
mod cube_no_framework;
mod egui;
mod scene_viewer;
mod skinning;
mod static_gltf;
mod textured_quad;

#[cfg(target_arch = "wasm32")]
use log::info as println;

#[cfg(test)]
mod tests;

struct ExampleDesc {
    name: &'static str,
    run: fn(),
}

const EXAMPLES: &[ExampleDesc] = &[
    ExampleDesc { name: "animation", run: animation::main },
    ExampleDesc { name: "cube", run: cube::main },
    ExampleDesc { name: "cube-no-framework", run: cube_no_framework::main },
    ExampleDesc { name: "egui", run: egui::main },
    ExampleDesc { name: "scene_viewer", run: scene_viewer::main },
    ExampleDesc { name: "skinning", run: skinning::main },
    ExampleDesc { name: "static_gltf", run: static_gltf::main },
    ExampleDesc { name: "textured_quad", run: textured_quad::main },
];

fn print_examples() {
    println!("Usage: cargo run <example_name>\n");
    println!("Available examples:");
    for example in EXAMPLES {
        println!("    {}", example.name);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn main_with_name(example_name: Option<String>) {
    let Some(example_name) = example_name else {
        print_examples();
        return;
    };

    let Some(example) = EXAMPLES.iter().find(|example| example.name == example_name) else {
        println!("Unknown example: {}\n", example_name);
        print_examples();
        return;
    };

    (example.run)();
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    main_with_name(std::env::args().nth(1))
}

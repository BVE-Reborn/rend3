mod animation;
mod cube;
mod cube_no_framework;
mod egui;
mod scene_viewer;
mod skinning;
mod static_gltf;
mod textured_quad;

#[cfg(test)]
mod tests;

struct ExampleDesc {
    name: &'static str,
    run: fn(),
}

const EXAMPLES: &[ExampleDesc] = &[
    ExampleDesc {
        name: "animation",
        run: animation::main,
    },
    ExampleDesc {
        name: "cube",
        run: cube::main,
    },
    ExampleDesc {
        name: "cube-no-framework",
        run: cube_no_framework::main,
    },
    ExampleDesc {
        name: "egui",
        run: egui::main,
    },
    ExampleDesc {
        name: "scene-viewer",
        run: scene_viewer::main,
    },
    ExampleDesc {
        name: "skinning",
        run: skinning::main,
    },
    ExampleDesc {
        name: "static-gltf",
        run: static_gltf::main,
    },
    ExampleDesc {
        name: "textured-quad",
        run: textured_quad::main,
    },
];

fn print_examples() {
    println!("Usage: cargo run <example_name>");
    println!();
    println!("Available examples:");
    for example in EXAMPLES {
        println!("    {}", example.name);
    }
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    let Some(example_name) = std::env::args().nth(1) else {
        print_examples();
        return;
    };

    let Some(example) = EXAMPLES.iter().find(|example| example.name == example_name) else {
        println!("Unknown example: {}", example_name);
        println!();
        print_examples();
        return;
    };

    (example.run)();
}

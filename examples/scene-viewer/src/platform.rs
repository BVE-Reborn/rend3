#![allow(non_snake_case, unused)]
cfg_if::cfg_if!(
    if #[cfg(target_os = "macos")] {
        // https://stackoverflow.com/a/16125341 reference
        pub mod Scancodes {
            pub const W: u32 = 0x0D;
            pub const A: u32 = 0x00;
            pub const S: u32 = 0x01;
            pub const D: u32 = 0x02;
            pub const Q: u32 = 0x0C;
            pub const Z: u32 = 0x06;
            pub const P: u32 = 0x23;
            pub const SEMICOLON: u32 = 0x29;
            pub const QUOTE: u32 = 0x27;
            pub const COMMA: u32 = 0x2B;
            pub const PERIOD: u32 = 0x2F;
            pub const SHIFT: u32 = 0x38;
            pub const ESCAPE: u32 = 0x35;
            pub const LALT: u32 = 0x3A; // Actually Left Option
        }
    } else if #[cfg(target_arch = "wasm32")] {
        pub mod Scancodes {
            use winit::keyboard::KeyCode;
            pub const W: u32 = KeyCode::KeyW as u32;
            pub const A: u32 = KeyCode::KeyA as u32;
            pub const S: u32 = KeyCode::KeyS as u32;
            pub const D: u32 = KeyCode::KeyD as u32;
            pub const Q: u32 = KeyCode::KeyQ as u32;
            pub const Z: u32 = KeyCode::KeyZ as u32;
            pub const P: u32 = KeyCode::KeyP as u32;
            pub const SEMICOLON: u32 = KeyCode::Semicolon as u32;
            pub const QUOTE: u32 = KeyCode::Quote as u32;
            pub const COMMA: u32 = KeyCode::Comma as u32;
            pub const PERIOD: u32 = KeyCode::Period as u32;
            pub const SHIFT: u32 = KeyCode::ShiftLeft as u32;
            pub const ESCAPE: u32 = KeyCode::Escape as u32;
            pub const LALT: u32 = KeyCode::AltLeft as u32;
        }
    } else {
        pub mod Scancodes {
            pub const W: u32 = 0x11;
            pub const A: u32 = 0x1E;
            pub const S: u32 = 0x1F;
            pub const D: u32 = 0x20;
            pub const Q: u32 = 0x10;
            pub const Z: u32 = 0x2C;
            pub const P: u32 = 0x19;
            pub const SEMICOLON: u32 = 0x27;
            pub const QUOTE: u32 = 0x28;
            pub const COMMA: u32 = 0x33;
            pub const PERIOD: u32 = 0x34;
            pub const SHIFT: u32 = 0x2A;
            pub const ESCAPE: u32 = 0x01;
            pub const LALT: u32 = 0x38;
        }
    }
);

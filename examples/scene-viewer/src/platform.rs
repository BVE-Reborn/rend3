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
            pub const W: u32 = 0x57;
            pub const A: u32 = 0x41;
            pub const S: u32 = 0x53;
            pub const D: u32 = 0x44;
            pub const Q: u32 = 0x51;
            pub const Z: u32 = 0x5a;
            pub const P: u32 = 0x50;
            pub const SEMICOLON: u32 = 0xba;
            pub const QUOTE: u32 = 0xde;
            pub const COMMA: u32 = 0xbc;
            pub const PERIOD: u32 = 0xbe;
            pub const SHIFT: u32 = 0x10;
            pub const ESCAPE: u32 = 0x1b;
            pub const LALT: u32 = 0x12;
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

use shaderc::Compiler;
use std::cell::RefCell;

pub struct TLS {
    pub(crate) shader_compiler: Compiler,
}
impl TLS {
    pub fn new() -> Option<RefCell<Self>> {
        Some(RefCell::new(Self {
            shader_compiler: Compiler::new()?,
        }))
    }
}

impl AsMut<TLS> for TLS {
    fn as_mut(&mut self) -> &mut TLS {
        self
    }
}

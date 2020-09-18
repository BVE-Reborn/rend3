use shaderc::Compiler;

pub struct TLS {
    pub(crate) shader_compiler: Compiler,
}
impl TLS {
    pub fn new() -> Option<Self> {
        Some(Self {
            shader_compiler: Compiler::new()?,
        })
    }
}

impl AsMut<TLS> for TLS {
    fn as_mut(&mut self) -> &mut TLS {
        self
    }
}

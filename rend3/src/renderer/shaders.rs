use crate::{
    renderer::{COMPUTE_POOL, SHADER_COMPILE_PRIORITY},
    Renderer, TLS,
};
use fnv::FnvBuildHasher;
use futures::future::{ready, Either};
use parking_lot::Mutex;
use shaderc::{CompileOptions, OptimizationLevel, ShaderKind, SourceLanguage, TargetEnv};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    future::Future,
    hash::{Hash, Hasher},
    mem::discriminant,
    sync::Arc,
};
use switchyard::Switchyard;
use wgpu::{Device, ShaderModule, ShaderModuleSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderArguments {
    file: String,
    defines: Vec<(String, Option<String>)>,
    kind: ShaderKind,
    debug: bool,
}

impl Hash for ShaderArguments {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.file.as_bytes());
        state.write_u8(self.debug as u8);
        state.write_u8(self.kind as u8);
        for (key, value) in &self.defines {
            state.write(key.as_bytes());
            discriminant(&value).hash(state);
            if let Some(ref value) = value {
                state.write(value.as_bytes())
            }
        }
    }
}

pub struct ShaderManager {
    cache: Mutex<HashMap<ShaderArguments, Arc<ShaderModule>, FnvBuildHasher>>,
}
impl ShaderManager {
    pub fn new() -> Self {
        let cache = Mutex::new(HashMap::with_hasher(FnvBuildHasher::default()));

        Self { cache }
    }

    pub fn compile_shader<TLD>(
        &self,
        renderer: &Arc<Renderer<TLD>>,
        args: ShaderArguments,
    ) -> impl Future<Output = Arc<ShaderModule>>
    where
        TLD: AsMut<TLS> + 'static,
    {
        let renderer_clone = renderer.clone();

        if let Some(module) = self.cache.lock().get(&args) {
            return Either::Left(ready(Arc::clone(module)));
        }

        Either::Right(
            renderer
                .yard
                .spawn_local(COMPUTE_POOL, SHADER_COMPILE_PRIORITY, move |tls| async move {
                    // TODO: make fallible
                    let contents = std::fs::read_to_string(&args.file).expect("Could not read file");

                    let mut options = CompileOptions::new().unwrap();
                    options.set_generate_debug_info();
                    options.set_source_language(SourceLanguage::GLSL);
                    options.set_target_env(TargetEnv::Vulkan, 0);
                    options.set_optimization_level(match args.debug {
                        true => OptimizationLevel::Performance,
                        false => OptimizationLevel::Zero,
                    });
                    for (key, value) in &args.defines {
                        options.add_macro_definition(&key, value.as_deref());
                    }

                    let mut tls_borrow = tls.borrow_mut();
                    let tls = tls_borrow.as_mut();

                    let result = tls.shader_compiler.compile_into_spirv(
                        &contents,
                        args.kind,
                        &args.file,
                        "main",
                        Some(&options),
                    );

                    drop(tls_borrow);

                    let binary = result.unwrap_or_else(|e| panic!("error compiling shader: {}", e));

                    let bytes = binary.as_binary();

                    let module = Arc::new(
                        renderer_clone
                            .device
                            .create_shader_module(ShaderModuleSource::SpirV(Cow::Borrowed(bytes))),
                    );

                    renderer_clone
                        .shader_manager
                        .cache
                        .lock()
                        .insert(args, Arc::clone(&module));

                    module
                }),
        )
    }
}

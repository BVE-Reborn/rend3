use crate::{
    renderer::{COMPUTE_POOL, SHADER_COMPILE_PRIORITY},
    ShaderError, TLS,
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
use tracing_futures::Instrument;
use wgpu::{Device, ShaderModule, ShaderModuleSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderArguments {
    pub file: String,
    pub defines: Vec<(String, Option<String>)>,
    pub kind: ShaderKind,
    pub debug: bool,
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
    pub fn new() -> Arc<Self> {
        let cache = Mutex::new(HashMap::with_hasher(FnvBuildHasher::default()));

        Arc::new(Self { cache })
    }

    pub fn compile_shader<TLD>(
        self: &Arc<Self>,
        yard: &Switchyard<RefCell<TLD>>,
        device: Arc<Device>,
        args: ShaderArguments,
    ) -> impl Future<Output = Result<Arc<ShaderModule>, ShaderError>>
    where
        TLD: AsMut<TLS> + 'static,
    {
        if let Some(module) = self.cache.lock().get(&args) {
            return Either::Left(ready(Ok(Arc::clone(module))));
        }

        let span = tracing::warn_span!("Compiling Shader", ?args);

        let this = Arc::clone(self);

        Either::Right(yard.spawn_local(COMPUTE_POOL, SHADER_COMPILE_PRIORITY, move |tls| {
            async move {
                span!(file_guard, WARN, "Loading File");

                let contents =
                    std::fs::read_to_string(&args.file).map_err(|e| ShaderError::FileError(e, args.clone()))?;

                drop(file_guard);
                span!(compile_guard, WARN, "Shader Compilationc");

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

                let binary = tls
                    .shader_compiler
                    .compile_into_spirv(&contents, args.kind, &args.file, "main", Some(&options))
                    .map_err(|e| ShaderError::CompileError(e, args.clone()))?;

                drop(tls_borrow);

                let bytes = binary.as_binary();

                drop(compile_guard);

                let module = Arc::new(device.create_shader_module(ShaderModuleSource::SpirV(Cow::Borrowed(bytes))));

                span!(cache_guard, WARN, "Add to cache");

                this.cache.lock().insert(args, Arc::clone(&module));

                drop(cache_guard);

                Ok(module)
            }
            .instrument(span)
        }))
    }
}

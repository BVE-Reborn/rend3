use crate::ShaderError;
use fnv::FnvBuildHasher;
use shaderc::{CompileOptions, Compiler, OptimizationLevel, ResolvedInclude, ShaderKind, SourceLanguage, TargetEnv};
use std::{
    borrow::Cow,
    collections::HashMap,
    future::Future,
    hash::{Hash, Hasher},
    mem::discriminant,
    path::Path,
    sync::Arc,
    thread,
    thread::JoinHandle,
};
use wgpu::{Device, ShaderModule, ShaderModuleSource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderArguments {
    pub file: String,
    pub defines: Vec<(String, Option<String>)>,
    pub kind: ShaderKind,
    pub debug: bool,
}

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for ShaderArguments {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.file.as_bytes());
        state.write_u8(self.debug as u8);
        state.write_u8(self.kind as u8);
        for (key, value) in &self.defines {
            state.write(key.as_bytes());
            discriminant(value).hash(state);
            if let Some(ref value) = value {
                state.write(value.as_bytes())
            }
        }
    }
}

pub type ShaderCompileResult = Result<Arc<ShaderModule>, ShaderError>;

pub struct ShaderManager {
    shader_thread: Option<JoinHandle<()>>,
    sender: flume::Sender<CompileCommand>,
}
impl ShaderManager {
    pub fn new(device: Arc<Device>) -> Self {
        let (sender, receiver) = flume::unbounded();

        let shader_thread = Some(
            thread::Builder::new()
                .name("rend3 shader-compilation".into())
                .spawn(move || compile_shader_loop(device, receiver))
                .unwrap(),
        );

        Self { shader_thread, sender }
    }

    pub fn compile_shader(&self, args: ShaderArguments) -> impl Future<Output = ShaderCompileResult> {
        let (sender, receiver) = flume::bounded(1);

        self.sender.send(CompileCommand::Compile(args, sender)).unwrap();

        async move { receiver.recv_async().await.unwrap() }
    }
}

impl Drop for ShaderManager {
    fn drop(&mut self) {
        self.sender.send(CompileCommand::Stop).unwrap();
        self.shader_thread.take().unwrap().join().unwrap();
    }
}

#[derive(Debug, Clone)]
enum CompileCommand {
    Compile(ShaderArguments, flume::Sender<ShaderCompileResult>),
    Stop,
}

fn compile_shader_loop(device: Arc<Device>, receiver: flume::Receiver<CompileCommand>) {
    let mut compiler = shaderc::Compiler::new().unwrap();
    let mut cache = HashMap::with_hasher(FnvBuildHasher::default());

    while let Ok(command) = receiver.recv() {
        match command {
            CompileCommand::Compile(args, sender) => {
                let result = if let Some(module) = cache.get(&args) {
                    Ok(Arc::clone(module))
                } else {
                    let result = compile_shader(&mut compiler, &device, &args);
                    if let Ok(ref module) = result {
                        cache.insert(args, Arc::clone(module));
                    }
                    result
                };

                sender.send(result).unwrap();
            }
            CompileCommand::Stop => return,
        }
    }
}

fn compile_shader(compiler: &mut Compiler, device: &Device, args: &ShaderArguments) -> ShaderCompileResult {
    span_transfer!(_ -> file_span, WARN, "Loading File");

    tracing::debug!("Compiling shader {:?}", args);

    let contents = std::fs::read_to_string(&args.file).map_err(|e| ShaderError::FileError(e, args.clone()))?;

    span_transfer!(file_span -> compile_span, WARN, "Shader Compilation");

    let mut options = CompileOptions::new().unwrap();
    options.set_generate_debug_info();
    options.set_source_language(SourceLanguage::GLSL);
    options.set_target_env(TargetEnv::Vulkan, 0);
    options.set_optimization_level(match args.debug {
        true => OptimizationLevel::Zero,
        false => OptimizationLevel::Performance,
    });
    for (key, value) in &args.defines {
        options.add_macro_definition(&key, value.as_deref());
    }
    options.set_include_callback(|include, _ty, src, _depth| {
        let joined = Path::new(src)
            .parent()
            .ok_or_else(|| {
                format!(
                    "Cannot find include <{}> relative to file {} as there is no parent directory",
                    include, src
                )
            })?
            .join(Path::new(include));
        let contents = std::fs::read_to_string(&joined)
            .map_err(|e| format!("Error while loading include <{}> from file {}: {}", include, src, e))?;
        Ok(ResolvedInclude {
            resolved_name: joined.to_string_lossy().to_string(),
            content: contents,
        })
    });

    let binary = compiler
        .compile_into_spirv(&contents, args.kind, &args.file, "main", Some(&options))
        .map_err(|e| ShaderError::CompileError(e, args.clone()))?;

    let bytes = binary.as_binary();

    span_transfer!(compile_span -> module_create_span, WARN, "Create Shader Module");

    let module = Arc::new(device.create_shader_module(ShaderModuleSource::SpirV(Cow::Borrowed(bytes))));

    Ok(module)
}

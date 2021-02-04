use crate::{
    datatypes::ShaderHandle,
    list::{ShaderSourceType, SourceShaderDescriptor},
    registry::ResourceRegistry,
    ShaderError,
};
use parking_lot::RwLock;
use shaderc::{CompileOptions, Compiler, OptimizationLevel, ResolvedInclude, SourceLanguage, TargetEnv};
use std::{borrow::Cow, future::Future, path::Path, sync::Arc, thread, thread::JoinHandle};
use wgpu::{Device, ShaderFlags, ShaderModule, ShaderModuleDescriptor, ShaderSource};

pub type ShaderCompileResult = Result<Arc<ShaderModule>, ShaderError>;

const BUILTIN_SHADERS: include_dir::Dir = include_dir::include_dir!("./shaders");

pub struct ShaderManager {
    shader_thread: Option<JoinHandle<()>>,
    sender: flume::Sender<CompileCommand>,
    registry: RwLock<ResourceRegistry<Arc<ShaderModule>>>,
}
impl ShaderManager {
    pub fn new(device: Arc<Device>) -> Arc<Self> {
        let (sender, receiver) = flume::unbounded();

        let shader_thread = Some(
            thread::Builder::new()
                .name("rend3 shader-compilation".into())
                .spawn(move || compile_shader_loop(device, receiver))
                .unwrap(),
        );

        let registry = RwLock::new(ResourceRegistry::new());

        Arc::new(Self {
            shader_thread,
            sender,
            registry,
        })
    }

    pub fn allocate(&self) -> ShaderHandle {
        ShaderHandle(self.registry.read().allocate())
    }

    pub fn allocate_async_insert(self: &Arc<Self>, args: SourceShaderDescriptor) -> impl Future<Output = ShaderHandle> {
        let handle = ShaderHandle(self.registry.read().allocate());
        let fut = self.compile_shader(args);
        let this = Arc::clone(self);
        async move {
            let res = fut.await.unwrap();
            this.registry.write().insert(handle.0, res);
            handle
        }
    }

    pub fn insert(&self, handle: ShaderHandle, shader: Arc<ShaderModule>) {
        self.registry.write().insert(handle.0, shader);
    }

    pub fn remove(&self, handle: ShaderHandle) {
        self.registry.write().remove(handle.0);
    }

    pub fn get(&self, handle: ShaderHandle) -> Arc<ShaderModule> {
        self.registry.read().get(handle.0).clone()
    }

    pub fn compile_shader(&self, args: SourceShaderDescriptor) -> impl Future<Output = ShaderCompileResult> {
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
    Compile(SourceShaderDescriptor, flume::Sender<ShaderCompileResult>),
    Stop,
}

fn compile_shader_loop(device: Arc<Device>, receiver: flume::Receiver<CompileCommand>) {
    let mut compiler = shaderc::Compiler::new().unwrap();

    while let Ok(command) = receiver.recv() {
        match command {
            CompileCommand::Compile(args, sender) => {
                let result = compile_shader(&mut compiler, &device, &args);

                sender.send(result).unwrap();
            }
            CompileCommand::Stop => return,
        }
    }
}

fn compile_shader(compiler: &mut Compiler, device: &Device, args: &SourceShaderDescriptor) -> ShaderCompileResult {
    span_transfer!(_ -> file_span, WARN, "Loading File");

    tracing::debug!("Compiling shader {:?}", args);

    let contents = match args.source {
        ShaderSourceType::File(ref file) => {
            std::fs::read_to_string(file).map_err(|e| ShaderError::FileError(e, args.clone()))?
        }
        ShaderSourceType::Builtin(ref file) => BUILTIN_SHADERS
            .get_file(file)
            .ok_or_else(|| ShaderError::Builtin(args.clone()))?
            .contents_utf8()
            .unwrap()
            .to_string(),
        ShaderSourceType::Value(ref code) => code.clone(),
    };

    let file_name = match args.source {
        ShaderSourceType::File(ref file) | ShaderSourceType::Builtin(ref file) => &**file,
        ShaderSourceType::Value(_) => "./file",
    };

    let builtin = matches!(args.source, ShaderSourceType::Builtin(_));

    span_transfer!(file_span -> compile_span, WARN, "Shader Compilation");

    let mut options = CompileOptions::new().unwrap();
    options.set_generate_debug_info();
    options.set_source_language(SourceLanguage::GLSL);
    options.set_target_env(TargetEnv::Vulkan, 0);
    options.set_optimization_level(match cfg!(debug_assertions) {
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
        let contents = if builtin {
            let dedot = path_dedot::ParseDot::parse_dot(&joined).unwrap();
            BUILTIN_SHADERS
                .get_file(dedot)
                .ok_or_else(|| {
                    format!(
                        "Error while locating builtin include <{}> from file {} for path {}",
                        include,
                        src,
                        joined.display()
                    )
                })?
                .contents_utf8()
                .unwrap()
                .to_string()
        } else {
            std::fs::read_to_string(&joined).map_err(|e| {
                format!(
                    "Error while loading include <{}> from file {} for path {}: {}",
                    include,
                    src,
                    joined.display(),
                    e
                )
            })?
        };
        Ok(ResolvedInclude {
            resolved_name: joined.to_string_lossy().to_string(),
            content: contents,
        })
    });

    let binary = compiler
        .compile_into_spirv(&contents, args.stage.into(), &file_name, "main", Some(&options))
        .map_err(|e| ShaderError::CompileError(e, args.clone()))?;

    let bytes = binary.as_binary();

    span_transfer!(compile_span -> module_create_span, WARN, "Create Shader Module");

    let module = Arc::new(device.create_shader_module(&ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::SpirV(Cow::Borrowed(bytes)),
        flags: ShaderFlags::VALIDATION,
    }));

    Ok(module)
}

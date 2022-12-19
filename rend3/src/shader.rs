//! Holds the shader processing infrastructure for all shaders.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use handlebars::{Context, Handlebars, Helper, HelperDef, Output, RenderContext, RenderError};
use parking_lot::Mutex;
use rend3_types::{Material, MaterialArray, VertexAttributeId};
use rust_embed::RustEmbed;
use serde::Serialize;

use crate::RendererProfile;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/shaders"]
struct Rend3ShaderSources;

#[derive(Debug, Default, Serialize)]
pub struct ShaderConfig {
    pub profile: Option<RendererProfile>,
}

pub struct ShaderVertexBufferConfig {
    specs: Vec<VertexBufferSpec>,
}
impl ShaderVertexBufferConfig {
    pub fn from_material<M: Material>() -> Self {
        let supported = M::supported_attributes();
        let required = M::required_attributes();

        let mut specs = Vec::with_capacity(supported.as_ref().len());
        for attribute in supported.into_iter() {
            specs.push(VertexBufferSpec {
                attribute: *attribute,
                optional: !required.as_ref().contains(&attribute),
            });
        }

        Self { specs }
    }
}

struct VertexBufferSpec {
    attribute: VertexAttributeId,
    optional: bool,
}

pub struct ShaderPreProcessor {
    files: HashMap<String, String>,
}

impl ShaderPreProcessor {
    pub fn new() -> Self {
        let mut v = Self { files: HashMap::new() };
        v.add_shaders_embed::<Rend3ShaderSources>("rend3");
        v
    }

    pub fn add_shaders_embed<T: RustEmbed>(&mut self, prefix: &str) {
        for file in T::iter() {
            let contents = String::from_utf8(T::get(&file).unwrap().data.into_owned()).unwrap();
            self.files.insert(format!("{prefix}/{file}"), contents);
        }
    }

    pub fn add_shader(&mut self, name: &str, contents: &str) {
        self.files.insert(name.to_owned(), contents.to_owned());
    }

    pub fn files(&self) -> std::collections::hash_map::Keys<'_, String, String> {
        self.files.keys()
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.files.get(name)
    }

    pub fn render_shader<T>(
        &self,
        base: &str,
        user_config: &T,
        buffer_config: Option<&ShaderVertexBufferConfig>,
    ) -> Result<String, RenderError>
    where
        T: Serialize,
    {
        #[derive(Serialize)]
        struct BufferConfigWrapper<'a, T> {
            vertex_array_counts: usize,
            #[serde(flatten)]
            user_config: &'a T,
        }

        let mut include_status = Mutex::new(HashSet::new());
        include_status.get_mut().insert(base.to_string());

        let mut registry = Handlebars::new();
        registry.set_strict_mode(true);
        registry.set_dev_mode(cfg!(debug_assertions));
        registry.register_escape_fn(handlebars::no_escape);
        registry.register_helper("include", Box::new(ShaderIncluder::new(base, &self.files)));
        if let Some(config) = buffer_config {
            registry.register_helper("vertex_fetch", Box::new(ShaderVertexBufferHelper::new(config)));
        }
        let contents = self.files.get(base).ok_or_else(|| {
            RenderError::new(format!(
                "Base shader {base} is not registered. All registered shaders: {}",
                registered_shader_string(&self.files)
            ))
        })?;

        let vertex_array_counts = if let Some(buffer_config) = buffer_config {
            buffer_config.specs.len()
        } else {
            0
        };

        registry.render_template(
            contents,
            &BufferConfigWrapper {
                vertex_array_counts,
                user_config,
            },
        )
    }
}

impl Default for ShaderPreProcessor {
    fn default() -> Self {
        Self::new()
    }
}

fn registered_shader_string(files: &HashMap<String, String>) -> String {
    let mut v: Vec<_> = files.keys().cloned().collect();
    v.sort_unstable();
    v.join(", ")
}

struct ShaderIncluder<'a> {
    files: &'a HashMap<String, String>,
    include_state: Mutex<HashSet<String>>,
}
impl<'a> ShaderIncluder<'a> {
    fn new(base: &str, files: &'a HashMap<String, String>) -> Self {
        Self {
            files,
            include_state: Mutex::new({
                let mut set = HashSet::new();
                set.insert(base.to_owned());
                set
            }),
        }
    }
}
impl<'a> HelperDef for ShaderIncluder<'a> {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        _rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> handlebars::HelperResult {
        let file_name_value = h
            .param(0)
            .ok_or_else(|| RenderError::new("include helper must have a single argument for the include path"))?
            .value();
        let file_name = match file_name_value {
            handlebars::JsonValue::String(s) => s,
            _ => return Err(RenderError::new("include helper's first argument must be a string")),
        };

        let mut include_status = self.include_state.try_lock().unwrap();
        if include_status.contains(file_name) {
            return Ok(());
        }
        include_status.insert(file_name.clone());
        drop(include_status);

        let contents = self.files.get(file_name).ok_or_else(|| {
            RenderError::new(format!(
                "Included file \"{file_name}\" is not registered. All registered files: {}",
                registered_shader_string(self.files)
            ))
        })?;

        out.write(&r.render_template(contents, ctx.data())?)?;

        Ok(())
    }
}

struct ShaderVertexBufferHelper<'a> {
    config: &'a ShaderVertexBufferConfig,
}

impl<'a> ShaderVertexBufferHelper<'a> {
    fn new(config: &'a ShaderVertexBufferConfig) -> Self {
        Self { config }
    }
}

impl<'a> HelperDef for ShaderVertexBufferHelper<'a> {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'reg, 'rc>,
        r: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        _rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> handlebars::HelperResult {
        let object_buffer_value = h
            .param(0)
            .ok_or_else(|| {
                RenderError::new("Vertex buffer helper must have an argument pointing to the buffer of objects")
            })?
            .relative_path();
        let object_buffer = match object_buffer_value {
            Some(s) => s,
            _ => {
                return Err(RenderError::new(
                    "Vertex buffer helper's first argument must be a string",
                ))
            }
        };
        let batch_buffer_value = h
            .param(1)
            .ok_or_else(|| {
                RenderError::new("Vertex buffer helper must have an argument pointing to the buffer of batch data")
            })?
            .relative_path();
        let batch_buffer = match batch_buffer_value {
            Some(s) => s,
            _ => {
                return Err(RenderError::new(
                    "Vertex buffer helper's second argument must be a string",
                ))
            }
        };

        let template = self
            .generate_template(h, object_buffer, batch_buffer)
            .map_err(|_| RenderError::new(format!("Failed to writeln vertex template string")))?;

        out.write(&r.render_template(&template, ctx.data())?)?;

        Ok(())
    }
}

impl<'a> ShaderVertexBufferHelper<'a> {
    fn generate_template(
        &self,
        h: &Helper,
        object_buffer: &str,
        batch_buffer: &str,
    ) -> Result<String, std::fmt::Error> {
        let includes = r#"{{include "rend3/vertex_attributes.wgsl"}}"#;

        let unpack_function = format!(
            "
            fn unpack_vertex_index(vertex_index: u32) -> Indices {{
                 let local_object_id = vertex_index >> 24u;
                 let vertex_id = vertex_index & 0xFFFFFFu;
                 let object_id = {batch_buffer}.ranges[local_object_id].object_id;
                 
                 return Indices(vertex_id, object_id);
            }}"
        );

        let mut input_struct = String::new();
        writeln!(input_struct, "struct VertexInput {{")?;
        let mut input_function = String::new();
        writeln!(input_function, "fn get_vertices(indices: Indices) -> VertexInput {{")?;
        writeln!(input_function, "    var verts: VertexInput;")?;
        for requested_attribute in &h.params()[2..] {
            let (attr_idx, spec) = self
                .config
                .specs
                .iter()
                .enumerate()
                .find_map(
                    |(idx, s)| match s.attribute.name() == requested_attribute.relative_path().unwrap() {
                        true => Some((idx, s)),
                        false => None,
                    },
                )
                .unwrap();

            writeln!(
                input_struct,
                "    {}: {},",
                spec.attribute.name(),
                spec.attribute.metadata().shader_type
            )?;

            writeln!(
                input_function,
                "    let {}_offset = {object_buffer}[indices.object].vertex_attribute_start_offsets[{attr_idx}];",
                spec.attribute.name(),
            )?;

            if spec.optional {
                writeln!(
                    input_function,
                    "    if ({}_offset != 0xFFFFFFFFu) {{",
                    spec.attribute.name()
                )?;

                writeln!(
                    input_function,
                    "        verts.{name} = {}({name}_offset, indices.vertex);",
                    spec.attribute.metadata().shader_extract_fn,
                    name = spec.attribute.name(),
                )?;

                writeln!(input_function, "    }}")?;

                if let Some(default_value) = spec.attribute.default_value() {
                    writeln!(
                        input_function,
                        "else {{ verts.{name} = {default_value}; }}",
                        name = spec.attribute.name(),
                    )?;
                }
            } else {
                writeln!(
                    input_function,
                    "    verts.{name} = {}({name}_offset, indices.vertex);",
                    spec.attribute.metadata().shader_extract_fn,
                    name = spec.attribute.name(),
                )?;
            }
        }
        writeln!(input_struct, "}}")?;
        writeln!(input_function, "    return verts;")?;
        writeln!(input_function, "}}")?;

        let template = format!("{includes}{unpack_function}{input_struct}{input_function}");

        Ok(template)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ShaderConfig, ShaderPreProcessor};

    #[test]
    fn simple_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        pp.add_shader("other", "other");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config, None).unwrap();

        assert_eq!(output, "other simple");
    }

    #[test]
    fn recursive_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        pp.add_shader("other", "{{include \"simple\"}} other");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config, None).unwrap();

        assert_eq!(output, " other simple");
    }

    #[test]
    fn error_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config, None);

        assert!(output.is_err(), "Expected error, got {output:?}");
    }

    #[test]
    fn no_arg_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include}} simple");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config, None);

        assert!(output.is_err(), "Expected error, got {output:?}");
    }
}

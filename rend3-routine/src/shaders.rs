//! Holds the shader processing infrastructure for all shaders.

use std::collections::{HashMap, HashSet};

use handlebars::{Context, Handlebars, Helper, HelperDef, Output, RenderContext, RenderError};
use parking_lot::Mutex;
use rend3::RendererProfile;
use rust_embed::RustEmbed;
use serde::Serialize;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/shaders/src"]
pub struct RawShaderSources;

#[derive(Debug, Serialize)]
pub struct ShaderConfig {
    pub profile: Option<RendererProfile>,
}

pub struct ShaderPreProcessor {
    files: HashMap<String, String>,
}

impl ShaderPreProcessor {
    pub fn new() -> Self {
        Self { files: HashMap::new() }
    }

    pub fn add_inherent_shaders(&mut self) {
        for file in RawShaderSources::iter() {
            let contents = String::from_utf8(RawShaderSources::get(&file).unwrap().data.into_owned()).unwrap();
            self.files.insert(file.into_owned(), contents);
        }
    }

    pub fn add_shader(&mut self, name: &str, contents: &str) {
        self.files.insert(name.to_owned(), contents.to_owned());
    }

    pub fn render_shader(&self, base: &str, config: &ShaderConfig) -> Result<String, RenderError> {
        let mut include_status = Mutex::new(HashSet::new());
        include_status.get_mut().insert(base.to_string());

        let mut registry = Handlebars::new();
        registry.set_strict_mode(true);
        registry.set_dev_mode(cfg!(debug_assertions));
        registry.register_escape_fn(handlebars::no_escape);
        registry.register_helper("include", Box::new(ShaderIncluder::new(base, &self.files)));
        let contents = self
            .files
            .get(base)
            .ok_or_else(|| RenderError::new(format!("base shader {base} is not registered")))?;

        registry.render_template(&contents, config)
    }
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
        _ctx: &'rc Context,
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

        let contents = self
            .files
            .get(file_name)
            .ok_or_else(|| RenderError::new(format!("included file \"{file_name}\" is not registered")))?;

        out.write(&r.render_template(contents, &())?)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use codespan_reporting::{
        diagnostic::{Diagnostic, Label},
        files::SimpleFile,
        term::{
            self,
            termcolor::{ColorChoice, StandardStream},
        },
    };
    use naga::WithSpan;
    use rend3::RendererProfile;

    use crate::shaders::{RawShaderSources, ShaderConfig, ShaderPreProcessor};

    fn print_err(error: &dyn Error) {
        eprint!("{}", error);

        let mut e = error.source();
        if e.is_some() {
            eprintln!(": ");
        } else {
            eprintln!();
        }

        while let Some(source) = e {
            eprintln!("\t{}", source);
            e = source.source();
        }
    }

    pub fn emit_annotated_error<E: Error>(ann_err: &WithSpan<E>, filename: &str, source: &str) {
        let files = SimpleFile::new(filename, source);
        let config = codespan_reporting::term::Config::default();
        let writer = StandardStream::stderr(ColorChoice::Auto);

        let diagnostic = Diagnostic::error().with_labels(
            ann_err
                .spans()
                .map(|(span, desc)| Label::primary((), span.to_range().unwrap()).with_message(desc.to_owned()))
                .collect(),
        );

        term::emit(&mut writer.lock(), &config, &files, &diagnostic).expect("cannot write error");
    }

    #[test]
    fn simple_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        pp.add_shader("other", "other");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config).unwrap();

        assert_eq!(output, "other simple");
    }

    #[test]
    fn recursive_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        pp.add_shader("other", "{{include \"simple\"}} other");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config).unwrap();

        assert_eq!(output, " other simple");
    }

    #[test]
    fn error_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include \"other\"}} simple");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config);

        assert!(output.is_err(), "Expected error, got {output:?}");
    }

    #[test]
    fn no_arg_include() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_shader("simple", "{{include}} simple");
        let config = ShaderConfig { profile: None };
        let output = pp.render_shader("simple", &config);

        assert!(output.is_err(), "Expected error, got {output:?}");
    }

    #[test]
    fn validate_inherent_shaders() {
        let mut pp = ShaderPreProcessor::new();
        pp.add_inherent_shaders();

        for shader in RawShaderSources::iter() {
            if !shader.contains(".wgsl") {
                continue;
            }

            let configs = if pp.files[&*shader].contains("#if") {
                &[
                    ShaderConfig {
                        profile: Some(RendererProfile::CpuDriven),
                    },
                    ShaderConfig {
                        profile: Some(RendererProfile::GpuDriven),
                    },
                ][..]
            } else {
                &[ShaderConfig { profile: None }][..]
            };

            for config in configs {
                let serialized_config = serde_json::to_value(config).unwrap();
                println!("Testing shader {shader} with config {serialized_config:?}");

                let output = pp.render_shader(&shader, config);

                assert!(
                    output.is_ok(),
                    "Expected preprocessing success, got {output:?}"
                );
                let output = output.unwrap_or_else(|e| panic!("Expected preprocessing success, got {e:?}"));

                let sm = match naga::front::wgsl::parse_str(&output) {
                    Ok(m) => m,
                    Err(e) => {
                        e.emit_to_stderr_with_path(&output, &shader);
                        panic!();
                    }
                };

                let mut validator =
                    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());

                match validator.validate(&sm) {
                    Ok(_) => {}
                    Err(err) => {
                        emit_annotated_error(&err, &shader, &output);
                        print_err(&err);
                        panic!()
                    }
                };
            }
        }
    }
}

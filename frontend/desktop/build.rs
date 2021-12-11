use std::error::Error;

#[cfg(feature = "compile-shaders")]
mod shaders {
    use shaderc::{
        CompileOptions, Compiler, IncludeType, OptimizationLevel, ResolvedInclude, ShaderKind,
    };
    use std::{
        error::Error,
        fs, io,
        path::{Path, PathBuf},
    };

    const SRC: &str = "shaders/src";
    const OUT: &str = "shaders/out";

    fn compile_shader(
        compiler: &mut Compiler,
        compile_options: &CompileOptions,
        source: &str,
        shader_kind: ShaderKind,
        path_in_shaders: &Path,
        output_path_in_shaders: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let path_in_shaders_str = path_in_shaders.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid non-UTF-8 shader path")
        })?;
        let output = match compiler.compile_into_spirv(
            source,
            shader_kind,
            path_in_shaders_str,
            "main",
            Some(compile_options),
        ) {
            Ok(output) => output,
            Err(shaderc::Error::CompilationError(error_count, description)) => {
                eprintln!(
                    "Shader compilation failed, {} error{} emitted:\n{}",
                    error_count,
                    if error_count != 1 { "s" } else { "" },
                    description,
                );
                return Err("Couldn't compile shaders".into());
            }
            Err(err) => return Err(err.into()),
        };
        let complete_out_path = {
            let mut path = Path::new(OUT).join(output_path_in_shaders).into_os_string();
            path.push(".spv");
            PathBuf::from(path)
        };
        fs::write(complete_out_path, output.as_binary_u8())?;
        Ok(())
    }

    fn traverse_dir(
        compiler: &mut Compiler,
        compile_options: &CompileOptions,
        complete_path: &Path,
        path_in_shaders: &Path,
    ) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(complete_path)? {
            let entry = entry?;

            let complete_path = entry.path();
            let path_in_shaders = path_in_shaders.join(&entry.file_name());
            let complete_path_str = complete_path.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid non-UTF-8 shader path")
            })?;
            println!("cargo:rerun-if-changed={}", complete_path_str);

            let metadata = fs::metadata(&complete_path)?;
            if metadata.is_dir() {
                traverse_dir(compiler, compile_options, &complete_path, &path_in_shaders)?;
            } else if metadata.is_file() {
                let source = fs::read_to_string(&complete_path)?;

                let shader_kind = match match complete_path.extension() {
                    Some(ext) => {
                        if let Some(ext) = ext.to_str() {
                            ext
                        } else {
                            continue;
                        }
                    }
                    None => continue,
                } {
                    "vert" => ShaderKind::Vertex,
                    "frag" => ShaderKind::Fragment,
                    "comp" => ShaderKind::Compute,
                    "glsl" => ShaderKind::InferFromSource,
                    _ => continue,
                };

                let srgb_aware_new_file_names = if shader_kind == ShaderKind::Fragment {
                    complete_path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .and_then(|stem_str| stem_str.strip_suffix("-srgb-aware"))
                        .map(|file_stem| {
                            (
                                format!("{}-linear.frag", file_stem),
                                format!("{}-srgb.frag", file_stem),
                            )
                        })
                } else {
                    None
                };

                let output_path_in_shaders =
                    if let Some((new_linear_file_name, _)) = srgb_aware_new_file_names.as_ref() {
                        path_in_shaders.with_file_name(new_linear_file_name)
                    } else {
                        path_in_shaders.clone()
                    };

                compile_shader(
                    compiler,
                    compile_options,
                    &source,
                    shader_kind,
                    &path_in_shaders,
                    &output_path_in_shaders,
                )?;

                if let Some((_, new_srgb_file_name)) = srgb_aware_new_file_names.as_ref() {
                    let srgb_output_path_in_shaders =
                        path_in_shaders.with_file_name(new_srgb_file_name);
                    let mut compile_options = compile_options
                        .clone()
                        .expect("couldn't clone compile options");
                    compile_options.add_macro_definition("SRGB", None);
                    compile_shader(
                        compiler,
                        &compile_options,
                        &source,
                        shader_kind,
                        &path_in_shaders,
                        &srgb_output_path_in_shaders,
                    )?;
                }
            } else {
                unimplemented!("Unknown file type");
            }
        }

        Ok(())
    }

    pub fn compile() -> Result<(), Box<dyn Error>> {
        println!("cargo:rerun-if-changed={}", SRC);

        fs::create_dir_all(OUT)?;

        let mut compiler = Compiler::new().expect("Couldn't create shader compiler");
        let mut compile_options =
            CompileOptions::new().expect("Couldn't create shader compiler options");
        compile_options.set_optimization_level(OptimizationLevel::Performance);
        compile_options.set_include_callback(|path, include_type, src_path, _| {
            let src_path = Path::new(src_path);
            let path = match include_type {
                IncludeType::Relative => Path::new(src_path.parent().unwrap()).join(path),
                IncludeType::Standard => Path::new(SRC).join(path),
            };
            match fs::read_to_string(&path) {
                Ok(content) => Ok(ResolvedInclude {
                    resolved_name: path.into_os_string().into_string().unwrap(),
                    content,
                }),
                Err(error) => Err(format!("{:?}", error)),
            }
        });

        traverse_dir(
            &mut compiler,
            &compile_options,
            Path::new(SRC),
            Path::new(""),
        )
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "compile-shaders")]
    shaders::compile()?;
    Ok(())
}

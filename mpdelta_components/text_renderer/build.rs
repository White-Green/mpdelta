use cargo_gpu_install::install::Install;
use cargo_gpu_install::spirv_builder::SpirvMetadata;
use std::env;
use std::error::Error;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let shader_crate = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("shader");
    let backend = Install::from_shader_crate(shader_crate.clone()).run()?;
    let mut builder = backend.to_spirv_builder(shader_crate, "spirv-unknown-vulkan1.3");
    builder.build_script.defaults = true;
    builder.spirv_metadata = SpirvMetadata::Full;
    let spv_result = builder.build()?;
    let path_to_spv = spv_result.module.unwrap_single();

    println!("cargo::rustc-env=SHADER_PATH={}", path_to_spv.display());

    Ok(())
}

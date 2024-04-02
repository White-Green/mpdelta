use shader_builder::{ShaderBuildError, ShaderBuilder};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), ShaderBuildError> {
    ShaderBuilder::new(PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).parent().unwrap(), PathBuf::from(env::var("OUT_DIR").unwrap()))
        .add_crate("mpdelta_components/shaders/font_rendering", [])
        .build()
}

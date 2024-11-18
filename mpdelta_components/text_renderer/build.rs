use shader_builder::{ShaderBuildError, ShaderBuilder};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), ShaderBuildError> {
    ShaderBuilder::new(env::var("CARGO_MANIFEST_DIR").unwrap(), PathBuf::from(env::var("OUT_DIR").unwrap())).add_crate("shader", []).build()
}

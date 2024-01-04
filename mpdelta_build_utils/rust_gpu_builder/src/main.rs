use clap::Parser;
use std::error::Error;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long)]
    base: PathBuf,
    #[clap(long)]
    out_dir: PathBuf,
    #[clap(long = "crate")]
    crate_path: PathBuf,
    #[clap(long = "capability")]
    capabilities: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    inner::main()
}

#[cfg(feature = "spirv-builder")]
mod inner {
    use crate::Args;
    use clap::Parser;
    use spirv_builder::{Capability, MetadataPrintout, SpirvBuilder};
    use std::error::Error;
    use std::fs;
    use std::path::Path;
    use std::str::FromStr;

    pub fn main() -> Result<(), Box<dyn Error>> {
        println!("cargo:rerun-if-changed={}", env!("CARGO_MANIFEST_DIR"));
        let Args { base, out_dir, crate_path, capabilities } = Args::parse();
        println!("cargo:rerun-if-changed={}", base.join(&crate_path).display());
        let spirv_builder = SpirvBuilder::new(base.join(&crate_path), "spirv-unknown-spv1.3");
        let spirv_builder = capabilities.into_iter().fold(Ok(spirv_builder), |builder, capability| Ok(builder?.capability(Capability::from_str(&capability)?))).map_err(|()| "Capability parse error")?;
        let result = spirv_builder.print_metadata(MetadataPrintout::Full).build()?;
        let spv_file = result.module.unwrap_single();
        fs::copy(spv_file, out_dir.join(Path::new(&crate_path).iter().next_back().unwrap()).with_extension("spv"))?;
        Ok(())
    }
}

#[cfg(not(feature = "spirv-builder"))]
mod inner {
    use std::error::Error;

    pub fn main() -> Result<(), Box<dyn Error>> {
        panic!("spirv-builder feature is not enabled, rust_gpu_builder is not available");
    }
}

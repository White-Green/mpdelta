use clap::Parser;
use spirv_builder::{MetadataPrintout, SpirvBuilder};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    base: PathBuf,
    #[clap(short, long)]
    out_dir: PathBuf,
    #[clap(short, long = "crate")]
    crates: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", env!("CARGO_MANIFEST_DIR"));
    let Args { base, out_dir, crates } = Args::parse();
    for crate_path in crates {
        println!("cargo:rerun-if-changed={}", base.join(&crate_path).display());
        let result = SpirvBuilder::new(base.join(&crate_path), "spirv-unknown-spv1.5").print_metadata(MetadataPrintout::Full).build()?;
        let spv_file = result.module.unwrap_single();
        fs::copy(spv_file, out_dir.join(Path::new(&crate_path).iter().next_back().unwrap()).with_extension("spv"))?;
    }
    Ok(())
}

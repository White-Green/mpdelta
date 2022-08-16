use std::env;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

#[derive(Debug)]
pub struct ShaderBuilder {
    base: PathBuf,
    out_dir: PathBuf,
    crates: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShaderBuildError {
    #[error("cargo command is not succeed; exit status: {0}")]
    CargoNotSucceed(ExitStatus),
    #[error("IO error: {0}")]
    IOError(#[from] io::Error),
}

impl ShaderBuilder {
    pub fn new(base: impl AsRef<Path>, out_dir: impl AsRef<Path>) -> ShaderBuilder {
        ShaderBuilder {
            base: base.as_ref().to_path_buf(),
            out_dir: out_dir.as_ref().to_path_buf(),
            crates: Vec::new(),
        }
    }

    pub fn add_crate(self, crate_path: impl AsRef<Path>) -> Self {
        let ShaderBuilder { base, out_dir, mut crates } = self;
        crates.push(crate_path.as_ref().to_path_buf());
        ShaderBuilder { base, out_dir, crates }
    }

    pub fn build(self) -> Result<(), ShaderBuildError> {
        let ShaderBuilder { base, out_dir, crates } = self;
        let mut cargo = Command::new(dbg!(concat!(env!("CARGO_HOME"), "/bin/cargo")));
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
        cargo
            .current_dir(dbg!(workspace_dir.join("mpdelta_build_utils").join("rust_gpu_builder")))
            .envs(env::vars())
            .env_remove("RUSTUP_TOOLCHAIN")
            .args(["run", "--release"])
            .arg("--target-dir")
            .arg(workspace_dir.join("target_for_shaders"))
            .arg("--")
            .arg("--base")
            .arg(base)
            .arg("--out-dir")
            .arg(out_dir);
        crates.into_iter().for_each(|crate_path| {
            cargo.arg("--crate").arg(crate_path);
        });
        cargo.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        match cargo.status()? {
            status if !status.success() => Err(ShaderBuildError::CargoNotSucceed(status)),
            _ => Ok(()),
        }
    }
}

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::{env, io};

#[derive(Debug)]
pub struct ShaderBuilder {
    base: PathBuf,
    out_dir: PathBuf,
    crates: Vec<(PathBuf, Vec<String>)>,
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

    pub fn add_crate<'a, I: IntoIterator<Item = &'a str>>(self, crate_path: impl AsRef<Path>, capabilities: I) -> Self {
        let ShaderBuilder { base, out_dir, mut crates } = self;
        crates.push((crate_path.as_ref().to_path_buf(), capabilities.into_iter().map(|cap| cap.to_string()).collect()));
        ShaderBuilder { base, out_dir, crates }
    }

    pub fn build(self) -> Result<(), ShaderBuildError> {
        let ShaderBuilder { base, out_dir, crates } = self;
        for (crate_path, capabilities) in crates {
            let mut cargo = Command::new(dbg!(concat!(env!("CARGO_HOME"), "/bin/cargo")));
            let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
            cargo
                .current_dir(dbg!(workspace_dir.join("mpdelta_build_utils").join("rust_gpu_builder")))
                .envs(env::vars())
                .env_remove("RUSTUP_TOOLCHAIN")
                .env_remove("RUSTC_WORKSPACE_WRAPPER")
                .args(["run", "--release"])
                .arg("--target-dir")
                .arg(workspace_dir.join("target_for_shaders"))
                .args(["--features", "spirv-builder"])
                .arg("--")
                .arg("--base")
                .arg(&base)
                .arg("--out-dir")
                .arg(&out_dir)
                .arg("--crate")
                .arg(crate_path);
            capabilities.into_iter().for_each(|capability| {
                cargo.arg("--capability").arg(capability);
            });
            cargo.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            match cargo.status()? {
                status if !status.success() => return Err(ShaderBuildError::CargoNotSucceed(status)),
                _ => {}
            }
        }
        Ok(())
    }
}

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub fn run_up(root: &Path) -> Result<()> {
    if which::which("devcontainer").is_ok() {
        println!("Starting environment with Dev Container CLI...");
        let status = Command::new("devcontainer")
            .arg("up")
            .arg("--workspace-folder")
            .arg(root)
            .status()
            .context("failed to invoke devcontainer CLI")?;

        if status.success() {
            println!("Environment started successfully.");
            return Ok(());
        }

        bail!("devcontainer up failed with exit code {:?}", status.code());
    }

    if which::which("docker").is_ok() {
        let dockerfile_path = root.join(".devcontainer").join("Dockerfile");
        if !dockerfile_path.is_file() {
            bail!(
                "{} not found. Run `devsync init` first.",
                dockerfile_path.display()
            );
        }

        let image_tag = format!(
            "devsync-{}:local",
            root.file_name()
                .map(|name| name.to_string_lossy().to_lowercase().replace(' ', "-"))
                .unwrap_or_else(|| "workspace".to_string())
        );

        println!("Dev Container CLI not found; building Docker image fallback...");
        let status = Command::new("docker")
            .arg("build")
            .arg("-f")
            .arg(&dockerfile_path)
            .arg("-t")
            .arg(&image_tag)
            .arg(root)
            .status()
            .context("failed to invoke docker build")?;

        if !status.success() {
            bail!("docker build failed with exit code {:?}", status.code());
        }

        println!("Built image `{}`.", image_tag);
        println!(
            "Run this to open a shell:\n  docker run --rm -it -v {}:/workspace -w /workspace {} bash",
            root.display(),
            image_tag
        );

        return Ok(());
    }

    bail!("No supported runtime found. Install `devcontainer` CLI or Docker and retry.");
}

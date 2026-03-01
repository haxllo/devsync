use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub fn run_up(root: &Path) -> Result<()> {
    maybe_print_buildx_hint();

    if which::which("devcontainer").is_ok() {
        println!("Starting environment with Dev Container CLI...");
        let mut cmd = Command::new("devcontainer");
        cmd.arg("up").arg("--workspace-folder").arg(root);
        apply_buildkit_env(&mut cmd);
        let status = cmd.status().context("failed to invoke devcontainer CLI")?;

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
        let mut cmd = Command::new("docker");
        cmd.arg("build")
            .arg("-f")
            .arg(&dockerfile_path)
            .arg("-t")
            .arg(&image_tag)
            .arg(root);
        apply_buildkit_env(&mut cmd);
        let status = cmd.status().context("failed to invoke docker build")?;

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

fn apply_buildkit_env(cmd: &mut Command) {
    if std::env::var_os("DOCKER_BUILDKIT").is_none() {
        cmd.env("DOCKER_BUILDKIT", "1");
    }
    if std::env::var_os("COMPOSE_DOCKER_CLI_BUILD").is_none() {
        cmd.env("COMPOSE_DOCKER_CLI_BUILD", "1");
    }
}

fn maybe_print_buildx_hint() {
    if which::which("docker").is_err() {
        return;
    }

    let buildx_ok = Command::new("docker")
        .arg("buildx")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !buildx_ok {
        println!(
            "Hint: Docker buildx plugin is not available. Install it to remove legacy builder warnings."
        );
    }
}

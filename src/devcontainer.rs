use crate::lockfile::DevsyncLock;
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::fs;
use std::path::Path;

pub fn generate_devcontainer(
    root: &Path,
    lock: &DevsyncLock,
    force: bool,
    primary_only: bool,
) -> Result<()> {
    let devcontainer_dir = root.join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir)
        .with_context(|| format!("failed to create {}", devcontainer_dir.display()))?;

    let dockerfile_path = devcontainer_dir.join("Dockerfile");
    let config_path = devcontainer_dir.join("devcontainer.json");

    write_if_allowed(
        &dockerfile_path,
        &build_dockerfile(lock, primary_only),
        force,
        "Dockerfile",
    )?;

    let config_json = build_devcontainer_json(lock, primary_only);
    let serialized = serde_json::to_string_pretty(&config_json)
        .context("failed to serialize devcontainer.json")?;

    write_if_allowed(
        &config_path,
        &(serialized + "\n"),
        force,
        "devcontainer.json",
    )?;

    Ok(())
}

fn write_if_allowed(path: &Path, content: &str, force: bool, file_label: &str) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists at {}. Re-run with --force to overwrite.",
            file_label,
            path.display()
        );
    }

    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn build_devcontainer_json(lock: &DevsyncLock, primary_only: bool) -> serde_json::Value {
    let mut extensions = vec!["eamodio.gitlens".to_string()];
    if stack_enabled(lock, "node", primary_only) {
        extensions.push("dbaeumer.vscode-eslint".to_string());
    }
    if stack_enabled(lock, "python", primary_only) {
        extensions.push("ms-python.python".to_string());
        extensions.push("charliermarsh.ruff".to_string());
    }
    if stack_enabled(lock, "rust", primary_only) {
        extensions.push("rust-lang.rust-analyzer".to_string());
    }

    let post_create_command = build_post_create_command(lock, primary_only);

    let mut json_obj = json!({
        "name": format!("DevSync: {}", lock.project.name),
        "build": {
            "dockerfile": "Dockerfile",
            "context": ".."
        },
        "customizations": {
            "vscode": {
                "extensions": extensions
            }
        },
        "remoteUser": "root"
    });

    if let Some(command) = post_create_command {
        json_obj["postCreateCommand"] = json!(command);
    }

    json_obj
}

fn build_dockerfile(lock: &DevsyncLock, primary_only: bool) -> String {
    let mut dockerfile = String::new();
    dockerfile.push_str("FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04\n\n");

    let mut apt_packages = vec![
        "curl".to_string(),
        "ca-certificates".to_string(),
        "git".to_string(),
        "build-essential".to_string(),
    ];
    if stack_enabled(lock, "python", primary_only) {
        apt_packages.push("python3".to_string());
        apt_packages.push("python3-pip".to_string());
    }

    let apt_packages = apt_packages.join(" ");
    dockerfile.push_str(
        &format!(
            "RUN apt-get update \\\n    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \\\n        {apt_packages} \\\n    && rm -rf /var/lib/apt/lists/*\n\n"
        ),
    );

    if stack_enabled(lock, "node", primary_only) {
        dockerfile.push_str(
            "# Install Node.js LTS for Node projects\nRUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \\\n    && apt-get update \\\n    && DEBIAN_FRONTEND=noninteractive apt-get install -y nodejs \\\n    && rm -rf /var/lib/apt/lists/*\n\n",
        );
    }

    if stack_enabled(lock, "rust", primary_only) {
        dockerfile.push_str(
            "# Install Rust toolchain for Rust projects\nRUN curl https://sh.rustup.rs -sSf | sh -s -- -y \\\n    && ln -s /root/.cargo/bin/rustc /usr/local/bin/rustc \\\n    && ln -s /root/.cargo/bin/cargo /usr/local/bin/cargo\n\n",
        );
    }

    dockerfile.push_str("WORKDIR /workspace\n");
    dockerfile
}

fn build_post_create_command(lock: &DevsyncLock, primary_only: bool) -> Option<String> {
    let mut commands: Vec<String> = Vec::new();
    let mut install_steps: Vec<String> = Vec::new();

    if stack_enabled(lock, "node", primary_only) {
        match lock.package_managers.node.as_deref() {
            Some("pnpm") => {
                install_steps.push(
                    "if [ -f package.json ]; then corepack enable && HUSKY=0 pnpm install; else echo 'Skipping pnpm install: package.json not found'; fi"
                        .to_string(),
                )
            }
            Some("yarn") => {
                install_steps.push(
                    "if [ -f package.json ]; then corepack enable && HUSKY=0 yarn install; else echo 'Skipping yarn install: package.json not found'; fi"
                        .to_string(),
                )
            }
            Some("npm") => install_steps.push(
                "if [ -f package.json ]; then HUSKY=0 npm install; else echo 'Skipping npm install: package.json not found'; fi"
                    .to_string(),
            ),
            Some("bun") => install_steps.push(
                "if [ -f package.json ]; then HUSKY=0 bun install; else echo 'Skipping bun install: package.json not found'; fi"
                    .to_string(),
            ),
            _ => {}
        }
    }

    if stack_enabled(lock, "python", primary_only) {
        match lock.package_managers.python.as_deref() {
            Some("uv") => install_steps.push(
                "if [ -f pyproject.toml ] || [ -f uv.lock ]; then uv sync; else echo 'Skipping uv sync: pyproject.toml/uv.lock not found'; fi"
                    .to_string(),
            ),
            Some("poetry") => install_steps.push(
                "if [ -f pyproject.toml ] || [ -f poetry.lock ]; then poetry install; else echo 'Skipping poetry install: pyproject.toml/poetry.lock not found'; fi"
                    .to_string(),
            ),
            Some("pipenv") => install_steps.push(
                "if [ -f Pipfile ]; then pipenv install; else echo 'Skipping pipenv install: Pipfile not found'; fi"
                    .to_string(),
            ),
            Some("pip") => install_steps.push(
                "if [ -f requirements.txt ]; then python3 -m pip install -r requirements.txt; else echo 'Skipping pip install: requirements.txt not found'; fi"
                    .to_string(),
            ),
            _ => {}
        }
    }

    if stack_enabled(lock, "rust", primary_only) {
        install_steps.push(
            "if [ -f Cargo.toml ]; then cargo fetch; else echo 'Skipping cargo fetch: Cargo.toml not found'; fi"
                .to_string(),
        );
    }

    if install_steps.is_empty() {
        None
    } else {
        commands.push(
            "git config --global --add safe.directory ${containerWorkspaceFolder} || true"
                .to_string(),
        );
        commands.extend(install_steps);
        Some(commands.join(" && "))
    }
}

fn has_stack(lock: &DevsyncLock, stack: &str) -> bool {
    lock.project.stacks.iter().any(|detected| detected == stack)
}

fn stack_enabled(lock: &DevsyncLock, stack: &str, primary_only: bool) -> bool {
    if !has_stack(lock, stack) {
        return false;
    }
    if !primary_only {
        return true;
    }

    match lock.primary_stack.as_deref() {
        Some(primary_stack) => primary_stack == stack,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{PackageManagerSection, ProjectSection, RuntimeSection};

    fn sample_lock() -> DevsyncLock {
        DevsyncLock {
            schema_version: 1,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            project: ProjectSection {
                name: "sample".to_string(),
                root: "/tmp/sample".to_string(),
                stacks: vec!["node".to_string(), "rust".to_string()],
            },
            runtimes: RuntimeSection {
                node: Some("20".to_string()),
                python: None,
                rust: Some("1.79.0".to_string()),
            },
            package_managers: PackageManagerSection {
                node: Some("pnpm".to_string()),
                python: None,
            },
            services: vec![],
            run_hints: vec!["cargo run -p sample".to_string()],
            primary_run_hint: Some("cargo run -p sample".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec![],
        }
    }

    #[test]
    fn post_create_respects_primary_only_mode() {
        let lock = sample_lock();

        let default_cmd =
            build_post_create_command(&lock, false).expect("default command expected");
        assert!(default_cmd.contains("safe.directory"));
        assert!(default_cmd.contains("if [ -f package.json ]"));
        assert!(default_cmd.contains("if [ -f Cargo.toml ]"));
        assert!(default_cmd.contains("pnpm install"));
        assert!(default_cmd.contains("cargo fetch"));

        let primary_cmd = build_post_create_command(&lock, true).expect("primary command expected");
        assert!(primary_cmd.contains("safe.directory"));
        assert!(!primary_cmd.contains("pnpm install"));
        assert!(primary_cmd.contains("cargo fetch"));
    }
}

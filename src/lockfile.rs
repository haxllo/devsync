use crate::detect::Detection;
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevsyncLock {
    pub schema_version: u32,
    pub generated_at: String,
    pub project: ProjectSection,
    pub runtimes: RuntimeSection,
    pub package_managers: PackageManagerSection,
    pub services: Vec<String>,
    #[serde(default)]
    pub run_hints: Vec<String>,
    #[serde(default)]
    pub primary_run_hint: Option<String>,
    #[serde(default)]
    pub primary_stack: Option<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSection {
    pub name: String,
    pub root: String,
    pub stacks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSection {
    pub node: Option<String>,
    pub python: Option<String>,
    pub rust: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManagerSection {
    pub node: Option<String>,
    pub python: Option<String>,
}

impl DevsyncLock {
    pub fn from_detection(detection: &Detection, previous: Option<&DevsyncLock>) -> Self {
        let mut lock = Self {
            schema_version: 1,
            generated_at: Utc::now().to_rfc3339(),
            project: ProjectSection {
                name: detection.project_name.clone(),
                root: detection.project_root.display().to_string(),
                stacks: detection.detected_stacks.clone(),
            },
            runtimes: RuntimeSection {
                node: detection.node_version.clone(),
                python: detection.python_version.clone(),
                rust: detection.rust_toolchain.clone(),
            },
            package_managers: PackageManagerSection {
                node: detection.node_package_manager.clone(),
                python: detection.python_package_manager.clone(),
            },
            services: detection.services.clone(),
            run_hints: detection.run_hints.clone(),
            primary_run_hint: detection.primary_run_hint.clone(),
            primary_stack: detection.primary_stack.clone(),
            recommendations: detection.recommendations.clone(),
        };

        // Preserve generated_at when lock content is unchanged to keep lock refresh deterministic.
        if let Some(existing) = previous {
            if lock.same_content(existing) {
                lock.generated_at = existing.generated_at.clone();
            }
        }

        lock
    }

    fn same_content(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.project == other.project
            && self.runtimes == other.runtimes
            && self.package_managers == other.package_managers
            && self.services == other.services
            && self.run_hints == other.run_hints
            && self.primary_run_hint == other.primary_run_hint
            && self.primary_stack == other.primary_stack
            && self.recommendations == other.recommendations
    }
}

pub fn write_lock(path: &Path, lock: &DevsyncLock, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists; rerun with --force to overwrite",
            path.display()
        );
    }

    let serialized = toml::to_string_pretty(lock).context("failed to serialize devsync.lock")?;
    fs::write(path, serialized).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn read_lock(path: &Path) -> Result<DevsyncLock> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read lockfile: {}", path.display()))?;
    let parsed = toml::from_str(&content)
        .with_context(|| format!("failed to parse lockfile: {}", path.display()))?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Detection;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn writes_and_reads_lockfile() {
        let dir = tempdir().expect("tempdir should be created");
        let lock_path = dir.path().join("devsync.lock");

        let detection = Detection {
            project_name: "demo".to_string(),
            project_root: PathBuf::from("/tmp/demo"),
            detected_stacks: vec!["node".to_string()],
            node_version: Some("20".to_string()),
            python_version: None,
            rust_toolchain: None,
            node_package_manager: Some("pnpm".to_string()),
            python_package_manager: None,
            services: vec!["postgres".to_string()],
            run_hints: vec!["cargo run -p demo".to_string()],
            primary_run_hint: Some("cargo run -p demo".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec!["pin versions".to_string()],
        };

        let lock = DevsyncLock::from_detection(&detection, None);
        write_lock(&lock_path, &lock, false).expect("lockfile should be written");

        let parsed = read_lock(&lock_path).expect("lockfile should be read");

        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.project.name, "demo");
        assert_eq!(parsed.runtimes.node.as_deref(), Some("20"));
        assert_eq!(parsed.package_managers.node.as_deref(), Some("pnpm"));
        assert!(parsed.services.contains(&"postgres".to_string()));
        assert!(parsed.run_hints.contains(&"cargo run -p demo".to_string()));
        assert_eq!(
            parsed.primary_run_hint.as_deref(),
            Some("cargo run -p demo")
        );
        assert_eq!(parsed.primary_stack.as_deref(), Some("rust"));
    }

    #[test]
    fn preserves_generated_at_when_content_is_unchanged() {
        let detection = Detection {
            project_name: "demo".to_string(),
            project_root: PathBuf::from("/tmp/demo"),
            detected_stacks: vec!["node".to_string()],
            node_version: Some("20".to_string()),
            python_version: None,
            rust_toolchain: None,
            node_package_manager: Some("pnpm".to_string()),
            python_package_manager: None,
            services: vec!["postgres".to_string()],
            run_hints: vec!["cargo run -p demo".to_string()],
            primary_run_hint: Some("cargo run -p demo".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec!["pin versions".to_string()],
        };

        let existing = DevsyncLock {
            schema_version: 1,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            project: ProjectSection {
                name: "demo".to_string(),
                root: "/tmp/demo".to_string(),
                stacks: vec!["node".to_string()],
            },
            runtimes: RuntimeSection {
                node: Some("20".to_string()),
                python: None,
                rust: None,
            },
            package_managers: PackageManagerSection {
                node: Some("pnpm".to_string()),
                python: None,
            },
            services: vec!["postgres".to_string()],
            run_hints: vec!["cargo run -p demo".to_string()],
            primary_run_hint: Some("cargo run -p demo".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec!["pin versions".to_string()],
        };

        let regenerated = DevsyncLock::from_detection(&detection, Some(&existing));
        assert_eq!(regenerated.generated_at, existing.generated_at);
    }
}

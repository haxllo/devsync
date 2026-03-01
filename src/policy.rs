use crate::lockfile::DevsyncLock;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    pub schema_version: u32,
    pub approved_base_images: Vec<String>,
    pub require_pinned_runtimes: bool,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            approved_base_images: vec![
                "mcr.microsoft.com/devcontainers/base:ubuntu-24.04".to_string(),
            ],
            require_pinned_runtimes: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyReport {
    pub project: String,
    pub policy_source: String,
    pub passed: bool,
    pub checks: Vec<PolicyCheck>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PolicyCheck {
    pub key: String,
    pub label: String,
    pub status: PolicyStatus,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub message: String,
    pub passing: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStatus {
    Ok,
    Warn,
}

pub fn run_policy(
    root: &Path,
    lock: Option<&DevsyncLock>,
    policy_path: Option<&Path>,
) -> Result<PolicyReport> {
    let (config, policy_source) = load_policy_config(root, policy_path)?;
    let mut checks = Vec::new();

    checks.push(check_policy_schema(config.schema_version));
    checks.push(check_base_image(root, &config.approved_base_images)?);

    if config.require_pinned_runtimes {
        checks.extend(check_runtime_pinning(lock));
    } else {
        checks.push(PolicyCheck {
            key: "policy.runtime_pinning".to_string(),
            label: "Pinned runtime policy".to_string(),
            status: PolicyStatus::Ok,
            expected: Some("require_pinned_runtimes = true".to_string()),
            actual: Some("false".to_string()),
            message: "Pinned runtime enforcement disabled by policy configuration.".to_string(),
            passing: true,
        });
    }

    let passed = checks.iter().all(|check| check.passing);
    Ok(PolicyReport {
        project: root.display().to_string(),
        policy_source,
        passed,
        checks,
    })
}

pub fn render_report(report: &PolicyReport) {
    println!("DevSync Policy\n==============");
    println!("Project: {}", report.project);
    println!("Policy source: {}", report.policy_source);
    println!();

    for check in &report.checks {
        let prefix = match check.status {
            PolicyStatus::Ok => "[ok]",
            PolicyStatus::Warn => "[warn]",
        };
        println!("{} {}: {}", prefix, check.label, check.message);
        if let Some(expected) = &check.expected {
            println!("  expected: {}", expected);
        }
        if let Some(actual) = &check.actual {
            println!("  actual: {}", actual);
        }
    }

    println!();
    if report.passed {
        println!("Result: policy-compliant");
    } else {
        println!("Result: policy violations found");
    }
}

fn load_policy_config(root: &Path, policy_path: Option<&Path>) -> Result<(PolicyConfig, String)> {
    let selected_path = match policy_path {
        Some(path) => Some(path.to_path_buf()),
        None => {
            let inferred = root.join("devsync.policy.toml");
            if inferred.is_file() {
                Some(inferred)
            } else {
                None
            }
        }
    };

    if let Some(path) = selected_path {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read policy file {}", path.display()))?;
        let parsed: PolicyConfig = toml::from_str(&raw)
            .with_context(|| format!("failed to parse policy file {}", path.display()))?;
        Ok((parsed, path.display().to_string()))
    } else {
        Ok((PolicyConfig::default(), "builtin-default".to_string()))
    }
}

fn check_policy_schema(schema_version: u32) -> PolicyCheck {
    if schema_version == SUPPORTED_SCHEMA_VERSION {
        PolicyCheck {
            key: "policy.schema".to_string(),
            label: "Policy schema version".to_string(),
            status: PolicyStatus::Ok,
            expected: Some(SUPPORTED_SCHEMA_VERSION.to_string()),
            actual: Some(schema_version.to_string()),
            message: "Policy schema is supported.".to_string(),
            passing: true,
        }
    } else {
        PolicyCheck {
            key: "policy.schema".to_string(),
            label: "Policy schema version".to_string(),
            status: PolicyStatus::Warn,
            expected: Some(SUPPORTED_SCHEMA_VERSION.to_string()),
            actual: Some(schema_version.to_string()),
            message: format!(
                "Unsupported policy schema version {}. Upgrade DevSync or adjust policy file.",
                schema_version
            ),
            passing: false,
        }
    }
}

fn check_base_image(root: &Path, approved_base_images: &[String]) -> Result<PolicyCheck> {
    if approved_base_images.is_empty() {
        return Ok(PolicyCheck {
            key: "policy.base_image".to_string(),
            label: "Approved base image".to_string(),
            status: PolicyStatus::Ok,
            expected: Some("non-empty approved_base_images".to_string()),
            actual: None,
            message: "No base-image restrictions configured (approved_base_images empty)."
                .to_string(),
            passing: true,
        });
    }

    let dockerfile_path: PathBuf = root.join(".devcontainer").join("Dockerfile");
    if !dockerfile_path.is_file() {
        return Ok(PolicyCheck {
            key: "policy.base_image".to_string(),
            label: "Approved base image".to_string(),
            status: PolicyStatus::Warn,
            expected: Some(approved_base_images.join(", ")),
            actual: None,
            message: format!(
                "Missing {}; cannot validate approved base image policy.",
                dockerfile_path.display()
            ),
            passing: false,
        });
    }

    let dockerfile_raw = fs::read_to_string(&dockerfile_path)
        .with_context(|| format!("failed to read {}", dockerfile_path.display()))?;
    let Some(base_image) = parse_base_image(&dockerfile_raw) else {
        return Ok(PolicyCheck {
            key: "policy.base_image".to_string(),
            label: "Approved base image".to_string(),
            status: PolicyStatus::Warn,
            expected: Some(approved_base_images.join(", ")),
            actual: None,
            message: format!(
                "Could not parse base image from {}",
                dockerfile_path.display()
            ),
            passing: false,
        });
    };

    if approved_base_images
        .iter()
        .any(|allowed| allowed == &base_image)
    {
        Ok(PolicyCheck {
            key: "policy.base_image".to_string(),
            label: "Approved base image".to_string(),
            status: PolicyStatus::Ok,
            expected: Some(approved_base_images.join(", ")),
            actual: Some(base_image),
            message: "Dockerfile base image is approved by policy.".to_string(),
            passing: true,
        })
    } else {
        Ok(PolicyCheck {
            key: "policy.base_image".to_string(),
            label: "Approved base image".to_string(),
            status: PolicyStatus::Warn,
            expected: Some(approved_base_images.join(", ")),
            actual: Some(base_image),
            message: "Dockerfile base image is not in approved_base_images.".to_string(),
            passing: false,
        })
    }
}

fn parse_base_image(dockerfile: &str) -> Option<String> {
    for line in dockerfile.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.to_ascii_uppercase().starts_with("FROM ") {
            continue;
        }

        let mut tokens = trimmed.split_whitespace();
        let _from = tokens.next()?;
        let mut candidate = tokens.next()?;
        while candidate.starts_with("--") {
            candidate = tokens.next()?;
        }

        return Some(candidate.to_string());
    }
    None
}

fn check_runtime_pinning(lock: Option<&DevsyncLock>) -> Vec<PolicyCheck> {
    let Some(lock) = lock else {
        return vec![PolicyCheck {
            key: "policy.runtime.lock_present".to_string(),
            label: "Runtime pinning source".to_string(),
            status: PolicyStatus::Warn,
            expected: Some("devsync.lock present".to_string()),
            actual: None,
            message: "devsync.lock missing; cannot enforce pinned runtime policy.".to_string(),
            passing: false,
        }];
    };

    let mut checks = Vec::new();
    checks.extend(runtime_pin_check(
        &lock.project.stacks,
        "node",
        lock.runtimes.node.as_deref(),
    ));
    checks.extend(runtime_pin_check(
        &lock.project.stacks,
        "python",
        lock.runtimes.python.as_deref(),
    ));
    checks.extend(runtime_pin_check(
        &lock.project.stacks,
        "rust",
        lock.runtimes.rust.as_deref(),
    ));
    checks
}

fn runtime_pin_check(stacks: &[String], stack: &str, value: Option<&str>) -> Vec<PolicyCheck> {
    if !stacks.iter().any(|detected| detected == stack) {
        return Vec::new();
    }

    let label = format!("Pinned runtime: {stack}");
    let key = format!("policy.runtime.{stack}");
    match value {
        Some(raw) if is_runtime_pinned(raw) => vec![PolicyCheck {
            key,
            label,
            status: PolicyStatus::Ok,
            expected: Some("exact version pin".to_string()),
            actual: Some(raw.to_string()),
            message: "Runtime version is pinned to a deterministic value.".to_string(),
            passing: true,
        }],
        Some(raw) => vec![PolicyCheck {
            key,
            label,
            status: PolicyStatus::Warn,
            expected: Some("exact version pin".to_string()),
            actual: Some(raw.to_string()),
            message: "Runtime value is not strictly pinned (range/channel/wildcard detected)."
                .to_string(),
            passing: false,
        }],
        None => vec![PolicyCheck {
            key,
            label,
            status: PolicyStatus::Warn,
            expected: Some("exact version pin".to_string()),
            actual: None,
            message: "Runtime value missing from lockfile for detected stack.".to_string(),
            passing: false,
        }],
    }
}

fn is_runtime_pinned(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if !trimmed.chars().any(|ch| ch.is_ascii_digit()) {
        return false;
    }
    if trimmed
        .chars()
        .any(|ch| matches!(ch, '>' | '<' | '^' | '~' | '*' | '|' | '=' | ' '))
    {
        return false;
    }
    if trimmed.contains("x") || trimmed.contains("X") {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{PackageManagerSection, ProjectSection, RuntimeSection};
    use tempfile::tempdir;

    fn sample_lock(node: Option<&str>, rust: Option<&str>) -> DevsyncLock {
        DevsyncLock {
            schema_version: 1,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            project: ProjectSection {
                name: "sample".to_string(),
                root: "/tmp/sample".to_string(),
                stacks: vec!["node".to_string(), "rust".to_string()],
            },
            runtimes: RuntimeSection {
                node: node.map(ToString::to_string),
                python: None,
                rust: rust.map(ToString::to_string),
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
    fn runtime_pin_policy_flags_unpinned_values() {
        let dir = tempdir().expect("tempdir should be created");
        let devcontainer_dir = dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).expect("devcontainer dir should exist");
        fs::write(
            devcontainer_dir.join("Dockerfile"),
            "FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04\n",
        )
        .expect("dockerfile should be written");

        let report = run_policy(
            dir.path(),
            Some(&sample_lock(Some(">=20"), Some("stable"))),
            None,
        )
        .expect("policy run should succeed");
        assert!(!report.passed);
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.key == "policy.runtime.node" && !check.passing)
        );
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.key == "policy.runtime.rust" && !check.passing)
        );
    }

    #[test]
    fn base_image_policy_uses_config_file() {
        let dir = tempdir().expect("tempdir should be created");
        let devcontainer_dir = dir.path().join(".devcontainer");
        fs::create_dir_all(&devcontainer_dir).expect("devcontainer dir should exist");
        fs::write(devcontainer_dir.join("Dockerfile"), "FROM ubuntu:24.04\n")
            .expect("dockerfile should be written");

        let policy_path = dir.path().join("custom.policy.toml");
        fs::write(
            &policy_path,
            r#"
schema_version = 1
approved_base_images = ["mcr.microsoft.com/devcontainers/base:ubuntu-24.04"]
require_pinned_runtimes = false
"#,
        )
        .expect("policy should be written");

        let report = run_policy(
            dir.path(),
            Some(&sample_lock(Some("20.11.1"), Some("1.79.0"))),
            Some(&policy_path),
        )
        .expect("policy run should succeed");
        assert!(!report.passed);
        let base_image = report
            .checks
            .iter()
            .find(|check| check.key == "policy.base_image")
            .expect("base image check should exist");
        assert!(!base_image.passing);
        assert_eq!(base_image.actual.as_deref(), Some("ubuntu:24.04"));
    }

    #[test]
    fn parse_base_image_supports_platform_argument() {
        let parsed = parse_base_image(
            "FROM --platform=linux/amd64 mcr.microsoft.com/devcontainers/base:ubuntu-24.04 AS base",
        );
        assert_eq!(
            parsed.as_deref(),
            Some("mcr.microsoft.com/devcontainers/base:ubuntu-24.04")
        );
    }
}

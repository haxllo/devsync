use crate::lockfile::read_lock;
use crate::{policy, secrets};
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct ActivationReport {
    pub project: String,
    pub ready: bool,
    pub score: u8,
    pub items: Vec<ActivationItem>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationItem {
    pub key: String,
    pub label: String,
    pub state: ActivationState,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivationState {
    Complete,
    Action,
}

pub fn run_activation(root: &Path) -> Result<ActivationReport> {
    let mut items = Vec::new();

    let lock_path = root.join("devsync.lock");
    let lock = if lock_path.is_file() {
        match read_lock(&lock_path) {
            Ok(lock) => {
                items.push(ActivationItem {
                    key: "lock.present".to_string(),
                    label: "Lockfile".to_string(),
                    state: ActivationState::Complete,
                    message: "devsync.lock exists and is readable.".to_string(),
                });
                Some(lock)
            }
            Err(err) => {
                items.push(ActivationItem {
                    key: "lock.present".to_string(),
                    label: "Lockfile".to_string(),
                    state: ActivationState::Action,
                    message: format!("devsync.lock exists but failed to parse: {err}"),
                });
                None
            }
        }
    } else {
        items.push(ActivationItem {
            key: "lock.present".to_string(),
            label: "Lockfile".to_string(),
            state: ActivationState::Action,
            message: "Run `devsync init` to generate devsync.lock.".to_string(),
        });
        None
    };

    let has_devcontainer_json = root
        .join(".devcontainer")
        .join("devcontainer.json")
        .is_file();
    let has_dockerfile = root.join(".devcontainer").join("Dockerfile").is_file();
    if has_devcontainer_json && has_dockerfile {
        items.push(ActivationItem {
            key: "devcontainer.generated".to_string(),
            label: "Devcontainer artifacts".to_string(),
            state: ActivationState::Complete,
            message: "Both .devcontainer/devcontainer.json and Dockerfile are present.".to_string(),
        });
    } else {
        items.push(ActivationItem {
            key: "devcontainer.generated".to_string(),
            label: "Devcontainer artifacts".to_string(),
            state: ActivationState::Action,
            message:
                "Generate .devcontainer files with `devsync init` (or pull with --with-devcontainer)."
                    .to_string(),
        });
    }

    let policy_report = policy::run_policy(root, lock.as_ref(), None)?;
    if policy_report.passed {
        items.push(ActivationItem {
            key: "policy".to_string(),
            label: "Policy checks".to_string(),
            state: ActivationState::Complete,
            message: "Repository passes configured policy checks.".to_string(),
        });
    } else {
        items.push(ActivationItem {
            key: "policy".to_string(),
            label: "Policy checks".to_string(),
            state: ActivationState::Action,
            message: "Run `devsync policy` and resolve reported policy violations.".to_string(),
        });
    }

    let secret_report = secrets::run_secret_lint(root)?;
    if secret_report.passed {
        items.push(ActivationItem {
            key: "secret_lint".to_string(),
            label: "Secret lint".to_string(),
            state: ActivationState::Complete,
            message: "No secret-like values detected in generated artifacts.".to_string(),
        });
    } else {
        items.push(ActivationItem {
            key: "secret_lint".to_string(),
            label: "Secret lint".to_string(),
            state: ActivationState::Action,
            message: "Run `devsync secret-lint` and remove embedded credentials.".to_string(),
        });
    }

    let completed = items
        .iter()
        .filter(|item| item.state == ActivationState::Complete)
        .count();
    let score = if items.is_empty() {
        0
    } else {
        ((completed * 100) / items.len()) as u8
    };
    let ready = items
        .iter()
        .all(|item| item.state == ActivationState::Complete);
    let next_actions = items
        .iter()
        .filter(|item| item.state == ActivationState::Action)
        .map(|item| item.message.clone())
        .collect();

    Ok(ActivationReport {
        project: root.display().to_string(),
        ready,
        score,
        items,
        next_actions,
    })
}

pub fn render_report(report: &ActivationReport) {
    println!("DevSync Activation\n==================");
    println!("Project: {}", report.project);
    println!("Readiness score: {}%", report.score);
    println!(
        "Status: {}",
        if report.ready {
            "ready"
        } else {
            "action required"
        }
    );
    println!();

    for item in &report.items {
        let prefix = match item.state {
            ActivationState::Complete => "[ok]",
            ActivationState::Action => "[action]",
        };
        println!("{} {}: {}", prefix, item.label, item.message);
    }

    if !report.next_actions.is_empty() {
        println!();
        println!("Next actions:");
        for action in &report.next_actions {
            println!("- {}", action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{
        DevsyncLock, PackageManagerSection, ProjectSection, RuntimeSection, write_lock,
    };
    use std::fs;
    use tempfile::tempdir;

    fn sample_lock(root: &Path) -> DevsyncLock {
        DevsyncLock {
            schema_version: 1,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            project: ProjectSection {
                name: "sample".to_string(),
                root: root.display().to_string(),
                stacks: vec!["rust".to_string()],
            },
            runtimes: RuntimeSection {
                node: None,
                python: None,
                rust: Some("1.79.0".to_string()),
            },
            package_managers: PackageManagerSection {
                node: None,
                python: None,
            },
            services: vec![],
            run_hints: vec!["cargo run".to_string()],
            primary_run_hint: Some("cargo run".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec![],
        }
    }

    #[test]
    fn activation_ready_when_required_artifacts_exist() {
        let dir = tempdir().expect("tempdir should be created");
        write_lock(
            &dir.path().join("devsync.lock"),
            &sample_lock(dir.path()),
            false,
        )
        .expect("lock should be written");
        fs::create_dir_all(dir.path().join(".devcontainer"))
            .expect("devcontainer dir should be created");
        fs::write(
            dir.path().join(".devcontainer").join("Dockerfile"),
            "FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04\n",
        )
        .expect("dockerfile should be written");
        fs::write(
            dir.path().join(".devcontainer").join("devcontainer.json"),
            "{}\n",
        )
        .expect("devcontainer json should be written");

        let report = run_activation(dir.path()).expect("activation should run");
        assert!(report.ready);
        assert_eq!(report.score, 100);
    }

    #[test]
    fn activation_reports_actions_when_lock_is_missing() {
        let dir = tempdir().expect("tempdir should be created");
        let report = run_activation(dir.path()).expect("activation should run");
        assert!(!report.ready);
        assert!(
            report
                .items
                .iter()
                .any(|item| item.key == "lock.present" && item.state == ActivationState::Action)
        );
    }
}

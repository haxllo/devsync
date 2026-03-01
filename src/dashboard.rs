use crate::activation;
use crate::detect::detect_project;
use crate::roi::{RoiInput, RoiReport, compute_roi};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DashboardOptions {
    pub root: PathBuf,
    pub max_repos: Option<usize>,
    pub roi_input: RoiInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardReport {
    pub generated_at: String,
    pub root: String,
    pub repos_scanned: usize,
    pub in_scope_repos: usize,
    pub ready_repos: usize,
    pub avg_readiness_score: f64,
    pub repos: Vec<DashboardRepoRow>,
    pub roi: RoiReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRepoRow {
    pub repo: String,
    pub in_scope: bool,
    pub stacks: Vec<String>,
    pub activation_ready: Option<bool>,
    pub readiness_score: Option<u8>,
    pub note: Option<String>,
}

pub fn build_dashboard(options: DashboardOptions) -> Result<DashboardReport> {
    let repos = discover_repos(&options.root, options.max_repos)?;
    let mut rows = Vec::new();
    let mut in_scope_repos = 0usize;
    let mut ready_repos = 0usize;
    let mut readiness_total = 0usize;
    let mut readiness_count = 0usize;

    for repo in repos {
        match detect_project(&repo) {
            Ok(detection) => {
                if detection.detected_stacks.is_empty() {
                    rows.push(DashboardRepoRow {
                        repo: repo.display().to_string(),
                        in_scope: false,
                        stacks: Vec::new(),
                        activation_ready: None,
                        readiness_score: None,
                        note: Some("out-of-scope".to_string()),
                    });
                    continue;
                }

                in_scope_repos += 1;
                match activation::run_activation(&repo) {
                    Ok(activation) => {
                        if activation.ready {
                            ready_repos += 1;
                        }
                        readiness_total += usize::from(activation.score);
                        readiness_count += 1;
                        rows.push(DashboardRepoRow {
                            repo: repo.display().to_string(),
                            in_scope: true,
                            stacks: detection.detected_stacks,
                            activation_ready: Some(activation.ready),
                            readiness_score: Some(activation.score),
                            note: None,
                        });
                    }
                    Err(err) => rows.push(DashboardRepoRow {
                        repo: repo.display().to_string(),
                        in_scope: true,
                        stacks: detection.detected_stacks,
                        activation_ready: Some(false),
                        readiness_score: Some(0),
                        note: Some(format!("activation-error: {err}")),
                    }),
                }
            }
            Err(err) => rows.push(DashboardRepoRow {
                repo: repo.display().to_string(),
                in_scope: false,
                stacks: Vec::new(),
                activation_ready: None,
                readiness_score: None,
                note: Some(format!("detect-error: {err}")),
            }),
        }
    }

    let avg_readiness_score = if readiness_count > 0 {
        round2((readiness_total as f64) / (readiness_count as f64))
    } else {
        0.0
    };
    let roi = compute_roi(&options.roi_input)?;

    Ok(DashboardReport {
        generated_at: Utc::now().to_rfc3339(),
        root: options.root.display().to_string(),
        repos_scanned: rows.len(),
        in_scope_repos,
        ready_repos,
        avg_readiness_score,
        repos: rows,
        roi,
    })
}

pub fn write_dashboard(report: &DashboardReport, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let serialized =
        serde_json::to_string_pretty(report).context("failed to serialize dashboard report")?;
    fs::write(output, serialized + "\n")
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(())
}

fn discover_repos(root: &Path, max_repos: Option<usize>) -> Result<Vec<PathBuf>> {
    let mut repos = Vec::new();
    for entry in WalkDir::new(root).min_depth(2).max_depth(3) {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry.file_name() != ".git" {
            continue;
        }
        if let Some(parent) = entry.path().parent() {
            repos.push(parent.to_path_buf());
        }
    }
    repos.sort();
    if let Some(limit) = max_repos {
        repos.truncate(limit);
    }
    Ok(repos)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
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
                name: "repo-a".to_string(),
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
    fn dashboard_builds_summary() {
        let root = tempdir().expect("tempdir should be created");
        let repo_a = root.path().join("team").join("repo-a");
        let repo_b = root.path().join("team").join("repo-b");
        fs::create_dir_all(repo_a.join(".git")).expect("repo-a git should exist");
        fs::create_dir_all(repo_b.join(".git")).expect("repo-b git should exist");
        fs::write(
            repo_a.join("Cargo.toml"),
            "[package]\nname=\"a\"\nversion=\"0.1.0\"\n",
        )
        .expect("cargo should be written");
        write_lock(&repo_a.join("devsync.lock"), &sample_lock(&repo_a), false)
            .expect("lock should be written");
        fs::create_dir_all(repo_a.join(".devcontainer")).expect("devcontainer should exist");
        fs::write(
            repo_a.join(".devcontainer").join("Dockerfile"),
            "FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04\n",
        )
        .expect("dockerfile should be written");
        fs::write(
            repo_a.join(".devcontainer").join("devcontainer.json"),
            "{}\n",
        )
        .expect("devcontainer json should be written");

        let report = build_dashboard(DashboardOptions {
            root: root.path().to_path_buf(),
            max_repos: None,
            roi_input: RoiInput {
                team_size: 10,
                monthly_hires: 1.0,
                onboarding_hours_before: 6.0,
                onboarding_hours_after: 2.0,
                drift_incidents_per_dev: 0.5,
                drift_hours_per_incident: 1.0,
                drift_reduction_pct: 50.0,
                hourly_rate: 100.0,
                price_per_dev: 15.0,
            },
        })
        .expect("dashboard should build");

        assert_eq!(report.repos_scanned, 2);
        assert_eq!(report.in_scope_repos, 1);
        assert_eq!(report.ready_repos, 1);
        assert!(report.avg_readiness_score >= 99.0);
    }
}

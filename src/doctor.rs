use crate::lockfile::DevsyncLock;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub project: String,
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DoctorCheck {
    pub key: String,
    pub label: String,
    pub category: CheckCategory,
    pub status: CheckStatus,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub message: String,
    pub passing: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Info,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckCategory {
    Tooling,
    Runtime,
    Lockfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePolicy {
    All,
    Runtime,
    Lockfile,
    Tooling,
    RuntimeAndLock,
    None,
}

pub fn run_doctor(root: &Path, lock: Option<&DevsyncLock>) -> Result<DoctorReport> {
    let mut checks = Vec::new();

    checks.push(check_tool("docker", "Docker engine", "docker"));
    checks.push(check_tool(
        "devcontainer",
        "Dev Container CLI",
        "devcontainer",
    ));

    if let Some(lock) = lock {
        checks.push(check_runtime(
            "runtime.node",
            "Node runtime",
            lock.runtimes.node.as_deref(),
            "node",
            &["--version"],
        ));
        checks.push(check_runtime(
            "runtime.python",
            "Python runtime",
            lock.runtimes.python.as_deref(),
            "python3",
            &["--version"],
        ));
        checks.push(check_runtime(
            "runtime.rust",
            "Rust runtime",
            lock.runtimes.rust.as_deref(),
            "rustc",
            &["--version"],
        ));

        if lock.schema_version == 1 {
            checks.push(DoctorCheck {
                key: "lock.schema".to_string(),
                label: "Lock schema version".to_string(),
                category: CheckCategory::Lockfile,
                status: CheckStatus::Ok,
                expected: Some("1".to_string()),
                actual: Some(lock.schema_version.to_string()),
                message: "Lock schema is supported by this CLI version.".to_string(),
                passing: true,
            });
        } else {
            checks.push(DoctorCheck {
                key: "lock.schema".to_string(),
                label: "Lock schema version".to_string(),
                category: CheckCategory::Lockfile,
                status: CheckStatus::Warn,
                expected: Some("1".to_string()),
                actual: Some(lock.schema_version.to_string()),
                message: format!(
                    "Lock schema v{} is not recognized by this CLI version.",
                    lock.schema_version
                ),
                passing: false,
            });
        }
    } else {
        checks.push(DoctorCheck {
            key: "lock.present".to_string(),
            label: "Lockfile present".to_string(),
            category: CheckCategory::Lockfile,
            status: CheckStatus::Warn,
            expected: Some("devsync.lock".to_string()),
            actual: None,
            message: "devsync.lock not found. Run `devsync init` first for version-aware checks."
                .to_string(),
            passing: false,
        });
    }

    let healthy = checks.iter().all(|check| check.passing);

    Ok(DoctorReport {
        project: root.display().to_string(),
        healthy,
        checks,
    })
}

pub fn report_should_fail(report: &DoctorReport, policy: FailurePolicy) -> bool {
    if matches!(policy, FailurePolicy::None) {
        return false;
    }

    report
        .checks
        .iter()
        .filter(|check| !check.passing)
        .any(|check| match policy {
            FailurePolicy::All => true,
            FailurePolicy::Runtime => check.category == CheckCategory::Runtime,
            FailurePolicy::Lockfile => check.category == CheckCategory::Lockfile,
            FailurePolicy::Tooling => check.category == CheckCategory::Tooling,
            FailurePolicy::RuntimeAndLock => {
                check.category == CheckCategory::Runtime
                    || check.category == CheckCategory::Lockfile
            }
            FailurePolicy::None => false,
        })
}

pub fn render_report(report: &DoctorReport) {
    println!("DevSync Doctor\n==============");
    println!("Project: {}", report.project);
    println!();

    for check in &report.checks {
        let prefix = match check.status {
            CheckStatus::Ok => "[ok]",
            CheckStatus::Warn => "[warn]",
            CheckStatus::Info => "[info]",
        };
        let category = match check.category {
            CheckCategory::Tooling => "tooling",
            CheckCategory::Runtime => "runtime",
            CheckCategory::Lockfile => "lockfile",
        };

        println!(
            "{} [{}] {}: {}",
            prefix, category, check.label, check.message
        );

        if let Some(expected) = &check.expected {
            println!("  expected: {}", expected);
        }
        if let Some(actual) = &check.actual {
            println!("  actual: {}", actual);
        }
    }

    println!();
    if report.healthy {
        println!("Result: healthy");
    } else {
        println!("Result: issues found");
    }
}

fn check_tool(key: &str, label: &str, command: &str) -> DoctorCheck {
    match which::which(command) {
        Ok(path) => DoctorCheck {
            key: key.to_string(),
            label: label.to_string(),
            category: CheckCategory::Tooling,
            status: CheckStatus::Ok,
            expected: Some(format!("{command} installed")),
            actual: Some(path.display().to_string()),
            message: format!("Found `{command}`."),
            passing: true,
        },
        Err(_) => DoctorCheck {
            key: key.to_string(),
            label: label.to_string(),
            category: CheckCategory::Tooling,
            status: CheckStatus::Warn,
            expected: Some(format!("{command} installed")),
            actual: None,
            message: format!("Missing `{command}`."),
            passing: false,
        },
    }
}

fn check_runtime(
    key: &str,
    label: &str,
    expected: Option<&str>,
    command: &str,
    args: &[&str],
) -> DoctorCheck {
    match expected {
        None => DoctorCheck {
            key: key.to_string(),
            label: label.to_string(),
            category: CheckCategory::Runtime,
            status: CheckStatus::Info,
            expected: None,
            actual: None,
            message: "No version pinned in lockfile.".to_string(),
            passing: true,
        },
        Some(expected_version) => {
            if which::which(command).is_err() {
                return DoctorCheck {
                    key: key.to_string(),
                    label: label.to_string(),
                    category: CheckCategory::Runtime,
                    status: CheckStatus::Warn,
                    expected: Some(expected_version.to_string()),
                    actual: None,
                    message: format!(
                        "Expected `{expected_version}` but `{command}` is not installed."
                    ),
                    passing: false,
                };
            }

            let output = Command::new(command).args(args).output();
            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let version_text = if stdout.is_empty() { stderr } else { stdout };
                    let matches = expected_matches_actual(expected_version, &version_text);

                    if matches {
                        DoctorCheck {
                            key: key.to_string(),
                            label: label.to_string(),
                            category: CheckCategory::Runtime,
                            status: CheckStatus::Ok,
                            expected: Some(expected_version.to_string()),
                            actual: Some(version_text),
                            message: "Installed runtime matches lockfile expectation.".to_string(),
                            passing: true,
                        }
                    } else {
                        DoctorCheck {
                            key: key.to_string(),
                            label: label.to_string(),
                            category: CheckCategory::Runtime,
                            status: CheckStatus::Warn,
                            expected: Some(expected_version.to_string()),
                            actual: Some(version_text),
                            message: "Installed runtime does not match lockfile expectation."
                                .to_string(),
                            passing: false,
                        }
                    }
                }
                Ok(output) => DoctorCheck {
                    key: key.to_string(),
                    label: label.to_string(),
                    category: CheckCategory::Runtime,
                    status: CheckStatus::Warn,
                    expected: Some(expected_version.to_string()),
                    actual: None,
                    message: format!(
                        "Failed to execute `{command}` (exit code {:?}).",
                        output.status.code()
                    ),
                    passing: false,
                },
                Err(err) => DoctorCheck {
                    key: key.to_string(),
                    label: label.to_string(),
                    category: CheckCategory::Runtime,
                    status: CheckStatus::Warn,
                    expected: Some(expected_version.to_string()),
                    actual: None,
                    message: format!("Failed to execute `{command}` ({err})."),
                    passing: false,
                },
            }
        }
    }
}

fn expected_matches_actual(expected: &str, actual: &str) -> bool {
    let token = extract_version_token(expected);
    match token {
        Some(token) => actual.contains(&token),
        None => true,
    }
}

fn extract_version_token(raw: &str) -> Option<String> {
    let mut token = String::new();
    let mut started = false;

    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            token.push(ch);
            started = true;
        } else if started && (ch == '.' || ch.is_ascii_alphabetic()) {
            token.push(ch);
        } else if started {
            break;
        }
    }

    if token.is_empty() { None } else { Some(token) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_version_token() {
        assert_eq!(extract_version_token(">=20"), Some("20".to_string()));
        assert_eq!(extract_version_token("v1.77.2"), Some("1.77.2".to_string()));
        assert_eq!(extract_version_token("python"), None);
    }

    #[test]
    fn expected_matching_works() {
        assert!(expected_matches_actual(">=20", "v20.11.1"));
        assert!(!expected_matches_actual("3.11", "Python 3.10.8"));
    }

    #[test]
    fn report_health_depends_on_passing_checks() {
        let report = DoctorReport {
            project: "/tmp/project".to_string(),
            healthy: false,
            checks: vec![DoctorCheck {
                key: "docker".to_string(),
                label: "Docker engine".to_string(),
                category: CheckCategory::Tooling,
                status: CheckStatus::Warn,
                expected: Some("docker installed".to_string()),
                actual: None,
                message: "Missing docker".to_string(),
                passing: false,
            }],
        };

        assert!(!report.healthy);
    }

    #[test]
    fn failure_policy_targets_categories() {
        let report = DoctorReport {
            project: "/tmp/project".to_string(),
            healthy: false,
            checks: vec![
                DoctorCheck {
                    key: "docker".to_string(),
                    label: "Docker engine".to_string(),
                    category: CheckCategory::Tooling,
                    status: CheckStatus::Warn,
                    expected: Some("docker installed".to_string()),
                    actual: None,
                    message: "Missing docker".to_string(),
                    passing: false,
                },
                DoctorCheck {
                    key: "runtime.rust".to_string(),
                    label: "Rust runtime".to_string(),
                    category: CheckCategory::Runtime,
                    status: CheckStatus::Warn,
                    expected: Some("1.80".to_string()),
                    actual: Some("rustc 1.79.0".to_string()),
                    message: "Mismatch".to_string(),
                    passing: false,
                },
            ],
        };

        assert!(report_should_fail(&report, FailurePolicy::All));
        assert!(report_should_fail(&report, FailurePolicy::Tooling));
        assert!(report_should_fail(&report, FailurePolicy::Runtime));
        assert!(report_should_fail(&report, FailurePolicy::RuntimeAndLock));
        assert!(!report_should_fail(&report, FailurePolicy::Lockfile));
        assert!(!report_should_fail(&report, FailurePolicy::None));
    }
}

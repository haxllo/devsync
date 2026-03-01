use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;

const TARGET_FILES: [&str; 3] = [
    "devsync.lock",
    ".devcontainer/devcontainer.json",
    ".devcontainer/Dockerfile",
];

#[derive(Debug, Clone, Serialize)]
pub struct SecretLintReport {
    pub project: String,
    pub passed: bool,
    pub scanned_files: Vec<String>,
    pub findings: Vec<SecretFinding>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecretFinding {
    pub path: String,
    pub line: usize,
    pub rule: String,
    pub snippet: String,
}

pub fn run_secret_lint(root: &Path) -> Result<SecretLintReport> {
    let mut scanned_files = Vec::new();
    let mut findings = Vec::new();

    for relative in TARGET_FILES {
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }

        scanned_files.push(relative.to_string());
        findings.extend(scan_file(root, &path)?);
    }

    let passed = findings.is_empty();
    Ok(SecretLintReport {
        project: root.display().to_string(),
        passed,
        scanned_files,
        findings,
    })
}

pub fn render_report(report: &SecretLintReport) {
    println!("DevSync Secret Lint\n==================");
    println!("Project: {}", report.project);
    if report.scanned_files.is_empty() {
        println!("Scanned files: none (no generated artifacts found)");
    } else {
        println!("Scanned files: {}", report.scanned_files.join(", "));
    }
    println!();

    if report.findings.is_empty() {
        println!("Result: no secret exposure findings");
        return;
    }

    println!("Findings:");
    for finding in &report.findings {
        println!(
            "- {}:{} [{}] {}",
            finding.path, finding.line, finding.rule, finding.snippet
        );
    }
    println!();
    println!("Result: potential secret exposure detected");
}

fn scan_file(root: &Path, path: &Path) -> Result<Vec<SecretFinding>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let display_path = relative_path(root, path);
    let mut findings = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;

        if let Some(rule) = detect_token_prefix(line) {
            findings.push(SecretFinding {
                path: display_path.clone(),
                line: line_number,
                rule: rule.to_string(),
                snippet: trimmed_snippet(line),
            });
        }

        if let Some(rule) = detect_assignment_secret(line) {
            findings.push(SecretFinding {
                path: display_path.clone(),
                line: line_number,
                rule: rule.to_string(),
                snippet: trimmed_snippet(line),
            });
        }
    }

    Ok(findings)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn detect_token_prefix(line: &str) -> Option<&'static str> {
    for token in tokenize(line) {
        if token.starts_with("AKIA")
            && token.len() == 20
            && token
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        {
            return Some("aws-access-key");
        }
        if token.starts_with("ghp_") && token.len() >= 40 {
            return Some("github-token");
        }
        if token.starts_with("sk-") && token.len() >= 24 {
            return Some("api-token-prefix");
        }
    }

    None
}

fn tokenize(line: &str) -> Vec<&str> {
    line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .filter(|token| !token.is_empty())
        .collect()
}

fn detect_assignment_secret(line: &str) -> Option<&'static str> {
    let lowercase = line.to_ascii_lowercase();
    let keywords = [
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "aws_access_key_id",
        "aws_secret_access_key",
    ];

    if !keywords.iter().any(|keyword| lowercase.contains(keyword)) {
        return None;
    }

    let assignment = line
        .split_once('=')
        .or_else(|| line.split_once(':'))
        .map(|(_, value)| value.trim())?;

    if assignment.is_empty() {
        return None;
    }

    let value = assignment
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .trim_end_matches(',')
        .trim();
    if value.len() < 8 {
        return None;
    }
    if value.starts_with('$') || value.starts_with('<') || value.starts_with('{') {
        return None;
    }
    if !value.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }

    if looks_like_placeholder(&lowercase) || looks_like_placeholder(&value.to_ascii_lowercase()) {
        return None;
    }

    Some("secret-assignment")
}

fn looks_like_placeholder(value: &str) -> bool {
    let markers = [
        "example",
        "changeme",
        "replace_me",
        "your_",
        "dummy",
        "sample",
        "placeholder",
        "test",
        "${",
        "$(",
    ];
    markers.iter().any(|marker| value.contains(marker))
}

fn trimmed_snippet(line: &str) -> String {
    const MAX_LEN: usize = 120;
    let trimmed = line.trim();
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..MAX_LEN])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detects_secret_assignment() {
        let dir = tempdir().expect("tempdir should be created");
        fs::write(
            dir.path().join("devsync.lock"),
            r#"api_key = "abcd1234secretvalue""#,
        )
        .expect("lock should be written");

        let report = run_secret_lint(dir.path()).expect("lint should run");
        assert!(!report.passed);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.rule == "secret-assignment")
        );
    }

    #[test]
    fn ignores_placeholder_values() {
        let dir = tempdir().expect("tempdir should be created");
        fs::write(
            dir.path().join("devsync.lock"),
            r#"token = "your_token_here""#,
        )
        .expect("lock should be written");

        let report = run_secret_lint(dir.path()).expect("lint should run");
        assert!(report.passed);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn detects_known_token_prefixes() {
        let dir = tempdir().expect("tempdir should be created");
        let token = "AKIA1234567890ABCD12";
        fs::create_dir_all(dir.path().join(".devcontainer"))
            .expect("devcontainer dir should be created");
        fs::write(
            dir.path().join(".devcontainer").join("Dockerfile"),
            format!("FROM ubuntu:24.04\nENV AWS_ACCESS_KEY_ID={token}\n"),
        )
        .expect("dockerfile should be written");

        let report = run_secret_lint(dir.path()).expect("lint should run");
        assert!(!report.passed);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.rule == "aws-access-key")
        );
    }
}

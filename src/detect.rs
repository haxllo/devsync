use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Detection {
    pub project_name: String,
    pub project_root: PathBuf,
    pub detected_stacks: Vec<String>,
    pub node_version: Option<String>,
    pub python_version: Option<String>,
    pub rust_toolchain: Option<String>,
    pub node_package_manager: Option<String>,
    pub python_package_manager: Option<String>,
    pub services: Vec<String>,
    pub run_hints: Vec<String>,
    pub primary_run_hint: Option<String>,
    pub primary_stack: Option<String>,
    pub recommendations: Vec<String>,
}

pub fn detect_project(root: &Path) -> Result<Detection> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve project path: {}", root.display()))?;

    let project_name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "workspace".to_string());

    let has_package_json = root.join("package.json").is_file();
    let has_node_workspace = root.join("pnpm-workspace.yaml").is_file();
    let has_node_lockfiles = has_any_node_lockfile(&root);
    let has_pyproject = root.join("pyproject.toml").is_file();
    let has_requirements = has_any_requirements_file(&root);
    let has_rust = root.join("Cargo.toml").is_file();

    let mut detected_stacks = Vec::new();
    if has_package_json || has_node_workspace || has_node_lockfiles {
        detected_stacks.push("node".to_string());
    }
    if has_pyproject || has_requirements || root.join("Pipfile").is_file() {
        detected_stacks.push("python".to_string());
    }
    if has_rust {
        detected_stacks.push("rust".to_string());
    }

    let node_version = detect_node_version(&root)?;
    let python_version = detect_python_version(&root)?;
    let rust_toolchain = detect_rust_toolchain(&root)?;

    let node_package_manager = detect_node_package_manager(&root);
    let python_package_manager = detect_python_package_manager(&root);

    let services = detect_services(&root)?;
    let run_hints = detect_run_hints(&root, node_package_manager.as_deref())?;
    let primary_run_hint = select_primary_run_hint(&detected_stacks, &run_hints);
    let primary_stack = primary_run_hint
        .as_deref()
        .and_then(infer_stack_from_run_hint)
        .or_else(|| {
            if detected_stacks.len() == 1 {
                detected_stacks.first().cloned()
            } else {
                None
            }
        });

    let mut recommendations = Vec::new();
    if detected_stacks.is_empty() {
        recommendations.push(
            "No Node/Python/Rust stack markers found; add manifest files before onboarding teammates."
                .to_string(),
        );
    }

    if detected_stacks.iter().any(|stack| stack == "node") && node_version.is_none() {
        recommendations.push(
            "Pin Node version with .nvmrc or .node-version for deterministic installs.".to_string(),
        );
    }

    if detected_stacks.iter().any(|stack| stack == "python") && python_version.is_none() {
        recommendations.push(
            "Pin Python version with .python-version or pyproject.toml requires-python."
                .to_string(),
        );
    }

    if detected_stacks.iter().any(|stack| stack == "rust") && rust_toolchain.is_none() {
        recommendations.push(
            "Pin Rust toolchain via rust-toolchain.toml for reproducible builds.".to_string(),
        );
    }

    if detected_stacks.iter().any(|stack| stack == "node") && node_package_manager.is_none() {
        recommendations
            .push("Add a lockfile (pnpm-lock.yaml/yarn.lock/package-lock.json).".to_string());
    }

    if detected_stacks.iter().any(|stack| stack == "python") && python_package_manager.is_none() {
        recommendations
            .push("Add uv.lock/poetry.lock/requirements*.txt to lock dependencies.".to_string());
    }

    Ok(Detection {
        project_name,
        project_root: root,
        detected_stacks,
        node_version,
        python_version,
        rust_toolchain,
        node_package_manager,
        python_package_manager,
        services,
        run_hints,
        primary_run_hint,
        primary_stack,
        recommendations,
    })
}

fn has_any_requirements_file(root: &Path) -> bool {
    [
        "requirements.txt",
        "requirements-dev.txt",
        "requirements.prod.txt",
    ]
    .iter()
    .any(|file| root.join(file).is_file())
}

fn has_any_node_lockfile(root: &Path) -> bool {
    [
        "pnpm-lock.yaml",
        "yarn.lock",
        "package-lock.json",
        "bun.lock",
        "bun.lockb",
    ]
    .iter()
    .any(|file| root.join(file).is_file())
}

fn detect_node_version(root: &Path) -> Result<Option<String>> {
    for file in [".nvmrc", ".node-version"] {
        let path = root.join(file);
        if path.is_file() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let value = raw.trim();
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
    }

    let package_json = root.join("package.json");
    if package_json.is_file() {
        let raw = fs::read_to_string(&package_json)
            .with_context(|| format!("failed to read {}", package_json.display()))?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", package_json.display()))?;

        if let Some(engine_node) = parsed
            .get("engines")
            .and_then(|engines| engines.get("node"))
            .and_then(|node| node.as_str())
        {
            let cleaned = engine_node.trim();
            if !cleaned.is_empty() {
                return Ok(Some(cleaned.to_string()));
            }
        }
    }

    Ok(None)
}

fn detect_python_version(root: &Path) -> Result<Option<String>> {
    let pyver = root.join(".python-version");
    if pyver.is_file() {
        let raw = fs::read_to_string(&pyver)
            .with_context(|| format!("failed to read {}", pyver.display()))?;
        let value = raw.trim();
        if !value.is_empty() {
            return Ok(Some(value.to_string()));
        }
    }

    let pyproject = root.join("pyproject.toml");
    if pyproject.is_file() {
        let raw = fs::read_to_string(&pyproject)
            .with_context(|| format!("failed to read {}", pyproject.display()))?;
        let parsed: TomlValue = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", pyproject.display()))?;

        if let Some(value) = parsed
            .get("project")
            .and_then(|project| project.get("requires-python"))
            .and_then(TomlValue::as_str)
        {
            let cleaned = value.trim();
            if !cleaned.is_empty() {
                return Ok(Some(cleaned.to_string()));
            }
        }

        if let Some(value) = parsed
            .get("tool")
            .and_then(|tool| tool.get("poetry"))
            .and_then(|poetry| poetry.get("dependencies"))
            .and_then(|deps| deps.get("python"))
            .and_then(TomlValue::as_str)
        {
            let cleaned = value.trim();
            if !cleaned.is_empty() {
                return Ok(Some(cleaned.to_string()));
            }
        }
    }

    Ok(None)
}

fn detect_rust_toolchain(root: &Path) -> Result<Option<String>> {
    let rust_toolchain_toml = root.join("rust-toolchain.toml");
    if rust_toolchain_toml.is_file() {
        let raw = fs::read_to_string(&rust_toolchain_toml)
            .with_context(|| format!("failed to read {}", rust_toolchain_toml.display()))?;
        let parsed: TomlValue = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", rust_toolchain_toml.display()))?;

        if let Some(channel) = parsed
            .get("toolchain")
            .and_then(|toolchain| toolchain.get("channel"))
            .and_then(TomlValue::as_str)
        {
            let cleaned = channel.trim();
            if !cleaned.is_empty() {
                return Ok(Some(cleaned.to_string()));
            }
        }
    }

    let rust_toolchain = root.join("rust-toolchain");
    if rust_toolchain.is_file() {
        let raw = fs::read_to_string(&rust_toolchain)
            .with_context(|| format!("failed to read {}", rust_toolchain.display()))?;
        let first_line = raw
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'));
        if let Some(value) = first_line {
            return Ok(Some(value.to_string()));
        }
    }

    Ok(None)
}

fn detect_node_package_manager(root: &Path) -> Option<String> {
    let mappings = [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("package-lock.json", "npm"),
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
    ];

    for (file, pm) in mappings {
        if root.join(file).is_file() {
            return Some(pm.to_string());
        }
    }

    let package_json_path = root.join("package.json");
    if package_json_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&package_json_path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(pm) = parsed
                    .get("packageManager")
                    .and_then(|value| value.as_str())
                {
                    let value = pm.trim();
                    if !value.is_empty() {
                        let normalized = value
                            .split('@')
                            .next()
                            .map(str::trim)
                            .filter(|token| !token.is_empty())
                            .unwrap_or(value);
                        return Some(normalized.to_string());
                    }
                }
            }
        }
    }

    if root.join("package.json").is_file() {
        Some("npm".to_string())
    } else {
        None
    }
}

fn detect_python_package_manager(root: &Path) -> Option<String> {
    let mappings = [
        ("uv.lock", "uv"),
        ("poetry.lock", "poetry"),
        ("Pipfile.lock", "pipenv"),
        ("Pipfile", "pipenv"),
        ("requirements.txt", "pip"),
    ];

    for (file, pm) in mappings {
        if root.join(file).is_file() {
            return Some(pm.to_string());
        }
    }

    let pyproject = root.join("pyproject.toml");
    if pyproject.is_file() {
        if let Ok(raw) = fs::read_to_string(&pyproject) {
            if let Ok(parsed) = toml::from_str::<TomlValue>(&raw) {
                if parsed
                    .get("tool")
                    .and_then(|tool| tool.get("poetry"))
                    .is_some()
                {
                    return Some("poetry".to_string());
                }
                if parsed.get("tool").and_then(|tool| tool.get("uv")).is_some() {
                    return Some("uv".to_string());
                }
            }
        }

        Some("pip".to_string())
    } else {
        None
    }
}

fn detect_services(root: &Path) -> Result<Vec<String>> {
    let compose_candidates = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];

    let mut services = Vec::new();

    for file in compose_candidates {
        let path = root.join(file);
        if !path.is_file() {
            continue;
        }

        push_unique(&mut services, "compose");

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .to_lowercase();

        if content.contains("postgres") {
            push_unique(&mut services, "postgres");
        }
        if content.contains("mysql") || content.contains("mariadb") {
            push_unique(&mut services, "mysql");
        }
        if content.contains("redis") {
            push_unique(&mut services, "redis");
        }
        if content.contains("mongo") {
            push_unique(&mut services, "mongodb");
        }
    }

    Ok(services)
}

fn detect_run_hints(root: &Path, node_package_manager: Option<&str>) -> Result<Vec<String>> {
    let mut hints = Vec::new();

    if let Some(hint) = detect_rust_run_hint(root)? {
        hints.push(hint);
    }
    if let Some(hint) = detect_node_run_hint(root, node_package_manager)? {
        hints.push(hint);
    }

    Ok(hints)
}

fn detect_node_run_hint(root: &Path, node_package_manager: Option<&str>) -> Result<Option<String>> {
    let package_json = root.join("package.json");
    if !package_json.is_file() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&package_json)
        .with_context(|| format!("failed to read {}", package_json.display()))?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", package_json.display()))?;
    let Some(scripts) = parsed.get("scripts").and_then(|value| value.as_object()) else {
        return Ok(None);
    };

    let preferred = ["dev", "start", "serve"];
    let script_name = preferred
        .iter()
        .find(|name| scripts.get(**name).is_some())
        .copied();

    let Some(script_name) = script_name else {
        return Ok(None);
    };

    let pm = node_package_manager.unwrap_or("npm");
    let hint = match pm {
        "pnpm" => format!("pnpm {script_name}"),
        "yarn" => format!("yarn {script_name}"),
        "bun" => format!("bun run {script_name}"),
        _ => format!("npm run {script_name}"),
    };

    Ok(Some(hint))
}

fn select_primary_run_hint(detected_stacks: &[String], run_hints: &[String]) -> Option<String> {
    let has_rust = detected_stacks.iter().any(|stack| stack == "rust");
    if has_rust {
        if let Some(rust_hint) = run_hints
            .iter()
            .find(|hint| hint.starts_with("cargo run"))
            .cloned()
        {
            return Some(rust_hint);
        }
    }

    let has_python = detected_stacks.iter().any(|stack| stack == "python");
    if has_python {
        if let Some(py_hint) = run_hints
            .iter()
            .find(|hint| hint.contains("python") || hint.contains("uv run"))
            .cloned()
        {
            return Some(py_hint);
        }
    }

    let has_node = detected_stacks.iter().any(|stack| stack == "node");
    if has_node {
        if let Some(node_hint) = run_hints
            .iter()
            .find(|hint| {
                hint.starts_with("pnpm ")
                    || hint.starts_with("npm ")
                    || hint.starts_with("yarn ")
                    || hint.starts_with("bun ")
            })
            .cloned()
        {
            return Some(node_hint);
        }
    }

    run_hints.first().cloned()
}

fn infer_stack_from_run_hint(hint: &str) -> Option<String> {
    let normalized = hint.trim().to_lowercase();
    if normalized.starts_with("cargo run") {
        return Some("rust".to_string());
    }
    if normalized.starts_with("pnpm ")
        || normalized.starts_with("npm ")
        || normalized.starts_with("yarn ")
        || normalized.starts_with("bun ")
    {
        return Some("node".to_string());
    }
    if normalized.contains("python") || normalized.starts_with("uv run") {
        return Some("python".to_string());
    }
    None
}

fn detect_rust_run_hint(root: &Path) -> Result<Option<String>> {
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;
    let parsed: TomlValue = toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", cargo_toml.display()))?;

    if parsed.get("package").is_some() {
        return Ok(Some("cargo run".to_string()));
    }

    let maybe_members = parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(TomlValue::as_array);

    let Some(members) = maybe_members else {
        return Ok(None);
    };

    if members.len() != 1 {
        return Ok(None);
    }

    let member = members[0].as_str().map(str::trim).unwrap_or("");
    if member.is_empty() {
        return Ok(None);
    }

    let member_cargo = root.join(member).join("Cargo.toml");
    if !member_cargo.is_file() {
        return Ok(None);
    }

    let member_raw = fs::read_to_string(&member_cargo)
        .with_context(|| format!("failed to read {}", member_cargo.display()))?;
    let member_parsed: TomlValue = toml::from_str(&member_raw)
        .with_context(|| format!("failed to parse {}", member_cargo.display()))?;

    let package_name = member_parsed
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(TomlValue::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());

    let Some(package_name) = package_name else {
        return Ok(None);
    };

    Ok(Some(format!("cargo run -p {package_name}")))
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn detects_node_and_python_stack() {
        let dir = tempdir().expect("tempdir should be created");
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            "{\"name\":\"sample\",\"engines\":{\"node\":\">=20\"}}",
        )
        .expect("package.json should be written");
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'")
            .expect("pnpm lock should be written");
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nname='sample'\nrequires-python='>=3.11'\n",
        )
        .expect("pyproject should be written");

        let detection = detect_project(root).expect("detection should succeed");

        assert!(detection.detected_stacks.contains(&"node".to_string()));
        assert!(detection.detected_stacks.contains(&"python".to_string()));
        assert_eq!(detection.node_version.as_deref(), Some(">=20"));
        assert_eq!(detection.python_version.as_deref(), Some(">=3.11"));
        assert_eq!(detection.node_package_manager.as_deref(), Some("pnpm"));
        assert_eq!(detection.python_package_manager.as_deref(), Some("pip"));
        assert_eq!(detection.primary_run_hint, None);
        assert_eq!(detection.primary_stack, None);
    }

    #[test]
    fn detects_compose_services() {
        let dir = tempdir().expect("tempdir should be created");
        let root = dir.path();

        fs::write(
            root.join("docker-compose.yml"),
            "services:\n  db:\n    image: postgres:16\n  cache:\n    image: redis:7\n",
        )
        .expect("compose file should be written");

        let detection = detect_project(root).expect("detection should succeed");

        assert!(detection.services.contains(&"compose".to_string()));
        assert!(detection.services.contains(&"postgres".to_string()));
        assert!(detection.services.contains(&"redis".to_string()));
    }

    #[test]
    fn detects_package_manager_from_package_json_field() {
        let dir = tempdir().expect("tempdir should be created");
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            "{\"name\":\"sample\",\"packageManager\":\"pnpm@9.2.0\"}",
        )
        .expect("package.json should be written");

        let detection = detect_project(root).expect("detection should succeed");
        assert_eq!(detection.node_package_manager.as_deref(), Some("pnpm"));
    }

    #[test]
    fn validates_phase1_fixture_matrix() {
        struct FixtureCase<'a> {
            name: &'a str,
            stacks: &'a [&'a str],
            node_pm: Option<&'a str>,
            python_pm: Option<&'a str>,
            services: &'a [&'a str],
        }

        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let cases = [
            FixtureCase {
                name: "node-npm",
                stacks: &["node"],
                node_pm: Some("npm"),
                python_pm: None,
                services: &[],
            },
            FixtureCase {
                name: "node-pnpm-workspace",
                stacks: &["node"],
                node_pm: Some("pnpm"),
                python_pm: None,
                services: &[],
            },
            FixtureCase {
                name: "node-yarn",
                stacks: &["node"],
                node_pm: Some("yarn"),
                python_pm: None,
                services: &[],
            },
            FixtureCase {
                name: "python-uv",
                stacks: &["python"],
                node_pm: None,
                python_pm: Some("uv"),
                services: &[],
            },
            FixtureCase {
                name: "python-poetry",
                stacks: &["python"],
                node_pm: None,
                python_pm: Some("poetry"),
                services: &[],
            },
            FixtureCase {
                name: "python-pipenv",
                stacks: &["python"],
                node_pm: None,
                python_pm: Some("pipenv"),
                services: &[],
            },
            FixtureCase {
                name: "rust-toolchain-toml",
                stacks: &["rust"],
                node_pm: None,
                python_pm: None,
                services: &[],
            },
            FixtureCase {
                name: "rust-toolchain-file",
                stacks: &["rust"],
                node_pm: None,
                python_pm: None,
                services: &[],
            },
            FixtureCase {
                name: "fullstack-next-postgres",
                stacks: &["node"],
                node_pm: Some("pnpm"),
                python_pm: None,
                services: &["compose", "postgres", "redis"],
            },
            FixtureCase {
                name: "polyglot-monorepo",
                stacks: &["node", "python", "rust"],
                node_pm: Some("npm"),
                python_pm: Some("pip"),
                services: &["compose", "mysql", "mongodb"],
            },
        ];

        for case in cases {
            let root = fixture_root.join(case.name);
            let detection = detect_project(&root).unwrap_or_else(|err| {
                panic!(
                    "fixture {} should be detected successfully: {err}",
                    case.name
                )
            });

            for expected_stack in case.stacks {
                assert!(
                    detection
                        .detected_stacks
                        .iter()
                        .any(|stack| stack == expected_stack),
                    "fixture {} missing stack {}",
                    case.name,
                    expected_stack
                );
            }

            assert_eq!(
                detection.node_package_manager.as_deref(),
                case.node_pm,
                "fixture {} node package manager mismatch",
                case.name
            );
            assert_eq!(
                detection.python_package_manager.as_deref(),
                case.python_pm,
                "fixture {} python package manager mismatch",
                case.name
            );

            for service in case.services {
                assert!(
                    detection
                        .services
                        .iter()
                        .any(|detected| detected == service),
                    "fixture {} missing service {}",
                    case.name,
                    service
                );
            }
        }
    }

    #[test]
    fn detects_rust_workspace_run_hint() {
        let dir = tempdir().expect("tempdir should be created");
        let root = dir.path();
        let member_dir = root.join("apps/core");
        fs::create_dir_all(&member_dir).expect("member dir should be created");

        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/core\"]\nresolver = \"2\"\n",
        )
        .expect("workspace Cargo.toml should be written");
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"swiftfind-core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("member Cargo.toml should be written");

        let detection = detect_project(root).expect("detection should succeed");
        assert!(
            detection
                .run_hints
                .iter()
                .any(|hint| hint == "cargo run -p swiftfind-core")
        );
        assert_eq!(
            detection.primary_run_hint.as_deref(),
            Some("cargo run -p swiftfind-core")
        );
        assert_eq!(detection.primary_stack.as_deref(), Some("rust"));
    }

    #[test]
    fn detects_node_primary_run_hint_from_scripts() {
        let dir = tempdir().expect("tempdir should be created");
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            "{\"name\":\"web\",\"packageManager\":\"pnpm@9\",\"scripts\":{\"dev\":\"vite\"}}",
        )
        .expect("package.json should be written");
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'")
            .expect("pnpm lock should be written");

        let detection = detect_project(root).expect("detection should succeed");
        assert_eq!(detection.primary_run_hint.as_deref(), Some("pnpm dev"));
        assert_eq!(detection.primary_stack.as_deref(), Some("node"));
    }
}

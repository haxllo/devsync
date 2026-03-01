use crate::devcontainer;
use crate::lockfile::{DevsyncLock, read_lock, write_lock};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleBinding {
    pub subject: String,
    pub role: RegistryRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RegistryRole {
    Viewer,
    Member,
    Admin,
}

impl RegistryRole {
    fn as_str(&self) -> &'static str {
        match self {
            RegistryRole::Viewer => "viewer",
            RegistryRole::Member => "member",
            RegistryRole::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub org: String,
    pub project: String,
    pub latest: Option<String>,
    #[serde(default)]
    pub versions: Vec<VersionMeta>,
    #[serde(default)]
    pub role_bindings: Vec<RoleBinding>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    pub version: String,
    pub created_at: String,
    pub created_by: String,
    pub prebuild_cache: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub schema_version: u32,
    pub org: String,
    pub project: String,
    pub version: String,
    pub created_at: String,
    pub created_by: String,
    pub prebuild_cache: Option<String>,
    #[serde(default)]
    pub role_bindings: Vec<RoleBinding>,
    pub lock: DevsyncLock,
}

#[derive(Debug, Clone)]
pub struct RegistryTarget {
    pub org: String,
    pub project: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct RegistryProjectRef {
    pub org: String,
    pub project: String,
}

#[derive(Debug, Clone)]
pub struct PushOptions {
    pub registry_root: Option<PathBuf>,
    pub actor: Option<String>,
    pub grants: Vec<String>,
    pub prebuild_cache: Option<String>,
    pub auth_token: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct PullOptions {
    pub registry_root: Option<PathBuf>,
    pub actor: Option<String>,
    pub force: bool,
    pub with_devcontainer: bool,
    pub primary_only: bool,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub registry_root: Option<PathBuf>,
    pub actor: Option<String>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditListOptions {
    pub registry_root: Option<PathBuf>,
    pub actor: Option<String>,
    pub auth_token: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    pub org: String,
    pub project: String,
    pub latest: Option<String>,
    pub versions: Vec<VersionMeta>,
    pub role_bindings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PushResult {
    pub org: String,
    pub project: String,
    pub version: String,
    pub path: PathBuf,
    pub prebuild_cache: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PullResult {
    pub org: String,
    pub project: String,
    pub version: String,
    pub lockfile_path: PathBuf,
    pub prebuild_cache: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub registry_root: Option<PathBuf>,
    pub bind: String,
    pub auth_token: Option<String>,
    pub once: bool,
}

#[derive(Debug, Clone)]
pub struct ServeResult {
    pub bind: String,
    pub registry_root: PathBuf,
    pub requests_handled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub schema_version: u32,
    pub occurred_at: String,
    pub actor: String,
    pub action: String,
    pub org: String,
    pub project: String,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemotePushRequest {
    target: String,
    actor: Option<String>,
    grants: Vec<String>,
    prebuild_cache: Option<String>,
    force: bool,
    lock: DevsyncLock,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemotePushResponse {
    org: String,
    project: String,
    version: String,
    prebuild_cache: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemotePullRequest {
    target: String,
    actor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemotePullResponse {
    org: String,
    project: String,
    version: String,
    prebuild_cache: Option<String>,
    lock: DevsyncLock,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemoteListRequest {
    project: String,
    actor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemoteAuditRequest {
    project: String,
    actor: Option<String>,
    limit: usize,
}

pub fn parse_target(raw: &str) -> Result<RegistryTarget> {
    let (project_ref, version) = raw
        .rsplit_once('@')
        .ok_or_else(|| anyhow::anyhow!("target must be in org/project@version format"))?;

    let project = parse_project_ref(project_ref)?;
    let version = version.trim();
    if version.is_empty() {
        bail!("target version cannot be empty")
    }

    Ok(RegistryTarget {
        org: project.org,
        project: project.project,
        version: version.to_string(),
    })
}

pub fn parse_project_ref(raw: &str) -> Result<RegistryProjectRef> {
    let (org, project) = raw
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("project must be in org/project format"))?;

    let org = org.trim();
    let project = project.trim();

    if org.is_empty() || project.is_empty() {
        bail!("org and project must be non-empty")
    }

    Ok(RegistryProjectRef {
        org: org.to_string(),
        project: project.to_string(),
    })
}

fn push_lock_to_registry(
    lock: DevsyncLock,
    target: &RegistryTarget,
    options: PushOptions,
) -> Result<PushResult> {
    let registry_root = resolve_registry_root(options.registry_root)?;
    let actor = resolve_actor(options.actor);
    let project_dir = project_dir(&registry_root, &target.org, &target.project);
    let versions_dir = project_dir.join("versions");
    let index_path = project_dir.join("index.toml");
    let version_path = versions_dir.join(format!("{}.toml", target.version));

    fs::create_dir_all(&versions_dir)
        .with_context(|| format!("failed to create {}", versions_dir.display()))?;

    let parsed_grants = parse_grants(&options.grants)?;
    let now = Utc::now().to_rfc3339();
    let version_exists = version_path.is_file();

    let mut index = if index_path.is_file() {
        let mut existing = read_index(&index_path)?;

        let actor_role = find_role(&existing.role_bindings, &actor);
        if !can_push(actor_role.as_ref()) {
            bail!(
                "actor `{}` does not have push permission for {}/{}",
                actor,
                target.org,
                target.project
            );
        }

        if !parsed_grants.is_empty() {
            if !can_manage_roles(actor_role.as_ref()) {
                bail!("actor `{}` is not allowed to modify role bindings", actor);
            }
            merge_bindings(&mut existing.role_bindings, parsed_grants);
        }

        existing
    } else {
        let mut initial_bindings = parsed_grants;
        if find_role(&initial_bindings, &actor).is_none() {
            initial_bindings.push(RoleBinding {
                subject: actor.clone(),
                role: RegistryRole::Admin,
            });
        }

        RegistryIndex {
            schema_version: 1,
            org: target.org.clone(),
            project: target.project.clone(),
            latest: None,
            versions: Vec::new(),
            role_bindings: initial_bindings,
            updated_at: now.clone(),
        }
    };

    if version_path.is_file() && !options.force {
        bail!(
            "version {} already exists at {}. Use --force to overwrite",
            target.version,
            version_path.display()
        );
    }

    let entry = RegistryEntry {
        schema_version: 1,
        org: target.org.clone(),
        project: target.project.clone(),
        version: target.version.clone(),
        created_at: now.clone(),
        created_by: actor.clone(),
        prebuild_cache: options.prebuild_cache.clone(),
        role_bindings: index.role_bindings.clone(),
        lock,
    };

    write_entry(&version_path, &entry)?;

    upsert_version_meta(
        &mut index.versions,
        VersionMeta {
            version: target.version.clone(),
            created_at: now.clone(),
            created_by: actor.clone(),
            prebuild_cache: options.prebuild_cache.clone(),
        },
    );

    index.latest = Some(target.version.clone());
    index.updated_at = now;
    write_index(&index_path, &index)?;

    append_audit_event(
        &registry_root,
        AuditEvent {
            schema_version: 1,
            occurred_at: Utc::now().to_rfc3339(),
            actor: actor.clone(),
            action: if version_exists {
                "environment.update".to_string()
            } else {
                "environment.create".to_string()
            },
            org: target.org.clone(),
            project: target.project.clone(),
            version: Some(target.version.clone()),
        },
    )?;

    Ok(PushResult {
        org: target.org.clone(),
        project: target.project.clone(),
        version: target.version.clone(),
        path: version_path,
        prebuild_cache: options.prebuild_cache,
    })
}

fn fetch_registry_entry(target: &RegistryTarget, options: PullOptions) -> Result<RegistryEntry> {
    let registry_root = resolve_registry_root(options.registry_root)?;
    let actor = resolve_actor(options.actor);
    let project_dir = project_dir(&registry_root, &target.org, &target.project);
    let index_path = project_dir.join("index.toml");

    if !index_path.is_file() {
        bail!(
            "registry project {}/{} not found",
            target.org,
            target.project
        );
    }

    let index = read_index(&index_path)?;
    let actor_role = find_role(&index.role_bindings, &actor);
    if !can_pull(actor_role.as_ref()) {
        bail!(
            "actor `{}` does not have pull permission for {}/{}",
            actor,
            target.org,
            target.project
        );
    }

    let resolved_version = if target.version == "latest" {
        index.latest.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "project {}/{} has no published versions",
                target.org,
                target.project
            )
        })?
    } else {
        target.version.clone()
    };

    let version_path = project_dir
        .join("versions")
        .join(format!("{}.toml", resolved_version));

    if !version_path.is_file() {
        bail!(
            "version {} for {}/{} not found",
            resolved_version,
            target.org,
            target.project
        );
    }

    let entry = read_entry(&version_path)?;
    append_audit_event(
        &registry_root,
        AuditEvent {
            schema_version: 1,
            occurred_at: Utc::now().to_rfc3339(),
            actor,
            action: "environment.use".to_string(),
            org: target.org.clone(),
            project: target.project.clone(),
            version: Some(entry.version.clone()),
        },
    )?;

    Ok(entry)
}

pub fn push_environment(
    project_path: &Path,
    target: &RegistryTarget,
    options: PushOptions,
) -> Result<PushResult> {
    let lock_path = project_path.join("devsync.lock");
    let lock = read_lock(&lock_path).with_context(|| {
        format!(
            "failed to load {}. Run `devsync init` first.",
            lock_path.display()
        )
    })?;

    push_lock_to_registry(lock, target, options)
}

pub fn pull_environment(
    project_path: &Path,
    target: &RegistryTarget,
    options: PullOptions,
) -> Result<PullResult> {
    let entry = fetch_registry_entry(target, options.clone())?;
    let lock_path = project_path.join("devsync.lock");

    write_lock(&lock_path, &entry.lock, options.force)?;

    if options.with_devcontainer {
        devcontainer::generate_devcontainer(
            project_path,
            &entry.lock,
            options.force,
            options.primary_only,
        )?;
    }

    Ok(PullResult {
        org: target.org.clone(),
        project: target.project.clone(),
        version: entry.version.clone(),
        lockfile_path: lock_path,
        prebuild_cache: entry.prebuild_cache,
    })
}

pub fn list_versions(project: &RegistryProjectRef, options: ListOptions) -> Result<ListResult> {
    let registry_root = resolve_registry_root(options.registry_root)?;
    let actor = resolve_actor(options.actor);
    let project_dir = project_dir(&registry_root, &project.org, &project.project);
    let index_path = project_dir.join("index.toml");

    if !index_path.is_file() {
        bail!(
            "registry project {}/{} not found",
            project.org,
            project.project
        );
    }

    let index = read_index(&index_path)?;
    let actor_role = find_role(&index.role_bindings, &actor);
    if !can_pull(actor_role.as_ref()) {
        bail!(
            "actor `{}` does not have list permission for {}/{}",
            actor,
            project.org,
            project.project
        );
    }

    append_audit_event(
        &registry_root,
        AuditEvent {
            schema_version: 1,
            occurred_at: Utc::now().to_rfc3339(),
            actor,
            action: "environment.list".to_string(),
            org: project.org.clone(),
            project: project.project.clone(),
            version: None,
        },
    )?;

    Ok(ListResult {
        org: index.org,
        project: index.project,
        latest: index.latest,
        versions: index.versions,
        role_bindings: render_bindings(&index.role_bindings),
    })
}

pub fn list_audit_events(
    project: &RegistryProjectRef,
    options: AuditListOptions,
) -> Result<Vec<AuditEvent>> {
    let registry_root = resolve_registry_root(options.registry_root)?;
    let actor = resolve_actor(options.actor);
    let project_dir = project_dir(&registry_root, &project.org, &project.project);
    let index_path = project_dir.join("index.toml");
    let audit_path = registry_root.join("audit.log");

    if !index_path.is_file() {
        bail!(
            "registry project {}/{} not found",
            project.org,
            project.project
        );
    }

    let index = read_index(&index_path)?;
    let actor_role = find_role(&index.role_bindings, &actor);
    if !can_manage_roles(actor_role.as_ref()) {
        bail!(
            "actor `{}` does not have audit permission for {}/{}",
            actor,
            project.org,
            project.project
        );
    }

    if !audit_path.is_file() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&audit_path)
        .with_context(|| format!("failed to read audit log {}", audit_path.display()))?;
    let mut events = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let event: AuditEvent = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "failed to parse audit event on line {} in {}",
                index + 1,
                audit_path.display()
            )
        })?;

        if event.org == project.org && event.project == project.project {
            events.push(event);
        }
    }

    events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
    let limit = options.limit.max(1);
    events.truncate(limit);
    Ok(events)
}

pub fn push_environment_remote(
    project_path: &Path,
    target: &RegistryTarget,
    registry_url: &str,
    options: PushOptions,
) -> Result<PushResult> {
    let auth_token = resolve_auth_token(options.auth_token.clone());
    let lock_path = project_path.join("devsync.lock");
    let lock = read_lock(&lock_path).with_context(|| {
        format!(
            "failed to load {}. Run `devsync init` first.",
            lock_path.display()
        )
    })?;

    let request = RemotePushRequest {
        target: format!("{}/{}@{}", target.org, target.project, target.version),
        actor: options.actor,
        grants: options.grants,
        prebuild_cache: options.prebuild_cache,
        force: options.force,
        lock,
    };

    let response: RemotePushResponse = post_json(registry_url, "/v1/push", &request, auth_token)?;
    Ok(PushResult {
        org: response.org,
        project: response.project,
        version: response.version,
        path: PathBuf::from("<remote>"),
        prebuild_cache: response.prebuild_cache,
    })
}

pub fn pull_environment_remote(
    project_path: &Path,
    target: &RegistryTarget,
    registry_url: &str,
    options: PullOptions,
) -> Result<PullResult> {
    let auth_token = resolve_auth_token(options.auth_token.clone());
    let request = RemotePullRequest {
        target: format!("{}/{}@{}", target.org, target.project, target.version),
        actor: options.actor,
    };
    let response: RemotePullResponse = post_json(registry_url, "/v1/pull", &request, auth_token)?;

    let lock_path = project_path.join("devsync.lock");
    write_lock(&lock_path, &response.lock, options.force)?;

    if options.with_devcontainer {
        devcontainer::generate_devcontainer(
            project_path,
            &response.lock,
            options.force,
            options.primary_only,
        )?;
    }

    Ok(PullResult {
        org: response.org,
        project: response.project,
        version: response.version,
        lockfile_path: lock_path,
        prebuild_cache: response.prebuild_cache,
    })
}

pub fn list_versions_remote(
    project: &RegistryProjectRef,
    registry_url: &str,
    options: ListOptions,
) -> Result<ListResult> {
    let request = RemoteListRequest {
        project: format!("{}/{}", project.org, project.project),
        actor: options.actor,
    };
    post_json(
        registry_url,
        "/v1/list",
        &request,
        resolve_auth_token(options.auth_token),
    )
}

pub fn list_audit_events_remote(
    project: &RegistryProjectRef,
    registry_url: &str,
    options: AuditListOptions,
) -> Result<Vec<AuditEvent>> {
    let request = RemoteAuditRequest {
        project: format!("{}/{}", project.org, project.project),
        actor: options.actor,
        limit: options.limit.max(1),
    };
    post_json(
        registry_url,
        "/v1/audit",
        &request,
        resolve_auth_token(options.auth_token),
    )
}

pub fn serve_registry_http(options: ServeOptions) -> Result<ServeResult> {
    let registry_root = resolve_registry_root(options.registry_root)?;
    fs::create_dir_all(&registry_root)
        .with_context(|| format!("failed to create {}", registry_root.display()))?;
    let auth_token = resolve_auth_token(options.auth_token);

    let listener = TcpListener::bind(&options.bind)
        .with_context(|| format!("failed to bind {}", options.bind))?;
    let mut requests_handled = 0usize;

    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept registry connection")?;
        handle_registry_http_connection(&mut stream, &registry_root, auth_token.as_deref())?;
        requests_handled += 1;

        if options.once {
            break;
        }
    }

    Ok(ServeResult {
        bind: options.bind,
        registry_root,
        requests_handled,
    })
}

pub fn resolve_registry_root(input: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = input {
        return Ok(path);
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".devsync").join("registry"));
        }
    }

    bail!("failed to determine registry root; pass --registry explicitly")
}

pub fn parse_grants(raw_grants: &[String]) -> Result<Vec<RoleBinding>> {
    let mut bindings = Vec::new();

    for raw in raw_grants {
        let (subject, role_raw) = raw
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("grant must be subject:role"))?;

        let subject = subject.trim();
        if subject.is_empty() {
            bail!("grant subject cannot be empty")
        }

        bindings.push(RoleBinding {
            subject: subject.to_string(),
            role: parse_role(role_raw.trim())?,
        });
    }

    Ok(bindings)
}

pub fn render_bindings(bindings: &[RoleBinding]) -> Vec<String> {
    bindings
        .iter()
        .map(|binding| format!("{}:{}", binding.subject, binding.role.as_str()))
        .collect()
}

fn parse_role(raw: &str) -> Result<RegistryRole> {
    match raw.to_lowercase().as_str() {
        "viewer" => Ok(RegistryRole::Viewer),
        "member" => Ok(RegistryRole::Member),
        "admin" => Ok(RegistryRole::Admin),
        _ => bail!("invalid role `{}`; expected admin/member/viewer", raw),
    }
}

fn find_role(bindings: &[RoleBinding], actor: &str) -> Option<RegistryRole> {
    if bindings.is_empty() {
        return Some(RegistryRole::Admin);
    }

    let explicit = bindings
        .iter()
        .find(|binding| binding.subject == actor)
        .map(|binding| binding.role.clone());
    if explicit.is_some() {
        return explicit;
    }

    bindings
        .iter()
        .find(|binding| binding.subject == "*")
        .map(|binding| binding.role.clone())
}

fn can_pull(role: Option<&RegistryRole>) -> bool {
    role.is_some()
}

fn can_push(role: Option<&RegistryRole>) -> bool {
    matches!(role, Some(RegistryRole::Admin | RegistryRole::Member))
}

fn can_manage_roles(role: Option<&RegistryRole>) -> bool {
    matches!(role, Some(RegistryRole::Admin))
}

fn merge_bindings(existing: &mut Vec<RoleBinding>, incoming: Vec<RoleBinding>) {
    for binding in incoming {
        if let Some(found) = existing
            .iter_mut()
            .find(|candidate| candidate.subject == binding.subject)
        {
            found.role = binding.role;
        } else {
            existing.push(binding);
        }
    }
}

fn upsert_version_meta(versions: &mut Vec<VersionMeta>, meta: VersionMeta) {
    if let Some(existing) = versions
        .iter_mut()
        .find(|candidate| candidate.version == meta.version)
    {
        *existing = meta;
    } else {
        versions.push(meta);
    }
}

fn resolve_actor(input: Option<String>) -> String {
    input
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("USER").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn resolve_auth_token(input: Option<String>) -> Option<String> {
    input
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("DEVSYNC_AUTH_TOKEN").ok())
        .filter(|value| !value.trim().is_empty())
}

fn project_dir(root: &Path, org: &str, project: &str) -> PathBuf {
    root.join(org).join(project)
}

fn read_index(path: &Path) -> Result<RegistryIndex> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read registry index {}", path.display()))?;
    toml::from_str::<RegistryIndex>(&content)
        .with_context(|| format!("failed to parse registry index {}", path.display()))
}

fn write_index(path: &Path, index: &RegistryIndex) -> Result<()> {
    let serialized = toml::to_string_pretty(index).context("failed to serialize registry index")?;
    fs::write(path, serialized)
        .with_context(|| format!("failed to write registry index {}", path.display()))?;
    Ok(())
}

fn read_entry(path: &Path) -> Result<RegistryEntry> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read registry entry {}", path.display()))?;
    toml::from_str::<RegistryEntry>(&content)
        .with_context(|| format!("failed to parse registry entry {}", path.display()))
}

fn write_entry(path: &Path, entry: &RegistryEntry) -> Result<()> {
    let serialized = toml::to_string_pretty(entry).context("failed to serialize registry entry")?;
    fs::write(path, serialized)
        .with_context(|| format!("failed to write registry entry {}", path.display()))?;
    Ok(())
}

fn append_audit_event(registry_root: &Path, event: AuditEvent) -> Result<()> {
    fs::create_dir_all(registry_root)
        .with_context(|| format!("failed to create {}", registry_root.display()))?;
    let audit_path = registry_root.join("audit.log");
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&audit_path)
        .with_context(|| format!("failed to open {}", audit_path.display()))?;
    let line = serde_json::to_string(&event).context("failed to serialize registry audit event")?;
    writeln!(writer, "{line}")
        .with_context(|| format!("failed to write {}", audit_path.display()))?;
    Ok(())
}

fn post_json<TReq, TRes>(
    base_url: &str,
    route: &str,
    payload: &TReq,
    auth_token: Option<String>,
) -> Result<TRes>
where
    TReq: Serialize,
    TRes: for<'de> Deserialize<'de>,
{
    let endpoint = parse_http_endpoint(base_url)?;
    let body = serde_json::to_vec(payload).context("failed to serialize HTTP request payload")?;
    let request_path = format!("{}{}", endpoint.base_path, route);

    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .with_context(|| format!("failed to connect to {}", base_url))?;

    let auth_header = auth_token
        .as_deref()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\n{}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        request_path,
        endpoint.authority,
        auth_header,
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .context("failed to write HTTP request headers")?;
    stream
        .write_all(&body)
        .context("failed to write HTTP request body")?;
    stream.flush().context("failed to flush HTTP request")?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .context("failed to read HTTP response")?;

    let (status_code, response_body) = split_http_response(&response_bytes)?;
    if !(200..300).contains(&status_code) {
        let err_text = String::from_utf8_lossy(response_body);
        bail!(
            "registry HTTP {} returned {}: {}",
            base_url,
            status_code,
            err_text
        );
    }

    serde_json::from_slice(response_body).context("failed to parse HTTP response body")
}

fn handle_registry_http_connection(
    stream: &mut TcpStream,
    registry_root: &Path,
    required_auth_token: Option<&str>,
) -> Result<()> {
    let request = read_http_request(stream)?;
    if let Some(expected) = required_auth_token {
        let provided = request
            .header("authorization")
            .and_then(extract_bearer_token);
        if provided != Some(expected) {
            let response = HttpResponse::unauthorized_json(
                br#"{"error":"unauthorized","message":"missing or invalid bearer token"}"#.to_vec(),
            );
            write_http_response(stream, &response)?;
            return Ok(());
        }
    }

    let response = match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/v1/push") => {
            let payload: RemotePushRequest =
                serde_json::from_slice(&request.body).context("invalid push request payload")?;
            let target = parse_target(&payload.target)?;
            let result = push_lock_to_registry(
                payload.lock,
                &target,
                PushOptions {
                    registry_root: Some(registry_root.to_path_buf()),
                    actor: payload.actor,
                    grants: payload.grants,
                    prebuild_cache: payload.prebuild_cache,
                    auth_token: None,
                    force: payload.force,
                },
            )?;

            let body = serde_json::to_vec(&RemotePushResponse {
                org: result.org,
                project: result.project,
                version: result.version,
                prebuild_cache: result.prebuild_cache,
            })
            .context("failed to serialize push response")?;
            HttpResponse::ok_json(body)
        }
        ("POST", "/v1/pull") => {
            let payload: RemotePullRequest =
                serde_json::from_slice(&request.body).context("invalid pull request payload")?;
            let target = parse_target(&payload.target)?;
            let entry = fetch_registry_entry(
                &target,
                PullOptions {
                    registry_root: Some(registry_root.to_path_buf()),
                    actor: payload.actor,
                    force: true,
                    with_devcontainer: false,
                    primary_only: false,
                    auth_token: None,
                },
            )?;

            let body = serde_json::to_vec(&RemotePullResponse {
                org: entry.org,
                project: entry.project,
                version: entry.version,
                prebuild_cache: entry.prebuild_cache,
                lock: entry.lock,
            })
            .context("failed to serialize pull response")?;
            HttpResponse::ok_json(body)
        }
        ("POST", "/v1/list") => {
            let payload: RemoteListRequest =
                serde_json::from_slice(&request.body).context("invalid list request payload")?;
            let project = parse_project_ref(&payload.project)?;
            let list = list_versions(
                &project,
                ListOptions {
                    registry_root: Some(registry_root.to_path_buf()),
                    actor: payload.actor,
                    auth_token: None,
                },
            )?;
            let body = serde_json::to_vec(&list).context("failed to serialize list response")?;
            HttpResponse::ok_json(body)
        }
        ("POST", "/v1/audit") => {
            let payload: RemoteAuditRequest =
                serde_json::from_slice(&request.body).context("invalid audit request payload")?;
            let project = parse_project_ref(&payload.project)?;
            let events = list_audit_events(
                &project,
                AuditListOptions {
                    registry_root: Some(registry_root.to_path_buf()),
                    actor: payload.actor,
                    auth_token: None,
                    limit: payload.limit.max(1),
                },
            )?;
            let body = serde_json::to_vec(&events).context("failed to serialize audit response")?;
            HttpResponse::ok_json(body)
        }
        _ => HttpResponse::not_found_json(
            br#"{"error":"not found","routes":["POST /v1/push","POST /v1/pull","POST /v1/list","POST /v1/audit"]}"#
                .to_vec(),
        ),
    };

    write_http_response(stream, &response)?;
    Ok(())
}

#[derive(Debug)]
struct HttpEndpoint {
    host: String,
    port: u16,
    authority: String,
    base_path: String,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, key: &str) -> Option<&str> {
        let lowered = key.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(name, _)| name == &lowered)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    status_text: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn ok_json(body: Vec<u8>) -> Self {
        Self {
            status_code: 200,
            status_text: "OK",
            content_type: "application/json",
            body,
        }
    }

    fn not_found_json(body: Vec<u8>) -> Self {
        Self {
            status_code: 404,
            status_text: "Not Found",
            content_type: "application/json",
            body,
        }
    }

    fn unauthorized_json(body: Vec<u8>) -> Self {
        Self {
            status_code: 401,
            status_text: "Unauthorized",
            content_type: "application/json",
            body,
        }
    }
}

fn parse_http_endpoint(raw: &str) -> Result<HttpEndpoint> {
    let url = raw.trim();
    if !url.starts_with("http://") {
        bail!("only http:// registry URLs are supported right now")
    }

    let rest = &url["http://".len()..];
    let (authority_part, path_part) = if let Some(index) = rest.find('/') {
        (&rest[..index], &rest[index..])
    } else {
        (rest, "")
    };

    if authority_part.trim().is_empty() {
        bail!("registry URL host cannot be empty")
    }

    let (host, port) = if let Some((h, p)) = authority_part.rsplit_once(':') {
        let parsed = p
            .parse::<u16>()
            .with_context(|| format!("invalid registry URL port `{}` in `{}`", p, raw))?;
        (h.to_string(), parsed)
    } else {
        (authority_part.to_string(), 80)
    };

    if host.trim().is_empty() {
        bail!("registry URL host cannot be empty")
    }

    let normalized_path = if path_part.is_empty() || path_part == "/" {
        String::new()
    } else {
        path_part.trim_end_matches('/').to_string()
    };

    Ok(HttpEndpoint {
        host,
        port,
        authority: authority_part.to_string(),
        base_path: normalized_path,
    })
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];
    let mut header_end = None;

    while header_end.is_none() {
        let read = stream
            .read(&mut temp)
            .context("failed to read HTTP request headers")?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        header_end = find_header_end(&buffer);
        if buffer.len() > 64 * 1024 {
            bail!("HTTP request headers too large");
        }
    }

    let Some(headers_end) = header_end else {
        bail!("malformed HTTP request: missing header terminator");
    };

    let headers = &buffer[..headers_end];
    let headers_text =
        String::from_utf8(headers.to_vec()).context("request headers are not UTF-8")?;
    let mut lines = headers_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP request line"))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP method"))?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP path"))?
        .to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let header_name = name.trim().to_ascii_lowercase();
        let header_value = value.trim().to_string();
        if header_name == "content-length" {
            content_length = header_value
                .parse::<usize>()
                .context("invalid Content-Length header")?;
        }
        headers.push((header_name, header_value));
    }

    let body_start = headers_end + 4;
    let mut body = if buffer.len() > body_start {
        buffer[body_start..].to_vec()
    } else {
        Vec::new()
    };

    while body.len() < content_length {
        let read = stream
            .read(&mut temp)
            .context("failed to read HTTP request body")?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&temp[..read]);
    }
    body.truncate(content_length);

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(stream: &mut TcpStream, response: &HttpResponse) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        response.status_text,
        response.content_type,
        response.body.len()
    );
    stream
        .write_all(header.as_bytes())
        .context("failed to write HTTP response headers")?;
    stream
        .write_all(&response.body)
        .context("failed to write HTTP response body")?;
    stream.flush().context("failed to flush HTTP response")?;
    Ok(())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn split_http_response(bytes: &[u8]) -> Result<(u16, &[u8])> {
    let Some(header_end) = find_header_end(bytes) else {
        bail!("malformed HTTP response: missing header terminator");
    };

    let header_text = String::from_utf8(bytes[..header_end].to_vec())
        .context("HTTP response headers are not UTF-8")?;
    let status_line = header_text
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status line"))?;
    let mut parts = status_line.split_whitespace();
    let _http_version = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP version in status line"))?;
    let code_text = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing status code in status line"))?;
    let status_code = code_text
        .parse::<u16>()
        .with_context(|| format!("invalid status code `{}`", code_text))?;

    Ok((status_code, &bytes[header_end + 4..]))
}

fn extract_bearer_token(raw: &str) -> Option<&str> {
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{PackageManagerSection, ProjectSection, RuntimeSection};
    use tempfile::tempdir;

    fn sample_lock() -> DevsyncLock {
        DevsyncLock {
            schema_version: 1,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            project: ProjectSection {
                name: "sample".to_string(),
                root: "/tmp/sample".to_string(),
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
            run_hints: vec!["cargo run -p sample".to_string()],
            primary_run_hint: Some("cargo run -p sample".to_string()),
            primary_stack: Some("rust".to_string()),
            recommendations: vec![],
        }
    }

    #[test]
    fn parses_registry_target() {
        let target = parse_target("acme/api@v1").expect("target should parse");
        assert_eq!(target.org, "acme");
        assert_eq!(target.project, "api");
        assert_eq!(target.version, "v1");
    }

    #[test]
    fn push_and_pull_round_trip() {
        let project = tempdir().expect("project tempdir should exist");
        let registry = tempdir().expect("registry tempdir should exist");

        let lock_path = project.path().join("devsync.lock");
        write_lock(&lock_path, &sample_lock(), false).expect("lockfile should be written");

        let target = parse_target("acme/sample@v1").expect("target should parse");
        let push = push_environment(
            project.path(),
            &target,
            PushOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("alice".to_string()),
                grants: vec!["bob:viewer".to_string()],
                prebuild_cache: Some("s3://cache/sample:v1".to_string()),
                auth_token: None,
                force: false,
            },
        )
        .expect("push should succeed");
        assert_eq!(push.version, "v1");

        fs::remove_file(&lock_path).expect("local lockfile should be removed");

        let pull = pull_environment(
            project.path(),
            &parse_target("acme/sample@latest").expect("latest target should parse"),
            PullOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("bob".to_string()),
                force: false,
                with_devcontainer: false,
                primary_only: false,
                auth_token: None,
            },
        )
        .expect("pull should succeed");

        assert_eq!(pull.version, "v1");
        assert!(pull.lockfile_path.is_file());
    }

    #[test]
    fn push_permission_is_enforced() {
        let project = tempdir().expect("project tempdir should exist");
        let registry = tempdir().expect("registry tempdir should exist");

        let lock_path = project.path().join("devsync.lock");
        write_lock(&lock_path, &sample_lock(), false).expect("lockfile should be written");

        push_environment(
            project.path(),
            &parse_target("acme/sample@v1").expect("target should parse"),
            PushOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("alice".to_string()),
                grants: vec!["bob:viewer".to_string()],
                prebuild_cache: None,
                auth_token: None,
                force: false,
            },
        )
        .expect("initial push should succeed");

        let denied = push_environment(
            project.path(),
            &parse_target("acme/sample@v2").expect("target should parse"),
            PushOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("bob".to_string()),
                grants: Vec::new(),
                prebuild_cache: None,
                auth_token: None,
                force: false,
            },
        )
        .expect_err("viewer push should fail");

        assert!(denied.to_string().contains("does not have push permission"));
    }

    #[test]
    fn audit_log_tracks_create_use_and_list_events() {
        let project = tempdir().expect("project tempdir should exist");
        let registry = tempdir().expect("registry tempdir should exist");

        let lock_path = project.path().join("devsync.lock");
        write_lock(&lock_path, &sample_lock(), false).expect("lockfile should be written");

        let target = parse_target("acme/sample@v1").expect("target should parse");
        push_environment(
            project.path(),
            &target,
            PushOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("alice".to_string()),
                grants: vec!["bob:viewer".to_string()],
                prebuild_cache: None,
                auth_token: None,
                force: false,
            },
        )
        .expect("push should succeed");

        fs::remove_file(&lock_path).expect("local lockfile should be removed");
        pull_environment(
            project.path(),
            &parse_target("acme/sample@latest").expect("target should parse"),
            PullOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("bob".to_string()),
                force: false,
                with_devcontainer: false,
                primary_only: false,
                auth_token: None,
            },
        )
        .expect("pull should succeed");

        list_versions(
            &parse_project_ref("acme/sample").expect("project ref should parse"),
            ListOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("bob".to_string()),
                auth_token: None,
            },
        )
        .expect("list should succeed");

        let events = list_audit_events(
            &parse_project_ref("acme/sample").expect("project ref should parse"),
            AuditListOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("alice".to_string()),
                auth_token: None,
                limit: 20,
            },
        )
        .expect("audit list should succeed for admin");

        let actions: Vec<String> = events.into_iter().map(|event| event.action).collect();
        assert!(actions.iter().any(|action| action == "environment.create"));
        assert!(actions.iter().any(|action| action == "environment.use"));
        assert!(actions.iter().any(|action| action == "environment.list"));
    }

    #[test]
    fn audit_log_requires_admin_role() {
        let project = tempdir().expect("project tempdir should exist");
        let registry = tempdir().expect("registry tempdir should exist");

        let lock_path = project.path().join("devsync.lock");
        write_lock(&lock_path, &sample_lock(), false).expect("lockfile should be written");

        push_environment(
            project.path(),
            &parse_target("acme/sample@v1").expect("target should parse"),
            PushOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("alice".to_string()),
                grants: vec!["bob:viewer".to_string()],
                prebuild_cache: None,
                auth_token: None,
                force: false,
            },
        )
        .expect("push should succeed");

        let denied = list_audit_events(
            &parse_project_ref("acme/sample").expect("project ref should parse"),
            AuditListOptions {
                registry_root: Some(registry.path().to_path_buf()),
                actor: Some("bob".to_string()),
                auth_token: None,
                limit: 10,
            },
        )
        .expect_err("viewer should not access audit log");

        assert!(
            denied
                .to_string()
                .contains("does not have audit permission")
        );
    }
}

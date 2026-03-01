use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthStore {
    schema_version: u32,
    #[serde(default)]
    keys: Vec<ApiKeyRecord>,
}

impl Default for AuthStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub subject: String,
    pub service: String,
    pub org: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub token_sha256: String,
    pub rate_limit_per_minute: u32,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiKeySummary {
    pub id: String,
    pub subject: String,
    pub service: String,
    pub org: Option<String>,
    pub scopes: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub note: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct CreateApiKeyInput {
    pub subject: String,
    pub service: String,
    pub org: Option<String>,
    pub scopes: Vec<String>,
    pub ttl_days: Option<i64>,
    pub rate_limit_per_minute: u32,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedApiKey {
    pub id: String,
    pub token: String,
    pub subject: String,
    pub service: String,
    pub org: Option<String>,
    pub scopes: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AccessIdentity {
    pub key_id: String,
    pub subject: String,
    pub org: Option<String>,
    pub rate_limit_per_minute: u32,
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequirement<'a> {
    pub service: &'a str,
    pub scope: &'a str,
    pub target_org: Option<&'a str>,
    pub require_unscoped_key: bool,
}

#[derive(Debug, Clone)]
pub enum AuthDecision {
    Unauthorized(String),
    Forbidden(String),
}

impl AuthDecision {
    pub fn status_code(&self) -> u16 {
        match self {
            AuthDecision::Unauthorized(_) => 401,
            AuthDecision::Forbidden(_) => 403,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            AuthDecision::Unauthorized(message) => message,
            AuthDecision::Forbidden(message) => message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthRuntime {
    store: AuthStore,
}

#[derive(Debug, Default)]
pub struct RateLimiter {
    counters: HashMap<String, (i64, u32)>,
}

impl RateLimiter {
    pub fn allow(&mut self, key: &str, limit_per_minute: u32) -> bool {
        let limit = limit_per_minute.max(1);
        let minute = Utc::now().timestamp() / 60;
        let entry = self
            .counters
            .entry(key.to_string())
            .or_insert((minute, 0_u32));

        if entry.0 != minute {
            *entry = (minute, 0);
        }

        if entry.1 >= limit {
            return false;
        }

        entry.1 += 1;
        true
    }
}

pub fn resolve_auth_store_path(input: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = input {
        return Ok(path);
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".devsync").join("auth_keys.toml"));
        }
    }

    bail!("failed to resolve auth store path; pass --auth-store")
}

pub fn init_runtime(path: &Path) -> Result<AuthRuntime> {
    let store = load_or_init_store(path)?;
    Ok(AuthRuntime { store })
}

pub fn create_api_key(path: &Path, input: CreateApiKeyInput) -> Result<CreatedApiKey> {
    let mut store = load_or_init_store(path)?;

    let subject = input.subject.trim();
    if subject.is_empty() {
        bail!("subject cannot be empty")
    }
    let service = normalize_service(&input.service)?;
    if input.scopes.is_empty() {
        bail!("at least one scope is required")
    }

    let mut scopes = Vec::new();
    for scope in input.scopes {
        let normalized = normalize_scope(scope.trim())?;
        if !scope_allowed_for_service(&normalized, &service) {
            bail!(
                "scope `{}` is not valid for service `{}`",
                normalized,
                service
            )
        }
        if !scopes.contains(&normalized) {
            scopes.push(normalized);
        }
    }

    if input.rate_limit_per_minute == 0 {
        bail!("rate limit must be greater than zero")
    }

    let id = make_key_id(store.keys.len());
    let token = make_token();
    let token_sha256 = sha256_hex(&token);
    let now = Utc::now();
    let normalized_org = input.org.filter(|value| !value.trim().is_empty());
    let expires_at = match input.ttl_days {
        Some(days) if days > 0 => Some((now + Duration::days(days)).to_rfc3339()),
        Some(_) => bail!("ttl-days must be positive"),
        None => None,
    };

    let record = ApiKeyRecord {
        id: id.clone(),
        subject: subject.to_string(),
        service: service.clone(),
        org: normalized_org.clone(),
        scopes: scopes.clone(),
        token_sha256,
        rate_limit_per_minute: input.rate_limit_per_minute,
        created_at: now.to_rfc3339(),
        expires_at: expires_at.clone(),
        revoked_at: None,
        note: input.note.filter(|value| !value.trim().is_empty()),
    };

    store.keys.push(record);
    save_store(path, &store)?;

    Ok(CreatedApiKey {
        id,
        token,
        subject: subject.to_string(),
        service,
        org: normalized_org,
        scopes,
        rate_limit_per_minute: input.rate_limit_per_minute,
        expires_at,
    })
}

pub fn list_api_keys(path: &Path) -> Result<Vec<ApiKeySummary>> {
    let store = load_or_init_store(path)?;
    let now = Utc::now();

    let mut keys: Vec<ApiKeySummary> = store
        .keys
        .iter()
        .map(|record| ApiKeySummary {
            id: record.id.clone(),
            subject: record.subject.clone(),
            service: record.service.clone(),
            org: record.org.clone(),
            scopes: record.scopes.clone(),
            rate_limit_per_minute: record.rate_limit_per_minute,
            created_at: record.created_at.clone(),
            expires_at: record.expires_at.clone(),
            revoked_at: record.revoked_at.clone(),
            note: record.note.clone(),
            active: is_active(record, now),
        })
        .collect();

    keys.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(keys)
}

pub fn revoke_api_key(path: &Path, key_id: &str) -> Result<ApiKeySummary> {
    let mut store = load_or_init_store(path)?;
    let now = Utc::now().to_rfc3339();
    let Some(record) = store.keys.iter_mut().find(|record| record.id == key_id) else {
        bail!("auth key `{}` not found", key_id);
    };
    if record.revoked_at.is_none() {
        record.revoked_at = Some(now);
    }
    let snapshot = record.clone();
    save_store(path, &store)?;

    Ok(ApiKeySummary {
        id: snapshot.id,
        subject: snapshot.subject,
        service: snapshot.service,
        org: snapshot.org,
        scopes: snapshot.scopes,
        rate_limit_per_minute: snapshot.rate_limit_per_minute,
        created_at: snapshot.created_at,
        expires_at: snapshot.expires_at,
        revoked_at: snapshot.revoked_at,
        note: snapshot.note,
        active: false,
    })
}

pub fn authorize(
    runtime: Option<&AuthRuntime>,
    legacy_auth_token: Option<&str>,
    bearer_token: Option<&str>,
    requirement: AuthorizationRequirement<'_>,
) -> std::result::Result<AccessIdentity, AuthDecision> {
    let auth_enabled = runtime.is_some() || legacy_auth_token.is_some();
    if !auth_enabled {
        return Ok(AccessIdentity {
            key_id: "anonymous".to_string(),
            subject: "anonymous".to_string(),
            org: None,
            rate_limit_per_minute: 120,
        });
    }

    let Some(token) = bearer_token else {
        return Err(AuthDecision::Unauthorized(
            "missing bearer token".to_string(),
        ));
    };

    if let Some(expected) = legacy_auth_token {
        if token == expected {
            return Ok(AccessIdentity {
                key_id: "legacy-token".to_string(),
                subject: "legacy".to_string(),
                org: None,
                rate_limit_per_minute: 120,
            });
        }
    }

    let Some(runtime) = runtime else {
        return Err(AuthDecision::Unauthorized(
            "invalid bearer token".to_string(),
        ));
    };

    let token_sha256 = sha256_hex(token);
    let now = Utc::now();
    let Some(record) = runtime
        .store
        .keys
        .iter()
        .find(|record| record.token_sha256 == token_sha256)
    else {
        return Err(AuthDecision::Unauthorized(
            "invalid bearer token".to_string(),
        ));
    };

    if !is_active(record, now) {
        return Err(AuthDecision::Unauthorized(
            "auth key is revoked or expired".to_string(),
        ));
    }

    if !service_matches(&record.service, requirement.service) {
        return Err(AuthDecision::Forbidden(format!(
            "auth key is not allowed for service `{}`",
            requirement.service
        )));
    }

    if !scope_granted(&record.scopes, requirement.scope) {
        return Err(AuthDecision::Forbidden(format!(
            "missing required scope `{}`",
            requirement.scope
        )));
    }

    if requirement.require_unscoped_key && record.org.is_some() {
        return Err(AuthDecision::Forbidden(
            "route requires an unscoped key".to_string(),
        ));
    }

    if let (Some(required_org), Some(bound_org)) = (requirement.target_org, record.org.as_deref()) {
        if required_org != bound_org {
            return Err(AuthDecision::Forbidden(format!(
                "auth key is scoped to org `{}`",
                bound_org
            )));
        }
    }

    Ok(AccessIdentity {
        key_id: record.id.clone(),
        subject: record.subject.clone(),
        org: record.org.clone(),
        rate_limit_per_minute: record.rate_limit_per_minute.max(1),
    })
}

pub fn extract_bearer_token(raw: &str) -> Option<&str> {
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn load_or_init_store(path: &Path) -> Result<AuthStore> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    if !path.is_file() {
        let store = AuthStore::default();
        save_store(path, &store)?;
        return Ok(store);
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read auth store {}", path.display()))?;
    let store: AuthStore = toml::from_str(&raw)
        .with_context(|| format!("failed to parse auth store {}", path.display()))?;

    if store.schema_version != 1 {
        bail!(
            "auth store schema {} is not supported",
            store.schema_version
        )
    }

    Ok(store)
}

fn save_store(path: &Path, store: &AuthStore) -> Result<()> {
    let serialized = toml::to_string_pretty(store).context("failed to serialize auth store")?;
    fs::write(path, serialized)
        .with_context(|| format!("failed to write auth store {}", path.display()))
}

fn normalize_service(raw: &str) -> Result<String> {
    let service = raw.trim().to_ascii_lowercase();
    match service.as_str() {
        "registry" | "billing" | "*" => Ok(service),
        _ => bail!("invalid service `{}`; expected registry|billing|*", raw),
    }
}

fn normalize_scope(raw: &str) -> Result<String> {
    let scope = raw.trim().to_ascii_lowercase();
    match scope.as_str() {
        "registry.read" | "registry.write" | "registry.admin" | "billing.read"
        | "billing.write" | "billing.admin" | "*" => Ok(scope),
        _ => bail!("invalid scope `{}`", raw),
    }
}

fn scope_allowed_for_service(scope: &str, service: &str) -> bool {
    if service == "*" || scope == "*" {
        return true;
    }
    scope.starts_with(&format!("{}.", service))
}

fn service_matches(key_service: &str, requested_service: &str) -> bool {
    key_service == "*" || key_service == requested_service
}

fn scope_granted(scopes: &[String], required_scope: &str) -> bool {
    if scopes.iter().any(|scope| scope == "*") {
        return true;
    }
    if scopes.iter().any(|scope| scope == required_scope) {
        return true;
    }
    if let Some((prefix, _)) = required_scope.split_once('.') {
        let admin_scope = format!("{}.admin", prefix);
        scopes.iter().any(|scope| scope == &admin_scope)
    } else {
        false
    }
}

fn is_active(record: &ApiKeyRecord, now: DateTime<Utc>) -> bool {
    if record.revoked_at.is_some() {
        return false;
    }

    if let Some(expires_at) = &record.expires_at {
        let parsed = DateTime::parse_from_rfc3339(expires_at)
            .map(|value| value.with_timezone(&Utc))
            .ok();
        if let Some(expires_at) = parsed {
            return expires_at > now;
        }
    }

    true
}

fn make_key_id(seed: usize) -> String {
    format!("key_{}_{}", Utc::now().timestamp_millis(), seed + 1)
}

fn make_token() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    format!("dsk_{}", to_hex(&bytes))
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    to_hex(&digest)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn key_create_list_revoke_flow() {
        let temp = tempdir().expect("tempdir should exist");
        let path = temp.path().join("auth_keys.toml");

        let created = create_api_key(
            &path,
            CreateApiKeyInput {
                subject: "alice".to_string(),
                service: "registry".to_string(),
                org: Some("acme".to_string()),
                scopes: vec!["registry.read".to_string()],
                ttl_days: Some(30),
                rate_limit_per_minute: 60,
                note: Some("pilot key".to_string()),
            },
        )
        .expect("key should be created");
        assert!(created.token.starts_with("dsk_"));

        let listed = list_api_keys(&path).expect("keys should list");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].active);

        let revoked = revoke_api_key(&path, &created.id).expect("key should revoke");
        assert!(!revoked.active);

        let listed_after = list_api_keys(&path).expect("keys should list after revoke");
        assert!(!listed_after[0].active);
    }

    #[test]
    fn authorization_enforces_scope_org_and_service() {
        let temp = tempdir().expect("tempdir should exist");
        let path = temp.path().join("auth_keys.toml");

        let created = create_api_key(
            &path,
            CreateApiKeyInput {
                subject: "alice".to_string(),
                service: "registry".to_string(),
                org: Some("acme".to_string()),
                scopes: vec!["registry.write".to_string()],
                ttl_days: None,
                rate_limit_per_minute: 60,
                note: None,
            },
        )
        .expect("key should create");

        let runtime = init_runtime(&path).expect("runtime should init");

        let allowed = authorize(
            Some(&runtime),
            None,
            Some(&created.token),
            AuthorizationRequirement {
                service: "registry",
                scope: "registry.write",
                target_org: Some("acme"),
                require_unscoped_key: false,
            },
        )
        .expect("request should authorize");
        assert_eq!(allowed.subject, "alice");

        let denied_scope = authorize(
            Some(&runtime),
            None,
            Some(&created.token),
            AuthorizationRequirement {
                service: "registry",
                scope: "registry.admin",
                target_org: Some("acme"),
                require_unscoped_key: false,
            },
        )
        .expect_err("scope should deny");
        assert!(matches!(denied_scope, AuthDecision::Forbidden(_)));

        let denied_org = authorize(
            Some(&runtime),
            None,
            Some(&created.token),
            AuthorizationRequirement {
                service: "registry",
                scope: "registry.write",
                target_org: Some("other"),
                require_unscoped_key: false,
            },
        )
        .expect_err("org should deny");
        assert!(matches!(denied_org, AuthDecision::Forbidden(_)));

        let denied_service = authorize(
            Some(&runtime),
            None,
            Some(&created.token),
            AuthorizationRequirement {
                service: "billing",
                scope: "billing.read",
                target_org: None,
                require_unscoped_key: false,
            },
        )
        .expect_err("service should deny");
        assert!(matches!(denied_service, AuthDecision::Forbidden(_)));
    }

    #[test]
    fn rate_limiter_caps_per_minute() {
        let mut limiter = RateLimiter::default();
        assert!(limiter.allow("key-1", 2));
        assert!(limiter.allow("key-1", 2));
        assert!(!limiter.allow("key-1", 2));

        assert!(limiter.allow("key-2", 1));
        assert!(!limiter.allow("key-2", 1));
    }
}

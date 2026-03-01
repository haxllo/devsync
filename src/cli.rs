use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "devsync",
    version,
    about = "Reproducible developer environment bootstrapper"
)]
pub struct Cli {
    /// Project path to inspect and operate on.
    #[arg(long, short = 'p', global = true, default_value = ".")]
    pub path: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Generate devsync.lock and .devcontainer files.
    Init {
        /// Overwrite existing generated files.
        #[arg(long)]
        force: bool,
        /// Skip generating .devcontainer files.
        #[arg(long)]
        skip_devcontainer: bool,
        /// Generate devcontainer for the inferred primary stack only.
        #[arg(long)]
        primary_only: bool,
    },
    /// Regenerate the lockfile only.
    Lock {
        /// Overwrite existing devsync.lock if present.
        #[arg(long)]
        force: bool,
    },
    /// Inspect a project and print detection details without writing files.
    Survey {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Check runtime and tooling compatibility with devsync.lock.
    Doctor {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
        /// Choose which failing checks should return non-zero exit code.
        #[arg(long, value_enum, default_value_t = FailOn::All)]
        fail_on: FailOn,
    },
    /// Push current environment lock to shared registry.
    Push {
        /// Registry target in `org/project@version` format.
        target: String,
        /// Optional registry root path. Defaults to `~/.devsync/registry`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional registry HTTP base URL (for example `http://127.0.0.1:8787`).
        #[arg(long)]
        registry_url: Option<String>,
        /// Actor identity for permission checks and audit fields.
        #[arg(long)]
        actor: Option<String>,
        /// Grant access in `subject:role` format. Repeatable.
        #[arg(long = "grant")]
        grants: Vec<String>,
        /// Optional prebuild cache pointer (URL or identifier).
        #[arg(long)]
        prebuild_cache: Option<String>,
        /// Optional bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Overwrite version if it already exists.
        #[arg(long)]
        force: bool,
    },
    /// Pull an environment lock from shared registry.
    Pull {
        /// Registry target in `org/project@version` format.
        target: String,
        /// Optional registry root path. Defaults to `~/.devsync/registry`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional registry HTTP base URL (for example `http://127.0.0.1:8787`).
        #[arg(long)]
        registry_url: Option<String>,
        /// Actor identity for permission checks.
        #[arg(long)]
        actor: Option<String>,
        /// Overwrite existing `devsync.lock`.
        #[arg(long)]
        force: bool,
        /// Also generate `.devcontainer` files from pulled lock.
        #[arg(long)]
        with_devcontainer: bool,
        /// If generating devcontainer, use primary stack only.
        #[arg(long)]
        primary_only: bool,
        /// Optional bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
    },
    /// List versions in a registry project.
    RegistryLs {
        /// Project reference in `org/project` format.
        project: String,
        /// Optional registry root path. Defaults to `~/.devsync/registry`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional registry HTTP base URL (for example `http://127.0.0.1:8787`).
        #[arg(long)]
        registry_url: Option<String>,
        /// Actor identity for permission checks.
        #[arg(long)]
        actor: Option<String>,
        /// Optional bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Read audit events for a registry project.
    RegistryAudit {
        /// Project reference in `org/project` format.
        project: String,
        /// Optional registry root path. Defaults to `~/.devsync/registry`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional registry HTTP base URL (for example `http://127.0.0.1:8787`).
        #[arg(long)]
        registry_url: Option<String>,
        /// Actor identity for permission checks.
        #[arg(long)]
        actor: Option<String>,
        /// Optional bearer token for remote registry auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Maximum number of events to return.
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Run local registry HTTP server over file-backed storage.
    RegistryServe {
        /// Bind address, e.g. `127.0.0.1:8787`.
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: String,
        /// Optional registry root path. Defaults to `~/.devsync/registry`.
        #[arg(long)]
        registry: Option<PathBuf>,
        /// Optional billing storage root used for entitlement checks.
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Enforce org entitlement before serving registry routes.
        #[arg(long)]
        enforce_entitlements: bool,
        /// Optional bearer token required for all HTTP API requests.
        #[arg(long)]
        auth_token: Option<String>,
        /// Optional API key store path for scoped auth keys.
        #[arg(long)]
        auth_store: Option<PathBuf>,
        /// Handle a single request then exit (for smoke tests).
        #[arg(long)]
        once: bool,
    },
    /// Create an API key for registry/billing HTTP APIs.
    AuthKeyCreate {
        /// Optional auth key store path (defaults to `~/.devsync/auth_keys.toml`).
        #[arg(long)]
        auth_store: Option<PathBuf>,
        /// Subject label for this key (e.g. service account name).
        #[arg(long)]
        subject: String,
        /// Service scope: `registry`, `billing`, or `*`.
        #[arg(long, default_value = "*")]
        service: String,
        /// Optional org binding for least-privilege access.
        #[arg(long)]
        org: Option<String>,
        /// Permission scopes. Repeatable (`registry.read`, `registry.write`, `registry.admin`, `billing.read`, `billing.write`, `billing.admin`, `*`).
        #[arg(long = "scope", required = true)]
        scopes: Vec<String>,
        /// Optional TTL in days.
        #[arg(long)]
        ttl_days: Option<i64>,
        /// Requests-per-minute limit for this key.
        #[arg(long, default_value_t = 120)]
        rate_limit_rpm: u32,
        /// Optional operator note.
        #[arg(long)]
        note: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// List API keys from auth store.
    AuthKeyLs {
        /// Optional auth key store path (defaults to `~/.devsync/auth_keys.toml`).
        #[arg(long)]
        auth_store: Option<PathBuf>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Revoke an API key.
    AuthKeyRevoke {
        /// API key id (for example `key_...`).
        key_id: String,
        /// Optional auth key store path (defaults to `~/.devsync/auth_keys.toml`).
        #[arg(long)]
        auth_store: Option<PathBuf>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Check whether an org currently has active entitlement.
    EntitlementCheck {
        /// Organization identifier.
        org: String,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Validate governance policies for generated environment artifacts.
    Policy {
        /// Optional policy file path. Defaults to `<project>/devsync.policy.toml` if present.
        #[arg(long)]
        policy: Option<PathBuf>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Lint generated artifacts for likely secret exposure.
    SecretLint {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Show activation status and next steps for a repository.
    Activate {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Estimate ROI and recommended plan for DevSync adoption.
    Roi {
        /// Team size (number of developers).
        #[arg(long)]
        team_size: u32,
        /// Monthly new-developer onboardings.
        #[arg(long, default_value_t = 2.0)]
        monthly_hires: f64,
        /// Average onboarding hours before DevSync.
        #[arg(long, default_value_t = 6.0)]
        onboarding_hours_before: f64,
        /// Average onboarding hours after DevSync.
        #[arg(long, default_value_t = 1.5)]
        onboarding_hours_after: f64,
        /// Monthly drift incidents per developer before DevSync.
        #[arg(long, default_value_t = 0.5)]
        drift_incidents_per_dev: f64,
        /// Mean hours spent per drift incident.
        #[arg(long, default_value_t = 1.5)]
        drift_hours_per_incident: f64,
        /// Expected drift incident reduction percentage (0-100).
        #[arg(long, default_value_t = 50.0)]
        drift_reduction_pct: f64,
        /// Fully loaded engineering hourly rate in USD.
        #[arg(long, default_value_t = 90.0)]
        hourly_rate: f64,
        /// DevSync price per developer/month in USD.
        #[arg(long, default_value_t = 15.0)]
        price_per_dev: f64,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Export activation + ROI dashboard JSON for GTM reporting.
    DashboardExport {
        /// Root directory containing repositories (searches `*/.git` two levels deep).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Optional output file path. If omitted, JSON is printed to stdout.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Maximum repositories to include (for quick reports).
        #[arg(long)]
        max_repos: Option<usize>,
        /// Team size (number of developers).
        #[arg(long)]
        team_size: u32,
        /// Monthly new-developer onboardings.
        #[arg(long, default_value_t = 2.0)]
        monthly_hires: f64,
        /// Average onboarding hours before DevSync.
        #[arg(long, default_value_t = 6.0)]
        onboarding_hours_before: f64,
        /// Average onboarding hours after DevSync.
        #[arg(long, default_value_t = 1.5)]
        onboarding_hours_after: f64,
        /// Monthly drift incidents per developer before DevSync.
        #[arg(long, default_value_t = 0.5)]
        drift_incidents_per_dev: f64,
        /// Mean hours spent per drift incident.
        #[arg(long, default_value_t = 1.5)]
        drift_hours_per_incident: f64,
        /// Expected drift incident reduction percentage (0-100).
        #[arg(long, default_value_t = 50.0)]
        drift_reduction_pct: f64,
        /// Fully loaded engineering hourly rate in USD.
        #[arg(long, default_value_t = 90.0)]
        hourly_rate: f64,
        /// DevSync price per developer/month in USD.
        #[arg(long, default_value_t = 15.0)]
        price_per_dev: f64,
    },
    /// List available billing plans.
    BillingPlanLs {
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Create or update an org subscription.
    BillingSubscribe {
        /// Organization identifier.
        org: String,
        /// Plan id (`team`, `business`, `enterprise` by default).
        #[arg(long)]
        plan: String,
        /// Number of seats.
        #[arg(long, default_value_t = 1)]
        seats: u32,
        /// Optional billing contact email.
        #[arg(long)]
        customer_email: Option<String>,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// List subscriptions.
    BillingSubscriptionLs {
        /// Optional org filter.
        #[arg(long)]
        org: Option<String>,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Run billing cycle and issue due invoices.
    BillingCycle {
        /// Optional effective time (RFC3339). Defaults to now.
        #[arg(long)]
        at: Option<String>,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// List invoices.
    BillingInvoiceLs {
        /// Optional org filter.
        #[arg(long)]
        org: Option<String>,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Mark invoice as paid.
    BillingInvoicePay {
        /// Invoice id.
        invoice_id: String,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// List billing events (webhook outbox).
    BillingEvents {
        /// Optional org filter.
        #[arg(long)]
        org: Option<String>,
        /// Only include pending (not yet delivered) events.
        #[arg(long)]
        pending_only: bool,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Mark a billing event as delivered.
    BillingEventAck {
        /// Event id.
        event_id: String,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional billing HTTP base URL (for example `http://127.0.0.1:8795`).
        #[arg(long, conflicts_with = "billing")]
        billing_url: Option<String>,
        /// Optional bearer token for remote billing auth (or `DEVSYNC_AUTH_TOKEN` env var).
        #[arg(long)]
        auth_token: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Run local billing HTTP API server over file-backed storage.
    BillingServe {
        /// Bind address, e.g. `127.0.0.1:8795`.
        #[arg(long, default_value = "127.0.0.1:8795")]
        bind: String,
        /// Optional billing storage root (defaults to `~/.devsync/billing`).
        #[arg(long)]
        billing: Option<PathBuf>,
        /// Optional bearer token required for all HTTP API requests.
        #[arg(long)]
        auth_token: Option<String>,
        /// Optional API key store path for scoped auth keys.
        #[arg(long)]
        auth_store: Option<PathBuf>,
        /// Handle a single request then exit (for smoke tests).
        #[arg(long)]
        once: bool,
    },
    /// Bring up a dev environment using Dev Containers (or Docker fallback).
    Up,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FailOn {
    /// Fail on any warning.
    All,
    /// Fail only runtime-version mismatch/missing checks.
    Runtime,
    /// Fail only lockfile schema/presence checks.
    Lockfile,
    /// Fail only tooling checks (docker/devcontainer presence).
    Tooling,
    /// Fail on runtime + lockfile checks, ignore tooling warnings.
    RuntimeAndLock,
    /// Never fail based on doctor warnings (always exit 0).
    None,
}

impl FailOn {
    pub fn as_str(self) -> &'static str {
        match self {
            FailOn::All => "all",
            FailOn::Runtime => "runtime",
            FailOn::Lockfile => "lockfile",
            FailOn::Tooling => "tooling",
            FailOn::RuntimeAndLock => "runtime-and-lock",
            FailOn::None => "none",
        }
    }
}

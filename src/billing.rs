use crate::auth::{self, AuthorizationRequirement};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct StoreOptions {
    pub billing_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub billing_root: Option<PathBuf>,
    pub bind: String,
    pub auth_token: Option<String>,
    pub auth_store: Option<PathBuf>,
    pub once: bool,
}

#[derive(Debug, Clone)]
pub struct ServeResult {
    pub bind: String,
    pub billing_root: PathBuf,
    pub requests_handled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPlan {
    pub id: String,
    pub name: String,
    pub price_per_seat_cents: u32,
    pub interval: String,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub org: String,
    pub plan_id: String,
    pub seats: u32,
    pub customer_email: Option<String>,
    pub status: SubscriptionStatus,
    pub started_at: String,
    pub current_period_start: String,
    pub current_period_end: String,
    pub next_invoice_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Open,
    Paid,
    Voided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub org: String,
    pub subscription_id: String,
    pub period_start: String,
    pub period_end: String,
    pub amount_cents: u32,
    pub status: InvoiceStatus,
    pub issued_at: String,
    pub due_at: String,
    pub paid_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingEvent {
    pub id: String,
    pub event_type: String,
    pub occurred_at: String,
    pub org: String,
    pub subscription_id: Option<String>,
    pub invoice_id: Option<String>,
    pub payload: String,
    pub delivered_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleResult {
    pub effective_at: String,
    pub invoices_created: usize,
    pub events_created: usize,
}

#[derive(Debug, Clone)]
pub struct CreateSubscriptionInput {
    pub org: String,
    pub plan_id: String,
    pub seats: u32,
    pub customer_email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListFilter {
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntitlementReport {
    pub org: String,
    pub entitled: bool,
    pub reason: String,
    pub plan_id: Option<String>,
    pub seats: Option<u32>,
    pub status: Option<SubscriptionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BillingStore {
    schema_version: u32,
    #[serde(default)]
    plans: Vec<BillingPlan>,
    #[serde(default)]
    subscriptions: Vec<Subscription>,
    #[serde(default)]
    invoices: Vec<Invoice>,
    #[serde(default)]
    events: Vec<BillingEvent>,
}

impl Default for BillingStore {
    fn default() -> Self {
        let mut store = Self {
            schema_version: 1,
            plans: Vec::new(),
            subscriptions: Vec::new(),
            invoices: Vec::new(),
            events: Vec::new(),
        };
        seed_default_plans(&mut store);
        store
    }
}

pub fn list_plans(options: StoreOptions) -> Result<Vec<BillingPlan>> {
    let root = resolve_billing_root(options.billing_root)?;
    let store = load_store(&root)?;
    Ok(store.plans)
}

pub fn create_or_update_subscription(
    options: StoreOptions,
    input: CreateSubscriptionInput,
) -> Result<Subscription> {
    if input.org.trim().is_empty() {
        bail!("org cannot be empty");
    }
    if input.plan_id.trim().is_empty() {
        bail!("plan cannot be empty");
    }
    if input.seats == 0 {
        bail!("seats must be greater than zero");
    }

    let root = resolve_billing_root(options.billing_root)?;
    let mut store = load_store(&root)?;
    let plan_exists = store.plans.iter().any(|plan| plan.id == input.plan_id);
    if !plan_exists {
        bail!("plan `{}` not found", input.plan_id);
    }

    let now = Utc::now();
    let now_str = now.to_rfc3339();

    if let Some(existing) = store
        .subscriptions
        .iter_mut()
        .find(|sub| sub.org == input.org && sub.status == SubscriptionStatus::Active)
    {
        existing.plan_id = input.plan_id.clone();
        existing.seats = input.seats;
        existing.customer_email = input.customer_email.clone();
        store.events.push(BillingEvent {
            id: make_id("evt", store.events.len()),
            event_type: "subscription.updated".to_string(),
            occurred_at: now_str.clone(),
            org: existing.org.clone(),
            subscription_id: Some(existing.id.clone()),
            invoice_id: None,
            payload: format!(
                "{{\"plan\":\"{}\",\"seats\":{}}}",
                existing.plan_id, existing.seats
            ),
            delivered_at: None,
        });
    } else {
        let period_end = now + Duration::days(30);
        let subscription = Subscription {
            id: make_id("sub", store.subscriptions.len()),
            org: input.org.clone(),
            plan_id: input.plan_id.clone(),
            seats: input.seats,
            customer_email: input.customer_email.clone(),
            status: SubscriptionStatus::Active,
            started_at: now_str.clone(),
            current_period_start: now_str.clone(),
            current_period_end: period_end.to_rfc3339(),
            next_invoice_at: period_end.to_rfc3339(),
        };
        store.events.push(BillingEvent {
            id: make_id("evt", store.events.len()),
            event_type: "subscription.created".to_string(),
            occurred_at: now_str.clone(),
            org: subscription.org.clone(),
            subscription_id: Some(subscription.id.clone()),
            invoice_id: None,
            payload: format!(
                "{{\"plan\":\"{}\",\"seats\":{}}}",
                subscription.plan_id, subscription.seats
            ),
            delivered_at: None,
        });
        store.subscriptions.push(subscription);
    }

    save_store(&root, &store)?;
    let subscription = store
        .subscriptions
        .iter()
        .find(|sub| sub.org == input.org && sub.status == SubscriptionStatus::Active)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("subscription was not persisted"))?;
    Ok(subscription)
}

pub fn list_subscriptions(options: StoreOptions, filter: ListFilter) -> Result<Vec<Subscription>> {
    let root = resolve_billing_root(options.billing_root)?;
    let store = load_store(&root)?;
    let subscriptions = store
        .subscriptions
        .into_iter()
        .filter(|sub| match filter.org.as_deref() {
            Some(org) => sub.org == org,
            None => true,
        })
        .collect();
    Ok(subscriptions)
}

pub fn check_entitlement(options: StoreOptions, org: &str) -> Result<EntitlementReport> {
    let org = org.trim();
    if org.is_empty() {
        bail!("org cannot be empty");
    }
    let subscriptions = list_subscriptions(
        options,
        ListFilter {
            org: Some(org.to_string()),
        },
    )?;
    Ok(entitlement_from_subscriptions(
        org.to_string(),
        subscriptions,
    ))
}

pub fn entitlement_from_subscriptions(
    org: String,
    subscriptions: Vec<Subscription>,
) -> EntitlementReport {
    if let Some(active) = subscriptions
        .iter()
        .find(|subscription| subscription.status == SubscriptionStatus::Active)
    {
        return EntitlementReport {
            org,
            entitled: true,
            reason: "active subscription".to_string(),
            plan_id: Some(active.plan_id.clone()),
            seats: Some(active.seats),
            status: Some(active.status.clone()),
        };
    }

    if let Some(canceled) = subscriptions
        .iter()
        .find(|subscription| subscription.status == SubscriptionStatus::Canceled)
    {
        return EntitlementReport {
            org,
            entitled: false,
            reason: "subscription is canceled".to_string(),
            plan_id: Some(canceled.plan_id.clone()),
            seats: Some(canceled.seats),
            status: Some(canceled.status.clone()),
        };
    }

    EntitlementReport {
        org,
        entitled: false,
        reason: "no subscription found".to_string(),
        plan_id: None,
        seats: None,
        status: None,
    }
}

pub fn run_cycle(options: StoreOptions, at: Option<&str>) -> Result<CycleResult> {
    let root = resolve_billing_root(options.billing_root)?;
    let mut store = load_store(&root)?;
    let effective_at = parse_or_now(at)?;
    let mut invoices_created = 0usize;
    let mut events_created = 0usize;

    let plans = store.plans.clone();
    for subscription in &mut store.subscriptions {
        if subscription.status != SubscriptionStatus::Active {
            continue;
        }
        let next_invoice_at = parse_timestamp(&subscription.next_invoice_at)?;
        if next_invoice_at > effective_at {
            continue;
        }

        let Some(plan) = plans.iter().find(|plan| plan.id == subscription.plan_id) else {
            continue;
        };
        let amount_cents = plan.price_per_seat_cents.saturating_mul(subscription.seats);
        let invoice = Invoice {
            id: make_id("inv", store.invoices.len()),
            org: subscription.org.clone(),
            subscription_id: subscription.id.clone(),
            period_start: subscription.current_period_start.clone(),
            period_end: subscription.current_period_end.clone(),
            amount_cents,
            status: InvoiceStatus::Open,
            issued_at: effective_at.to_rfc3339(),
            due_at: (effective_at + Duration::days(14)).to_rfc3339(),
            paid_at: None,
        };
        store.invoices.push(invoice.clone());
        invoices_created += 1;

        store.events.push(BillingEvent {
            id: make_id("evt", store.events.len()),
            event_type: "invoice.created".to_string(),
            occurred_at: effective_at.to_rfc3339(),
            org: subscription.org.clone(),
            subscription_id: Some(subscription.id.clone()),
            invoice_id: Some(invoice.id),
            payload: format!("{{\"amount_cents\":{}}}", amount_cents),
            delivered_at: None,
        });
        events_created += 1;

        let current_end = parse_timestamp(&subscription.current_period_end)?;
        let new_end = current_end + Duration::days(30);
        subscription.current_period_start = current_end.to_rfc3339();
        subscription.current_period_end = new_end.to_rfc3339();
        subscription.next_invoice_at = new_end.to_rfc3339();
    }

    save_store(&root, &store)?;
    Ok(CycleResult {
        effective_at: effective_at.to_rfc3339(),
        invoices_created,
        events_created,
    })
}

pub fn list_invoices(options: StoreOptions, filter: ListFilter) -> Result<Vec<Invoice>> {
    let root = resolve_billing_root(options.billing_root)?;
    let store = load_store(&root)?;
    let invoices = store
        .invoices
        .into_iter()
        .filter(|invoice| match filter.org.as_deref() {
            Some(org) => invoice.org == org,
            None => true,
        })
        .collect();
    Ok(invoices)
}

pub fn mark_invoice_paid(options: StoreOptions, invoice_id: &str) -> Result<Invoice> {
    let root = resolve_billing_root(options.billing_root)?;
    let mut store = load_store(&root)?;
    let now = Utc::now();

    let Some(invoice) = store
        .invoices
        .iter_mut()
        .find(|invoice| invoice.id == invoice_id)
    else {
        bail!("invoice `{}` not found", invoice_id);
    };

    invoice.status = InvoiceStatus::Paid;
    invoice.paid_at = Some(now.to_rfc3339());
    let invoice_snapshot = invoice.clone();

    store.events.push(BillingEvent {
        id: make_id("evt", store.events.len()),
        event_type: "invoice.paid".to_string(),
        occurred_at: now.to_rfc3339(),
        org: invoice_snapshot.org.clone(),
        subscription_id: Some(invoice_snapshot.subscription_id.clone()),
        invoice_id: Some(invoice_snapshot.id.clone()),
        payload: format!("{{\"amount_cents\":{}}}", invoice_snapshot.amount_cents),
        delivered_at: None,
    });

    save_store(&root, &store)?;
    Ok(invoice_snapshot)
}

pub fn list_events(
    options: StoreOptions,
    filter: ListFilter,
    pending_only: bool,
) -> Result<Vec<BillingEvent>> {
    let root = resolve_billing_root(options.billing_root)?;
    let store = load_store(&root)?;
    let mut events: Vec<BillingEvent> = store
        .events
        .into_iter()
        .filter(|event| match filter.org.as_deref() {
            Some(org) => event.org == org,
            None => true,
        })
        .filter(|event| !pending_only || event.delivered_at.is_none())
        .collect();
    events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
    Ok(events)
}

pub fn ack_event(options: StoreOptions, event_id: &str) -> Result<BillingEvent> {
    let root = resolve_billing_root(options.billing_root)?;
    let mut store = load_store(&root)?;
    let now = Utc::now().to_rfc3339();
    let Some(event) = store.events.iter_mut().find(|event| event.id == event_id) else {
        bail!("event `{}` not found", event_id);
    };
    event.delivered_at = Some(now);
    let snapshot = event.clone();
    save_store(&root, &store)?;
    Ok(snapshot)
}

pub fn list_plans_remote(base_url: &str, auth_token: Option<String>) -> Result<Vec<BillingPlan>> {
    post_json_remote(
        base_url,
        "/v1/billing/plans/list",
        &ApiEmpty {},
        resolve_auth_token(auth_token),
    )
}

pub fn create_or_update_subscription_remote(
    base_url: &str,
    auth_token: Option<String>,
    input: CreateSubscriptionInput,
) -> Result<Subscription> {
    post_json_remote(
        base_url,
        "/v1/billing/subscriptions/create",
        &ApiCreateSubscriptionRequest {
            org: input.org,
            plan: input.plan_id,
            seats: input.seats,
            customer_email: input.customer_email,
        },
        resolve_auth_token(auth_token),
    )
}

pub fn list_subscriptions_remote(
    base_url: &str,
    auth_token: Option<String>,
    filter: ListFilter,
) -> Result<Vec<Subscription>> {
    post_json_remote(
        base_url,
        "/v1/billing/subscriptions/list",
        &ApiListFilterRequest { org: filter.org },
        resolve_auth_token(auth_token),
    )
}

pub fn run_cycle_remote(
    base_url: &str,
    auth_token: Option<String>,
    at: Option<&str>,
) -> Result<CycleResult> {
    post_json_remote(
        base_url,
        "/v1/billing/cycle/run",
        &ApiRunCycleRequest {
            at: at.map(ToString::to_string),
        },
        resolve_auth_token(auth_token),
    )
}

pub fn list_invoices_remote(
    base_url: &str,
    auth_token: Option<String>,
    filter: ListFilter,
) -> Result<Vec<Invoice>> {
    post_json_remote(
        base_url,
        "/v1/billing/invoices/list",
        &ApiListFilterRequest { org: filter.org },
        resolve_auth_token(auth_token),
    )
}

pub fn mark_invoice_paid_remote(
    base_url: &str,
    auth_token: Option<String>,
    invoice_id: &str,
) -> Result<Invoice> {
    post_json_remote(
        base_url,
        "/v1/billing/invoices/pay",
        &ApiInvoicePayRequest {
            invoice_id: invoice_id.to_string(),
        },
        resolve_auth_token(auth_token),
    )
}

pub fn list_events_remote(
    base_url: &str,
    auth_token: Option<String>,
    filter: ListFilter,
    pending_only: bool,
) -> Result<Vec<BillingEvent>> {
    post_json_remote(
        base_url,
        "/v1/billing/events/list",
        &ApiListEventsRequest {
            org: filter.org,
            pending_only,
        },
        resolve_auth_token(auth_token),
    )
}

pub fn ack_event_remote(
    base_url: &str,
    auth_token: Option<String>,
    event_id: &str,
) -> Result<BillingEvent> {
    post_json_remote(
        base_url,
        "/v1/billing/events/ack",
        &ApiAckEventRequest {
            event_id: event_id.to_string(),
        },
        resolve_auth_token(auth_token),
    )
}

pub fn serve_billing_http(options: ServeOptions) -> Result<ServeResult> {
    let billing_root = resolve_billing_root(options.billing_root)?;
    fs::create_dir_all(&billing_root)
        .with_context(|| format!("failed to create {}", billing_root.display()))?;
    let auth_token = resolve_auth_token(options.auth_token);
    let auth_runtime = match options.auth_store {
        Some(path) => Some(
            auth::init_runtime(&path)
                .with_context(|| format!("failed to load auth store {}", path.display()))?,
        ),
        None => None,
    };
    let mut rate_limiter = auth::RateLimiter::default();

    let listener = TcpListener::bind(&options.bind)
        .with_context(|| format!("failed to bind {}", options.bind))?;
    let mut requests_handled = 0usize;

    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept billing connection")?;
        handle_http_connection(
            &mut stream,
            &billing_root,
            auth_token.as_deref(),
            auth_runtime.as_ref(),
            &mut rate_limiter,
        )?;
        requests_handled += 1;
        if options.once {
            break;
        }
    }

    Ok(ServeResult {
        bind: options.bind,
        billing_root,
        requests_handled,
    })
}

pub fn resolve_billing_root(input: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = input {
        return Ok(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join(".devsync").join("billing"));
        }
    }
    bail!("failed to determine billing root; pass --billing explicitly")
}

fn seed_default_plans(store: &mut BillingStore) {
    if !store.plans.is_empty() {
        return;
    }
    store.plans.push(BillingPlan {
        id: "team".to_string(),
        name: "Team".to_string(),
        price_per_seat_cents: 1500,
        interval: "monthly".to_string(),
        features: vec![
            "team registry".to_string(),
            "basic audit logs".to_string(),
            "policy checks".to_string(),
        ],
    });
    store.plans.push(BillingPlan {
        id: "business".to_string(),
        name: "Business".to_string(),
        price_per_seat_cents: 2900,
        interval: "monthly".to_string(),
        features: vec![
            "sso integration".to_string(),
            "policy packs".to_string(),
            "extended audit retention".to_string(),
        ],
    });
    store.plans.push(BillingPlan {
        id: "enterprise".to_string(),
        name: "Enterprise".to_string(),
        price_per_seat_cents: 4900,
        interval: "monthly".to_string(),
        features: vec![
            "private control plane".to_string(),
            "sla".to_string(),
            "custom support".to_string(),
        ],
    });
}

fn load_store(root: &Path) -> Result<BillingStore> {
    fs::create_dir_all(root).with_context(|| format!("failed to create {}", root.display()))?;
    let path = store_path(root);
    if !path.is_file() {
        let store = BillingStore::default();
        save_store(root, &store)?;
        return Ok(store);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read billing store {}", path.display()))?;
    let mut store: BillingStore = toml::from_str(&content)
        .with_context(|| format!("failed to parse billing store {}", path.display()))?;
    if store.schema_version != 1 {
        bail!(
            "billing store schema {} is not supported",
            store.schema_version
        );
    }
    seed_default_plans(&mut store);
    Ok(store)
}

fn save_store(root: &Path, store: &BillingStore) -> Result<()> {
    let path = store_path(root);
    let serialized = toml::to_string_pretty(store).context("failed to serialize billing store")?;
    fs::write(&path, serialized)
        .with_context(|| format!("failed to write billing store {}", path.display()))?;
    Ok(())
}

fn store_path(root: &Path) -> PathBuf {
    root.join("store.toml")
}

fn make_id(prefix: &str, seed: usize) -> String {
    format!("{}_{}_{}", prefix, Utc::now().timestamp_millis(), seed + 1)
}

fn parse_or_now(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(raw) => parse_timestamp(raw),
        None => Ok(Utc::now()),
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid RFC3339 timestamp `{}`", value))
        .map(|ts| ts.with_timezone(&Utc))
}

fn resolve_auth_token(input: Option<String>) -> Option<String> {
    input
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("DEVSYNC_AUTH_TOKEN").ok())
        .filter(|value| !value.trim().is_empty())
}

#[derive(Debug)]
struct HttpEndpoint {
    host: String,
    port: u16,
    authority: String,
    base_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiCreateSubscriptionRequest {
    org: String,
    plan: String,
    seats: u32,
    customer_email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiListFilterRequest {
    org: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiRunCycleRequest {
    at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiInvoicePayRequest {
    invoice_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiListEventsRequest {
    org: Option<String>,
    pending_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiAckEventRequest {
    event_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiEmpty {}

fn post_json_remote<TReq, TRes>(
    base_url: &str,
    route: &str,
    payload: &TReq,
    auth_token: Option<String>,
) -> Result<TRes>
where
    TReq: Serialize,
    TRes: for<'de> Deserialize<'de>,
{
    let endpoint = parse_http_endpoint_remote(base_url)?;
    let body = serde_json::to_vec(payload).context("failed to serialize billing HTTP payload")?;
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
        .context("failed to write billing HTTP request headers")?;
    stream
        .write_all(&body)
        .context("failed to write billing HTTP request body")?;
    stream
        .flush()
        .context("failed to flush billing HTTP request")?;

    let mut response_bytes = Vec::new();
    stream
        .read_to_end(&mut response_bytes)
        .context("failed to read billing HTTP response")?;
    let (status_code, response_body) = split_http_response(&response_bytes)?;
    if !(200..300).contains(&status_code) {
        let error_text = String::from_utf8_lossy(response_body);
        bail!(
            "billing HTTP {} returned {}: {}",
            base_url,
            status_code,
            error_text
        );
    }

    serde_json::from_slice(response_body).context("failed to parse billing HTTP response body")
}

fn parse_http_endpoint_remote(raw: &str) -> Result<HttpEndpoint> {
    let url = raw.trim();
    if !url.starts_with("http://") {
        bail!("only http:// billing URLs are supported right now")
    }

    let rest = &url["http://".len()..];
    let (authority_part, path_part) = if let Some(index) = rest.find('/') {
        (&rest[..index], &rest[index..])
    } else {
        (rest, "")
    };

    if authority_part.trim().is_empty() {
        bail!("billing URL host cannot be empty")
    }

    let (host, port) = if let Some((host, port)) = authority_part.rsplit_once(':') {
        let parsed = port
            .parse::<u16>()
            .with_context(|| format!("invalid billing URL port `{}` in `{}`", port, raw))?;
        (host.to_string(), parsed)
    } else {
        (authority_part.to_string(), 80)
    };

    if host.trim().is_empty() {
        bail!("billing URL host cannot be empty")
    }

    let base_path = if path_part.is_empty() || path_part == "/" {
        String::new()
    } else {
        path_part.trim_end_matches('/').to_string()
    };

    Ok(HttpEndpoint {
        host,
        port,
        authority: authority_part.to_string(),
        base_path,
    })
}

fn handle_http_connection(
    stream: &mut TcpStream,
    billing_root: &Path,
    required_auth_token: Option<&str>,
    auth_runtime: Option<&auth::AuthRuntime>,
    rate_limiter: &mut auth::RateLimiter,
) -> Result<()> {
    let request = read_http_request(stream)?;
    let bearer_token = request
        .header("authorization")
        .and_then(auth::extract_bearer_token);

    let (response, identity, target_org) = match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/v1/billing/plans/list") => {
            match authorize_billing_request(
                auth_runtime,
                required_auth_token,
                bearer_token,
                "billing.read",
                None,
                false,
            ) {
                Ok(identity) => {
                    if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                        (
                            too_many_requests_response(identity.rate_limit_per_minute),
                            Some(identity),
                            None,
                        )
                    } else {
                        (
                            handle_json_response(list_plans(StoreOptions {
                                billing_root: Some(billing_root.to_path_buf()),
                            })),
                            Some(identity),
                            None,
                        )
                    }
                }
                Err(response) => (response, None, None),
            }
        }
        ("POST", "/v1/billing/subscriptions/create") => {
            match serde_json::from_slice::<ApiCreateSubscriptionRequest>(&request.body) {
                Ok(payload) => {
                    let org = payload.org.clone();
                    match authorize_billing_request(
                        auth_runtime,
                        required_auth_token,
                        bearer_token,
                        "billing.write",
                        Some(&org),
                        false,
                    ) {
                        Ok(identity) => {
                            if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                                (
                                    too_many_requests_response(identity.rate_limit_per_minute),
                                    Some(identity),
                                    Some(org),
                                )
                            } else {
                                (
                                    handle_json_response(create_or_update_subscription(
                                        StoreOptions {
                                            billing_root: Some(billing_root.to_path_buf()),
                                        },
                                        CreateSubscriptionInput {
                                            org: payload.org,
                                            plan_id: payload.plan,
                                            seats: payload.seats,
                                            customer_email: payload.customer_email,
                                        },
                                    )),
                                    Some(identity),
                                    Some(org),
                                )
                            }
                        }
                        Err(response) => (response, None, Some(org)),
                    }
                }
                Err(err) => (
                    bad_request_response(format!("invalid create subscription payload: {}", err)),
                    None,
                    None,
                ),
            }
        }
        ("POST", "/v1/billing/subscriptions/list") => {
            let payload: ApiListFilterRequest =
                serde_json::from_slice(&request.body).unwrap_or(ApiListFilterRequest { org: None });
            let (scope, org, require_unscoped) = match payload.org.clone() {
                Some(org) => ("billing.read", Some(org), false),
                None => ("billing.admin", None, true),
            };
            match authorize_billing_request(
                auth_runtime,
                required_auth_token,
                bearer_token,
                scope,
                org.as_deref(),
                require_unscoped,
            ) {
                Ok(identity) => {
                    if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                        (
                            too_many_requests_response(identity.rate_limit_per_minute),
                            Some(identity),
                            org,
                        )
                    } else {
                        (
                            handle_json_response(list_subscriptions(
                                StoreOptions {
                                    billing_root: Some(billing_root.to_path_buf()),
                                },
                                ListFilter {
                                    org: payload.org.clone(),
                                },
                            )),
                            Some(identity),
                            org,
                        )
                    }
                }
                Err(response) => (response, None, org),
            }
        }
        ("POST", "/v1/billing/cycle/run") => {
            let payload: ApiRunCycleRequest =
                serde_json::from_slice(&request.body).unwrap_or(ApiRunCycleRequest { at: None });
            match authorize_billing_request(
                auth_runtime,
                required_auth_token,
                bearer_token,
                "billing.admin",
                None,
                true,
            ) {
                Ok(identity) => {
                    if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                        (
                            too_many_requests_response(identity.rate_limit_per_minute),
                            Some(identity),
                            None,
                        )
                    } else {
                        (
                            handle_json_response(run_cycle(
                                StoreOptions {
                                    billing_root: Some(billing_root.to_path_buf()),
                                },
                                payload.at.as_deref(),
                            )),
                            Some(identity),
                            None,
                        )
                    }
                }
                Err(response) => (response, None, None),
            }
        }
        ("POST", "/v1/billing/invoices/list") => {
            let payload: ApiListFilterRequest =
                serde_json::from_slice(&request.body).unwrap_or(ApiListFilterRequest { org: None });
            let (scope, org, require_unscoped) = match payload.org.clone() {
                Some(org) => ("billing.read", Some(org), false),
                None => ("billing.admin", None, true),
            };
            match authorize_billing_request(
                auth_runtime,
                required_auth_token,
                bearer_token,
                scope,
                org.as_deref(),
                require_unscoped,
            ) {
                Ok(identity) => {
                    if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                        (
                            too_many_requests_response(identity.rate_limit_per_minute),
                            Some(identity),
                            org,
                        )
                    } else {
                        (
                            handle_json_response(list_invoices(
                                StoreOptions {
                                    billing_root: Some(billing_root.to_path_buf()),
                                },
                                ListFilter {
                                    org: payload.org.clone(),
                                },
                            )),
                            Some(identity),
                            org,
                        )
                    }
                }
                Err(response) => (response, None, org),
            }
        }
        ("POST", "/v1/billing/invoices/pay") => {
            match serde_json::from_slice::<ApiInvoicePayRequest>(&request.body) {
                Ok(payload) => {
                    let org = match lookup_invoice_org(billing_root, &payload.invoice_id) {
                        Ok(Some(org)) => org,
                        Ok(None) => {
                            return write_http_response(
                                stream,
                                &bad_request_response(format!(
                                    "invoice `{}` not found",
                                    payload.invoice_id
                                )),
                            )
                        }
                        Err(err) => {
                            return write_http_response(
                                stream,
                                &bad_request_response(err.to_string()),
                            )
                        }
                    };
                    match authorize_billing_request(
                        auth_runtime,
                        required_auth_token,
                        bearer_token,
                        "billing.write",
                        Some(&org),
                        false,
                    ) {
                        Ok(identity) => {
                            if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                                (
                                    too_many_requests_response(identity.rate_limit_per_minute),
                                    Some(identity),
                                    Some(org),
                                )
                            } else {
                                (
                                    handle_json_response(mark_invoice_paid(
                                        StoreOptions {
                                            billing_root: Some(billing_root.to_path_buf()),
                                        },
                                        &payload.invoice_id,
                                    )),
                                    Some(identity),
                                    Some(org),
                                )
                            }
                        }
                        Err(response) => (response, None, Some(org)),
                    }
                }
                Err(err) => (
                    bad_request_response(format!("invalid invoice pay payload: {}", err)),
                    None,
                    None,
                ),
            }
        }
        ("POST", "/v1/billing/events/list") => {
            let payload: ApiListEventsRequest = serde_json::from_slice(&request.body).unwrap_or(
                ApiListEventsRequest {
                    org: None,
                    pending_only: false,
                },
            );
            let (scope, org, require_unscoped) = match payload.org.clone() {
                Some(org) => ("billing.read", Some(org), false),
                None => ("billing.admin", None, true),
            };
            match authorize_billing_request(
                auth_runtime,
                required_auth_token,
                bearer_token,
                scope,
                org.as_deref(),
                require_unscoped,
            ) {
                Ok(identity) => {
                    if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                        (
                            too_many_requests_response(identity.rate_limit_per_minute),
                            Some(identity),
                            org,
                        )
                    } else {
                        (
                            handle_json_response(list_events(
                                StoreOptions {
                                    billing_root: Some(billing_root.to_path_buf()),
                                },
                                ListFilter {
                                    org: payload.org.clone(),
                                },
                                payload.pending_only,
                            )),
                            Some(identity),
                            org,
                        )
                    }
                }
                Err(response) => (response, None, org),
            }
        }
        ("POST", "/v1/billing/events/ack") => {
            match serde_json::from_slice::<ApiAckEventRequest>(&request.body) {
                Ok(payload) => {
                    let org = match lookup_event_org(billing_root, &payload.event_id) {
                        Ok(Some(org)) => org,
                        Ok(None) => {
                            return write_http_response(
                                stream,
                                &bad_request_response(format!("event `{}` not found", payload.event_id)),
                            )
                        }
                        Err(err) => {
                            return write_http_response(
                                stream,
                                &bad_request_response(err.to_string()),
                            )
                        }
                    };
                    match authorize_billing_request(
                        auth_runtime,
                        required_auth_token,
                        bearer_token,
                        "billing.write",
                        Some(&org),
                        false,
                    ) {
                        Ok(identity) => {
                            if !rate_limiter.allow(&identity.key_id, identity.rate_limit_per_minute) {
                                (
                                    too_many_requests_response(identity.rate_limit_per_minute),
                                    Some(identity),
                                    Some(org),
                                )
                            } else {
                                (
                                    handle_json_response(ack_event(
                                        StoreOptions {
                                            billing_root: Some(billing_root.to_path_buf()),
                                        },
                                        &payload.event_id,
                                    )),
                                    Some(identity),
                                    Some(org),
                                )
                            }
                        }
                        Err(response) => (response, None, Some(org)),
                    }
                }
                Err(err) => (
                    bad_request_response(format!("invalid event ack payload: {}", err)),
                    None,
                    None,
                ),
            }
        }
        _ => (
            HttpResponse::json(
                404,
                "Not Found",
                br#"{"error":"not found","routes":["POST /v1/billing/plans/list","POST /v1/billing/subscriptions/create","POST /v1/billing/subscriptions/list","POST /v1/billing/cycle/run","POST /v1/billing/invoices/list","POST /v1/billing/invoices/pay","POST /v1/billing/events/list","POST /v1/billing/events/ack"]}"#
                    .to_vec(),
            ),
            None,
            None,
        ),
    };

    if let Err(err) = append_access_log(
        billing_root,
        &request,
        response.status_code,
        identity.as_ref(),
        target_org.as_deref(),
    ) {
        eprintln!("failed to write billing access log: {}", err);
    }

    write_http_response(stream, &response)?;
    Ok(())
}

fn handle_json_response<T: Serialize>(result: Result<T>) -> HttpResponse {
    match result {
        Ok(payload) => match serde_json::to_vec(&payload) {
            Ok(body) => HttpResponse::json(200, "OK", body),
            Err(err) => HttpResponse::json(
                500,
                "Internal Server Error",
                format!("{{\"error\":\"serialization\",\"message\":\"{}\"}}", err).into_bytes(),
            ),
        },
        Err(err) => HttpResponse::json(
            400,
            "Bad Request",
            format!("{{\"error\":\"bad_request\",\"message\":\"{}\"}}", err).into_bytes(),
        ),
    }
}

fn bad_request_response(message: String) -> HttpResponse {
    let body = serde_json::to_vec(&serde_json::json!({
        "error": "bad_request",
        "message": message,
    }))
    .unwrap_or_else(|_| br#"{"error":"bad_request"}"#.to_vec());
    HttpResponse::json(400, "Bad Request", body)
}

fn too_many_requests_response(limit: u32) -> HttpResponse {
    let body = serde_json::to_vec(&serde_json::json!({
        "error": "rate_limited",
        "message": format!("rate limit exceeded ({} requests/minute)", limit.max(1)),
    }))
    .unwrap_or_else(|_| br#"{"error":"rate_limited"}"#.to_vec());
    HttpResponse::json(429, "Too Many Requests", body)
}

fn authorize_billing_request(
    auth_runtime: Option<&auth::AuthRuntime>,
    required_auth_token: Option<&str>,
    bearer_token: Option<&str>,
    scope: &str,
    target_org: Option<&str>,
    require_unscoped_key: bool,
) -> std::result::Result<auth::AccessIdentity, HttpResponse> {
    match auth::authorize(
        auth_runtime,
        required_auth_token,
        bearer_token,
        AuthorizationRequirement {
            service: "billing",
            scope,
            target_org,
            require_unscoped_key,
        },
    ) {
        Ok(identity) => Ok(identity),
        Err(decision) => {
            let status = decision.status_code();
            let status_text = if status == 401 {
                "Unauthorized"
            } else {
                "Forbidden"
            };
            let error = if status == 401 {
                "unauthorized"
            } else {
                "forbidden"
            };
            let body = serde_json::to_vec(&serde_json::json!({
                "error": error,
                "message": decision.message(),
            }))
            .unwrap_or_else(|_| br#"{"error":"unauthorized"}"#.to_vec());
            Err(HttpResponse::json(status, status_text, body))
        }
    }
}

fn lookup_invoice_org(billing_root: &Path, invoice_id: &str) -> Result<Option<String>> {
    let invoices = list_invoices(
        StoreOptions {
            billing_root: Some(billing_root.to_path_buf()),
        },
        ListFilter { org: None },
    )?;
    Ok(invoices
        .into_iter()
        .find(|invoice| invoice.id == invoice_id)
        .map(|invoice| invoice.org))
}

fn lookup_event_org(billing_root: &Path, event_id: &str) -> Result<Option<String>> {
    let events = list_events(
        StoreOptions {
            billing_root: Some(billing_root.to_path_buf()),
        },
        ListFilter { org: None },
        false,
    )?;
    Ok(events
        .into_iter()
        .find(|event| event.id == event_id)
        .map(|event| event.org))
}

#[derive(Debug, Serialize)]
struct AccessLogEntry {
    occurred_at: String,
    method: String,
    path: String,
    status_code: u16,
    subject: String,
    key_id: String,
    org: Option<String>,
}

fn append_access_log(
    billing_root: &Path,
    request: &HttpRequest,
    status_code: u16,
    identity: Option<&auth::AccessIdentity>,
    target_org: Option<&str>,
) -> Result<()> {
    let access_path = billing_root.join("access.log");
    let mut writer = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&access_path)
        .with_context(|| format!("failed to open {}", access_path.display()))?;

    let entry = AccessLogEntry {
        occurred_at: Utc::now().to_rfc3339(),
        method: request.method.clone(),
        path: request.path.clone(),
        status_code,
        subject: identity
            .map(|value| value.subject.clone())
            .unwrap_or_else(|| "anonymous".to_string()),
        key_id: identity
            .map(|value| value.key_id.clone())
            .unwrap_or_else(|| "anonymous".to_string()),
        org: target_org
            .map(ToString::to_string)
            .or_else(|| identity.and_then(|value| value.org.clone())),
    };
    let line = serde_json::to_string(&entry).context("failed to serialize access log entry")?;
    use std::io::Write as _;
    writeln!(writer, "{line}")
        .with_context(|| format!("failed to write {}", access_path.display()))?;
    Ok(())
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
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(status_code: u16, status_text: &'static str, body: Vec<u8>) -> Self {
        Self {
            status_code,
            status_text,
            body,
        }
    }
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

    let headers_text = String::from_utf8(buffer[..headers_end].to_vec())
        .context("request headers are not UTF-8")?;
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
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        response.status_text,
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
        .context("billing HTTP response headers are not UTF-8")?;
    let status_line = header_text
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status line"))?;
    let mut parts = status_line.split_whitespace();
    let _http_version = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP version in status line"))?;
    let code = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing status code in status line"))?;
    let status_code = code
        .parse::<u16>()
        .with_context(|| format!("invalid status code `{}`", code))?;

    Ok((status_code, &bytes[header_end + 4..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_default_plans() {
        let root = tempdir().expect("tempdir should be created");
        let plans = list_plans(StoreOptions {
            billing_root: Some(root.path().to_path_buf()),
        })
        .expect("plans should list");
        assert!(plans.iter().any(|plan| plan.id == "team"));
        assert!(plans.iter().any(|plan| plan.id == "business"));
    }

    #[test]
    fn subscription_cycle_generates_invoice_and_event() {
        let root = tempdir().expect("tempdir should be created");
        let options = StoreOptions {
            billing_root: Some(root.path().to_path_buf()),
        };
        let subscription = create_or_update_subscription(
            options.clone(),
            CreateSubscriptionInput {
                org: "acme".to_string(),
                plan_id: "team".to_string(),
                seats: 5,
                customer_email: None,
            },
        )
        .expect("subscription should be created");
        assert_eq!(subscription.plan_id, "team");

        let cycle =
            run_cycle(options.clone(), Some("2099-01-01T00:00:00Z")).expect("cycle should run");
        assert!(cycle.invoices_created > 0);

        let invoices = list_invoices(
            options.clone(),
            ListFilter {
                org: Some("acme".to_string()),
            },
        )
        .expect("invoices should list");
        assert!(!invoices.is_empty());
        let events = list_events(
            options,
            ListFilter {
                org: Some("acme".to_string()),
            },
            false,
        )
        .expect("events should list");
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "invoice.created")
        );
    }

    #[test]
    fn invoice_payment_emits_event() {
        let root = tempdir().expect("tempdir should be created");
        let options = StoreOptions {
            billing_root: Some(root.path().to_path_buf()),
        };
        create_or_update_subscription(
            options.clone(),
            CreateSubscriptionInput {
                org: "acme".to_string(),
                plan_id: "team".to_string(),
                seats: 3,
                customer_email: None,
            },
        )
        .expect("subscription should be created");
        run_cycle(options.clone(), Some("2099-01-01T00:00:00Z")).expect("cycle should run");
        let invoice_id = list_invoices(
            options.clone(),
            ListFilter {
                org: Some("acme".to_string()),
            },
        )
        .expect("invoices should list")
        .first()
        .expect("invoice should exist")
        .id
        .clone();

        let paid = mark_invoice_paid(options.clone(), &invoice_id).expect("invoice should be paid");
        assert_eq!(paid.status, InvoiceStatus::Paid);
        let events = list_events(
            options.clone(),
            ListFilter {
                org: Some("acme".to_string()),
            },
            false,
        )
        .expect("events should list");
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "invoice.paid")
        );

        let pending = list_events(
            options.clone(),
            ListFilter {
                org: Some("acme".to_string()),
            },
            true,
        )
        .expect("pending events should list");
        assert!(!pending.is_empty());

        let acked = ack_event(options, &pending[0].id).expect("event should be acknowledged");
        assert!(acked.delivered_at.is_some());
    }

    #[test]
    fn parses_remote_http_endpoint() {
        let endpoint =
            parse_http_endpoint_remote("http://127.0.0.1:8795/api").expect("endpoint should parse");
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 8795);
        assert_eq!(endpoint.base_path, "/api");
    }
}

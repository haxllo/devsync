mod activation;
mod auth;
mod billing;
mod cli;
mod dashboard;
mod detect;
mod devcontainer;
mod doctor;
mod lockfile;
mod policy;
mod registry;
mod roi;
mod secrets;
mod up;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Commands, FailOn};
use detect::detect_project;
use lockfile::{DevsyncLock, read_lock, write_lock};
use registry::{
    AuditListOptions, ListOptions, PullOptions, PushOptions, list_audit_events,
    list_audit_events_remote, list_versions, parse_project_ref, parse_target, pull_environment,
    pull_environment_remote, push_environment, push_environment_remote, serve_registry_http,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_path = cli
        .path
        .canonicalize()
        .with_context(|| format!("failed to resolve project path: {}", cli.path.display()))?;

    match cli.command {
        Commands::Init {
            force,
            skip_devcontainer,
            primary_only,
        } => run_init(&project_path, force, skip_devcontainer, primary_only),
        Commands::Lock { force } => run_lock(&project_path, force),
        Commands::Survey { json } => run_survey(&project_path, json),
        Commands::Doctor { json, fail_on } => run_doctor(&project_path, json, fail_on),
        Commands::Push {
            target,
            registry,
            registry_url,
            actor,
            grants,
            prebuild_cache,
            auth_token,
            force,
        } => run_push(
            &project_path,
            &target,
            registry_url,
            PushOptions {
                registry_root: registry,
                actor,
                grants,
                prebuild_cache,
                auth_token,
                force,
            },
        ),
        Commands::Pull {
            target,
            registry,
            registry_url,
            actor,
            force,
            with_devcontainer,
            primary_only,
            auth_token,
        } => run_pull(
            &project_path,
            &target,
            registry_url,
            PullOptions {
                registry_root: registry,
                actor,
                force,
                with_devcontainer,
                primary_only,
                auth_token,
            },
        ),
        Commands::RegistryLs {
            project,
            registry,
            registry_url,
            actor,
            auth_token,
            json,
        } => run_registry_ls(
            &project,
            registry_url,
            ListOptions {
                registry_root: registry,
                actor,
                auth_token,
            },
            json,
        ),
        Commands::RegistryAudit {
            project,
            registry,
            registry_url,
            actor,
            auth_token,
            limit,
            json,
        } => run_registry_audit(
            &project,
            registry_url,
            AuditListOptions {
                registry_root: registry,
                actor,
                auth_token,
                limit,
            },
            json,
        ),
        Commands::RegistryServe {
            bind,
            registry,
            billing,
            enforce_entitlements,
            auth_token,
            auth_store,
            once,
        } => run_registry_serve(
            bind,
            registry,
            billing,
            enforce_entitlements,
            auth_token,
            auth_store,
            once,
        ),
        Commands::AuthKeyCreate {
            auth_store,
            subject,
            service,
            org,
            scopes,
            ttl_days,
            rate_limit_rpm,
            note,
            json,
        } => run_auth_key_create(
            auth_store,
            auth::CreateApiKeyInput {
                subject,
                service,
                org,
                scopes,
                ttl_days,
                rate_limit_per_minute: rate_limit_rpm,
                note,
            },
            json,
        ),
        Commands::AuthKeyLs { auth_store, json } => run_auth_key_ls(auth_store, json),
        Commands::AuthKeyRevoke {
            key_id,
            auth_store,
            json,
        } => run_auth_key_revoke(key_id, auth_store, json),
        Commands::EntitlementCheck {
            org,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_entitlement_check(org, billing, billing_url, auth_token, json),
        Commands::Policy { policy, json } => run_policy(&project_path, policy, json),
        Commands::SecretLint { json } => run_secret_lint(&project_path, json),
        Commands::Activate { json } => run_activate(&project_path, json),
        Commands::Roi {
            team_size,
            monthly_hires,
            onboarding_hours_before,
            onboarding_hours_after,
            drift_incidents_per_dev,
            drift_hours_per_incident,
            drift_reduction_pct,
            hourly_rate,
            price_per_dev,
            json,
        } => run_roi(
            roi::RoiInput {
                team_size,
                monthly_hires,
                onboarding_hours_before,
                onboarding_hours_after,
                drift_incidents_per_dev,
                drift_hours_per_incident,
                drift_reduction_pct,
                hourly_rate,
                price_per_dev,
            },
            json,
        ),
        Commands::Up => up::run_up(&project_path),
        Commands::DashboardExport {
            root,
            output,
            max_repos,
            team_size,
            monthly_hires,
            onboarding_hours_before,
            onboarding_hours_after,
            drift_incidents_per_dev,
            drift_hours_per_incident,
            drift_reduction_pct,
            hourly_rate,
            price_per_dev,
        } => run_dashboard_export(
            &project_path,
            root,
            output,
            max_repos,
            roi::RoiInput {
                team_size,
                monthly_hires,
                onboarding_hours_before,
                onboarding_hours_after,
                drift_incidents_per_dev,
                drift_hours_per_incident,
                drift_reduction_pct,
                hourly_rate,
                price_per_dev,
            },
        ),
        Commands::BillingPlanLs {
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_plan_ls(billing, billing_url, auth_token, json),
        Commands::BillingSubscribe {
            org,
            plan,
            seats,
            customer_email,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_subscribe(
            org,
            plan,
            seats,
            customer_email,
            billing,
            billing_url,
            auth_token,
            json,
        ),
        Commands::BillingSubscriptionLs {
            org,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_subscription_ls(org, billing, billing_url, auth_token, json),
        Commands::BillingCycle {
            at,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_cycle(at, billing, billing_url, auth_token, json),
        Commands::BillingInvoiceLs {
            org,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_invoice_ls(org, billing, billing_url, auth_token, json),
        Commands::BillingInvoicePay {
            invoice_id,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_invoice_pay(invoice_id, billing, billing_url, auth_token, json),
        Commands::BillingEvents {
            org,
            pending_only,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_events(org, pending_only, billing, billing_url, auth_token, json),
        Commands::BillingEventAck {
            event_id,
            billing,
            billing_url,
            auth_token,
            json,
        } => run_billing_event_ack(event_id, billing, billing_url, auth_token, json),
        Commands::BillingServe {
            bind,
            billing,
            auth_token,
            auth_store,
            once,
        } => run_billing_serve(bind, billing, auth_token, auth_store, once),
    }
}

fn run_init(
    project_path: &std::path::Path,
    force: bool,
    skip_devcontainer: bool,
    primary_only: bool,
) -> Result<()> {
    let detection = detect_project(project_path)?;
    let lock_path = project_path.join("devsync.lock");
    let previous = read_existing_lock(&lock_path);
    let lock = DevsyncLock::from_detection(&detection, previous.as_ref());

    write_lock(&lock_path, &lock, force)?;

    if !skip_devcontainer {
        devcontainer::generate_devcontainer(project_path, &lock, force, primary_only)?;
    }

    println!("Generated {}", lock_path.display());
    if !skip_devcontainer {
        println!("Generated .devcontainer/devcontainer.json and .devcontainer/Dockerfile");
        if primary_only {
            println!("Devcontainer mode: primary stack only");
        }
    }

    print_detection_summary(&detection);
    Ok(())
}

fn run_lock(project_path: &std::path::Path, force: bool) -> Result<()> {
    let detection = detect_project(project_path)?;
    let lock_path = project_path.join("devsync.lock");
    let previous = read_existing_lock(&lock_path);
    let lock = DevsyncLock::from_detection(&detection, previous.as_ref());

    write_lock(&lock_path, &lock, force)?;
    println!("Updated {}", lock_path.display());
    print_detection_summary(&detection);
    Ok(())
}

fn run_survey(project_path: &std::path::Path, json: bool) -> Result<()> {
    let detection = detect_project(project_path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&detection)
                .context("failed to serialize survey output as JSON")?
        );
    } else {
        print_detection_summary(&detection);
    }
    Ok(())
}

fn run_push(
    project_path: &std::path::Path,
    target_raw: &str,
    registry_url: Option<String>,
    options: PushOptions,
) -> Result<()> {
    let target = parse_target(target_raw)?;
    let result = if let Some(url) = registry_url.as_deref() {
        push_environment_remote(project_path, &target, url, options)?
    } else {
        push_environment(project_path, &target, options)?
    };

    println!(
        "Pushed {}/{}@{} to {}",
        result.org,
        result.project,
        result.version,
        result.path.display()
    );
    if let Some(cache) = result.prebuild_cache {
        println!("Prebuild cache pointer: {}", cache);
    }
    Ok(())
}

fn run_pull(
    project_path: &std::path::Path,
    target_raw: &str,
    registry_url: Option<String>,
    options: PullOptions,
) -> Result<()> {
    let target = parse_target(target_raw)?;
    let result = if let Some(url) = registry_url.as_deref() {
        pull_environment_remote(project_path, &target, url, options)?
    } else {
        pull_environment(project_path, &target, options)?
    };

    println!(
        "Pulled {}/{}@{} to {}",
        result.org,
        result.project,
        result.version,
        result.lockfile_path.display()
    );
    if let Some(cache) = result.prebuild_cache {
        println!("Prebuild cache pointer: {}", cache);
    }
    Ok(())
}

fn run_registry_ls(
    project_raw: &str,
    registry_url: Option<String>,
    options: ListOptions,
    json: bool,
) -> Result<()> {
    let project = parse_project_ref(project_raw)?;
    let result = if let Some(url) = registry_url.as_deref() {
        registry::list_versions_remote(&project, url, options)?
    } else {
        list_versions(&project, options)?
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .context("failed to serialize registry list as JSON")?
        );
        return Ok(());
    }

    println!("Registry project: {}/{}", result.org, result.project);
    println!("Latest: {}", result.latest.as_deref().unwrap_or("-"));
    if !result.role_bindings.is_empty() {
        println!("Roles: {}", result.role_bindings.join(", "));
    }

    if result.versions.is_empty() {
        println!("Versions: none");
        return Ok(());
    }

    println!("Versions:");
    for version in result.versions {
        let cache = version.prebuild_cache.unwrap_or_else(|| "-".to_string());
        println!(
            "- {} (created_by={}, created_at={}, cache={})",
            version.version, version.created_by, version.created_at, cache
        );
    }

    Ok(())
}

fn run_registry_serve(
    bind: String,
    registry: Option<std::path::PathBuf>,
    billing: Option<std::path::PathBuf>,
    enforce_entitlements: bool,
    auth_token: Option<String>,
    auth_store: Option<PathBuf>,
    once: bool,
) -> Result<()> {
    let result = serve_registry_http(registry::ServeOptions {
        registry_root: registry,
        billing_root: billing,
        enforce_entitlements,
        bind,
        auth_token,
        auth_store,
        once,
    })?;

    println!(
        "Registry server stopped (bind={}, root={}, handled={})",
        result.bind,
        result.registry_root.display(),
        result.requests_handled
    );
    Ok(())
}

fn run_registry_audit(
    project_raw: &str,
    registry_url: Option<String>,
    options: AuditListOptions,
    json: bool,
) -> Result<()> {
    let project = parse_project_ref(project_raw)?;
    let events = if let Some(url) = registry_url.as_deref() {
        list_audit_events_remote(&project, url, options)?
    } else {
        list_audit_events(&project, options)?
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&events)
                .context("failed to serialize registry audit events as JSON")?
        );
        return Ok(());
    }

    if events.is_empty() {
        println!("No audit events found.");
        return Ok(());
    }

    println!("Audit events (most recent first):");
    for event in events {
        println!(
            "- {} {} {}/{}{} actor={}",
            event.occurred_at,
            event.action,
            event.org,
            event.project,
            event
                .version
                .as_deref()
                .map(|v| format!("@{}", v))
                .unwrap_or_default(),
            event.actor
        );
    }

    Ok(())
}

fn run_policy(
    project_path: &std::path::Path,
    policy_path: Option<std::path::PathBuf>,
    json: bool,
) -> Result<()> {
    let lock_path = project_path.join("devsync.lock");
    let lock = if lock_path.is_file() {
        Some(read_lock(&lock_path)?)
    } else {
        None
    };

    let report = policy::run_policy(project_path, lock.as_ref(), policy_path.as_deref())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize policy report as JSON")?
        );
    } else {
        policy::render_report(&report);
    }

    if report.passed {
        Ok(())
    } else {
        anyhow::bail!("policy violations found")
    }
}

fn run_secret_lint(project_path: &std::path::Path, json: bool) -> Result<()> {
    let report = secrets::run_secret_lint(project_path)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize secret-lint report as JSON")?
        );
    } else {
        secrets::render_report(&report);
    }

    if report.passed {
        Ok(())
    } else {
        anyhow::bail!("secret exposure findings detected")
    }
}

fn run_activate(project_path: &std::path::Path, json: bool) -> Result<()> {
    let report = activation::run_activation(project_path)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize activation report as JSON")?
        );
    } else {
        activation::render_report(&report);
    }

    if report.ready {
        Ok(())
    } else {
        anyhow::bail!("activation checklist has pending actions")
    }
}

fn run_roi(input: roi::RoiInput, json: bool) -> Result<()> {
    let report = roi::compute_roi(&input)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("failed to serialize ROI as JSON")?
        );
    } else {
        roi::render_report(&report);
    }

    Ok(())
}

fn run_dashboard_export(
    project_path: &std::path::Path,
    root: Option<PathBuf>,
    output: Option<PathBuf>,
    max_repos: Option<usize>,
    roi_input: roi::RoiInput,
) -> Result<()> {
    let root = match root {
        Some(path) => path
            .canonicalize()
            .with_context(|| format!("failed to resolve dashboard root: {}", path.display()))?,
        None => project_path.to_path_buf(),
    };

    let report = dashboard::build_dashboard(dashboard::DashboardOptions {
        root: root.clone(),
        max_repos,
        roi_input,
    })?;
    let serialized = serde_json::to_string_pretty(&report)
        .context("failed to serialize dashboard report as JSON")?;

    if let Some(path) = output {
        dashboard::write_dashboard(&report, &path)?;
        println!(
            "Dashboard report written to {} (repos_scanned={}, in_scope={}, ready={}, avg_score={}%)",
            path.display(),
            report.repos_scanned,
            report.in_scope_repos,
            report.ready_repos,
            report.avg_readiness_score
        );
    } else {
        println!("{serialized}");
    }

    Ok(())
}

fn run_billing_plan_ls(
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let plans = if let Some(url) = billing_url.as_deref() {
        billing::list_plans_remote(url, auth_token)?
    } else {
        billing::list_plans(billing::StoreOptions {
            billing_root: billing_root.clone(),
        })?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&plans).context("failed to serialize plans as JSON")?
        );
        return Ok(());
    }

    if let Some(url) = billing_url {
        println!("Billing URL: {}", url);
    } else {
        let resolved = billing::resolve_billing_root(billing_root)?;
        println!("Billing root: {}", resolved.display());
    }
    if plans.is_empty() {
        println!("Plans: none");
        return Ok(());
    }
    println!("Plans:");
    for plan in plans {
        println!(
            "- {} ({}) ${:.2}/seat/month",
            plan.id,
            plan.name,
            f64::from(plan.price_per_seat_cents) / 100.0
        );
    }
    Ok(())
}

fn run_entitlement_check(
    org: String,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let report = if let Some(url) = billing_url.as_deref() {
        let subscriptions = billing::list_subscriptions_remote(
            url,
            auth_token,
            billing::ListFilter {
                org: Some(org.clone()),
            },
        )?;
        billing::entitlement_from_subscriptions(org.clone(), subscriptions)
    } else {
        billing::check_entitlement(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            &org,
        )?
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize entitlement report as JSON")?
        );
        return Ok(());
    }

    println!(
        "Entitlement org={} entitled={} reason={}",
        report.org, report.entitled, report.reason
    );
    if let Some(plan) = report.plan_id {
        println!("Plan: {}", plan);
    }
    if let Some(seats) = report.seats {
        println!("Seats: {}", seats);
    }
    Ok(())
}

fn run_billing_subscribe(
    org: String,
    plan: String,
    seats: u32,
    customer_email: Option<String>,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let input = billing::CreateSubscriptionInput {
        org,
        plan_id: plan,
        seats,
        customer_email,
    };
    let subscription = if let Some(url) = billing_url.as_deref() {
        billing::create_or_update_subscription_remote(url, auth_token, input)?
    } else {
        billing::create_or_update_subscription(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            input,
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&subscription)
                .context("failed to serialize subscription as JSON")?
        );
        return Ok(());
    }
    println!(
        "Subscription {} active for org={} plan={} seats={}",
        subscription.id, subscription.org, subscription.plan_id, subscription.seats
    );
    Ok(())
}

fn run_billing_subscription_ls(
    org: Option<String>,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let subscriptions = if let Some(url) = billing_url.as_deref() {
        billing::list_subscriptions_remote(
            url,
            auth_token,
            billing::ListFilter { org: org.clone() },
        )?
    } else {
        billing::list_subscriptions(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            billing::ListFilter { org: org.clone() },
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&subscriptions)
                .context("failed to serialize subscriptions as JSON")?
        );
        return Ok(());
    }
    if subscriptions.is_empty() {
        println!("Subscriptions: none");
        return Ok(());
    }
    println!("Subscriptions:");
    for sub in subscriptions {
        println!(
            "- {} org={} plan={} seats={} status={:?}",
            sub.id, sub.org, sub.plan_id, sub.seats, sub.status
        );
    }
    Ok(())
}

fn run_billing_cycle(
    at: Option<String>,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let result = if let Some(url) = billing_url.as_deref() {
        billing::run_cycle_remote(url, auth_token, at.as_deref())?
    } else {
        billing::run_cycle(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            at.as_deref(),
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).context("failed to serialize cycle result")?
        );
        return Ok(());
    }
    println!(
        "Billing cycle at {} created {} invoices and {} events",
        result.effective_at, result.invoices_created, result.events_created
    );
    Ok(())
}

fn run_billing_invoice_ls(
    org: Option<String>,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let invoices = if let Some(url) = billing_url.as_deref() {
        billing::list_invoices_remote(url, auth_token, billing::ListFilter { org: org.clone() })?
    } else {
        billing::list_invoices(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            billing::ListFilter { org: org.clone() },
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&invoices).context("failed to serialize invoices")?
        );
        return Ok(());
    }
    if invoices.is_empty() {
        println!("Invoices: none");
        return Ok(());
    }
    println!("Invoices:");
    for invoice in invoices {
        println!(
            "- {} org={} amount=${:.2} status={:?} due_at={}",
            invoice.id,
            invoice.org,
            f64::from(invoice.amount_cents) / 100.0,
            invoice.status,
            invoice.due_at
        );
    }
    Ok(())
}

fn run_billing_invoice_pay(
    invoice_id: String,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let invoice = if let Some(url) = billing_url.as_deref() {
        billing::mark_invoice_paid_remote(url, auth_token, &invoice_id)?
    } else {
        billing::mark_invoice_paid(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            &invoice_id,
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&invoice).context("failed to serialize paid invoice")?
        );
        return Ok(());
    }
    println!(
        "Invoice {} marked paid (org={}, amount=${:.2})",
        invoice.id,
        invoice.org,
        f64::from(invoice.amount_cents) / 100.0
    );
    Ok(())
}

fn run_billing_events(
    org: Option<String>,
    pending_only: bool,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let events = if let Some(url) = billing_url.as_deref() {
        billing::list_events_remote(
            url,
            auth_token,
            billing::ListFilter { org: org.clone() },
            pending_only,
        )?
    } else {
        billing::list_events(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            billing::ListFilter { org: org.clone() },
            pending_only,
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&events).context("failed to serialize events")?
        );
        return Ok(());
    }
    if events.is_empty() {
        println!("Billing events: none");
        return Ok(());
    }
    println!("Billing events:");
    for event in events {
        println!(
            "- {} {} org={} delivered={}",
            event.id,
            event.event_type,
            event.org,
            event.delivered_at.as_deref().unwrap_or("pending")
        );
    }
    Ok(())
}

fn run_billing_event_ack(
    event_id: String,
    billing_root: Option<PathBuf>,
    billing_url: Option<String>,
    auth_token: Option<String>,
    json: bool,
) -> Result<()> {
    let event = if let Some(url) = billing_url.as_deref() {
        billing::ack_event_remote(url, auth_token, &event_id)?
    } else {
        billing::ack_event(
            billing::StoreOptions {
                billing_root: billing_root.clone(),
            },
            &event_id,
        )?
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&event).context("failed to serialize event ack")?
        );
        return Ok(());
    }
    println!(
        "Event {} acknowledged at {}",
        event.id,
        event.delivered_at.as_deref().unwrap_or("-")
    );
    Ok(())
}

fn run_billing_serve(
    bind: String,
    billing_root: Option<PathBuf>,
    auth_token: Option<String>,
    auth_store: Option<PathBuf>,
    once: bool,
) -> Result<()> {
    let result = billing::serve_billing_http(billing::ServeOptions {
        billing_root,
        bind,
        auth_token,
        auth_store,
        once,
    })?;
    println!(
        "Billing server stopped (bind={}, root={}, handled={})",
        result.bind,
        result.billing_root.display(),
        result.requests_handled
    );
    Ok(())
}

fn run_auth_key_create(
    auth_store: Option<PathBuf>,
    input: auth::CreateApiKeyInput,
    json: bool,
) -> Result<()> {
    let path = auth::resolve_auth_store_path(auth_store)?;
    let created = auth::create_api_key(&path, input)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&created)
                .context("failed to serialize auth key as JSON")?
        );
        return Ok(());
    }

    println!("Created auth key {}", created.id);
    println!("Store: {}", path.display());
    println!("Token: {}", created.token);
    println!("Service: {}", created.service);
    println!("Scopes: {}", created.scopes.join(", "));
    if let Some(org) = created.org {
        println!("Org scope: {}", org);
    }
    if let Some(expires_at) = created.expires_at {
        println!("Expires at: {}", expires_at);
    }
    Ok(())
}

fn run_auth_key_ls(auth_store: Option<PathBuf>, json: bool) -> Result<()> {
    let path = auth::resolve_auth_store_path(auth_store)?;
    let keys = auth::list_api_keys(&path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&keys).context("failed to serialize auth keys as JSON")?
        );
        return Ok(());
    }

    println!("Auth store: {}", path.display());
    if keys.is_empty() {
        println!("Keys: none");
        return Ok(());
    }
    println!("Keys:");
    for key in keys {
        println!(
            "- {} subject={} service={} scopes={} active={} rpm={}{}",
            key.id,
            key.subject,
            key.service,
            key.scopes.join(","),
            key.active,
            key.rate_limit_per_minute,
            key.org
                .as_deref()
                .map(|org| format!(" org={}", org))
                .unwrap_or_default()
        );
    }
    Ok(())
}

fn run_auth_key_revoke(key_id: String, auth_store: Option<PathBuf>, json: bool) -> Result<()> {
    let path = auth::resolve_auth_store_path(auth_store)?;
    let revoked = auth::revoke_api_key(&path, &key_id)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&revoked)
                .context("failed to serialize revoked auth key as JSON")?
        );
        return Ok(());
    }

    println!(
        "Revoked key {} (subject={}, service={})",
        revoked.id, revoked.subject, revoked.service
    );
    Ok(())
}

fn run_doctor(project_path: &std::path::Path, json: bool, fail_on: FailOn) -> Result<()> {
    let lock_path = project_path.join("devsync.lock");
    let lock = if lock_path.is_file() {
        Some(read_lock(&lock_path)?)
    } else {
        None
    };

    let report = doctor::run_doctor(project_path, lock.as_ref())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize doctor report as JSON")?
        );
    } else {
        doctor::render_report(&report);
    }

    let policy = to_failure_policy(fail_on);
    let should_fail = doctor::report_should_fail(&report, policy);
    if !json && !report.healthy && !should_fail {
        println!(
            "Exit policy `{}` allows success despite warnings.",
            fail_on.as_str()
        );
    }

    if !should_fail {
        Ok(())
    } else {
        anyhow::bail!("doctor found issues")
    }
}

fn read_existing_lock(lock_path: &std::path::Path) -> Option<DevsyncLock> {
    if !lock_path.is_file() {
        return None;
    }

    read_lock(lock_path).ok()
}

fn to_failure_policy(fail_on: FailOn) -> doctor::FailurePolicy {
    match fail_on {
        FailOn::All => doctor::FailurePolicy::All,
        FailOn::Runtime => doctor::FailurePolicy::Runtime,
        FailOn::Lockfile => doctor::FailurePolicy::Lockfile,
        FailOn::Tooling => doctor::FailurePolicy::Tooling,
        FailOn::RuntimeAndLock => doctor::FailurePolicy::RuntimeAndLock,
        FailOn::None => doctor::FailurePolicy::None,
    }
}

fn print_detection_summary(detection: &detect::Detection) {
    println!("\nDetection summary\n-----------------");
    println!("Project: {}", detection.project_name);

    if detection.detected_stacks.is_empty() {
        println!("Stacks: none");
    } else {
        println!("Stacks: {}", detection.detected_stacks.join(", "));
    }

    println!(
        "Runtimes: node={:?}, python={:?}, rust={:?}",
        detection.node_version, detection.python_version, detection.rust_toolchain
    );

    println!(
        "Package managers: node={:?}, python={:?}",
        detection.node_package_manager, detection.python_package_manager
    );

    if detection.services.is_empty() {
        println!("Services: none detected");
    } else {
        println!("Services: {}", detection.services.join(", "));
    }

    if detection.run_hints.is_empty() {
        println!("Run hints: none");
    } else {
        println!("Run hints:");
        for hint in &detection.run_hints {
            println!("- {hint}");
        }
    }

    if let Some(primary) = &detection.primary_run_hint {
        println!("Primary run command: {primary}");
    }
    if let Some(primary_stack) = &detection.primary_stack {
        println!("Primary stack: {primary_stack}");
    }

    if !detection.recommendations.is_empty() {
        println!("\nRecommendations");
        for recommendation in &detection.recommendations {
            println!("- {recommendation}");
        }
    }
}

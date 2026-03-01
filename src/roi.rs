use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RoiInput {
    pub team_size: u32,
    pub monthly_hires: f64,
    pub onboarding_hours_before: f64,
    pub onboarding_hours_after: f64,
    pub drift_incidents_per_dev: f64,
    pub drift_hours_per_incident: f64,
    pub drift_reduction_pct: f64,
    pub hourly_rate: f64,
    pub price_per_dev: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoiReport {
    pub team_size: u32,
    pub recommended_plan: String,
    pub monthly_onboarding_cost_before: f64,
    pub monthly_onboarding_cost_after: f64,
    pub monthly_drift_cost_before: f64,
    pub monthly_drift_cost_after: f64,
    pub monthly_gross_savings: f64,
    pub monthly_subscription_cost: f64,
    pub monthly_net_savings: f64,
    pub roi_percent: f64,
}

pub fn compute_roi(input: &RoiInput) -> Result<RoiReport> {
    validate_input(input)?;

    let onboarding_cost_before =
        input.monthly_hires * input.onboarding_hours_before * input.hourly_rate;
    let onboarding_cost_after =
        input.monthly_hires * input.onboarding_hours_after * input.hourly_rate;

    let drift_hours_before =
        f64::from(input.team_size) * input.drift_incidents_per_dev * input.drift_hours_per_incident;
    let drift_hours_after = drift_hours_before * (1.0 - (input.drift_reduction_pct / 100.0));
    let drift_cost_before = drift_hours_before * input.hourly_rate;
    let drift_cost_after = drift_hours_after * input.hourly_rate;

    let gross_savings =
        (onboarding_cost_before - onboarding_cost_after) + (drift_cost_before - drift_cost_after);
    let subscription_cost = f64::from(input.team_size) * input.price_per_dev;
    let net_savings = gross_savings - subscription_cost;
    let roi_percent = if subscription_cost > 0.0 {
        (net_savings / subscription_cost) * 100.0
    } else {
        0.0
    };

    Ok(RoiReport {
        team_size: input.team_size,
        recommended_plan: recommended_plan(input.team_size).to_string(),
        monthly_onboarding_cost_before: round2(onboarding_cost_before),
        monthly_onboarding_cost_after: round2(onboarding_cost_after),
        monthly_drift_cost_before: round2(drift_cost_before),
        monthly_drift_cost_after: round2(drift_cost_after),
        monthly_gross_savings: round2(gross_savings),
        monthly_subscription_cost: round2(subscription_cost),
        monthly_net_savings: round2(net_savings),
        roi_percent: round2(roi_percent),
    })
}

pub fn render_report(report: &RoiReport) {
    println!("DevSync ROI\n===========");
    println!("Team size: {}", report.team_size);
    println!("Recommended plan: {}", report.recommended_plan);
    println!();
    println!(
        "Onboarding cost: ${:.2} -> ${:.2} / month",
        report.monthly_onboarding_cost_before, report.monthly_onboarding_cost_after
    );
    println!(
        "Drift cost: ${:.2} -> ${:.2} / month",
        report.monthly_drift_cost_before, report.monthly_drift_cost_after
    );
    println!(
        "Gross savings: ${:.2} / month",
        report.monthly_gross_savings
    );
    println!(
        "DevSync cost: ${:.2} / month",
        report.monthly_subscription_cost
    );
    println!("Net savings: ${:.2} / month", report.monthly_net_savings);
    println!("ROI: {:.2}%", report.roi_percent);
}

fn validate_input(input: &RoiInput) -> Result<()> {
    if input.team_size == 0 {
        bail!("team_size must be greater than zero");
    }
    for (label, value) in [
        ("monthly_hires", input.monthly_hires),
        ("onboarding_hours_before", input.onboarding_hours_before),
        ("onboarding_hours_after", input.onboarding_hours_after),
        ("drift_incidents_per_dev", input.drift_incidents_per_dev),
        ("drift_hours_per_incident", input.drift_hours_per_incident),
        ("hourly_rate", input.hourly_rate),
        ("price_per_dev", input.price_per_dev),
    ] {
        if value < 0.0 {
            bail!("{label} must be non-negative");
        }
    }
    if !(0.0..=100.0).contains(&input.drift_reduction_pct) {
        bail!("drift_reduction_pct must be between 0 and 100");
    }
    Ok(())
}

fn recommended_plan(team_size: u32) -> &'static str {
    if team_size <= 50 {
        "Team"
    } else if team_size <= 200 {
        "Business"
    } else {
        "Enterprise"
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_positive_roi_case() {
        let report = compute_roi(&RoiInput {
            team_size: 20,
            monthly_hires: 2.0,
            onboarding_hours_before: 8.0,
            onboarding_hours_after: 2.0,
            drift_incidents_per_dev: 0.5,
            drift_hours_per_incident: 2.0,
            drift_reduction_pct: 50.0,
            hourly_rate: 100.0,
            price_per_dev: 15.0,
        })
        .expect("roi should compute");

        assert_eq!(report.recommended_plan, "Team");
        assert!(report.monthly_gross_savings > report.monthly_subscription_cost);
        assert!(report.roi_percent > 0.0);
    }

    #[test]
    fn rejects_invalid_reduction_percent() {
        let err = compute_roi(&RoiInput {
            team_size: 10,
            monthly_hires: 1.0,
            onboarding_hours_before: 6.0,
            onboarding_hours_after: 2.0,
            drift_incidents_per_dev: 0.5,
            drift_hours_per_incident: 1.0,
            drift_reduction_pct: 150.0,
            hourly_rate: 80.0,
            price_per_dev: 12.0,
        })
        .expect_err("invalid percent should fail");
        assert!(
            err.to_string()
                .contains("drift_reduction_pct must be between 0 and 100")
        );
    }
}

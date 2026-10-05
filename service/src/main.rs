mod arbor;
mod money;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use tracing_subscriber::EnvFilter;

use arbor::{ArborClient, Purchase};

/// Each child's meal account is topped up to this balance every week.
const TARGET_BALANCE_PENCE: i64 = 1600;

#[derive(Serialize)]
struct ChildSnapshot {
    student: String,
    account_id: u64,
    balance_pence: i64,
    top_up_pence: i64,
    days: Vec<DaySnapshot>,
}

#[derive(Serialize)]
struct DaySnapshot {
    date: NaiveDate,
    total_pence: i64,
    purchases: Vec<Purchase>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let base_url = env("ARBOR_BASE_URL")?;
    let email = env("ARBOR_EMAIL")?;
    let password = env("ARBOR_PASSWORD")?;

    let client = ArborClient::new(base_url)?;
    client.login(&email, &password).await?;
    tracing::info!("logged in to Arbor");

    let mut snapshot = Vec::new();
    for account in client.meal_accounts().await? {
        let mut days = Vec::new();
        for day in client.day_spends(account.account_id).await? {
            let purchases = client.purchases(account.account_id, day.date).await?;
            days.push(DaySnapshot {
                date: day.date,
                total_pence: day.total_pence,
                purchases,
            });
            // Be gentle with Arbor: these are one request per day of the term.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        tracing::info!(student = %account.student, balance_pence = account.balance_pence, days = days.len(), "read account");
        snapshot.push(ChildSnapshot {
            top_up_pence: (TARGET_BALANCE_PENCE - account.balance_pence).max(0),
            student: account.student,
            account_id: account.account_id,
            balance_pence: account.balance_pence,
            days,
        });
    }

    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("environment variable {name} is not set"))
}

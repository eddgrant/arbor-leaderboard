mod arbor;
mod categories;
mod config;
mod money;
mod mqtt;
mod stats;
mod store;
mod sync;

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use chrono_tz::Europe::London;
use tracing_subscriber::EnvFilter;

use arbor::ArborClient;
use config::Config;
use mqtt::Publisher;
use stats::{ChildStats, child_stats};
use store::Store;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let once = std::env::args().any(|a| a == "--once");
    let config = Config::from_env()?;
    let client = ArborClient::new(&config.arbor_base_url)?;
    let mut store = Store::open(&config.db_path)?;
    let publisher = match &config.mqtt {
        Some(mqtt) => Some(Publisher::connect(mqtt).await?),
        None => None,
    };

    loop {
        match run_once(&config, &client, &mut store, publisher.as_ref()).await {
            Ok(()) => {}
            // A failed fetch leaves "Last successful fetch" stale, which Home Assistant can alert on.
            Err(e) if !once => tracing::error!(error = format!("{e:#}"), "fetch failed"),
            Err(e) => return Err(e),
        }
        if once {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(config.fetch_interval) => {}
            _ = shutdown_signal() => break,
        }
    }

    if let Some(publisher) = publisher {
        publisher.shutdown().await;
    }
    Ok(())
}

async fn run_once(
    config: &Config,
    client: &ArborClient,
    store: &mut Store,
    publisher: Option<&Publisher>,
) -> Result<()> {
    let today = today_in_london();
    let synced = sync::sync(
        client,
        store,
        &config.arbor_email,
        &config.arbor_password,
        today,
    )
    .await?;

    let mut all_stats: Vec<ChildStats> = Vec::new();
    for s in &synced {
        all_stats.push(child_stats(
            store,
            &s.account,
            today,
            s.term_start,
            config.target_balance_pence,
        )?);
    }

    match publisher {
        Some(publisher) => {
            for stats in &all_stats {
                publisher.publish_child(stats).await?;
            }
            publisher.publish_last_success(Utc::now()).await?;
            tracing::info!(children = all_stats.len(), "published to MQTT");
        }
        None => println!("{}", serde_json::to_string_pretty(&all_stats)?),
    }
    Ok(())
}

fn today_in_london() -> NaiveDate {
    Utc::now().with_timezone(&London).date_naive()
}

async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("installing SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    tracing::info!("shutting down");
}

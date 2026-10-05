use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::mqtt::MqttConfig;

pub struct Config {
    pub arbor_base_url: String,
    pub arbor_email: String,
    pub arbor_password: String,
    pub db_path: PathBuf,
    /// Without MQTT settings the service prints its figures as JSON instead of publishing them.
    pub mqtt: Option<MqttConfig>,
    pub fetch_interval: Duration,
    pub target_balance_pence: i64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let mqtt = match optional("MQTT_HOST") {
            Some(host) => Some(MqttConfig {
                host,
                port: parsed("MQTT_PORT", 1883)?,
                username: optional("MQTT_USERNAME"),
                password: optional("MQTT_PASSWORD"),
            }),
            None => None,
        };
        Ok(Self {
            arbor_base_url: required("ARBOR_BASE_URL")?,
            arbor_email: required("ARBOR_EMAIL")?,
            arbor_password: required("ARBOR_PASSWORD")?,
            db_path: optional("DB_PATH")
                .unwrap_or_else(|| "arbor.db".into())
                .into(),
            mqtt,
            fetch_interval: Duration::from_secs(60 * parsed("FETCH_INTERVAL_MINUTES", 120)?),
            target_balance_pence: parsed("TARGET_BALANCE_PENCE", 1600)?,
        })
    }
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("environment variable {name} is not set"))
}

fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn parsed<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match optional(name) {
        Some(v) => v
            .parse()
            .with_context(|| format!("{name} has an invalid value: {v}")),
        None => Ok(default),
    }
}

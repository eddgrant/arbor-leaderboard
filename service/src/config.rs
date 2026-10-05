use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::categories::Categories;
use crate::money::parse_pence;
use crate::mqtt::MqttConfig;

/// Default balance each meal account is topped up to: £16.
const DEFAULT_TARGET_BALANCE_PENCE: i64 = 1600;

pub struct Config {
    pub arbor_base_url: String,
    pub arbor_email: String,
    pub arbor_password: String,
    pub db_path: PathBuf,
    /// Without MQTT settings the service prints its figures as JSON instead of publishing them.
    pub mqtt: Option<MqttConfig>,
    pub fetch_interval: Duration,
    pub target_balance_pence: i64,
    /// Port for the `/health` endpoint. Not served with `--once`.
    pub http_port: u16,
    /// From `CATEGORIES_FILE`, or the built-in defaults.
    pub categories: Categories,
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
            target_balance_pence: target_balance_pence(optional("TARGET_BALANCE"))?,
            http_port: parsed("HTTP_PORT", 8080)?,
            categories: Categories::load(
                optional("CATEGORIES_FILE").map(PathBuf::from).as_deref(),
            )?,
        })
    }
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("environment variable {name} is not set"))
}

/// Parses `TARGET_BALANCE`, given in pounds such as "16", "12.50" or "£16".
fn target_balance_pence(value: Option<String>) -> Result<i64> {
    let Some(value) = value else {
        return Ok(DEFAULT_TARGET_BALANCE_PENCE);
    };
    match parse_pence(&value) {
        Some(pence) if pence > 0 => Ok(pence),
        _ => bail!("TARGET_BALANCE must be an amount in pounds such as 16 or 12.50, not {value:?}"),
    }
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

#[cfg(test)]
mod tests {
    use super::target_balance_pence;

    #[test]
    fn target_balance_is_given_in_pounds() {
        assert_eq!(target_balance_pence(None).unwrap(), 1600);
        assert_eq!(target_balance_pence(Some("20".into())).unwrap(), 2000);
        assert_eq!(target_balance_pence(Some("12.50".into())).unwrap(), 1250);
        assert_eq!(target_balance_pence(Some("£16".into())).unwrap(), 1600);
        assert!(target_balance_pence(Some("lots".into())).is_err());
        assert!(target_balance_pence(Some("0".into())).is_err());
        assert!(target_balance_pence(Some("-5".into())).is_err());
    }
}

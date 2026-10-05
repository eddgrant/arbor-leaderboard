//! Publishes figures to Home Assistant over MQTT, using MQTT discovery so the entities appear
//! without any Home Assistant configuration.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Outgoing, QoS};
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use crate::categories::{Categories, Category};
use crate::stats::ChildStats;

const DISCOVERY_PREFIX: &str = "homeassistant";
const BASE_TOPIC: &str = "arbor-leaderboard";
const STATUS_TOPIC: &str = "arbor-leaderboard/status";

pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub struct Publisher {
    client: AsyncClient,
    event_loop: JoinHandle<()>,
}

struct SensorSpec {
    key: &'static str,
    name: &'static str,
    unit: Option<&'static str>,
    device_class: Option<&'static str>,
    state_class: Option<&'static str>,
    icon: Option<&'static str>,
}

/// A sensor as published, built from `CHILD_SENSORS` plus one per configured category.
struct Sensor {
    key: String,
    name: String,
    value_template: String,
    unit: Option<&'static str>,
    device_class: Option<&'static str>,
    state_class: Option<&'static str>,
    icon: Option<String>,
}

const CHILD_SENSORS: &[SensorSpec] = &[
    SensorSpec {
        key: "balance",
        name: "Balance",
        unit: Some("GBP"),
        device_class: Some("monetary"),
        state_class: Some("total"),
        icon: None,
    },
    SensorSpec {
        key: "top_up",
        name: "Top-up needed",
        unit: Some("GBP"),
        device_class: Some("monetary"),
        state_class: None,
        icon: Some("mdi:cash-plus"),
    },
    SensorSpec {
        key: "week_spend",
        name: "Spend this week",
        unit: Some("GBP"),
        device_class: Some("monetary"),
        state_class: Some("total"),
        icon: None,
    },
    SensorSpec {
        key: "term_spend",
        name: "Spend this term",
        unit: Some("GBP"),
        device_class: Some("monetary"),
        state_class: Some("total"),
        icon: None,
    },
    SensorSpec {
        key: "term_items",
        name: "Items this term",
        unit: Some("items"),
        device_class: None,
        state_class: Some("measurement"),
        icon: Some("mdi:food"),
    },
    SensorSpec {
        key: "last_purchase_date",
        name: "Last purchase",
        unit: None,
        device_class: Some("date"),
        state_class: None,
        icon: None,
    },
];

impl Publisher {
    pub async fn connect(config: &MqttConfig) -> Result<Self> {
        let mut options = MqttOptions::new("arbor-leaderboard", &config.host, config.port);
        options.set_keep_alive(Duration::from_secs(60));
        options.set_last_will(LastWill::new(
            STATUS_TOPIC,
            "offline",
            QoS::AtLeastOnce,
            true,
        ));
        if let (Some(user), Some(pass)) = (&config.username, &config.password) {
            options.set_credentials(user, pass);
        }
        let (client, mut event_loop) = AsyncClient::new(options, 64);

        // rumqttc only makes progress while its event loop is polled, and reconnects on its own.
        let event_loop = tokio::spawn(async move {
            loop {
                match event_loop.poll().await {
                    Ok(Event::Outgoing(Outgoing::Disconnect)) => break,
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "MQTT connection error; retrying");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let publisher = Self { client, event_loop };
        publisher
            .publish_retained(STATUS_TOPIC, "online".into())
            .await?;
        publisher.publish_service_discovery().await?;
        Ok(publisher)
    }

    pub async fn publish_child(&self, stats: &ChildStats, categories: &Categories) -> Result<()> {
        let state_topic = format!("{BASE_TOPIC}/{}/state", stats.account_id);
        for (topic, payload) in child_discovery(stats, &state_topic, categories) {
            self.publish_retained(&topic, payload.to_string()).await?;
        }
        self.publish_retained(&state_topic, serde_json::to_string(stats)?)
            .await
    }

    /// Removes a sensor from Home Assistant, e.g. for a category that's no longer configured.
    pub async fn remove_child_sensor(&self, account_id: u64, key: &str) -> Result<()> {
        self.publish_retained(&discovery_topic(account_id, key), String::new())
            .await
    }

    pub async fn publish_last_success(&self, at: DateTime<Utc>) -> Result<()> {
        let payload = json!({ "last_success": at.to_rfc3339() });
        self.publish_retained(&format!("{BASE_TOPIC}/service/state"), payload.to_string())
            .await
    }

    /// Marks the service offline and flushes queued messages before returning.
    pub async fn shutdown(self) {
        let _ = self.publish_retained(STATUS_TOPIC, "offline".into()).await;
        let _ = self.client.disconnect().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), self.event_loop).await;
    }

    async fn publish_service_discovery(&self) -> Result<()> {
        let payload = json!({
            "name": "Last successful fetch",
            "unique_id": "arbor_leaderboard_last_success",
            "state_topic": format!("{BASE_TOPIC}/service/state"),
            "value_template": "{{ value_json.last_success }}",
            "device_class": "timestamp",
            "availability_topic": STATUS_TOPIC,
            "device": {
                "identifiers": ["arbor_leaderboard_service"],
                "name": "Arbor leaderboard",
                "manufacturer": "arbor-leaderboard",
                "sw_version": env!("CARGO_PKG_VERSION"),
            },
        });
        self.publish_retained(
            &format!("{DISCOVERY_PREFIX}/sensor/arbor_leaderboard/last_success/config"),
            payload.to_string(),
        )
        .await
    }

    async fn publish_retained(&self, topic: &str, payload: String) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, true, payload)
            .await?;
        Ok(())
    }
}

/// The discovery topic key for a category's sensor, e.g. "term_puddings".
pub fn category_sensor_key(category: &Category) -> String {
    format!("term_{}", category.key)
}

fn child_sensors(categories: &Categories) -> Vec<Sensor> {
    let fixed = CHILD_SENSORS.iter().map(|s| Sensor {
        key: s.key.to_string(),
        name: s.name.to_string(),
        value_template: format!("{{{{ value_json.{} }}}}", s.key),
        unit: s.unit,
        device_class: s.device_class,
        state_class: s.state_class,
        icon: s.icon.map(str::to_string),
    });
    let per_category = categories.iter().map(|c| Sensor {
        key: category_sensor_key(c),
        name: format!("{} this term", c.name),
        value_template: format!("{{{{ value_json.categories['{}'] }}}}", c.key),
        unit: Some("items"),
        device_class: None,
        state_class: Some("measurement"),
        icon: Some(c.icon.clone().unwrap_or_else(|| "mdi:tag".into())),
    });
    fixed.chain(per_category).collect()
}

fn discovery_topic(account_id: u64, key: &str) -> String {
    format!("{DISCOVERY_PREFIX}/sensor/arbor_{account_id}/{key}/config")
}

/// Discovery messages for one child's sensors, as (topic, payload) pairs.
fn child_discovery(
    stats: &ChildStats,
    state_topic: &str,
    categories: &Categories,
) -> Vec<(String, Value)> {
    let node_id = format!("arbor_{}", stats.account_id);
    let device = json!({
        "identifiers": [node_id],
        "name": format!("Arbor {}", stats.student),
        "manufacturer": "Arbor Education",
        "model": "Meal account",
    });
    child_sensors(categories)
        .into_iter()
        .map(|s| {
            let mut payload = json!({
                "name": s.name,
                "unique_id": format!("{node_id}_{}", s.key),
                "state_topic": state_topic,
                "value_template": s.value_template,
                "availability_topic": STATUS_TOPIC,
                "device": device,
            });
            let fields = [
                ("unit_of_measurement", s.unit.map(str::to_string)),
                ("device_class", s.device_class.map(str::to_string)),
                ("state_class", s.state_class.map(str::to_string)),
                ("icon", s.icon),
            ];
            for (field, value) in fields {
                if let Some(value) = value {
                    payload[field] = json!(value);
                }
            }
            if s.key == "last_purchase_date" {
                // Exposes the items as an attribute: {{ state_attr(entity, 'items') }}.
                payload["json_attributes_topic"] = json!(state_topic);
                payload["json_attributes_template"] =
                    json!("{{ {'items': value_json.last_purchase_items} | tojson }}");
            }
            (discovery_topic(stats.account_id, &s.key), payload)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn categories() -> Categories {
        Categories::from_yaml("categories:\n  puddings:\n    icon: mdi:cupcake\n    keywords: [cake]\n  pizza: [pizza]\n").unwrap()
    }

    fn stats() -> ChildStats {
        ChildStats {
            account_id: 42,
            student: "Child A".into(),
            balance: 3.4,
            top_up: 12.6,
            week_spend: 5.2,
            term_spend: 9.35,
            term_items: 5,
            categories: [("puddings".to_string(), 2), ("pizza".to_string(), 1)].into(),
            last_purchase_date: None,
            last_purchase_items: vec![],
        }
    }

    #[test]
    fn discovery_describes_each_sensor_on_one_device() {
        let messages = child_discovery(&stats(), "arbor-leaderboard/42/state", &categories());
        assert_eq!(messages.len(), CHILD_SENSORS.len() + 2);

        let (topic, balance) = &messages[0];
        assert_eq!(topic, "homeassistant/sensor/arbor_42/balance/config");
        assert_eq!(balance["unique_id"], "arbor_42_balance");
        assert_eq!(balance["value_template"], "{{ value_json.balance }}");
        assert_eq!(balance["unit_of_measurement"], "GBP");
        assert_eq!(balance["device"]["name"], "Arbor Child A");
        assert!(balance.get("icon").is_none());
    }

    #[test]
    fn each_category_gets_a_sensor_with_a_stable_id() {
        let messages = child_discovery(&stats(), "arbor-leaderboard/42/state", &categories());
        let (topic, puddings) = &messages[CHILD_SENSORS.len()];
        assert_eq!(topic, "homeassistant/sensor/arbor_42/term_puddings/config");
        // Same unique_id as before categories were configurable, so existing entities carry on.
        assert_eq!(puddings["unique_id"], "arbor_42_term_puddings");
        assert_eq!(puddings["name"], "Puddings this term");
        assert_eq!(
            puddings["value_template"],
            "{{ value_json.categories['puddings'] }}"
        );
        assert_eq!(puddings["icon"], "mdi:cupcake");

        let (_, pizza) = &messages[CHILD_SENSORS.len() + 1];
        assert_eq!(pizza["name"], "Pizza this term");
        assert_eq!(pizza["icon"], "mdi:tag");
    }

    #[test]
    fn every_value_template_refers_to_a_state_field() {
        let state = serde_json::to_value(stats()).unwrap();
        for spec in CHILD_SENSORS {
            assert!(
                state.get(spec.key).is_some(),
                "state has no field {}",
                spec.key
            );
        }
        for category in categories().iter() {
            assert!(state["categories"].get(&category.key).is_some());
        }
    }
}

//! Extracts data from Arbor's `?format=javascript` responses.
//!
//! These responses describe UI component trees rather than data, so each parser walks the tree
//! looking for the `mis-property-row` components that carry the values we want.

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

use crate::money::parse_pence;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MealAccount {
    pub account_id: u64,
    pub student: String,
    pub balance_pence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DaySpend {
    pub date: NaiveDate,
    pub total_pence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Purchase {
    pub name: String,
    pub price_pence: i64,
}

/// Meal accounts and balances from the guardian home dashboard (`/guardians/home-ui/dashboard`).
pub fn meal_accounts(dashboard: &Value) -> Vec<MealAccount> {
    let mut accounts = Vec::new();
    for_each_property_row(dashboard, &mut |props| {
        let Some(label) = props.get("value").and_then(Value::as_str).map(strip_html) else {
            return;
        };
        let Some(student) = label.strip_suffix(": Meals") else {
            return;
        };
        let account_id = props
            .get("url")
            .and_then(Value::as_str)
            .and_then(account_id_from_url);
        let balance = props
            .get("description")
            .and_then(Value::as_str)
            .and_then(|d| d.strip_prefix("Balance:"))
            .and_then(parse_pence);
        if let (Some(account_id), Some(balance_pence)) = (account_id, balance) {
            accounts.push(MealAccount {
                account_id,
                student: student.trim().to_string(),
                balance_pence,
            });
        }
    });
    accounts
}

/// Daily spend totals for the current term from an account dashboard
/// (`/guardians/customer-account-ui/dashboard/customer-account-id/{id}`).
pub fn day_spends(account_dashboard: &Value) -> Vec<DaySpend> {
    let mut days = Vec::new();
    for_each_property_row(account_dashboard, &mut |props| {
        let date = props
            .get("url")
            .and_then(Value::as_str)
            .and_then(date_from_url);
        let total = props
            .get("value")
            .and_then(Value::as_str)
            .and_then(parse_pence);
        if let (Some(date), Some(total_pence)) = (date, total) {
            days.push(DaySpend { date, total_pence });
        }
    });
    days.sort_by_key(|d| d.date);
    days.dedup_by_key(|d| d.date);
    days
}

/// Individual purchases from a day view
/// (`/guardians/customer-account-ui/view-payments-on-date/date/{date}/customer-account-id/{id}`).
pub fn purchases(day: &Value) -> Vec<Purchase> {
    let mut items = Vec::new();
    for_each_property_row(day, &mut |props| {
        let Some(label) = props.get("fieldLabel").and_then(Value::as_str) else {
            return;
        };
        let Some(value) = props.get("value").and_then(Value::as_str) else {
            return;
        };
        let Some(price_pence) = parse_pence(value) else {
            return;
        };
        // Labels repeat the price, e.g. "Panini £2.55" with value "£2.55".
        let name = label
            .trim()
            .strip_suffix(value.trim())
            .unwrap_or(label)
            .trim();
        items.push(Purchase {
            name: name.to_string(),
            price_pence,
        });
    });
    items
}

fn for_each_property_row(node: &Value, f: &mut impl FnMut(&serde_json::Map<String, Value>)) {
    match node {
        Value::Array(items) => items.iter().for_each(|n| for_each_property_row(n, f)),
        Value::Object(map) => {
            if map.get("xtype").and_then(Value::as_str) == Some("mis-property-row")
                && let Some(Value::Object(props)) = map.get("props")
            {
                f(props);
            }
            map.values().for_each(|n| for_each_property_row(n, f));
        }
        _ => {}
    }
}

fn account_id_from_url(url: &str) -> Option<u64> {
    let rest = url.split("customer-account-id/").nth(1)?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn date_from_url(url: &str) -> Option<NaiveDate> {
    let rest = url.split("/date/").nth(1)?;
    NaiveDate::parse_from_str(rest.get(..10)?, "%Y-%m-%d").ok()
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn reads_meal_accounts_from_home_dashboard() {
        let accounts = meal_accounts(&fixture("home-dashboard.json"));
        assert_eq!(
            accounts,
            vec![
                MealAccount {
                    account_id: 101,
                    student: "Child A".into(),
                    balance_pence: 1340
                },
                MealAccount {
                    account_id: 102,
                    student: "Child B".into(),
                    balance_pence: -120
                },
            ]
        );
    }

    #[test]
    fn reads_day_spends_from_account_dashboard() {
        let days = day_spends(&fixture("account-dashboard.json"));
        assert_eq!(
            days,
            vec![
                DaySpend {
                    date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
                    total_pence: 365
                },
                DaySpend {
                    date: NaiveDate::from_ymd_opt(2026, 9, 29).unwrap(),
                    total_pence: 415
                },
            ]
        );
    }

    #[test]
    fn reads_purchases_from_day_view() {
        let items = purchases(&fixture("day.json"));
        assert_eq!(
            items,
            vec![
                Purchase {
                    name: "pizza veggie".into(),
                    price_pence: 160
                },
                Purchase {
                    name: "Panini".into(),
                    price_pence: 255
                },
            ]
        );
    }
}

//! Per-child figures published to Home Assistant, calculated from stored purchases.

use anyhow::Result;
use chrono::{Datelike, Days, NaiveDate};
use serde::Serialize;

use crate::arbor::MealAccount;
use crate::categories::{Category, categorise};
use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChildStats {
    pub account_id: u64,
    pub student: String,
    /// Pounds, as Home Assistant expects for monetary sensors.
    pub balance: f64,
    pub top_up: f64,
    pub week_spend: f64,
    pub term_spend: f64,
    pub term_items: usize,
    pub term_puddings: usize,
    pub term_drinks: usize,
    pub last_purchase_date: Option<NaiveDate>,
    pub last_purchase_items: Vec<String>,
}

/// `term_start` is the first day Arbor lists for the current term; weeks start on Monday.
pub fn child_stats(
    store: &Store,
    account: &MealAccount,
    today: NaiveDate,
    term_start: NaiveDate,
    target_balance_pence: i64,
) -> Result<ChildStats> {
    let week_start = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let term = store.purchases_between(account.account_id, term_start, today)?;

    let week_spend: i64 = term
        .iter()
        .filter(|p| p.date >= week_start)
        .map(|p| p.purchase.price_pence)
        .sum();
    let count = |c: Category| {
        term.iter()
            .filter(|p| categorise(&p.purchase.name) == c)
            .count()
    };
    let last_purchase_date = term.last().map(|p| p.date);
    let last_purchase_items = term
        .iter()
        .filter(|p| Some(p.date) == last_purchase_date)
        .map(|p| p.purchase.name.clone())
        .collect();

    Ok(ChildStats {
        account_id: account.account_id,
        student: account.student.clone(),
        balance: pounds(account.balance_pence),
        top_up: pounds((target_balance_pence - account.balance_pence).max(0)),
        week_spend: pounds(week_spend),
        term_spend: pounds(term.iter().map(|p| p.purchase.price_pence).sum()),
        term_items: term.len(),
        term_puddings: count(Category::Pudding),
        term_drinks: count(Category::Drink),
        last_purchase_date,
        last_purchase_items,
    })
}

fn pounds(pence: i64) -> f64 {
    pence as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arbor::Purchase;
    use chrono::Utc;

    fn date(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn item(name: &str, price_pence: i64) -> Purchase {
        Purchase {
            name: name.into(),
            price_pence,
        }
    }

    #[test]
    fn calculates_week_and_term_figures() {
        let mut store = Store::open_in_memory().unwrap();
        let account = MealAccount {
            account_id: 1,
            student: "Child A".into(),
            balance_pence: 340,
        };
        store.record_balance(&account, Utc::now()).unwrap();
        // Previous week (Mon 28 Sep) and this week (Mon 5 Oct, Tue 6 Oct).
        store
            .replace_day(
                1,
                date(9, 28),
                415,
                &[item("Panini", 255), item("traybake", 160)],
            )
            .unwrap();
        store
            .replace_day(1, date(10, 5), 255, &[item("Panini", 255)])
            .unwrap();
        store
            .replace_day(
                1,
                date(10, 6),
                265,
                &[item("cupcake", 160), item("milkshake 1.05", 105)],
            )
            .unwrap();

        let stats = child_stats(&store, &account, date(10, 7), date(9, 1), 1600).unwrap();

        assert_eq!(stats.balance, 3.40);
        assert_eq!(stats.top_up, 12.60);
        assert_eq!(stats.week_spend, 5.20);
        assert_eq!(stats.term_spend, 9.35);
        assert_eq!(stats.term_items, 5);
        assert_eq!(stats.term_puddings, 2);
        assert_eq!(stats.term_drinks, 1);
        assert_eq!(stats.last_purchase_date, Some(date(10, 6)));
        assert_eq!(stats.last_purchase_items, ["cupcake", "milkshake 1.05"]);
    }

    #[test]
    fn no_top_up_when_balance_is_at_or_above_target() {
        let store = Store::open_in_memory().unwrap();
        let account = MealAccount {
            account_id: 1,
            student: "Child A".into(),
            balance_pence: 1750,
        };
        store.record_balance(&account, Utc::now()).unwrap();

        let stats = child_stats(&store, &account, date(10, 4), date(9, 1), 1600).unwrap();
        assert_eq!(stats.top_up, 0.0);
        assert_eq!(stats.last_purchase_date, None);
    }
}

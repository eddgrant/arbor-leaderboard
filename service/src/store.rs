//! SQLite storage for balances and purchases.
//!
//! Arbor only shows the current term, so this is where history accumulates. It also lets each
//! fetch skip days whose purchases are already stored.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::arbor::{MealAccount, Purchase};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    account_id INTEGER PRIMARY KEY,
    student    TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS balances (
    account_id    INTEGER NOT NULL REFERENCES accounts(account_id),
    fetched_at    TEXT    NOT NULL,
    balance_pence INTEGER NOT NULL,
    PRIMARY KEY (account_id, fetched_at)
);
CREATE TABLE IF NOT EXISTS day_totals (
    account_id  INTEGER NOT NULL REFERENCES accounts(account_id),
    date        TEXT    NOT NULL,
    total_pence INTEGER NOT NULL,
    PRIMARY KEY (account_id, date)
);
-- Category sensors last published to Home Assistant per account, so sensors for categories
-- that are no longer configured can be removed.
CREATE TABLE IF NOT EXISTS published_category_sensors (
    account_id INTEGER NOT NULL,
    sensor_key TEXT    NOT NULL,
    PRIMARY KEY (account_id, sensor_key)
);
CREATE TABLE IF NOT EXISTS purchases (
    account_id  INTEGER NOT NULL REFERENCES accounts(account_id),
    date        TEXT    NOT NULL,
    position    INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    price_pence INTEGER NOT NULL,
    PRIMARY KEY (account_id, date, position)
);
";

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatedPurchase {
    pub date: NaiveDate,
    pub purchase: Purchase,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Records the account and a balance reading taken at `at`.
    pub fn record_balance(&self, account: &MealAccount, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO accounts (account_id, student) VALUES (?1, ?2)
             ON CONFLICT (account_id) DO UPDATE SET student = excluded.student",
            params![account.account_id as i64, account.student],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO balances (account_id, fetched_at, balance_pence) VALUES (?1, ?2, ?3)",
            params![account.account_id as i64, at.to_rfc3339(), account.balance_pence],
        )?;
        Ok(())
    }

    /// The day total stored with the last fetch of that day's purchases, if any.
    pub fn day_total(&self, account_id: u64, date: NaiveDate) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT total_pence FROM day_totals WHERE account_id = ?1 AND date = ?2",
                params![account_id as i64, date.to_string()],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Replaces a day's purchases and records the day total they were fetched against.
    pub fn replace_day(
        &mut self,
        account_id: u64,
        date: NaiveDate,
        total_pence: i64,
        purchases: &[Purchase],
    ) -> Result<()> {
        let date = date.to_string();
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM purchases WHERE account_id = ?1 AND date = ?2",
            params![account_id as i64, date],
        )?;
        for (position, p) in purchases.iter().enumerate() {
            tx.execute(
                "INSERT INTO purchases (account_id, date, position, name, price_pence)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    account_id as i64,
                    date,
                    position as i64,
                    p.name,
                    p.price_pence
                ],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO day_totals (account_id, date, total_pence) VALUES (?1, ?2, ?3)",
            params![account_id as i64, date, total_pence],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Records the category sensors now published for an account, returning the ones that were
    /// published before but aren't any more.
    pub fn replace_published_category_sensors(
        &mut self,
        account_id: u64,
        sensor_keys: &[String],
    ) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let previous: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT sensor_key FROM published_category_sensors WHERE account_id = ?1",
            )?;
            stmt.query_map(params![account_id as i64], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        tx.execute(
            "DELETE FROM published_category_sensors WHERE account_id = ?1",
            params![account_id as i64],
        )?;
        for key in sensor_keys {
            tx.execute(
                "INSERT INTO published_category_sensors (account_id, sensor_key) VALUES (?1, ?2)",
                params![account_id as i64, key],
            )?;
        }
        tx.commit()?;
        Ok(previous
            .into_iter()
            .filter(|k| !sensor_keys.contains(k))
            .collect())
    }

    /// Purchases for an account with dates in `from..=to`, oldest first.
    pub fn purchases_between(
        &self,
        account_id: u64,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DatedPurchase>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, name, price_pence FROM purchases
             WHERE account_id = ?1 AND date BETWEEN ?2 AND ?3
             ORDER BY date, position",
        )?;
        let rows = stmt.query_map(
            params![account_id as i64, from.to_string(), to.to_string()],
            |row| {
                let date: String = row.get(0)?;
                Ok((date, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
            },
        )?;
        rows.map(|row| {
            let (date, name, price_pence) = row?;
            Ok(DatedPurchase {
                date: date
                    .parse()
                    .with_context(|| format!("bad date {date} in database"))?,
                purchase: Purchase { name, price_pence },
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }

    fn item(name: &str, price_pence: i64) -> Purchase {
        Purchase {
            name: name.into(),
            price_pence,
        }
    }

    fn store_with_account() -> Store {
        let store = Store::open_in_memory().unwrap();
        let account = MealAccount {
            account_id: 1,
            student: "Child A".into(),
            balance_pence: 500,
        };
        store.record_balance(&account, Utc::now()).unwrap();
        store
    }

    #[test]
    fn replacing_a_day_overwrites_its_purchases() {
        let mut store = store_with_account();
        store
            .replace_day(1, date(28), 160, &[item("pizza", 160)])
            .unwrap();
        store
            .replace_day(1, date(28), 415, &[item("pizza", 160), item("panini", 255)])
            .unwrap();

        assert_eq!(store.day_total(1, date(28)).unwrap(), Some(415));
        let names: Vec<_> = store
            .purchases_between(1, date(28), date(28))
            .unwrap()
            .into_iter()
            .map(|p| p.purchase.name)
            .collect();
        assert_eq!(names, ["pizza", "panini"]);
    }

    #[test]
    fn purchases_between_is_inclusive_and_ordered() {
        let mut store = store_with_account();
        store
            .replace_day(1, date(30), 100, &[item("c", 100)])
            .unwrap();
        store
            .replace_day(1, date(28), 100, &[item("a", 100)])
            .unwrap();
        store
            .replace_day(1, date(29), 100, &[item("b", 100)])
            .unwrap();

        let got: Vec<_> = store
            .purchases_between(1, date(28), date(29))
            .unwrap()
            .into_iter()
            .map(|p| p.purchase.name)
            .collect();
        assert_eq!(got, ["a", "b"]);
    }

    #[test]
    fn reports_category_sensors_that_are_no_longer_published() {
        let mut store = store_with_account();
        let keys = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        assert!(
            store
                .replace_published_category_sensors(1, &keys(&["term_puddings", "term_drinks"]))
                .unwrap()
                .is_empty()
        );
        let removed = store
            .replace_published_category_sensors(1, &keys(&["term_puddings", "term_pizza"]))
            .unwrap();
        assert_eq!(removed, ["term_drinks"]);
        assert!(
            store
                .replace_published_category_sensors(1, &keys(&["term_puddings", "term_pizza"]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unknown_day_has_no_total() {
        let store = store_with_account();
        assert_eq!(store.day_total(1, date(1)).unwrap(), None);
    }
}

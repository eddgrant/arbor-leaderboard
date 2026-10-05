//! One fetch from Arbor into the store.

use std::time::Duration;

use anyhow::Result;
use chrono::{NaiveDate, Utc};

use crate::arbor::{ArborClient, MealAccount};
use crate::store::Store;

/// Pause between day requests, to be gentle with Arbor.
const REQUEST_GAP: Duration = Duration::from_millis(250);

pub struct SyncedAccount {
    pub account: MealAccount,
    /// The first day Arbor lists for the current term, or `today` if it lists none yet.
    pub term_start: NaiveDate,
}

/// Records each child's balance, and fetches purchases for any day whose total differs from the
/// stored one (new days, and days still being added to).
pub async fn sync(
    client: &ArborClient,
    store: &mut Store,
    email: &str,
    password: &str,
    today: NaiveDate,
) -> Result<Vec<SyncedAccount>> {
    client.login(email, password).await?;
    let now = Utc::now();
    let mut synced = Vec::new();
    let mut days_fetched = 0;

    for account in client.meal_accounts().await? {
        store.record_balance(&account, now)?;
        let days = client.day_spends(account.account_id).await?;
        for day in &days {
            if store.day_total(account.account_id, day.date)? == Some(day.total_pence) {
                continue;
            }
            tokio::time::sleep(REQUEST_GAP).await;
            let purchases = client.purchases(account.account_id, day.date).await?;
            store.replace_day(account.account_id, day.date, day.total_pence, &purchases)?;
            days_fetched += 1;
        }
        let term_start = days.first().map_or(today, |d| d.date);
        synced.push(SyncedAccount {
            account,
            term_start,
        });
    }

    tracing::info!(accounts = synced.len(), days_fetched, "synced from Arbor");
    Ok(synced)
}

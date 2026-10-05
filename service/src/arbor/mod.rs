//! Client for the Arbor parent portal.
//!
//! Arbor has no API for parents, so this uses the same requests as the portal's own web pages:
//! a JSON login, then page URLs with `?format=javascript`, which return the page as a JSON
//! component tree.

pub mod parse;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};

pub use parse::{DaySpend, MealAccount, Purchase};

pub struct ArborClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct LoginResponse {
    success: bool,
    #[serde(default)]
    items: Vec<LoginItem>,
}

#[derive(Deserialize)]
struct LoginItem {
    logged_in: bool,
    session_id: Option<String>,
}

impl ArborClient {
    /// `base_url` is the school's Arbor address, e.g. `https://myschool.uk.arbor.sc`.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(concat!("arbor-leaderboard/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// Logs in and establishes a session cookie, mirroring login.arbor.sc's `login.js`.
    pub async fn login(&self, email: &str, password: &str) -> Result<()> {
        let response: LoginResponse = self
            .http
            .post(format!("{}/auth/login?lang=en", self.base_url))
            .json(&json!({ "items": [{ "username": email, "password": password }] }))
            .send()
            .await
            .context("sending Arbor login request")?
            .error_for_status()?
            .json()
            .await
            .context("reading Arbor login response")?;

        let session_id = match response.items.first() {
            Some(LoginItem {
                logged_in: true,
                session_id: Some(id),
            }) if response.success => id.clone(),
            _ => bail!("Arbor login was rejected (check email and password)"),
        };

        // Visiting the school URL with the session ID sets the session cookie.
        self.http
            .get(format!("{}/?session={session_id}&lang=en", self.base_url))
            .send()
            .await
            .context("starting Arbor session")?
            .error_for_status()?;
        Ok(())
    }

    pub async fn meal_accounts(&self) -> Result<Vec<MealAccount>> {
        let page = self.page("/guardians/home-ui/dashboard").await?;
        let accounts = parse::meal_accounts(&page);
        if accounts.is_empty() {
            bail!(
                "no meal accounts found on the dashboard; the session may have expired or the page layout changed"
            );
        }
        Ok(accounts)
    }

    pub async fn day_spends(&self, account_id: u64) -> Result<Vec<DaySpend>> {
        let page = self
            .page(&format!(
                "/guardians/customer-account-ui/dashboard/customer-account-id/{account_id}"
            ))
            .await?;
        Ok(parse::day_spends(&page))
    }

    pub async fn purchases(&self, account_id: u64, date: NaiveDate) -> Result<Vec<Purchase>> {
        let page = self
            .page(&format!(
                "/guardians/customer-account-ui/view-payments-on-date/date/{date}/customer-account-id/{account_id}"
            ))
            .await?;
        Ok(parse::purchases(&page))
    }

    async fn page(&self, path: &str) -> Result<Value> {
        let url = format!("{}{path}?format=javascript", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("fetching {path}"))?;
        let response = response
            .error_for_status()
            .with_context(|| format!("fetching {path}"))?;
        response
            .json()
            .await
            .with_context(|| format!("{path} did not return JSON; the session may have expired"))
    }
}

pub mod config;
pub mod http_data;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use http_data::*;
use log::{debug, info};
use reqwest::Client;
use reqwest_cookie_store::CookieStoreMutex;
use rpassword::prompt_password;
use std::string::String;
use std::sync::Arc;

use crate::{
    handler::hackaru::config::update_config,
    tracker::config::{Handler, Side},
};

use self::config::{create_config, HackaruConfig};
#[derive(Debug, Default)]
pub struct Hackaru {
    client: Client,
    config: HackaruConfig,
}

#[async_trait]
impl Handler for Hackaru {
    async fn handle(
        &self,
        side: &Side,
        duration: &(DateTime<Local>, DateTime<Local>),
    ) -> Result<()> {
        let activity_start = ActivityStartRequest {
            activity: ActivityStartData {
                description: side.label.clone(),
                project_id: self.config.project_id,
                started_at: duration.0.to_rfc3339(),
            },
        };

        let response = self
            .client
            .post(format!(
                "{}/{}",
                self.config.hackaru_url.trim_end_matches('/'),
                self.config.activities_rel_url.trim_matches('/')
            ))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_start)
            .send()
            .await
            .context("failed to start Hackaru activity")?
            .error_for_status()
            .context("Hackaru rejected the activity start")?
            .json::<ActivityResponse>()
            .await
            .context("failed to parse Hackaru activity response")?;

        let activity_end = ActivityEndRequest {
            activity: ActivityEndData {
                id: response.id,
                stopped_at: duration.1.to_rfc3339(),
            },
        };

        self.client
            .put(format!(
                "{}/{}/{}",
                self.config.hackaru_url.trim_end_matches('/'),
                self.config.activities_rel_url.trim_matches('/'),
                response.id
            ))
            .header("x-requested-with", "XMLHttpRequest")
            .json(&activity_end)
            .send()
            .await
            .context("failed to stop Hackaru activity")?
            .error_for_status()
            .context("Hackaru rejected the activity stop")?;
        Ok(())
    }
}

pub async fn create_handler(setup: bool) -> Result<Hackaru> {
    let mut config = create_config()?;
    setup_vendor_config(setup, &mut config).await?;
    let cookie_store = create_cookie_store(&config)?;
    let client = create_client(&cookie_store)?;

    if !has_cookies(&cookie_store)? {
        auth(&client, &config).await?;
        save_cookies(&cookie_store, &mut config)?;
        update_config(&config)?;
    }

    Ok(Hackaru { client, config })
}

fn has_cookies(cookie_store: &Arc<CookieStoreMutex>) -> Result<bool> {
    let store = cookie_store
        .lock()
        .map_err(|_| anyhow::anyhow!("Hackaru cookie store lock was poisoned"))?;
    Ok(store.iter_unexpired().count() > 0)
}

fn save_cookies(cookie_store: &Arc<CookieStoreMutex>, config: &mut HackaruConfig) -> Result<()> {
    let cookie_store = cookie_store
        .lock()
        .map_err(|_| anyhow::anyhow!("Hackaru cookie store lock was poisoned"))?;

    let mut json = Vec::new();
    cookie_store
        .save_json(&mut json)
        .map_err(|error| anyhow::anyhow!("failed to serialize Hackaru cookies: {error}"))?;

    config.cookies =
        String::from_utf8(json).context("serialized Hackaru cookies were not valid UTF-8")?;
    update_config(config)
}

fn create_client(cookie_store: &Arc<CookieStoreMutex>) -> Result<Client> {
    Client::builder()
        .cookie_store(true)
        .cookie_provider(Arc::clone(cookie_store))
        .build()
        .context("failed to create Hackaru HTTP client")
}

async fn setup_vendor_config(setup: bool, config: &mut HackaruConfig) -> Result<()> {
    if setup || config.hackaru_url.is_empty() {
        let mut hackaru_url = String::new();
        let mut message = String::from("Provide your Hackaru URL");
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut hackaru_url)
            .context("failed to read Hackaru URL")?;

        hackaru_url = hackaru_url.trim().to_string();

        if !hackaru_url.is_empty() {
            config.hackaru_url = hackaru_url;
            update_config(config)?;
        }
    }

    if setup || config.project_id == 0 {
        let mut project_id = String::new();
        let mut message = String::from("Provide your Hackaru project ID");
        if config.project_id != 0 {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.project_id).as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut project_id)
            .context("failed to read Hackaru project ID")?;

        project_id = project_id.trim().to_string();

        if !project_id.is_empty() {
            config.project_id = project_id
                .parse::<u64>()
                .context("Hackaru project ID must be a number")?;
            update_config(config)?;
        }
    }

    if setup || config.email.is_empty() {
        let mut email = String::new();
        let mut message = String::from("Provide your Hackaru email");
        if !config.email.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.email).as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut email)
            .context("failed to read Hackaru email")?;

        email = email.trim().to_string();

        if !email.is_empty() {
            config.email = email;
            update_config(config)?;
        }
    }

    if setup || config.password.is_empty() {
        let mut message = String::from("Provide your Hackaru password");
        if !config.password.is_empty() {
            message.push_str("\nleave blank to use current value");
        }
        let password = prompt_password(message)
            .context("failed to read Hackaru password")?
            .trim()
            .to_string();

        if !password.is_empty() {
            config.password = password;
            update_config(config)?;
        }
    }
    Ok(())
}

async fn auth(client: &Client, config: &HackaruConfig) -> Result<()> {
    let login = LoginRequest {
        user: UserRequest {
            email: config.email.clone(),
            password: config.password.clone(),
        },
    };

    client
        .post(format!(
            "{}/auth/auth_tokens",
            config.hackaru_url.trim_end_matches('/')
        ))
        .json(&login)
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .context("failed to authenticate with Hackaru")?
        .error_for_status()
        .context("Hackaru rejected the provided credentials")?;

    debug!("Authenticated with Hackaru");
    Ok(())
}

fn create_cookie_store(config: &HackaruConfig) -> Result<Arc<CookieStoreMutex>> {
    let cookie_store = config.get_cookie_store()?;
    Ok(Arc::new(CookieStoreMutex::new(cookie_store)))
}

use crate::handler::example::config::{create_config, update_config, ExampleConfig};
use crate::tracker::config::{Handler, Side};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use log::debug;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use simplelog::info;

pub mod config;

#[derive(Debug, Default)]
pub struct Example {
    client: Client,
    config: ExampleConfig,
}

#[async_trait]
impl Handler for Example {
    async fn handle(
        &self,
        side: &Side,
        duration: &(DateTime<Local>, DateTime<Local>),
    ) -> Result<()> {
        info!(
            "Called Example handler with side {side} and duration {:?}",
            duration
        );

        let response = self
            .client
            .post(self.config.base_url.trim_end_matches('/').to_string())
            .header(CONTENT_TYPE, "application/json")
            .header("x-api-key", &self.config.api_key)
            .send()
            .await
            .context("failed to send request to Example handler")?
            .error_for_status()
            .context("Example API rejected the request")?;

        debug!(
            "Response: {}",
            response
                .text()
                .await
                .context("failed to read Example API response")?
        );
        Ok(())
    }
}

pub async fn create_handler(setup: bool) -> Result<Example> {
    let mut config = create_config()?;
    let client = Client::builder()
        .build()
        .context("failed to create Example HTTP client")?;
    update_vendor_config(&mut config, setup)?;

    Ok(Example { client, config })
}

fn update_vendor_config(config: &mut ExampleConfig, setup: bool) -> Result<()> {
    if setup || config.api_key.is_empty() {
        let mut api_key = String::new();
        let mut message = String::from("Provide your Example API key");
        if config.api_key.is_empty() {
            message.push_str("\n leave blank to skip");
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut api_key)
            .context("failed to read Example API key")?;
        api_key = api_key.trim().to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config)?;
        }
    }
    Ok(())
}

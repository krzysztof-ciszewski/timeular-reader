use crate::handler::example::config::{create_config, update_config, ExampleConfig};
use crate::tracker::config::{Handler, Side};
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
    async fn handle(&self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        info!(
            "Called Example handler with side {side} and duration {:?}",
            duration
        );

        let response = self
            .client
            .post(format!("{}", self.config.base_url.trim_end_matches('/'),))
            .header(CONTENT_TYPE, "application/json")
            .header("x-api-key", &self.config.api_key)
            .send()
            .await;

        if response.is_err() {
            info!("API Error {}", response.unwrap_err());
            return;
        }

        debug!("Response: {}", response.unwrap().text().await.unwrap());
    }
}

pub async fn create_handler(setup: bool) -> Example {
    let mut config = create_config();
    let client = Client::builder().build().unwrap();
    update_vendor_config(&mut config, setup);

    return Example { client, config };
}

fn update_vendor_config(config: &mut ExampleConfig, setup: bool) {
    if setup || config.api_key.is_empty() {
        let mut api_key = String::new();
        let mut message =
            String::from_utf8("Provide your Example api_key".as_bytes().to_vec()).unwrap();
        if config.api_key.is_empty() {
            message.push_str(
                format!("\ncurrent value {}, leave blank to skip", config.api_key).as_str(),
            );
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut api_key)
            .expect("Please provide api_key");
        api_key = api_key.trim().to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(&config);
        }
    }
}

use crate::handler::example::config::{create_config, update_config, ExampleConfig};
use crate::tracker::config::{Handler, Side};
use crate::tracker::side_project::prompt_side_projects;
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
            "Called Example handler with side {side}, project \"{}\" and duration {:?}",
            self.config.project_id_for_side(side.side_num),
            duration
        );

        let response = self
            .client
            .post(self.config.base_url.trim_end_matches('/').to_string())
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

pub async fn create_handler(setup: bool, sides: &[Side]) -> Example {
    let mut config = create_config();
    let client = Client::builder().build().unwrap();
    update_vendor_config(&mut config, setup, sides);

    Example { client, config }
}

fn update_vendor_config(config: &mut ExampleConfig, setup: bool, sides: &[Side]) {
    if setup && prompt_side_projects("Example", sides, &mut config.side_projects) {
        update_config(config);
    }

    if setup || config.api_key.is_empty() {
        let mut api_key = String::new();
        let mut message =
            String::from_utf8("Provide your Example api_key".as_bytes().to_vec()).unwrap();
        if config.api_key.is_empty() {
            message.push_str("\n leave blank to skip");
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut api_key)
            .expect("Please provide api_key");
        api_key = api_key.trim().to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config);
        }
    }
}

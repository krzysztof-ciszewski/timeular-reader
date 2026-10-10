use crate::handler::example::config::{create_config, update_config, ExampleConfig};
use crate::tracker::config::{Handler, Side, TimeEntry};
use crate::tracker::side_project::prompt_side_projects;
use anyhow::{Context as _, Result};
use async_trait::async_trait;
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
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        info!(
            "Called Example handler with side {}, project \"{}\" and duration {:?}",
            entry.side,
            self.config.project_id_for_side(entry.side.side_num),
            entry
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

pub async fn create_handler(setup: bool, sides: &[Side]) -> Result<Example> {
    let mut config = create_config()?;
    let client = Client::builder()
        .build()
        .context("failed to create Example HTTP client")?;
    update_vendor_config(&mut config, setup, sides).await?;

    Ok(Example { client, config })
}

async fn update_vendor_config(
    config: &mut ExampleConfig,
    setup: bool,
    sides: &[Side],
) -> Result<()> {
    if setup && prompt_side_projects("Example", sides, &mut config.side_projects).await? {
        update_config(config)?;
    }

    if setup || config.api_key.is_empty() {
        let mut message = String::from("Provide your Example API key");
        if config.api_key.is_empty() {
            message.push_str("\n leave blank to skip");
        }
        info!("{message}");

        let api_key = crate::prompt::read_line()
            .await
            .context("failed to read Example API key")?;
        let api_key = api_key.trim().to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{config::ExampleConfig, Example};
    use crate::{
        test_support::{time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;

    #[tokio::test]
    async fn sends_api_key_and_surfaces_http_errors() {
        let server = MockServer::start(2, |index, _| {
            if index == 0 {
                (200, "{}".into())
            } else {
                (500, "{}".into())
            }
        });
        let config = ExampleConfig {
            base_url: format!("{}/records", server.url()),
            api_key: "token".into(),
            ..ExampleConfig::default()
        };
        let handler = Example {
            client: Client::new(),
            config,
        };

        handler.handle(&time_entry(1, "Work")).await.unwrap();
        let error = handler.handle(&time_entry(1, "Break")).await.unwrap_err();

        assert!(format!("{error:#}").contains("Example API rejected the request"));
        let requests = server.finish();
        assert!(requests[0].starts_with("POST /records HTTP/1.1"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("x-api-key: token"));
        assert!(requests[1].starts_with("POST /records HTTP/1.1"));
    }
}
